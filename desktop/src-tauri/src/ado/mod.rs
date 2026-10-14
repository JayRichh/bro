//! Azure DevOps for the window: the client, store and per-repo context
//! live in the `kete-ado` crate (shared with `kete ado`); this module adds
//! what needs the daemon connection, and the Tauri commands.

pub mod branches;
pub mod prs;
pub mod tasks;
pub mod ui;

pub use kete_ado::*;

/// The window's way into a repo's ADO context: a daemon worktree id, or
/// a path.
pub trait AdoStateExt {
    /// The ADO context for `worktree`: a daemon worktree id, or a path.
    async fn context_for(
        &self,
        client: &crate::daemon::Client,
        worktree: &str,
    ) -> Result<AdoContext, String>;
}

impl AdoStateExt for AdoState {
    async fn context_for(
        &self,
        client: &crate::daemon::Client,
        worktree: &str,
    ) -> Result<AdoContext, String> {
        self.context(&checkout_path(client, worktree)?).await
    }
}

/// The checkout behind `worktree`: a daemon worktree id, or a directory.
pub fn checkout_path(
    client: &crate::daemon::Client,
    worktree: &str,
) -> Result<std::path::PathBuf, String> {
    if let Some(w) = client
        .snapshot()
        .worktrees
        .iter()
        .find(|w| w.id.0 == worktree)
    {
        return Ok(w.path.clone());
    }
    let path = std::path::PathBuf::from(worktree);
    if !worktree.is_empty() && path.is_dir() {
        Ok(path)
    } else {
        Err("That worktree is gone.".into())
    }
}
