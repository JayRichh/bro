//! What a session's state means on the board and in the attention order:
//! its KANBAN lane and its attention tier. Pure functions over the snapshot
//! types, so the terminal app, the daemon and the desktop window (through
//! its Tauri side) read one rule instead of three copies of it.
//!
//! Each is per session and knows nothing of what is on screen: a filter
//! (a project, a worktree, an account) is the caller's, applied before or
//! after. What a lane needs beyond the session's own row comes in
//! [`CardFacts`], so a new lane can take a new fact without every caller's
//! signature changing.

use crate::entities::{Agent, AgentStatus};
use serde::{Deserialize, Serialize};

/// A KANBAN lane. [`Lane::ALL`] is the left-to-right order both apps lay
/// the board out in, and the serde names are the desktop's lane ids.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Lane {
    /// Waiting on you: a permission prompt or a question (the red dot).
    #[serde(rename = "needs")]
    NeedsInput,
    /// Mid-turn (the yellow dot).
    #[serde(rename = "running")]
    Running,
    /// A finished turn nobody has read yet (the blue dot).
    #[serde(rename = "unseen")]
    Unseen,
    /// Read, idle, or never run: nothing for you to do.
    #[serde(rename = "done")]
    Done,
    /// Idle and read, its checkout's pull request still open: work handed
    /// on to review rather than finished.
    #[serde(rename = "pr")]
    PrOpen,
}

impl Lane {
    /// The board's columns, left to right.
    pub const ALL: [Lane; 5] = [
        Lane::NeedsInput,
        Lane::Running,
        Lane::Unseen,
        Lane::Done,
        Lane::PrOpen,
    ];

    pub fn title(self) -> &'static str {
        match self {
            Lane::NeedsInput => "Needs you",
            Lane::Running => "Running",
            Lane::Unseen => "Unread",
            Lane::Done => "Done",
            Lane::PrOpen => "PR open",
        }
    }

    /// Its place in [`Lane::ALL`].
    pub fn index(self) -> usize {
        Lane::ALL.iter().position(|l| *l == self).unwrap_or(0)
    }
}

/// What the board knows about a session beyond its own row.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CardFacts {
    /// Its checkout's pull request is open (a draft counts; merged and
    /// closed do not).
    pub pr_open: bool,
}

/// The lane a session's card goes in. One lane per card, the most urgent
/// that applies: waiting on you over working over unread, and an open pull
/// request only once nothing else about the session needs you — a session
/// asking a question on a branch under review is asking a question first.
///
/// The Unread lane takes any unseen session, not only a finished one: the
/// daemon clears `unseen` when a session leaves finished, so in practice
/// the two agree.
pub fn lane_of(agent: &Agent, facts: &CardFacts) -> Lane {
    match agent.status {
        AgentStatus::NeedsFeedback => Lane::NeedsInput,
        AgentStatus::Running => Lane::Running,
        _ if agent.unseen => Lane::Unseen,
        _ if facts.pr_open => Lane::PrOpen,
        _ => Lane::Done,
    }
}

/// Where a session sorts when the question is "what wants me first": the
/// `/` palette before a query, `.` and `,`, and the orchestrator's triage.
/// Ordered most urgent first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Attention {
    NeedsFeedback,
    Running,
    /// Finished a turn nobody has read ([`is_unread`]).
    Unseen,
    /// Everything else: read, never run, stopped.
    Rest,
}

impl Attention {
    /// 0 for the most urgent up to 3 for the rest: the number the desktop
    /// palette sorts by.
    pub fn rank(self) -> u8 {
        self as u8
    }
}

/// Finished a turn that no client has looked at: what the DONE BADGE and
/// the orchestrator's `unread` state count. A stopped session that is
/// somehow still flagged unseen is not unread.
pub fn is_unread(agent: &Agent) -> bool {
    agent.status == AgentStatus::Finished && agent.unseen
}

/// A session's attention tier: its own status, read against [`is_unread`].
pub fn attention(agent: &Agent) -> Attention {
    match agent.status {
        AgentStatus::NeedsFeedback => Attention::NeedsFeedback,
        AgentStatus::Running => Attention::Running,
        _ if is_unread(agent) => Attention::Unseen,
        _ => Attention::Rest,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entities::{AgentKind, AgentRole};
    use crate::ids::{AgentId, WorktreeId};

    fn agent(status: AgentStatus, unseen: bool) -> Agent {
        Agent {
            id: AgentId("a".into()),
            worktree_id: WorktreeId("w".into()),
            name: "a".into(),
            status,
            archived: false,
            archived_at: 0,
            unseen,
            status_changed_at: 0,
            kind: AgentKind::Claude,
            custom_harness: None,
            model: None,
            effort: None,
            session_id: None,
            cloud_session_id: None,
            issue_url: None,
            role: AgentRole::Worker,
            sort_order: 0,
            alive: true,
            recent_prompts: Vec::new(),
            profile: None,
            parent: None,
        }
    }

    /// Every client reads a session's lane and tier off this one table.
    #[test]
    fn each_state_has_one_lane_and_one_tier() {
        use AgentStatus::*;
        use Attention as T;
        // (status, unseen, pr open) → (lane, tier)
        let table = [
            (
                NeedsFeedback,
                false,
                false,
                Lane::NeedsInput,
                T::NeedsFeedback,
            ),
            (
                NeedsFeedback,
                true,
                true,
                Lane::NeedsInput,
                T::NeedsFeedback,
            ),
            (Running, false, false, Lane::Running, T::Running),
            (Running, true, true, Lane::Running, T::Running),
            (Finished, true, false, Lane::Unseen, T::Unseen),
            (Finished, true, true, Lane::Unseen, T::Unseen),
            (Finished, false, true, Lane::PrOpen, T::Rest),
            (Finished, false, false, Lane::Done, T::Rest),
            (Fresh, false, false, Lane::Done, T::Rest),
            (Fresh, false, true, Lane::PrOpen, T::Rest),
            // A stopped session still flagged unseen: the card stays in
            // Unread, but nothing finished, so it is not unread.
            (Terminated, true, false, Lane::Unseen, T::Rest),
            (Disconnected, true, true, Lane::Unseen, T::Rest),
            (Terminated, false, false, Lane::Done, T::Rest),
            (Disconnected, false, true, Lane::PrOpen, T::Rest),
        ];
        for (status, unseen, pr_open, lane, tier) in table {
            let a = agent(status, unseen);
            let facts = CardFacts { pr_open };
            assert_eq!(
                lane_of(&a, &facts),
                lane,
                "{status:?} unseen={unseen} pr={pr_open}"
            );
            assert_eq!(attention(&a), tier, "{status:?} unseen={unseen}");
            assert_eq!(is_unread(&a), tier == T::Unseen);
        }
        assert_eq!(
            [T::NeedsFeedback, T::Running, T::Unseen, T::Rest].map(T::rank),
            [0, 1, 2, 3]
        );
    }

    /// The desktop names lanes by these ids; the order is the board's.
    #[test]
    fn lanes_serialize_as_the_desktop_ids_in_board_order() {
        let ids: Vec<String> = Lane::ALL
            .iter()
            .map(|l| serde_json::to_string(l).unwrap())
            .collect();
        assert_eq!(
            ids,
            [
                "\"needs\"",
                "\"running\"",
                "\"unseen\"",
                "\"done\"",
                "\"pr\""
            ]
        );
        assert_eq!(Lane::PrOpen.index(), 4);
    }
}
