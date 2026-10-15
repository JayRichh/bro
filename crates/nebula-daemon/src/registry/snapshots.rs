//! SESSION SNAPSHOTS (`nebula_core::session_snapshot`): the checkout as a
//! session found it when it started, and at the start and end of each of
//! its turns, so the review can show one session's changes in a worktree
//! several share.
//!
//! Snapshots run git (an add into a temp index, a write-tree, a merge-tree
//! at a turn end), so they go to one worker thread, in order, never on a
//! hook path or under a daemon lock. The session start is taken when its
//! PTY is installed and only when it has none in its worktree yet, so a
//! respawn, a resume, a restart or a prewarm adoption keeps the original.
//! A turn starts when the status goes to running (from anything else: a
//! prompt, a permission granted) and ends at finished or needs feedback,
//! or with the process (terminated); each end folds the turn into the session's base and tip
//! (the core module doc has the rule) and prunes turns past
//! [`KEEP_TURNS`].
//!
//! A turn's start snapshot waits behind whatever is queued ahead of it, so
//! in a busy worktree it can land after the agent's first edits, which
//! then drop out of the session's changes (the core module doc's one
//! too-little case).
//!
//! Odd edges, each resolved toward showing too much rather than too
//! little:
//! - a start while a turn is still open (a restart or a death between)
//!   ends the open one there;
//! - an end with no turn ever seen (a turn running across the upgrade
//!   from 0.51) is a turn from the session's tip, or its start, to now;
//! - any other end with no open turn, and a death with none, is ignored.
//!
//! The refs live under this data dir's daemon id
//! (`refs/kete/d/<id>/sessions/<agent>/…`) and go with the session's row,
//! its worktree's or its project's. At startup, once a day after, and in
//! a project's repo when it is added or removed, a sweep moves this
//! daemon's sessions' 0.51 refs into its namespace and deletes refs in its own
//! namespace for sessions it no longer has (a row lost to a crash). It
//! never deletes a ref outside its namespace by absence: another daemon
//! (a test's, WSL's beside Windows') keeps its own in the same repo.

use super::*;
use nebula_core::session_snapshot::{self as snap, SessionChanges, Slot, SnapshotPoint};
use std::sync::mpsc;

/// Turns kept per session (rows and refs); older ones are already folded
/// into the base and tip. A handful under test, so pruning is reached.
#[cfg(not(test))]
pub(crate) const KEEP_TURNS: u32 = 50;
#[cfg(test)]
pub(crate) const KEEP_TURNS: u32 = 3;

/// What a snapshot marks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Edge {
    /// The session's PTY is in: its start, unless it has one.
    SessionStart,
    /// The status went to running.
    TurnStart,
    /// The status went to finished or needs feedback.
    TurnEnd,
    /// The process died while working (terminated): ends an open turn,
    /// never makes one.
    Died,
}

enum Job {
    Take {
        agent: AgentId,
        edge: Edge,
    },
    /// Delete these sessions' refs in the repo at `repo`.
    DropRefs {
        repo: PathBuf,
        agents: Vec<String>,
    },
    /// Adopt 0.51 refs and sweep orphans in the repo at `repo`, or in
    /// every project's repo.
    Sweep {
        repo: Option<PathBuf>,
    },
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
                    let daemon_id = match store.daemon_id() {
                        Ok(id) => id,
                        Err(error) => {
                            tracing::warn!(%error, "no daemon id: session snapshots off");
                            return;
                        }
                    };
                    for job in rx {
                        run_job(&store, &daemon_id, job);
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
            edge: Edge::SessionStart,
        });
    }

    /// A session's status went to running: a turn starts.
    pub(super) fn snapshot_turn_start(&self, agent: &AgentId) {
        self.snapshot_job(Job::Take {
            agent: agent.clone(),
            edge: Edge::TurnStart,
        });
    }

    /// A session's turn ended; `died`: with its process.
    pub(super) fn snapshot_turn_end(&self, agent: &AgentId, died: bool) {
        self.snapshot_job(Job::Take {
            agent: agent.clone(),
            edge: if died { Edge::Died } else { Edge::TurnEnd },
        });
    }

    /// Delete `agents`' snapshot refs in the repo at `repo`, in the worker.
    pub(super) fn drop_snapshot_refs(&self, repo: PathBuf, agents: Vec<String>) {
        if !agents.is_empty() {
            self.snapshot_job(Job::DropRefs { repo, agents });
        }
    }

    /// Adopt this daemon's 0.51 refs and sweep its orphaned ones in every
    /// project's repo, in the worker: at startup and once a day after.
    pub fn sweep_snapshot_refs(&self) {
        self.snapshot_job(Job::Sweep { repo: None });
    }

    /// [`Self::sweep_snapshot_refs`] in the repo at `repo` alone: a
    /// project just added, or just removed (its sessions gone with it, so
    /// every ref this daemon left there is an orphan).
    pub(super) fn sweep_snapshot_refs_in(&self, repo: PathBuf) {
        self.snapshot_job(Job::Sweep { repo: Some(repo) });
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

    /// `ClientRequest::SessionChanges`: the session's base and tip as the
    /// two trees to diff (a 0.51 session not yet through a 0.52 turn: its
    /// start and last), and how many sessions in its worktree have some.
    pub fn session_changes(&self, agent: &AgentId) -> Result<SessionChanges> {
        let row = self.store.get_agent(agent)?.context("session not found")?;
        let snaps = self.store.session_snapshots(agent)?;
        let worktree = snaps
            .as_ref()
            .map(|s| s.worktree.clone())
            .unwrap_or(row.worktree_id);
        let sessions = self.store.snapshot_sessions_in(&worktree)?;
        let (start, last) = match snaps {
            Some(s) => match (s.base, s.tip, s.last) {
                (Some(base), Some(tip), Some(last)) => (
                    Some(SnapshotPoint {
                        tree: Some(base),
                        ..s.start
                    }),
                    Some(SnapshotPoint {
                        tree: Some(tip),
                        ..last
                    }),
                ),
                (_, _, last) => (Some(s.start), last),
            },
            None => (None, None),
        };
        Ok(SessionChanges {
            agent: agent.clone(),
            worktree: Some(worktree),
            start,
            last,
            sessions,
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

fn run_job(store: &Store, daemon_id: &str, job: Job) {
    match job {
        Job::Take { agent, edge } => {
            if let Err(error) = take(store, daemon_id, &agent, edge) {
                tracing::debug!(%error, agent = %agent, "session snapshot failed");
            }
        }
        Job::DropRefs { repo, agents } => {
            if repo.is_dir() && !snap::drop_refs(&repo, daemon_id, &agents) {
                tracing::debug!(repo = %repo.display(), "dropping session refs failed");
            }
        }
        Job::Sweep { repo } => {
            if let Err(error) = sweep(store, daemon_id, repo.as_deref()) {
                tracing::debug!(%error, "session ref sweep failed");
            }
        }
        #[cfg(test)]
        Job::Flush(done) => {
            let _ = done.send(());
        }
    }
}

/// One `for-each-ref` and at most one `update-ref` per repo
/// (`session_snapshot::tidy`). `only`: that repo, whether or not a
/// project still has it.
fn sweep(store: &Store, daemon_id: &str, only: Option<&Path>) -> Result<()> {
    let live = store.agent_ids()?;
    let mut repos = match only {
        Some(repo) => vec![repo.to_path_buf()],
        None => store.project_repos()?,
    };
    repos.sort();
    repos.dedup();
    for repo in repos.iter().filter(|r| r.is_dir()) {
        let done = snap::tidy(repo, daemon_id, &live);
        if !done.adopted.is_empty() {
            tracing::info!(repo = %repo.display(), count = done.adopted.len(), "adopted 0.51 session refs");
        }
        if !done.swept.is_empty() {
            tracing::info!(repo = %repo.display(), count = done.swept.len(), "swept orphaned session refs");
        }
    }
    Ok(())
}

fn take(store: &Store, daemon_id: &str, id: &AgentId, edge: Edge) -> Result<()> {
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
    let repo = worktree.path.as_path();
    let held = store.session_snapshots(id)?;
    // No start in this worktree (a new session, one from before snapshots,
    // or one that moved): whatever this is, it is the start too.
    let fresh = held.as_ref().is_none_or(|s| s.worktree != worktree.id);
    if edge == Edge::SessionStart && !fresh {
        return Ok(());
    }
    let keep = |slot: Slot, tree: &Option<String>| {
        if let Some(tree) = tree {
            snap::keep(repo, daemon_id, id.as_str(), slot, tree);
        }
    };
    if held.is_some() {
        // Its 0.51 refs, if any, into this daemon's namespace (moved
        // sessions: the old worktree's trees go below anyway).
        snap::adopt_legacy(repo, daemon_id, &[id.as_str().to_string()]);
    }
    let point = snap::take(repo);
    if fresh {
        if held.is_some() {
            snap::drop_refs(repo, daemon_id, &[id.as_str().to_string()]);
        }
        store.set_snapshot_start(id, &worktree.id, &point)?;
        keep(Slot::Start, &point.tree);
    }
    match edge {
        Edge::SessionStart => {}
        Edge::TurnStart => {
            if let Some(open) = store.open_turn(id)? {
                end_turn(store, daemon_id, repo, id, open, &point)?;
            }
            let n = store.begin_turn(id, &point)?;
            keep(Slot::TurnStart(n), &point.tree);
        }
        Edge::TurnEnd | Edge::Died => {
            let open = match store.open_turn(id)? {
                Some(open) => open,
                // A death after a turn ended, or an end already taken.
                None if edge == Edge::Died || !store.session_turns(id)?.is_empty() => return Ok(()),
                None => {
                    // No start seen: from where the session last stood.
                    let snaps = store.session_snapshots(id)?;
                    let from = snaps
                        .as_ref()
                        .and_then(|s| s.tip.clone().or_else(|| s.start.tree.clone()));
                    let at = snaps.map_or(point.at, |s| s.last.map_or(s.start.at, |l| l.at));
                    let from = SnapshotPoint {
                        head: None,
                        tree: from,
                        at,
                    };
                    let n = store.begin_turn(id, &from)?;
                    keep(Slot::TurnStart(n), &from.tree);
                    store
                        .open_turn(id)?
                        .context("the turn just opened is not there")?
                }
            };
            end_turn(store, daemon_id, repo, id, open, &point)?;
        }
    }
    Ok(())
}

/// End turn `open` at `point` and fold it into the session's base and tip.
fn end_turn(
    store: &Store,
    daemon_id: &str,
    repo: &Path,
    id: &AgentId,
    open: crate::store::SessionTurn,
    point: &SnapshotPoint,
) -> Result<()> {
    store.end_turn(id, open.n, point)?;
    store.set_snapshot_last(id, point)?;
    if let Some(tree) = &point.tree {
        snap::keep(repo, daemon_id, id.as_str(), Slot::Last, tree);
        snap::keep(repo, daemon_id, id.as_str(), Slot::TurnEnd(open.n), tree);
    }
    // Unavailable trees never fold a gap away, so nothing of the session's
    // can drop out: an end that could not be taken clears the tip (the
    // review says unavailable, and the next gap is not folded); a start
    // that could not be taken leaves the base (the gap counts as the
    // session's).
    let snaps = store.session_snapshots(id)?;
    let (base, tip) = snaps.map_or((None, None), |s| (s.base, s.tip));
    let folded = match (base, tip, &open.start_tree) {
        (None, _, start) => start.clone(),
        (Some(base), Some(tip), Some(start)) if &tip != start => {
            Some(snap::fold_gap(repo, &tip, &base, start).unwrap_or(base))
        }
        (Some(base), _, _) => Some(base),
    };
    if let Some(base) = folded {
        store.set_snapshot_view(id, &base, point.tree.as_deref())?;
        snap::keep(repo, daemon_id, id.as_str(), Slot::Base, &base);
        if let Some(end) = &point.tree {
            snap::keep(repo, daemon_id, id.as_str(), Slot::Tip, end);
        }
    }
    let gone = store.prune_turns(id, KEEP_TURNS)?;
    if !gone.is_empty() {
        snap::drop_turns(repo, daemon_id, id.as_str(), &gone);
    }
    Ok(())
}
