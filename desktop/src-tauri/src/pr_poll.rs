//! Each checkout's pull request, now and then, for the repo rows' chips
//! and the board's Ship lane: Azure DevOps for ADO remotes, `gh` for
//! the rest. Every checkout is asked, the main one too, and a `gh` PR is
//! reported in any state (open, merged, closed), so the window can say a
//! branch landed. A manual refresh wakes the poll early.
//!
//! With Auto-clean merged worktrees on, a linked worktree whose PR is a
//! merge is handed to the daemon (`CleanupMergedWorktree`), once per run,
//! as the terminal app does. The daemon re-checks everything that makes
//! the delete safe (never the root or default branch, HEAD still the
//! merged head, a clean checkout, nothing running) under the lock every
//! worktree delete takes, so the window and a terminal app both asking is
//! harmless: the second finds the worktree gone and is refused. Its
//! refusals are not shown; the next poll simply sees the worktree again.

use crate::ado::prs::{self, PrCtx};
use crate::ado::{AdoContext, AdoState};
use crate::daemon::{self, Client};
use crate::git;
use nebula_core::{ClientRequest, WorktreeId};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tauri::{Manager, State};
use tokio::sync::Notify;

/// Between two polls, unless a refresh wakes it.
const POLL_GAP: Duration = Duration::from_secs(90);

/// Wakes the poll before its gap is up.
#[derive(Default)]
pub struct PrRefresh(Arc<Notify>);

/// Ask every checkout's pull request again now (the menu's and the
/// palette's Refresh pull requests).
#[tauri::command]
pub fn refresh_prs(refresh: State<'_, PrRefresh>) {
    refresh.0.notify_one();
}

/// The open ADO pull request from `branch`. Its client is silent: the
/// poll never opens a sign-in window, and finds a token once a
/// user-initiated command has signed in. A sign-in failure pauses the
/// org's poll until a command works for it again.
async fn ado_open_pr(ado: &AdoState, ctx: AdoContext, branch: &str) -> Option<daemon::PrInfo> {
    let org = ctx.remote.org_url.clone();
    if ado.is_paused(&org) {
        return None;
    }
    let res = async {
        let ctx = PrCtx::new(ctx).await?;
        prs::for_source(&ctx.ado.client, &ctx.repo_id, branch).await
    }
    .await;
    match res {
        Ok(list) => list.into_iter().next().map(|p| daemon::PrInfo {
            number: p.id.into(),
            url: p.url,
            title: p.title,
            draft: p.is_draft,
            state: "open".into(),
            head_sha: String::new(),
        }),
        Err(e) => {
            if prs::is_auth_error(&e) {
                ado.pause(&org);
            }
            None
        }
    }
}

/// One checkout the poll asks about.
struct Wanted {
    id: String,
    path: PathBuf,
    branch: String,
    is_main: bool,
}

pub async fn poll_prs(client: Arc<Client>, app: tauri::AppHandle) {
    let ado = app.state::<AdoState>();
    let refresh = app.state::<PrRefresh>().0.clone();
    let have_gh = tauri::async_runtime::spawn_blocking(git::have_gh)
        .await
        .unwrap_or(false);
    // Worktrees already handed to the daemon for cleanup this run.
    let mut requested: HashSet<String> = HashSet::new();
    loop {
        let model = client.snapshot();
        let wanted: Vec<Wanted> = model
            .worktrees
            .iter()
            .map(|w| Wanted {
                id: w.id.0.clone(),
                path: w.path.clone(),
                branch: w.branch.clone(),
                is_main: w.is_main,
            })
            .collect();
        let mut out = HashMap::new();
        let mut merged = Vec::new();
        for w in wanted {
            // Several ADO remotes and none of them origin reads as none.
            let pr = match ado.silent_context(&w.path).await.ok().flatten() {
                Some(ctx) => ado_open_pr(&ado, ctx, &w.branch).await,
                None if have_gh => {
                    let (path, branch) = (w.path.clone(), w.branch.clone());
                    tauri::async_runtime::spawn_blocking(move || git::branch_pr(&path, &branch))
                        .await
                        .ok()
                        .flatten()
                }
                None => None,
            };
            if let Some(pr) = pr {
                let id = w.id.clone();
                if pr.state == "merged" && !w.is_main && !pr.head_sha.is_empty() {
                    merged.push((w, pr.clone()));
                }
                out.insert(id, pr);
            }
        }
        client.set_prs(out);
        if nebula_config::Config::load().auto_cleanup_merged {
            for (w, pr) in merged {
                if requested.contains(&w.id) {
                    continue;
                }
                // A PR still open onto this branch would lose its base.
                let (path, branch) = (w.path.clone(), w.branch.clone());
                let stacked = tauri::async_runtime::spawn_blocking(move || {
                    git::has_open_pr_onto(&path, &branch)
                })
                .await
                .ok()
                .flatten();
                if stacked != Some(false) {
                    continue;
                }
                requested.insert(w.id.clone());
                let _ = client.send(ClientRequest::CleanupMergedWorktree {
                    req_id: client.req_id(),
                    id: WorktreeId(w.id),
                    branch: w.branch,
                    pr_number: pr.number,
                    pr_url: pr.url,
                    head_sha: pr.head_sha,
                });
            }
        }
        tokio::select! {
            _ = tokio::time::sleep(POLL_GAP) => {}
            _ = refresh.notified() => {}
        }
    }
}
