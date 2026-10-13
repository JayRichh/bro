//! Where a checkout lives on the web, derived from its git remote.
//!
//! Every hosting form of the same repository — `git@github.com:o/r.git`,
//! `ssh://git@github.com/o/r`, `https://github.com/o/r.git` — points at one
//! browsable page, so the hotkey works out that page rather than asking the
//! user to keep a link around for it.
//!
//! Synchronous `std::process` like git_diff.rs: this runs on a key press,
//! and `git remote` is a config read with no network in it.
//!
//! Two details the naive `s/git@/https:\/\//` version gets wrong:
//!
//! * **Credentials.** A remote cloned with a token in it
//!   (`https://x:ghp_…@github.com/o/r`) would hand that token to the
//!   browser — and to its history and sync. The userinfo is dropped.
//!
//! * **Local clones.** `/srv/git/repo` and `../sibling` are perfectly good
//!   remotes with no web page at all. Those are a flash, not a bad URL.

use std::path::Path;

/// The web page for the repository checked out at `root`. `Err` is a
/// user-facing flash message.
pub fn repo_url(root: &Path) -> Result<String, String> {
    let remote = configured_remote(root)?;
    web_url(&remote).ok_or_else(|| format!("remote has no web page: {remote}"))
}

/// The remote to follow: `origin` when it exists, else whichever one git
/// lists first. A repo with several remotes and no `origin` is rare enough
/// that guessing beats a picker.
fn configured_remote(root: &Path) -> Result<String, String> {
    if let Some(url) = remote_url(root, "origin") {
        return Ok(url);
    }
    let names = run_git(root, &["remote"])?;
    let first = names
        .lines()
        .map(str::trim)
        .find(|name| !name.is_empty())
        .ok_or_else(|| "no git remote on this repo".to_string())?;
    remote_url(root, first).ok_or_else(|| format!("no URL configured for remote {first}"))
}

fn remote_url(root: &Path, name: &str) -> Option<String> {
    let url = run_git(root, &["remote", "get-url", name]).ok()?;
    let url = url.trim().to_string();
    (!url.is_empty()).then_some(url)
}

/// `git -C root <args>`, stdout on success. A failing git (no remote by
/// that name, not a repo) is `Err` carrying its own complaint.
fn run_git(root: &Path, args: &[&str]) -> Result<String, String> {
    let out = crate::git_diff::git_command(root)
        .args(args)
        .output()
        .map_err(|e| format!("failed to run git: {e}"))?;
    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr);
        return Err(match stderr.trim() {
            "" => format!("git {} failed", args.join(" ")),
            msg => msg.to_string(),
        });
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

/// Moved to nebula-core (shared with the desktop window).
pub use nebula_core::remote::web_url;
