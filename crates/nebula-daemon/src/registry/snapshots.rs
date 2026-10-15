//! SESSION SNAPSHOTS (`nebula_core::session_snapshot`): the checkout as a
//! session found it when it started, and as each of its turns left it, so
//! the review can show one session's changes in a worktree several share.
//!
//! Snapshots run git (an add into a temp index, a write-tree), so they go
//! to one worker thread, in order, never on a hook path or under a daemon
//! lock. The start one is taken when the session's PTY is installed and
//! only when it has none in its worktree yet, so a respawn, a resume, a
//! restart or a prewarm adoption keeps the original; the last one at every
//! turn end (finished, needs feedback). The refs that keep the trees from
//! `git gc` go with the session's row, its worktree's or its project's, and
//! any a crash left behind are pruned at startup.

use super::*;
use nebula_core::session_snapshot::{self as snap, SessionChanges, Which};
use std::sync::mpsc;

enum Job {
    /// Take `agent`'s start snapshot if it has none in its worktree, or,
    /// at a turn end (`boundary`), its last one.
    Take { agent: AgentId, boundary: bool },
    /// Delete these sessions' refs in the repo at `repo`.
    DropRefs { repo: PathBuf, agents: Vec<String> },
    /// Delete every session ref with no snapshot row, in every project.
    Prune,
    /// Answer once every job sent before it is done.
    #[cfg(test)]
    Flush(mpsc::Sender<()>),
}

/// The snapshot worker: started on its first job, ended when the daemon
/// (and with it the sender) goes.
#[derive(Default)]
pub(crate) struct Snapshotter {
    tx: Mutex<Option<mpsc::Sender<Job>>>,
}

impl Daemon {
    fn snapshot_job(&self, job: Job) {
        let mut tx = self.snapshots.tx.lock();
        let sender = tx.get_or_insert_with(|| {
            let (tx, rx) = mpsc::channel::<Job>();
            let store = self.store.clone();
            let spawned = std::thread::Builder::new()
                .name("kete-snapshots".into())
                .spawn(move || {
                    for job in rx {
                        run_job(&store, job);
                    }
                });
            if let Err(error) = spawned {
                tracing::warn!(%error, "session snapshot worker did not start");
            }
            tx
        });
        if sender.send(job).is_err() {
            *tx = None;
        }
    }

    /// A session's PTY is in: its start snapshot, unless it has one.
    pub(super) fn snapshot_session_start(&self, agent: &AgentId) {
        self.snapshot_job(Job::Take {
            agent: agent.clone(),
            boundary: false,
        });
    }

    /// A session's turn ended: its last snapshot.
    pub(super) fn snapshot_turn_end(&self, agent: &AgentId) {
        self.snapshot_job(Job::Take {
            agent: agent.clone(),
            boundary: true,
        });
    }

    /// Delete `agents`' snapshot refs in the repo at `repo`, in the worker.
    pub(super) fn drop_snapshot_refs(&self, repo: PathBuf, agents: Vec<String>) {
        if !agents.is_empty() {
            self.snapshot_job(Job::DropRefs { repo, agents });
        }
    }

    /// The sessions under `project` or `worktree` that have snapshots,
    /// with the repo their refs are in: read before a delete takes the rows.
    pub(super) fn snapshot_refs_under(
        &self,
        project: &ProjectId,
        worktree: Option<&WorktreeId>,
    ) -> Option<(PathBuf, Vec<String>)> {
        let repo = self.store.get_project(project).ok()??.repo_path;
        let scope = match worktree {
            Some(_) => None,
            None => Some(project),
        };
        let agents = self.store.snapshot_agents_under(scope, worktree).ok()?;
        (!agents.is_empty()).then_some((repo, agents))
    }

    /// Delete session refs a crash or an older build left with no row.
    pub fn prune_session_refs(&self) {
        self.snapshot_job(Job::Prune);
    }

    /// `ClientRequest::SessionChanges`: the session's snapshots, and how
    /// many sessions in its worktree have some.
    pub fn session_changes(&self, agent: &AgentId) -> Result<SessionChanges> {
        let row = self.store.get_agent(agent)?.context("session not found")?;
        let snaps = self.store.session_snapshots(agent)?;
        let worktree = snaps
            .as_ref()
            .map(|s| s.worktree.clone())
            .unwrap_or(row.worktree_id);
        let sessions = self.store.snapshot_sessions_in(&worktree)?;
        Ok(match snaps {
            Some(s) => SessionChanges {
                agent: agent.clone(),
                worktree: Some(worktree),
                start: Some(s.start),
                last: s.last,
                sessions,
            },
            None => SessionChanges {
                agent: agent.clone(),
                worktree: Some(worktree),
                start: None,
                last: None,
                sessions,
            },
        })
    }

    /// Wait for every snapshot job queued so far.
    #[cfg(test)]
    pub(crate) fn settle_snapshots(&self) {
        let (tx, rx) = mpsc::channel();
        self.snapshot_job(Job::Flush(tx));
        let _ = rx.recv_timeout(std::time::Duration::from_secs(60));
    }
}

fn run_job(store: &Store, job: Job) {
    match job {
        Job::Take { agent, boundary } => {
            if let Err(error) = take(store, &agent, boundary) {
                tracing::debug!(%error, agent = %agent, "session snapshot failed");
            }
        }
        Job::DropRefs { repo, agents } => {
            if repo.is_dir() && !snap::drop_refs(&repo, &agents) {
                tracing::debug!(repo = %repo.display(), "dropping session refs failed");
            }
        }
        Job::Prune => {
            if let Err(error) = prune(store) {
                tracing::debug!(%error, "pruning session refs failed");
            }
        }
        #[cfg(test)]
        Job::Flush(done) => {
            let _ = done.send(());
        }
    }
}

fn take(store: &Store, id: &AgentId, boundary: bool) -> Result<()> {
    let Some(agent) = store.get_agent(id)? else {
        return Ok(());
    };
    // A Cloud row's work happens in the cloud sandbox, not in this checkout.
    if agent.cloud_session_id.is_some() {
        return Ok(());
    }
    let Some(worktree) = store.get_worktree(&agent.worktree_id)? else {
        return Ok(());
    };
    if !worktree.path.is_dir() {
        return Ok(());
    }
    let held = store.session_snapshots(id)?;
    // No start in this worktree (a new session, one from before snapshots,
    // or one that moved): whatever this is, it is the start.
    let fresh = held.as_ref().is_none_or(|s| s.worktree != worktree.id);
    if !boundary && !fresh {
        return Ok(());
    }
    let point = snap::take(&worktree.path);
    let which = if fresh { Which::Start } else { Which::Last };
    if let Some(tree) = &point.tree {
        snap::keep(&worktree.path, id.as_str(), which, tree);
    }
    if fresh {
        store.set_snapshot_start(id, &worktree.id, &point)?;
    } else {
        store.set_snapshot_last(id, &point)?;
    }
    Ok(())
}

fn prune(store: &Store) -> Result<()> {
    let kept: std::collections::HashSet<String> = store.snapshot_agents()?.into_iter().collect();
    let (projects, _, _, _) = store.load_tree()?;
    for project in projects {
        if !project.repo_path.is_dir() {
            continue;
        }
        let stale: Vec<String> = snap::kept_agents(&project.repo_path)
            .into_iter()
            .filter(|id| !kept.contains(id))
            .collect();
        if !stale.is_empty() {
            snap::drop_refs(&project.repo_path, &stale);
        }
    }
    Ok(())
}
