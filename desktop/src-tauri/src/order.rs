//! The user's order for projects, worktrees and sessions, from a drag or a
//! move up / down in the window: one `Reorder`, answered by `Ack`, whose
//! upserts the model folds in like any other. The request is protocol 56;
//! an older daemon drops the whole connection on one it cannot decode, so
//! it is refused here, before it is sent.
//!
//! Every view lays rows out by `nebula_core::ReorderScope`'s rule:
//! `sort_order`, then the view's own tiebreak, a project's main checkout first.

use crate::daemon::Client;
use crate::orchestrator::RELOAD_HINT;
use crate::sessions::ack;
use crate::Res;
use nebula_core::{ClientRequest, ProjectId, ReorderScope, WorktreeId, REORDER_PROTOCOL};
use serde::Deserialize;
use std::sync::Arc;
use tauri::State;

/// What the window reorders, as the TS side names it
/// (`{ kind: 'worktrees', project }`). The core enum travels positionally
/// over the socket and stays untagged; this is only the JS shape.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Scope {
    Projects,
    Worktrees { project: String },
    Agents { worktree: String },
}

impl From<Scope> for ReorderScope {
    fn from(scope: Scope) -> Self {
        match scope {
            Scope::Projects => ReorderScope::Projects,
            Scope::Worktrees { project } => ReorderScope::Worktrees(ProjectId(project)),
            Scope::Agents { worktree } => ReorderScope::Agents(WorktreeId(worktree)),
        }
    }
}

/// `ids` is the scope's new order, first to last: the whole scope, or its
/// front (the daemon appends what is left out, in its old order).
pub async fn reorder_in(client: &Client, scope: Scope, ids: Vec<String>) -> Res<()> {
    match client.daemon_protocol() {
        0 => return Err("Not connected to the kete daemon.".into()),
        v if v < REORDER_PROTOCOL => return Err(RELOAD_HINT.into()),
        _ => {}
    }
    let scope = ReorderScope::from(scope);
    ack(client, |req_id| ClientRequest::Reorder {
        req_id,
        scope,
        ids,
    })
    .await
    .map(drop)
}

#[tauri::command]
pub async fn reorder(client: State<'_, Arc<Client>>, scope: Scope, ids: Vec<String>) -> Res<()> {
    reorder_in(&client, scope, ids).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sessions::testkit::{ok, run};

    #[test]
    fn the_scope_reads_as_the_ts_side_writes_it() {
        let scope = |json| ReorderScope::from(serde_json::from_value::<Scope>(json).unwrap());
        assert_eq!(
            scope(serde_json::json!({ "kind": "projects" })),
            ReorderScope::Projects
        );
        assert_eq!(
            scope(serde_json::json!({ "kind": "worktrees", "project": "p" })),
            ReorderScope::Worktrees(ProjectId("p".into()))
        );
        assert_eq!(
            scope(serde_json::json!({ "kind": "agents", "worktree": "w" })),
            ReorderScope::Agents(WorktreeId("w".into()))
        );
    }

    #[tokio::test]
    async fn a_reorder_is_sent_only_to_a_daemon_that_takes_it() {
        let (client, mut rx) = Client::connected_for_test();
        client.set_model(|m| m.protocol_version = REORDER_PROTOCOL - 1);
        let r = reorder_in(&client, Scope::Projects, vec!["p".into()]).await;
        assert_eq!(r.unwrap_err(), RELOAD_HINT);
        assert!(rx.try_recv().is_err(), "nothing went out");

        client.set_model(|m| m.protocol_version = REORDER_PROTOCOL);
        let (r, seen) = run(
            &client,
            &mut rx,
            reorder_in(
                &client,
                Scope::Agents {
                    worktree: "w".into(),
                },
                vec!["b".into(), "a".into()],
            ),
            ok,
        )
        .await;
        r.unwrap();
        assert!(matches!(
            &seen[0],
            ClientRequest::Reorder { scope: ReorderScope::Agents(w), ids, .. }
                if w.0 == "w" && ids == &["b".to_string(), "a".to_string()]
        ));
    }
}
