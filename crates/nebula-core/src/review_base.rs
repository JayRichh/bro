//! What a worktree's diff is read against. An agent that commits its work
//! takes it out of `git diff HEAD`, so the review reads from where the
//! branch forked instead: the MERGE-BASE of HEAD and the base branch —
//! the `worktree_base_branch` setting, else the repo's default branch
//! (`origin/HEAD`, then `main`/`master`). Shared by the terminal app's
//! diff viewer and the desktop window's Diff view, so both name the same
//! base and list the same files and commits.
//!
//! The picking is pure ([`pick_base`]); [`resolve`] feeds it from git.
//! Nothing here fetches: the refs are what the clone last saw. Reading
//! only, and with git's optional locks off (the index refresh `git diff`
//! does would otherwise race an agent's own `git add`).

use std::path::Path;
use std::process::{Command, Output, Stdio};

/// The two things a worktree's diff can be read against.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Against {
    /// The merge-base with the base branch: everything the branch changed,
    /// committed or not.
    Base,
    /// HEAD: the uncommitted changes alone, what staging works on.
    #[default]
    Head,
}

impl Against {
    pub fn other(self) -> Self {
        match self {
            Against::Base => Against::Head,
            Against::Head => Against::Base,
        }
    }

    /// The wire and settings word: `base` or `head`.
    pub fn as_str(self) -> &'static str {
        match self {
            Against::Base => "base",
            Against::Head => "head",
        }
    }

    pub fn parse(word: &str) -> Option<Self> {
        match word.trim() {
            "base" => Some(Against::Base),
            "head" => Some(Against::Head),
            _ => None,
        }
    }
}

/// Where the worktree's branch forked from its base branch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReviewBase {
    /// The merge-base commit, full SHA.
    pub sha: String,
    /// The base branch by name, `origin/` dropped: `main`, `develop`.
    pub branch: String,
    /// The ref it was read from: `origin/main`, or a local `main`.
    pub refname: String,
}

impl ReviewBase {
    pub fn short(&self) -> &str {
        &self.sha[..self.sha.len().min(7)]
    }

    /// `vs main (merge-base abc1234)`: what both apps' headers say.
    pub fn label(&self) -> String {
        format!("vs {} (merge-base {})", self.branch, self.short())
    }
}

/// What the header says for `against`, given the base there is.
pub fn against_label(against: Against, base: Option<&ReviewBase>) -> String {
    match (against, base) {
        (Against::Base, Some(base)) => base.label(),
        _ => "vs HEAD".to_string(),
    }
}

/// What a diff opens on: the base when there is one (the resolver already
/// said None for the base branch's own checkout), HEAD otherwise. `asked`
/// is what the reader picked last, honoured where it can be.
pub fn default_against(base: Option<&ReviewBase>, asked: Option<Against>) -> Against {
    match (base, asked) {
        (None, _) => Against::Head,
        (Some(_), Some(asked)) => asked,
        (Some(_), None) => Against::Base,
    }
}

/// The `worktree_base_branch` setting as a branch name, or None for the
/// default: trimmed, a leading `origin/` dropped, as the daemon reads it
/// (`nebula-daemon` `Config::worktree_base_branch`).
pub fn normalize_setting(raw: &str) -> Option<&str> {
    let name = raw.trim();
    let name = name.strip_prefix("origin/").unwrap_or(name).trim();
    (!name.is_empty()).then_some(name)
}

/// The base ref to read from, and the branch name the header gives it,
/// out of `refs` — the full refnames the clone has among the candidates
/// ([`candidate_refs`]) — and `origin_head`, what `refs/remotes/origin/HEAD`
/// points at (`refs/remotes/origin/main`). In order: the configured branch
/// (origin's copy first, as the daemon cuts worktrees from it), origin's
/// default branch, then `main` and `master`, origin's before local. A
/// configured branch the repo lacks falls through to the default, as the
/// daemon's `add_worktree_off_configured` does. None when the checkout
/// is ON that branch (`current`), or nothing matched.
pub fn pick_base(
    current: &str,
    configured: Option<&str>,
    refs: &[String],
    origin_head: Option<&str>,
) -> Option<(String, String)> {
    let has = |full: &str| refs.iter().any(|r| r == full);
    let both = |name: &str| {
        [
            (format!("refs/remotes/origin/{name}"), name.to_string()),
            (format!("refs/heads/{name}"), name.to_string()),
        ]
    };
    let mut order: Vec<(String, String)> = Vec::new();
    if let Some(name) = configured {
        order.extend(both(name));
    }
    if let Some(name) = origin_head.and_then(|h| h.strip_prefix("refs/remotes/origin/")) {
        if name != "HEAD" {
            order.push((format!("refs/remotes/origin/{name}"), name.to_string()));
        }
    }
    order.extend(both("main"));
    order.extend(both("master"));
    let (full, branch) = order.into_iter().find(|(full, _)| has(full))?;
    if branch == current {
        return None;
    }
    let short = full
        .strip_prefix("refs/remotes/")
        .or_else(|| full.strip_prefix("refs/heads/"))
        .unwrap_or(&full)
        .to_string();
    Some((short, branch))
}

/// The refnames [`pick_base`] may choose from, for one `for-each-ref`.
pub fn candidate_refs(configured: Option<&str>) -> Vec<String> {
    let mut names: Vec<&str> = configured.into_iter().collect();
    names.extend(["main", "master"]);
    let mut out = vec!["refs/remotes/origin/HEAD".to_string()];
    for name in names {
        out.push(format!("refs/remotes/origin/{name}"));
        out.push(format!("refs/heads/{name}"));
    }
    out
}

/// `git -C root` with no optional locks, no prompt and no console window.
fn git(root: &Path, args: &[&str]) -> Option<Output> {
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
    cmd.output().ok()
}

fn git_ok(root: &Path, args: &[&str]) -> Option<Vec<u8>> {
    let out = git(root, args)?;
    out.status.success().then_some(out.stdout)
}

fn git_line(root: &Path, args: &[&str]) -> Option<String> {
    let out = git_ok(root, args)?;
    let line = String::from_utf8_lossy(&out).trim().to_string();
    (!line.is_empty()).then_some(line)
}

/// The base of the checkout at `root`: the merge-base of HEAD with the
/// base branch [`pick_base`] chooses. `configured` is the raw
/// `worktree_base_branch` setting. None on a detached HEAD, on the base
/// branch itself, with no base branch in the clone, or with no shared
/// history.
pub fn resolve(root: &Path, configured: &str) -> Option<ReviewBase> {
    let configured = normalize_setting(configured);
    let current = git_line(root, &["symbolic-ref", "-q", "--short", "HEAD"])?;
    let mut args = vec![
        "for-each-ref".to_string(),
        "--format=%(refname)%00%(symref)".to_string(),
    ];
    args.extend(candidate_refs(configured));
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    let listed = git_ok(root, &args)?;
    let mut refs = Vec::new();
    let mut origin_head = None;
    for line in String::from_utf8_lossy(&listed).lines() {
        let (name, target) = line.split_once('\0').unwrap_or((line, ""));
        if name == "refs/remotes/origin/HEAD" {
            // Listed means it resolves (git skips a dangling one), so the
            // branch it names is there too, though no pattern asked for it.
            if !target.is_empty() {
                origin_head = Some(target.to_string());
                refs.push(target.to_string());
            }
        } else {
            refs.push(name.to_string());
        }
    }
    let (refname, branch) = pick_base(&current, configured, &refs, origin_head.as_deref())?;
    let sha = git_line(root, &["merge-base", "HEAD", &refname])?;
    Some(ReviewBase {
        sha,
        branch,
        refname,
    })
}

/// One commit made since the base.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommitLine {
    pub sha: String,
    pub short: String,
    pub subject: String,
}

/// The most commits [`commits_since`] lists.
pub const MAX_COMMITS: usize = 200;

/// The commits on HEAD since `base`, newest first, at most `cap`.
pub fn commits_since(root: &Path, base: &str, cap: usize) -> Vec<CommitLine> {
    let range = format!("{base}..HEAD");
    let max = format!("--max-count={cap}");
    let Some(out) = git_ok(
        root,
        &["log", "--no-color", &max, "--format=%H%x1f%h%x1f%s", &range],
    ) else {
        return Vec::new();
    };
    parse_log(&String::from_utf8_lossy(&out))
}

/// `sha \x1f short \x1f subject` per line.
pub fn parse_log(text: &str) -> Vec<CommitLine> {
    text.lines()
        .filter_map(|line| {
            let mut cols = line.splitn(3, '\u{1f}');
            let (sha, short, subject) = (cols.next()?, cols.next()?, cols.next().unwrap_or(""));
            (!sha.is_empty()).then(|| CommitLine {
                sha: sha.to_string(),
                short: short.to_string(),
                subject: subject.to_string(),
            })
        })
        .collect()
}

/// One path changed since the base, working tree included.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BaseChange {
    /// `A`dded, `M`odified, `D`eleted, `R`enamed, or `U`ntracked (new and
    /// not added: an add too, that `git add` has not seen).
    pub status: char,
    pub path: String,
    /// Where a rename came from.
    pub from: Option<String>,
}

/// Every path the working tree differs in from commit `base`: what was
/// committed since, staged and not (`git diff <base> --name-status`), and
/// each untracked file, as added. Sorted by path.
pub fn files_since(root: &Path, base: &str) -> Result<Vec<BaseChange>, String> {
    let out = git(
        root,
        &[
            "diff",
            "--name-status",
            "-z",
            "-M",
            "--no-ext-diff",
            base,
            "--",
        ],
    )
    .ok_or_else(|| "could not run git".to_string())?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr);
        return Err(format!("git diff failed: {}", err.trim()));
    }
    let mut files = parse_name_status(&out.stdout);
    if let Some(raw) = git_ok(root, &["ls-files", "--others", "--exclude-standard", "-z"]) {
        for path in raw.split(|b| *b == 0).filter(|p| !p.is_empty()) {
            files.push(BaseChange {
                status: 'U',
                path: String::from_utf8_lossy(path).into_owned(),
                from: None,
            });
        }
    }
    files.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(files)
}

/// `git diff --name-status -z`: `X\0path\0`, or `R100\0from\0to\0`. A copy
/// reads as an add; a type change as a modification.
pub fn parse_name_status(raw: &[u8]) -> Vec<BaseChange> {
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
            b'A' | b'C' => 'A',
            b'D' => 'D',
            b'R' => 'R',
            _ => 'M',
        };
        let from = if letter == b'C' { None } else { from };
        files.push(BaseChange { status, path, from });
    }
    files
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

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

    fn commit(repo: &Path, file: &str, text: &str, msg: &str) -> String {
        std::fs::write(repo.join(file), text).unwrap();
        run(repo, &["add", "."]);
        run(repo, &["commit", "-q", "-m", msg]);
        run(repo, &["rev-parse", "HEAD"])
    }

    /// A repo on `main` with one commit, its sha.
    fn repo(dir: &tempfile::TempDir) -> (PathBuf, String) {
        let repo = dir.path().join("repo");
        std::fs::create_dir(&repo).unwrap();
        run(&repo, &["init", "-q", "-b", "main"]);
        run(&repo, &["config", "user.email", "t@t"]);
        run(&repo, &["config", "user.name", "t"]);
        run(&repo, &["config", "commit.gpgsign", "false"]);
        let first = commit(&repo, "a.txt", "one\n", "init");
        (repo, first)
    }

    fn refs(names: &[&str]) -> Vec<String> {
        names.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn the_setting_reads_as_the_daemon_reads_it() {
        assert_eq!(normalize_setting(""), None);
        assert_eq!(normalize_setting("  "), None);
        assert_eq!(normalize_setting(" develop "), Some("develop"));
        assert_eq!(normalize_setting("origin/master"), Some("master"));
        assert_eq!(normalize_setting("origin/"), None);
    }

    #[test]
    fn the_configured_branch_wins_origins_copy_first() {
        let have = refs(&[
            "refs/heads/develop",
            "refs/remotes/origin/develop",
            "refs/remotes/origin/main",
        ]);
        let got = pick_base(
            "feat",
            Some("develop"),
            &have,
            Some("refs/remotes/origin/main"),
        );
        assert_eq!(got, Some(("origin/develop".into(), "develop".into())));
        // Only a local copy: used as named.
        let have = refs(&["refs/heads/develop", "refs/remotes/origin/main"]);
        let got = pick_base("feat", Some("develop"), &have, None);
        assert_eq!(got, Some(("develop".into(), "develop".into())));
    }

    #[test]
    fn a_configured_branch_the_repo_lacks_falls_back_to_the_default() {
        let have = refs(&["refs/remotes/origin/trunk", "refs/heads/main"]);
        let got = pick_base(
            "feat",
            Some("develop"),
            &have,
            Some("refs/remotes/origin/trunk"),
        );
        assert_eq!(got, Some(("origin/trunk".into(), "trunk".into())));
        // No origin/HEAD: main, then master.
        let got = pick_base("feat", None, &refs(&["refs/heads/master"]), None);
        assert_eq!(got, Some(("master".into(), "master".into())));
    }

    #[test]
    fn the_base_branch_itself_and_a_repo_without_one_have_no_base() {
        let have = refs(&["refs/remotes/origin/main", "refs/heads/main"]);
        assert_eq!(pick_base("main", None, &have, None), None);
        assert_eq!(pick_base("feat", None, &[], None), None);
    }

    #[test]
    fn opening_defaults_to_the_base_when_there_is_one() {
        let base = ReviewBase {
            sha: "0123456789abcdef".into(),
            branch: "main".into(),
            refname: "main".into(),
        };
        assert_eq!(default_against(Some(&base), None), Against::Base);
        assert_eq!(
            default_against(Some(&base), Some(Against::Head)),
            Against::Head
        );
        assert_eq!(default_against(None, Some(Against::Base)), Against::Head);
        assert_eq!(
            against_label(Against::Base, Some(&base)),
            "vs main (merge-base 0123456)"
        );
        assert_eq!(against_label(Against::Head, Some(&base)), "vs HEAD");
        assert_eq!(against_label(Against::Base, None), "vs HEAD");
    }

    #[test]
    fn name_status_reads_renames_copies_and_deletes() {
        let raw = b"M\0a.txt\0R100\0old.rs\0new.rs\0D\0gone\0C75\0src\0copy\0A\0added\0";
        let got = parse_name_status(raw);
        let short: Vec<(char, &str, Option<&str>)> = got
            .iter()
            .map(|c| (c.status, c.path.as_str(), c.from.as_deref()))
            .collect();
        assert_eq!(
            short,
            vec![
                ('M', "a.txt", None),
                ('R', "new.rs", Some("old.rs")),
                ('D', "gone", None),
                ('A', "copy", None),
                ('A', "added", None),
            ]
        );
    }

    #[test]
    fn a_branch_sees_its_commits_and_its_dirty_tree_since_the_fork() {
        let dir = tempfile::tempdir().unwrap();
        let (repo, fork) = repo(&dir);
        // origin/main as a fetched ref, with origin/HEAD at it.
        run(&repo, &["update-ref", "refs/remotes/origin/main", &fork]);
        run(
            &repo,
            &[
                "symbolic-ref",
                "refs/remotes/origin/HEAD",
                "refs/remotes/origin/main",
            ],
        );
        // main itself on the base branch: nothing to compare with.
        assert_eq!(resolve(&repo, ""), None);

        run(&repo, &["checkout", "-q", "-b", "feat"]);
        commit(&repo, "b.txt", "two\n", "add b");
        commit(&repo, "a.txt", "one\nmore\n", "grow a");
        // main moves on after the fork: not this branch's change.
        run(&repo, &["checkout", "-q", "main"]);
        let later = commit(&repo, "c.txt", "main only\n", "main moves");
        run(&repo, &["update-ref", "refs/remotes/origin/main", &later]);
        run(&repo, &["checkout", "-q", "feat"]);
        std::fs::write(repo.join("b.txt"), "two\nedited\n").unwrap();
        std::fs::write(repo.join("new.txt"), "fresh\n").unwrap();

        let base = resolve(&repo, "").expect("a base");
        assert_eq!(base.sha, fork);
        assert_eq!(base.branch, "main");
        assert_eq!(base.refname, "origin/main");

        let files = files_since(&repo, &base.sha).unwrap();
        let got: Vec<(char, &str)> = files.iter().map(|f| (f.status, f.path.as_str())).collect();
        assert_eq!(got, vec![('M', "a.txt"), ('A', "b.txt"), ('U', "new.txt")]);

        let commits = commits_since(&repo, &base.sha, MAX_COMMITS);
        let subjects: Vec<&str> = commits.iter().map(|c| c.subject.as_str()).collect();
        assert_eq!(subjects, vec!["grow a", "add b"]);
        assert!(commits[0].short.len() >= 7);
    }

    #[test]
    fn origins_default_branch_wins_over_a_local_main() {
        let dir = tempfile::tempdir().unwrap();
        let (repo, first) = repo(&dir);
        run(&repo, &["checkout", "-q", "-b", "feat"]);
        let trunk = commit(&repo, "t.txt", "trunk\n", "on trunk");
        commit(&repo, "f.txt", "feat\n", "feat");
        // origin's default is `trunk`, not the local `main` still at `first`.
        run(&repo, &["update-ref", "refs/remotes/origin/trunk", &trunk]);
        run(
            &repo,
            &[
                "symbolic-ref",
                "refs/remotes/origin/HEAD",
                "refs/remotes/origin/trunk",
            ],
        );
        let base = resolve(&repo, "").expect("a base");
        assert_eq!(base.branch, "trunk");
        assert_eq!(base.refname, "origin/trunk");
        assert_eq!(base.sha, trunk);
        assert_ne!(base.sha, first);
    }

    #[test]
    fn the_setting_picks_a_local_branch_and_detached_has_no_base() {
        let dir = tempfile::tempdir().unwrap();
        let (repo, _) = repo(&dir);
        run(&repo, &["checkout", "-q", "-b", "develop"]);
        let fork = commit(&repo, "d.txt", "dev\n", "dev");
        run(&repo, &["checkout", "-q", "-b", "feat"]);
        commit(&repo, "e.txt", "feat\n", "feat");
        // No remote at all: `origin/develop` reads as the local develop.
        let base = resolve(&repo, "origin/develop").expect("a base");
        assert_eq!(
            (base.branch.as_str(), base.sha.as_str()),
            ("develop", fork.as_str())
        );
        // Unset: the local main.
        assert_eq!(resolve(&repo, "").expect("a base").branch, "main");
        run(&repo, &["checkout", "-q", "--detach"]);
        assert_eq!(resolve(&repo, ""), None);
    }
}
