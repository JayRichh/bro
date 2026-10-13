//! Privileged commands for the ORCHESTRATOR SESSION, and its provisioning.
//!
//! The CLI wrappers are just another same-user socket client, so prompt text
//! is not part of the trust boundary. Every privileged operation here starts
//! by checking the caller row persisted in SQLite is the orchestrator.
//!
//! There is at most one orchestrator per machine: the `role` column carries
//! the pin (migration 33 backs it with a partial unique index), and every
//! path that grants the role — [`Daemon::start_orchestrator`],
//! [`Daemon::set_orchestrator`] — demotes whichever row held it before.
//! `CreateAgent` cannot grant it at all (see `server.rs`).

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use anyhow::{bail, Context, Result};
use nebula_core::{
    Agent, AgentId, AgentKind, AgentRole, AgentStatus, EntityId, Link, OrchestratorSession,
    OutputTail, Project, ProjectId, ReviewTabKind, ServerEvent, SessionRef, SubagentSummary,
    TerminalTab, Worktree, WorktreeId, MAX_CLOUD_PROMPT_BYTES,
};

use crate::registry::{CreateAgentSpec, Daemon};

/// The orchestrator's system prompt (or, for a harness without a
/// system-prompt flag, the opening of its first prompt), injected by role on
/// every cold spawn — see [`launch_rule`].
pub const CLAUDE_ORCHESTRATOR_GUIDANCE: &str = "[kete] You are the pinned ORCHESTRATOR session \
for kete: the one session on this machine that coordinates work across every project, worktree \
and session, so the user does not have to click through them. The daemon refuses the commands \
below unless this session is the persisted orchestrator.\n\n\
## Reading the room\n\
  kete orchestrator list [--json]\n\
Every live session in ATTENTION ORDER, the order the user's own `/` palette uses: \
`needs_feedback` (blocked on a permission or a question: the user's attention first), then \
`running` (mid-turn: leave it be), then `unread` (finished a turn nobody has looked at yet), \
then `idle`, `terminated` and `disconnected`. Each row names its project, branch, harness, \
parent session and open subagents. Use `--json` when you need exact ids; names work everywhere \
an id does, as long as they are unambiguous.\n\
  kete orchestrator read <session> [--lines N]\n\
Recent context: the harness transcript when kete knows it, else the cleaned terminal tail.\n\n\
## Acting\n\
  kete orchestrator send <session> \"<prompt>\" [--no-wait] [--timeout S]\n\
Delivers a follow-up labelled `[from orchestrator]`. A busy target is waited on until idle; \
`--no-wait` refuses instead. You cannot send to yourself.\n\
  kete orchestrator spawn --project <project> [--worktree <branch>] [--base <ref>] \
[--kind claude|codex|...] [--name N] [--model M] [--effort E] [--profile P] \"<task>\"\n\
Starts a worker on a self-contained task. `--worktree <branch>` reuses that checkout or cuts a \
new worktree for it (from `--base`, else the project's base branch): prefer a fresh worktree \
for any change that could collide with work already running on a branch.\n\
  kete orchestrator review <session>... [--tab terminal|diff|history|pr]...\n\
Opens kete's tabbed review modal for the user on one or more sessions.\n\
The everyday commands work here too: `kete sessions`, `kete read <session>`, and \
`kete ask <session> \"<question>\"` (send a question, wait, and print the answer). Inside your \
own turn you may use subagents for research or parallel reading; delegate real changes to \
spawned workers so they show up in kete where the user can see them.\n\n\
## Rules\n\
Tell, then ask: report what you found or did in a line or two, then ask before anything \
destructive or irreversible. Never approve another session's permission prompt unless the user \
explicitly tells you to; surface it instead. Prefer `read` over `ask` when the answer is already \
on screen, and do not interrupt a `running` session unless the user asked you to.\n\n\
## Azure DevOps\n\
(Reserved: `kete ado ...` commands for work items, pull requests and pipelines arrive in a later \
release. Until then, do not guess at them.)";

/// Name a started orchestrator gets when the request names none.
pub const DEFAULT_ORCHESTRATOR_NAME: &str = "orchestrator";

/// The tag every orchestrator follow-up opens with, so the receiving model
/// (and the user reading its pane) knows where the turn came from.
const FROM_ORCHESTRATOR: &str = "[from orchestrator]";

/// How long a waiting `send` waits for a busy target, when the request says 0.
const DEFAULT_TELL_TIMEOUT: Duration = Duration::from_secs(120);

pub type OrchestratorTree = (
    Vec<Project>,
    Vec<Worktree>,
    Vec<Agent>,
    Vec<TerminalTab>,
    Vec<Link>,
);

/// What `StartOrchestrator` asks for.
#[derive(Debug, Clone, Default)]
pub struct StartOrchestratorSpec {
    pub kind: Option<AgentKind>,
    pub custom_harness: Option<String>,
    pub model: Option<String>,
    pub effort: Option<String>,
    pub profile: Option<String>,
    /// None: the dedicated orchestrator workspace (see
    /// [`Daemon::orchestrator_home`]).
    pub project: Option<ProjectId>,
    pub name: Option<String>,
    pub replace: bool,
}

/// Every orchestrator-spawned worker's knobs, `OrchestratorSpawn[With]`.
#[derive(Debug, Clone)]
pub struct OrchestratorSpawnSpec {
    pub project: ProjectId,
    pub worktree: Option<WorktreeId>,
    pub branch: Option<String>,
    pub base: Option<String>,
    pub name: String,
    pub kind: AgentKind,
    pub custom_harness: Option<String>,
    pub model: Option<String>,
    pub effort: Option<String>,
    pub profile: Option<String>,
    pub starting_prompt: String,
}

/// Fold the role's guidance into a spawn's launch rule (the PR / issue
/// scope, when the row has one): the orchestrator's guidance leads, so it
/// rides the system-prompt flag where the harness maps one and opens a cold
/// spawn's first prompt where it prepends instead.
pub(crate) fn launch_rule(role: AgentRole, rule: Option<String>) -> Option<String> {
    if !role.is_orchestrator() {
        return rule;
    }
    Some(match rule {
        Some(rule) => format!("{CLAUDE_ORCHESTRATOR_GUIDANCE}\n\n{rule}"),
        None => CLAUDE_ORCHESTRATOR_GUIDANCE.to_string(),
    })
}

impl Daemon {
    pub(crate) fn require_orchestrator(&self, caller: &AgentId) -> Result<Agent> {
        let agent = self
            .store
            .get_agent(caller)?
            .context("orchestrator session not found")?;
        if agent.archived {
            bail!("orchestrator session is archived");
        }
        if agent.role != AgentRole::Orchestrator {
            bail!("this command is only available to the orchestrator session");
        }
        Ok(agent)
    }

    // ---- provisioning ----

    /// Where an orchestrator started without a project runs: a folder of
    /// its own under the data dir, so its hooks, notes and scratch files
    /// never land in one of the user's repos.
    pub fn orchestrator_home(&self) -> PathBuf {
        self.orchestrator_home_override()
            .unwrap_or_else(|| nebula_core::paths::data_dir().join("orchestrator"))
    }

    /// The root worktree an orchestrator lives in: `project`'s, or the
    /// dedicated workspace's — registered as a project named
    /// `orchestrator` (git-initialized) the first time one is started.
    async fn orchestrator_worktree(
        self: &Arc<Self>,
        project: Option<&ProjectId>,
    ) -> Result<Worktree> {
        let project = match project {
            Some(id) => {
                self.store.get_project(id)?.context("project not found")?;
                id.clone()
            }
            None => {
                let home = self.orchestrator_home();
                match self.project_at(&home)? {
                    Some(id) => id,
                    None => {
                        let created = self
                            .add_project(&home, Some(DEFAULT_ORCHESTRATOR_NAME.into()), true)
                            .await
                            .context("set up the orchestrator workspace")?;
                        let EntityId::Project(id) = created else {
                            bail!("the orchestrator workspace did not register as a project");
                        };
                        id
                    }
                }
            }
        };
        self.store
            .load_tree()?
            .1
            .into_iter()
            .filter(|w| w.project_id == project)
            .min_by_key(|w| (!w.is_main, w.sort_order))
            .context("project has no worktrees")
    }

    /// The project whose repo is `dir`, compared canonically (git reports
    /// repo paths in its own spelling, the data dir comes from the env).
    fn project_at(&self, dir: &Path) -> Result<Option<ProjectId>> {
        let Ok(want) = std::fs::canonicalize(dir) else {
            return Ok(None);
        };
        Ok(self
            .store
            .load_tree()?
            .0
            .into_iter()
            .find(|p| std::fs::canonicalize(&p.repo_path).is_ok_and(|have| have == want))
            .map(|p| p.id))
    }

    /// Start the machine's one orchestrator. A live (unarchived) one
    /// already pinned refuses unless `replace`; either way the previous
    /// holder is demoted to a worker — its session keeps running — before
    /// the new row is created with the role, so its very first spawn
    /// carries the orchestrator guidance.
    pub async fn start_orchestrator(
        self: &Arc<Self>,
        spec: StartOrchestratorSpec,
    ) -> Result<EntityId> {
        let previous = self.store.orchestrator()?;
        if let Some(current) = previous.as_ref().filter(|a| !a.archived) {
            if !spec.replace {
                bail!(
                    "an orchestrator is already running: {} ({}); replace it to start another",
                    current.name,
                    current.id
                );
            }
        }
        let worktree = self.orchestrator_worktree(spec.project.as_ref()).await?;
        let demoted = self.store.set_orchestrator(None)?;
        let name = spec
            .name
            .map(|n| n.trim().to_string())
            .filter(|n| !n.is_empty())
            .unwrap_or_else(|| DEFAULT_ORCHESTRATOR_NAME.into());
        let created = self
            .create_agent(CreateAgentSpec {
                parent: None,
                profile: spec.profile,
                worktree: worktree.id,
                name,
                kind: spec.kind.unwrap_or(AgentKind::Claude),
                custom_harness: spec.custom_harness,
                model: spec.model,
                effort: spec.effort,
                auto_title: false,
                cloud_prompt: None,
                starting_prompt: None,
                pr_url: None,
                issue_url: None,
                role: AgentRole::Orchestrator,
            })
            .await;
        match created {
            Ok(id) => {
                for agent in &demoted {
                    let _ = self.broadcast_agent(agent);
                }
                Ok(id)
            }
            Err(e) => {
                // Nothing was started: the pin goes back where it was.
                if let Some(prev) = previous.filter(|_| !demoted.is_empty()) {
                    let _ = self.store.set_orchestrator(Some(&prev.id));
                }
                Err(e)
            }
        }
    }

    /// Pin an existing agent as the orchestrator ("Make orchestrator"),
    /// demoting whoever held the role. A live, idle session is handed the
    /// guidance as its next turn at once; one mid-turn, or not running,
    /// gets it with its next cold spawn's system prompt.
    pub fn set_orchestrator(self: &Arc<Self>, id: &AgentId) -> Result<()> {
        let agent = self.store.get_agent(id)?.context("agent not found")?;
        if agent.archived {
            bail!("an archived session cannot be the orchestrator; unarchive it first");
        }
        if agent.cloud_session_id.is_some() {
            bail!("a Claude Cloud session cannot be the orchestrator: it has no local CLI");
        }
        let changed = self.store.set_orchestrator(Some(id))?;
        for agent in &changed {
            let _ = self.broadcast_agent(agent);
        }
        if changed.contains(id) && crate::session_context::is_idle(agent.status) {
            if let Some(session) = self.session(&SessionRef::Agent(id.clone())) {
                let note = format!(
                    "{CLAUDE_ORCHESTRATOR_GUIDANCE}\n\nThe user just made this session the \
                     orchestrator. Acknowledge in one line; do nothing else yet."
                );
                if let Err(e) = crate::session_context::write_prompt(&session, &note) {
                    tracing::warn!(agent = %id, error = %e, "orchestrator guidance delivery failed");
                }
            }
        }
        Ok(())
    }

    /// Demote whichever row holds the role. None held is not an error.
    pub fn clear_orchestrator(self: &Arc<Self>) -> Result<()> {
        for agent in self.store.set_orchestrator(None)? {
            let _ = self.broadcast_agent(&agent);
        }
        Ok(())
    }

    /// Whether `pid` runs inside a live AGENT session — the PTY child or
    /// any descendant of it. Provisioning requests from such a process are
    /// refused, so a worker cannot mint or become the orchestrator through
    /// its own shell. Terminal sessions are the user's own shells and pass.
    pub fn pid_inside_agent_session(&self, pid: u32) -> bool {
        let roots: HashSet<u32> = self
            .session_pids()
            .into_iter()
            .filter(|(sref, _, _)| matches!(sref, SessionRef::Agent(_)))
            .map(|(_, pid, _)| pid)
            .collect();
        if roots.is_empty() {
            return false;
        }
        descends_from(&process_parents(), pid, &roots)
    }

    // ---- privileged verbs ----

    pub fn orchestrator_tree(&self, caller: &AgentId) -> Result<OrchestratorTree> {
        self.require_orchestrator(caller)?;
        let (projects, worktrees, mut agents, mut terminals) = self.store.load_tree()?;
        for a in &mut agents {
            a.alive = self.is_alive(&SessionRef::Agent(a.id.clone()));
        }
        for t in &mut terminals {
            t.alive = self.is_alive(&SessionRef::Terminal(t.id.clone()));
        }
        Ok((
            projects,
            worktrees,
            agents,
            terminals,
            self.store.load_links()?,
        ))
    }

    /// Every live session in attention order, for `kete orchestrator list`.
    pub fn orchestrator_sessions(&self, caller: &AgentId) -> Result<Vec<OrchestratorSession>> {
        self.require_orchestrator(caller)?;
        let summaries = self.session_summaries()?;
        let agents: HashMap<AgentId, Agent> = self
            .store
            .load_tree()?
            .2
            .into_iter()
            .map(|a| (a.id.clone(), a))
            .collect();
        let mut subagents: HashMap<AgentId, Vec<SubagentSummary>> = HashMap::new();
        for sub in self.subagent_rows() {
            subagents
                .entry(sub.parent_agent.clone())
                .or_default()
                .push(sub);
        }
        Ok(attention_ordered(summaries, &agents, &mut subagents))
    }

    /// Legacy `OrchestratorSend`: the target by id, waiting as `send` does.
    pub async fn orchestrator_send(
        self: &Arc<Self>,
        caller: &AgentId,
        target: &AgentId,
        message: &str,
    ) -> Result<AgentId> {
        self.orchestrator_tell(caller, target.as_str(), message, Duration::ZERO, true)
            .await
    }

    /// Deliver a labelled follow-up to another agent, by id or unambiguous
    /// fuzzy name. A busy target is waited on until idle when `wait`, else
    /// refused. Returns the agent it went to.
    pub async fn orchestrator_tell(
        self: &Arc<Self>,
        caller: &AgentId,
        target: &str,
        message: &str,
        timeout: Duration,
        wait: bool,
    ) -> Result<AgentId> {
        self.require_orchestrator(caller)?;
        let text = validate_message(message)?;
        let timeout = if timeout.is_zero() {
            DEFAULT_TELL_TIMEOUT
        } else {
            timeout
        };
        let deadline = tokio::time::Instant::now() + timeout;
        let SessionRef::Agent(target_id) = self.resolve_session_target(target)? else {
            bail!("follow-ups can only be sent to agent sessions");
        };
        if &target_id == caller {
            bail!("the orchestrator cannot send to itself; name another session");
        }
        // Subscribed before the status is read, so a turn that ends in
        // between is not missed.
        let mut status_rx = self.events.subscribe();
        let agent = self
            .store
            .get_agent(&target_id)?
            .context("target agent not found")?;
        if agent.archived {
            bail!("target agent is archived");
        }
        if agent.cloud_session_id.is_some() {
            bail!("target agent is a Claude Cloud session; use its cloud message path");
        }
        if !crate::session_context::is_idle(agent.status) {
            if !wait {
                bail!(
                    "{} is {}; retry without --no-wait to queue behind its turn",
                    agent.name,
                    agent.status.as_str()
                );
            }
            crate::session_context::wait_until_idle(&mut status_rx, &target_id, deadline).await?;
        }
        let sref = SessionRef::Agent(target_id.clone());
        let session =
            self.ensure_session(&sref, crate::pty::DEFAULT_COLS, crate::pty::DEFAULT_ROWS)?;
        crate::session_context::write_prompt(&session, &format!("{FROM_ORCHESTRATOR}\n{text}"))?;
        Ok(target_id)
    }

    pub fn orchestrator_read(
        &self,
        caller: &AgentId,
        session: &SessionRef,
        max_bytes: u32,
    ) -> Result<Option<OutputTail>> {
        self.require_orchestrator(caller)?;
        Ok(self.read_output_tail_clean(session, max_bytes))
    }

    /// Transcript-aware recent context, by id or fuzzy name.
    pub fn orchestrator_read_session(
        &self,
        caller: &AgentId,
        target: &str,
        max_lines: u32,
    ) -> Result<crate::session_context::SessionRead> {
        self.require_orchestrator(caller)?;
        self.read_session_context(target, max_lines)
    }

    pub async fn orchestrator_spawn(
        self: &Arc<Self>,
        caller: &AgentId,
        spec: OrchestratorSpawnSpec,
    ) -> Result<EntityId> {
        self.require_orchestrator(caller)?;
        let OrchestratorSpawnSpec {
            project,
            worktree,
            branch,
            base,
            name,
            kind,
            custom_harness,
            model,
            effort,
            profile,
            starting_prompt,
        } = spec;
        let project = &project;
        let worktree = match worktree {
            Some(id) => {
                let wt = self
                    .store
                    .get_worktree(&id)?
                    .context("worktree not found")?;
                if &wt.project_id != project {
                    bail!("worktree does not belong to the requested project");
                }
                wt
            }
            None => {
                if branch.as_ref().is_none_or(|b| b.trim().is_empty()) {
                    if let Some(existing) = self
                        .store
                        .load_tree()?
                        .1
                        .into_iter()
                        .filter(|w| &w.project_id == project)
                        .min_by_key(|w| (!w.is_main, w.sort_order))
                    {
                        existing
                    } else {
                        bail!("project has no worktrees");
                    }
                } else {
                    let branch = slugify_branch(branch.unwrap_or_default().as_str());
                    let branch = if branch.is_empty() {
                        random_branch_name()
                    } else {
                        branch
                    };
                    if let Some(existing) = self
                        .store
                        .load_tree()?
                        .1
                        .into_iter()
                        .find(|w| &w.project_id == project && w.branch == branch)
                    {
                        existing
                    } else {
                        let created = self
                            .create_worktree(project, &branch, base.as_deref())
                            .await?;
                        let EntityId::Worktree(id) = created else {
                            unreachable!("create_worktree returns a worktree id");
                        };
                        self.store
                            .get_worktree(&id)?
                            .context("created worktree not found")?
                    }
                }
            }
        };
        let (_, _, agents, _) = self.store.load_tree()?;
        let taken = agents
            .iter()
            .filter(|a| a.worktree_id == worktree.id)
            .map(|a| a.name.clone())
            .collect::<Vec<_>>();
        let name = if name.trim().is_empty() {
            crate::sibling::sibling_name(&taken)
        } else {
            name
        };
        self.create_agent(CreateAgentSpec {
            parent: Some(caller.clone()),
            profile,
            worktree: worktree.id,
            name,
            kind,
            custom_harness,
            model,
            effort,
            auto_title: true,
            cloud_prompt: None,
            starting_prompt: Some(starting_prompt),
            pr_url: None,
            issue_url: None,
            role: AgentRole::Worker,
        })
        .await
    }

    pub fn orchestrator_open_review(
        &self,
        caller: &AgentId,
        sessions: Vec<SessionRef>,
        tabs: Vec<ReviewTabKind>,
    ) -> Result<()> {
        self.require_orchestrator(caller)?;
        if sessions.is_empty() {
            bail!("no sessions named for review");
        }
        for session in &sessions {
            match session {
                SessionRef::Agent(id) => {
                    self.store.get_agent(id)?.context("agent not found")?;
                }
                SessionRef::Terminal(id) => {
                    self.store.get_terminal(id)?.context("terminal not found")?;
                }
            }
        }
        self.broadcast(ServerEvent::ReviewOpened {
            opener: caller.clone(),
            sessions,
            tabs: if tabs.is_empty() {
                vec![
                    ReviewTabKind::Terminal,
                    ReviewTabKind::Diff,
                    ReviewTabKind::History,
                    ReviewTabKind::PullRequest,
                ]
            } else {
                tabs
            },
        });
        Ok(())
    }
}

/// The attention tier a session sorts into: what the user's `/` palette
/// uses, so the orchestrator triages in the order the user would.
fn attention_tier(agent: Option<&Agent>) -> u8 {
    match agent {
        Some(a) => match a.status {
            AgentStatus::NeedsFeedback => 0,
            AgentStatus::Running => 1,
            AgentStatus::Finished if a.unseen => 2,
            _ => 3,
        },
        None => 4,
    }
}

/// `kete sessions` rows in attention order, each grown by role, parent,
/// subagents and its triage state.
fn attention_ordered(
    summaries: Vec<nebula_core::SessionSummary>,
    agents: &HashMap<AgentId, Agent>,
    subagents: &mut HashMap<AgentId, Vec<SubagentSummary>>,
) -> Vec<OrchestratorSession> {
    let mut rows: Vec<OrchestratorSession> = summaries
        .into_iter()
        .map(|summary| {
            let agent = match &summary.session {
                SessionRef::Agent(id) => agents.get(id),
                SessionRef::Terminal(_) => None,
            };
            let state = match agent {
                Some(a) if a.status == AgentStatus::Finished && a.unseen => "unread".to_string(),
                _ => summary.status.clone(),
            };
            OrchestratorSession {
                attention: 0,
                state,
                role: agent.map(|a| a.role).unwrap_or_default(),
                parent: agent.and_then(|a| a.parent.clone()),
                status_changed_at: agent.map(|a| a.status_changed_at).unwrap_or(0),
                subagents: agent
                    .and_then(|a| subagents.remove(&a.id))
                    .unwrap_or_default(),
                summary,
            }
        })
        .collect();
    rows.sort_by_key(|row| {
        let agent = match &row.summary.session {
            SessionRef::Agent(id) => agents.get(id),
            SessionRef::Terminal(_) => None,
        };
        (
            attention_tier(agent),
            std::cmp::Reverse(row.status_changed_at),
            row.summary.name.clone(),
        )
    });
    for (i, row) in rows.iter_mut().enumerate() {
        row.attention = i as u32;
    }
    rows
}

/// Whether `pid` is one of `roots` or descends from one, walking `parents`
/// (pid → ppid) upward. Bounded, so a cycle in a racy snapshot ends.
fn descends_from(parents: &HashMap<u32, u32>, pid: u32, roots: &HashSet<u32>) -> bool {
    let mut at = pid;
    for _ in 0..128 {
        if roots.contains(&at) {
            return true;
        }
        match parents.get(&at) {
            Some(&parent) if parent != at && parent != 0 => at = parent,
            _ => return false,
        }
    }
    false
}

/// The machine's process table as pid → ppid; empty when it can't be read.
#[cfg(windows)]
fn process_parents() -> HashMap<u32, u32> {
    nebula_core::platform::process_table()
        .into_iter()
        .map(|row| (row.pid, row.ppid))
        .collect()
}

#[cfg(unix)]
fn process_parents() -> HashMap<u32, u32> {
    let table = std::process::Command::new("ps")
        .args(["-axo", "pid=,ppid="])
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .unwrap_or_default();
    table
        .lines()
        .filter_map(|line| {
            let mut cols = line.split_whitespace();
            Some((cols.next()?.parse().ok()?, cols.next()?.parse().ok()?))
        })
        .collect()
}

fn validate_message(message: &str) -> Result<&str> {
    let text = message.trim();
    if text.is_empty() {
        bail!("message is empty");
    }
    if text.contains('\0') {
        bail!("message cannot contain NUL bytes");
    }
    if text.len() > MAX_CLOUD_PROMPT_BYTES {
        bail!(
            "message is too long (max {} KiB)",
            MAX_CLOUD_PROMPT_BYTES / 1024
        );
    }
    Ok(text)
}

fn slugify_branch(raw: &str) -> String {
    raw.trim()
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect::<String>()
        .split('-')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("-")
}

fn random_branch_name() -> String {
    format!("orchestrator-{}", nebula_core::clock::now_ms())
}

#[cfg(test)]
mod tests;
