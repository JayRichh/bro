//! The user's order for projects, worktrees and sessions: the
//! `ClientRequest::Reorder` a drag or a move up / move down sends. The
//! daemon writes it and answers with an upsert per row that moved, which
//! the tree folds in like any other; nothing is reordered here first.
//!
//! The rule every client lays rows out by is `nebula_core::ReorderScope`'s:
//! `sort_order`, then the view's own tiebreak, a project's main checkout first.

use crate::app::{App, PendingIntent};
use nebula_core::{ClientRequest, ReorderScope, REORDER_PROTOCOL};

/// What a move says to a daemon too old to take one.
#[cfg(unix)]
pub const DAEMON_TOO_OLD: &str =
    "the running daemon predates reordering: run `kete reload` (every session keeps running), then reopen kete";
/// Windows daemons can't restart in place: `kete reload` refuses there.
#[cfg(windows)]
pub const DAEMON_TOO_OLD: &str =
    "the running daemon predates reordering: run `kete kill` (stops all sessions), then reopen kete";

/// Can the daemon take `Reorder`? Below [`REORDER_PROTOCOL`] it cannot
/// decode the request and would drop the connection on it. None (no
/// handshake yet: tests) counts as yes, as `daemon_provisions` does.
pub fn daemon_reorders(app: &App) -> bool {
    app.orch
        .daemon_protocol
        .is_none_or(|v| v >= REORDER_PROTOCOL)
}

/// Queue the new order of `scope`, first to last (the whole scope, or
/// just its front: the daemon appends whatever `ids` leaves out). A daemon
/// too old for it gets nothing and the flash says what to run; false then.
pub fn reorder(
    app: &mut App,
    scope: ReorderScope,
    ids: Vec<String>,
    out: &mut Vec<ClientRequest>,
) -> bool {
    if !daemon_reorders(app) {
        app.chrome.flash = Some(DAEMON_TOO_OLD.into());
        app.chrome.dirty = true;
        return false;
    }
    let req_id = app.alloc_req_id(PendingIntent::None);
    out.push(ClientRequest::Reorder { req_id, scope, ids });
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use nebula_core::WorktreeId;

    #[test]
    fn a_reorder_goes_out_only_to_a_daemon_that_takes_it() {
        let mut app = App::new();
        let scope = ReorderScope::Agents(WorktreeId("w".into()));
        let mut out = Vec::new();

        app.orch.daemon_protocol = Some(REORDER_PROTOCOL - 1);
        assert!(!reorder(
            &mut app,
            scope.clone(),
            vec!["a".into()],
            &mut out
        ));
        assert!(out.is_empty(), "an older daemon would drop the connection");
        assert_eq!(app.chrome.flash.as_deref(), Some(DAEMON_TOO_OLD));

        app.orch.daemon_protocol = Some(REORDER_PROTOCOL);
        assert!(reorder(
            &mut app,
            scope.clone(),
            vec!["b".into(), "a".into()],
            &mut out
        ));
        match out.as_slice() {
            [ClientRequest::Reorder {
                scope: sent, ids, ..
            }] => {
                assert_eq!(sent, &scope);
                assert_eq!(ids, &["b".to_string(), "a".to_string()]);
            }
            other => panic!("expected one Reorder, got {other:?}"),
        }
    }
}
