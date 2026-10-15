//! What a session's state means on the board and in the attention order:
//! its KANBAN lane, its attention tier, and what letting its card go on
//! another lane does. Pure functions over the snapshot types, so the
//! terminal app, the daemon and the desktop window (through its Tauri
//! side) read one rule instead of three copies of it.
//!
//! Each is per session and knows nothing of what is on screen: a filter
//! (a [`crate::scope::Scope`], a project) is the caller's, applied before
//! or after; [`split_by_scope`] does the board's. What a lane needs beyond
//! the session's own row comes in [`CardFacts`], so a new lane can take a
//! new fact without every caller's signature changing.

use crate::entities::{Agent, AgentStatus};
use serde::{Deserialize, Serialize};

/// A KANBAN lane. [`Lane::ALL`] is the left-to-right order both apps lay
/// the board out in, and the serde names are the desktop's lane ids.
///
/// The ids predate the review lane and are kept so saved state and the
/// window's code keep reading them: `unseen` is Review (the old Unread
/// lane, which it grew out of), `pr` is Ship (the old PR open lane).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Lane {
    /// Mid-turn (the yellow dot).
    #[serde(rename = "running")]
    Running,
    /// Waiting on you: a permission prompt or a question (the red dot).
    #[serde(rename = "needs")]
    NeedsInput,
    /// Work to look at: a finished turn nobody has read yet (the blue
    /// dot), or a checkout with changes no pull request carries.
    #[serde(rename = "unseen")]
    Review,
    /// Its checkout's pull request is open or a draft: handed on, waiting
    /// on review or the merge.
    #[serde(rename = "pr")]
    Ship,
    /// Read, and nothing left to ship: no changes, or its pull request
    /// merged or closed. A session that never ran is here too.
    #[serde(rename = "done")]
    Done,
}

impl Lane {
    /// The board's columns, left to right.
    pub const ALL: [Lane; 5] = [
        Lane::Running,
        Lane::NeedsInput,
        Lane::Review,
        Lane::Ship,
        Lane::Done,
    ];

    pub fn title(self) -> &'static str {
        match self {
            Lane::Running => "Running",
            Lane::NeedsInput => "Needs you",
            Lane::Review => "Review",
            Lane::Ship => "Ship",
            Lane::Done => "Done",
        }
    }

    /// Its place in [`Lane::ALL`].
    pub fn index(self) -> usize {
        Lane::ALL.iter().position(|l| *l == self).unwrap_or(0)
    }
}

/// Where a checkout's pull request stands, as a card reads it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PrState {
    Open,
    Draft,
    Merged,
    Closed,
}

impl PrState {
    /// From a host's state word (`open`, `merged`, `closed`, any case;
    /// Azure DevOps' `completed` and `abandoned` too) and its draft flag.
    /// A source that names no state reports open pull requests only, so
    /// an empty or unknown word is open.
    pub fn from_state(state: &str, draft: bool) -> PrState {
        match state.to_ascii_lowercase().as_str() {
            "merged" | "completed" => PrState::Merged,
            "closed" | "abandoned" => PrState::Closed,
            _ if draft => PrState::Draft,
            _ => PrState::Open,
        }
    }

    /// Open or a draft: still to land.
    pub fn is_open(self) -> bool {
        matches!(self, PrState::Open | PrState::Draft)
    }
}

/// What the board knows about a session beyond its own row. Each app fills
/// it from what it already holds; nothing here reads git.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CardFacts {
    /// Its checkout has work no push carries: uncommitted or untracked
    /// files, or commits ahead of its upstream. False when unknown.
    pub has_changes: bool,
    /// Its checkout's pull request, when one is known.
    pub pr: Option<PrState>,
}

/// The lane a session's card goes in. One lane per card, the first that
/// applies:
///
/// 1. waiting on you: Needs you; mid-turn: Running;
/// 2. unseen (a turn finished while you were away): Review, whatever its
///    pull request; reading it comes first;
/// 3. a pull request open or a draft: Ship;
/// 4. a merged or closed pull request: Done, changes or not (what is left
///    is the worktree's to clean up, not the board's to review);
/// 5. changes and no pull request: Review, for a session that ran
///    (finished, or stopped since). One that never ran did not make them;
/// 6. else Done.
///
/// The unseen rule takes any unseen session, not only a finished one: the
/// daemon clears `unseen` when a session leaves finished, so in practice
/// the two agree.
pub fn lane_of(agent: &Agent, facts: &CardFacts) -> Lane {
    match agent.status {
        AgentStatus::NeedsFeedback => Lane::NeedsInput,
        AgentStatus::Running => Lane::Running,
        _ if agent.unseen => Lane::Review,
        _ if facts.pr.is_some_and(PrState::is_open) => Lane::Ship,
        _ if facts.pr.is_some() => Lane::Done,
        AgentStatus::Fresh => Lane::Done,
        _ if facts.has_changes => Lane::Review,
        _ => Lane::Done,
    }
}

/// Where a board card is let go: a lane, or the ARCHIVED zone the board
/// shows while a card is held.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DropTarget {
    Lane(Lane),
    Archived,
}

impl DropTarget {
    /// Every place a card can be let go: the lanes in board order, then
    /// Archived.
    pub const ALL: [DropTarget; 6] = [
        DropTarget::Lane(Lane::Running),
        DropTarget::Lane(Lane::NeedsInput),
        DropTarget::Lane(Lane::Review),
        DropTarget::Lane(Lane::Ship),
        DropTarget::Lane(Lane::Done),
        DropTarget::Archived,
    ];

    /// The window's id for it: the lane's serde id, or `archived`.
    pub fn id(self) -> &'static str {
        match self {
            DropTarget::Lane(Lane::Running) => "running",
            DropTarget::Lane(Lane::NeedsInput) => "needs",
            DropTarget::Lane(Lane::Review) => "unseen",
            DropTarget::Lane(Lane::Ship) => "pr",
            DropTarget::Lane(Lane::Done) => "done",
            DropTarget::Archived => "archived",
        }
    }
}

/// What letting a card go on a [`DropTarget`] does. The apps carry it out;
/// the merge, which changes something on the host, is confirmed first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(tag = "act", content = "reason", rename_all = "snake_case")]
pub enum DropVerb {
    /// Its own lane: beside a card of its own checkout, a reorder;
    /// anywhere else on it, nothing.
    Reorder,
    /// Review to Done: the session is marked seen.
    MarkSeen,
    /// Review to Ship: the pull request composer opens for its checkout.
    OpenPr,
    /// Ship to Done: its pull request merges, once the user confirms.
    MergePr,
    /// Onto Archived: the session is archived.
    Archive,
    /// Done onto Archived: archived, and the checkout's safe delete (the
    /// one that refuses unpushed work) is offered.
    ArchiveAndOfferWorktreeDelete,
    /// A move the user can't make, and why.
    Refuse(&'static str),
}

/// What letting `agent`'s card (in the lane [`lane_of`] gives it with
/// `facts`) go on `to` does. Only moves the user can cause act; a lane
/// the agent or the pull request owns refuses with the reason.
pub fn drop_verb(agent: &Agent, facts: &CardFacts, to: DropTarget) -> DropVerb {
    let from = lane_of(agent, facts);
    let to = match to {
        DropTarget::Archived if from == Lane::Done => {
            return DropVerb::ArchiveAndOfferWorktreeDelete
        }
        DropTarget::Archived => return DropVerb::Archive,
        DropTarget::Lane(l) => l,
    };
    if from == to {
        return DropVerb::Reorder;
    }
    match (from, to) {
        (_, Lane::NeedsInput) => {
            DropVerb::Refuse("Only the agent puts a card in Needs you, by asking.")
        }
        (_, Lane::Running) => DropVerb::Refuse(
            "Only the agent puts a card in Running, by starting a turn: send it a follow-up.",
        ),
        (Lane::NeedsInput, _) => DropVerb::Refuse(
            "Needs you is the agent\u{2019}s: the card leaves it when you answer the session.",
        ),
        (Lane::Running, _) => DropVerb::Refuse(
            "Running is the agent\u{2019}s: the card moves on when the turn ends.",
        ),
        // Read already: its changes hold it in Review, and marking it seen
        // would leave it where it is.
        (Lane::Review, Lane::Done) if !agent.unseen => DropVerb::Refuse(
            "Its worktree has changes no pull request carries: commit and push them, ship them (drop on Ship) or archive the card.",
        ),
        (Lane::Review, Lane::Done) => DropVerb::MarkSeen,
        (Lane::Review, Lane::Ship) if !facts.has_changes => {
            DropVerb::Refuse("Nothing to ship: its worktree has no changes or unpushed commits.")
        }
        (Lane::Review, Lane::Ship) => DropVerb::OpenPr,
        (Lane::Ship, Lane::Done) if facts.pr == Some(PrState::Draft) => DropVerb::Refuse(
            "Its pull request is a draft: mark it ready for review before it can merge.",
        ),
        (Lane::Ship, Lane::Done) => DropVerb::MergePr,
        (Lane::Ship, Lane::Review) => DropVerb::Refuse(
            "Ship follows the worktree\u{2019}s pull request: the card leaves it when the PR merges or closes.",
        ),
        (Lane::Done, Lane::Review) => DropVerb::Refuse(
            "Review comes from a turn finishing or from new changes; a card cannot be sent back to it.",
        ),
        (Lane::Done, _) => DropVerb::Refuse(
            "Nothing to ship from Done: its worktree has no changes, or its pull request already landed.",
        ),
        // Review onto Review and Ship onto Ship: `from == to`, returned
        // above.
        (Lane::Review | Lane::Ship, _) => DropVerb::Reorder,
    }
}

/// The board split by a scope: each lane's admitted cards, and how many
/// more the scope leaves out there ("2 more outside scope").
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Scoped<T> {
    /// Lane by lane in [`Lane::ALL`] order, each in the order given.
    pub lanes: Vec<Vec<T>>,
    /// The cards each lane would hold but the scope leaves out, in
    /// [`Lane::ALL`] order.
    pub hidden: [usize; Lane::ALL.len()],
}

/// Sort `cards` into lanes by `lane`, keeping those `admitted` takes and
/// counting the rest lane by lane. Order within a lane is the input's.
pub fn split_by_scope<T>(
    cards: impl IntoIterator<Item = T>,
    lane: impl Fn(&T) -> Lane,
    admitted: impl Fn(&T) -> bool,
) -> Scoped<T> {
    let mut out = Scoped {
        lanes: Lane::ALL.iter().map(|_| Vec::new()).collect(),
        hidden: [0; Lane::ALL.len()],
    };
    for card in cards {
        let i = lane(&card).index();
        if admitted(&card) {
            out.lanes[i].push(card);
        } else {
            out.hidden[i] += 1;
        }
    }
    out
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

    fn facts(has_changes: bool, pr: Option<PrState>) -> CardFacts {
        CardFacts { has_changes, pr }
    }

    /// Every client reads a session's lane and tier off this one table.
    #[test]
    fn each_state_has_one_lane_and_one_tier() {
        use AgentStatus::*;
        use Attention as T;
        use Lane as L;
        use PrState::*;
        // (status, unseen, changes, pr) → (lane, tier)
        let table = [
            // The agent's lanes win over everything.
            (
                NeedsFeedback,
                false,
                false,
                None,
                L::NeedsInput,
                T::NeedsFeedback,
            ),
            (
                NeedsFeedback,
                true,
                true,
                Some(Open),
                L::NeedsInput,
                T::NeedsFeedback,
            ),
            (Running, false, false, None, L::Running, T::Running),
            (Running, true, true, Some(Open), L::Running, T::Running),
            // Unseen is Review, whatever its pull request.
            (Finished, true, false, None, L::Review, T::Unseen),
            (Finished, true, false, Some(Open), L::Review, T::Unseen),
            (Finished, true, true, Some(Merged), L::Review, T::Unseen),
            // Read: changes and no PR is Review; an open PR or draft Ship.
            (Finished, false, true, None, L::Review, T::Rest),
            (Finished, false, true, Some(Open), L::Ship, T::Rest),
            (Finished, false, false, Some(Draft), L::Ship, T::Rest),
            // A landed PR is Done, changes or not.
            (Finished, false, true, Some(Merged), L::Done, T::Rest),
            (Finished, false, true, Some(Closed), L::Done, T::Rest),
            (Finished, false, false, Some(Closed), L::Done, T::Rest),
            (Finished, false, false, None, L::Done, T::Rest),
            // Never ran: its checkout's changes are not its work.
            (Fresh, false, false, None, L::Done, T::Rest),
            (Fresh, false, true, None, L::Done, T::Rest),
            (Fresh, false, false, Some(Open), L::Ship, T::Rest),
            // Stopped after running: its changes stand.
            (Terminated, false, true, None, L::Review, T::Rest),
            (Disconnected, false, true, None, L::Review, T::Rest),
            (Terminated, false, false, None, L::Done, T::Rest),
            (Disconnected, false, false, Some(Draft), L::Ship, T::Rest),
            // A stopped session still flagged unseen: the card stays in
            // Review, but nothing finished, so it is not unread.
            (Terminated, true, false, None, L::Review, T::Rest),
            (Disconnected, true, false, Some(Open), L::Review, T::Rest),
        ];
        for (status, unseen, changes, pr, lane, tier) in table {
            let a = agent(status, unseen);
            assert_eq!(
                lane_of(&a, &facts(changes, pr)),
                lane,
                "{status:?} unseen={unseen} changes={changes} pr={pr:?}"
            );
            assert_eq!(attention(&a), tier, "{status:?} unseen={unseen}");
            assert_eq!(is_unread(&a), tier == T::Unseen);
        }
        assert_eq!(
            [T::NeedsFeedback, T::Running, T::Unseen, T::Rest].map(T::rank),
            [0, 1, 2, 3]
        );
    }

    /// The desktop names lanes by these ids, the old ones kept; the order
    /// is the board's.
    #[test]
    fn lanes_serialize_as_the_desktop_ids_in_board_order() {
        let ids: Vec<String> = Lane::ALL
            .iter()
            .map(|l| serde_json::to_string(l).unwrap())
            .collect();
        assert_eq!(
            ids,
            [
                "\"running\"",
                "\"needs\"",
                "\"unseen\"",
                "\"pr\"",
                "\"done\""
            ]
        );
        assert_eq!(
            Lane::ALL.map(Lane::title),
            ["Running", "Needs you", "Review", "Ship", "Done"]
        );
        assert_eq!(Lane::Done.index(), 4);
        let back: Lane = serde_json::from_str("\"unseen\"").unwrap();
        assert_eq!(back, Lane::Review);
        for t in DropTarget::ALL {
            if let DropTarget::Lane(l) = t {
                assert_eq!(
                    format!("\"{}\"", t.id()),
                    serde_json::to_string(&l).unwrap()
                );
            }
        }
    }

    #[test]
    fn a_hosts_state_word_reads_as_a_pr_state() {
        use PrState::*;
        assert_eq!(PrState::from_state("open", false), Open);
        assert_eq!(PrState::from_state("OPEN", true), Draft);
        assert_eq!(PrState::from_state("", false), Open);
        assert_eq!(PrState::from_state("merged", true), Merged);
        assert_eq!(PrState::from_state("completed", false), Merged);
        assert_eq!(PrState::from_state("closed", false), Closed);
        assert_eq!(PrState::from_state("abandoned", false), Closed);
        assert!(Open.is_open() && Draft.is_open());
        assert!(!Merged.is_open() && !Closed.is_open());
    }

    /// Every lane onto every target, for one card in each lane.
    #[test]
    fn each_drop_has_one_verb() {
        use AgentStatus::*;
        use DropVerb as V;
        use PrState::*;
        let refused = |v: DropVerb| matches!(v, V::Refuse(_));
        // A card in each lane, and the facts that put it there.
        let running = (agent(Running, false), facts(true, None));
        let needs = (agent(NeedsFeedback, false), facts(true, None));
        let unread = (agent(Finished, true), facts(false, None));
        let unread_changes = (agent(Finished, true), facts(true, None));
        let changes = (agent(Finished, false), facts(true, None));
        let open = (agent(Finished, false), facts(true, Some(Open)));
        let draft = (agent(Finished, false), facts(true, Some(Draft)));
        let done = (agent(Finished, false), facts(false, None));
        let merged = (agent(Finished, false), facts(true, Some(Merged)));
        let lane = |t: &(Agent, CardFacts)| lane_of(&t.0, &t.1);
        assert_eq!(
            [&running, &needs, &unread, &changes, &open, &done].map(lane),
            [
                Lane::Running,
                Lane::NeedsInput,
                Lane::Review,
                Lane::Review,
                Lane::Ship,
                Lane::Done
            ]
        );
        let verb = |t: &(Agent, CardFacts), to: DropTarget| drop_verb(&t.0, &t.1, to);
        let to = DropTarget::Lane;
        // (card, target) → the verb; None: refused.
        let table: [(&(Agent, CardFacts), DropTarget, Option<DropVerb>); 20] = [
            // Its own lane reorders; Archived archives from anywhere.
            (&running, to(Lane::Running), Some(V::Reorder)),
            (&open, to(Lane::Ship), Some(V::Reorder)),
            (&running, DropTarget::Archived, Some(V::Archive)),
            (&needs, DropTarget::Archived, Some(V::Archive)),
            (&changes, DropTarget::Archived, Some(V::Archive)),
            (&open, DropTarget::Archived, Some(V::Archive)),
            // Done to Archived offers the safe worktree delete too.
            (
                &done,
                DropTarget::Archived,
                Some(V::ArchiveAndOfferWorktreeDelete),
            ),
            (
                &merged,
                DropTarget::Archived,
                Some(V::ArchiveAndOfferWorktreeDelete),
            ),
            // Review: to Done marks an unread card seen; to Ship opens the
            // composer when there is something to ship.
            (&unread, to(Lane::Done), Some(V::MarkSeen)),
            (&unread_changes, to(Lane::Done), Some(V::MarkSeen)),
            (&changes, to(Lane::Done), None),
            (&changes, to(Lane::Ship), Some(V::OpenPr)),
            (&unread_changes, to(Lane::Ship), Some(V::OpenPr)),
            (&unread, to(Lane::Ship), None),
            // Ship: to Done merges an open PR; a draft can't merge.
            (&open, to(Lane::Done), Some(V::MergePr)),
            (&draft, to(Lane::Done), None),
            (&open, to(Lane::Review), None),
            // Done goes nowhere but Archived.
            (&done, to(Lane::Review), None),
            (&done, to(Lane::Ship), None),
            (&merged, to(Lane::Ship), None),
        ];
        for (i, (card, target, want)) in table.into_iter().enumerate() {
            let got = verb(card, target);
            match want {
                Some(v) => assert_eq!(got, v, "row {i}"),
                None => assert!(refused(got), "row {i}: {got:?}"),
            }
        }
        // Nothing drops into the agent's lanes, and nothing leaves them
        // but to Archived.
        for card in [&running, &needs, &unread, &changes, &open, &done] {
            for l in [Lane::Running, Lane::NeedsInput] {
                if lane(card) != l {
                    assert!(refused(verb(card, to(l))), "{:?} onto {l:?}", lane(card));
                }
            }
        }
        for card in [&running, &needs] {
            for l in [Lane::Review, Lane::Ship, Lane::Done] {
                assert!(refused(verb(card, to(l))), "{:?} onto {l:?}", lane(card));
            }
        }
        // Every refusal says why.
        for card in [&running, &needs, &unread, &changes, &open, &draft, &done] {
            for t in DropTarget::ALL {
                if let V::Refuse(why) = verb(card, t) {
                    assert!(!why.is_empty());
                }
            }
        }
        assert_eq!(
            serde_json::to_value(V::Refuse("no")).unwrap(),
            serde_json::json!({ "act": "refuse", "reason": "no" })
        );
        assert_eq!(
            serde_json::to_value(V::ArchiveAndOfferWorktreeDelete).unwrap(),
            serde_json::json!({ "act": "archive_and_offer_worktree_delete" })
        );
    }

    #[test]
    fn a_scope_splits_the_board_and_counts_what_it_hides() {
        // (lane, admitted)
        let cards = [
            (Lane::Review, true),
            (Lane::Review, false),
            (Lane::Review, false),
            (Lane::Running, true),
            (Lane::Done, false),
            (Lane::Review, true),
        ];
        let s = split_by_scope(cards.iter().enumerate(), |c| c.1 .0, |c| c.1 .1);
        let ids = |l: Lane| s.lanes[l.index()].iter().map(|c| c.0).collect::<Vec<_>>();
        assert_eq!(ids(Lane::Review), [0, 5]);
        assert_eq!(ids(Lane::Running), [3]);
        assert!(ids(Lane::Done).is_empty());
        assert_eq!(s.hidden, [0, 0, 2, 0, 1]);
    }
}
