//! Which session made which change, when several share a worktree. The
//! daemon snapshots a session's checkout when the session starts and again
//! at each turn boundary (finished, needs feedback): HEAD, and a git tree
//! of the whole working tree as it stands, untracked files included and
//! ignored ones not. A session's changes are the diff from its start
//! snapshot to its last one ([`SessionChanges::trees`]).
//!
//! The tree is built without touching the checkout: a copy of the index
//! (`GIT_INDEX_FILE` at a temp file, so git's stat cache spares re-hashing
//! what has not changed), `git add -A` into the copy, `git write-tree`.
//! The real index, the files and HEAD are left alone; the only writes are
//! objects in the object store and the `refs/kete/sessions/<id>/…` refs
//! that keep the trees from `git gc` ([`keep`], [`drop_refs`]).
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

/// Which of a session's two snapshots.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Which {
    Start,
    Last,
}

impl Which {
    pub fn as_str(self) -> &'static str {
        match self {
            Which::Start => "start",
            Which::Last => "last",
        }
    }
}

/// Where a session's snapshot trees are kept from `git gc`. Shared by
/// every checkout of the repo (refs outside `HEAD` are per repo, not per
/// worktree).
pub const REF_PREFIX: &str = "refs/kete/sessions/";

/// `refs/kete/sessions/<agent>/<which>`; None for an id that would not
/// make a safe refname (agent ids are ULIDs, so never in practice).
pub fn ref_name(agent: &str, which: Which) -> Option<String> {
    let safe = !agent.is_empty()
        && agent
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_');
    safe.then(|| format!("{REF_PREFIX}{agent}/{}", which.as_str()))
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

/// Point `refs/kete/sessions/<agent>/<which>` at `tree` in the repo at
/// `repo` (any of its checkouts), so `git gc` keeps it.
pub fn keep(repo: &Path, agent: &str, which: Which, tree: &str) -> bool {
    let Some(name) = ref_name(agent, which) else {
        return false;
    };
    git_ok(repo, &["update-ref", &name, tree]).is_some()
}

/// Delete both snapshot refs of each of `agents` in the repo at `repo`.
/// One transaction; refs that are not there are not an error.
pub fn drop_refs(repo: &Path, agents: &[String]) -> bool {
    let mut script = String::new();
    for agent in agents {
        for which in [Which::Start, Which::Last] {
            if let Some(name) = ref_name(agent, which) {
                script.push_str(&format!("delete {name}\n"));
            }
        }
    }
    if script.is_empty() {
        return true;
    }
    let mut cmd = command(repo, &["update-ref", "--stdin"]);
    cmd.stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    let Ok(mut child) = cmd.spawn() else {
        return false;
    };
    if let Some(mut stdin) = child.stdin.take() {
        use std::io::Write as _;
        let _ = stdin.write_all(script.as_bytes());
    }
    child.wait().is_ok_and(|s| s.success())
}

/// The agent ids that have snapshot refs in the repo at `repo`.
pub fn kept_agents(repo: &Path) -> Vec<String> {
    let Some(out) = git_ok(repo, &["for-each-ref", "--format=%(refname)", REF_PREFIX]) else {
        return Vec::new();
    };
    let mut ids: Vec<String> = String::from_utf8_lossy(&out)
        .lines()
        .filter_map(|r| r.strip_prefix(REF_PREFIX)?.split('/').next())
        .map(str::to_string)
        .collect();
    ids.dedup();
    ids
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
    /// session's refs leaves the others' alone.
    #[test]
    fn refs_keep_trees_and_go_with_their_session() {
        let dir = tempfile::tempdir().unwrap();
        let repo = repo(&dir);
        std::fs::write(repo.join("loose.txt"), "only in a snapshot\n").unwrap();
        let tree = take(&repo).tree.unwrap();
        assert!(keep(&repo, "A1", Which::Start, &tree));
        assert!(keep(&repo, "A1", Which::Last, &tree));
        assert!(keep(&repo, "B2", Which::Last, &tree));
        assert!(!keep(&repo, "../evil", Which::Start, &tree));
        assert_eq!(kept_agents(&repo), vec!["A1".to_string(), "B2".to_string()]);
        run(&repo, &["gc", "-q", "--prune=now"]);
        assert_eq!(run(&repo, &["cat-file", "-t", &tree]), "tree");
        // A missing ref in the batch is not an error.
        assert!(drop_refs(&repo, &["A1".into(), "C3".into()]));
        assert_eq!(kept_agents(&repo), vec!["B2".to_string()]);
        assert!(drop_refs(&repo, &[]));
        assert_eq!(
            ref_name("A1", Which::Last).unwrap(),
            "refs/kete/sessions/A1/last"
        );
        assert_eq!(ref_name("", Which::Last), None);
    }
}
