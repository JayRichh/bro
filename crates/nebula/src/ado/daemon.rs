//! `kete ado start-task`'s [`Workspace`]: the daemon over one-shot
//! connections (the CLI holds no subscription), and the repo's git.
//!
//! The new worktree is asked for with `CreateWorktree`, exactly as the
//! desktop asks. The agent is the orchestrator's child when the command
//! runs inside the orchestrator session (`OrchestratorSpawnWith` on the new
//! worktree, protocol 54), else a plain `CreateAgent` with the Agents
//! tab's model and effort.

use anyhow::{anyhow, bail, Result};
use kete_ado::tasks::{taken_from_refs, NewWorktree, Workspace, CREATE_WORKTREE_TIMEOUT};
use kete_ado::{AdoContext, NewAgent};
use nebula_core::codec::{read_frame, write_frame};
use nebula_core::{
    Agent, AgentId, AgentKind, AgentRole, ClientRequest, EntityId, Project, ProjectId, ServerEvent,
    Worktree, WorktreeId,
};
use nebula_tui::ipc::{connect_or_spawn, Connection};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// The daemon protocol that knows `OrchestratorSpawnWith`.
const ORCHESTRATOR_PROTOCOL: u32 = 54;
/// One request per connection, so one id.
const REQ_ID: u64 = 1;

/// What the daemon's tree says, as far as Start task cares.
#[derive(Debug, Clone, Default)]
pub struct Tree {
    pub projects: Vec<Project>,
    pub worktrees: Vec<Worktree>,
    pub agents: Vec<Agent>,
}

pub struct DaemonWorkspace {
    project: ProjectId,
    repo: PathBuf,
    remote_name: String,
    tree: Mutex<Tree>,
    protocol: u32,
    /// This session, when the command runs inside one.
    caller: Option<AgentId>,
}

impl DaemonWorkspace {
    /// The workspace for `ctx`'s checkout: its project in the daemon
    /// (spawned when none runs), found by path.
    pub async fn connect(ctx: &AdoContext) -> Result<Self> {
        let (tree, protocol) = fetch_tree().await?;
        let project = project_for_checkout(&tree, &ctx.path).ok_or_else(|| {
            anyhow!(
                "{} is not a kete project: run `kete add .` there first.",
                ctx.path.display()
            )
        })?;
        Ok(Self {
            project,
            repo: ctx.path.clone(),
            remote_name: ctx.remote_name.clone(),
            tree: Mutex::new(tree),
            protocol,
            caller: nebula_core::env::non_empty(nebula_core::env::AGENT_ID).map(AgentId),
        })
    }

    fn tree(&self) -> Tree {
        self.tree.lock().unwrap_or_else(|p| p.into_inner()).clone()
    }

    /// The checkout path of worktree `id`, refreshed from the daemon.
    pub fn worktree_path(&self, id: &str) -> Option<String> {
        self.refresh();
        self.tree()
            .worktrees
            .iter()
            .find(|w| w.id.0 == id)
            .map(|w| w.path.display().to_string())
    }

    /// Read the daemon's tree again (blocking this worker thread: the
    /// trait's lookup is synchronous).
    fn refresh(&self) {
        let fresh = tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(fetch_tree())
        });
        if let Ok((tree, _)) = fresh {
            *self.tree.lock().unwrap_or_else(|p| p.into_inner()) = tree;
        }
    }
}

#[async_trait::async_trait]
impl Workspace for DaemonWorkspace {
    async fn taken_branches(&self) -> Result<Vec<String>, String> {
        let refs = kete_ado::git_out(
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
            self.tree()
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
        let req = ClientRequest::CreateWorktree {
            req_id: REQ_ID,
            project: self.project.clone(),
            branch: branch.to_string(),
            base: base.map(str::to_string),
        };
        match tokio::time::timeout(CREATE_WORKTREE_TIMEOUT, one_shot(req)).await {
            Err(_) => Ok(NewWorktree::NoAnswer),
            Ok(Ok(Some(EntityId::Worktree(id)))) => Ok(NewWorktree::Made(id.0)),
            Ok(Ok(_)) => Err("The daemon did not report the new worktree.".into()),
            Ok(Err(e)) => Err(e.to_string()),
        }
    }

    fn worktree_on(&self, branch: &str) -> Option<String> {
        self.refresh();
        self.tree()
            .worktrees
            .iter()
            .find(|w| w.project_id == self.project && w.branch == branch)
            .map(|w| w.id.0.clone())
    }

    async fn settle_worktree(&self, branch: &str, base: Option<&str>) {
        // As the desktop: a branch cut from `origin/main` would track main.
        if base.is_some_and(|b| b.starts_with(&format!("{}/", self.remote_name))) {
            let _ = kete_ado::git_out(&self.repo, &["branch", "--unset-upstream", branch]).await;
        }
    }

    async fn create_agent(&self, spec: NewAgent) -> Result<String, String> {
        let cfg = nebula_config::Config::load();
        let tree = self.tree();
        let req = spawn_request(
            &cfg,
            &tree,
            self.caller.as_ref(),
            self.protocol,
            &self.project,
            spec,
        )
        .map_err(|e| e.to_string())?;
        match one_shot(req).await {
            Ok(Some(EntityId::Agent(id))) => Ok(id.0),
            Ok(_) => Err("The daemon did not report the new agent.".into()),
            Err(e) => Err(e.to_string()),
        }
    }
}

/// `--project` (and `--worktree`): the checkout a command acts in, from
/// the daemon's tree.
pub async fn project_checkout(project: &str, worktree: Option<&str>) -> Result<PathBuf> {
    let (tree, _) = fetch_tree().await?;
    resolve_checkout(&tree, project, worktree)
}

/// The project named `project` (its id, else its name, case folded and
/// unambiguous), then its worktree on `branch`, else its main checkout.
pub fn resolve_checkout(tree: &Tree, project: &str, branch: Option<&str>) -> Result<PathBuf> {
    let want = project.trim();
    let named: Vec<&Project> = match tree.projects.iter().find(|p| p.id.0 == want) {
        Some(p) => vec![p],
        None => tree
            .projects
            .iter()
            .filter(|p| p.name.eq_ignore_ascii_case(want))
            .collect(),
    };
    let p = match named.as_slice() {
        [p] => *p,
        [] => {
            let mut names: Vec<&str> = tree.projects.iter().map(|p| p.name.as_str()).collect();
            names.sort_unstable();
            bail!(
                "No kete project is called {want:?}. Projects: {}.",
                if names.is_empty() {
                    "none".to_string()
                } else {
                    names.join(", ")
                }
            )
        }
        _ => bail!(
            "Several kete projects are called {want:?}: name it by id (kete sessions --json)."
        ),
    };
    let mine = tree.worktrees.iter().filter(|w| w.project_id == p.id);
    let found = match branch.map(str::trim).filter(|b| !b.is_empty()) {
        Some(b) => mine
            .clone()
            .find(|w| w.branch == b)
            .or_else(|| mine.clone().find(|w| w.branch.eq_ignore_ascii_case(b)))
            .ok_or_else(|| anyhow!("{} has no worktree on {b}.", p.name))?,
        None => mine
            .clone()
            .find(|w| w.is_main)
            .ok_or_else(|| anyhow!("{} has no checkout kete knows.", p.name))?,
    };
    Ok(found.path.clone())
}

/// The project whose worktree is checked out at `path`.
pub fn project_for_checkout(tree: &Tree, path: &Path) -> Option<ProjectId> {
    let want = normal(path);
    tree.worktrees
        .iter()
        .find(|w| normal(&w.path) == want)
        .map(|w| w.project_id.clone())
}

/// A path for comparing: canonical when it exists, separators and (on
/// Windows) case folded.
fn normal(p: &Path) -> String {
    let p = std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf());
    let s = p.to_string_lossy().replace('\\', "/");
    let s = s.trim_end_matches('/').to_string();
    if cfg!(windows) {
        s.to_lowercase()
    } else {
        s
    }
}

/// The harness `id` names: a built-in, else a custom harness in the
/// registry. Empty is the quick prompt's harness.
pub fn resolve_kind(cfg: &nebula_config::Config, id: &str) -> Result<(AgentKind, Option<String>)> {
    let id = id.trim();
    if id.is_empty() {
        return Ok((cfg.quick_prompt_kind(), None));
    }
    if let Some(kind) = AgentKind::parse(id).filter(|k| *k != AgentKind::Custom) {
        return Ok((kind, None));
    }
    if cfg.harness_registry().iter().any(|h| h.id == id) {
        return Ok((AgentKind::Custom, Some(id.to_string())));
    }
    bail!("unknown harness {id}")
}

/// The request that starts `spec` in its worktree: the orchestrator's
/// child when `caller` is the orchestrator, else a plain session.
pub fn spawn_request(
    cfg: &nebula_config::Config,
    tree: &Tree,
    caller: Option<&AgentId>,
    protocol: u32,
    project: &ProjectId,
    spec: NewAgent,
) -> Result<ClientRequest> {
    let (kind, custom) = resolve_kind(cfg, &spec.kind)?;
    let (model, effort) = cfg.launch_model_effort(kind, custom.as_deref(), spec.model, spec.effort);
    let prompt = spec
        .prompt
        .map(|p| p.trim().to_string())
        .filter(|p| !p.is_empty());
    let profile = spec.profile.filter(|p| !p.trim().is_empty());
    let orchestrator = caller.filter(|c| {
        tree.agents
            .iter()
            .any(|a| &a.id == *c && a.role == AgentRole::Orchestrator && !a.archived)
    });
    if let Some(caller) = orchestrator {
        if protocol < ORCHESTRATOR_PROTOCOL {
            bail!(
                "the running daemon speaks protocol v{protocol}, too old to parent the agent \
                 under the orchestrator: {}",
                crate::upgrade::RESTART_DAEMON
            );
        }
        return Ok(ClientRequest::OrchestratorSpawnWith {
            req_id: REQ_ID,
            caller: caller.clone(),
            project: project.clone(),
            worktree: Some(WorktreeId(spec.worktree)),
            branch: None,
            base: None,
            name: String::new(),
            kind,
            custom_harness: custom,
            model,
            effort,
            profile,
            starting_prompt: prompt.unwrap_or_default(),
        });
    }
    Ok(ClientRequest::CreateAgent {
        req_id: REQ_ID,
        worktree: WorktreeId(spec.worktree),
        name: next_agent_name(&tree.agents),
        kind,
        custom_harness: custom,
        model,
        effort,
        auto_title: true,
        cloud_prompt: None,
        starting_prompt: prompt,
        issue_url: None,
        role: Default::default(),
        profile,
    })
}

/// The next free `agent-N`, as the desktop names a new session.
pub fn next_agent_name(agents: &[Agent]) -> String {
    let n = agents
        .iter()
        .filter_map(|a| a.name.strip_prefix("agent-")?.parse::<u32>().ok())
        .max()
        .unwrap_or(0);
    format!("agent-{}", n + 1)
}

async fn fetch_tree() -> Result<(Tree, u32)> {
    let mut conn = connect_or_spawn().await?;
    write_frame(&mut conn.stream, &ClientRequest::Subscribe).await?;
    loop {
        match read_frame::<ServerEvent, _>(&mut conn.stream).await? {
            Some(ServerEvent::Snapshot {
                projects,
                worktrees,
                agents,
                ..
            }) => {
                let tree = Tree {
                    projects,
                    worktrees,
                    agents,
                };
                return Ok((tree, conn.protocol_version));
            }
            Some(_) => continue,
            None => bail!("the daemon closed the connection before sending its sessions"),
        }
    }
}

/// Send `req` on a fresh connection and wait for its Ack (what it made)
/// or Error.
async fn one_shot(req: ClientRequest) -> Result<Option<EntityId>> {
    let mut conn: Connection = connect_or_spawn().await?;
    write_frame(&mut conn.stream, &req).await?;
    loop {
        match read_frame::<ServerEvent, _>(&mut conn.stream).await? {
            Some(ServerEvent::Ack { req_id, created }) if req_id == REQ_ID => return Ok(created),
            Some(ServerEvent::Error {
                req_id: Some(r),
                message,
            }) if r == REQ_ID => bail!("{message}"),
            Some(_) => continue,
            None => bail!("the daemon closed the connection before replying"),
        }
    }
}
