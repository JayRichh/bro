//! A worktree's changes, for the diff view, and its pull request, for the
//! kanban's PR open lane. Both shell out (`git`, `gh`) with no console
//! window, as the daemon's own helpers do.
//!
//! Diffs are pinned to commits: a range diff runs from the merge-base of
//! its two ends, and every blob is read at a named side. A side that cannot
//! be read is an error or an honest [`Blob`] state, never empty text.

use serde::{Deserialize, Serialize};
use std::path::{Component, Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};

/// Files above this are shown as too large rather than diffed.
const MAX_FILE_BYTES: u64 = 2 * 1024 * 1024;

#[derive(Debug, Clone, Serialize)]
pub struct DiffFile {
    pub path: String,
    /// Where a rename came from.
    pub from: Option<String>,
    /// `M`odified, `A`dded, `D`eleted, `R`enamed, `U`ntracked, `C`onflicted.
    pub status: char,
    pub added: Option<u64>,
    pub removed: Option<u64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DiffPair {
    pub original: String,
    pub modified: String,
    /// Why there is no text to compare (binary, too large), if so.
    pub note: Option<String>,
}

fn command(program: &str, cwd: &Path) -> Command {
    let mut cmd = Command::new(program);
    cmd.current_dir(cwd)
        .stdin(Stdio::null())
        .stderr(Stdio::null());
    #[cfg(windows)]
    nebula_core::platform::no_window(&mut cmd);
    cmd
}

/// Runs git and hands back the whole output, success or not. Only failing
/// to start git at all is an error here.
fn git_raw(cwd: &Path, args: &[&str]) -> Result<Output, String> {
    command("git", cwd)
        .args(["-c", "core.quotepath=off"])
        .args(args)
        .stderr(Stdio::piped())
        .output()
        .map_err(|e| format!("could not run git: {e}"))
}

fn git(cwd: &Path, args: &[&str]) -> Result<Vec<u8>, String> {
    let out = git_raw(cwd, args)?;
    if !out.status.success() {
        return Err(git_failure(args, &out));
    }
    Ok(out.stdout)
}

/// `git <verb> failed: <first stderr line>`.
fn git_failure(args: &[&str], out: &Output) -> String {
    let verb = args.first().unwrap_or(&"");
    let stderr = String::from_utf8_lossy(&out.stderr);
    match stderr.lines().map(str::trim).find(|l| !l.is_empty()) {
        Some(line) => format!("git {verb} failed: {line}"),
        None => format!("git {verb} failed"),
    }
}

/// Whether git exits cleanly, for the `--verify` and `-e` style probes.
fn git_ok(cwd: &Path, args: &[&str]) -> Result<bool, String> {
    Ok(git_raw(cwd, args)?.status.success())
}

fn git_text(cwd: &Path, args: &[&str]) -> Result<String, String> {
    let out = git(cwd, args)?;
    Ok(String::from_utf8_lossy(&out).trim().to_string())
}

/// Every changed path in the checkout: staged, unstaged and untracked, each
/// once, with line counts against HEAD where git has them.
pub fn changed_files(root: &Path) -> Result<Vec<DiffFile>, String> {
    let status = git(
        root,
        &["status", "--porcelain=v1", "-z", "--untracked-files=all"],
    )?;
    let mut files = Vec::new();
    let mut parts = status.split(|b| *b == 0).filter(|p| !p.is_empty());
    while let Some(entry) = parts.next() {
        if entry.len() < 4 {
            continue;
        }
        let (x, y) = (entry[0] as char, entry[1] as char);
        let path = String::from_utf8_lossy(&entry[3..]).into_owned();
        let renamed = x == 'R' || y == 'R' || x == 'C';
        let from = if renamed {
            parts
                .next()
                .map(|p| String::from_utf8_lossy(p).into_owned())
        } else {
            None
        };
        let status = match (x, y) {
            ('?', '?') => 'U',
            ('U', _) | (_, 'U') | ('A', 'A') | ('D', 'D') => 'C',
            _ if renamed => 'R',
            ('A', _) => 'A',
            ('D', _) | (_, 'D') => 'D',
            _ => 'M',
        };
        files.push(DiffFile {
            path,
            from,
            status,
            added: None,
            removed: None,
        });
    }
    // Line counts for tracked changes; a repo with no commit yet has none.
    if let Ok(numstat) = git(root, &["diff", "HEAD", "--numstat", "-z", "-M"]) {
        apply_numstat(&mut files, &numstat);
    }
    for f in files.iter_mut().filter(|f| f.status == 'U') {
        if let Ok(text) = std::fs::read(root.join(&f.path)) {
            if !is_binary(&text) {
                f.added = Some(text.split(|b| *b == b'\n').count() as u64);
                f.removed = Some(0);
            }
        }
    }
    files.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(files)
}

/// `git diff --numstat -z`: `add\tdel\tpath\0`, or for a rename
/// `add\tdel\t\0from\0to\0`. Binary files count `-`.
fn apply_numstat(files: &mut [DiffFile], raw: &[u8]) {
    let mut parts = raw.split(|b| *b == 0);
    while let Some(head) = parts.next() {
        if head.is_empty() {
            continue;
        }
        let text = String::from_utf8_lossy(head);
        let mut cols = text.splitn(3, '\t');
        let (Some(a), Some(d), Some(path)) = (cols.next(), cols.next(), cols.next()) else {
            continue;
        };
        let path = if path.is_empty() {
            let _from = parts.next();
            parts
                .next()
                .map(|p| String::from_utf8_lossy(p).into_owned())
                .unwrap_or_default()
        } else {
            path.to_string()
        };
        if let Some(f) = files.iter_mut().find(|f| f.path == path) {
            f.added = a.parse().ok();
            f.removed = d.parse().ok();
        }
    }
}

fn is_binary(bytes: &[u8]) -> bool {
    bytes.iter().take(8000).any(|b| *b == 0)
}

/// HEAD's side and the disk's side of one changed file. A side that should
/// exist and cannot be read is an error, not an empty editor.
pub fn file_pair(root: &Path, file: &DiffFile) -> Result<DiffPair, String> {
    let head_path = file.from.as_deref().unwrap_or(&file.path);
    let original = if matches!(file.status, 'A' | 'U') {
        Blob::Text(String::new())
    } else {
        match blob(root, &DiffSide::Commit("HEAD".into()), head_path)? {
            Blob::Missing => return Err(format!("{head_path} is missing at HEAD")),
            b => b,
        }
    };
    let modified = if file.status == 'D' {
        Blob::Text(String::new())
    } else {
        match blob(root, &DiffSide::Worktree, &file.path)? {
            Blob::Missing => return Err(format!("{} is missing on disk", file.path)),
            b => b,
        }
    };
    let note = |note: String| DiffPair {
        original: String::new(),
        modified: String::new(),
        note: Some(note),
    };
    match (original, modified) {
        (Blob::TooLarge(n), _) | (_, Blob::TooLarge(n)) => {
            Ok(note(format!("{} KB, too large to diff here", n / 1024)))
        }
        (Blob::Binary, _) | (_, Blob::Binary) => Ok(note("Binary file".into())),
        (Blob::Text(original), Blob::Text(modified)) => Ok(DiffPair {
            original,
            modified,
            note: None,
        }),
        // Missing was turned into an error above.
        _ => Err("unreadable diff side".into()),
    }
}

/// Where a blob is read from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "rev", rename_all = "lowercase")]
pub enum DiffSide {
    /// The file on disk.
    Worktree,
    /// The staged copy (stage 0).
    Index,
    /// A commit, by SHA or any rev git resolves to one.
    Commit(String),
}

/// One side of a diff, honestly: text, or why there is none.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "state", content = "value", rename_all = "camelCase")]
pub enum Blob {
    Text(String),
    /// The path does not exist on that side.
    Missing,
    /// Bytes on that side, over the cap.
    TooLarge(u64),
    /// A NUL byte in the first 8000 bytes, or not UTF-8.
    Binary,
}

impl Blob {
    fn from_bytes(bytes: Vec<u8>) -> Blob {
        if is_binary(&bytes) {
            return Blob::Binary;
        }
        match String::from_utf8(bytes) {
            Ok(text) => Blob::Text(text),
            Err(_) => Blob::Binary,
        }
    }
}

/// A repo-relative path, with forward slashes, that stays inside the repo.
fn repo_path(path: &str) -> Result<String, String> {
    let clean = path.replace('\\', "/");
    let p = Path::new(&clean);
    let escapes = p.components().any(|c| {
        matches!(
            c,
            Component::ParentDir | Component::RootDir | Component::Prefix(_)
        )
    });
    if clean.is_empty() || escapes {
        return Err(format!("{path} is not a path inside the repo"));
    }
    Ok(clean.trim_start_matches("./").to_string())
}

/// The content of `path` at `side`, capped at 2 MB.
pub fn blob(root: &Path, side: &DiffSide, path: &str) -> Result<Blob, String> {
    let path = repo_path(path)?;
    let spec = match side {
        DiffSide::Worktree => return worktree_blob(&root.join(&path)),
        DiffSide::Index => format!(":0:{path}"),
        DiffSide::Commit(rev) => {
            let commit = format!("{rev}^{{commit}}");
            if !git_ok(root, &["rev-parse", "--verify", "--quiet", &commit])? {
                return Err(format!("{rev} is not in this clone; fetch it first"));
            }
            format!("{rev}:{path}")
        }
    };
    if !git_ok(root, &["cat-file", "-e", &spec])? {
        return Ok(Blob::Missing);
    }
    let kind = git_text(root, &["cat-file", "-t", &spec])?;
    if kind != "blob" {
        return Err(format!("{path} is a {kind} there, not a file"));
    }
    let size: u64 = git_text(root, &["cat-file", "-s", &spec])?
        .parse()
        .map_err(|_| "git cat-file gave no size".to_string())?;
    if size > MAX_FILE_BYTES {
        return Ok(Blob::TooLarge(size));
    }
    Ok(Blob::from_bytes(git(root, &["cat-file", "blob", &spec])?))
}

fn worktree_blob(file: &Path) -> Result<Blob, String> {
    let meta = match std::fs::metadata(file) {
        Ok(m) => m,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Blob::Missing),
        Err(e) => return Err(format!("could not read {}: {e}", file.display())),
    };
    if meta.is_dir() {
        return Err(format!("{} is a folder, not a file", file.display()));
    }
    if meta.len() > MAX_FILE_BYTES {
        return Ok(Blob::TooLarge(meta.len()));
    }
    let bytes =
        std::fs::read(file).map_err(|e| format!("could not read {}: {e}", file.display()))?;
    Ok(Blob::from_bytes(bytes))
}

/// The commit `rev` names, as a full SHA.
pub fn resolve_commit(root: &Path, rev: &str) -> Result<String, String> {
    let commit = format!("{rev}^{{commit}}");
    git_text(root, &["rev-parse", "--verify", "--quiet", &commit])
        .map_err(|_| format!("{rev} is not in this clone; fetch it first"))
}

/// Where `base` and `tip` forked, as a SHA. The range diff runs from here,
/// so changes landed on `base` since the fork do not show as reverts.
pub fn merge_base(root: &Path, base: &str, tip: &str) -> Result<String, String> {
    let base = resolve_commit(root, base)?;
    let tip = resolve_commit(root, tip)?;
    git_text(root, &["merge-base", &base, &tip])
        .map_err(|_| "these commits share no history in this clone; fetch more of it".to_string())
}

/// The files `tip` changes since it forked from `base`.
pub fn diff_files_range(root: &Path, base: &str, tip: &str) -> Result<Vec<DiffFile>, String> {
    let from = merge_base(root, base, tip)?;
    let to = resolve_commit(root, tip)?;
    let names = git(
        root,
        &[
            "diff",
            "--name-status",
            "-z",
            "-M",
            "--no-ext-diff",
            &from,
            &to,
        ],
    )?;
    let mut files = parse_name_status(&names);
    let numstat = git(
        root,
        &["diff", "--numstat", "-z", "-M", "--no-ext-diff", &from, &to],
    )?;
    apply_numstat(&mut files, &numstat);
    files.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(files)
}

/// `git diff --name-status -z`: `X\0path\0`, or `R100\0from\0to\0`.
fn parse_name_status(raw: &[u8]) -> Vec<DiffFile> {
    let mut files = Vec::new();
    let mut parts = raw.split(|b| *b == 0).filter(|p| !p.is_empty());
    let text = |p: &[u8]| String::from_utf8_lossy(p).into_owned();
    while let Some(code) = parts.next() {
        let Some(&letter) = code.first() else {
            continue;
        };
        let Some(first) = parts.next() else {
            break;
        };
        let (from, path) = if matches!(letter, b'R' | b'C') {
            let Some(to) = parts.next() else {
                break;
            };
            (Some(text(first)), text(to))
        } else {
            (None, text(first))
        };
        let status = match letter {
            b'A' => 'A',
            b'D' => 'D',
            b'R' | b'C' => 'R',
            b'U' => 'C',
            _ => 'M',
        };
        // A copy keeps its source, so it reads as an add.
        let status = if letter == b'C' { 'A' } else { status };
        files.push(DiffFile {
            path,
            from,
            status,
            added: None,
            removed: None,
        });
    }
    files
}

/// One `@@` hunk of a file's diff, with the file header it needs to be
/// applied on its own.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Hunk {
    /// The `@@ -a,b +c,d @@` line.
    pub header: String,
    /// `diff --git`, `---`, `+++` and this hunk: a patch `git apply` takes.
    pub patch: String,
    pub old_start: u32,
    pub new_start: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum HunkAction {
    /// Worktree hunk into the index.
    Stage,
    /// Staged hunk back out of the index.
    Unstage,
    /// Worktree hunk reverted on disk.
    Discard,
}

/// The hunks of `path`: staged (index against HEAD) or not (worktree
/// against index). A binary or unchanged file has none.
pub fn hunks(root: &Path, path: &str, staged: bool) -> Result<Vec<Hunk>, String> {
    let path = repo_path(path)?;
    let mut args = vec!["diff", "-U3", "--no-color", "--no-ext-diff"];
    if staged {
        args.push("--cached");
    }
    args.extend(["--", &path]);
    let raw = git(root, &args)?;
    let text = String::from_utf8(raw)
        .map_err(|_| format!("{path} is not UTF-8 text; stage it whole instead"))?;
    Ok(parse_hunks(&text))
}

/// Splits one file's `git diff` output into self-contained hunk patches.
/// Lines keep their own endings, so CRLF content survives the round trip.
fn parse_hunks(diff: &str) -> Vec<Hunk> {
    let mut header = String::new();
    let mut hunks: Vec<Hunk> = Vec::new();
    for line in diff.split_inclusive('\n') {
        if line.starts_with("@@") {
            let (old_start, new_start) = hunk_starts(line);
            hunks.push(Hunk {
                header: line.trim_end().to_string(),
                patch: format!("{header}{line}"),
                old_start,
                new_start,
            });
        } else if let Some(h) = hunks.last_mut() {
            if line.starts_with("diff --git") {
                // A second file: not ours, as the diff is limited to one path.
                break;
            }
            h.patch.push_str(line);
        } else {
            header.push_str(line);
        }
    }
    hunks
}

/// `@@ -12,3 +14,5 @@ fn x` gives (12, 14).
fn hunk_starts(line: &str) -> (u32, u32) {
    let mut old = 0;
    let mut new = 0;
    for word in line.split_whitespace().skip(1) {
        let start = |w: &str| w[1..].split(',').next().and_then(|n| n.parse().ok());
        if word.starts_with('-') {
            old = start(word).unwrap_or(0);
        } else if word.starts_with('+') {
            new = start(word).unwrap_or(0);
            break;
        }
    }
    (old, new)
}

/// Stages, unstages or discards one hunk, through `git apply` on a temp
/// patch file. Stage and discard take unstaged hunks; unstage takes staged.
pub fn apply_hunk(root: &Path, hunk: &Hunk, action: HunkAction) -> Result<(), String> {
    if !hunk.patch.starts_with("diff --git") || !hunk.patch.contains("\n@@") {
        return Err("that hunk has no file header to apply against".into());
    }
    let patch = TempPatch::write(&hunk.patch)?;
    let file = patch.0.to_string_lossy().into_owned();
    let mut args = vec!["apply", "--whitespace=nowarn"];
    match action {
        HunkAction::Stage => args.push("--cached"),
        HunkAction::Unstage => args.extend(["--cached", "-R"]),
        HunkAction::Discard => args.push("-R"),
    }
    args.push(&file);
    git(root, &args).map(|_| ())
}

/// A patch file in the temp dir, removed when dropped.
struct TempPatch(PathBuf);

impl TempPatch {
    fn write(text: &str) -> Result<TempPatch, String> {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let name = format!(
            "kete-hunk-{}-{}.patch",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        );
        let path = std::env::temp_dir().join(name);
        std::fs::write(&path, text.as_bytes())
            .map_err(|e| format!("could not write the hunk patch: {e}"))?;
        Ok(TempPatch(path))
    }
}

impl Drop for TempPatch {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

/// Fetches from `origin` whichever of `refs` this clone lacks: a full ref
/// (`refs/pull/12/merge`) lands under the same name, a SHA is fetched by id,
/// and a branch (`main`, `origin/main`) lands in `refs/remotes/origin/`.
/// A shallow clone where the first two refs share no history is deepened
/// by 200 commits.
pub fn ensure_refs(root: &Path, refs: &[&str]) -> Result<(), String> {
    for r in refs {
        let r = r.trim();
        if r.is_empty() {
            return Err("an empty ref was asked for".into());
        }
        if has_commit(root, r)? {
            continue;
        }
        let refspec = if r.starts_with("refs/") {
            format!("+{r}:{r}")
        } else if is_sha(r) {
            r.to_string()
        } else {
            let branch = r.strip_prefix("origin/").unwrap_or(r);
            format!("+refs/heads/{branch}:refs/remotes/origin/{branch}")
        };
        git(root, &["fetch", "--quiet", "--no-tags", "origin", &refspec])?;
        if !has_commit(root, &commit_name(root, r)?)? {
            return Err(format!("{r} is not on origin"));
        }
    }
    if let [a, b, ..] = refs {
        let shallow = git_text(root, &["rev-parse", "--is-shallow-repository"])? == "true";
        if shallow && merge_base(root, &commit_name(root, a)?, &commit_name(root, b)?).is_err() {
            git(root, &["fetch", "--quiet", "--deepen=200", "origin"])?;
        }
    }
    Ok(())
}

fn has_commit(root: &Path, rev: &str) -> Result<bool, String> {
    let commit = format!("{rev}^{{commit}}");
    git_ok(root, &["rev-parse", "--verify", "--quiet", &commit])
}

/// `rev` if it resolves, else the `origin/` branch [`ensure_refs`] fetched
/// it into.
pub fn commit_name(root: &Path, rev: &str) -> Result<String, String> {
    if has_commit(root, rev)? {
        return Ok(rev.to_string());
    }
    Ok(format!(
        "origin/{}",
        rev.strip_prefix("origin/").unwrap_or(rev)
    ))
}

fn is_sha(s: &str) -> bool {
    (7..=64).contains(&s.len()) && s.bytes().all(|b| b.is_ascii_hexdigit())
}

/// The open pull request for `branch`, asked of `gh` in the checkout.
/// None when there is none, or no `gh`, or no network.
pub fn open_pr(root: &Path, branch: &str) -> Option<crate::daemon::PrInfo> {
    let out = command("gh", root)
        .args([
            "pr",
            "view",
            branch,
            "--json",
            "number,url,title,state,isDraft",
        ])
        .stdout(Stdio::piped())
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).ok()?;
    if v.get("state")?.as_str()? != "OPEN" {
        return None;
    }
    Some(crate::daemon::PrInfo {
        number: v.get("number")?.as_u64()?,
        url: v.get("url")?.as_str()?.to_string(),
        title: v.get("title")?.as_str().unwrap_or_default().to_string(),
        draft: v.get("isDraft").and_then(|d| d.as_bool()).unwrap_or(false),
    })
}

/// Whether `gh` is on PATH at all, so the lookups are skipped cleanly.
pub fn have_gh() -> bool {
    let mut cmd = Command::new("gh");
    cmd.arg("--version")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    #[cfg(windows)]
    nebula_core::platform::no_window(&mut cmd);
    cmd.status().map(|s| s.success()).unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numstat_fills_plain_and_renamed_entries() {
        let mut files = vec![
            DiffFile {
                path: "a.rs".into(),
                from: None,
                status: 'M',
                added: None,
                removed: None,
            },
            DiffFile {
                path: "new.rs".into(),
                from: Some("old.rs".into()),
                status: 'R',
                added: None,
                removed: None,
            },
        ];
        apply_numstat(&mut files, b"3\t1\ta.rs\x002\t0\t\x00old.rs\x00new.rs\x00");
        assert_eq!((files[0].added, files[0].removed), (Some(3), Some(1)));
        assert_eq!((files[1].added, files[1].removed), (Some(2), Some(0)));
    }

    /// A throwaway repo under the temp dir, removed when dropped.
    struct Repo(PathBuf);

    impl Repo {
        fn new(tag: &str) -> Repo {
            static NEXT: AtomicU64 = AtomicU64::new(0);
            let dir = std::env::temp_dir().join(format!(
                "kete-git-test-{tag}-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).unwrap();
            let repo = Repo(dir);
            repo.git(&["init", "-q", "-b", "main"]);
            repo.git(&["config", "user.name", "Kete Test"]);
            repo.git(&["config", "user.email", "test@example.invalid"]);
            repo.git(&["config", "core.autocrlf", "false"]);
            repo.git(&["config", "commit.gpgsign", "false"]);
            repo
        }

        fn git(&self, args: &[&str]) -> String {
            let out = git_raw(&self.0, args).unwrap();
            assert!(
                out.status.success(),
                "git {args:?}: {}",
                String::from_utf8_lossy(&out.stderr)
            );
            String::from_utf8_lossy(&out.stdout).trim().to_string()
        }

        fn write(&self, path: &str, bytes: &[u8]) {
            let p = self.0.join(path);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(p, bytes).unwrap();
        }

        fn read(&self, path: &str) -> String {
            std::fs::read_to_string(self.0.join(path)).unwrap()
        }

        fn commit(&self, msg: &str) -> String {
            self.git(&["add", "-A"]);
            self.git(&["commit", "-q", "-m", msg]);
            self.git(&["rev-parse", "HEAD"])
        }

        fn staged(&self, path: &str) -> String {
            self.git(&["show", &format!(":{path}")])
        }
    }

    impl Drop for Repo {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    /// Twenty numbered lines with `eol` endings.
    fn lines(eol: &str) -> String {
        (1..=20).map(|n| format!("line {n}{eol}")).collect()
    }

    /// Lines 2 and 18 changed: two hunks, far enough apart at -U3.
    fn two_edits(eol: &str) -> String {
        lines(eol)
            .replace(&format!("line 2{eol}"), &format!("line two{eol}"))
            .replace(&format!("line 18{eol}"), &format!("line eighteen{eol}"))
    }

    #[test]
    fn a_path_missing_at_a_commit_is_missing_not_empty() {
        let repo = Repo::new("missing");
        repo.write("a.txt", b"hello\n");
        let sha = repo.commit("one");
        let side = DiffSide::Commit(sha);
        assert_eq!(blob(&repo.0, &side, "nope.txt").unwrap(), Blob::Missing);
        assert_eq!(
            blob(&repo.0, &side, "a.txt").unwrap(),
            Blob::Text("hello\n".into())
        );
        assert_eq!(
            blob(&repo.0, &DiffSide::Worktree, "gone.txt").unwrap(),
            Blob::Missing
        );
        assert_eq!(
            blob(&repo.0, &DiffSide::Index, "gone.txt").unwrap(),
            Blob::Missing
        );
    }

    #[test]
    fn a_commit_not_in_the_clone_is_an_error() {
        let repo = Repo::new("nocommit");
        repo.write("a.txt", b"hello\n");
        repo.commit("one");
        let side = DiffSide::Commit("0123456789abcdef0123456789abcdef01234567".into());
        let e = blob(&repo.0, &side, "a.txt").unwrap_err();
        assert!(e.contains("not in this clone"), "{e}");
    }

    #[test]
    fn a_path_outside_the_repo_is_refused() {
        let repo = Repo::new("escape");
        assert!(blob(&repo.0, &DiffSide::Worktree, "../x").is_err());
        assert!(blob(&repo.0, &DiffSide::Worktree, "/etc/passwd").is_err());
    }

    #[test]
    fn a_3mb_file_is_too_large_on_every_side() {
        let repo = Repo::new("large");
        let big = vec![b'a'; 3 * 1024 * 1024];
        repo.write("big.txt", &big);
        let sha = repo.commit("big");
        let size = big.len() as u64;
        for side in [DiffSide::Worktree, DiffSide::Index, DiffSide::Commit(sha)] {
            assert_eq!(
                blob(&repo.0, &side, "big.txt").unwrap(),
                Blob::TooLarge(size),
                "{side:?}"
            );
        }
    }

    #[test]
    fn a_nul_byte_is_binary_on_every_side() {
        let repo = Repo::new("binary");
        repo.write("bin.dat", b"abc\0def");
        let sha = repo.commit("bin");
        for side in [DiffSide::Worktree, DiffSide::Index, DiffSide::Commit(sha)] {
            assert_eq!(
                blob(&repo.0, &side, "bin.dat").unwrap(),
                Blob::Binary,
                "{side:?}"
            );
        }
    }

    #[test]
    fn file_pair_errors_instead_of_going_blank() {
        let repo = Repo::new("pair");
        repo.write("a.txt", b"one\n");
        repo.commit("one");
        repo.write("a.txt", b"two\n");
        let ok = DiffFile {
            path: "a.txt".into(),
            from: None,
            status: 'M',
            added: None,
            removed: None,
        };
        let pair = file_pair(&repo.0, &ok).unwrap();
        assert_eq!(
            (pair.original.as_str(), pair.modified.as_str()),
            ("one\n", "two\n")
        );
        let ghost = DiffFile {
            path: "ghost.txt".into(),
            ..ok
        };
        assert!(file_pair(&repo.0, &ghost).is_err());
    }

    #[test]
    fn staging_hunk_one_leaves_hunk_two_unstaged() {
        for eol in ["\n", "\r\n"] {
            let repo = Repo::new("stage");
            repo.write("f.txt", lines(eol).as_bytes());
            repo.commit("base");
            repo.write("f.txt", two_edits(eol).as_bytes());

            let unstaged = hunks(&repo.0, "f.txt", false).unwrap();
            assert_eq!(unstaged.len(), 2, "{eol:?}");
            assert!(unstaged[0].patch.starts_with("diff --git"));
            assert_eq!(unstaged[0].old_start, 1);
            assert!(unstaged[1].old_start > 10);

            apply_hunk(&repo.0, &unstaged[0], HunkAction::Stage).unwrap();

            let index = repo.staged("f.txt");
            assert!(index.contains("line two"), "{eol:?}");
            assert!(!index.contains("line eighteen"), "{eol:?}");
            let staged = hunks(&repo.0, "f.txt", true).unwrap();
            assert_eq!(staged.len(), 1);
            assert_eq!(staged[0].old_start, 1);
            let left = hunks(&repo.0, "f.txt", false).unwrap();
            assert_eq!(left.len(), 1);
            assert!(left[0].patch.contains("+line eighteen"));
            // The disk is untouched, CRLF and all.
            assert_eq!(repo.read("f.txt"), two_edits(eol));

            apply_hunk(&repo.0, &staged[0], HunkAction::Unstage).unwrap();
            assert!(hunks(&repo.0, "f.txt", true).unwrap().is_empty());
            assert_eq!(repo.staged("f.txt").replace('\r', ""), lines("\n").trim());
        }
    }

    #[test]
    fn discard_reverts_only_that_hunk() {
        for eol in ["\n", "\r\n"] {
            let repo = Repo::new("discard");
            repo.write("f.txt", lines(eol).as_bytes());
            repo.commit("base");
            repo.write("f.txt", two_edits(eol).as_bytes());

            let unstaged = hunks(&repo.0, "f.txt", false).unwrap();
            apply_hunk(&repo.0, &unstaged[1], HunkAction::Discard).unwrap();

            let want = lines(eol).replace(&format!("line 2{eol}"), &format!("line two{eol}"));
            assert_eq!(repo.read("f.txt"), want, "{eol:?}");
            let left = hunks(&repo.0, "f.txt", false).unwrap();
            assert_eq!(left.len(), 1);
            assert!(left[0].patch.contains("+line two"));
        }
    }

    #[test]
    fn a_hunk_without_a_file_header_is_refused() {
        let repo = Repo::new("header");
        let bare = Hunk {
            header: "@@ -1 +1 @@".into(),
            patch: "@@ -1 +1 @@\n-a\n+b\n".into(),
            old_start: 1,
            new_start: 1,
        };
        assert!(apply_hunk(&repo.0, &bare, HunkAction::Stage).is_err());
    }

    #[test]
    fn range_diff_runs_from_the_merge_base_not_the_target_tip() {
        let repo = Repo::new("range");
        repo.write("shared.txt", b"base\n");
        let fork = repo.commit("base");
        repo.git(&["checkout", "-q", "-b", "feature"]);
        repo.write("feature.txt", b"mine\n");
        repo.commit("feature work");
        repo.git(&["checkout", "-q", "main"]);
        repo.write("landed.txt", b"theirs\n");
        repo.write("shared.txt", b"base\nmore\n");
        repo.commit("main moved on");

        assert_eq!(merge_base(&repo.0, "main", "feature").unwrap(), fork);
        let files = diff_files_range(&repo.0, "main", "feature").unwrap();
        let paths: Vec<_> = files.iter().map(|f| (f.path.as_str(), f.status)).collect();
        assert_eq!(paths, vec![("feature.txt", 'A')]);
        assert_eq!((files[0].added, files[0].removed), (Some(1), Some(0)));
    }

    #[test]
    fn range_diff_reports_renames_and_deletes() {
        let repo = Repo::new("rename");
        repo.write("old.txt", lines("\n").as_bytes());
        repo.write("dead.txt", b"x\n");
        let base = repo.commit("base");
        repo.git(&["mv", "old.txt", "new.txt"]);
        repo.git(&["rm", "-q", "dead.txt"]);
        let tip = repo.commit("move");
        let files = diff_files_range(&repo.0, &base, &tip).unwrap();
        assert_eq!(files.len(), 2);
        assert_eq!((files[0].path.as_str(), files[0].status), ("dead.txt", 'D'));
        assert_eq!(files[1].path, "new.txt");
        assert_eq!(files[1].status, 'R');
        assert_eq!(files[1].from.as_deref(), Some("old.txt"));
    }

    #[test]
    fn ensure_refs_fetches_a_missing_pull_merge_ref() {
        let origin = Repo::new("origin");
        origin.write("a.txt", b"a\n");
        origin.commit("one");
        origin.git(&["checkout", "-q", "-b", "topic"]);
        origin.write("b.txt", b"b\n");
        let pr = origin.commit("pr");
        origin.git(&["update-ref", "refs/pull/7/merge", &pr]);
        origin.git(&["checkout", "-q", "main"]);

        let clone = Repo::new("clone");
        let url = origin.0.to_string_lossy().replace('\\', "/");
        clone.git(&["remote", "add", "origin", &url]);
        clone.git(&["fetch", "-q", "origin", "main"]);
        assert!(!has_commit(&clone.0, "refs/pull/7/merge").unwrap());

        ensure_refs(&clone.0, &["main", "refs/pull/7/merge"]).unwrap();
        assert_eq!(clone.git(&["rev-parse", "refs/pull/7/merge"]), pr);
        assert!(has_commit(&clone.0, &commit_name(&clone.0, "main").unwrap()).unwrap());
        let files = diff_files_range(&clone.0, "origin/main", "refs/pull/7/merge").unwrap();
        assert_eq!(files.len(), 1);

        let e = ensure_refs(&clone.0, &["refs/pull/99/merge"]).unwrap_err();
        assert!(e.contains("failed") || e.contains("not on origin"), "{e}");
    }

    #[test]
    fn hunk_starts_reads_both_sides() {
        assert_eq!(hunk_starts("@@ -12,3 +14,5 @@ fn x"), (12, 14));
        assert_eq!(hunk_starts("@@ -1 +1 @@"), (1, 1));
        assert_eq!(hunk_starts("@@ -0,0 +1,2 @@"), (0, 1));
    }
}
