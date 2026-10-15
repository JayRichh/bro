//! TRACKED SETTINGS: the files Kete writes into a checkout for an agent's
//! hooks (`.claude/settings.local.json`, Copilot's
//! `.github/copilot/settings.local.json`) that git tracks there.
//!
//! Since 0.52 the daemon keeps them out of `git status` through the
//! repository's `info/exclude`, but an exclude does nothing for a file git
//! already tracks: before 0.52 an agent's `git add -A` could commit one,
//! and then every spawn rewrites it (a modified file on a fresh card) and
//! the next `git add -A` commits Kete's hook commands, with this machine's
//! paths in them, again. Kete never untracks a file itself: the apps say
//! once per checkout which file it is and the command that untracks it
//! ([`untrack_command`]), and the user runs it.

use std::path::Path;
use std::process::{Command, Stdio};

/// The files Kete writes into a checkout, relative to its root.
pub const KETE_WRITTEN: &[&str] = &[
    ".claude/settings.local.json",
    ".github/copilot/settings.local.json",
];

/// Which of [`KETE_WRITTEN`] git tracks in the checkout at `root`. Empty
/// when none is, and when git can't say (not a checkout, no git).
pub fn tracked_in(root: &Path) -> Vec<&'static str> {
    let mut cmd = Command::new("git");
    cmd.arg("-C")
        .arg(root)
        .args(["-c", "core.quotepath=off", "ls-files", "-z", "--"])
        .args(KETE_WRITTEN)
        .env("GIT_OPTIONAL_LOCKS", "0")
        .env("GIT_TERMINAL_PROMPT", "0")
        .stdin(Stdio::null());
    #[cfg(windows)]
    crate::platform::no_window(&mut cmd);
    let Ok(out) = cmd.output() else {
        return Vec::new();
    };
    if !out.status.success() {
        return Vec::new();
    }
    let listed = String::from_utf8_lossy(&out.stdout);
    KETE_WRITTEN
        .iter()
        .copied()
        .filter(|f| listed.split('\0').any(|l| l == *f))
        .collect()
}

/// The command, run in the checkout, that stops git tracking `file` and
/// leaves it on disk.
pub fn untrack_command(file: &str) -> String {
    format!("git rm --cached {file}")
}

/// The one-line notice for checkouts found tracking Kete's files: each
/// `(label, file)` is a checkout's name for the user (`alpha feat/login`)
/// and one file it tracks. None for none.
pub fn notice(found: &[(String, &'static str)]) -> Option<String> {
    let (label, file) = found.first()?;
    let command = untrack_command(file);
    Some(match found.len() {
        1 => format!("{label} tracks Kete's {file}: run {command} there, then commit"),
        n if found.iter().all(|(_, f)| f == file) => format!(
            "{n} checkouts track Kete's {file} ({label} and {} more): run {command} in each, then commit",
            n - 1
        ),
        n => format!(
            "{n} checkouts track Kete's hook settings ({label} and {} more): run git rm --cached on the file in each, then commit",
            n - 1
        ),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn git(dir: &Path, args: &[&str]) {
        let out = Command::new("git")
            .arg("-C")
            .arg(dir)
            .args(args)
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
    }

    #[test]
    fn a_committed_hooks_file_is_found_and_an_untracked_one_is_not() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        git(root, &["init", "-q", "-b", "main"]);
        std::fs::create_dir_all(root.join(".claude")).unwrap();
        std::fs::write(root.join(".claude/settings.local.json"), "{}").unwrap();
        assert!(tracked_in(root).is_empty(), "on disk is not tracked");
        git(root, &["add", "-f", ".claude/settings.local.json"]);
        assert_eq!(tracked_in(root), vec![".claude/settings.local.json"]);
        git(
            root,
            &["rm", "-q", "--cached", ".claude/settings.local.json"],
        );
        assert!(tracked_in(root).is_empty());
        assert!(root.join(".claude/settings.local.json").exists());
        assert!(tracked_in(&root.join("nowhere")).is_empty());
    }

    #[test]
    fn the_notice_names_the_command() {
        assert_eq!(notice(&[]), None);
        let one = notice(&[("alpha feat/login".into(), KETE_WRITTEN[0])]).unwrap();
        assert_eq!(
            one,
            "alpha feat/login tracks Kete's .claude/settings.local.json: run git rm --cached .claude/settings.local.json there, then commit"
        );
        let two = notice(&[
            ("alpha main".into(), KETE_WRITTEN[0]),
            ("beta spike".into(), KETE_WRITTEN[1]),
        ])
        .unwrap();
        assert!(two.starts_with("2 checkouts"), "{two}");
        assert!(two.contains("git rm --cached on the file"), "{two}");
        let same = notice(&[
            ("alpha main".into(), KETE_WRITTEN[0]),
            ("beta spike".into(), KETE_WRITTEN[0]),
        ])
        .unwrap();
        assert!(
            same.contains("run git rm --cached .claude/settings.local.json in each"),
            "{same}"
        );
    }
}
