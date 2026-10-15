//! Which session made which change, when several share a worktree. The
//! daemon snapshots a session's checkout when the session starts, when
//! each of its turns starts (status to running) and when each ends
//! (finished, needs feedback, terminated): HEAD, and a git tree of the
//! whole working tree as it stands, untracked files included and ignored
//! ones not.
//!
//! A session's changes are what changed inside its own turns. The daemon
//! keeps two trees per session, a base `B` and a tip `T`, and folds each
//! turn `(s, e)` in as it ends:
//!
//! - the first turn: `B = s`, `T = e`;
//! - a later one: `B = merge(T → s onto B)`, `T = e`. The gap between the
//!   previous turn's end and this one's start (`T → s`) is other people's
//!   work; [`fold_gap`] replays it onto `B` with a three-way merge, so it
//!   lands on both sides of the diff and drops out of it.
//!
//! `diff(B, T)` ([`SessionChanges::trees`]) is then exactly the sum of the
//! session's turns, files another session touched between them included:
//! a hunk of theirs sits in both trees, the session's own hunks only in
//! `T`. The limits, all on the side of showing too much rather than too
//! little:
//!
//! - edits another session makes *during* this one's turn are inside
//!   `s → e` and count as this session's: a turn is a window of time;
//! - where a gap edit overlaps lines this session had already changed, the
//!   merge keeps the session's version (`-X ours`), so the other edit shows
//!   as part of this session's diff;
//! - a turn whose start tree could not be taken (unavailable), or a merge
//!   git refuses, leaves `B` where it was: the gap counts as the session's;
//! - snapshots run in order on one worker, so one taken late (another
//!   session's add ahead of it) can move a turn's edge.
//!
//! Turns are kept per session ([`Slot::TurnStart`], [`Slot::TurnEnd`]) for
//! the last few only: `B` and `T` already hold the effect of every turn,
//! so pruning old ones loses nothing.
//!
//! The tree is built without touching the checkout: a copy of the index
//! (`GIT_INDEX_FILE` at a temp file, so git's stat cache spares re-hashing
//! what has not changed), `git add -A` into the copy, `git write-tree`.
//! The real index, the files and HEAD are left alone; the only writes are
//! objects in the object store and the refs that keep the trees from
//! `git gc` ([`keep`], [`drop_refs`]).
//!
//! The refs live under the writing daemon's own id
//! (`refs/kete/d/<daemon>/sessions/<agent>/…`, [`ref_name`]), stable per
//! data dir, so a daemon can sweep refs for sessions it no longer has
//! ([`sweep`]) without touching those of another daemon (a test's, WSL's
//! beside Windows') that shares the repo. 0.51 wrote
//! `refs/kete/sessions/<agent>/{start,last}` with no owner; a daemon moves
//! those of its own sessions into its namespace ([`adopt_legacy`]) and
//! never deletes one by absence.
//!
//! A working tree whose untracked files are too many or too big, or an
//! add that runs past [`Limits::timeout`], records no tree ("unavailable")
//! rather than hold the daemon's snapshot worker.

use crate::review_base::{parse_name_status, BaseChange};
use crate::{AgentId, WorktreeId};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

/// One snapshot of a session's checkout.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SnapshotPoint {
    /// HEAD then, full SHA; None on a branch with no commit yet.
    pub head: Option<String>,
    /// The working tree then, as a tree object; None when it could not be
    /// taken (too big, too slow, git failed): "unavailable".
    pub tree: Option<String>,
    /// Epoch ms it was taken.
    pub at: i64,
}

/// What `ClientRequest::SessionChanges` answers: a session's two
/// snapshots, for the client to diff itself (`git diff <start> <last>`,
/// [`files_between`]) as both apps' diff readers read git directly.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionChanges {
    pub agent: AgentId,
    /// The worktree the snapshots were taken in.
    pub worktree: Option<WorktreeId>,
    /// Taken when the session started; None for a session from before the
    /// snapshots existed, until its first turn ends.
    pub start: Option<SnapshotPoint>,
    /// Taken at its latest turn boundary; None until a turn has ended.
    pub last: Option<SnapshotPoint>,
    /// Sessions in the same worktree that have snapshots, this one
    /// included: the review offers "this session" only when it is more
    /// than one.
    pub sessions: u32,
}

impl SessionChanges {
    /// The tree pair to diff, when both snapshots were taken and are there.
    pub fn trees(&self) -> Option<(&str, &str)> {
        let start = self.start.as_ref()?.tree.as_deref()?;
        let last = self.last.as_ref()?.tree.as_deref()?;
        Some((start, last))
    }
}

/// Which of a session's kept trees.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Slot {
    /// The checkout when the session started.
    Start,
    /// The checkout at its latest turn end.
    Last,
    /// The folded base `B` (module doc).
    Base,
    /// The folded tip `T`.
    Tip,
    /// Turn `n`'s start (`t<n>/s`).
    TurnStart(u32),
    /// Turn `n`'s end (`t<n>/e`).
    TurnEnd(u32),
}

impl Slot {
    pub fn suffix(self) -> String {
        match self {
            Slot::Start => "start".into(),
            Slot::Last => "last".into(),
            Slot::Base => "base".into(),
            Slot::Tip => "tip".into(),
            Slot::TurnStart(n) => format!("t{n}/s"),
            Slot::TurnEnd(n) => format!("t{n}/e"),
        }
    }
}

/// Where 0.51 kept a session's two snapshot trees, owner unknown. Shared by
/// every checkout of the repo (refs outside `HEAD` are per repo, not per
/// worktree), as are the namespaced ones.
pub const LEGACY_PREFIX: &str = "refs/kete/sessions/";

/// An id fit for one refname component: agent ids are ULIDs and daemon
/// ids hex, so anything else is refused rather than escaped.
fn safe_component(id: &str) -> bool {
    !id.is_empty()
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

/// `refs/kete/d/<daemon>/sessions/`: everything `daemon` keeps in a repo.
/// The `d/` segment keeps any daemon id clear of the 0.51 `sessions/`.
pub fn namespace(daemon: &str) -> Option<String> {
    safe_component(daemon).then(|| format!("refs/kete/d/{daemon}/sessions/"))
}

/// `refs/kete/d/<daemon>/sessions/<agent>/<slot>`; None for an id that
/// would not make a safe refname.
pub fn ref_name(daemon: &str, agent: &str, slot: Slot) -> Option<String> {
    let ns = namespace(daemon)?;
    safe_component(agent).then(|| format!("{ns}{agent}/{}", slot.suffix()))
}

/// How much a snapshot may cost before it is skipped.
#[derive(Debug, Clone, Copy)]
pub struct Limits {
    /// Untracked, not ignored files: more than this and no tree is taken.
    pub max_untracked_files: usize,
    /// Their total size, bytes.
    pub max_untracked_bytes: u64,
    /// The add, end to end.
    pub timeout: Duration,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            max_untracked_files: 20_000,
            max_untracked_bytes: 64 * 1024 * 1024,
            timeout: Duration::from_secs(20),
        }
    }
}

fn command(root: &Path, args: &[&str]) -> Command {
    let mut cmd = Command::new("git");
    cmd.arg("-C")
        .arg(root)
        .args(["-c", "core.quotepath=off"])
        .args(args)
        .env("GIT_OPTIONAL_LOCKS", "0")
        .env("GIT_TERMINAL_PROMPT", "0")
        .stdin(Stdio::null());
    #[cfg(windows)]
    crate::platform::no_window(&mut cmd);
    cmd
}

fn git_ok(root: &Path, args: &[&str]) -> Option<Vec<u8>> {
    let out = command(root, args).output().ok()?;
    out.status.success().then_some(out.stdout)
}

fn git_line(root: &Path, args: &[&str]) -> Option<String> {
    let out = git_ok(root, args)?;
    let line = String::from_utf8_lossy(&out).trim().to_string();
    (!line.is_empty()).then_some(line)
}

/// Run `cmd` to its end or `deadline`, whichever first; killed at the
/// deadline (None). Its output is not read: callers send it to null.
fn run_until(mut cmd: Command, deadline: Instant) -> Option<std::process::ExitStatus> {
    let mut child = cmd
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return Some(status),
            Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(15)),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
        }
    }
}

/// A temp index file that is removed (with git's lock beside it) on drop.
struct TempIndex(PathBuf);

impl TempIndex {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let n = NEXT.fetch_add(1, Ordering::Relaxed);
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.subsec_nanos())
            .unwrap_or(0);
        let name = format!("kete-snapshot-{}-{nanos}-{n}.index", std::process::id());
        Self(std::env::temp_dir().join(name))
    }
}

impl Drop for TempIndex {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
        let mut lock = self.0.clone().into_os_string();
        lock.push(".lock");
        let _ = std::fs::remove_file(lock);
    }
}

/// The untracked, not ignored files under `root` fit `limits`.
fn untracked_fit(root: &Path, limits: &Limits) -> bool {
    let Some(raw) = git_ok(root, &["ls-files", "--others", "--exclude-standard", "-z"]) else {
        return false;
    };
    let mut files = 0usize;
    let mut bytes = 0u64;
    for path in raw.split(|b| *b == 0).filter(|p| !p.is_empty()) {
        files += 1;
        if files > limits.max_untracked_files {
            return false;
        }
        let path = root.join(String::from_utf8_lossy(path).as_ref());
        bytes += std::fs::symlink_metadata(&path)
            .map(|m| m.len())
            .unwrap_or(0);
        if bytes > limits.max_untracked_bytes {
            return false;
        }
    }
    true
}

/// Snapshot the checkout at `root` now, within the default [`Limits`].
pub fn take(root: &Path) -> SnapshotPoint {
    take_within(root, &Limits::default())
}

/// [`take`] within `limits`.
pub fn take_within(root: &Path, limits: &Limits) -> SnapshotPoint {
    let at = crate::clock::now_ms();
    let head = git_line(root, &["rev-parse", "--verify", "-q", "HEAD"]);
    let tree = tree_of(root, head.is_some(), limits);
    SnapshotPoint { head, tree, at }
}

fn tree_of(root: &Path, has_head: bool, limits: &Limits) -> Option<String> {
    let deadline = Instant::now() + limits.timeout;
    if !untracked_fit(root, limits) {
        return None;
    }
    let temp = TempIndex::new();
    // Start from a copy of the real index: its stat data lets the add skip
    // every file that has not changed. No index yet (a fresh clone with
    // nothing staged reads HEAD's tree instead) or a copy that failed:
    // start from HEAD's tree, and the add hashes what differs.
    let index = git_line(root, &["rev-parse", "--git-path", "index"]).map(|p| {
        let p = PathBuf::from(p);
        if p.is_absolute() {
            p
        } else {
            root.join(p)
        }
    });
    // Read and written rather than `fs::copy`: a plain read shares the file
    // for delete, so a session's `git add` renaming its lock over the index
    // meanwhile is never refused.
    let copied = index
        .and_then(|index| std::fs::read(index).ok())
        .is_some_and(|bytes| std::fs::write(&temp.0, bytes).is_ok());
    if !copied && has_head {
        let mut cmd = command(root, &["read-tree", "HEAD"]);
        cmd.env("GIT_INDEX_FILE", &temp.0);
        if !run_until(cmd, deadline)?.success() {
            return None;
        }
    }
    // `--ignore-errors`: a file another process holds open (Windows) is
    // left as the index copy has it instead of failing the whole tree.
    let mut add = command(root, &["add", "-A", "--ignore-errors"]);
    add.env("GIT_INDEX_FILE", &temp.0);
    run_until(add, deadline)?;
    let mut write = command(root, &["write-tree"]);
    write.env("GIT_INDEX_FILE", &temp.0);
    let out: Output = write.output().ok()?;
    let tree = String::from_utf8_lossy(&out.stdout).trim().to_string();
    (out.status.success() && !tree.is_empty()).then_some(tree)
}

/// Run git with `input` on its stdin; its stdout when it exits 0.
fn git_fed(mut cmd: Command, input: &[u8]) -> Option<Vec<u8>> {
    cmd.stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    let mut child = cmd.spawn().ok()?;
    let mut stdin = child.stdin.take()?;
    let input = input.to_vec();
    // Written from a thread: a large script must not fill the pipe while
    // git's stdout, unread, fills the other one.
    let writer = std::thread::spawn(move || {
        use std::io::Write as _;
        let _ = stdin.write_all(&input);
    });
    let out = child.wait_with_output().ok()?;
    let _ = writer.join();
    out.status.success().then_some(out.stdout)
}

/// Apply `script` (`git update-ref --stdin` lines) as one transaction.
fn update_refs(repo: &Path, script: &str) -> bool {
    if script.is_empty() {
        return true;
    }
    git_fed(command(repo, &["update-ref", "--stdin"]), script.as_bytes()).is_some()
}

/// Every ref under `prefix` in the repo at `repo`, with what it points at.
fn refs_under(repo: &Path, prefix: &str) -> Vec<(String, String)> {
    let Some(out) = git_ok(
        repo,
        &["for-each-ref", "--format=%(refname) %(objectname)", prefix],
    ) else {
        return Vec::new();
    };
    String::from_utf8_lossy(&out)
        .lines()
        .filter_map(|l| {
            let (name, oid) = l.split_once(' ')?;
            Some((name.to_string(), oid.to_string()))
        })
        .collect()
}

/// The distinct first components under `prefix` (agent ids), sorted.
fn agents_under(repo: &Path, prefix: &str) -> Vec<String> {
    let mut ids: Vec<String> = refs_under(repo, prefix)
        .into_iter()
        .filter_map(|(name, _)| Some(name.strip_prefix(prefix)?.split('/').next()?.to_string()))
        .collect();
    ids.sort();
    ids.dedup();
    ids
}

/// Point `daemon`'s `<agent>/<slot>` ref at `tree` in the repo at `repo`
/// (any of its checkouts), so `git gc` keeps it.
pub fn keep(repo: &Path, daemon: &str, agent: &str, slot: Slot, tree: &str) -> bool {
    let Some(name) = ref_name(daemon, agent, slot) else {
        return false;
    };
    git_ok(repo, &["update-ref", &name, tree]).is_some()
}

/// Delete every ref `daemon` keeps for each of `agents` in the repo at
/// `repo`, and their 0.51 refs (the caller owns these sessions). One
/// transaction; none there is not an error.
pub fn drop_refs(repo: &Path, daemon: &str, agents: &[String]) -> bool {
    let Some(ns) = namespace(daemon) else {
        return false;
    };
    let mut script = String::new();
    for agent in agents.iter().filter(|a| safe_component(a)) {
        for prefix in [format!("{ns}{agent}/"), format!("{LEGACY_PREFIX}{agent}/")] {
            for (name, _) in refs_under(repo, &prefix) {
                script.push_str(&format!("delete {name}\n"));
            }
        }
    }
    update_refs(repo, &script)
}

/// Delete `daemon`'s refs for turns `turns` of `agent`.
pub fn drop_turns(repo: &Path, daemon: &str, agent: &str, turns: &[u32]) -> bool {
    let mut script = String::new();
    for &n in turns {
        for slot in [Slot::TurnStart(n), Slot::TurnEnd(n)] {
            if let Some(name) = ref_name(daemon, agent, slot) {
                script.push_str(&format!("delete {name}\n"));
            }
        }
    }
    update_refs(repo, &script)
}

/// The agent ids `daemon` keeps refs for in the repo at `repo`.
pub fn kept_agents(repo: &Path, daemon: &str) -> Vec<String> {
    namespace(daemon).map_or_else(Vec::new, |ns| agents_under(repo, &ns))
}

/// The agent ids with 0.51 refs (no owner) in the repo at `repo`.
pub fn legacy_agents(repo: &Path) -> Vec<String> {
    agents_under(repo, LEGACY_PREFIX)
}

/// Move the 0.51 refs of `agents` (sessions the caller owns) into
/// `daemon`'s namespace, same slot names. One transaction per call; a
/// session with none is skipped. Only ever called with ids from the
/// caller's own database: the refs of a session it does not know are
/// another daemon's to move.
pub fn adopt_legacy(repo: &Path, daemon: &str, agents: &[String]) -> bool {
    let mut script = String::new();
    for agent in agents.iter().filter(|a| safe_component(a)) {
        let prefix = format!("{LEGACY_PREFIX}{agent}/");
        for (old, oid) in refs_under(repo, &prefix) {
            let Some(slot) = old.strip_prefix(&prefix) else {
                continue;
            };
            let Some(ns) = namespace(daemon) else {
                return false;
            };
            script.push_str(&format!("update {ns}{agent}/{slot} {oid}\n"));
            script.push_str(&format!("delete {old} {oid}\n"));
        }
    }
    update_refs(repo, &script)
}

/// Delete `daemon`'s refs for every session not in `live` in the repo at
/// `repo`: what a crash between a row's delete and its refs' left behind.
/// Only `daemon`'s namespace is read; 0.51 refs and other daemons' are
/// never swept. Returns the ids swept.
pub fn sweep(repo: &Path, daemon: &str, live: &std::collections::HashSet<String>) -> Vec<String> {
    let gone: Vec<String> = kept_agents(repo, daemon)
        .into_iter()
        .filter(|a| !live.contains(a))
        .collect();
    let Some(ns) = namespace(daemon) else {
        return Vec::new();
    };
    let mut script = String::new();
    for agent in &gone {
        for (name, _) in refs_under(repo, &format!("{ns}{agent}/")) {
            script.push_str(&format!("delete {name}\n"));
        }
    }
    if update_refs(repo, &script) {
        gone
    } else {
        Vec::new()
    }
}

/// Replay the gap `prev_end → next_start` (others' work between two of a
/// session's turns) onto its base `base`: a three-way merge with
/// `prev_end` as the merge base, written as a tree. Lines both changed
/// keep `base`'s (`-X ours`); a path git still cannot merge (one side
/// deleted it, a file met a directory) is put back as `base` has it. None
/// when git refuses the merge altogether: the caller keeps `base`.
pub fn fold_gap(root: &Path, prev_end: &str, base: &str, next_start: &str) -> Option<String> {
    let merge_base = format!("--merge-base={prev_end}");
    let out = command(
        root,
        &[
            "merge-tree",
            "--write-tree",
            "-z",
            "--name-only",
            "-X",
            "ours",
            &merge_base,
            base,
            next_start,
        ],
    )
    .output()
    .ok()?;
    // 0 clean, 1 conflicted (the tree is still written); anything else is
    // a refusal (a git too old to take bare trees among them).
    let clean = match out.status.code() {
        Some(0) => true,
        Some(1) => false,
        _ => return None,
    };
    let mut fields = out.stdout.split(|b| *b == 0);
    let tree = String::from_utf8_lossy(fields.next()?).trim().to_string();
    if tree.is_empty() {
        return None;
    }
    if clean {
        return Some(tree);
    }
    // `--name-only`: the conflicted paths follow the tree, up to an empty
    // field before the messages.
    let paths: Vec<String> = fields
        .take_while(|f| !f.is_empty())
        .map(|f| String::from_utf8_lossy(f).into_owned())
        .collect();
    restore_paths(root, &tree, base, &paths)
}

/// `tree` with each of `paths` as `from` has it (gone where `from` has
/// none), built in a temp index.
fn restore_paths(root: &Path, tree: &str, from: &str, paths: &[String]) -> Option<String> {
    let temp = TempIndex::new();
    let in_temp = |args: &[&str]| {
        let mut cmd = command(root, args);
        cmd.env("GIT_INDEX_FILE", &temp.0);
        cmd
    };
    let ok = |mut cmd: Command| {
        cmd.stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .is_ok_and(|s| s.success())
    };
    if !ok(in_temp(&["read-tree", tree])) {
        return None;
    }
    if !paths.is_empty() {
        let mut list = Vec::new();
        for p in paths {
            list.extend_from_slice(p.as_bytes());
            list.push(0);
        }
        git_fed(
            in_temp(&["update-index", "-z", "--force-remove", "--stdin"]),
            &list,
        )?;
        let mut ls = vec!["ls-tree", "-r", "-z", "--full-tree", from, "--"];
        ls.extend(paths.iter().map(String::as_str));
        let entries = git_ok(root, &ls)?;
        if !entries.is_empty() {
            git_fed(in_temp(&["update-index", "-z", "--index-info"]), &entries)?;
        }
    }
    let out = in_temp(&["write-tree"]).output().ok()?;
    let tree = String::from_utf8_lossy(&out.stdout).trim().to_string();
    (out.status.success() && !tree.is_empty()).then_some(tree)
}

/// Every path that differs between trees (or commits) `from` and `to`, as
/// `git diff --name-status` reads it. Sorted by path.
pub fn files_between(root: &Path, from: &str, to: &str) -> Result<Vec<BaseChange>, String> {
    let out = command(
        root,
        &[
            "diff",
            "--name-status",
            "-z",
            "-M",
            "--no-ext-diff",
            from,
            to,
            "--",
        ],
    )
    .output()
    .map_err(|e| format!("could not run git: {e}"))?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr);
        return Err(format!("git diff failed: {}", err.trim()));
    }
    let mut files = parse_name_status(&out.stdout);
    files.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(files)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(repo: &Path, args: &[&str]) -> String {
        let out = Command::new("git")
            .arg("-C")
            .arg(repo)
            .args(args)
            .output()
            .expect("run git");
        assert!(
            out.status.success(),
            "git {args:?}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8_lossy(&out.stdout).trim().to_string()
    }

    fn repo(dir: &tempfile::TempDir) -> PathBuf {
        let repo = dir.path().join("repo");
        std::fs::create_dir(&repo).unwrap();
        run(&repo, &["init", "-q", "-b", "main"]);
        run(&repo, &["config", "user.email", "t@t"]);
        run(&repo, &["config", "user.name", "t"]);
        run(&repo, &["config", "commit.gpgsign", "false"]);
        run(&repo, &["config", "core.autocrlf", "false"]);
        std::fs::write(repo.join(".gitignore"), "target/\n").unwrap();
        std::fs::write(repo.join("a.txt"), "one\n").unwrap();
        std::fs::write(repo.join("gone.txt"), "bye\n").unwrap();
        run(&repo, &["add", "."]);
        run(&repo, &["commit", "-q", "-m", "init"]);
        repo
    }

    fn names(files: &[BaseChange]) -> Vec<(char, &str)> {
        files.iter().map(|f| (f.status, f.path.as_str())).collect()
    }

    /// Untracked files are in the tree, ignored ones are not, a deleted
    /// file is gone from it, and the index, HEAD and the files are as they
    /// were.
    #[test]
    fn a_snapshot_holds_the_working_tree_and_leaves_the_checkout_alone() {
        let dir = tempfile::tempdir().unwrap();
        let repo = repo(&dir);
        let start = take(&repo);
        let head = run(&repo, &["rev-parse", "HEAD"]);
        assert_eq!(start.head.as_deref(), Some(head.as_str()));
        let start_tree = start.tree.clone().expect("a tree");
        assert_eq!(start_tree, run(&repo, &["rev-parse", "HEAD^{tree}"]));

        std::fs::write(repo.join("a.txt"), "one\ntwo\n").unwrap();
        std::fs::write(repo.join("new.txt"), "fresh\n").unwrap();
        std::fs::create_dir(repo.join("target")).unwrap();
        std::fs::write(repo.join("target").join("out.bin"), "built\n").unwrap();
        std::fs::remove_file(repo.join("gone.txt")).unwrap();
        // Something staged by hand, to see the index kept as it was.
        std::fs::write(repo.join("staged.txt"), "staged\n").unwrap();
        run(&repo, &["add", "staged.txt"]);
        let status_before = run(&repo, &["status", "--porcelain"]);
        let index = repo.join(".git").join("index");
        let index_before = std::fs::read(&index).unwrap();

        let last = take(&repo);
        let last_tree = last.tree.clone().expect("a tree");
        assert_ne!(last_tree, start_tree);
        assert_eq!(
            std::fs::read(&index).unwrap(),
            index_before,
            "index untouched"
        );
        assert_eq!(run(&repo, &["status", "--porcelain"]), status_before);
        assert_eq!(run(&repo, &["rev-parse", "HEAD"]), head);
        assert_eq!(
            std::fs::read_to_string(repo.join("new.txt")).unwrap(),
            "fresh\n"
        );

        let files = files_between(&repo, &start_tree, &last_tree).unwrap();
        assert_eq!(
            names(&files),
            vec![
                ('M', "a.txt"),
                ('D', "gone.txt"),
                ('A', "new.txt"),
                ('A', "staged.txt")
            ]
        );
        let changes = SessionChanges {
            agent: AgentId("a".into()),
            worktree: None,
            start: Some(start),
            last: Some(last),
            sessions: 2,
        };
        assert_eq!(
            changes.trees(),
            Some((start_tree.as_str(), last_tree.as_str()))
        );
    }

    #[test]
    fn too_many_or_too_big_untracked_files_are_unavailable() {
        let dir = tempfile::tempdir().unwrap();
        let repo = repo(&dir);
        for i in 0..3 {
            std::fs::write(repo.join(format!("u{i}.txt")), "0123456789").unwrap();
        }
        let few = Limits {
            max_untracked_files: 2,
            ..Limits::default()
        };
        let point = take_within(&repo, &few);
        assert!(point.head.is_some());
        assert_eq!(point.tree, None, "too many");
        let small = Limits {
            max_untracked_bytes: 20,
            ..Limits::default()
        };
        assert_eq!(take_within(&repo, &small).tree, None, "too big");
        assert!(take(&repo).tree.is_some());
        let changes = SessionChanges {
            agent: AgentId("a".into()),
            worktree: None,
            start: Some(take_within(&repo, &few)),
            last: Some(take(&repo)),
            sessions: 1,
        };
        assert_eq!(changes.trees(), None);
    }

    #[test]
    fn a_repo_with_no_commit_yet_still_snapshots() {
        let dir = tempfile::tempdir().unwrap();
        let repo = dir.path().join("empty");
        std::fs::create_dir(&repo).unwrap();
        run(&repo, &["init", "-q", "-b", "main"]);
        std::fs::write(repo.join("first.txt"), "hi\n").unwrap();
        let point = take(&repo);
        assert_eq!(point.head, None);
        let tree = point.tree.expect("a tree");
        assert_eq!(run(&repo, &["ls-tree", "--name-only", &tree]), "first.txt");
        assert_eq!(run(&repo, &["status", "--porcelain"]), "?? first.txt");
    }

    /// The trees survive gc while their refs are there; dropping a
    /// session's refs (every slot, turns included) leaves the others'
    /// alone, and another daemon's.
    #[test]
    fn refs_keep_trees_and_go_with_their_session() {
        let dir = tempfile::tempdir().unwrap();
        let repo = repo(&dir);
        std::fs::write(repo.join("loose.txt"), "only in a snapshot\n").unwrap();
        let tree = take(&repo).tree.unwrap();
        assert!(keep(&repo, "d1", "A1", Slot::Start, &tree));
        assert!(keep(&repo, "d1", "A1", Slot::TurnEnd(3), &tree));
        assert!(keep(&repo, "d1", "B2", Slot::Last, &tree));
        assert!(keep(&repo, "d2", "A1", Slot::Base, &tree));
        assert!(!keep(&repo, "d1", "../evil", Slot::Start, &tree));
        assert!(!keep(&repo, "d/1", "A1", Slot::Start, &tree));
        assert_eq!(
            kept_agents(&repo, "d1"),
            vec!["A1".to_string(), "B2".to_string()]
        );
        run(&repo, &["gc", "-q", "--prune=now"]);
        assert_eq!(run(&repo, &["cat-file", "-t", &tree]), "tree");
        // A missing session in the batch is not an error.
        assert!(drop_refs(&repo, "d1", &["A1".into(), "C3".into()]));
        assert_eq!(kept_agents(&repo, "d1"), vec!["B2".to_string()]);
        assert_eq!(kept_agents(&repo, "d2"), vec!["A1".to_string()]);
        assert!(drop_refs(&repo, "d1", &[]));
        assert_eq!(
            ref_name("d1", "A1", Slot::TurnStart(7)).unwrap(),
            "refs/kete/d/d1/sessions/A1/t7/s"
        );
        assert_eq!(ref_name("d1", "", Slot::Last), None);
    }

    /// Turn refs go by number; the rest of the session's stay.
    #[test]
    fn turn_refs_prune_alone() {
        let dir = tempfile::tempdir().unwrap();
        let repo = repo(&dir);
        let tree = take(&repo).tree.unwrap();
        for n in 1..=3 {
            keep(&repo, "d1", "A1", Slot::TurnStart(n), &tree);
            keep(&repo, "d1", "A1", Slot::TurnEnd(n), &tree);
        }
        keep(&repo, "d1", "A1", Slot::Base, &tree);
        assert!(drop_turns(&repo, "d1", "A1", &[1, 2]));
        let left: Vec<String> = refs_under(&repo, "refs/kete/")
            .into_iter()
            .map(|(n, _)| n)
            .collect();
        assert_eq!(
            left,
            vec![
                "refs/kete/d/d1/sessions/A1/base",
                "refs/kete/d/d1/sessions/A1/t3/e",
                "refs/kete/d/d1/sessions/A1/t3/s",
            ]
        );
    }

    /// The sweep takes only its own daemon's refs for sessions it no
    /// longer has: another daemon's, and 0.51's, stay whatever their ids.
    #[test]
    fn a_sweep_never_touches_another_daemons_refs() {
        let dir = tempfile::tempdir().unwrap();
        let repo = repo(&dir);
        let tree = take(&repo).tree.unwrap();
        keep(&repo, "mine", "live", Slot::Start, &tree);
        keep(&repo, "mine", "dead", Slot::Start, &tree);
        keep(&repo, "mine", "dead", Slot::TurnEnd(1), &tree);
        keep(&repo, "theirs", "other", Slot::Start, &tree);
        keep(&repo, "theirs", "dead", Slot::Start, &tree);
        run(
            &repo,
            &["update-ref", "refs/kete/sessions/old/start", &tree],
        );
        let live: std::collections::HashSet<String> = ["live".to_string()].into();
        assert_eq!(sweep(&repo, "mine", &live), vec!["dead".to_string()]);
        assert_eq!(kept_agents(&repo, "mine"), vec!["live".to_string()]);
        assert_eq!(
            kept_agents(&repo, "theirs"),
            vec!["dead".to_string(), "other".to_string()]
        );
        assert_eq!(legacy_agents(&repo), vec!["old".to_string()]);
        // The other daemon's sweep, knowing none of mine, leaves mine.
        assert_eq!(
            sweep(&repo, "theirs", &["other".to_string()].into()),
            vec!["dead".to_string()]
        );
        assert_eq!(kept_agents(&repo, "mine"), vec!["live".to_string()]);
    }

    /// 0.51 refs move into the namespace for the sessions named, same
    /// trees, and the rest stay where they were.
    #[test]
    fn legacy_refs_are_adopted_only_for_known_sessions() {
        let dir = tempfile::tempdir().unwrap();
        let repo = repo(&dir);
        let tree = take(&repo).tree.unwrap();
        run(&repo, &["update-ref", "refs/kete/sessions/A1/start", &tree]);
        run(&repo, &["update-ref", "refs/kete/sessions/A1/last", &tree]);
        run(&repo, &["update-ref", "refs/kete/sessions/B2/start", &tree]);
        assert!(adopt_legacy(&repo, "d1", &["A1".into(), "Z9".into()]));
        assert_eq!(legacy_agents(&repo), vec!["B2".to_string()]);
        let mine = refs_under(&repo, "refs/kete/d/d1/sessions/");
        assert_eq!(
            mine,
            vec![
                ("refs/kete/d/d1/sessions/A1/last".to_string(), tree.clone()),
                ("refs/kete/d/d1/sessions/A1/start".to_string(), tree.clone()),
            ]
        );
        // Nothing to move is not an error.
        assert!(adopt_legacy(&repo, "d1", &["A1".into()]));
    }

    fn tree_with(repo: &Path, files: &[(&str, &str)]) -> String {
        for (path, body) in files {
            std::fs::write(repo.join(path), body).unwrap();
        }
        take(repo).tree.unwrap()
    }

    fn blob(repo: &Path, tree: &str, path: &str) -> Option<String> {
        let out = Command::new("git")
            .arg("-C")
            .arg(repo)
            .args(["cat-file", "-p", &format!("{tree}:{path}")])
            .output()
            .unwrap();
        out.status
            .success()
            .then(|| String::from_utf8_lossy(&out.stdout).into_owned())
    }

    /// A gap edit in a file the session touched lands on the base, so the
    /// diff to the tip shows only the session's own lines; a gap edit to a
    /// file it never touched lands too and drops out.
    #[test]
    fn a_gap_folds_into_the_base_and_out_of_the_diff() {
        let dir = tempfile::tempdir().unwrap();
        let repo = repo(&dir);
        let lines = |top: &str, bottom: &str| format!("{top}\n2\n3\n4\n5\n6\n7\n{bottom}\n");
        let s1 = tree_with(&repo, &[("f.txt", &lines("1", "8")), ("o.txt", "x\n")]);
        let e1 = tree_with(&repo, &[("f.txt", &lines("MINE", "8"))]);
        // Someone else, between the turns: the bottom of f, all of o.
        let s2 = tree_with(
            &repo,
            &[("f.txt", &lines("MINE", "THEIRS")), ("o.txt", "y\n")],
        );
        let base = fold_gap(&repo, &e1, &s1, &s2).expect("a merge");
        assert_eq!(blob(&repo, &base, "f.txt").unwrap(), lines("1", "THEIRS"));
        assert_eq!(blob(&repo, &base, "o.txt").unwrap(), "y\n");
        let files = files_between(&repo, &base, &s2).unwrap();
        assert_eq!(names(&files), vec![('M', "f.txt")]);
        // Overlap: they rewrote the line the session had changed. The
        // session's version is kept on the base: their line shows as the
        // session's (too much, never too little).
        let s3 = tree_with(&repo, &[("f.txt", &lines("OVER", "THEIRS"))]);
        let base3 = fold_gap(&repo, &s2, &base, &s3).expect("a merge");
        assert_eq!(blob(&repo, &base3, "f.txt").unwrap(), lines("1", "THEIRS"));
        // They deleted a file the session had edited: kept as the base had
        // it, so the delete shows as the session's.
        std::fs::remove_file(repo.join("f.txt")).unwrap();
        let s4 = take(&repo).tree.unwrap();
        let base4 = fold_gap(&repo, &s3, &base3, &s4).expect("a merge");
        assert_eq!(blob(&repo, &base4, "f.txt").unwrap(), lines("1", "THEIRS"));
        assert_eq!(fold_gap(&repo, "nope", &base, &s2), None);
    }
}
