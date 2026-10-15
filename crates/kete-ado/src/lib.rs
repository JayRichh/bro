//! Azure DevOps: remote parsing, branch naming, tokens, the REST client,
//! the `ado.json` store, and the per-repo context the commands work in.
//!
//! No UI here: the desktop window wraps these in Tauri commands over its
//! daemon connection, and `kete ado` calls them from the command line.

pub mod attribution;
pub mod auth;
pub mod branches;
pub mod client;
pub mod git;
pub mod naming;
pub mod prs;
pub mod remote;
pub mod ship;
pub mod store;
pub mod tasks;

/// An agent to start: the desktop's `create_agent` argument, and what
/// Start task starts in the new worktree.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NewAgent {
    /// Empty when the worktree does not exist yet (Start task fills it).
    #[serde(default)]
    pub worktree: String,
    pub kind: String,
    pub profile: Option<String>,
    pub prompt: Option<String>,
    /// A model picked for this one launch; None takes the Agents tab's.
    #[serde(default)]
    pub model: Option<String>,
    /// An effort picked for this one launch; None takes the Agents tab's.
    #[serde(default)]
    pub effort: Option<String>,
}

use auth::TokenSource;
use client::{AdoClient, AdoError, IdCache};
use remote::{parse_remote, AdoRemote};
use serde::Deserialize;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use store::{AdoStore, RepoKey};

/// `connectionData` is a preview-only resource; every caller asks for this.
pub const CONNECTION_API_VERSION: &str = "7.1-preview.1";

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

/// The app-wide ADO state Tauri manages, once: one token source (so its
/// per-org cache is shared), one store, and per org one client for the
/// commands and one silent client for background work.
pub struct AdoState {
    pub store: Arc<AdoStore>,
    pub tokens: Arc<TokenSource>,
    clients: Mutex<HashMap<String, Arc<AdoClient>>>,
    silent_clients: Mutex<HashMap<String, Arc<AdoClient>>>,
    states: StateCache,
    /// Who I am and each repo's GUIDs, per org, shared by every client.
    ids: Arc<IdCache>,
    /// Orgs whose background PR poll stopped after a sign-in failure, so
    /// the poll never keeps failing on its own. A command that works for
    /// the org starts it again.
    paused: Mutex<HashSet<String>>,
}

impl AdoState {
    pub fn system() -> Self {
        let store = Arc::new(AdoStore::open(AdoStore::default_path()));
        let saver = store.clone();
        let tokens = TokenSource::system().with_tenants(
            store.tenants(),
            Box::new(move |org, tenant| saver.set_tenant(org, tenant)),
        );
        Self::new(store, Arc::new(tokens))
    }

    pub fn new(store: Arc<AdoStore>, tokens: Arc<TokenSource>) -> Self {
        Self {
            store,
            tokens,
            clients: Mutex::new(HashMap::new()),
            silent_clients: Mutex::new(HashMap::new()),
            states: StateCache::default(),
            ids: Arc::default(),
            paused: Mutex::new(HashSet::new()),
        }
    }

    /// Forget the org's cached credential and the ids learned under it.
    pub fn invalidate(&self, org_url: &str) {
        self.tokens.invalidate(org_url);
        self.ids.clear(org_url);
    }

    /// Sign in to the org again with `az login` (its tenant when known),
    /// then start over: no cached credential or ids, and the PR poll
    /// running again.
    pub async fn sign_in(&self, org_url: &str) -> Result<(), AdoError> {
        let res = self.tokens.sign_in(org_url).await;
        self.invalidate(org_url);
        self.resume(org_url);
        res.map_err(AdoError::Auth)
    }

    pub fn client(&self, org_url: &str) -> Arc<AdoClient> {
        self.client_in(&self.clients, org_url, false)
    }

    /// The org's client that never opens a sign-in window: it finds a
    /// token once a command has signed in, or fails.
    pub fn silent_client(&self, org_url: &str) -> Arc<AdoClient> {
        self.client_in(&self.silent_clients, org_url, true)
    }

    fn client_in(
        &self,
        map: &Mutex<HashMap<String, Arc<AdoClient>>>,
        org_url: &str,
        silent: bool,
    ) -> Arc<AdoClient> {
        let mut clients = map.lock().unwrap_or_else(|p| p.into_inner());
        clients
            .entry(org_url.to_string())
            .or_insert_with(|| {
                let c = AdoClient::new(org_url, self.tokens.clone()).with_ids(self.ids.clone());
                Arc::new(if silent { c.silent() } else { c })
            })
            .clone()
    }

    pub fn is_paused(&self, org_url: &str) -> bool {
        self.paused
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .contains(org_url)
    }

    pub fn pause(&self, org_url: &str) {
        self.paused
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .insert(org_url.to_string());
    }

    pub fn resume(&self, org_url: &str) {
        self.paused
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .remove(org_url);
    }

    /// The ADO context for the checkout at `path`, from its git remote.
    /// No HTTP happens here.
    pub async fn context(&self, path: &Path) -> Result<AdoContext, String> {
        let (remote_name, remote) = ado_remote(path)
            .await?
            .ok_or_else(|| "This repo has no Azure DevOps remote.".to_string())?;
        // A user command for the org (PRs, tasks, anything) may sign in, so
        // its paused PR poll tries again; still failing, it pauses again.
        self.resume(&remote.org_url);
        Ok(self.make_context(path, remote_name, remote, false))
    }

    /// As [`context`](Self::context), for background work: its client
    /// never opens a sign-in window, and a checkout without an ADO remote
    /// is `None` rather than an error.
    pub async fn silent_context(&self, path: &Path) -> Result<Option<AdoContext>, String> {
        Ok(ado_remote(path)
            .await?
            .map(|(name, remote)| self.make_context(path, name, remote, true)))
    }

    fn make_context(
        &self,
        path: &Path,
        remote_name: String,
        remote: AdoRemote,
        silent: bool,
    ) -> AdoContext {
        let client = if silent {
            self.silent_client(&remote.org_url)
        } else {
            self.client(&remote.org_url)
        };
        AdoContext::new(
            path.to_path_buf(),
            remote_name,
            remote,
            client,
            self.store.clone(),
            self.states.clone(),
        )
    }
}

/// One repo's ADO handles: where it is, who it is on ADO, the client and
/// the store.
pub struct AdoContext {
    pub path: PathBuf,
    /// The git remote's name (`origin`).
    pub remote_name: String,
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
    pub default_team: Option<TeamRef>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TeamRef {
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
        remote_name: String,
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
            remote_name,
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
        meta.default_team
            .map(|t| t.name)
            .filter(|n| !n.is_empty())
            .ok_or_else(|| AdoError::Local("Choose a team".into()))
    }

    /// The repo's ADO default branch as a remote ref (`origin/main`),
    /// from the repository's metadata. None for an empty repo.
    pub async fn default_branch_ref(&self) -> Result<Option<String>, AdoError> {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct Repo {
            #[serde(default)]
            default_branch: Option<String>,
        }
        let repo: Repo = self
            .client
            .get(&format!(
                "/{}/_apis/git/repositories/{}",
                enc(&self.remote.project),
                enc(&self.remote.repo)
            ))
            .await?;
        Ok(repo
            .default_branch
            .map(|b| b.trim_start_matches("refs/heads/").to_string())
            .filter(|b| !b.is_empty())
            .map(|b| format!("{}/{b}", self.remote_name)))
    }

    /// Who is signed in.
    pub async fn me(&self) -> Result<Me, AdoError> {
        // connectionData is a location-service resource without a released
        // 7.1 version, so it is asked for as preview, the same version
        // `prs::my_id` uses.
        let v: serde_json::Value = self
            .client
            .get(&format!(
                "/_apis/connectionData?api-version={}",
                CONNECTION_API_VERSION
            ))
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

/// `KETE_ADO_BASE`: a local mock server (`http://127.0.0.1:<port>`) that
/// every ADO request goes to instead of Azure DevOps, signed with a dummy
/// token. For screenshots and manual tests; loopback only, so neither a
/// real token nor a real request is ever involved. Debug builds only:
/// a release build never reads the variable.
#[cfg(debug_assertions)]
pub fn base_override() -> Option<String> {
    static BASE: std::sync::OnceLock<Option<String>> = std::sync::OnceLock::new();
    BASE.get_or_init(|| {
        std::env::var("KETE_ADO_BASE")
            .ok()
            .and_then(|v| parse_base(&v))
    })
    .clone()
}

#[cfg(not(debug_assertions))]
pub fn base_override() -> Option<String> {
    None
}

/// A mock base as `http://host:port`, or None unless it is plain http to
/// a loopback host (`127.0.0.1`, `localhost`, `[::1]`) on an explicit
/// port, with no user info, path, query or fragment.
#[cfg(debug_assertions)]
pub fn parse_base(raw: &str) -> Option<String> {
    let u = reqwest::Url::parse(raw.trim()).ok()?;
    if u.scheme() != "http" || !u.username().is_empty() || u.password().is_some() {
        return None;
    }
    if !matches!(u.path(), "" | "/") || u.query().is_some() || u.fragment().is_some() {
        return None;
    }
    // The parser has already lowercased and normalised the host.
    let host = u.host_str()?;
    if !matches!(host, "127.0.0.1" | "localhost" | "[::1]") {
        return None;
    }
    let port = u.port()?;
    Some(format!("http://{host}:{port}"))
}

/// `url` with its scheme and host swapped for `base`'s, path kept.
pub fn rebase_url(url: &str, base: &str) -> String {
    let Some(rest) = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))
    else {
        return url.to_string();
    };
    match rest.find('/') {
        Some(i) => format!("{base}{}", &rest[i..]),
        None => base.to_string(),
    }
}

/// The repo's ADO remote and its name: `origin` when it is one, else the
/// only remote that is. `None` when no remote is; several and none of
/// them `origin` is an error.
pub async fn ado_remote(path: &Path) -> Result<Option<(String, AdoRemote)>, String> {
    if let Ok(url) = git_out(path, &["remote", "get-url", "origin"]).await {
        if let Some(r) = parse_remote(&url) {
            return Ok(Some(("origin".into(), r)));
        }
    }
    let names = git_out(path, &["remote"]).await?;
    let mut found = Vec::new();
    for name in names.lines().map(str::trim).filter(|n| !n.is_empty()) {
        if let Ok(url) = git_out(path, &["remote", "get-url", name]).await {
            if let Some(r) = parse_remote(&url) {
                found.push((name.to_string(), r));
            }
        }
    }
    match found.len() {
        1 => Ok(Some(found.remove(0))),
        0 => Ok(None),
        _ => Err("This repo has several Azure DevOps remotes and none is origin.".into()),
    }
}

/// The remote a range diff fetches from: the repo's ADO remote (so a PR's
/// `refs/pull/{id}/merge` comes from Azure DevOps even when `origin` is
/// another host), else [`branches::pick_remote`]'s, else `origin`.
pub async fn range_remote(path: &Path) -> String {
    if let Ok(Some((name, _))) = ado_remote(path).await {
        return name;
    }
    let p = path.to_path_buf();
    tokio::task::spawn_blocking(move || branches::pick_remote(&p))
        .await
        .ok()
        .filter(|r| !r.is_empty())
        .unwrap_or_else(|| "origin".into())
}

#[cfg(any(test, feature = "test-support"))]
pub mod testing {
    //! A context over a mock server, for the command modules' tests.
    use super::*;
    use crate::auth::{Credential, TokenSource};
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
            "origin".into(),
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
    use crate::auth::fake::Fake;
    use httpmock::prelude::*;

    #[test]
    fn rebase_url_swaps_scheme_and_host_only() {
        let base = "http://127.0.0.1:7070";
        assert_eq!(
            rebase_url("https://dev.azure.com/synth/p/_apis/x?a=1", base),
            "http://127.0.0.1:7070/synth/p/_apis/x?a=1"
        );
        assert_eq!(
            rebase_url("https://vssps.dev.azure.com/synth/_apis/identities", base),
            "http://127.0.0.1:7070/synth/_apis/identities"
        );
        assert_eq!(rebase_url("https://synth.visualstudio.com", base), base);
    }

    #[cfg(debug_assertions)]
    #[test]
    fn parse_base_takes_only_a_plain_loopback_port() {
        assert_eq!(
            parse_base("http://127.0.0.1:7417/").as_deref(),
            Some("http://127.0.0.1:7417")
        );
        assert_eq!(
            parse_base(" http://LOCALHOST:9 ").as_deref(),
            Some("http://localhost:9")
        );
        assert_eq!(
            parse_base("http://[::1]:8080").as_deref(),
            Some("http://[::1]:8080")
        );
        for bad in [
            "http://127.0.0.1:1@dev.azure.com",
            "http://user:pw@127.0.0.1:7417",
            "http://localhost.evil.example:7417",
            "http://127.0.0.1.evil.example:7417",
            "https://127.0.0.1:7417",
            "http://127.0.0.1",
            "http://127.0.0.1:7417/synth",
            "http://127.0.0.1:7417/?x=1",
            "http://127.0.0.1:7417/#f",
            "http://10.0.0.1:7417",
            "not a url",
        ] {
            assert_eq!(parse_base(bad), None, "{bad}");
        }
    }

    fn temp_repo(remote: Option<&str>) -> PathBuf {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static N: AtomicUsize = AtomicUsize::new(0);
        let dir = std::env::temp_dir().join(format!(
            "kete-ado-state-{}-{}",
            std::process::id(),
            N.fetch_add(1, Ordering::SeqCst)
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let git = |args: &[&str]| {
            let ok = std::process::Command::new("git")
                .current_dir(&dir)
                .args(args)
                .output()
                .unwrap()
                .status
                .success();
            assert!(ok, "git {args:?}");
        };
        git(&["init", "-q"]);
        if let Some(url) = remote {
            git(&["remote", "add", "origin", url]);
        }
        dir
    }

    #[tokio::test]
    async fn one_state_gives_prompting_and_silent_contexts() {
        let fake = Fake::new().on("git", "password=pat\n");
        let ado = AdoState::new(testing::temp_store(), Arc::new(TokenSource::new(fake)));
        let repo = temp_repo(Some("https://dev.azure.com/tnt/Platform/_git/api"));

        let loud = ado.context(&repo).await.unwrap();
        let quiet = ado.silent_context(&repo).await.unwrap().unwrap();
        assert_eq!(quiet.remote_name, "origin");
        assert_eq!(quiet.key, loud.key);
        let org = "https://dev.azure.com/tnt";
        assert!(Arc::ptr_eq(&loud.client, &ado.client(org)));
        assert!(Arc::ptr_eq(&quiet.client, &ado.silent_client(org)));
        assert!(!Arc::ptr_eq(&loud.client, &quiet.client));
        // One store for both.
        assert!(Arc::ptr_eq(&loud.store, &quiet.store));
        assert!(Arc::ptr_eq(&loud.store, &ado.store));
        let _ = std::fs::remove_dir_all(&repo);
    }

    #[tokio::test]
    async fn a_silent_client_from_the_state_never_prompts() {
        let s = MockServer::start();
        s.mock(|w, t| {
            w.path("/p/_apis/x");
            t.status(200).json_body(serde_json::json!({}));
        });
        let fake = Fake::new().on("git", "password=pat\n");
        let ado = AdoState::new(
            testing::temp_store(),
            Arc::new(TokenSource::new(fake.clone())),
        );
        let _: serde_json::Value = ado
            .silent_client(&s.base_url())
            .get("/p/_apis/x")
            .await
            .unwrap();
        assert_eq!(fake.count("git", "credential.interactive=never"), 1);
    }

    #[tokio::test]
    async fn no_ado_remote_is_none_when_silent_and_an_error_otherwise() {
        let ado = AdoState::new(
            testing::temp_store(),
            Arc::new(TokenSource::fixed(auth::Credential::Bearer("tok".into()))),
        );
        let repo = temp_repo(Some("https://github.com/JayRichh/kete.git"));
        assert!(ado.silent_context(&repo).await.unwrap().is_none());
        assert_eq!(
            ado.context(&repo).await.err().as_deref(),
            Some("This repo has no Azure DevOps remote.")
        );
        let _ = std::fs::remove_dir_all(&repo);
    }

    #[tokio::test]
    async fn a_command_context_resumes_the_poll_and_the_poll_does_not() {
        let ado = AdoState::new(
            testing::temp_store(),
            Arc::new(TokenSource::fixed(auth::Credential::Bearer("tok".into()))),
        );
        let repo = temp_repo(Some("https://dev.azure.com/tnt/Platform/_git/api"));
        let org = "https://dev.azure.com/tnt";
        ado.pause(org);
        ado.silent_context(&repo).await.unwrap().unwrap();
        assert!(ado.is_paused(org));
        ado.context(&repo).await.unwrap();
        assert!(!ado.is_paused(org));
        let _ = std::fs::remove_dir_all(&repo);
    }

    #[test]
    fn one_org_shares_its_ids_and_an_invalidate_clears_them() {
        let ado = AdoState::new(
            testing::temp_store(),
            Arc::new(TokenSource::fixed(auth::Credential::Bearer("tok".into()))),
        );
        let org = "https://dev.azure.com/tnt";
        ado.client(org).ids().set_me(org, "me-1");
        ado.client("https://dev.azure.com/other")
            .ids()
            .set_me("https://dev.azure.com/other", "me-2");
        // The silent client of the org sees the same ids.
        assert_eq!(
            ado.silent_client(org).ids().me(org).as_deref(),
            Some("me-1")
        );
        ado.invalidate(org);
        assert_eq!(ado.client(org).ids().me(org), None);
        assert_eq!(
            ado.client(org)
                .ids()
                .me("https://dev.azure.com/other")
                .as_deref(),
            Some("me-2")
        );
    }

    #[tokio::test]
    async fn sign_in_clears_the_ids_and_resumes_the_poll() {
        let ado = AdoState::new(
            testing::temp_store(),
            Arc::new(TokenSource::fixed(auth::Credential::Bearer("tok".into()))),
        );
        let org = "https://dev.azure.com/tnt";
        ado.client(org).ids().set_me(org, "me-1");
        ado.pause(org);
        ado.sign_in(org).await.unwrap();
        assert!(!ado.is_paused(org));
        assert_eq!(ado.client(org).ids().me(org), None);
    }

    #[tokio::test]
    async fn a_range_diff_reads_the_ado_remote_even_when_origin_is_elsewhere() {
        let both = temp_repo(Some("https://github.com/JayRichh/kete.git"));
        let ok = std::process::Command::new("git")
            .current_dir(&both)
            .args([
                "remote",
                "add",
                "azure",
                "https://dev.azure.com/tnt/Platform/_git/api",
            ])
            .status()
            .unwrap()
            .success();
        assert!(ok);
        assert_eq!(range_remote(&both).await, "azure");
        // Origin on ADO: origin.
        let ado = temp_repo(Some("https://dev.azure.com/tnt/Platform/_git/api"));
        assert_eq!(range_remote(&ado).await, "origin");
        // No ADO remote at all: the plain pick.
        let gh = temp_repo(Some("https://github.com/JayRichh/kete.git"));
        assert_eq!(range_remote(&gh).await, "origin");
        let none = temp_repo(None);
        assert_eq!(range_remote(&none).await, "origin");
        for d in [both, ado, gh, none] {
            let _ = std::fs::remove_dir_all(d);
        }
    }

    #[test]
    fn pausing_is_per_org() {
        let ado = AdoState::new(
            testing::temp_store(),
            Arc::new(TokenSource::fixed(auth::Credential::Bearer("tok".into()))),
        );
        ado.pause("https://dev.azure.com/a");
        assert!(ado.is_paused("https://dev.azure.com/a"));
        assert!(!ado.is_paused("https://dev.azure.com/b"));
        ado.resume("https://dev.azure.com/a");
        assert!(!ado.is_paused("https://dev.azure.com/a"));
    }

    #[test]
    fn encodes_path_segments() {
        assert_eq!(enc("Platform Team"), "Platform%20Team");
        assert_eq!(enc("User Story"), "User%20Story");
        assert_eq!(enc("a/b"), "a%2Fb");
        assert_eq!(enc("Kete-1.x_~"), "Kete-1.x_~");
    }
}
