//! Git status/diff readers for the diff modal.
//!
//! The readers are synchronous `std::process`; who calls them is what
//! changed. They used to run inside the key handler — opening the modal,
//! switching files — and the loop waited: 47 ms to open on a small checkout
//! and 11 ms per file walked under the INPUT LATENCY PROBE, 80 ms to over
//! a second for the `git status` alone on a large one. A view with
//! BACKGROUND READS (`view_jobs`) runs them on the blocking pool instead;
//! one without — every view a test builds — still calls them inline.

use crate::app::DiffView;
use std::path::Path;
use std::process::{Command, Output};

/// Keep pathological diffs from bloating the overlay state.
pub const MAX_DIFF_LINES: usize = 20_000;

/// One changed file from `git status --porcelain=v1 -z`.
#[derive(Debug, Clone, PartialEq)]
pub struct DiffFile {
    /// Path relative to the worktree root (for renames: the NEW path).
    pub path: String,
    /// Pre-rename path for R/C entries.
    pub orig_path: Option<String>,
    /// The two porcelain status columns, e.g. ['M',' '], [' ','M'], ['?','?'].
    pub xy: [char; 2],
}

impl DiffFile {
    pub fn is_untracked(&self) -> bool {
        self.xy == ['?', '?']
    }

    /// The raw two-character code for the list column ("M ", " M", "??", …).
    pub fn status_str(&self) -> String {
        self.xy.iter().collect()
    }
}

/// Line classification for coloring; styling itself lives in ui.rs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiffLineKind {
    Add,
    Remove,
    Hunk,
    Header,
    Context,
}

pub fn classify_diff_line(line: &str) -> DiffLineKind {
    const HEADERS: [&str; 11] = [
        "diff --git",
        "index ",
        "new file",
        "deleted file",
        "similarity ",
        "dissimilarity ",
        "rename ",
        "copy ",
        "old mode",
        "new mode",
        "Binary files",
    ];
    if line.starts_with("+++") || line.starts_with("---") {
        DiffLineKind::Header
    } else if line.starts_with('+') {
        DiffLineKind::Add
    } else if line.starts_with('-') {
        DiffLineKind::Remove
    } else if line.starts_with("@@") {
        DiffLineKind::Hunk
    } else if HEADERS.iter().any(|h| line.starts_with(h)) {
        DiffLineKind::Header
    } else {
        DiffLineKind::Context
    }
}

/// `git -C root`, with the locks git takes on its own initiative switched
/// off. `git status` and `git diff` refresh the index's stat cache as a
/// side effect and, when it is stale — it always is while an agent edits
/// files — write it back through `.git/index.lock`. The WORKTREES PANEL's
/// changed-files badge polls `status` every two seconds on the checkout
/// the user is working in, so left on, that write lands under the agent's
/// own `git add` / `commit` / `checkout`, which then fails with "Unable to
/// create '.git/index.lock': File exists" (issue #15). Nothing the TUI
/// runs needs the refresh persisted — the agent CLIs themselves run their
/// background git with `--no-optional-locks` for the same reason — and
/// commands that must lock (the BRANCH SWITCHER's switch, stash and commit) still do: `GIT_OPTIONAL_LOCKS=0`
/// only skips the optional ones. Every TUI-side git goes through here.
pub(crate) fn git_command(root: &Path) -> Command {
    let mut cmd = Command::new("git");
    cmd.arg("-C").arg(root).env("GIT_OPTIONAL_LOCKS", "0");
    cmd
}

pub(crate) fn run_git(root: &Path, args: &[&str]) -> Result<Output, String> {
    git_command(root)
        .args(args)
        .output()
        .map_err(|e| format!("failed to run git: {e}"))
}

/// Changed files (staged + unstaged + untracked) in status order.
/// `Err` is a user-facing flash message.
pub fn changed_files(root: &Path) -> Result<Vec<DiffFile>, String> {
    let output = run_git(root, &["status", "--porcelain=v1", "-z", "-uall"])?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("git status failed: {}", stderr.trim()));
    }
    Ok(parse_status_z(&output.stdout))
}

/// Parse NUL-separated porcelain v1: `XY path\0`, and for X ∈ {R, C} a second
/// NUL-terminated field holds the ORIGINAL path (`XY new\0old\0`).
pub fn parse_status_z(bytes: &[u8]) -> Vec<DiffFile> {
    let mut files = Vec::new();
    let mut fields = bytes.split(|b| *b == 0);
    while let Some(entry) = fields.next() {
        let entry = String::from_utf8_lossy(entry);
        if entry.len() < 4 {
            continue;
        }
        let mut chars = entry.chars();
        let x = chars.next().unwrap_or(' ');
        let y = chars.next().unwrap_or(' ');
        let path = entry[3..].to_string();
        let orig_path = if matches!(x, 'R' | 'C') {
            fields
                .next()
                .map(|f| String::from_utf8_lossy(f).into_owned())
        } else {
            None
        };
        files.push(DiffFile {
            path,
            orig_path,
            xy: [x, y],
        });
    }
    files
}

/// Lines added and removed across a checkout's uncommitted changes, as the
/// DIFF VIEWER shows them: tracked files against HEAD, staged or not, and
/// every line of an untracked file as added. What the LAUNCHER VIEW's cards
/// print after their changed-file count (CARD LINE COUNTS).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct LineChanges {
    pub added: u64,
    pub removed: u64,
}

impl LineChanges {
    pub fn is_empty(self) -> bool {
        self.added == 0 && self.removed == 0
    }
}

/// The biggest untracked file read to count its lines; a bigger one — a
/// build artifact, a dump — counts nothing rather than stall the read.
const UNTRACKED_FILE_CAP: u64 = 1 << 20;
/// The most untracked bytes one count reads, so a checkout full of new
/// files costs a bounded amount per poll; files past it count nothing.
const UNTRACKED_READ_BUDGET: u64 = 16 << 20;

/// Count the changed lines behind `files`, the list `changed_files` just
/// read there: one `git diff --numstat` for the tracked ones — against
/// HEAD, or git's empty tree on a checkout with no commit yet — and a read
/// from disk for each untracked one. None when git couldn't say.
pub fn line_changes(root: &Path, files: &[DiffFile]) -> Option<LineChanges> {
    let mut total = LineChanges::default();
    if files.iter().any(|f| !f.is_untracked()) {
        total = match tracked_line_changes(root, "HEAD") {
            Some(lines) => lines,
            // No commit yet: every tracked line is new.
            None if !has_head(root) => tracked_line_changes(root, &empty_tree(root)?)?,
            None => return None,
        };
    }
    total.added += untracked_lines(root, files);
    Some(total)
}

/// `git diff <base> --numstat`, summed.
fn tracked_line_changes(root: &Path, base: &str) -> Option<LineChanges> {
    let output = run_git(
        root,
        &[
            "diff",
            base,
            "--numstat",
            "-z",
            "--no-color",
            "--no-ext-diff",
            "--",
        ],
    )
    .ok()?;
    output
        .status
        .success()
        .then(|| parse_numstat_z(&output.stdout))
}

/// Git's empty tree in this repo's hash, for an unborn HEAD to diff from.
fn empty_tree(root: &Path) -> Option<String> {
    let output = run_git(root, &["hash-object", "-t", "tree", "/dev/null"]).ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).trim().to_string())
}

/// Sum `git diff --numstat -z`: `added\tremoved\tpath\0` per file, and for
/// a rename or copy `added\tremoved\t\0old\0new\0`. A binary file reads
/// `-\t-` and adds nothing.
pub fn parse_numstat_z(bytes: &[u8]) -> LineChanges {
    let mut total = LineChanges::default();
    let mut fields = bytes.split(|b| *b == 0);
    while let Some(field) = fields.next() {
        let field = String::from_utf8_lossy(field);
        let mut parts = field.splitn(3, '\t');
        let (Some(added), Some(removed), Some(path)) = (parts.next(), parts.next(), parts.next())
        else {
            continue;
        };
        total.added += added.parse::<u64>().unwrap_or(0);
        total.removed += removed.parse::<u64>().unwrap_or(0);
        if path.is_empty() {
            // A rename: its two paths are the next two fields.
            fields.next();
            fields.next();
        }
    }
    total
}

/// Every line of the untracked `files`, read from disk within the caps
/// above; a directory (a nested repo), a symlink or a binary file counts
/// nothing, as it adds no text lines to the diff.
fn untracked_lines(root: &Path, files: &[DiffFile]) -> u64 {
    let mut budget = UNTRACKED_READ_BUDGET;
    let mut lines = 0;
    for file in files.iter().filter(|f| f.is_untracked()) {
        let path = root.join(&file.path);
        let Ok(meta) = std::fs::symlink_metadata(&path) else {
            continue;
        };
        if !meta.is_file() || meta.len() > UNTRACKED_FILE_CAP || meta.len() > budget {
            continue;
        }
        budget -= meta.len();
        if let Ok(bytes) = std::fs::read(&path) {
            lines += count_lines(&bytes);
        }
    }
    lines
}

/// A file's lines as `git diff --numstat` counts them: every newline, plus
/// a last line without one; none for a binary file (a NUL in its first
/// 8000 bytes, git's own test).
fn count_lines(bytes: &[u8]) -> u64 {
    if bytes.iter().take(8000).any(|b| *b == 0) {
        return 0;
    }
    let newlines = bytes.iter().filter(|b| **b == b'\n').count() as u64;
    newlines + u64::from(bytes.last().is_some_and(|b| *b != b'\n'))
}

/// Every file in the checkout (tracked + untracked, gitignore respected) in
/// git listing order, for the fuzzy file finder. `Err` is a user-facing
/// flash message.
pub fn list_files(root: &Path) -> Result<Vec<String>, String> {
    let output = run_git(
        root,
        &[
            "ls-files",
            "-z",
            "--cached",
            "--others",
            "--exclude-standard",
        ],
    )?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("git ls-files failed: {}", stderr.trim()));
    }
    Ok(output
        .stdout
        .split(|b| *b == 0)
        .filter(|p| !p.is_empty())
        .map(|p| String::from_utf8_lossy(p).into_owned())
        .collect())
}

/// Does this checkout have any commit? Unborn HEAD changes the diff command.
pub fn has_head(root: &Path) -> bool {
    head_oid(root).is_some()
}

/// HEAD's commit OID, `None` on an unborn HEAD (or outside a repo). The
/// review marks are scoped to this: any HEAD move invalidates them.
pub fn head_oid(root: &Path) -> Option<String> {
    let output = run_git(root, &["rev-parse", "--verify", "--quiet", "HEAD"]).ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).trim().to_string())
}

/// Diff text for one file: its uncommitted changes, or (given a commit)
/// what that commit changed in it against its first parent. Never fails:
/// errors become the displayed text so the modal survives a repo vanishing
/// out from under it.
pub fn diff_for(root: &Path, file: &DiffFile, head_ok: bool, commit: Option<&str>) -> String {
    diff_with(root, file, head_ok, commit, None, None)
}

/// [`diff_for`] of a working-tree file against `base` (a merge-base SHA)
/// when given one — what was committed since and what is not yet,
/// together — else against HEAD.
pub fn diff_since(root: &Path, file: &DiffFile, head_ok: bool, base: Option<&str>) -> String {
    diff_with(root, file, head_ok, None, None, base)
}

/// How much context the FULL CONTEXT read asks git for: more lines than
/// any file the pane would show has, so one hunk is the whole file.
pub const FULL_CONTEXT: &str = "-U1000000";

/// [`diff_for`] with the whole file as context: what the DIFF VIEWER shows
/// with unchanged lines on (`Ctrl+o`). Read for display only — the ✓ marks
/// fingerprint `diff_for`'s three-line text, which this never replaces.
pub fn full_context_diff(root: &Path, file: &DiffFile, head_ok: bool) -> String {
    diff_with(root, file, head_ok, None, Some(FULL_CONTEXT), None)
}

fn diff_with(
    root: &Path,
    file: &DiffFile,
    head_ok: bool,
    commit: Option<&str>,
    context: Option<&str>,
    base: Option<&str>,
) -> String {
    let output = if let Some(sha) = commit {
        let mut args = vec![
            "show",
            "--format=",
            "--no-color",
            "--no-ext-diff",
            "-M",
            "--diff-merges=first-parent",
            sha,
            "--",
            &file.path,
        ];
        if let Some(orig) = &file.orig_path {
            args.push(orig);
        }
        run_git(root, &args)
    } else if file.is_untracked() {
        // --no-index exits 1 when the files differ; only >= 2 is an error.
        run_git(
            root,
            &[
                "diff",
                "--no-index",
                "--no-color",
                "--",
                "/dev/null",
                &file.path,
            ],
        )
    } else {
        let mut args = vec!["diff"];
        match base {
            Some(sha) => args.push(sha),
            None if head_ok => args.push("HEAD"),
            None => {}
        }
        args.extend(context);
        args.extend(["--no-color", "--no-ext-diff", "--", &file.path]);
        if let Some(orig) = &file.orig_path {
            args.push(orig);
        }
        run_git(root, &args)
    };
    let output = match output {
        Ok(o) => o,
        Err(e) => return e,
    };
    let ok = if file.is_untracked() {
        matches!(output.status.code(), Some(0) | Some(1))
    } else {
        output.status.success()
    };
    if !ok {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return format!("git diff failed: {}", stderr.trim());
    }
    let text = String::from_utf8_lossy(&output.stdout);
    if text.trim().is_empty() {
        return "(no textual changes)".to_string();
    }
    cap_lines(&text, MAX_DIFF_LINES, false)
}

/// The zero-context diff of `file` between `from` (the index, or HEAD
/// with `head`) and the worktree, as raw bytes split into hunks — the
/// stuff a CHANGE's patch is cut from. Raw because the text the pane
/// shows is decoded lossily, capped and stripped of its `\r`s, none of
/// which `git apply` would take.
fn zero_context_hunks(
    root: &Path,
    file: &DiffFile,
    head: bool,
) -> Result<(Vec<u8>, Vec<crate::diff_layout::RawHunk>), String> {
    let mut args = vec!["diff"];
    if head {
        args.push("HEAD");
    }
    args.extend(["-U0", "--no-color", "--no-ext-diff", "--", &file.path]);
    let output = run_git(root, &args)?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("git diff failed: {}", stderr.trim()));
    }
    Ok(crate::diff_layout::split_patch(&output.stdout))
}

/// What of `file` is not staged yet: the zero-context hunks between the
/// index and the worktree, by range. A CHANGE none of them covers is
/// staged in full (`diff_layout::covers`). None where git could not say.
pub fn unstaged_hunks(root: &Path, file: &DiffFile) -> Option<Vec<crate::diff_layout::RawHunk>> {
    if file.is_untracked() {
        return None;
    }
    let (_, hunks) = zero_context_hunks(root, file, false).ok()?;
    Some(
        hunks
            .into_iter()
            .map(|h| crate::diff_layout::RawHunk {
                bytes: Vec::new(),
                ..h
            })
            .collect(),
    )
}

/// `git apply <args> -` with `patch` on stdin, in `root`.
fn apply(root: &Path, args: &[&str], patch: &[u8]) -> Result<(), String> {
    use std::io::Write;
    use std::process::Stdio;
    let mut child = git_command(root)
        .arg("apply")
        .args(args)
        .arg("-")
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("failed to run git: {e}"))?;
    if let Some(mut stdin) = child.stdin.take() {
        stdin
            .write_all(patch)
            .map_err(|e| format!("git apply: {e}"))?;
    }
    let output = child
        .wait_with_output()
        .map_err(|e| format!("git apply: {e}"))?;
    if output.status.success() {
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        Err(format!("git apply failed: {}", stderr.trim()))
    }
}

/// ACCEPT a CHANGE: stage what of it is still unstaged — the zero-context
/// hunks between the index and the worktree that cover it, applied to the
/// index (`git apply --cached --unidiff-zero`). The worktree is not
/// touched. `Ok(false)` when nothing of it was left to stage.
pub fn stage_change(
    root: &Path,
    file: &DiffFile,
    change: &crate::diff_layout::Change,
) -> Result<bool, String> {
    let (header, hunks) = zero_context_hunks(root, file, false)?;
    let Some(patch) = crate::diff_layout::patch_for(&header, &hunks, change) else {
        return Ok(false);
    };
    apply(root, &["--cached", "--unidiff-zero"], &patch)?;
    Ok(true)
}

/// REJECT a CHANGE: put the worktree's lines back as HEAD has them — the
/// zero-context hunks between HEAD (the index, on an unborn HEAD) and the
/// worktree that cover it, applied in reverse to the worktree
/// (`git apply -R --unidiff-zero`). The index is not touched, so a part
/// already staged stays staged. `Ok(false)` when nothing covered it.
pub fn discard_change(
    root: &Path,
    file: &DiffFile,
    head_ok: bool,
    change: &crate::diff_layout::Change,
) -> Result<bool, String> {
    let (header, hunks) = zero_context_hunks(root, file, head_ok)?;
    let Some(patch) = crate::diff_layout::patch_for(&header, &hunks, change) else {
        return Ok(false);
    };
    apply(root, &["-R", "--unidiff-zero"], &patch)?;
    Ok(true)
}

/// What a capped text ends with, so the pane says why it stops short.
pub const TRUNCATED_MARK: &str = "\n… (truncated)";

/// The first `max` lines of `text`, joined, ending in [`TRUNCATED_MARK`]
/// when lines were dropped — or when `already_cut` says the caller trimmed
/// the text before handing it over (a byte cap), so the mark still shows.
/// Shared by the diff pane and the tree preview so both stop the same way.
pub fn cap_lines(text: &str, max: usize, already_cut: bool) -> String {
    let mut lines = text.lines();
    let mut out: String = lines.by_ref().take(max).collect::<Vec<_>>().join("\n");
    if lines.next().is_some() || already_cut {
        out.push_str(TRUNCATED_MARK);
    }
    out
}

/// Everything opening the DIFF VIEWER reads: the changed files, HEAD, and
/// the reviewed ✓ marks that still apply — each stored mark checked against
/// its file's diff as it is now, one `git diff` per mark, and the pruned
/// set written back. Off the loop for a view with BACKGROUND READS.
///
/// Read against the base branch (`nebula_core::review_base`) when there is
/// one and `asked` — the reader's last pick — does not say HEAD: the
/// files changed since the merge-base, committed or not, and the commits
/// since it. Its marks are the base's, scoped to the merge-base.
pub fn read_listing(
    root: &Path,
    asked: Option<nebula_core::review_base::Against>,
) -> Result<crate::view_jobs::DiffListing, String> {
    read_listing_with(root, asked, None)
}

/// [`read_listing`], with the base Kete `recorded` when it cut the
/// checkout read in preference to the derived one
/// (`review_base::resolve_with`).
pub fn read_listing_with(
    root: &Path,
    asked: Option<nebula_core::review_base::Against>,
    recorded: Option<&nebula_core::review_base::RecordedBase>,
) -> Result<crate::view_jobs::DiffListing, String> {
    use nebula_core::review_base::{self as rb, Against};
    let setting = crate::config::Config::load().worktree_base_branch;
    let base = rb::resolve_with(root, &setting, recorded);
    let against = rb::default_against(base.as_ref(), asked);
    let since = base
        .as_ref()
        .filter(|_| against == Against::Base)
        .map(|b| b.sha.clone());
    let files = match &since {
        Some(sha) => files_since(root, sha)?,
        None => changed_files(root)?,
    };
    let commits = base
        .as_ref()
        .map(|b| rb::commits_since(root, &b.sha, rb::MAX_COMMITS))
        .unwrap_or_default();
    let head = head_oid(root);
    let head_key = head.clone().unwrap_or_default();
    let scope = match &since {
        Some(sha) => crate::review::Scope::Base(sha),
        None => crate::review::Scope::Head(&head_key),
    };
    let stored = crate::review::load_scoped(root, scope);
    let reviewed: std::collections::HashMap<String, u64> = files
        .iter()
        .filter_map(|file| {
            let mark = *stored.get(&file.path)?;
            let diff = diff_since(root, file, head.is_some(), since.as_deref());
            (crate::review::fingerprint(&diff) == mark).then(|| (file.path.clone(), mark))
        })
        .collect();
    if reviewed.len() != stored.len() {
        crate::review::store_scoped(root, scope, &reviewed);
    }
    Ok(crate::view_jobs::DiffListing {
        files,
        head,
        reviewed,
        against,
        base,
        commits,
        session: false,
    })
}

/// One session's own changes as the list's files: what differs between
/// its start snapshot `from` and its last `to`
/// (`nebula_core::session_snapshot`). Read with the base and commits the
/// view already has, which the header keeps showing; no reviewed marks,
/// as those are scoped to the base or HEAD.
pub fn read_session_listing(
    root: &Path,
    from: &str,
    to: &str,
    against: nebula_core::review_base::Against,
    base: Option<nebula_core::review_base::ReviewBase>,
    commits: Vec<nebula_core::review_base::CommitLine>,
) -> Result<crate::view_jobs::DiffListing, String> {
    let files = nebula_core::session_snapshot::files_between(root, from, to)?
        .into_iter()
        .map(base_file)
        .collect();
    Ok(crate::view_jobs::DiffListing {
        files,
        head: head_oid(root),
        reviewed: std::collections::HashMap::new(),
        against,
        base,
        commits,
        session: true,
    })
}

/// One file's diff between two trees: a session's snapshots.
pub fn diff_between(root: &Path, file: &DiffFile, from: &str, to: &str) -> String {
    let mut args = vec![
        "diff",
        "--no-color",
        "--no-ext-diff",
        "-M",
        from,
        to,
        "--",
        &file.path,
    ];
    if let Some(orig) = &file.orig_path {
        args.push(orig);
    }
    let output = match run_git(root, &args) {
        Ok(o) => o,
        Err(e) => return e,
    };
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return format!("git diff failed: {}", stderr.trim());
    }
    let text = String::from_utf8_lossy(&output.stdout);
    if text.trim().is_empty() {
        return "(no textual changes)".to_string();
    }
    cap_lines(&text, MAX_DIFF_LINES, false)
}

/// `Ctrl+l`, or a click on the header's session chip, in the REVIEW UNIT:
/// read the list again as the reviewed session's changes alone, or as
/// everything in the worktree. Says what it did, or why it could not.
pub fn toggle_session(view: &mut DiffView) -> String {
    let Some(filter) = view.session.as_deref() else {
        return "no session to narrow to: this diff is not a session's review".into();
    };
    if !filter.offered() {
        return "one session has worked here: all of it is this session's".into();
    }
    let only = !view.session_listed;
    let trees = filter
        .changes
        .trees()
        .map(|(a, b)| (a.to_string(), b.to_string()));
    if only && trees.is_none() {
        return match (&filter.changes.start, &filter.changes.last) {
            (Some(_), Some(_)) => "this session's snapshot was too big to take: showing all".into(),
            _ => "no turn of this session has ended yet".into(),
        };
    }
    if let Some(filter) = view.session.as_deref_mut() {
        filter.only = only;
    }
    view.keep_open = true;
    let root = view.root.clone();
    let (against, base, commits) = (
        view.against,
        view.base.as_deref().cloned(),
        view.commits.clone(),
    );
    let recorded = view.recorded.clone();
    let read = move || match &trees {
        Some((from, to)) if only => read_session_listing(&root, from, to, against, base, commits),
        _ => read_listing_with(&root, Some(against), recorded.as_ref()),
    };
    match view.jobs.clone() {
        Some(jobs) => {
            let ticket = crate::view_jobs::ticket();
            view.listing = Some(ticket);
            jobs.run(move || {
                Some(crate::view_jobs::Answer::DiffListing {
                    ticket,
                    result: read(),
                })
            });
        }
        None => match read() {
            Ok(listing) => fill_view(view, listing),
            Err(e) => return e,
        },
    }
    if only {
        "this session's changes: from its start to its last turn".into()
    } else {
        "all changes in this worktree".into()
    }
}

/// The daemon's answer to `ClientRequest::SessionChanges`: kept on the
/// REVIEW UNIT it was asked for, if that is still the diff up.
pub fn land_session_changes(
    view: &mut DiffView,
    changes: nebula_core::session_snapshot::SessionChanges,
) {
    let asked = view
        .review
        .as_ref()
        .map(|r| &r.agent)
        .or(view.from_card.as_ref());
    if asked != Some(&changes.agent) {
        return;
    }
    let only = view.session.as_ref().is_some_and(|s| s.only);
    view.session = Some(Box::new(crate::app::SessionFilter { changes, only }));
}

/// The working tree's changes since commit `base` as the list's files:
/// each with the one-column status it has against the base (`A `, `M `,
/// `D `, `R ` with where it came from) and an untracked file as `??`, so
/// it is diffed as all added.
pub fn files_since(root: &Path, base: &str) -> Result<Vec<DiffFile>, String> {
    Ok(nebula_core::review_base::files_since(root, base)?
        .into_iter()
        .map(base_file)
        .collect())
}

/// One `review_base::BaseChange` as a list entry.
pub fn base_file(change: nebula_core::review_base::BaseChange) -> DiffFile {
    let xy = match change.status {
        'U' => ['?', '?'],
        status => [status, ' '],
    };
    DiffFile {
        path: change.path,
        orig_path: change.from,
        xy,
    }
}

/// `Ctrl+g`, or a click on the header's toggle: read the list again
/// against the other of the base and HEAD. Nothing without a base, or in
/// a pull request's view. Returns what it asked for, which the caller
/// remembers for the next open.
pub fn toggle_against(view: &mut DiffView) -> Option<nebula_core::review_base::Against> {
    if view.base.is_none() || view.prefetched.is_some() {
        return None;
    }
    let want = view.against.other();
    view.keep_open = true;
    let root = view.root.clone();
    let recorded = view.recorded.clone();
    // Against the base or HEAD is the worktree's whole list again.
    if let Some(filter) = view.session.as_deref_mut() {
        filter.only = false;
    }
    match view.jobs.clone() {
        Some(jobs) => {
            let ticket = crate::view_jobs::ticket();
            view.listing = Some(ticket);
            jobs.run(move || {
                Some(crate::view_jobs::Answer::DiffListing {
                    ticket,
                    result: read_listing_with(&root, Some(want), recorded.as_ref()),
                })
            });
        }
        None => {
            let listing = read_listing_with(&root, Some(want), recorded.as_ref()).ok()?;
            fill_view(view, listing);
        }
    }
    Some(want)
}

/// Put a listing into the view that was opened ahead of it (or built for
/// it): the files, HEAD, the marks — narrowed by whatever the filter holds
/// by now, reviewed files sunk, so the modal lands on the first unreviewed
/// file — and that file's diff.
///
/// A view opened on the badge's list (`event_loop::open_diff_view`) already
/// shows files, and maybe a reader who has moved among them: they stay on
/// the file they are on, wherever the fresh list puts it, and its diff is
/// read again only if it is no longer the same entry. A reader who has not
/// moved gets what a fresh open gives — the first unreviewed file.
pub fn fill_view(view: &mut DiffView, listing: crate::view_jobs::DiffListing) {
    let moved = !view.at_home() || view.scroll != 0;
    let before = view.selected_file().cloned();
    let head_ok = listing.head.is_some();
    let head_changed = !view.files.is_empty() && view.head_ok != head_ok;
    // Read against something else now: every diff in hand is the other's.
    let base_changed = view.against != listing.against
        || view.base.as_ref().map(|b| &b.sha) != listing.base.as_ref().map(|b| &b.sha)
        || view.session_listed != listing.session;
    view.session_listed = listing.session;
    let head_changed = head_changed || base_changed;
    view.against = listing.against;
    view.base = listing.base.map(Box::new);
    view.commits = listing.commits;
    view.keep_open = false;
    view.head_ok = head_ok;
    view.head_key = listing.head.unwrap_or_default();
    view.files = listing.files;
    view.reviewed = listing.reviewed;
    view.listing = None;
    view.recompute_matches();
    // The tree folds the fresh list the same way, keeping what the reader
    // folded; both lists then send the cursor home.
    if let Some(tree) = &view.tree {
        view.tree = Some(tree.rebuilt(&view.files, &view.filter));
    }
    view.selected = 0;
    // Home is the first unreviewed file — the flat list sinks the ✓ ones —
    // and the tree lands on it too.
    let home = view
        .matches
        .first()
        .map(|m| view.files[m.file].path.clone());
    let kept = before
        .as_ref()
        .filter(|_| moved)
        .is_some_and(|was| view.select_path(&was.path));
    if let (false, Some(path)) = (kept, home) {
        view.select_path(&path);
    }
    // Diffs read against the wrong idea of HEAD (the badge's list cannot
    // say whether there is one) are not worth keeping.
    if head_changed {
        view.cache.clear();
    }
    let changed = head_changed || view.selected_file() != before.as_ref() || view.settle();
    if changed {
        if view.log.is_some() {
            crate::git_log::load_selected(view);
        } else {
            load_selected_diff(view);
        }
    }
}

/// Reload `view.diff` for the currently selected file and reset the scroll.
/// A view whose diffs were fetched whole (a pull request) reads them out of
/// `prefetched` instead of shelling out — there is no local commit to ask
/// git about, and the text is already in hand. A directory row of the
/// tree list has no diff of its own: the pane lists what changed under
/// it.
///
/// A view with BACKGROUND READS never waits on git here. A file this modal
/// has read before is on screen on this keypress, out of `DiffView::cache`,
/// and re-read behind it — an agent may have edited it since — with the
/// reader's place kept if the text changed. One it has not keeps the last
/// file's text up for `view_jobs::STALE_GRACE` while git runs, which is
/// longer than a diff takes; past that the pane says `loading…`. Either way
/// the answer comes back through [`land_diff`].
pub fn load_selected_diff(view: &mut DiffView) {
    view.waiting = None;
    let Some(file) = view.selected_file().cloned() else {
        let summary = view.dir_summary().unwrap_or_default();
        view.show_diff(None, summary, false);
        return;
    };
    if let Some(chunks) = &view.prefetched {
        let diff = chunks
            .get(&file.path)
            .cloned()
            .unwrap_or_else(|| "(no diff for this file)".to_string());
        view.show_diff(Some(&file.path), diff, false);
        return;
    }
    let Some(jobs) = view.jobs.clone() else {
        let diff = match view.session_range() {
            Some((from, to)) => diff_between(&view.root, &file, from, to),
            None => diff_since(&view.root, &file, view.head_ok, view.diff_base()),
        };
        view.show_diff(Some(&file.path), diff, false);
        request_aux(view);
        return;
    };
    if let Some(text) = view.cached(&file.path) {
        view.show_diff(Some(&file.path), text.to_string(), false);
    }
    let ticket = crate::view_jobs::ticket();
    view.waiting = Some(ticket);
    request_diff(view, &jobs, file, ticket, false);
}

fn request_diff(
    view: &DiffView,
    jobs: &crate::view_jobs::Jobs,
    file: DiffFile,
    ticket: u64,
    prefetch: bool,
) {
    let (root, head_ok, id) = (view.root.clone(), view.head_ok, view.id);
    let base = view.diff_base().map(str::to_string);
    let range = view
        .session_range()
        .map(|(a, b)| (a.to_string(), b.to_string()));
    let work = move || {
        let diff = match &range {
            Some((from, to)) => diff_between(&root, &file, from, to),
            None => diff_since(&root, &file, head_ok, base.as_deref()),
        };
        Some(crate::view_jobs::Answer::DiffText {
            view: id,
            ticket,
            diff,
            path: file.path,
            prefetch,
        })
    };
    if prefetch {
        jobs.run(work);
    } else {
        jobs.run_with_grace(ticket, work);
    }
}

/// A background `git diff` came back. It is kept either way — it is the
/// freshest text there is for that file — and shown when it is the one the
/// cursor is waiting on. Then the row after the cursor is read ahead, once:
/// `↓` is the key this modal is walked with, and its next press finds the
/// text in hand.
pub fn land_diff(
    view: &mut DiffView,
    id: u64,
    ticket: u64,
    path: &str,
    diff: String,
    prefetch: bool,
) {
    if id != view.id {
        return;
    }
    view.cache_put(path, &diff);
    if prefetch || view.waiting != Some(ticket) {
        return;
    }
    view.waiting = None;
    let same_file = view.shown.as_deref() == Some(path);
    if !(same_file && view.diff == diff) {
        view.show_diff(Some(path), diff, same_file);
    }
    request_aux(view);
    crate::git_log::read_ahead(view);
    let next = view
        .file_after_cursor()
        .filter(|file| view.cached(&file.path).is_none())
        .cloned();
    if let (Some(file), Some(jobs)) = (next, view.jobs.clone()) {
        request_diff(view, &jobs, file, crate::view_jobs::ticket(), true);
    }
}

/// Read what goes beside the working-tree diff on screen — its whole-file
/// text while FULL CONTEXT is on, and its unstaged hunks, which say which
/// CHANGES are staged — off the loop for a view with BACKGROUND READS,
/// inline otherwise. Nothing for a diff that is not a stageable file's
/// (`DiffView::stageable`) or has no hunk to lay out.
pub fn request_aux(view: &mut DiffView) {
    if view.laid.is_none() {
        return;
    }
    let Some(file) = view.stageable().cloned() else {
        return;
    };
    let (root, head_ok, id, full) = (view.root.clone(), view.head_ok, view.id, view.full_context);
    let read = move || crate::app::DiffAux {
        full: full.then(|| full_context_diff(&root, &file, head_ok)),
        unstaged: unstaged_hunks(&root, &file),
        path: file.path,
    };
    match view.jobs.clone() {
        Some(jobs) => jobs.run(move || {
            Some(crate::view_jobs::Answer::DiffAux {
                view: id,
                aux: read(),
            })
        }),
        None => land_aux(view, id, read()),
    }
}

/// What [`request_aux`] read came back: kept while its file is still the
/// one on screen, and the diff laid out again if it brought the whole
/// file's text for FULL CONTEXT.
pub fn land_aux(view: &mut DiffView, id: u64, aux: crate::app::DiffAux) {
    if id != view.id || view.shown.as_deref() != Some(aux.path.as_str()) {
        return;
    }
    let relayout =
        view.full_context && view.aux.as_ref().and_then(|a| a.full.as_ref()) != aux.full.as_ref();
    view.aux = Some(aux);
    if relayout {
        view.relayout();
        view.scroll = view.scroll.min(view.max_scroll());
    }
}

/// The diff in flight has outlasted the grace the last file's text was kept
/// for: say so, rather than leave one file's diff under another's name.
pub fn diff_slow(view: &mut DiffView, ticket: u64) {
    if view.waiting == Some(ticket)
        && view.shown.as_deref() != view.selected_file().map(|f| f.path.as_str())
    {
        view.show_diff(None, "loading…".to_string(), false);
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn modified(path: &str) -> DiffFile {
        DiffFile {
            path: path.into(),
            orig_path: None,
            xy: [' ', 'M'],
        }
    }

    fn listing(paths: &[&str], reviewed: &[&str]) -> crate::view_jobs::DiffListing {
        crate::view_jobs::DiffListing {
            files: paths.iter().map(|p| modified(p)).collect(),
            head: Some("abc123".into()),
            reviewed: reviewed.iter().map(|p| (p.to_string(), 1)).collect(),
            against: nebula_core::review_base::Against::Head,
            base: None,
            commits: Vec::new(),
            session: false,
        }
    }

    /// A view opened on the badge's list, as `g` leaves it: files up, the
    /// `git status` that checks them still out.
    fn opened_on(paths: &[&str]) -> DiffView {
        // Not a repository: a diff read here is git's refusal, which is all
        // these tests need it to be.
        let mut view = DiffView::new(
            std::env::temp_dir().join("nebula-no-such-checkout"),
            "main".into(),
            paths.iter().map(|p| modified(p)).collect(),
            true,
        );
        view.listing = Some(7);
        view
    }

    fn selected(view: &DiffView) -> Option<&str> {
        view.selected_file().map(|f| f.path.as_str())
    }

    /// The fresh list lands under a reader who has moved: they stay on the
    /// file they were reading, wherever it now sits.
    #[test]
    fn a_reader_who_moved_stays_on_their_file_when_the_fresh_list_lands() {
        let mut view = opened_on(&["a.rs", "b.rs", "c.rs"]);
        view.select(1);
        view.show_diff(Some("b.rs"), "the diff of b".into(), false);

        fill_view(&mut view, listing(&["new.rs", "a.rs", "b.rs", "c.rs"], &[]));
        assert_eq!(view.listing, None);
        assert_eq!(selected(&view), Some("b.rs"), "now the third row");
        assert_eq!(view.diff, "the diff of b", "and it was not read again");
    }

    /// …and if that file is no longer changed, they are put on the first.
    #[test]
    fn a_file_that_left_the_list_hands_the_cursor_to_the_top() {
        let mut view = opened_on(&["a.rs", "b.rs", "c.rs"]);
        view.select(1);
        fill_view(&mut view, listing(&["a.rs", "c.rs"], &[]));
        assert_eq!(selected(&view), Some("a.rs"));
    }

    /// A reader who has not moved gets what a fresh open gives: reviewed ✓
    /// files sunk, the cursor on the first that is not.
    #[test]
    fn an_unmoved_reader_lands_on_the_first_unreviewed_file() {
        let mut view = opened_on(&["a.rs", "b.rs"]);
        fill_view(&mut view, listing(&["a.rs", "b.rs"], &["a.rs"]));
        assert_eq!(selected(&view), Some("b.rs"));
        assert_eq!(view.head_key, "abc123");
    }

    /// The tree list lands a fresh listing the same way the flat one does:
    /// folded over the new files, the reader's folds kept, an unmoved
    /// reader on the first unreviewed file and a moved one on theirs.
    #[test]
    fn the_fresh_list_lands_in_the_tree_the_same_way() {
        let mut view = opened_on(&["src/a.rs", "src/b.rs"]);
        view.toggle_tree();
        fill_view(&mut view, listing(&["src/a.rs", "src/b.rs"], &["src/a.rs"]));
        assert!(view.tree.is_some(), "still the tree");
        assert_eq!(selected(&view), Some("src/b.rs"), "the first unreviewed");

        // The reader is on b.rs; a file that turned up since joins the tree
        // under them.
        fill_view(&mut view, listing(&["new.rs", "src/a.rs", "src/b.rs"], &[]));
        assert_eq!(selected(&view), Some("src/b.rs"));
        assert!(view.select_path("new.rs"), "a row of its own now");
    }

    /// The read-ahead walks the list that is showing: in the tree that
    /// means the next file down, directories stepped over.
    #[test]
    fn the_row_read_ahead_is_the_next_file_of_the_list_on_screen() {
        let mut view = opened_on(&["src/a.rs", "src/b.rs"]);
        assert_eq!(
            view.file_after_cursor().map(|f| f.path.as_str()),
            Some("src/b.rs")
        );

        view.toggle_tree();
        // Up onto the `src/` row: the file after it is still a.rs.
        view.select_path("src");
        assert_eq!(
            view.file_after_cursor().map(|f| f.path.as_str()),
            Some("src/a.rs")
        );
        view.select_path("src/b.rs");
        assert_eq!(view.file_after_cursor(), None, "the end of the list");
    }

    /// The cache holds what the budget allows, newest kept, and never an
    /// entry that is most of the budget by itself.
    #[test]
    fn the_diff_cache_stays_inside_its_budget() {
        let mut view = opened_on(&["a.rs"]);
        let chunk = "x".repeat(crate::app::DIFF_CACHE_ENTRY_MAX);
        for n in 0..8 {
            view.cache_put(&format!("f{n}.rs"), &chunk);
        }
        let held: usize = view.cache.iter().map(|(_, text)| text.len()).sum();
        assert!(held <= crate::app::DIFF_CACHE_BYTES, "{held} bytes held");
        assert!(view.cached("f7.rs").is_some(), "the newest is kept");
        assert!(view.cached("f0.rs").is_none(), "the oldest went first");

        view.cache_put("huge.rs", &"y".repeat(crate::app::DIFF_CACHE_ENTRY_MAX + 1));
        assert!(
            view.cached("huge.rs").is_none(),
            "too big to be worth holding"
        );
    }

    #[test]
    fn cap_lines_keeps_the_head_and_marks_what_it_dropped() {
        assert_eq!(cap_lines("a\nb\nc", 5, false), "a\nb\nc");
        assert_eq!(
            cap_lines("a\nb\nc\n", 3, false),
            "a\nb\nc",
            "trailing newline is not a line"
        );
        assert_eq!(
            cap_lines("a\nb\nc", 2, false),
            format!("a\nb{TRUNCATED_MARK}")
        );
        assert_eq!(
            cap_lines("a\nb", 2, true),
            format!("a\nb{TRUNCATED_MARK}"),
            "byte cap forces the mark"
        );
        assert_eq!(cap_lines("a\nb\nc", 0, false), TRUNCATED_MARK);
        assert_eq!(cap_lines("", 3, false), "");
    }

    #[test]
    fn parse_status_z_handles_all_statuses() {
        let bytes =
            b"M  staged.rs\0 M unstaged.rs\0?? new.txt\0D  gone.rs\0R  renamed.rs\0original.rs\0";
        let files = parse_status_z(bytes);
        assert_eq!(files.len(), 5);
        assert_eq!(files[0].path, "staged.rs");
        assert_eq!(files[0].xy, ['M', ' ']);
        assert_eq!(files[1].xy, [' ', 'M']);
        assert!(files[2].is_untracked());
        assert_eq!(files[2].status_str(), "??");
        assert_eq!(files[3].xy, ['D', ' ']);
        assert_eq!(files[4].path, "renamed.rs");
        assert_eq!(files[4].orig_path.as_deref(), Some("original.rs"));
        assert!(files[0].orig_path.is_none());
    }

    #[test]
    fn classify_diff_line_covers_headers_vs_adds() {
        assert_eq!(classify_diff_line("+added"), DiffLineKind::Add);
        assert_eq!(classify_diff_line("+++ b/file"), DiffLineKind::Header);
        assert_eq!(classify_diff_line("-removed"), DiffLineKind::Remove);
        assert_eq!(classify_diff_line("--- a/file"), DiffLineKind::Header);
        assert_eq!(classify_diff_line("@@ -1,3 +1,4 @@"), DiffLineKind::Hunk);
        assert_eq!(
            classify_diff_line("diff --git a/x b/x"),
            DiffLineKind::Header
        );
        assert_eq!(
            classify_diff_line("index 123..456 100644"),
            DiffLineKind::Header
        );
        assert_eq!(
            classify_diff_line("Binary files a/x and b/x differ"),
            DiffLineKind::Header
        );
        assert_eq!(classify_diff_line(" context"), DiffLineKind::Context);
    }
    fn git(repo: &PathBuf, args: &[&str]) {
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
    }

    fn make_repo(dir: &tempfile::TempDir) -> PathBuf {
        let repo = dir.path().join("repo");
        std::fs::create_dir(&repo).unwrap();
        git(&repo, &["init", "-b", "main"]);
        git(&repo, &["config", "user.email", "t@t"]);
        git(&repo, &["config", "user.name", "t"]);
        std::fs::write(repo.join("tracked.txt"), "old line\n").unwrap();
        git(&repo, &["add", "."]);
        git(&repo, &["commit", "-m", "init"]);
        repo
    }

    #[test]
    fn changed_files_and_diff_for_from_real_repo() {
        let _env = crate::test_env::spawning();
        let dir = tempfile::tempdir().unwrap();
        let repo = make_repo(&dir);
        std::fs::write(repo.join("tracked.txt"), "new line\n").unwrap();
        std::fs::write(repo.join("fresh.txt"), "hello\n").unwrap();

        let files = changed_files(&repo).unwrap();
        assert_eq!(files.len(), 2);
        let tracked = files.iter().find(|f| f.path == "tracked.txt").unwrap();
        let fresh = files.iter().find(|f| f.path == "fresh.txt").unwrap();
        assert!(fresh.is_untracked());

        assert!(has_head(&repo));
        let diff = diff_for(&repo, tracked, true, None);
        assert!(diff.contains("-old line"), "{diff}");
        assert!(diff.contains("+new line"), "{diff}");
        // Untracked goes through the --no-index exit-1 path.
        let diff = diff_for(&repo, fresh, true, None);
        assert!(diff.contains("+hello"), "{diff}");
    }

    #[test]
    fn list_files_includes_untracked_and_respects_gitignore() {
        let _env = crate::test_env::spawning();
        let dir = tempfile::tempdir().unwrap();
        let repo = make_repo(&dir);
        std::fs::write(repo.join("fresh.txt"), "hello\n").unwrap();
        std::fs::write(repo.join(".gitignore"), "ignored.txt\n").unwrap();
        std::fs::write(repo.join("ignored.txt"), "nope\n").unwrap();

        let files = list_files(&repo).unwrap();
        assert!(files.contains(&"tracked.txt".to_string()), "{files:?}");
        assert!(files.contains(&"fresh.txt".to_string()), "{files:?}");
        assert!(!files.contains(&"ignored.txt".to_string()), "{files:?}");
    }

    #[test]
    fn list_files_errors_outside_a_repo() {
        let _env = crate::test_env::spawning();
        let dir = tempfile::tempdir().unwrap();
        let err = list_files(dir.path()).unwrap_err();
        assert!(err.contains("git ls-files failed"), "{err}");
    }

    #[test]
    fn head_oid_moves_with_commits() {
        let _env = crate::test_env::spawning();
        let dir = tempfile::tempdir().unwrap();
        let repo = make_repo(&dir);
        let first = head_oid(&repo).expect("committed repo has a HEAD");
        std::fs::write(repo.join("tracked.txt"), "new line\n").unwrap();
        git(&repo, &["commit", "-am", "second"]);
        let second = head_oid(&repo).expect("still has a HEAD");
        assert_ne!(first, second, "a commit moves the OID");
        assert!(head_oid(dir.path()).is_none(), "no repo, no OID");
    }

    #[test]
    fn unborn_head_falls_back_to_plain_diff() {
        let _env = crate::test_env::spawning();
        let dir = tempfile::tempdir().unwrap();
        let repo = dir.path().join("repo");
        std::fs::create_dir(&repo).unwrap();
        git(&repo, &["init", "-b", "main"]);
        std::fs::write(repo.join("only.txt"), "content\n").unwrap();

        assert!(!has_head(&repo));
        let files = changed_files(&repo).unwrap();
        assert_eq!(files.len(), 1);
        assert!(files[0].is_untracked());
        let diff = diff_for(&repo, &files[0], false, None);
        assert!(diff.contains("+content"), "{diff}");
    }

    /// The WORKTREES PANEL badge polls `changed_files` every two seconds on
    /// the checkout the user's agent is working in. A plain `git status`
    /// rewrites the index through `.git/index.lock` whenever its stat cache
    /// is stale, and an agent mid-`git add` or `commit` then fails with
    /// "index.lock: File exists" (issue #15). The poll must read without
    /// ever writing.
    #[cfg(unix)]
    #[test]
    fn changed_files_never_rewrites_the_index() {
        let _env = crate::test_env::spawning();
        use std::os::unix::fs::MetadataExt;
        use std::time::{Duration, UNIX_EPOCH};
        let dir = tempfile::tempdir().unwrap();
        let repo = make_repo(&dir);
        let index = repo.join(".git").join("index");
        // A rewrite lands as a rename over the file, so the inode moves;
        // the mtime is the second witness in case a filesystem reuses it.
        let stamp = || {
            let meta = std::fs::metadata(&index).unwrap();
            (meta.ino(), meta.modified().unwrap())
        };
        // Same content, a different (whole-second) mtime: the index's
        // stat cache no longer matches, which is exactly what makes a
        // status want to write the refreshed cache back.
        let stale = |secs: u64| {
            let file = std::fs::File::options()
                .write(true)
                .open(repo.join("tracked.txt"))
                .unwrap();
            file.set_modified(UNIX_EPOCH + Duration::from_secs(secs))
                .unwrap();
        };

        stale(1_000_000_000);
        let before = stamp();
        changed_files(&repo).unwrap();
        assert_eq!(stamp(), before, "the status poll rewrote the index");

        // The control: with optional locks allowed, the same status does
        // rewrite it — so the assertion above is testing something.
        stale(1_000_000_100);
        let before = stamp();
        let plain = Command::new("git")
            .arg("-C")
            .arg(&repo)
            .env("GIT_OPTIONAL_LOCKS", "1")
            .args(["status", "--porcelain=v1", "-z", "-uall"])
            .output()
            .unwrap();
        assert!(plain.status.success());
        assert_ne!(
            stamp(),
            before,
            "a plain status left the index alone, so this test proves nothing"
        );
    }

    /// Numstat records sum across files; a rename's two path fields are
    /// skipped rather than read as records, and a binary file's `-` adds
    /// nothing.
    #[test]
    fn parse_numstat_z_sums_files_renames_and_binaries() {
        let raw =
            b"3\t1\tsrc/a.rs\x00-\t-\tlogo.png\x0010\t2\t\x00old.rs\x00new.rs\x000\t4\tgone.rs\x00";
        assert_eq!(
            parse_numstat_z(raw),
            LineChanges {
                added: 13,
                removed: 7
            }
        );
        assert!(parse_numstat_z(b"").is_empty());
    }

    #[test]
    fn count_lines_counts_like_numstat() {
        assert_eq!(count_lines(b""), 0);
        assert_eq!(count_lines(b"one\n"), 1);
        assert_eq!(count_lines(b"one\ntwo"), 2);
        assert_eq!(count_lines(b"a\x00b\n"), 0, "binary");
    }

    /// Against a real checkout: a staged edit, an unstaged one and an
    /// untracked file all count, the way the DIFF VIEWER would show them,
    /// and an unborn HEAD counts its tracked files from nothing.
    #[test]
    fn line_changes_counts_staged_unstaged_and_untracked() {
        let _env = crate::test_env::spawning();
        let dir = tempfile::tempdir().unwrap();
        let repo = dir.path();
        let git = |args: &[&str]| {
            let out = run_git(repo, args).unwrap();
            assert!(out.status.success(), "git {args:?}: {out:?}");
        };
        git(&["init", "-q"]);
        git(&["config", "user.email", "t@t"]);
        git(&["config", "user.name", "t"]);
        std::fs::write(repo.join("a.txt"), "1\n2\n3\n").unwrap();
        git(&["add", "a.txt"]);
        let unborn = changed_files(repo).unwrap();
        assert_eq!(
            line_changes(repo, &unborn),
            Some(LineChanges {
                added: 3,
                removed: 0
            }),
            "no commit yet: every tracked line is new"
        );
        git(&["commit", "-qm", "init"]);

        std::fs::write(repo.join("a.txt"), "1\nTWO\n3\n4\n").unwrap();
        git(&["add", "a.txt"]);
        std::fs::write(repo.join("a.txt"), "1\nTWO\n4\n").unwrap();
        std::fs::write(repo.join("new.txt"), "x\ny").unwrap();
        let files = changed_files(repo).unwrap();
        assert_eq!(
            line_changes(repo, &files),
            Some(LineChanges {
                added: 4,
                removed: 2
            }),
            "HEAD 1 2 3 -> 1 TWO 4 is +2 -2, and new.txt's two lines add"
        );
        assert_eq!(line_changes(repo, &[]), Some(LineChanges::default()));
    }

    /// A file with two changes far enough apart to be two hunks, as the
    /// DIFF VIEWER reads it, and the CHANGES it lays out.
    fn two_changes(repo: &PathBuf, crlf: bool) -> (DiffFile, crate::diff_layout::DiffDoc) {
        let eol = if crlf { "\r\n" } else { "\n" };
        let body: Vec<String> = (1..=20).map(|n| format!("line {n}")).collect();
        std::fs::write(repo.join("two.txt"), body.join(eol) + eol).unwrap();
        git(repo, &["add", "two.txt"]);
        git(repo, &["commit", "-qm", "two"]);
        let mut edited = body.clone();
        edited[2] = "line three".into();
        edited.remove(16);
        std::fs::write(repo.join("two.txt"), edited.join(eol) + eol).unwrap();
        let file = changed_files(repo)
            .unwrap()
            .into_iter()
            .find(|f| f.path == "two.txt")
            .unwrap();
        let diff = diff_for(repo, &file, true, None);
        let doc = crate::diff_layout::DiffDoc::parse(&diff, "two.txt", false).unwrap();
        assert_eq!(doc.changes.len(), 2, "{diff}");
        (file, doc)
    }

    fn cached_diff(repo: &Path) -> String {
        let out = run_git(repo, &["diff", "--cached", "--no-color"]).unwrap();
        String::from_utf8_lossy(&out.stdout).into_owned()
    }

    /// ACCEPT stages one CHANGE and nothing else; the other stays
    /// unstaged, and the worktree is not touched. Asked again, the staged
    /// one has nothing left to stage.
    #[test]
    fn accepting_a_change_stages_that_change_alone() {
        let _env = crate::test_env::spawning();
        let dir = tempfile::tempdir().unwrap();
        let repo = make_repo(&dir);
        let (file, doc) = two_changes(&repo, false);
        let before = std::fs::read(repo.join("two.txt")).unwrap();

        assert_eq!(stage_change(&repo, &file, &doc.changes[0]), Ok(true));
        let staged = cached_diff(&repo);
        assert!(staged.contains("+line three"), "{staged}");
        assert!(
            !staged.contains("-line 17"),
            "only the first change: {staged}"
        );
        assert_eq!(std::fs::read(repo.join("two.txt")).unwrap(), before);

        let unstaged = unstaged_hunks(&repo, &file).unwrap();
        assert!(!unstaged
            .iter()
            .any(|h| crate::diff_layout::covers(h, &doc.changes[0])));
        assert!(unstaged
            .iter()
            .any(|h| crate::diff_layout::covers(h, &doc.changes[1])));
        assert_eq!(stage_change(&repo, &file, &doc.changes[0]), Ok(false));
    }

    /// REJECT puts one CHANGE's lines back as HEAD has them, in the
    /// worktree only — CRLF line endings and all.
    #[test]
    fn rejecting_a_change_reverts_that_change_in_the_worktree() {
        let _env = crate::test_env::spawning();
        let dir = tempfile::tempdir().unwrap();
        let repo = make_repo(&dir);
        git(&repo, &["config", "core.autocrlf", "false"]);
        let (file, doc) = two_changes(&repo, true);

        assert_eq!(
            discard_change(&repo, &file, true, &doc.changes[1]),
            Ok(true)
        );
        let now = std::fs::read_to_string(repo.join("two.txt")).unwrap();
        assert!(
            now.contains("line 17\r\n"),
            "the removed line is back: {now:?}"
        );
        assert!(now.contains("line three\r\n"), "the other change stays");
        assert!(cached_diff(&repo).is_empty(), "the index is not touched");
        let diff = diff_for(&repo, &file, true, None);
        let left = crate::diff_layout::DiffDoc::parse(&diff, "two.txt", false).unwrap();
        assert_eq!(left.changes.len(), 1);
    }

    /// FULL CONTEXT reads the whole file as one hunk, leaving `diff_for`'s
    /// three-line text — what the ✓ marks fingerprint — as it was.
    #[test]
    fn full_context_reads_the_whole_file() {
        let _env = crate::test_env::spawning();
        let dir = tempfile::tempdir().unwrap();
        let repo = make_repo(&dir);
        let (file, _) = two_changes(&repo, false);
        let full = full_context_diff(&repo, &file, true);
        let doc = crate::diff_layout::DiffDoc::parse(&full, "two.txt", true).unwrap();
        assert_eq!(doc.hunks.len(), 1);
        assert!(full.contains(" line 10"), "the middle is there");
        assert!(!diff_for(&repo, &file, true, None).contains(" line 10"));
    }

    #[test]
    fn changed_files_errors_outside_a_repo() {
        let _env = crate::test_env::spawning();
        let dir = tempfile::tempdir().unwrap();
        let err = changed_files(dir.path()).unwrap_err();
        assert!(err.contains("git status failed"), "{err}");
    }

    #[test]
    fn base_changes_become_one_column_entries_and_untracked_stays_untracked() {
        use nebula_core::review_base::BaseChange;
        let change = |status, path: &str, from: Option<&str>| BaseChange {
            status,
            path: path.into(),
            from: from.map(str::to_string),
        };
        let renamed = base_file(change('R', "new.rs", Some("old.rs")));
        assert_eq!(renamed.xy, ['R', ' ']);
        assert_eq!(renamed.orig_path.as_deref(), Some("old.rs"));
        assert_eq!(base_file(change('A', "a", None)).status_str(), "A ");
        assert!(base_file(change('U', "n", None)).is_untracked());
    }

    /// A repo on `feat`, forked from `main` at `fork`: one commit made on
    /// the branch (`b.txt`), one file edited and not committed (`tracked.txt`).
    fn branch_with_a_commit(dir: &tempfile::TempDir) -> (PathBuf, String) {
        let repo = make_repo(dir);
        git(&repo, &["config", "commit.gpgsign", "false"]);
        let fork = String::from_utf8(
            Command::new("git")
                .arg("-C")
                .arg(&repo)
                .args(["rev-parse", "HEAD"])
                .output()
                .unwrap()
                .stdout,
        )
        .unwrap()
        .trim()
        .to_string();
        git(&repo, &["checkout", "-q", "-b", "feat"]);
        std::fs::write(repo.join("b.txt"), "agent work\n").unwrap();
        git(&repo, &["add", "."]);
        git(&repo, &["commit", "-q", "-m", "agent commit"]);
        std::fs::write(repo.join("tracked.txt"), "new line\n").unwrap();
        (repo, fork)
    }

    #[test]
    fn a_branch_opens_against_its_base_and_keeps_its_commits_in_view() {
        use nebula_core::review_base::Against;
        let _env = crate::test_env::spawning();
        let dir = tempfile::tempdir().unwrap();
        let (repo, fork) = branch_with_a_commit(&dir);
        crate::review::with_store_path(dir.path().join("reviewed.json"), || {
            let listing = read_listing(&repo, None).unwrap();
            assert_eq!(listing.against, Against::Base);
            assert_eq!(
                listing.base.as_ref().map(|b| b.sha.as_str()),
                Some(fork.as_str())
            );
            let paths: Vec<&str> = listing.files.iter().map(|f| f.path.as_str()).collect();
            assert_eq!(paths, vec!["b.txt", "tracked.txt"]);
            let subjects: Vec<&str> = listing.commits.iter().map(|c| c.subject.as_str()).collect();
            assert_eq!(subjects, vec!["agent commit"]);

            let mut view = DiffView::new(repo.clone(), "feat".into(), Vec::new(), true);
            fill_view(&mut view, listing);
            assert_eq!(
                view.against_label(),
                format!("vs main (merge-base {})", &fork[..7])
            );
            // The committed file's diff reads against the base.
            assert!(view.select_path("b.txt"));
            load_selected_diff(&mut view);
            assert!(view.diff.contains("+agent work"), "{}", view.diff);
            // Staging works against HEAD: nothing to stage from here.
            assert!(view.stageable().is_none());

            // Toggled to HEAD: the commit leaves the list, staging is back.
            assert_eq!(toggle_against(&mut view), Some(Against::Head));
            assert_eq!(view.against, Against::Head);
            assert_eq!(view.against_label(), "vs HEAD");
            let paths: Vec<&str> = view.files.iter().map(|f| f.path.as_str()).collect();
            assert_eq!(paths, vec!["tracked.txt"]);
            assert_eq!(view.commits.len(), 1, "the commits stay listed");
            load_selected_diff(&mut view);
            assert!(view.stageable().is_some());
        });
    }

    #[test]
    fn reviewed_marks_against_the_base_survive_a_commit() {
        let _env = crate::test_env::spawning();
        let dir = tempfile::tempdir().unwrap();
        let (repo, _) = branch_with_a_commit(&dir);
        crate::review::with_store_path(dir.path().join("reviewed.json"), || {
            let mut view = DiffView::new(repo.clone(), "feat".into(), Vec::new(), true);
            fill_view(&mut view, read_listing(&repo, None).unwrap());
            assert!(view.select_path("tracked.txt"));
            load_selected_diff(&mut view);
            assert!(view.toggle_reviewed().is_some());
            view.store_marks();
            assert!(view.reviewed.contains_key("tracked.txt"));
            // The agent commits the file as it was approved: HEAD moves, the
            // merge-base does not, and the diff against it is the same.
            git(&repo, &["commit", "-q", "-am", "commit the edit"]);
            let listing = read_listing(&repo, None).unwrap();
            assert!(listing.reviewed.contains_key("tracked.txt"));
            // Against HEAD there is nothing left, marked or not.
            let head = read_listing(&repo, Some(nebula_core::review_base::Against::Head)).unwrap();
            assert!(head.files.is_empty() && head.reviewed.is_empty());
        });
    }

    #[test]
    fn the_base_branch_itself_reads_against_head() {
        let _env = crate::test_env::spawning();
        let dir = tempfile::tempdir().unwrap();
        let repo = make_repo(&dir);
        std::fs::write(repo.join("tracked.txt"), "new line\n").unwrap();
        crate::review::with_store_path(dir.path().join("reviewed.json"), || {
            let listing = read_listing(&repo, None).unwrap();
            assert_eq!(listing.against, nebula_core::review_base::Against::Head);
            assert!(listing.base.is_none() && listing.commits.is_empty());
            let mut view = DiffView::new(repo.clone(), "main".into(), Vec::new(), true);
            fill_view(&mut view, listing);
            assert_eq!(toggle_against(&mut view), None, "nothing to switch to");
        });
    }

    fn session_changes(
        agent: &str,
        start: Option<String>,
        last: Option<String>,
        sessions: u32,
    ) -> nebula_core::session_snapshot::SessionChanges {
        use nebula_core::session_snapshot::SnapshotPoint;
        let point = |tree: Option<String>| SnapshotPoint {
            head: None,
            tree,
            at: 1,
        };
        nebula_core::session_snapshot::SessionChanges {
            agent: nebula_core::AgentId(agent.into()),
            worktree: None,
            start: Some(point(start)),
            last: last.map(|t| point(Some(t))),
            sessions,
        }
    }

    fn reviewing(view: &mut DiffView, agent: &str) {
        view.review = Some(Box::new(crate::app::ReviewDock {
            agent: nebula_core::AgentId(agent.into()),
            worktree: nebula_core::WorktreeId("w".into()),
            verbs: Vec::new(),
            dock: ratatui::layout::Rect::default(),
        }));
    }

    /// The REVIEW UNIT narrows to one session's changes between its
    /// snapshots — another session's edit and the branch's commit drop out,
    /// staging and marks go quiet — and widens back to the worktree.
    #[test]
    fn a_review_narrows_to_its_sessions_changes_and_back() {
        use nebula_core::session_snapshot::take;
        let _env = crate::test_env::spawning();
        let dir = tempfile::tempdir().unwrap();
        let (repo, _) = branch_with_a_commit(&dir);
        crate::review::with_store_path(dir.path().join("reviewed.json"), || {
            let start = take(&repo).tree.unwrap();
            std::fs::write(repo.join("mine.txt"), "this session\n").unwrap();
            let last = take(&repo).tree.unwrap();
            std::fs::write(repo.join("theirs.txt"), "another session\n").unwrap();

            let mut view = DiffView::new(repo.clone(), "feat".into(), Vec::new(), true);
            fill_view(&mut view, read_listing(&repo, None).unwrap());
            reviewing(&mut view, "a1");
            let all: Vec<String> = view.files.iter().map(|f| f.path.clone()).collect();
            assert_eq!(all, vec!["b.txt", "mine.txt", "theirs.txt", "tracked.txt"]);

            // Another session's answer is not this review's.
            land_session_changes(
                &mut view,
                session_changes("other", Some(start.clone()), Some(last.clone()), 2),
            );
            assert!(view.session.is_none());
            land_session_changes(
                &mut view,
                session_changes("a1", Some(start.clone()), Some(last.clone()), 2),
            );
            assert!(view.session.as_ref().unwrap().offered());

            let said = toggle_session(&mut view);
            assert!(said.contains("this session"), "{said}");
            assert!(view.session_listed);
            let paths: Vec<&str> = view.files.iter().map(|f| f.path.as_str()).collect();
            assert_eq!(paths, vec!["mine.txt"]);
            assert_eq!(view.session_range(), Some((start.as_str(), last.as_str())));
            load_selected_diff(&mut view);
            assert!(view.diff.contains("+this session"), "{}", view.diff);
            assert!(
                view.stageable().is_none(),
                "a session's list stages nothing"
            );

            let said = toggle_session(&mut view);
            assert!(said.contains("all changes"), "{said}");
            assert!(!view.session_listed);
            let back: Vec<String> = view.files.iter().map(|f| f.path.clone()).collect();
            assert_eq!(back, all);
        });
    }

    #[test]
    fn the_session_filter_says_why_it_cannot_narrow() {
        let mut view = DiffView::new("/no-such".into(), "feat".into(), Vec::new(), true);
        assert!(toggle_session(&mut view).contains("not a session's review"));
        reviewing(&mut view, "a1");
        land_session_changes(&mut view, session_changes("a1", Some("t".into()), None, 1));
        assert!(!view.session.as_ref().unwrap().offered());
        assert!(toggle_session(&mut view).contains("one session"));
        land_session_changes(&mut view, session_changes("a1", Some("t".into()), None, 2));
        assert!(toggle_session(&mut view).contains("no turn"));
        land_session_changes(&mut view, session_changes("a1", None, Some("t".into()), 2));
        assert!(toggle_session(&mut view).contains("too big"));
        assert!(!view.session_listed);
    }
}
