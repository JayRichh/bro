//! Pull requests: the shared logic from `kete-ado`, and the window's way
//! to a PR context from a daemon worktree.

pub use kete_ado::prs::*;

use super::{AdoState, AdoStateExt};
use kete_ado::client::CmdError;

/// [`PrCtx`] for a user-initiated command in the window.
pub trait PrCtxExt: Sized {
    /// The PR context for a user-initiated command on `worktree` (a daemon
    /// worktree id or a path). Like every command context it starts the
    /// org's paused PR poll again ([`AdoState::context`]).
    async fn for_command(
        state: &AdoState,
        client: &crate::daemon::Client,
        worktree: &str,
    ) -> Result<Self, CmdError>;
}

impl PrCtxExt for PrCtx {
    async fn for_command(
        state: &AdoState,
        client: &crate::daemon::Client,
        worktree: &str,
    ) -> Result<PrCtx, CmdError> {
        let ado = state.context_for(client, worktree).await?;
        Ok(PrCtx::new(ado).await?)
    }
}
