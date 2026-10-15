//! Ship for the window: a worktree's pull request, opened and merged on
//! whichever forge its remote is, through `kete_ado::ship` (the code
//! `kete ship` runs too). Errors come back as `ShipError`
//! (`{ kind, forge, message, url }`), so a missing sign-in is a button.

use crate::ado::ship::{
    self, Created, MergeOpts, Merged, NewPr, Place, Ship, ShipError, ShipErrorKind, ShipStatus,
    Suggestion,
};
use crate::ado::AdoState;
use crate::daemon::Client;
use nebula_core::{Agent, AgentId, WorktreeId};
use serde::Serialize;
use std::sync::Arc;
use tauri::State;

type ShipRes<T> = Result<T, ShipError>;

/// [`ShipStatus`] and, for a branch with no open PR yet, what the
/// composer starts from.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ShipView {
    #[serde(flatten)]
    pub status: ShipStatus,
    pub suggestion: Option<Suggestion>,
}

/// The checkout behind a daemon worktree id, with the base setting the
/// diff reads.
fn place(client: &Client, worktree: &str) -> ShipRes<Place> {
    let path = crate::ado::checkout_path(client, worktree)
        .map_err(|e| ShipError::new(ShipErrorKind::Other, None, e))?;
    Ok(Place::new(
        path,
        nebula_config::Config::load().worktree_base_branch,
    ))
}

/// The session a suggestion is written from: `session` when named, else
/// the worktree's most recently active live row. Its name and last prompt.
pub fn session_words(
    agents: &[Agent],
    worktree: &str,
    session: Option<&str>,
) -> (Option<String>, Option<String>) {
    let wt = WorktreeId(worktree.to_string());
    let pick = match session {
        Some(id) => {
            let id = AgentId(id.to_string());
            agents.iter().find(|a| a.id == id)
        }
        None => agents
            .iter()
            .filter(|a| a.worktree_id == wt && !a.archived)
            .max_by_key(|a| a.status_changed_at),
    };
    match pick {
        Some(a) => (
            Some(a.name.clone()),
            a.recent_prompts.last().map(|p| p.text.clone()),
        ),
        None => (None, None),
    }
}

/// Where `worktree` stands for shipping, with a suggested title and body
/// while its branch has no open PR.
#[tauri::command]
pub async fn ship_status(
    client: State<'_, Arc<Client>>,
    ado: State<'_, AdoState>,
    worktree: String,
    session: Option<String>,
) -> ShipRes<ShipView> {
    let place = place(&client, &worktree)?;
    let status = Ship::new(&ado, false).status(&place).await;
    let open = status.pr.as_ref().is_some_and(ship::PrInfo::is_open);
    let suggestion = if open || status.branch.is_none() {
        None
    } else {
        let (name, prompt) =
            session_words(&client.snapshot().agents, &worktree, session.as_deref());
        Some(place.suggest(name.as_deref(), prompt.as_deref()).await)
    };
    Ok(ShipView { status, suggestion })
}

/// Push the worktree's branch and open its PR (or hand back the open one
/// the push updated).
#[tauri::command]
pub async fn ship_create_pr(
    client: State<'_, Arc<Client>>,
    ado: State<'_, AdoState>,
    worktree: String,
    pr: NewPr,
) -> ShipRes<Created> {
    let place = place(&client, &worktree)?;
    Ship::new(&ado, false).create_pr(&place, pr).await
}

/// Merge the worktree's open PR. The local branch and the worktree stay;
/// the safe worktree delete removes them.
#[tauri::command]
pub async fn ship_merge(
    client: State<'_, Arc<Client>>,
    ado: State<'_, AdoState>,
    worktree: String,
    opts: MergeOpts,
) -> ShipRes<Merged> {
    let place = place(&client, &worktree)?;
    Ship::new(&ado, false).merge_pr(&place, opts).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use nebula_core::{AgentKind, AgentRole, AgentStatus, PromptEntry};

    fn agent(id: &str, worktree: &str, at: i64, prompts: &[&str]) -> Agent {
        Agent {
            id: AgentId(id.into()),
            worktree_id: WorktreeId(worktree.into()),
            name: format!("name-{id}"),
            status: AgentStatus::Finished,
            archived: false,
            archived_at: 0,
            unseen: false,
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
            recent_prompts: prompts
                .iter()
                .map(|t| PromptEntry {
                    text: t.to_string(),
                    submitted_at: 0,
                })
                .collect(),
            profile: None,
            parent: None,
        }
    }

    #[test]
    fn the_suggestion_reads_the_named_session_else_the_worktrees_latest() {
        let mut gone = agent("c", "w1", 99, &["old"]);
        gone.archived = true;
        let agents = vec![
            agent("a", "w1", 5, &["first", "fix the login"]),
            agent("b", "w1", 9, &[]),
            agent("d", "w2", 50, &["elsewhere"]),
            gone,
        ];
        assert_eq!(
            session_words(&agents, "w1", Some("a")),
            (Some("name-a".into()), Some("fix the login".into()))
        );
        assert_eq!(
            session_words(&agents, "w1", None),
            (Some("name-b".into()), None)
        );
        assert_eq!(session_words(&agents, "w3", None), (None, None));
    }

    #[test]
    fn the_view_flattens_the_status_beside_the_suggestion() {
        let view = ShipView {
            status: ShipStatus {
                branch: Some("fix".into()),
                ..ShipStatus::default()
            },
            suggestion: Some(Suggestion {
                title: "Fix".into(),
                body: String::new(),
            }),
        };
        let v = serde_json::to_value(view).unwrap();
        assert_eq!(v["branch"], "fix");
        assert_eq!(v["suggestion"]["title"], "Fix");
        assert!(v["pr"].is_null() && v["forge"].is_null());
    }
}
