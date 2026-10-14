//! Sprint tasks: the shared logic from `kete-ado`, the daemon-backed
//! [`Workspace`] Start task uses in the window, and the Tauri commands.

pub use kete_ado::tasks::*;

use super::{AdoState, AdoStateExt};
use crate::daemon::{Client, RpcTimeout};
use crate::NewAgent;
use kete_ado::client::CmdError;
use kete_ado::git_out;
use kete_ado::naming::validate_branch_name;
use nebula_core::{ClientRequest, EntityId, ProjectId};
use std::path::PathBuf;
use std::sync::Arc;
use tauri::State;

/// [`Workspace`] over the daemon and the project's repo.
struct DaemonWorkspace {
    client: Arc<Client>,
    project: ProjectId,
    repo: PathBuf,
    remote_name: String,
}

#[async_trait::async_trait]
impl Workspace for DaemonWorkspace {
    async fn taken_branches(&self) -> Result<Vec<String>, String> {
        let refs = git_out(
            &self.repo,
            &[
                "for-each-ref",
                "--format=%(refname)",
                "refs/heads",
                "refs/remotes",
            ],
        )
        .await?;
        let mut taken = taken_from_refs(&refs, &self.remote_name);
        taken.extend(
            self.client
                .snapshot()
                .worktrees
                .iter()
                .filter(|w| w.project_id == self.project)
                .map(|w| w.branch.clone()),
        );
        Ok(taken)
    }

    async fn create_worktree(
        &self,
        branch: &str,
        base: Option<&str>,
    ) -> Result<NewWorktree, String> {
        let req_id = self.client.req_id();
        let created = self
            .client
            .rpc_within(
                req_id,
                ClientRequest::CreateWorktree {
                    req_id,
                    project: self.project.clone(),
                    branch: branch.to_string(),
                    base: base.map(str::to_string),
                },
                CREATE_WORKTREE_TIMEOUT,
            )
            .await;
        match created {
            Ok(Some(EntityId::Worktree(id))) => Ok(NewWorktree::Made(id.0)),
            Ok(_) => Err("The daemon did not report the new worktree.".into()),
            Err(e) if e.is::<RpcTimeout>() => Ok(NewWorktree::NoAnswer),
            Err(e) => Err(e.to_string()),
        }
    }

    fn worktree_on(&self, branch: &str) -> Option<String> {
        self.client
            .snapshot()
            .worktrees
            .iter()
            .find(|w| w.project_id == self.project && w.branch == branch)
            .map(|w| w.id.0.clone())
    }

    async fn settle_worktree(&self, branch: &str, base: Option<&str>) {
        // The daemon cuts from an already-qualified `origin/main` with
        // tracking on, which would make main the task branch's upstream.
        // The branch is new work: it gets its own upstream when pushed.
        if base.is_some_and(|b| b.starts_with(&format!("{}/", self.remote_name))) {
            let _ = git_out(&self.repo, &["branch", "--unset-upstream", branch]).await;
        }
    }

    async fn create_agent(&self, spec: NewAgent) -> Result<String, String> {
        crate::spawn_agent(&self.client, spec).await
    }
}

// ---- commands ----

/// The project's teams, for the team picker.
#[tauri::command]
pub async fn ado_teams(
    client: State<'_, Arc<Client>>,
    ado: State<'_, AdoState>,
    worktree: String,
) -> Result<Vec<String>, CmdError> {
    let ctx = ado.context_for(&client, &worktree).await?;
    teams(&ctx).await.map_err(CmdError::from)
}

/// The team's iterations, by start date.
#[tauri::command]
pub async fn ado_iterations(
    client: State<'_, Arc<Client>>,
    ado: State<'_, AdoState>,
    worktree: String,
) -> Result<Vec<Iteration>, CmdError> {
    let ctx = ado.context_for(&client, &worktree).await?;
    iterations(&ctx).await.map_err(CmdError::from)
}

/// The tasks in `scope`, grouped by parent.
#[tauri::command]
pub async fn ado_tasks(
    client: State<'_, Arc<Client>>,
    ado: State<'_, AdoState>,
    worktree: String,
    scope: TaskScope,
) -> Result<TaskList, CmdError> {
    let ctx = ado.context_for(&client, &worktree).await?;
    tasks_or_cached(&ctx, &scope, &today_utc())
        .await
        .map_err(CmdError::from)
}

/// Remember the team this repo's views use (empty: the default team).
#[tauri::command]
pub async fn ado_set_team(
    client: State<'_, Arc<Client>>,
    ado: State<'_, AdoState>,
    worktree: String,
    team: String,
) -> Result<(), CmdError> {
    let ctx = ado.context_for(&client, &worktree).await?;
    set_team(&ctx, &team).map_err(CmdError::from)
}

/// The prompt Start would give an agent for task `id`, for the dialog.
#[tauri::command]
pub async fn ado_task_prompt(
    client: State<'_, Arc<Client>>,
    ado: State<'_, AdoState>,
    worktree: String,
    id: u32,
) -> Result<String, CmdError> {
    let ctx = ado.context_for(&client, &worktree).await?;
    prompt_for(&ctx, id).await.map_err(CmdError::from)
}

/// Start a task: branch, worktree, task link, and optionally assign,
/// activate and an agent. `worktree` is any checkout of the repo.
#[tauri::command]
pub async fn ado_start_task(
    client: State<'_, Arc<Client>>,
    ado: State<'_, AdoState>,
    worktree: String,
    req: StartTask,
) -> Result<StartedTask, CmdError> {
    // Refuse a bad name before any git or HTTP.
    validate_branch_name(&req.branch, &[])?;
    let row = client
        .snapshot()
        .worktrees
        .into_iter()
        .find(|w| w.id.0 == worktree || w.path == std::path::Path::new(&worktree))
        .ok_or("That worktree is gone.")?;
    let ctx = ado.context(&row.path).await?;
    let ws = DaemonWorkspace {
        client: client.inner().clone(),
        project: row.project_id,
        repo: row.path,
        remote_name: ctx.remote_name.clone(),
    };
    start_task(&ctx, &ws, req).await
}
