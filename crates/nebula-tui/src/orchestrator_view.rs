//! The ORCHESTRATOR VIEW: F10's chat-first layout around the one pinned
//! orchestrator session.

use nebula_core::ClientRequest;

use crate::app::App;

/// F10.
pub fn toggle(app: &mut App, _out: &mut Vec<ClientRequest>) {
    app.chrome.flash = Some("no orchestrator yet".into());
    app.chrome.dirty = true;
}
