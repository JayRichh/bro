//! Azure DevOps: remote parsing, branch naming, tokens, the REST client,
//! the `ado.json` store, and the per-repo context the commands work in.
#![allow(dead_code)]

pub mod auth;
pub mod branches;
pub mod client;
pub mod naming;
pub mod prs;
pub mod remote;
pub mod store;
pub mod tasks;

use auth::TokenSource;
use client::{AdoClient, AdoError};
use remote::{parse_remote, AdoRemote};
use serde::Deserialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use store::{AdoStore, RepoKey};

/// A work item type's state and its category (`Proposed`, `InProgress`,
/// `Resolved`, `Completed`, `Removed`), in workflow order.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct WitState {
    pub name: String,
    #[serde(default)]
    pub category: String,
}

/// States per `org/project/type`, kept for the app's lifetime.
pub type StateCache = Arc<Mutex<HashMap<String, Vec<WitState>>>>;

/// The app-wide ADO state Tauri manages: one token source (so its per-org
/// cache is shared), one store, one client per org.
pub struct AdoState {
    pub store: Arc<AdoStore>,
    pub tokens: Arc<TokenSource>,
    clients: Mutex<HashMap<String, Arc<AdoClient>>>,
    states: StateCache,
}

impl AdoState {
    pub fn system() -> Self {
        Self::new(
            Arc::new(AdoStore::open(AdoStore::default_path())),
            Arc::new(TokenSource::system()),
        )
    }

    pub fn new(store: Arc<AdoStore>, tokens: Arc<TokenSource>) -> Self {
        Self {
            store,
            tokens,
            clients: Mutex::new(HashMap::new()),
            states: StateCache::default(),
        }
    }

    pub fn client(&self, org_url: &str) -> Arc<AdoClient> {
        let mut clients = self.clients.lock().unwrap_or_else(|p| p.into_inner());
        clients
            .entry(org_url.to_string())
            .or_insert_with(|| Arc::new(AdoClient::new(org_url, self.tokens.clone())))
            .clone()
    }

    /// The ADO context for `worktree`: a daemon worktree id, or a path.
    pub async fn context_for(
        &self,
        client: &crate::daemon::Client,
        worktree: &str,
    ) -> Result<AdoContext, String> {
        self.context(&checkout_path(client, worktree)?).await
    }

    /// The ADO context for the checkout at `path`, from its git remote.
    /// No HTTP happens here.
    pub async fn context(&self, path: &Path) -> Result<AdoContext, String> {
        let remote = ado_remote(path).await?;
        let client = self.client(&remote.org_url);
        Ok(AdoContext::new(
            path.to_path_buf(),
            remote,
            client,
            self.store.clone(),
            self.states.clone(),
        ))
    }
}

/// One repo's ADO handles: where it is, who it is on ADO, the client and
/// the store.
pub struct AdoContext {
    pub path: PathBuf,
    pub remote: AdoRemote,
    pub key: RepoKey,
    pub client: Arc<AdoClient>,
    pub store: Arc<AdoStore>,
    states: StateCache,
}

/// `GET _apis/projects/{project}`: the bits Kete uses.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectMeta {
    pub id: String,
    pub name: String,
    pub default_team: Option<TeamRef>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TeamRef {
    #[serde(default)]
    pub id: String,
    pub name: String,
}

/// The signed-in identity, from `_apis/connectionData`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Me {
    /// The identity id work items carry in `System.AssignedTo.id`.
    pub id: String,
    /// The sign-in name (`jay@tnt.co.nz`), what `System.AssignedTo` takes.
    pub unique_name: String,
    pub display_name: String,
}

impl AdoContext {
    pub fn new(
        path: PathBuf,
        remote: AdoRemote,
        client: Arc<AdoClient>,
        store: Arc<AdoStore>,
        states: StateCache,
    ) -> Self {
        let key = RepoKey {
            org: remote.org.clone(),
            project: remote.project.clone(),
            repo: remote.repo.clone(),
        };
        Self {
            path,
            remote,
            key,
            client,
            store,
            states,
        }
    }

    /// `/{project}/_apis/{rest}`.
    pub fn project_api(&self, rest: &str) -> String {
        format!("/{}/_apis/{rest}", enc(&self.remote.project))
    }

    /// `/{project}/{team}/_apis/{rest}`.
    pub fn team_api(&self, team: &str, rest: &str) -> String {
        format!("/{}/{}/_apis/{rest}", enc(&self.remote.project), enc(team))
    }

    /// The work item's page in the ADO web UI.
    pub fn work_item_web_url(&self, id: u32) -> String {
        format!(
            "{}/{}/_workitems/edit/{id}",
            self.client.org_url(),
            enc(&self.remote.project)
        )
    }

    pub async fn project_meta(&self) -> Result<ProjectMeta, AdoError> {
        self.client
            .get(&format!("/_apis/projects/{}", enc(&self.remote.project)))
            .await
    }

    /// The team this repo's views use: the one chosen and remembered in
    /// the store, else the project's default team.
    pub async fn team(&self) -> Result<String, AdoError> {
        if let Some(t) = self.store.read_repo(&self.key, |r| r.team.clone()) {
            if !t.is_empty() {
                return Ok(t);
            }
        }
        let meta = self.project_meta().await?;
        Ok(meta
            .default_team
            .map(|t| t.name)
            .unwrap_or_else(|| format!("{} Team", self.remote.project)))
    }

    /// Who is signed in.
    pub async fn me(&self) -> Result<Me, AdoError> {
        // connectionData is a location-service resource without a released
        // 7.1 version, so it is asked for as preview.
        let v: serde_json::Value = self
            .client
            .get("/_apis/connectionData?api-version=7.1-preview")
            .await?;
        let user = &v["authenticatedUser"];
        let id = user["id"].as_str().unwrap_or_default().to_string();
        if id.is_empty() {
            return Err(AdoError::Http(200, "no signed-in user in the reply".into()));
        }
        let unique_name = user["properties"]["Account"]["$value"]
            .as_str()
            .unwrap_or_default()
            .to_string();
        let display_name = user["providerDisplayName"]
            .as_str()
            .or_else(|| user["customDisplayName"].as_str())
            .unwrap_or_default()
            .to_string();
        Ok(Me {
            id,
            unique_name,
            display_name,
        })
    }

    /// A type's states in workflow order, cached for the app's lifetime.
    pub async fn states(&self, kind: &str) -> Result<Vec<WitState>, AdoError> {
        let cache_key = format!("{}/{}/{kind}", self.key.org, self.key.project);
        if let Some(s) = self
            .states
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .get(&cache_key)
        {
            return Ok(s.clone());
        }
        #[derive(Deserialize)]
        struct List {
            value: Vec<WitState>,
        }
        let list: List = self
            .client
            .get(&self.project_api(&format!("wit/workitemtypes/{}/states", enc(kind))))
            .await?;
        self.states
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .insert(cache_key, list.value.clone());
        Ok(list.value)
    }
}

/// The checkout behind `worktree`: a daemon worktree id, or a directory.
pub fn checkout_path(client: &crate::daemon::Client, worktree: &str) -> Result<PathBuf, String> {
    if let Some(w) = client
        .snapshot()
        .worktrees
        .iter()
        .find(|w| w.id.0 == worktree)
    {
        return Ok(w.path.clone());
    }
    let path = PathBuf::from(worktree);
    if !worktree.is_empty() && path.is_dir() {
        Ok(path)
    } else {
        Err("That worktree is gone.".into())
    }
}

/// Percent-encode one URL path segment.
pub fn enc(seg: &str) -> String {
    let mut out = String::with_capacity(seg.len());
    for b in seg.bytes() {
        if b.is_ascii_alphanumeric() || b"-._~".contains(&b) {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

/// Run git in `cwd` without a console window; stdout on success.
pub async fn git_out(cwd: &Path, args: &[&str]) -> Result<String, String> {
    let mut cmd = tokio::process::Command::new("git");
    cmd.args(args)
        .current_dir(cwd)
        .env("GIT_TERMINAL_PROMPT", "0")
        .stdin(std::process::Stdio::null())
        .kill_on_drop(true);
    #[cfg(windows)]
    cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    let out = cmd.output().await.map_err(|e| format!("git: {e}"))?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    } else {
        Err(String::from_utf8_lossy(&out.stderr).trim().to_string())
    }
}

/// The repo's ADO remote: `origin` when it is one, else the only remote
/// that is.
pub async fn ado_remote(path: &Path) -> Result<AdoRemote, String> {
    if let Ok(url) = git_out(path, &["remote", "get-url", "origin"]).await {
        if let Some(r) = parse_remote(&url) {
            return Ok(r);
        }
    }
    let names = git_out(path, &["remote"]).await?;
    let mut found = Vec::new();
    for name in names.lines().map(str::trim).filter(|n| !n.is_empty()) {
        if let Ok(url) = git_out(path, &["remote", "get-url", name]).await {
            if let Some(r) = parse_remote(&url) {
                found.push(r);
            }
        }
    }
    match found.len() {
        1 => Ok(found.remove(0)),
        0 => Err("This repo has no Azure DevOps remote.".into()),
        _ => Err("This repo has several Azure DevOps remotes and none is origin.".into()),
    }
}

#[cfg(test)]
pub(crate) mod testing {
    //! A context over a mock server, for the command modules' tests.
    use super::*;
    use crate::ado::auth::{Credential, TokenSource};
    use std::sync::atomic::{AtomicUsize, Ordering};

    pub fn temp_store() -> Arc<AdoStore> {
        static N: AtomicUsize = AtomicUsize::new(0);
        let dir = std::env::temp_dir().join(format!(
            "kete-ado-ctx-{}-{}",
            std::process::id(),
            N.fetch_add(1, Ordering::SeqCst)
        ));
        let _ = std::fs::remove_dir_all(&dir);
        Arc::new(AdoStore::open(dir.join("ado.json")))
    }

    pub fn context(base_url: &str, store: Arc<AdoStore>) -> AdoContext {
        let remote = AdoRemote {
            host: "dev.azure.com".into(),
            org: "tnt".into(),
            project: "Platform".into(),
            repo: "api".into(),
            org_url: base_url.to_string(),
        };
        let client = Arc::new(AdoClient::new(
            base_url,
            Arc::new(TokenSource::fixed(Credential::Bearer("tok".into()))),
        ));
        AdoContext::new(
            PathBuf::from("."),
            remote,
            client,
            store,
            StateCache::default(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encodes_path_segments() {
        assert_eq!(enc("Platform Team"), "Platform%20Team");
        assert_eq!(enc("User Story"), "User%20Story");
        assert_eq!(enc("a/b"), "a%2Fb");
        assert_eq!(enc("Kete-1.x_~"), "Kete-1.x_~");
    }
}
