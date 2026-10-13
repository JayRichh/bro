//! Claude's in-session subagents (the Task / Agent tool), as the
//! SubagentStart and SubagentStop hooks report them, for clients that draw
//! a session's agent tree. Held in daemon memory and carried across an
//! IN-PLACE RESTART in the restart state (`handoff.rs`), so a subagent
//! still running there keeps running and its SubagentStop still lands. A
//! cold start begins empty: every session died with the old daemon.

use super::*;
use nebula_core::{SubagentStatus, SubagentSummary};
use serde::{Deserialize, Serialize};
use std::collections::{HashSet, VecDeque};

/// Ended subagents kept per parent session: a long session's tree stays
/// bounded. Shared with clients, which prune the same way.
const ENDED_KEPT: usize = nebula_core::SUBAGENTS_ENDED_KEPT;
/// Task descriptions waiting for their subagent's SubagentStart, per
/// parent. More than this many unclaimed means the hooks have drifted.
const PENDING_TASKS_KEPT: usize = 16;
/// The tool a parent calls to start a subagent (`Task` in older Claude
/// Code, `Agent` since).
const SUBAGENT_TOOLS: [&str; 2] = ["Agent", "Task"];

/// What a hook POST says about subagents, beside its status event.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SubagentHookInfo {
    /// The payload's `agent_type` (`Explore`, `general-purpose`, ...).
    pub kind: Option<String>,
    /// The `description` of a Task / Agent tool call's input.
    pub task: Option<String>,
    /// The `subagent_type` of that input: the `agent_type` its
    /// SubagentStart will report.
    pub task_kind: Option<String>,
}

/// A Task / Agent call's description, waiting for its SubagentStart.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PendingTask {
    /// The `subagent_type` asked for; empty when the call named none.
    pub kind: String,
    pub description: String,
}

/// The book as the restart state carries it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SubagentCarry {
    #[serde(default)]
    pub rows: Vec<SubagentSummary>,
    #[serde(default)]
    pub pending: Vec<(AgentId, PendingTask)>,
}

#[derive(Default)]
pub(crate) struct SubagentBook {
    rows: HashMap<AgentId, Vec<SubagentSummary>>,
    pending_tasks: HashMap<AgentId, VecDeque<PendingTask>>,
}

impl SubagentBook {
    fn note_task(&mut self, parent: &AgentId, task: PendingTask) {
        let queue = self.pending_tasks.entry(parent.clone()).or_default();
        queue.push_back(task);
        while queue.len() > PENDING_TASKS_KEPT {
            queue.pop_front();
        }
    }

    fn start(&mut self, parent: &AgentId, id: String, kind: String, now: i64) -> SubagentSummary {
        // The oldest unclaimed call that asked for this subagent's type is
        // its own; failing a match (a call that named no type, a CLI that
        // reports none), calls start in the order they were made.
        let description = self
            .pending_tasks
            .get_mut(parent)
            .and_then(|queue| {
                let typed = (!kind.is_empty())
                    .then(|| queue.iter().position(|t| t.kind == kind))
                    .flatten();
                match typed {
                    Some(at) => queue.remove(at),
                    None => queue.pop_front(),
                }
            })
            .map(|t| t.description)
            .unwrap_or_default();
        let row = SubagentSummary {
            id,
            parent_agent: parent.clone(),
            kind,
            description,
            started_at: now,
            ended_at: None,
            status: SubagentStatus::Running,
        };
        let rows = self.rows.entry(parent.clone()).or_default();
        rows.retain(|r| r.id != row.id);
        rows.push(row.clone());
        row
    }

    fn end(
        &mut self,
        parent: &AgentId,
        id: Option<&str>,
        status: SubagentStatus,
        now: i64,
    ) -> Vec<SubagentSummary> {
        let Some(rows) = self.rows.get_mut(parent) else {
            return Vec::new();
        };
        let mut changed = Vec::new();
        for row in rows.iter_mut() {
            let named = id.is_none_or(|id| row.id == id);
            if named && row.status == SubagentStatus::Running {
                row.status = status;
                row.ended_at = Some(now);
                changed.push(row.clone());
            }
        }
        let ended = rows
            .iter()
            .filter(|r| r.status != SubagentStatus::Running)
            .count();
        let mut drop = ended.saturating_sub(ENDED_KEPT);
        rows.retain(|r| {
            if drop > 0 && r.status != SubagentStatus::Running {
                drop -= 1;
                return false;
            }
            true
        });
        changed
    }

    fn forget(&mut self, parent: &AgentId) {
        self.rows.remove(parent);
        self.pending_tasks.remove(parent);
    }

    fn all(&self) -> Vec<SubagentSummary> {
        let mut all: Vec<_> = self.rows.values().flatten().cloned().collect();
        all.sort_by(|a, b| {
            (a.started_at, &a.parent_agent.0, &a.id).cmp(&(b.started_at, &b.parent_agent.0, &b.id))
        });
        all
    }
}

impl Daemon {
    /// Every subagent the daemon knows of, oldest start first: the
    /// Snapshot's `subagents`.
    pub fn subagent_rows(&self) -> Vec<SubagentSummary> {
        self.subagents.lock().all()
    }

    /// Feed one hook delivery's subagent news: a Task / Agent tool call's
    /// description, a SubagentStart, a SubagentStop. Everything else is
    /// not this book's business.
    pub fn track_subagent(&self, parent: &AgentId, event: &HookEvent, info: SubagentHookInfo) {
        let now = nebula_core::clock::now_ms();
        let changed = {
            let mut book = self.subagents.lock();
            match event {
                HookEvent::PreToolUse {
                    tool_name: Some(tool),
                    subagent_id: None,
                } if SUBAGENT_TOOLS.contains(&tool.as_str()) => {
                    book.note_task(
                        parent,
                        PendingTask {
                            kind: info.task_kind.unwrap_or_default(),
                            description: info.task.unwrap_or_default(),
                        },
                    );
                    Vec::new()
                }
                HookEvent::SubagentStart {
                    subagent_id: Some(id),
                } => vec![book.start(parent, id.clone(), info.kind.unwrap_or_default(), now)],
                HookEvent::SubagentStop {
                    subagent_id: Some(id),
                } => book.end(parent, Some(id), SubagentStatus::Done, now),
                _ => Vec::new(),
            }
        };
        for sub in changed {
            self.broadcast(ServerEvent::SubagentChanged { sub });
        }
    }

    /// The parent session's process is gone: whatever it still had running
    /// can never report its end, so it ends as unknown.
    pub(crate) fn close_subagents(&self, parent: &AgentId) {
        let now = nebula_core::clock::now_ms();
        let changed = {
            let mut book = self.subagents.lock();
            book.pending_tasks.remove(parent);
            book.end(parent, None, SubagentStatus::EndedUnknown, now)
        };
        for sub in changed {
            self.broadcast(ServerEvent::SubagentChanged { sub });
        }
    }

    /// The parent row is deleted: drop its subagents with it.
    pub(crate) fn forget_subagents(&self, parent: &AgentId) {
        self.subagents.lock().forget(parent);
    }

    /// The whole book, for an IN-PLACE RESTART to hand the next image.
    pub fn carry_subagents(&self) -> SubagentCarry {
        let book = self.subagents.lock();
        SubagentCarry {
            rows: book.all(),
            pending: book
                .pending_tasks
                .iter()
                .flat_map(|(parent, queue)| queue.iter().map(|t| (parent.clone(), t.clone())))
                .collect(),
        }
    }

    /// Take back what the old image carried. A subagent whose parent's
    /// session came across (`live`) keeps running, and its SubagentStop
    /// will find it. One whose parent did not, has lost the process it ran
    /// in, so it ends as unknown, as a parent's exit would have ended it.
    /// Called while the daemon boots, before any client subscribes: the
    /// first Snapshot carries the result, so nothing is broadcast.
    pub fn restore_subagents(&self, carry: SubagentCarry, live: &HashSet<AgentId>) {
        let now = nebula_core::clock::now_ms();
        let mut book = self.subagents.lock();
        for mut row in carry.rows {
            if row.status == SubagentStatus::Running && !live.contains(&row.parent_agent) {
                row.status = SubagentStatus::EndedUnknown;
                row.ended_at = Some(now);
            }
            let rows = book.rows.entry(row.parent_agent.clone()).or_default();
            rows.retain(|r| r.id != row.id);
            rows.push(row);
        }
        for (parent, task) in carry.pending {
            if live.contains(&parent) {
                book.note_task(&parent, task);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn daemon() -> Arc<Daemon> {
        Daemon::new(
            Arc::new(Store::open_in_memory().unwrap()),
            HookEnv {
                port: 0,
                token: String::new(),
            },
        )
    }

    fn changes(rx: &mut broadcast::Receiver<ServerEvent>) -> Vec<SubagentSummary> {
        std::iter::from_fn(|| rx.try_recv().ok())
            .filter_map(|e| match e {
                ServerEvent::SubagentChanged { sub } => Some(sub),
                _ => None,
            })
            .collect()
    }

    fn start(id: &str) -> HookEvent {
        HookEvent::SubagentStart {
            subagent_id: Some(id.into()),
        }
    }

    fn stop(id: &str) -> HookEvent {
        HookEvent::SubagentStop {
            subagent_id: Some(id.into()),
        }
    }

    fn explore() -> SubagentHookInfo {
        SubagentHookInfo {
            kind: Some("Explore".into()),
            task: None,
            task_kind: None,
        }
    }

    #[test]
    fn a_start_then_a_stop_reads_running_then_done() {
        let daemon = daemon();
        let parent = AgentId("p".into());
        let mut rx = daemon.events.subscribe();

        // The parent's Agent tool call names the task before the start.
        daemon.track_subagent(
            &parent,
            &HookEvent::PreToolUse {
                tool_name: Some("Agent".into()),
                subagent_id: None,
            },
            SubagentHookInfo {
                kind: None,
                task: Some("Find the hook fixtures".into()),
                task_kind: Some("Explore".into()),
            },
        );
        assert!(
            changes(&mut rx).is_empty(),
            "a tool call alone starts nothing"
        );

        daemon.track_subagent(&parent, &start("agent-1"), explore());
        let started = changes(&mut rx);
        assert_eq!(started.len(), 1);
        assert_eq!(started[0].status, SubagentStatus::Running);
        assert_eq!(started[0].id, "agent-1");
        assert_eq!(started[0].parent_agent, parent);
        assert_eq!(started[0].kind, "Explore");
        assert_eq!(started[0].description, "Find the hook fixtures");
        assert_eq!(started[0].ended_at, None);
        assert_eq!(daemon.subagent_rows(), started);

        daemon.track_subagent(&parent, &stop("agent-1"), explore());
        let stopped = changes(&mut rx);
        assert_eq!(stopped.len(), 1);
        assert_eq!(stopped[0].status, SubagentStatus::Done);
        assert!(stopped[0].ended_at.is_some());
        assert_eq!(daemon.subagent_rows(), stopped);

        // A stop for a subagent never seen starting changes nothing.
        daemon.track_subagent(&parent, &stop("ghost"), explore());
        assert!(changes(&mut rx).is_empty());
    }

    #[test]
    fn the_parent_exiting_ends_its_open_subagents_as_unknown() {
        let daemon = daemon();
        let parent = AgentId("p".into());
        let other = AgentId("o".into());
        daemon.track_subagent(&parent, &start("a"), explore());
        daemon.track_subagent(&parent, &start("b"), explore());
        daemon.track_subagent(&parent, &stop("b"), explore());
        daemon.track_subagent(&other, &start("c"), explore());
        let mut rx = daemon.events.subscribe();

        daemon.close_subagents(&parent);

        let closed = changes(&mut rx);
        assert_eq!(closed.len(), 1, "only the open one is closed");
        assert_eq!(closed[0].id, "a");
        assert_eq!(closed[0].status, SubagentStatus::EndedUnknown);
        assert!(closed[0].ended_at.is_some());
        let rows = daemon.subagent_rows();
        let status = |id: &str| rows.iter().find(|r| r.id == id).unwrap().status;
        assert_eq!(
            status("b"),
            SubagentStatus::Done,
            "a finished one stays done"
        );
        assert_eq!(
            status("c"),
            SubagentStatus::Running,
            "another session's are untouched"
        );

        daemon.forget_subagents(&parent);
        assert_eq!(daemon.subagent_rows().len(), 1);
    }

    #[test]
    fn a_long_session_keeps_only_its_newest_ended_subagents() {
        let daemon = daemon();
        let parent = AgentId("p".into());
        daemon.track_subagent(&parent, &start("open"), explore());
        for i in 0..ENDED_KEPT + 5 {
            let id = format!("s{i}");
            daemon.track_subagent(&parent, &start(&id), explore());
            daemon.track_subagent(&parent, &stop(&id), explore());
        }
        let rows = daemon.subagent_rows();
        assert_eq!(rows.len(), ENDED_KEPT + 1);
        assert!(rows.iter().any(|r| r.id == "open"), "running ones are kept");
        assert!(!rows.iter().any(|r| r.id == "s0"), "the oldest ended go");
    }

    fn launch(kind: Option<&str>, task: &str) -> (HookEvent, SubagentHookInfo) {
        (
            HookEvent::PreToolUse {
                tool_name: Some("Agent".into()),
                subagent_id: None,
            },
            SubagentHookInfo {
                kind: None,
                task: Some(task.into()),
                task_kind: kind.map(String::from),
            },
        )
    }

    fn typed(kind: &str) -> SubagentHookInfo {
        SubagentHookInfo {
            kind: Some(kind.into()),
            task: None,
            task_kind: None,
        }
    }

    /// Parallel calls can start out of order: a start takes the oldest
    /// waiting call that asked for its type, and falls back to the oldest
    /// call of all when none did.
    #[test]
    fn a_start_takes_the_task_that_asked_for_its_type() {
        let daemon = daemon();
        let parent = AgentId("p".into());
        for (kind, task) in [
            (Some("Plan"), "Plan the migration"),
            (Some("Explore"), "Find the callers"),
            (None, "Anything at all"),
        ] {
            let (event, info) = launch(kind, task);
            daemon.track_subagent(&parent, &event, info);
        }
        daemon.track_subagent(&parent, &start("e"), typed("Explore"));
        daemon.track_subagent(&parent, &start("x"), typed("general-purpose"));
        daemon.track_subagent(&parent, &start("p"), typed("Plan"));
        let rows = daemon.subagent_rows();
        let task = |id: &str| {
            rows.iter()
                .find(|r| r.id == id)
                .unwrap()
                .description
                .clone()
        };
        assert_eq!(task("e"), "Find the callers");
        assert_eq!(task("x"), "Plan the migration", "no match: oldest first");
        assert_eq!(task("p"), "Anything at all");
    }

    /// IN-PLACE RESTART: the book rides the restart state (encoded as the
    /// state file is, `to_vec_named`). A carried session's subagent keeps
    /// running and its later Stop lands; one whose session did not come
    /// across ends as unknown; finished ones stay as they were.
    #[test]
    fn the_book_survives_an_in_place_restart() {
        let old = daemon();
        let carried = AgentId("carried".into());
        let lost = AgentId("lost".into());
        old.track_subagent(&carried, &start("a"), explore());
        old.track_subagent(&carried, &start("b"), explore());
        old.track_subagent(&carried, &stop("b"), explore());
        old.track_subagent(&lost, &start("c"), explore());
        // A call made just before the restart, its subagent not started.
        let (event, info) = launch(Some("Plan"), "Plan the next step");
        old.track_subagent(&carried, &event, info);

        let bytes = rmp_serde::to_vec_named(&old.carry_subagents()).unwrap();
        let carry: SubagentCarry = rmp_serde::from_slice(&bytes).unwrap();
        let new = daemon();
        new.restore_subagents(carry, &[carried.clone()].into_iter().collect());

        let rows = new.subagent_rows();
        let status = |id: &str| rows.iter().find(|r| r.id == id).unwrap().status;
        assert_eq!(status("a"), SubagentStatus::Running);
        assert_eq!(status("b"), SubagentStatus::Done);
        assert_eq!(status("c"), SubagentStatus::EndedUnknown);
        assert!(rows
            .iter()
            .find(|r| r.id == "c")
            .unwrap()
            .ended_at
            .is_some());

        let mut rx = new.events.subscribe();
        new.track_subagent(&carried, &stop("a"), explore());
        let stopped = changes(&mut rx);
        assert_eq!(stopped.len(), 1, "the carried subagent's Stop lands");
        assert_eq!(stopped[0].status, SubagentStatus::Done);

        // The waiting task came across with its session.
        new.track_subagent(&carried, &start("d"), typed("Plan"));
        let rows = new.subagent_rows();
        let d = rows.iter().find(|r| r.id == "d").unwrap();
        assert_eq!(d.description, "Plan the next step");
    }
}
