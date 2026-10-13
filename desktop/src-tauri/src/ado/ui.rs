//! The two small commands the Workshop shell needs: whether a worktree's
//! repo lives on Azure DevOps (the rail shows Tasks, Branches and PRs only
//! then), and saving a view's UI state into `ado.json` so it survives a
//! restart.

use super::store::{AdoStore, RepoKey};
use super::{ado_remote, checkout_path, AdoState};
use crate::daemon::Client;
use serde::Serialize;
use std::sync::Arc;
use tauri::State;

/// Largest UI blob one view may save, serialized. Views keep filters and
/// selections there, never data.
pub const UI_MAX_BYTES: usize = 64 * 1024;

/// The views that keep state, as the window names them (`Place.view` in
/// `shell/history.svelte.ts`). Anything else is refused, so `ado.json`
/// never collects stray keys.
pub const VIEWS: &[&str] = &[
    "overview", "board", "session", "diff", "tasks", "branches", "prs",
];

/// A repo's ADO remote as the window sees it, plus what it saved before.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AdoContextInfo {
    pub host: String,
    pub org: String,
    pub project: String,
    pub repo: String,
    pub org_url: String,
    /// `org/project/repo`: what `ado_save_ui` takes as `repo_key`.
    pub repo_key: String,
    /// Per-view UI state saved earlier, keyed by view; `{}` when none.
    pub ui: serde_json::Value,
}

/// The ADO remote of `worktree` (a daemon worktree id or a path), or None
/// when the repo has no ADO remote. No HTTP happens here.
#[tauri::command]
pub async fn ado_context(
    client: State<'_, Arc<Client>>,
    ado: State<'_, AdoState>,
    worktree: String,
) -> Result<Option<AdoContextInfo>, String> {
    let path = checkout_path(&client, &worktree)?;
    // No ADO remote is None; a failed lookup is an error the window retries.
    let Some((_, r)) = ado_remote(&path).await? else {
        return Ok(None);
    };
    let key = RepoKey {
        org: r.org.clone(),
        project: r.project.clone(),
        repo: r.repo.clone(),
    };
    let ui = saved_ui(&ado.store, &key);
    Ok(Some(AdoContextInfo {
        host: r.host,
        org: r.org,
        project: r.project,
        repo: r.repo,
        org_url: r.org_url,
        repo_key: key.to_string(),
        ui,
    }))
}

/// Save one view's UI state for a repo into `ado.json`.
#[tauri::command]
pub async fn ado_save_ui(
    ado: State<'_, AdoState>,
    repo_key: String,
    view: String,
    json: serde_json::Value,
) -> Result<(), String> {
    save_ui(&ado.store, &repo_key, &view, json)
}

fn saved_ui(store: &AdoStore, key: &RepoKey) -> serde_json::Value {
    store.read_repo(key, |s| match &s.ui {
        serde_json::Value::Object(_) => s.ui.clone(),
        _ => serde_json::Value::Object(Default::default()),
    })
}

/// `org/project/repo` back into a key. ADO names cannot hold a slash.
pub fn parse_repo_key(s: &str) -> Option<RepoKey> {
    let mut parts = s.split('/');
    let (org, project, repo) = (parts.next()?, parts.next()?, parts.next()?);
    if parts.next().is_some() || [org, project, repo].iter().any(|p| p.trim().is_empty()) {
        return None;
    }
    Some(RepoKey {
        org: org.into(),
        project: project.into(),
        repo: repo.into(),
    })
}

pub fn save_ui(
    store: &AdoStore,
    repo_key: &str,
    view: &str,
    json: serde_json::Value,
) -> Result<(), String> {
    let key = parse_repo_key(repo_key).ok_or_else(|| format!("Not a repo key: {repo_key}"))?;
    if !VIEWS.contains(&view) {
        return Err(format!("No view is called {view:?}."));
    }
    let size = serde_json::to_vec(&json).map(|v| v.len()).unwrap_or(0);
    if size > UI_MAX_BYTES {
        return Err(format!("The {view} view's state is too large to save."));
    }
    store
        .try_with_repo(&key, |s| {
            if !s.ui.is_object() {
                s.ui = serde_json::Value::Object(Default::default());
            }
            if let Some(map) = s.ui.as_object_mut() {
                map.insert(view.to_string(), json);
            }
        })
        .map_err(|e| format!("Could not save the view state: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ado::testing::temp_store;
    use serde_json::json;

    #[test]
    fn a_saved_view_reads_back_beside_the_others() {
        let store = temp_store();
        save_ui(
            &store,
            "tnt/Platform/api",
            "tasks",
            json!({ "filter": "gst" }),
        )
        .unwrap();
        save_ui(
            &store,
            "tnt/Platform/api",
            "branches",
            json!({ "open": ["jari"] }),
        )
        .unwrap();
        save_ui(
            &store,
            "tnt/Platform/api",
            "tasks",
            json!({ "filter": "rounding" }),
        )
        .unwrap();
        let key = parse_repo_key("tnt/Platform/api").unwrap();
        let ui = saved_ui(&store, &key);
        assert_eq!(ui["tasks"]["filter"], "rounding");
        assert_eq!(ui["branches"]["open"][0], "jari");

        // It is on disk, not only in memory.
        let reopened = AdoStore::open(store.path());
        assert_eq!(saved_ui(&reopened, &key)["tasks"]["filter"], "rounding");
    }

    #[test]
    fn an_unknown_repo_has_an_empty_ui() {
        let store = temp_store();
        let key = parse_repo_key("tnt/Platform/web").unwrap();
        assert_eq!(saved_ui(&store, &key), json!({}));
    }

    #[test]
    fn bad_keys_views_and_oversized_state_are_refused() {
        let store = temp_store();
        assert!(save_ui(&store, "tnt/Platform", "tasks", json!({})).is_err());
        assert!(save_ui(&store, "tnt//api", "tasks", json!({})).is_err());
        assert!(save_ui(&store, "a/b/c/d", "tasks", json!({})).is_err());
        assert!(save_ui(&store, "tnt/Platform/api", " ", json!({})).is_err());
        assert!(save_ui(&store, "tnt/Platform/api", "taskz", json!({})).is_err());
        assert!(save_ui(&store, "tnt/Platform/api", " tasks", json!({})).is_err());
        let big = "x".repeat(UI_MAX_BYTES + 1);
        assert!(save_ui(&store, "tnt/Platform/api", "tasks", json!({ "q": big })).is_err());
    }
}
