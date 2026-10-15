//! `kete ship`: a worktree's pull request from the command line, through
//! the shared `kete_ado::ship` (the desktop window's ship commands run the
//! same code). GitHub through `gh`, Azure DevOps through its API.
//!
//! Sign-in is never interactive: `gh` runs with its prompts off, every git
//! with `GCM_INTERACTIVE=never` / `GIT_TERMINAL_PROMPT=0`, and ADO through
//! the silent token sources `kete ado` uses. A missing sign-in is an error
//! that says how to sign in.

use crate::ado::daemon::{self, Tree};
use crate::ado::Target;
use crate::cli::ShipCommand;
use anyhow::{anyhow, Result};
use kete_ado::ship::{
    self, Created, ForgeKind, MergeOpts, Merged, NewPr, Place, Ship, ShipError, ShipErrorKind,
    ShipStatus,
};
use kete_ado::AdoState;
use nebula_core::{env, AgentId};
use std::fmt::Write as _;
use std::io::Read as _;
use std::path::{Path, PathBuf};

pub fn run_ship(command: ShipCommand, target: Target) -> Result<()> {
    // Before any thread or child exists: every git and gh inherits these,
    // so nothing can open a sign-in window or ask on the terminal.
    std::env::set_var("GCM_INTERACTIVE", "never");
    std::env::set_var("GIT_TERMINAL_PROMPT", "0");
    std::env::set_var("GH_PROMPT_DISABLED", "1");
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;
    let out = rt.block_on(run(command, target))?;
    if !out.is_empty() {
        println!("{}", out.trim_end());
    }
    Ok(())
}

async fn run(command: ShipCommand, target: Target) -> Result<String> {
    let (dir, tree) = checkout(&target).await?;
    let state = AdoState::system();
    let ship = Ship::new(&state, true);
    let place = Place::new(dir, nebula_config::Config::load().worktree_base_branch);
    match command {
        ShipCommand::Status { json } => {
            let st = ship.status(&place).await;
            Ok(if json {
                serde_json::to_string_pretty(&st)?
            } else {
                format_status(&st)
            })
        }
        ShipCommand::Pr {
            title,
            body,
            draft,
            base,
            json,
        } => {
            let body = match body.as_deref() {
                Some("-") => {
                    let mut text = String::new();
                    std::io::stdin().read_to_string(&mut text)?;
                    Some(text)
                }
                _ => body,
            };
            let title = title
                .map(|t| t.trim().to_string())
                .filter(|t| !t.is_empty());
            let (title, body) = match (title, body) {
                (Some(t), Some(b)) => (t, b),
                (title, body) => {
                    let (name, prompt) = session_for(tree.as_ref(), &place.path);
                    let s = place.suggest(name.as_deref(), prompt.as_deref()).await;
                    (title.unwrap_or(s.title), body.unwrap_or(s.body))
                }
            };
            let pr = NewPr {
                title,
                body,
                draft,
                base,
            };
            let made = ship.create_pr(&place, pr).await.map_err(ship_err)?;
            Ok(if json {
                serde_json::to_string_pretty(&made)?
            } else {
                format_created(&made)
            })
        }
        ShipCommand::Merge {
            method,
            delete_branch,
            json,
        } => {
            let opts = MergeOpts {
                method,
                delete_branch,
            };
            let merged = ship.merge_pr(&place, opts).await.map_err(ship_err)?;
            Ok(if json {
                serde_json::to_string_pretty(&merged)?
            } else {
                format_merged(&merged)
            })
        }
    }
}

/// The checkout the command acts in, and the daemon's tree when it was
/// read for that (or to find this session): `--project` and `--worktree`
/// from the tree, else the checkout the command runs in. Outside a
/// session and without either flag no daemon is asked, so none is started.
async fn checkout(target: &Target) -> Result<(PathBuf, Option<Tree>)> {
    let worktree = target.worktree.as_deref();
    if let Some(project) = &target.project {
        let (tree, _) = daemon::fetch_tree().await?;
        let dir = daemon::resolve_checkout(&tree, project, worktree)?;
        return Ok((dir, Some(tree)));
    }
    let cwd = std::env::current_dir()?;
    let top = kete_ado::git_out(&cwd, &["rev-parse", "--show-toplevel"])
        .await
        .map_err(|e| anyhow!("Not inside a git checkout: {e}"))?;
    let top = PathBuf::from(top.trim());
    if let Some(branch) = worktree {
        let (tree, _) = daemon::fetch_tree().await?;
        let project = daemon::project_for_checkout(&tree, &top).ok_or_else(|| {
            anyhow!(
                "{} is not a kete project: name one with --project, or run `kete add .` here.",
                top.display()
            )
        })?;
        let dir = daemon::resolve_checkout(&tree, &project.0, Some(branch))?;
        return Ok((dir, Some(tree)));
    }
    let tree = match env::non_empty(env::AGENT_ID) {
        Some(_) => daemon::fetch_tree().await.ok().map(|(t, _)| t),
        None => None,
    };
    Ok((top, tree))
}

/// The session whose name and last prompt suggest a title: this one when
/// the command runs inside it, else the checkout's most recent.
fn session_for(tree: Option<&Tree>, dir: &Path) -> (Option<String>, Option<String>) {
    let Some(tree) = tree else {
        return (None, None);
    };
    let Some(wt) = daemon::worktree_at(tree, dir) else {
        return (None, None);
    };
    let me = env::non_empty(env::AGENT_ID).map(AgentId);
    ship::session_words(&tree.agents, &wt.id, me.as_ref())
}

/// A ship failure in words, with the way to sign in when that is the cause
/// and the page to finish on when there is one.
pub fn ship_err(e: ShipError) -> anyhow::Error {
    let mut msg = e.message.clone();
    if e.kind == ShipErrorKind::SignIn && e.forge == Some(ForgeKind::Ado) {
        msg.push('\n');
        msg.push_str(crate::ado::SIGN_IN_HINT);
    }
    if let Some(url) = &e.url {
        msg.push('\n');
        msg.push_str(url);
    }
    anyhow!(msg)
}

fn forge_name(f: ForgeKind) -> &'static str {
    match f {
        ForgeKind::GitHub => "GitHub",
        ForgeKind::Ado => "Azure DevOps",
    }
}

fn pr_word(f: ForgeKind, n: u64) -> String {
    match f {
        ForgeKind::GitHub => format!("#{n}"),
        ForgeKind::Ado => format!("!{n}"),
    }
}

pub fn format_status(st: &ShipStatus) -> String {
    let mut out = String::new();
    let branch = st.branch.as_deref().unwrap_or("(detached HEAD)");
    let base = st
        .base
        .as_deref()
        .map(|b| format!(" -> {b}"))
        .unwrap_or_default();
    let forge = match (st.forge, st.remote.as_deref()) {
        (Some(f), Some(r)) => format!("  ({}, {r})", forge_name(f)),
        _ => String::new(),
    };
    let _ = writeln!(out, "{branch}{base}{forge}");
    if let Some(n) = st.unpushed.filter(|n| *n > 0) {
        let _ = writeln!(out, "{n} unpushed commit{}", if n == 1 { "" } else { "s" });
    }
    match &st.pr {
        Some(pr) => {
            let draft = if pr.draft { " draft" } else { "" };
            let _ = writeln!(
                out,
                "PR {} {}{draft}: {}",
                pr_word(pr.forge, pr.number),
                pr.state,
                pr.title
            );
            let _ = writeln!(out, "{}", pr.url);
        }
        None if st.error.is_none() && st.forge.is_some() => {
            let _ = writeln!(out, "No pull request yet: kete ship pr");
        }
        None => {}
    }
    if let Some(e) = &st.error {
        let _ = writeln!(out, "{}", ship_err(e.clone()));
    }
    out
}

fn format_created(c: &Created) -> String {
    let n = pr_word(c.pr.forge, c.pr.number);
    let mut out = if c.existing {
        format!("Pushed; {n} was already open: {}", c.pr.url)
    } else {
        let draft = if c.pr.draft { " (draft)" } else { "" };
        format!("Opened {n}{draft}: {}", c.pr.url)
    };
    if let Some(note) = &c.note {
        out.push('\n');
        out.push_str(note);
    }
    out
}

fn format_merged(m: &Merged) -> String {
    let n = pr_word(m.pr.forge, m.pr.number);
    let mut out = match m.state.as_str() {
        "queued" => format!("{n} is queued to complete: {}", m.pr.url),
        _ => format!("Merged {n}: {}", m.pr.url),
    };
    if m.branch_deleted {
        let _ = write!(out, "\nDeleted {} on the remote.", m.pr.head);
    }
    if let Some(note) = &m.note {
        out.push('\n');
        out.push_str(note);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use kete_ado::ship::PrInfo;

    fn pr(forge: ForgeKind, draft: bool) -> PrInfo {
        PrInfo {
            forge,
            number: 7,
            url: "https://x/7".into(),
            title: "Fix it".into(),
            state: "open".into(),
            draft,
            head: "fix".into(),
            base: "main".into(),
            head_sha: None,
        }
    }

    #[test]
    fn status_reads_as_one_line_per_fact() {
        let st = ShipStatus {
            forge: Some(ForgeKind::GitHub),
            remote: Some("origin".into()),
            branch: Some("fix".into()),
            base: Some("main".into()),
            unpushed: Some(2),
            pr: Some(pr(ForgeKind::GitHub, true)),
            error: None,
        };
        assert_eq!(
            format_status(&st),
            "fix -> main  (GitHub, origin)\n2 unpushed commits\nPR #7 open draft: Fix it\nhttps://x/7\n"
        );
        let none = ShipStatus {
            pr: None,
            unpushed: Some(0),
            ..st.clone()
        };
        assert!(format_status(&none).ends_with("No pull request yet: kete ship pr\n"));
        let signed_out = ShipStatus {
            forge: Some(ForgeKind::Ado),
            pr: None,
            error: Some(ShipError::new(
                ShipErrorKind::SignIn,
                Some(ForgeKind::Ado),
                "Sign in.",
            )),
            ..st
        };
        let text = format_status(&signed_out);
        assert!(
            text.contains("Sign in.\n") && text.contains("az login"),
            "{text}"
        );
        assert!(!text.contains("No pull request yet"));
    }

    #[test]
    fn created_and_merged_name_the_pr_the_forge_way() {
        let c = Created {
            pr: pr(ForgeKind::Ado, false),
            existing: true,
            note: None,
        };
        assert_eq!(
            format_created(&c),
            "Pushed; !7 was already open: https://x/7"
        );
        let m = Merged {
            pr: pr(ForgeKind::GitHub, false),
            state: "merged".into(),
            branch_deleted: true,
            note: None,
        };
        assert_eq!(
            format_merged(&m),
            "Merged #7: https://x/7\nDeleted fix on the remote."
        );
        let e = ShipError::new(ShipErrorKind::MergeInAdo, Some(ForgeKind::Ado), "Policies.");
        let mut e = e;
        e.url = Some("https://ado/pr/7".into());
        assert_eq!(ship_err(e).to_string(), "Policies.\nhttps://ado/pr/7");
    }
}
