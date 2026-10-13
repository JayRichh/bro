//! Commits Kete publishes carry no co-author lines (human or AI) and no
//! "Generated with" footers. This finds the unpushed commits whose messages
//! still have one, and rewrites those messages in place: trees, authors,
//! committers and dates stay exactly as they were.

use anyhow::{anyhow, bail, Context, Result};
use std::collections::{HashMap, HashSet};
use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};

/// Whether `line` is one the rule removes.
fn is_attribution_line(line: &str) -> bool {
    let line = line.strip_suffix('\r').unwrap_or(line);
    let head = line.get(..15).unwrap_or("");
    head.eq_ignore_ascii_case("co-authored-by:")
        || line.starts_with("🤖 Generated with")
        || line.starts_with("Generated with [")
}

/// `message` without its co-author lines and "Generated with" footers, and
/// whether any went. A message with none comes back exactly as given; one
/// that lost lines loses the blank lines they leave at its end too, and
/// ends in one newline.
pub fn strip_ai_trailers(message: &str) -> (String, bool) {
    if !message.split('\n').any(is_attribution_line) {
        return (message.to_string(), false);
    }
    let kept: Vec<&str> = message
        .split('\n')
        .filter(|line| !is_attribution_line(line))
        .collect();
    let mut out = kept.join("\n").trim_end().to_string();
    out.push('\n');
    (out, true)
}

/// The SHAs in `range` (anything `git rev-list` takes, such as
/// `origin/main..HEAD`), oldest first, whose messages
/// [`strip_ai_trailers`] would change.
pub fn find_unpushed_ai_commits(repo: &Path, range: &str) -> Result<Vec<String>> {
    let list = git(repo, &["rev-list", "--reverse", range])?;
    let mut found = Vec::new();
    for sha in list.split_whitespace() {
        let commit = RawCommit::read(repo, sha)?;
        if strip_ai_trailers(&commit.message).1 {
            found.push(sha.to_string());
        }
    }
    Ok(found)
}

/// Strip the co-author lines and footers from every commit on `branch` that
/// is on no remote yet (not in `base`, the branch's upstream, or any
/// remote-tracking ref), and move the branch to the rewritten tip. Returns
/// how many messages changed; 0 leaves everything as it was.
///
/// Refuses, before writing anything, when a checkout of `branch` has
/// uncommitted changes, or when a commit it would rewrite is signed (a new
/// message means a new commit, and the signature cannot come along).
pub fn rewrite_unpushed(repo: &Path, base: &str, branch: &str) -> Result<usize> {
    let branch_ref = format!("refs/heads/{branch}");
    let old_tip = git(
        repo,
        &["rev-parse", "--verify", &format!("{branch_ref}^{{commit}}")],
    )?
    .trim()
    .to_string();

    let mut args = vec![
        "rev-list".to_string(),
        "--topo-order".into(),
        "--reverse".into(),
        old_tip.clone(),
        format!("^{base}"),
    ];
    if let Ok(upstream) = git(
        repo,
        &[
            "rev-parse",
            "--verify",
            "--quiet",
            &format!("{branch_ref}@{{upstream}}"),
        ],
    ) {
        args.push(format!("^{}", upstream.trim()));
    }
    args.extend(["--not".into(), "--remotes".into()]);
    let arg_refs: Vec<&str> = args.iter().map(String::as_str).collect();
    let list = git(repo, &arg_refs)?;

    // Plan first: which commits change message, and which must be written
    // again because an ancestor did.
    let mut plan = Vec::new();
    let mut rewritten: HashSet<String> = HashSet::new();
    let mut changed = 0;
    for sha in list.split_whitespace() {
        let commit = RawCommit::read(repo, sha)?;
        let (message, stripped) = strip_ai_trailers(&commit.message);
        let ancestor_moved = commit.parents.iter().any(|p| rewritten.contains(p));
        if !stripped && !ancestor_moved {
            continue;
        }
        if commit.signed {
            bail!(
                "commit {} is signed; rewriting its message would drop the signature, so it was left alone",
                short(sha)
            );
        }
        if stripped {
            changed += 1;
        }
        rewritten.insert(sha.to_string());
        plan.push((sha.to_string(), commit, message));
    }
    if changed == 0 {
        return Ok(0);
    }
    ensure_clean_checkouts(repo, &branch_ref)?;

    let mut new_sha: HashMap<String, String> = HashMap::new();
    let mut new_tip = old_tip.clone();
    for (sha, commit, message) in plan {
        let parents: Vec<String> = commit
            .parents
            .iter()
            .map(|p| new_sha.get(p).cloned().unwrap_or_else(|| p.clone()))
            .collect();
        let made = commit_tree(repo, &commit, &parents, &message)?;
        new_sha.insert(sha.clone(), made.clone());
        if sha == old_tip {
            new_tip = made;
        }
    }
    if new_tip == old_tip {
        bail!("the rewrite did not reach the tip of {branch}");
    }
    // Compare-and-swap: a branch that moved meanwhile is left alone. The
    // new tip holds the same tree, so a checkout of the branch (HEAD
    // pointing at it) stays consistent with its index and files.
    git(
        repo,
        &[
            "update-ref",
            "-m",
            "kete: plain commit messages",
            &branch_ref,
            &new_tip,
            &old_tip,
        ],
    )?;
    Ok(changed)
}

/// Refuse when any checkout of `branch_ref` has uncommitted changes to
/// tracked files.
fn ensure_clean_checkouts(repo: &Path, branch_ref: &str) -> Result<()> {
    let list = git(repo, &["worktree", "list", "--porcelain"])?;
    let mut path: Option<&str> = None;
    for line in list.lines() {
        if let Some(p) = line.strip_prefix("worktree ") {
            path = Some(p);
        } else if line.strip_prefix("branch ") == Some(branch_ref) {
            let Some(dir) = path else { continue };
            let status = git(
                Path::new(dir),
                &["status", "--porcelain", "--untracked-files=no"],
            )?;
            if !status.trim().is_empty() {
                bail!("{dir} has uncommitted changes; commit or stash them first");
            }
        }
    }
    Ok(())
}

fn short(sha: &str) -> &str {
    sha.get(..8).unwrap_or(sha)
}

/// One commit object as git stores it.
struct RawCommit {
    tree: String,
    parents: Vec<String>,
    author: Ident,
    committer: Ident,
    encoding: Option<String>,
    /// Carries a `gpgsig`, `gpgsig-sha256` or `mergetag` header, which a
    /// rewrite cannot keep.
    signed: bool,
    message: String,
}

struct Ident {
    name: String,
    email: String,
    /// git's raw `<unix seconds> <+hhmm>`.
    date: String,
}

impl Ident {
    fn parse(value: &str) -> Result<Self> {
        let bad = || anyhow!("unreadable identity line: {value}");
        let open = value.find('<').ok_or_else(bad)?;
        let close = value.rfind('>').ok_or_else(bad)?;
        if close < open {
            return Err(bad());
        }
        Ok(Ident {
            name: value[..open].trim_end().to_string(),
            email: value[open + 1..close].to_string(),
            date: value[close + 1..].trim().to_string(),
        })
    }
}

impl RawCommit {
    fn read(repo: &Path, sha: &str) -> Result<Self> {
        let raw = git(repo, &["cat-file", "commit", sha])?;
        let (headers, message) = raw.split_once("\n\n").unwrap_or((raw.as_str(), ""));
        let mut tree = None;
        let mut parents = Vec::new();
        let mut author = None;
        let mut committer = None;
        let mut encoding = None;
        let mut signed = false;
        for line in headers.lines() {
            if line.starts_with(' ') {
                continue; // continuation of a multi-line header
            }
            let (key, value) = line.split_once(' ').unwrap_or((line, ""));
            match key {
                "tree" => tree = Some(value.to_string()),
                "parent" => parents.push(value.to_string()),
                "author" => author = Some(Ident::parse(value)?),
                "committer" => committer = Some(Ident::parse(value)?),
                "encoding" => encoding = Some(value.to_string()),
                "gpgsig" | "gpgsig-sha256" | "mergetag" => signed = true,
                _ => {}
            }
        }
        let missing = |what: &str| anyhow!("commit {} has no {what}", short(sha));
        Ok(RawCommit {
            tree: tree.ok_or_else(|| missing("tree"))?,
            parents,
            author: author.ok_or_else(|| missing("author"))?,
            committer: committer.ok_or_else(|| missing("committer"))?,
            encoding,
            signed,
            message: message.to_string(),
        })
    }
}

/// Write `commit` again over `parents` with `message`, every identity and
/// date taken from the original rather than the environment.
fn commit_tree(
    repo: &Path,
    commit: &RawCommit,
    parents: &[String],
    message: &str,
) -> Result<String> {
    let mut cmd = git_command(repo);
    if let Some(enc) = &commit.encoding {
        cmd.args(["-c", &format!("i18n.commitEncoding={enc}")]);
    }
    cmd.args(["commit-tree", "--no-gpg-sign", &commit.tree]);
    for parent in parents {
        cmd.args(["-p", parent]);
    }
    cmd.env("GIT_AUTHOR_NAME", &commit.author.name)
        .env("GIT_AUTHOR_EMAIL", &commit.author.email)
        .env("GIT_AUTHOR_DATE", format!("@{}", commit.author.date))
        .env("GIT_COMMITTER_NAME", &commit.committer.name)
        .env("GIT_COMMITTER_EMAIL", &commit.committer.email)
        .env("GIT_COMMITTER_DATE", format!("@{}", commit.committer.date))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = cmd.spawn().context("could not run git")?;
    child
        .stdin
        .take()
        .ok_or_else(|| anyhow!("git commit-tree has no stdin"))?
        .write_all(message.as_bytes())?;
    let out = child.wait_with_output()?;
    if !out.status.success() {
        bail!(
            "git commit-tree failed: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

fn git_command(repo: &Path) -> Command {
    let mut cmd = Command::new("git");
    cmd.current_dir(repo)
        .env_remove("GIT_DIR")
        .env_remove("GIT_WORK_TREE")
        .env_remove("GIT_INDEX_FILE")
        .stdin(Stdio::null());
    #[cfg(windows)]
    nebula_core::platform::no_window(&mut cmd);
    cmd
}

/// Run git in `repo`, returning stdout, or its stderr as the error.
fn git(repo: &Path, args: &[&str]) -> Result<String> {
    let out = git_command(repo)
        .args(args)
        .output()
        .context("could not run git")?;
    if !out.status.success() {
        bail!(
            "git {} failed: {}",
            args.first().unwrap_or(&""),
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn strips_every_co_author() {
        let m = "Fix rounding\n\nCo-Authored-By: Claude Opus <noreply@anthropic.com>\nCo-authored-by: Sam Lee <sam@tnt.co.nz>\n";
        let (out, changed) = strip_ai_trailers(m);
        assert!(changed);
        assert_eq!(out.trim_end(), "Fix rounding");
    }

    #[test]
    fn strips_generated_footer() {
        let (out, changed) = strip_ai_trailers(
            "Add x\n\n🤖 Generated with [Claude Code](https://claude.com/claude-code)\n",
        );
        assert!(changed);
        assert_eq!(out.trim_end(), "Add x");
        let (out, changed) = strip_ai_trailers("Add y\n\nGenerated with [Tool](https://x)\n");
        assert!(changed);
        assert_eq!(out, "Add y\n");
    }

    #[test]
    fn untouched_message_unchanged() {
        assert!(!strip_ai_trailers("Plain\n").1);
        // Exactly as given, trailing blank lines and all.
        let m = "Plain\n\nBody mentions co-authored-by: in passing\n\n";
        assert_eq!(strip_ai_trailers(m), (m.to_string(), false));
    }

    #[test]
    fn keeps_the_body_and_other_trailers() {
        let m = "Title\n\nBody line.\n\nReviewed-by: A <a@x>\nCO-AUTHORED-BY: B <b@x>\r\n";
        let (out, changed) = strip_ai_trailers(m);
        assert!(changed);
        assert_eq!(out, "Title\n\nBody line.\n\nReviewed-by: A <a@x>\n");
    }

    /// A scratch directory under the system temp dir, removed on drop.
    struct Scratch(PathBuf);

    impl Scratch {
        fn new(tag: &str) -> Self {
            let nanos = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let dir = std::env::temp_dir().join(format!(
                "kete-attribution-{tag}-{}-{nanos}",
                std::process::id()
            ));
            std::fs::create_dir_all(&dir).unwrap();
            Scratch(dir)
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn run(dir: &Path, args: &[&str]) -> String {
        git(dir, args).unwrap_or_else(|e| panic!("git {args:?}: {e}"))
    }

    /// Commit `file` = `text` with `message`, at a fixed author date and a
    /// different fixed committer date, so a rewrite that took the clock
    /// or the environment would show.
    fn commit(dir: &Path, file: &str, text: &str, message: &str, at: u64) {
        std::fs::write(dir.join(file), text).unwrap();
        run(dir, &["add", file]);
        let out = git_command(dir)
            .args(["commit", "--no-verify", "-q", "-F", "-"])
            .env("GIT_AUTHOR_NAME", "Ana Author")
            .env("GIT_AUTHOR_EMAIL", "ana@example.com")
            .env("GIT_AUTHOR_DATE", format!("@{at} +1300"))
            .env("GIT_COMMITTER_NAME", "Cam Committer")
            .env("GIT_COMMITTER_EMAIL", "cam@example.com")
            .env("GIT_COMMITTER_DATE", format!("@{} -0500", at + 77))
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .and_then(|mut child| {
                child.stdin.take().unwrap().write_all(message.as_bytes())?;
                child.wait_with_output()
            })
            .unwrap();
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
    }

    /// A bare remote holding `main` with one commit, and a clone with two
    /// local commits on top: the first with co-author lines, the second
    /// plain.
    fn fixture(scratch: &Scratch) -> (PathBuf, PathBuf) {
        let remote = scratch.0.join("remote.git");
        let work = scratch.0.join("work");
        std::fs::create_dir_all(&remote).unwrap();
        std::fs::create_dir_all(&work).unwrap();
        run(&remote, &["init", "-q", "--bare", "-b", "main"]);
        run(&work, &["init", "-q", "-b", "main"]);
        for (k, v) in [
            ("user.name", "Fixture"),
            ("user.email", "fixture@example.com"),
            ("commit.gpgsign", "false"),
            ("core.autocrlf", "false"),
        ] {
            run(&work, &["config", k, v]);
        }
        run(
            &work,
            &["remote", "add", "origin", &remote.to_string_lossy()],
        );
        commit(&work, "a.txt", "base\n", "Base\n", 1_700_000_000);
        run(&work, &["push", "-q", "-u", "origin", "main"]);
        commit(
            &work,
            "a.txt",
            "base\nmore\n",
            "Fix rounding\n\nCo-Authored-By: Claude Opus <noreply@anthropic.com>\nCo-authored-by: Sam Lee <sam@tnt.co.nz>\n",
            1_700_000_100,
        );
        commit(
            &work,
            "b.txt",
            "plain\n",
            "Plain follow-up\n",
            1_700_000_200,
        );
        (remote, work)
    }

    fn identities(dir: &Path, rev: &str) -> String {
        run(
            dir,
            &[
                "log",
                "-1",
                "--date=raw",
                "--format=%T|%an|%ae|%ad|%cn|%ce|%cd",
                rev,
            ],
        )
    }

    #[test]
    fn rewrite_only_touches_unpushed() {
        let scratch = Scratch::new("rewrite");
        let (_remote, work) = fixture(&scratch);
        let pushed = run(&work, &["rev-parse", "origin/main"]);
        let before_tip = identities(&work, "main");
        let before_mid = identities(&work, "main~1");

        let found = find_unpushed_ai_commits(&work, "origin/main..main").unwrap();
        assert_eq!(
            found,
            vec![run(&work, &["rev-parse", "main~1"]).trim().to_string()]
        );

        assert_eq!(rewrite_unpushed(&work, "origin/main", "main").unwrap(), 1);

        assert_eq!(run(&work, &["rev-parse", "origin/main"]), pushed);
        assert_eq!(run(&work, &["rev-parse", "main~2"]), pushed);
        // Trees, identities and dates exactly as they were, descendant too.
        assert_eq!(identities(&work, "main"), before_tip);
        assert_eq!(identities(&work, "main~1"), before_mid);
        assert_eq!(
            run(&work, &["log", "-1", "--format=%B", "main~1"]).trim_end(),
            "Fix rounding"
        );
        assert_eq!(
            run(&work, &["log", "-1", "--format=%B", "main"]),
            "Plain follow-up\n\n"
        );
        // HEAD followed the branch, and the checkout is still clean.
        assert_eq!(
            run(&work, &["symbolic-ref", "HEAD"]).trim(),
            "refs/heads/main"
        );
        assert_eq!(run(&work, &["status", "--porcelain"]), "");
        assert_eq!(
            std::fs::read_to_string(work.join("a.txt")).unwrap(),
            "base\nmore\n"
        );
        assert!(find_unpushed_ai_commits(&work, "origin/main..main")
            .unwrap()
            .is_empty());
        // Nothing left to do: a second pass changes nothing.
        let tip = run(&work, &["rev-parse", "main"]);
        assert_eq!(rewrite_unpushed(&work, "origin/main", "main").unwrap(), 0);
        assert_eq!(run(&work, &["rev-parse", "main"]), tip);
    }

    #[test]
    fn refuses_a_dirty_checkout() {
        let scratch = Scratch::new("dirty");
        let (_remote, work) = fixture(&scratch);
        let tip = run(&work, &["rev-parse", "main"]);
        std::fs::write(work.join("a.txt"), "edited\n").unwrap();
        let err = rewrite_unpushed(&work, "origin/main", "main").unwrap_err();
        assert!(err.to_string().contains("uncommitted changes"), "{err}");
        assert_eq!(run(&work, &["rev-parse", "main"]), tip);
    }

    #[test]
    fn refuses_a_signed_commit() {
        let scratch = Scratch::new("signed");
        let (_remote, work) = fixture(&scratch);
        // Re-make the tip as a signed commit carrying a co-author line.
        let raw = run(&work, &["cat-file", "commit", "main"]);
        let (headers, _) = raw.split_once("\n\n").unwrap();
        let signed = format!(
            "{headers}\ngpgsig -----BEGIN PGP SIGNATURE-----\n \n -----END PGP SIGNATURE-----\n\nSigned\n\nCo-authored-by: Sam Lee <sam@tnt.co.nz>\n"
        );
        let path = scratch.0.join("signed.txt");
        std::fs::write(&path, signed).unwrap();
        let sha = run(
            &work,
            &["hash-object", "-t", "commit", "-w", &path.to_string_lossy()],
        );
        let old = run(&work, &["rev-parse", "main"]);
        run(
            &work,
            &["update-ref", "refs/heads/main", sha.trim(), old.trim()],
        );

        let err = rewrite_unpushed(&work, "origin/main", "main").unwrap_err();
        assert!(err.to_string().contains("signed"), "{err}");
        assert_eq!(run(&work, &["rev-parse", "main"]), sha);
    }
}
