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
use nebula_core::{AgentId, WorktreeId};
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
    Ok(
        Place::new(path, nebula_config::Config::load().worktree_base_branch)
            .with_recorded(client.recorded_base(worktree)),
    )
}

/// Where `worktree` stands for shipping, with a suggested title and body
/// while its branch has no open PR. Silent, as the board asks it unprompted:
/// ADO never opens a sign-in window here, a missing sign-in is `error`.
#[tauri::command]
pub async fn ship_status(
    client: State<'_, Arc<Client>>,
    ado: State<'_, AdoState>,
    worktree: String,
    session: Option<String>,
) -> ShipRes<ShipView> {
    let place = place(&client, &worktree)?;
    let status = Ship::new(&ado, true).status(&place).await;
    let open = status.pr.as_ref().is_some_and(ship::PrInfo::is_open);
    let suggestion = if open || status.branch.is_none() {
        None
    } else {
        let (name, prompt) = ship::session_words(
            &client.snapshot().agents,
            &WorktreeId(worktree.clone()),
            session.map(AgentId).as_ref(),
        );
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

/// GitHub's sign-in for a ship error of kind `signIn`: `gh auth login` in a
/// new terminal in `worktree`, the way an account's login command runs (the
/// window opens the terminal). ADO's has its own command, `ado_sign_in`.
#[tauri::command]
pub async fn ship_gh_sign_in(
    client: State<'_, Arc<Client>>,
    worktree: String,
) -> Result<String, String> {
    crate::accounts::run_in_new_terminal(&client, worktree, GH_SIGN_IN_LINE).await
}

/// What `ship_gh_sign_in` types: fixed here, never taken from the window.
const GH_SIGN_IN_LINE: &str = "gh auth login";

#[cfg(test)]
mod tests {
    use super::*;
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
