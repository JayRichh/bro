//! The board and the attention order, worked out here with nebula-core's
//! rules (`nebula_core::board`, `nebula_core::order`) and handed to the
//! window in the model it already receives, so the window draws what the
//! terminal app would and derives none of it. Nothing here goes over the
//! daemon's socket.

use crate::daemon::{Model, PrInfo};
use nebula_core::board::{attention, lane_of, CardFacts, Lane};
use nebula_core::order::{ordered_worktrees, refill};
use nebula_core::{Agent, AgentRole};
use serde::Serialize;
use std::collections::HashMap;

/// One session's place on the board and in the attention order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Card {
    pub lane: Lane,
    /// 0 needs you, 1 running, 2 unread, 3 the rest.
    pub tier: u8,
}

/// Whether a checkout's pull request is open, as the window reads one: a
/// draft counts; a merged or closed one does not; a source that names no
/// state reports open ones only.
fn pr_open(pr: Option<&PrInfo>) -> bool {
    pr.is_some_and(|pr| pr.state.is_empty() || pr.state == "open")
}

/// Whether a session has a card on the window's board: live, and not the
/// orchestrator, which has a view of its own.
fn on_board(a: &Agent) -> bool {
    !a.archived && a.role != AgentRole::Orchestrator
}

/// Fill `model.cards` and `model.lanes` from the rest of it. Each lane is
/// newest status change first across checkouts, and the user's order among
/// one checkout's cards.
pub fn fill(model: &mut Model) {
    let mut cards = HashMap::with_capacity(model.agents.len());
    let mut lanes: HashMap<Lane, Vec<&Agent>> =
        Lane::ALL.iter().map(|l| (*l, Vec::new())).collect();
    for a in &model.agents {
        let facts = CardFacts {
            pr_open: pr_open(model.prs.get(&a.worktree_id.0)),
        };
        let lane = lane_of(a, &facts);
        cards.insert(
            a.id.0.clone(),
            Card {
                lane,
                tier: attention(a).rank(),
            },
        );
        if on_board(a) {
            lanes.entry(lane).or_default().push(a);
        }
    }
    let ordered = ordered_worktrees(&model.agents);
    model.lanes = lanes
        .into_iter()
        .map(|(lane, mut here)| {
            // Stable, so a tie keeps the model's order, as the window's
            // own sort did.
            here.sort_by_key(|a| std::cmp::Reverse(a.status_changed_at));
            let here = refill(here, &ordered, |a| (&a.worktree_id, a.sort_order));
            (lane, here.into_iter().map(|a| a.id.0.clone()).collect())
        })
        .collect();
    model.cards = cards;
}

#[cfg(test)]
mod tests {
    use super::*;
    use nebula_core::{AgentId, AgentKind, AgentStatus, WorktreeId};

    fn agent(id: &str, wt: &str, status: AgentStatus, unseen: bool, at: i64) -> Agent {
        Agent {
            id: AgentId(id.into()),
            worktree_id: WorktreeId(wt.into()),
            name: id.into(),
            status,
            archived: false,
            archived_at: 0,
            unseen,
            status_changed_at: at,
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

    fn pr(state: &str, draft: bool) -> PrInfo {
        PrInfo {
            number: 1,
            url: "u".into(),
            title: "t".into(),
            draft,
            state: state.into(),
            head_sha: String::new(),
        }
    }

    fn lane(m: &Model, l: Lane) -> Vec<&str> {
        m.lanes[&l].iter().map(String::as_str).collect()
    }

    /// The window reads each session's lane and tier from core's rule; a
    /// pull request counts while open or a draft, or when its source names
    /// no state.
    #[test]
    fn each_session_gets_cores_lane_and_tier() {
        use AgentStatus::*;
        let mut m = Model {
            agents: vec![
                agent("q", "w0", NeedsFeedback, false, 0),
                agent("r", "w0", Running, false, 0),
                agent("u", "w0", Finished, true, 0),
                agent("t", "w0", Terminated, true, 0),
                agent("open", "w1", Finished, false, 0),
                agent("draft", "w2", Finished, false, 0),
                agent("bare", "w3", Finished, false, 0),
                agent("merged", "w4", Finished, false, 0),
                agent("closed", "w5", Finished, false, 0),
                agent("none", "w6", Finished, false, 0),
            ],
            ..Model::default()
        };
        for (w, p) in [
            ("w1", pr("open", false)),
            ("w2", pr("open", true)),
            ("w3", pr("", false)),
            ("w4", pr("merged", false)),
            ("w5", pr("closed", false)),
        ] {
            m.prs.insert(w.into(), p);
        }
        fill(&mut m);
        let card = |id: &str| m.cards[id];
        assert_eq!(
            card("q"),
            Card {
                lane: Lane::NeedsInput,
                tier: 0
            }
        );
        assert_eq!(
            card("r"),
            Card {
                lane: Lane::Running,
                tier: 1
            }
        );
        assert_eq!(
            card("u"),
            Card {
                lane: Lane::Unseen,
                tier: 2
            }
        );
        assert_eq!(
            card("t"),
            Card {
                lane: Lane::Unseen,
                tier: 3
            },
            "stopped is not unread"
        );
        for id in ["open", "draft", "bare"] {
            assert_eq!(card(id).lane, Lane::PrOpen, "{id}");
        }
        for id in ["merged", "closed", "none"] {
            assert_eq!(card(id).lane, Lane::Done, "{id}");
        }
        assert_eq!(
            serde_json::to_value(card("open")).unwrap(),
            serde_json::json!({ "lane": "pr", "tier": 3 })
        );
    }

    /// Newest first across checkouts, the user's order among an ordered
    /// checkout's cards; archived sessions and the orchestrator have none.
    #[test]
    fn lanes_keep_age_across_checkouts_and_the_users_order_within_one() {
        let done = |id, wt, at, order| {
            let mut a = agent(id, wt, AgentStatus::Finished, false, at);
            a.sort_order = order;
            a
        };
        let mut gone = done("gone", "A", 9, 0);
        gone.archived = true;
        let mut orch = done("orch", "B", 9, 0);
        orch.role = AgentRole::Orchestrator;
        let mut m = Model {
            agents: vec![
                done("a1", "A", 4, 2),
                done("b1", "B", 3, 0),
                done("a2", "A", 2, 0),
                done("a3", "A", 1, 1),
                gone,
                orch,
            ],
            ..Model::default()
        };
        fill(&mut m);
        assert_eq!(lane(&m, Lane::Done), ["a2", "b1", "a3", "a1"]);
        assert!(lane(&m, Lane::Running).is_empty());
        assert!(m.cards.contains_key("gone"), "the palette still reads it");
        // Unordered, it is newest first, ties in the model's order.
        for a in &mut m.agents {
            a.sort_order = 0;
        }
        m.agents[1].status_changed_at = 4;
        fill(&mut m);
        assert_eq!(lane(&m, Lane::Done), ["a1", "b1", "a2", "a3"]);
    }
}
