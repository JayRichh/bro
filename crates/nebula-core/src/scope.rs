//! What the bench is looking at: a SCOPE narrows the board, the attention
//! strip, the bands, the quick prompt and the diff to one repo, one
//! worktree, one session or one account, or any mix of them. Pure, over
//! the snapshot types, so both apps read one rule.
//!
//! An `Agent` row names its worktree but not its project, so the repo test
//! takes the project the caller looked up for that worktree.

use crate::entities::Agent;
use crate::ids::{AgentId, ProjectId, WorktreeId};
use serde::{Deserialize, Serialize};

/// Each set field must match; an unset one matches anything, so the
/// default scope admits every session.
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Scope {
    #[serde(default)]
    pub repo: Option<ProjectId>,
    #[serde(default)]
    pub worktree: Option<WorktreeId>,
    #[serde(default)]
    pub session: Option<AgentId>,
    /// An account key as [`crate::accounts::session_account`] gives it: a
    /// profile's name, or `@claude` for a harness's own login.
    #[serde(default)]
    pub account: Option<String>,
}

impl Scope {
    /// Nothing narrowed: what a cleared scope chip leaves.
    pub fn is_everything(&self) -> bool {
        self == &Scope::default()
    }

    /// Does `agent` belong in this scope? `project` is the project of the
    /// agent's worktree, None when the caller could not find it (which
    /// only a scope with no repo admits).
    pub fn admits(&self, agent: &Agent, project: Option<&ProjectId>) -> bool {
        self.admits_worktree(&agent.worktree_id, project)
            && self.session.as_ref().is_none_or(|s| s == &agent.id)
            && self
                .account
                .as_deref()
                .is_none_or(|a| a == crate::accounts::session_account(agent))
    }

    /// Does the worktree `worktree` (in `project`) belong in this scope,
    /// session and account aside? For what has no session row: a band, a
    /// diff, a worktree with nobody in it.
    pub fn admits_worktree(&self, worktree: &WorktreeId, project: Option<&ProjectId>) -> bool {
        self.repo.as_ref().is_none_or(|r| Some(r) == project)
            && self.worktree.as_ref().is_none_or(|w| w == worktree)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entities::{AgentKind, AgentRole, AgentStatus};

    fn agent(id: &str, worktree: &str, profile: Option<&str>) -> Agent {
        Agent {
            id: AgentId(id.into()),
            worktree_id: WorktreeId(worktree.into()),
            name: id.into(),
            status: AgentStatus::Finished,
            archived: false,
            archived_at: 0,
            unseen: false,
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
            profile: profile.map(str::to_string),
            parent: None,
        }
    }

    fn scope(
        repo: Option<&str>,
        wt: Option<&str>,
        session: Option<&str>,
        acct: Option<&str>,
    ) -> Scope {
        Scope {
            repo: repo.map(|r| ProjectId(r.into())),
            worktree: wt.map(|w| WorktreeId(w.into())),
            session: session.map(|s| AgentId(s.into())),
            account: acct.map(str::to_string),
        }
    }

    /// One table for every field alone and together.
    #[test]
    fn admits_reads_each_set_field_and_ignores_the_rest() {
        let work = agent("a1", "w1", Some("work"));
        let own = agent("a2", "w2", None);
        let p = ProjectId("p".into());
        let q = ProjectId("q".into());
        // (scope, agent, its project, admitted)
        let table = [
            (scope(None, None, None, None), &work, Some(&p), true),
            (scope(None, None, None, None), &work, None, true),
            (scope(Some("p"), None, None, None), &work, Some(&p), true),
            (scope(Some("p"), None, None, None), &own, Some(&q), false),
            (scope(Some("p"), None, None, None), &work, None, false),
            (scope(None, Some("w1"), None, None), &work, Some(&p), true),
            (scope(None, Some("w1"), None, None), &own, Some(&p), false),
            (scope(None, None, Some("a2"), None), &own, Some(&q), true),
            (scope(None, None, Some("a2"), None), &work, Some(&p), false),
            (scope(None, None, None, Some("work")), &work, Some(&p), true),
            (scope(None, None, None, Some("work")), &own, Some(&p), false),
            // A harness's own login is its `@harness` key, never a blank.
            (
                scope(None, None, None, Some("@claude")),
                &own,
                Some(&p),
                true,
            ),
            (
                scope(None, None, None, Some("@claude")),
                &work,
                Some(&p),
                false,
            ),
            (
                scope(Some("p"), Some("w1"), Some("a1"), Some("work")),
                &work,
                Some(&p),
                true,
            ),
            (
                scope(Some("q"), Some("w1"), Some("a1"), Some("work")),
                &work,
                Some(&p),
                false,
            ),
            (
                scope(Some("p"), Some("w1"), Some("a1"), Some("home")),
                &work,
                Some(&p),
                false,
            ),
        ];
        for (i, (s, a, project, want)) in table.into_iter().enumerate() {
            assert_eq!(s.admits(a, project), want, "row {i}: {s:?} {}", a.id.0);
        }
    }

    #[test]
    fn a_worktree_is_admitted_session_and_account_aside() {
        let p = ProjectId("p".into());
        let w = WorktreeId("w1".into());
        let s = scope(Some("p"), None, Some("a9"), Some("work"));
        assert!(s.admits_worktree(&w, Some(&p)));
        assert!(!s.admits_worktree(&w, None));
        assert!(!scope(None, Some("w2"), None, None).admits_worktree(&w, Some(&p)));
    }

    #[test]
    fn the_default_is_everything_and_round_trips_as_camel_case() {
        assert!(Scope::default().is_everything());
        let s = scope(Some("p"), None, None, Some("@codex"));
        assert!(!s.is_everything());
        let json = serde_json::to_value(&s).unwrap();
        assert_eq!(json["repo"], "p");
        assert_eq!(json["account"], "@codex");
        let back: Scope = serde_json::from_str(r#"{"worktree":"w"}"#).unwrap();
        assert_eq!(back, scope(None, Some("w"), None, None));
    }
}
