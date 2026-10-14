//! The ADO REST client: auth header, `api-version`, error classes and one
//! `Retry-After` retry.

use crate::auth::{AuthError, TokenSource};
use reqwest::{Method, StatusCode};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt;
use std::sync::{Arc, Mutex};
use std::time::Duration;

pub const API_VERSION: &str = "7.1";
const JSON_PATCH: &str = "application/json-patch+json";
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);
const RETRY_AFTER_CAP: Duration = Duration::from_secs(30);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AdoError {
    Auth(AuthError),
    /// 401: the credential was refused.
    Unauthorized,
    /// 403: signed in, but not allowed into `area`.
    Forbidden {
        area: String,
    },
    NotFound,
    /// 409, with ADO's message.
    Conflict(String),
    /// A local or validation failure (git, input), shown as is.
    Local(String),
    Http(u16, String),
    Network(String),
}

impl AdoError {
    /// One sentence for the UI.
    pub fn user_message(&self) -> String {
        match self {
            AdoError::Auth(e) => e.to_string(),
            AdoError::Unauthorized => {
                "Azure DevOps did not accept your sign-in. Sign in again and retry.".into()
            }
            AdoError::Forbidden { area } => {
                format!("Your Azure DevOps account does not have access to {area} here.")
            }
            AdoError::NotFound => "Azure DevOps could not find that.".into(),
            AdoError::Conflict(msg) => format!("Azure DevOps refused the change: {msg}"),
            AdoError::Local(msg) => msg.clone(),
            AdoError::Http(code, msg) if msg.is_empty() => {
                format!("Azure DevOps answered with error {code}.")
            }
            AdoError::Http(code, msg) => {
                format!("Azure DevOps answered with error {code}: {msg}")
            }
            AdoError::Network(msg) => format!("Could not reach Azure DevOps: {msg}"),
        }
    }
}

impl fmt::Display for AdoError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.user_message())
    }
}

impl std::error::Error for AdoError {}

impl From<AuthError> for AdoError {
    fn from(e: AuthError) -> Self {
        AdoError::Auth(e)
    }
}

/// What kind of failure a command hit, so the window can offer the right
/// way out ("Sign in again" for `Auth`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ErrorKind {
    /// Not signed in, or the sign-in was refused (401, interaction
    /// required, no credential).
    Auth,
    /// Signed in, but not allowed into that area (403).
    Forbidden,
    /// Azure DevOps could not be reached.
    Offline,
    Other,
}

impl AdoError {
    pub fn kind(&self) -> ErrorKind {
        match self {
            AdoError::Auth(_) | AdoError::Unauthorized => ErrorKind::Auth,
            AdoError::Forbidden { .. } => ErrorKind::Forbidden,
            AdoError::Network(_) => ErrorKind::Offline,
            _ => ErrorKind::Other,
        }
    }
}

/// An ADO command's error as the window gets it: `{ kind, message }`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CmdError {
    pub kind: ErrorKind,
    pub message: String,
}

impl fmt::Display for CmdError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl From<AdoError> for CmdError {
    fn from(e: AdoError) -> Self {
        CmdError {
            kind: e.kind(),
            message: e.user_message(),
        }
    }
}

/// A local failure (git, input, a gone worktree), already in words.
impl From<String> for CmdError {
    fn from(message: String) -> Self {
        CmdError {
            kind: ErrorKind::Other,
            message,
        }
    }
}

impl From<&str> for CmdError {
    fn from(message: &str) -> Self {
        message.to_string().into()
    }
}

/// What a REST path touches, for "no access to ..." messages.
pub fn area_of(path: &str) -> &'static str {
    if path.contains("/wit/") {
        "work items"
    } else if path.contains("/git/") {
        "code"
    } else if path.contains("/identities") {
        "identities"
    } else if path.contains("/policy/") {
        "policies"
    } else if path.contains("/work/") {
        "boards"
    } else {
        "this area"
    }
}

/// Ids learned per org (who I am, each repo's GUIDs). They belong to
/// whoever is signed in, so a refused or dropped sign-in clears the org's.
#[derive(Default)]
pub struct IdCache {
    orgs: Mutex<HashMap<String, OrgIds>>,
}

#[derive(Default, Clone)]
struct OrgIds {
    /// My identity id (`connectionData`'s `authenticatedUser.id`).
    me: Option<String>,
    /// `project/repo` to (repo GUID, project GUID).
    repos: HashMap<String, (String, String)>,
}

impl IdCache {
    fn with<R>(&self, org_url: &str, f: impl FnOnce(&mut OrgIds) -> R) -> R {
        let mut orgs = self.orgs.lock().unwrap_or_else(|p| p.into_inner());
        f(orgs.entry(org_key(org_url)).or_default())
    }

    pub fn me(&self, org_url: &str) -> Option<String> {
        self.with(org_url, |o| o.me.clone())
    }

    pub fn set_me(&self, org_url: &str, id: &str) {
        self.with(org_url, |o| o.me = Some(id.to_string()));
    }

    pub fn repo(&self, org_url: &str, repo: &str) -> Option<(String, String)> {
        self.with(org_url, |o| o.repos.get(repo).cloned())
    }

    pub fn set_repo(&self, org_url: &str, repo: &str, ids: (String, String)) {
        self.with(org_url, |o| o.repos.insert(repo.to_string(), ids));
    }

    /// Forget everything learned for `org_url`, and only that org.
    pub fn clear(&self, org_url: &str) {
        self.orgs
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .remove(&org_key(org_url));
    }
}

/// An org URL as a map key: no trailing slash.
fn org_key(org_url: &str) -> String {
    org_url.trim_end_matches('/').to_string()
}

/// One org's REST API.
pub struct AdoClient {
    http: reqwest::Client,
    tokens: Arc<TokenSource>,
    org_url: String,
    /// Never prompt for a sign-in (background work).
    silent: bool,
    ids: Arc<IdCache>,
}

impl AdoClient {
    pub fn new(org_url: &str, tokens: Arc<TokenSource>) -> Self {
        let http = reqwest::Client::builder()
            .timeout(REQUEST_TIMEOUT)
            .build()
            .expect("reqwest client");
        Self {
            http,
            tokens,
            org_url: org_url.trim_end_matches('/').to_string(),
            silent: false,
            ids: Arc::default(),
        }
    }

    /// A client whose tokens come from [`TokenSource::token_silent`], so
    /// it never opens a sign-in window.
    pub fn silent(mut self) -> Self {
        self.silent = true;
        self
    }

    /// Share `ids` (the app's, so every client of an org sees one set).
    pub fn with_ids(mut self, ids: Arc<IdCache>) -> Self {
        self.ids = ids;
        self
    }

    pub fn ids(&self) -> &IdCache {
        &self.ids
    }

    async fn credential(&self) -> Result<crate::auth::Credential, AdoError> {
        Ok(if self.silent {
            self.tokens.token_silent(&self.org_url).await?
        } else {
            self.tokens.token(&self.org_url).await?
        })
    }

    pub fn org_url(&self) -> &str {
        &self.org_url
    }

    pub async fn get<T: DeserializeOwned>(&self, path: &str) -> Result<T, AdoError> {
        self.send(Method::GET, path, None).await
    }

    pub async fn post_json<T: DeserializeOwned>(
        &self,
        path: &str,
        body: &serde_json::Value,
    ) -> Result<T, AdoError> {
        self.send(Method::POST, path, Some(body)).await
    }

    pub async fn patch_json<T: DeserializeOwned>(
        &self,
        path: &str,
        body: &serde_json::Value,
    ) -> Result<T, AdoError> {
        self.send(Method::PATCH, path, Some(body)).await
    }

    /// A JSON Patch document (work item updates), sent as
    /// `application/json-patch+json`, which those endpoints require.
    pub async fn patch_json_patch<T: DeserializeOwned>(
        &self,
        path: &str,
        body: &serde_json::Value,
    ) -> Result<T, AdoError> {
        self.send_as(Method::PATCH, path, Some(body), Some(JSON_PATCH))
            .await
    }

    pub async fn put_json<T: DeserializeOwned>(
        &self,
        path: &str,
        body: &serde_json::Value,
    ) -> Result<T, AdoError> {
        self.send(Method::PUT, path, Some(body)).await
    }

    /// Absolute URLs (vssps) pass through; others join `org_url`. Adds
    /// `api-version=7.1` unless the path names one.
    fn url(&self, path: &str) -> String {
        let mut url = if path.starts_with("https://") {
            path.to_string()
        } else if path.starts_with('/') {
            format!("{}{path}", self.org_url)
        } else {
            format!("{}/{path}", self.org_url)
        };
        if !url.contains("api-version=") {
            url.push(if url.contains('?') { '&' } else { '?' });
            url.push_str("api-version=");
            url.push_str(API_VERSION);
        }
        match crate::base_override() {
            Some(base) => crate::rebase_url(&url, &base),
            None => url,
        }
    }

    async fn send<T: DeserializeOwned>(
        &self,
        method: Method,
        path: &str,
        body: Option<&serde_json::Value>,
    ) -> Result<T, AdoError> {
        self.send_as(method, path, body, None).await
    }

    async fn send_as<T: DeserializeOwned>(
        &self,
        method: Method,
        path: &str,
        body: Option<&serde_json::Value>,
        content_type: Option<&str>,
    ) -> Result<T, AdoError> {
        let url = self.url(path);
        let mut auth = self.credential().await?.header();
        let mut retried = false;
        let mut reauthed = false;
        loop {
            let mut req = self
                .http
                .request(method.clone(), &url)
                .header(reqwest::header::AUTHORIZATION, &auth)
                .header(reqwest::header::ACCEPT, "application/json");
            match (body, content_type) {
                (Some(b), Some(ct)) => {
                    req = req
                        .header(reqwest::header::CONTENT_TYPE, ct)
                        .body(b.to_string());
                }
                (Some(b), None) => req = req.json(b),
                (None, _) => {}
            }
            let resp = req
                .send()
                .await
                .map_err(|e| AdoError::Network(e.without_url().to_string()))?;
            let status = resp.status();
            if status == StatusCode::UNAUTHORIZED && !reauthed {
                // The credential was refused: drop it and try once more
                // with a fresh one. Only a prompting client tells GCM
                // (which signs the user out of it, for a GCM credential);
                // background work only forgets its copy.
                reauthed = true;
                if self.silent {
                    self.tokens.invalidate(&self.org_url);
                } else {
                    self.tokens.reject(&self.org_url).await;
                }
                self.ids.clear(&self.org_url);
                auth = self.credential().await?.header();
                continue;
            }
            if !retried
                && (status == StatusCode::TOO_MANY_REQUESTS
                    || status == StatusCode::SERVICE_UNAVAILABLE)
            {
                if let Some(wait) = retry_after(&resp) {
                    retried = true;
                    tokio::time::sleep(wait.min(RETRY_AFTER_CAP)).await;
                    continue;
                }
            }
            let bytes = resp
                .bytes()
                .await
                .map_err(|e| AdoError::Network(e.without_url().to_string()))?;
            if status.is_success() {
                let bytes: &[u8] = if bytes.iter().all(u8::is_ascii_whitespace) {
                    b"null"
                } else {
                    &bytes
                };
                return serde_json::from_slice(bytes).map_err(|e| {
                    AdoError::Http(status.as_u16(), format!("unreadable reply: {e}"))
                });
            }
            return Err(classify(status, path, &bytes));
        }
    }
}

/// `Retry-After` in whole seconds.
fn retry_after(resp: &reqwest::Response) -> Option<Duration> {
    let v = resp.headers().get(reqwest::header::RETRY_AFTER)?;
    let secs: u64 = v.to_str().ok()?.trim().parse().ok()?;
    Some(Duration::from_secs(secs))
}

fn classify(status: StatusCode, path: &str, body: &[u8]) -> AdoError {
    match status {
        StatusCode::UNAUTHORIZED => AdoError::Unauthorized,
        StatusCode::FORBIDDEN => AdoError::Forbidden {
            area: area_of(path).to_string(),
        },
        StatusCode::NOT_FOUND => AdoError::NotFound,
        StatusCode::CONFLICT => AdoError::Conflict(message_of(body)),
        s => AdoError::Http(s.as_u16(), message_of(body)),
    }
}

/// ADO's `message` field, else the start of the body.
fn message_of(body: &[u8]) -> String {
    if let Ok(v) = serde_json::from_slice::<serde_json::Value>(body) {
        if let Some(m) = v.get("message").and_then(|m| m.as_str()) {
            return m.to_string();
        }
    }
    String::from_utf8_lossy(body)
        .trim()
        .chars()
        .take(200)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::fake::Fake;
    use crate::auth::{Credential, TokenSource};
    use httpmock::prelude::*;
    use serde_json::json;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    fn test_client(s: &MockServer, cred: Credential) -> AdoClient {
        AdoClient::new(&s.base_url(), Arc::new(TokenSource::fixed(cred)))
    }

    #[tokio::test]
    async fn appends_api_version_and_bearer() {
        let s = MockServer::start();
        let m = s.mock(|w, t| {
            w.path("/p/_apis/x")
                .query_param("api-version", "7.1")
                .header("authorization", "Bearer tok");
            t.status(200).json_body(json!({"ok":1}));
        });
        let c = test_client(&s, Credential::Bearer("tok".into()));
        let v: serde_json::Value = c.get("/p/_apis/x").await.unwrap();
        m.assert();
        assert_eq!(v["ok"], 1);
    }

    #[tokio::test]
    async fn keeps_an_explicit_api_version() {
        let s = MockServer::start();
        let m = s.mock(|w, t| {
            w.path("/p/_apis/policy/evaluations")
                .query_param("api-version", "7.1-preview.1");
            t.status(200).json_body(json!({"value":[]}));
        });
        let c = test_client(&s, Credential::Bearer("tok".into()));
        let _: serde_json::Value = c
            .get("/p/_apis/policy/evaluations?api-version=7.1-preview.1")
            .await
            .unwrap();
        m.assert();
    }

    /// httpmock 0.7 cannot answer the same request two ways in turn, so
    /// this test serves 429 then 200 from a bare socket.
    #[tokio::test]
    async fn retry_after_is_honoured_once() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let replies = [
                "HTTP/1.1 429 Too Many Requests\r\nRetry-After: 0\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 8\r\nConnection: close\r\n\r\n{\"ok\":1}",
            ];
            let mut hits = 0;
            for reply in replies {
                let (mut sock, _) = listener.accept().await.unwrap();
                let mut buf = [0u8; 4096];
                let _ = sock.read(&mut buf).await.unwrap();
                sock.write_all(reply.as_bytes()).await.unwrap();
                sock.shutdown().await.unwrap();
                hits += 1;
            }
            hits
        });
        let c = AdoClient::new(
            &format!("http://{addr}"),
            Arc::new(TokenSource::fixed(Credential::Bearer("tok".into()))),
        );
        let v: serde_json::Value = c.get("/p/_apis/x").await.unwrap();
        assert_eq!(v["ok"], 1);
        assert_eq!(server.await.unwrap(), 2);
    }

    #[tokio::test]
    async fn second_429_is_returned() {
        let s = MockServer::start();
        let m = s.mock(|w, t| {
            w.path("/p/_apis/x");
            t.status(429).header("Retry-After", "0");
        });
        let c = test_client(&s, Credential::Bearer("tok".into()));
        let e = c.get::<serde_json::Value>("/p/_apis/x").await.unwrap_err();
        assert!(matches!(e, AdoError::Http(429, _)));
        assert_eq!(m.hits(), 2);
    }

    #[tokio::test]
    async fn unauthorized_rejects_and_retries_once() {
        let s = MockServer::start();
        let m = s.mock(|w, t| {
            w.path("/p/_apis/x");
            t.status(401);
        });
        let r = Fake::new().on("git", "password=pat\n");
        let c = AdoClient::new(&s.base_url(), Arc::new(TokenSource::new(r.clone())));
        let e = c.get::<serde_json::Value>("/p/_apis/x").await.unwrap_err();
        assert!(matches!(e, AdoError::Unauthorized));
        assert_eq!(m.hits(), 2);
        assert_eq!(r.count("git", "reject"), 1);
        assert_eq!(r.count("git", "fill"), 2);
    }

    #[tokio::test]
    async fn a_silent_401_never_rejects_and_clears_the_ids() {
        let s = MockServer::start();
        let m = s.mock(|w, t| {
            w.path("/p/_apis/x");
            t.status(401);
        });
        let r = Fake::new().on("git", "password=pat\n");
        let c = AdoClient::new(&s.base_url(), Arc::new(TokenSource::new(r.clone()))).silent();
        c.ids().set_me(&s.base_url(), "me-1");
        let e = c.get::<serde_json::Value>("/p/_apis/x").await.unwrap_err();
        assert!(matches!(e, AdoError::Unauthorized));
        assert_eq!(m.hits(), 2);
        assert_eq!(r.count("git", "reject"), 0);
        // The cached copy was dropped, so the retry fetched again.
        assert_eq!(r.count("git", "fill"), 2);
        assert_eq!(c.ids().me(&s.base_url()), None);
    }

    #[tokio::test]
    async fn an_az_credential_refused_never_reaches_gcm() {
        let s = MockServer::start();
        s.mock(|w, t| {
            w.path("/p/_apis/x");
            t.status(401);
        });
        let r = Fake::new()
            .fail("git", AuthError::NoCredential)
            .on("az", "{\"accessToken\":\"eyJ.eyJ0aWQiOiJ0LTkifQ.s\"}");
        let c = AdoClient::new(&s.base_url(), Arc::new(TokenSource::new(r.clone())));
        let e = c.get::<serde_json::Value>("/p/_apis/x").await.unwrap_err();
        assert!(matches!(e, AdoError::Unauthorized));
        assert_eq!(r.count("git", "reject"), 0);
    }

    #[test]
    fn clearing_one_org_keeps_another_whose_url_it_prefixes() {
        let ids = IdCache::default();
        ids.set_me("https://dev.azure.com/tnt", "a");
        ids.set_me("https://dev.azure.com/tnt2", "b");
        ids.set_repo(
            "https://dev.azure.com/tnt/",
            "p/r",
            ("r".into(), "p".into()),
        );
        ids.clear("https://dev.azure.com/tnt");
        assert_eq!(ids.me("https://dev.azure.com/tnt"), None);
        assert_eq!(ids.repo("https://dev.azure.com/tnt", "p/r"), None);
        assert_eq!(ids.me("https://dev.azure.com/tnt2").as_deref(), Some("b"));
    }

    #[tokio::test]
    async fn a_silent_client_never_lets_gcm_prompt() {
        let s = MockServer::start();
        s.mock(|w, t| {
            w.path("/p/_apis/x");
            t.status(200).json_body(json!({}));
        });
        let r = Fake::new().on("git", "password=pat\n");
        let c = AdoClient::new(&s.base_url(), Arc::new(TokenSource::new(r.clone()))).silent();
        let _: serde_json::Value = c.get("/p/_apis/x").await.unwrap();
        assert_eq!(r.count("git", "credential.interactive=never"), 1);
    }

    #[test]
    fn command_errors_carry_their_kind() {
        let kind = |e: AdoError| CmdError::from(e).kind;
        assert_eq!(kind(AdoError::Unauthorized), ErrorKind::Auth);
        assert_eq!(
            kind(AdoError::Auth(AuthError::InteractionRequired("x".into()))),
            ErrorKind::Auth
        );
        assert_eq!(
            kind(AdoError::Forbidden {
                area: "code".into()
            }),
            ErrorKind::Forbidden
        );
        assert_eq!(kind(AdoError::Network("down".into())), ErrorKind::Offline);
        assert_eq!(kind(AdoError::NotFound), ErrorKind::Other);
        let e = CmdError::from(AdoError::Unauthorized);
        assert_eq!(
            serde_json::to_value(&e).unwrap(),
            json!({"kind": "auth", "message": AdoError::Unauthorized.user_message()})
        );
        assert_eq!(CmdError::from("gone").kind, ErrorKind::Other);
    }

    #[test]
    fn local_errors_read_as_written() {
        assert_eq!(
            AdoError::Local("Add a title".into()).user_message(),
            "Add a title"
        );
    }

    #[tokio::test]
    async fn forbidden_names_the_area() {
        let s = MockServer::start();
        s.mock(|w, t| {
            w.path("/p/_apis/wit/workitems");
            t.status(403).json_body(json!({"message":"denied"}));
        });
        let c = test_client(&s, Credential::Bearer("tok".into()));
        let e = c
            .get::<serde_json::Value>("/p/_apis/wit/workitems")
            .await
            .unwrap_err();
        assert!(matches!(&e, AdoError::Forbidden { area } if area == "work items"));
        assert!(e.user_message().contains("work items"));
    }

    #[tokio::test]
    async fn unauthorized_is_distinct() {
        let s = MockServer::start();
        s.mock(|w, t| {
            w.path("/p/_apis/x");
            t.status(401);
        });
        let c = test_client(&s, Credential::Basic("Ong=".into()));
        let e = c.get::<serde_json::Value>("/p/_apis/x").await.unwrap_err();
        assert!(matches!(e, AdoError::Unauthorized));
    }

    #[tokio::test]
    async fn posts_json_and_maps_conflict() {
        let s = MockServer::start();
        let m = s.mock(|w, t| {
            w.method(POST)
                .path("/p/_apis/git/pullrequests")
                .json_body(json!({"title":"x"}));
            t.status(409)
                .json_body(json!({"message":"An active pull request already exists."}));
        });
        let c = test_client(&s, Credential::Bearer("tok".into()));
        let e = c
            .post_json::<serde_json::Value>("/p/_apis/git/pullrequests", &json!({"title":"x"}))
            .await
            .unwrap_err();
        m.assert();
        assert!(matches!(&e, AdoError::Conflict(m) if m.contains("already exists")));
    }

    #[test]
    fn areas() {
        assert_eq!(area_of("/p/_apis/wit/workitems/1"), "work items");
        assert_eq!(area_of("/p/_apis/git/repositories"), "code");
        assert_eq!(area_of("/_apis/identities?x"), "identities");
        assert_eq!(area_of("/p/_apis/policy/evaluations"), "policies");
        assert_eq!(area_of("/p/t/_apis/work/teamsettings"), "boards");
    }

    #[test]
    fn urls() {
        let c = AdoClient::new(
            "https://dev.azure.com/tnt/",
            Arc::new(TokenSource::fixed(Credential::Bearer("t".into()))),
        );
        assert_eq!(
            c.url("/p/_apis/x?$top=5"),
            "https://dev.azure.com/tnt/p/_apis/x?$top=5&api-version=7.1"
        );
        assert_eq!(
            c.url("https://app.vssps.visualstudio.com/_apis/profile/profiles/me"),
            "https://app.vssps.visualstudio.com/_apis/profile/profiles/me?api-version=7.1"
        );
    }
}
