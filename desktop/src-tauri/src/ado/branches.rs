//! Branches: the shared logic from `kete-ado`, plus the daemon's
//! worktrees and sessions on each row, and the Tauri commands.

pub use kete_ado::branches::*;

use super::AdoState;
use crate::daemon::{Client, Model};
use anyhow::bail;
use nebula_core::{ClientRequest, EntityId, ProjectId, WorktreeId};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tauri::State;

/// Fill worktree and session from the daemon model: a worktree of the
/// same project with this branch checked out, and its first live agent.
fn attach_sessions(rows: &mut [BranchRow], model: &Model, project: &ProjectId) {
    for row in rows {
        let Some(w) = model
            .worktrees
            .iter()
            .find(|w| &w.project_id == project && w.branch == row.name)
        else {
            continue;
        };
        row.worktree = Some(w.id.0.clone());
        row.session = model
            .agents
            .iter()
            .find(|a| a.worktree_id == w.id && !a.archived)
            .map(|a| a.id.0.clone());
    }
}

/// The checkout path and project of a worktree in the daemon model.
fn locate(client: &Client, worktree: &str) -> std::result::Result<(PathBuf, ProjectId), String> {
    client
        .snapshot()
        .worktrees
        .iter()
        .find(|w| w.id.0 == worktree)
        .map(|w| (w.path.clone(), w.project_id.clone()))
        .ok_or_else(|| "that worktree is gone".to_string())
}
/// Every branch, grouped by person, with its task, PR, worktree and
/// session. The PR lookup uses the silent client, so opening the list
/// never brings up a sign-in window; when it fails the branches come back
/// without PRs.
#[tauri::command]
pub async fn ado_branches(
    client: State<'_, Arc<Client>>,
    ado: State<'_, AdoState>,
    worktree: String,
) -> std::result::Result<BranchTree, String> {
    let (repo, project) = locate(&client, &worktree)?;
    let ctx = ado.silent_context(&repo).await.ok().flatten();
    let remote_name = remote_of(ctx.as_ref(), &repo).await?;
    let tasks = ctx
        .as_ref()
        .map(|c| c.store.read_repo(&c.key, |s| s.branch_tasks.clone()))
        .unwrap_or_default();
    let r = repo.clone();
    let (mut rows, me) =
        blocking(move || Ok((collect(&r, &remote_name)?, my_initials(&r)))).await?;
    let prs = match &ctx {
        Some(c) => branch_prs(c, &tasks).await,
        None => Vec::new(),
    };
    attach_prs(&mut rows, &prs, &tasks);
    attach_sessions(&mut rows, &client.snapshot(), &project);
    Ok(group(rows, &me))
}

#[tauri::command]
pub async fn ado_fetch(
    client: State<'_, Arc<Client>>,
    ado: State<'_, AdoState>,
    worktree: String,
) -> std::result::Result<(), String> {
    let (repo, _) = locate(&client, &worktree)?;
    let ctx = ado.silent_context(&repo).await.ok().flatten();
    let remote_name = remote_of(ctx.as_ref(), &repo).await?;
    blocking(move || fetch(&repo, &remote_name)).await
}

/// Check `branch` out in a worktree through the daemon. A branch already
/// in a worktree returns that worktree's id; a remote-only branch is cut
/// from `<remote>/<branch>` so it tracks it. Returns the worktree id.
#[tauri::command]
pub async fn ado_checkout_worktree(
    client: State<'_, Arc<Client>>,
    ado: State<'_, AdoState>,
    worktree: String,
    branch: String,
) -> std::result::Result<String, String> {
    let (repo, project) = locate(&client, &worktree)?;
    let model = client.snapshot();
    if let Some(w) = model
        .worktrees
        .iter()
        .find(|w| w.project_id == project && w.branch == branch)
    {
        return Ok(w.id.0.clone());
    }
    let ctx = ado.silent_context(&repo).await.ok().flatten();
    let remote_name = remote_of(ctx.as_ref(), &repo).await?;
    let name = branch.clone();
    let base = blocking(move || {
        let local = git(
            &repo,
            &[
                "show-ref",
                "--verify",
                "--quiet",
                &format!("refs/heads/{name}"),
            ],
        )
        .is_ok();
        if local {
            return Ok(None);
        }
        let remote_ref = format!("refs/remotes/{remote_name}/{name}");
        if remote_name.is_empty()
            || git(&repo, &["show-ref", "--verify", "--quiet", &remote_ref]).is_err()
        {
            bail!("there is no branch named {name}");
        }
        Ok(Some(format!("{remote_name}/{name}")))
    })
    .await?;
    let req_id = client.req_id();
    let created = client
        .rpc_within(
            req_id,
            ClientRequest::CreateWorktree {
                req_id,
                project: project.clone(),
                branch: branch.clone(),
                base,
            },
            super::tasks::CREATE_WORKTREE_TIMEOUT,
        )
        .await;
    match created {
        Ok(Some(EntityId::Worktree(WorktreeId(id)))) => Ok(id),
        Ok(_) => Err("the daemon did not say which worktree it made".into()),
        // A big repo's checkout can outlast the wait: look for it.
        Err(e) if e.is::<crate::daemon::RpcTimeout>() => {
            let deadline = tokio::time::Instant::now() + super::tasks::RECONCILE_WAIT;
            loop {
                if let Some(w) = client
                    .snapshot()
                    .worktrees
                    .iter()
                    .find(|w| w.project_id == project && w.branch == branch)
                {
                    return Ok(w.id.0.clone());
                }
                if tokio::time::Instant::now() >= deadline {
                    return Err(format!(
                        "the daemon did not answer in time, and no worktree on {branch} showed up"
                    ));
                }
                tokio::time::sleep(Duration::from_millis(500)).await;
            }
        }
        Err(e) => Err(e.to_string()),
    }
}

/// Delete a local branch. Refused while a worktree has it checked out.
#[tauri::command]
pub async fn ado_delete_local(
    client: State<'_, Arc<Client>>,
    worktree: String,
    branch: String,
    force: Option<bool>,
) -> std::result::Result<Deleted, String> {
    let (repo, project) = locate(&client, &worktree)?;
    if client
        .snapshot()
        .worktrees
        .iter()
        .any(|w| w.project_id == project && w.branch == branch)
    {
        return Err(format!(
            "{branch} is checked out in a worktree. Remove the worktree first."
        ));
    }
    let force = force.unwrap_or(false);
    blocking(move || delete_local(&repo, &branch, force)).await
}
