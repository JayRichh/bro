//! The board and the attention order, worked out here with nebula-core's
//! rules (`nebula_core::board`, `nebula_core::order`) and handed to the
//! window in the model it already receives, so the window draws what the
//! terminal app would and derives none of it. Nothing here goes over the
//! daemon's socket.

use crate::daemon::{Client, Model, PrInfo};
use nebula_core::board::{
    attention, drop_verb, lane_of, split_by_scope, CardFacts, DropTarget, DropVerb, Lane, PrState,
};
use nebula_core::order::{ordered_worktrees, refill};
use nebula_core::scope::Scope;
use nebula_core::{Agent, AgentRole, ProjectId};
use serde::Serialize;
use std::collections::HashMap;
use std::sync::Arc;
use tauri::State;

/// One session's place on the board and in the attention order.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Card {
    pub lane: Lane,
    /// 0 needs you, 1 running, 2 unread, 3 the rest.
    pub tier: u8,
    /// The window's scope admits it: what the attention strip counts and
    /// the board shows. Archived sessions are read the same way.
    pub in_scope: bool,
    /// What letting the card go on each lane (by its id) or on
    /// `archived` does: core's [`drop_verb`], so the window mirrors none
    /// of the rule.
    pub drops: HashMap<&'static str, DropVerb>,
}

/// A checkout's pull request as a card reads it.
fn pr_state(pr: Option<&PrInfo>) -> Option<PrState> {
    pr.map(|pr| PrState::from_state(&pr.state, pr.draft))
}

/// Whether a session has a card on the window's board: live, and not the
/// orchestrator, which has a view of its own.
fn on_board(a: &Agent) -> bool {
    !a.archived && a.role != AgentRole::Orchestrator
}

/// Fill `model.cards`, `model.lanes` and `model.hidden` from the rest of
/// it, the lanes narrowed to `model.scope`. Each lane is newest status
/// change first across checkouts, and the user's order among one
/// checkout's cards.
pub fn fill(model: &mut Model) {
    let project_of: HashMap<&str, &ProjectId> = model
        .worktrees
        .iter()
        .map(|w| (w.id.0.as_str(), &w.project_id))
        .collect();
    let mut cards = HashMap::with_capacity(model.agents.len());
    let mut board: Vec<(&Agent, Lane, bool)> = Vec::new();
    for a in &model.agents {
        let wt = a.worktree_id.0.as_str();
        let facts = CardFacts {
            pr: pr_state(model.prs.get(wt)),
            ..model.worktree_changes.get(wt).copied().unwrap_or_default()
        };
        let lane = lane_of(a, &facts);
        let in_scope = model.scope.admits(a, project_of.get(wt).copied());
        cards.insert(
            a.id.0.clone(),
            Card {
                lane,
                tier: attention(a).rank(),
                in_scope,
                drops: DropTarget::ALL
                    .iter()
                    .map(|t| (t.id(), drop_verb(a, &facts, *t)))
                    .collect(),
            },
        );
        if on_board(a) {
            board.push((a, lane, in_scope));
        }
    }
    let scoped = split_by_scope(board, |c| c.1, |c| c.2);
    let ordered = ordered_worktrees(&model.agents);
    model.lanes = Lane::ALL
        .iter()
        .zip(scoped.lanes)
        .map(|(lane, here)| {
            let mut here: Vec<&Agent> = here.into_iter().map(|c| c.0).collect();
            // Stable, so a tie keeps the model's order, as the window's
            // own sort did.
            here.sort_by_key(|a| std::cmp::Reverse(a.status_changed_at));
            let here = refill(here, &ordered, |a| (&a.worktree_id, a.sort_order));
            (*lane, here.into_iter().map(|a| a.id.0.clone()).collect())
        })
        .collect();
    model.hidden = Lane::ALL.iter().copied().zip(scoped.hidden).collect();
    model.cards = cards;
}

/// What the window is looking at: the board's lanes and every card's
/// `in_scope` follow it. The default (`{}`) is everything.
#[tauri::command]
pub fn board_scope(client: State<'_, Arc<Client>>, scope: Scope) {
    client.set_scope(scope);
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
        let card = |id: &str| (m.cards[id].lane, m.cards[id].tier);
        assert_eq!(card("q"), (Lane::NeedsInput, 0));
        assert_eq!(card("r"), (Lane::Running, 1));
        assert_eq!(card("u"), (Lane::Review, 2));
        assert_eq!(card("t"), (Lane::Review, 3), "stopped is not unread");
        for id in ["open", "draft", "bare"] {
            assert_eq!(card(id).0, Lane::Ship, "{id}");
        }
        for id in ["merged", "closed", "none"] {
            assert_eq!(card(id).0, Lane::Done, "{id}");
        }
        let v = serde_json::to_value(&m.cards["open"]).unwrap();
        assert_eq!(v["lane"], "pr");
        assert_eq!(v["tier"], 3);
        assert_eq!(v["in_scope"], true);
        // Each target's verb is core's: a Ship card merges onto Done.
        assert_eq!(v["drops"]["done"], serde_json::json!({ "act": "merge_pr" }));
        assert_eq!(v["drops"]["pr"], serde_json::json!({ "act": "reorder" }));
        assert_eq!(v["drops"]["archived"]["act"], "archive");
        assert_eq!(v["drops"]["needs"]["act"], "refuse");
        assert_eq!(m.cards["none"].drops.len(), DropTarget::ALL.len());
    }

    /// A checkout `worktree_git` last found with work no push carries puts
    /// a read session that ran in Review; one that never ran stays Done.
    #[test]
    fn changes_from_the_last_git_read_put_a_card_in_review() {
        use AgentStatus::*;
        let mut m = Model {
            agents: vec![
                agent("dirty", "w1", Finished, false, 0),
                agent("fresh", "w1", Fresh, false, 0),
                agent("clean", "w2", Finished, false, 0),
                agent("shipped", "w3", Finished, false, 0),
                agent("root", "w4", Finished, false, 0),
            ],
            ..Model::default()
        };
        let git = |changed, ahead_of_base, on_default_branch| {
            crate::git::WorktreeGit {
                changed,
                ahead_of_base,
                on_default_branch,
                ..Default::default()
            }
            .card_facts()
        };
        // w1: committed since the base, never pushed, tree clean.
        m.worktree_changes.insert("w1".into(), git(0, 2, false));
        m.worktree_changes.insert("w2".into(), git(0, 0, false));
        m.worktree_changes.insert("w3".into(), git(3, 0, false));
        // w4: the root checkout, dirty, on the base branch.
        m.worktree_changes.insert("w4".into(), git(1, 0, true));
        m.prs.insert("w3".into(), pr("merged", false));
        fill(&mut m);
        assert_eq!(m.cards["dirty"].lane, Lane::Review);
        assert_eq!(m.cards["dirty"].drops["pr"], DropVerb::OpenPr);
        assert_eq!(m.cards["fresh"].lane, Lane::Done);
        assert_eq!(m.cards["clean"].lane, Lane::Done);
        assert_eq!(m.cards["shipped"].lane, Lane::Done);
        assert_eq!(m.cards["root"].lane, Lane::Review);
        assert!(
            matches!(m.cards["root"].drops["pr"], DropVerb::Refuse(why) if why.contains("base branch"))
        );
        assert_eq!(
            m.cards["clean"].drops["archived"],
            DropVerb::ArchiveAndOfferWorktreeDelete
        );
    }

    /// The scope narrows the lanes and counts the rest lane by lane; every
    /// card still has its place, marked in or out of scope.
    #[test]
    fn the_scope_narrows_the_lanes_and_counts_what_it_hides() {
        use nebula_core::Worktree;
        use AgentStatus::*;
        let wt = |id: &str, project: &str| Worktree {
            id: WorktreeId(id.into()),
            project_id: ProjectId(project.into()),
            path: std::path::PathBuf::from(id),
            branch: id.into(),
            is_main: false,
            sort_order: 0,
        };
        let mut m = Model {
            worktrees: vec![wt("w1", "p"), wt("w2", "p"), wt("w3", "q")],
            agents: vec![
                agent("a", "w1", Running, false, 0),
                agent("b", "w2", Running, false, 0),
                agent("c", "w3", Running, false, 0),
                agent("d", "w3", Finished, true, 0),
                agent("e", "w1", Finished, true, 0),
            ],
            ..Model::default()
        };
        fill(&mut m);
        assert_eq!(m.hidden[&Lane::Running], 0, "the default is everything");
        assert_eq!(lane(&m, Lane::Running).len(), 3);
        m.scope = Scope {
            repo: Some(ProjectId("p".into())),
            ..Scope::default()
        };
        fill(&mut m);
        let mut running = lane(&m, Lane::Running);
        running.sort();
        assert_eq!(running, ["a", "b"]);
        assert_eq!(lane(&m, Lane::Review), ["e"]);
        assert_eq!(m.hidden[&Lane::Running], 1);
        assert_eq!(m.hidden[&Lane::Review], 1);
        assert_eq!(m.hidden[&Lane::Done], 0);
        assert!(!m.cards["c"].in_scope && m.cards["a"].in_scope);
        m.scope = Scope {
            worktree: Some(WorktreeId("w1".into())),
            ..Scope::default()
        };
        fill(&mut m);
        assert_eq!(lane(&m, Lane::Running), ["a"]);
        assert_eq!(m.hidden[&Lane::Running], 2);
        let v = serde_json::to_value(&m).unwrap();
        assert_eq!(v["hidden"]["running"], 2);
        assert_eq!(v["scope"]["worktree"], "w1");
        assert!(
            v.get("worktree_changes").is_none(),
            "the window never sees it"
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
        // The window gets it as JSON: lane ids as keys, every lane present.
        let v = serde_json::to_value(&m).unwrap();
        assert_eq!(
            v["lanes"]["done"],
            serde_json::json!(["a2", "b1", "a3", "a1"])
        );
        assert_eq!(v["lanes"]["pr"], serde_json::json!([]));
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
