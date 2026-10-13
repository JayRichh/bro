//! Azure DevOps pull requests: lists, votes, policies and threads.

use crate::ado::auth::TokenSource;
use crate::ado::client::{AdoClient, AdoError};
use crate::ado::remote::{parse_remote, AdoRemote};
use crate::ado::store::{AdoStore, RepoKey};
use serde::Serialize;
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex, OnceLock};

/// Policy evaluations are only offered as a preview API.
const POLICY_API_VERSION: &str = "7.1-preview.1";
/// `connectionData` is a preview-only resource.
const CONNECTION_API_VERSION: &str = "7.1-preview.1";

/// What a PR command needs: the org's client and the repo's ids.
pub struct PrCtx {
    pub client: AdoClient,
    pub repo_id: String,
    pub project_id: String,
    pub org_url: String,
    pub remote_name: String,
    pub repo_path: PathBuf,
    /// `https://vssps.dev.azure.com/{org}`, for identity search.
    pub vssps_base: String,
    pub key: RepoKey,
}

impl PrCtx {
    /// The context for the ADO repo behind `path`'s remote, or None when
    /// the checkout has no Azure DevOps remote.
    pub async fn from_worktree(
        path: &Path,
        tokens: Arc<TokenSource>,
    ) -> Result<Option<PrCtx>, AdoError> {
        let p = path.to_path_buf();
        let found = tokio::task::spawn_blocking(move || find_ado_remote(&p))
            .await
            .map_err(|e| AdoError::Network(e.to_string()))?
            .map_err(AdoError::Conflict)?;
        let Some((remote_name, remote)) = found else {
            return Ok(None);
        };
        Self::from_remote(path, remote_name, remote, tokens)
            .await
            .map(Some)
    }

    /// The context for a remote already found with [`find_ado_remote`].
    pub async fn from_remote(
        path: &Path,
        remote_name: String,
        remote: AdoRemote,
        tokens: Arc<TokenSource>,
    ) -> Result<PrCtx, AdoError> {
        let client = AdoClient::new(&remote.org_url, tokens);
        let (repo_id, project_id) = repo_ids(&client, &remote).await?;
        Ok(PrCtx {
            vssps_base: vssps_base(&remote.org_url)
                .unwrap_or_else(|| format!("https://vssps.dev.azure.com/{}", remote.org)),
            org_url: remote.org_url.clone(),
            key: RepoKey {
                org: remote.org.clone(),
                project: remote.project.clone(),
                repo: remote.repo.clone(),
            },
            client,
            repo_id,
            project_id,
            remote_name,
            repo_path: path.to_path_buf(),
        })
    }
}

/// The ADO state the PR commands and the PR poll share: one token source
/// (one cache, one sign-in prompt per org) and the store.
pub struct PrState {
    pub tokens: Arc<TokenSource>,
    pub store: Arc<AdoStore>,
    /// Orgs whose background poll stopped after a sign-in failure, so the
    /// poll never opens a sign-in window on its own. A command that works
    /// for the org starts it again.
    paused: Mutex<HashSet<String>>,
}

impl PrState {
    pub fn new(tokens: Arc<TokenSource>, store: Arc<AdoStore>) -> Self {
        Self {
            tokens,
            store,
            paused: Mutex::new(HashSet::new()),
        }
    }

    pub fn is_paused(&self, org_url: &str) -> bool {
        self.paused.lock().unwrap().contains(org_url)
    }

    pub fn pause(&self, org_url: &str) {
        self.paused.lock().unwrap().insert(org_url.to_string());
    }

    pub fn resume(&self, org_url: &str) {
        self.paused.lock().unwrap().remove(org_url);
    }

    /// The context for a worktree, for a user-initiated command. No ADO
    /// remote is an error here.
    pub async fn ctx(&self, path: &Path) -> Result<PrCtx, String> {
        match PrCtx::from_worktree(path, self.tokens.clone()).await {
            Ok(Some(ctx)) => {
                self.resume(&ctx.org_url);
                Ok(ctx)
            }
            Ok(None) => Err("This checkout has no Azure DevOps remote.".into()),
            Err(e) => Err(e.user_message()),
        }
    }
}

/// Whether an error means "sign in", which the background poll must not
/// keep asking for.
pub fn is_auth_error(e: &AdoError) -> bool {
    matches!(e, AdoError::Auth(_) | AdoError::Unauthorized)
}

/// The identity search host for an org URL: `dev.azure.com/{org}` and
/// `{org}.visualstudio.com` both give `https://vssps.dev.azure.com/{org}`.
pub fn vssps_base(org_url: &str) -> Option<String> {
    let rest = org_url
        .trim()
        .strip_prefix("https://")
        .or_else(|| org_url.trim().strip_prefix("http://"))?;
    let mut parts = rest.trim_end_matches('/').split('/');
    let host = parts.next()?.to_ascii_lowercase();
    let org = if host == "dev.azure.com" {
        parts.next()?.to_string()
    } else {
        host.strip_suffix(".visualstudio.com")?.to_string()
    };
    if org.is_empty() {
        return None;
    }
    Some(format!("https://vssps.dev.azure.com/{org}"))
}

fn git_command(cwd: &Path) -> Command {
    let mut cmd = Command::new("git");
    cmd.current_dir(cwd)
        .stdin(Stdio::null())
        .env("GIT_TERMINAL_PROMPT", "0");
    #[cfg(windows)]
    nebula_core::platform::no_window(&mut cmd);
    cmd
}

fn git_out(cwd: &Path, args: &[&str]) -> Option<String> {
    let out = git_command(cwd)
        .args(args)
        .stderr(Stdio::null())
        .output()
        .ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).trim().to_string())
}

/// The ADO remote to use: `origin` when it is one, else the only ADO
/// remote. Several ADO remotes without an ADO `origin` is an error.
pub fn find_ado_remote(repo: &Path) -> Result<Option<(String, AdoRemote)>, String> {
    let Some(names) = git_out(repo, &["remote"]) else {
        return Ok(None);
    };
    let mut found: Vec<(String, AdoRemote)> = names
        .lines()
        .map(str::trim)
        .filter(|n| !n.is_empty())
        .filter_map(|n| {
            let url = git_out(repo, &["remote", "get-url", n])?;
            Some((n.to_string(), parse_remote(&url)?))
        })
        .collect();
    if let Some(i) = found.iter().position(|(n, _)| n == "origin") {
        return Ok(Some(found.swap_remove(i)));
    }
    match found.len() {
        0 => Ok(None),
        1 => Ok(found.pop()),
        _ => Err("This checkout has several Azure DevOps remotes and none is origin.".into()),
    }
}

fn cache<V: Clone>(
    cell: &'static OnceLock<Mutex<HashMap<String, V>>>,
) -> &'static Mutex<HashMap<String, V>> {
    cell.get_or_init(|| Mutex::new(HashMap::new()))
}

static REPO_IDS: OnceLock<Mutex<HashMap<String, (String, String)>>> = OnceLock::new();
static MY_IDS: OnceLock<Mutex<HashMap<String, String>>> = OnceLock::new();

/// The repo GUID and project GUID, cached per repo.
async fn repo_ids(client: &AdoClient, r: &AdoRemote) -> Result<(String, String), AdoError> {
    let key = format!("{}/{}/{}", r.org_url, r.project, r.repo);
    if let Some(hit) = cache(&REPO_IDS).lock().unwrap().get(&key).cloned() {
        return Ok(hit);
    }
    let v: Value = client
        .get(&format!(
            "/{}/_apis/git/repositories/{}",
            enc(&r.project),
            enc(&r.repo)
        ))
        .await?;
    let repo_id = str_at(&v, &["id"]);
    let project_id = str_at(&v, &["project", "id"]);
    if repo_id.is_empty() || project_id.is_empty() {
        return Err(AdoError::NotFound);
    }
    let ids = (repo_id, project_id);
    cache(&REPO_IDS).lock().unwrap().insert(key, ids.clone());
    Ok(ids)
}

/// My identity id in this org (`connectionData`'s `authenticatedUser.id`),
/// cached per org.
pub async fn my_id(client: &AdoClient) -> Result<String, AdoError> {
    let org = client.org_url().to_string();
    if let Some(hit) = cache(&MY_IDS).lock().unwrap().get(&org).cloned() {
        return Ok(hit);
    }
    let v: Value = client
        .get(&format!(
            "/_apis/connectionData?api-version={CONNECTION_API_VERSION}"
        ))
        .await?;
    let id = str_at(&v, &["authenticatedUser", "id"]);
    if id.is_empty() {
        return Err(AdoError::Http(
            200,
            "no signed-in user in connectionData".into(),
        ));
    }
    cache(&MY_IDS).lock().unwrap().insert(org, id.clone());
    Ok(id)
}

/// Percent-encode a path segment or query value.
pub(crate) fn enc(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~') {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

fn at<'a>(v: &'a Value, path: &[&str]) -> &'a Value {
    path.iter().fold(v, |v, k| &v[*k])
}

fn str_at(v: &Value, path: &[&str]) -> String {
    at(v, path).as_str().unwrap_or_default().to_string()
}

fn strip_heads(r: &str) -> String {
    r.strip_prefix("refs/heads/").unwrap_or(r).to_string()
}

// ---------------------------------------------------------------------------
// Read side
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReviewerVote {
    pub id: String,
    pub name: String,
    /// 10 approved, 5 approved with suggestions, 0 no vote, -5 waiting for
    /// author, -10 rejected.
    pub vote: i8,
    pub required: bool,
}

/// The words for a vote, so the UI never shows it as colour alone.
pub fn vote_label(vote: i8) -> &'static str {
    match vote {
        10 => "approved",
        5 => "approved with suggestions",
        -5 => "waiting for author",
        -10 => "rejected",
        _ => "no vote",
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PrSummary {
    pub id: u32,
    pub title: String,
    pub source: String,
    pub target: String,
    pub is_draft: bool,
    pub status: String,
    pub created_by: String,
    pub reviewers: Vec<ReviewerVote>,
    pub work_items: Vec<u32>,
    /// Comment threads, system threads excluded.
    pub threads: u32,
    /// The PR's page in the browser.
    pub url: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum PolicyState {
    Approved,
    Rejected,
    Queued,
    Running,
    NotApplicable,
    NotEvaluated,
}

impl PolicyState {
    fn from_status(s: &str) -> Self {
        match s {
            "approved" => PolicyState::Approved,
            "rejected" | "broken" => PolicyState::Rejected,
            "queued" => PolicyState::Queued,
            "running" => PolicyState::Running,
            "notApplicable" => PolicyState::NotApplicable,
            _ => PolicyState::NotEvaluated,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PolicyRow {
    pub name: String,
    pub state: PolicyState,
    pub blocking: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Comment {
    pub author: String,
    pub text: String,
    pub at: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Thread {
    pub id: u32,
    pub status: String,
    pub file: Option<String>,
    pub line: Option<u32>,
    pub comments: Vec<Comment>,
}

fn pulls_path(repo_id: &str) -> String {
    format!("/_apis/git/repositories/{}/pullrequests", enc(repo_id))
}

/// A PR as the list and get endpoints return it, before work items and
/// thread counts are filled in.
fn summary_from(v: &Value, org_url: &str) -> Option<PrSummary> {
    let id = v.get("pullRequestId")?.as_u64()? as u32;
    let reviewers = v
        .get("reviewers")
        .and_then(Value::as_array)
        .map(|rs| {
            rs.iter()
                .map(|r| ReviewerVote {
                    id: str_at(r, &["id"]),
                    name: str_at(r, &["displayName"]),
                    vote: r
                        .get("vote")
                        .and_then(Value::as_i64)
                        .unwrap_or(0)
                        .clamp(-10, 10) as i8,
                    required: r.get("isRequired").and_then(Value::as_bool) == Some(true),
                })
                .collect()
        })
        .unwrap_or_default();
    let web = str_at(v, &["repository", "webUrl"]);
    let url = if !web.is_empty() {
        format!("{web}/pullrequest/{id}")
    } else {
        format!(
            "{}/{}/_git/{}/pullrequest/{id}",
            org_url.trim_end_matches('/'),
            enc(&str_at(v, &["repository", "project", "name"])),
            enc(&str_at(v, &["repository", "name"]))
        )
    };
    let work_items = v
        .get("workItemRefs")
        .and_then(Value::as_array)
        .map(|w| w.iter().filter_map(work_item_id).collect())
        .unwrap_or_default();
    Some(PrSummary {
        id,
        title: str_at(v, &["title"]),
        source: strip_heads(&str_at(v, &["sourceRefName"])),
        target: strip_heads(&str_at(v, &["targetRefName"])),
        is_draft: v.get("isDraft").and_then(Value::as_bool) == Some(true),
        status: str_at(v, &["status"]),
        created_by: str_at(v, &["createdBy", "displayName"]),
        reviewers,
        work_items,
        threads: 0,
        url,
    })
}

/// A work item ref's id, which ADO sends as a string.
fn work_item_id(w: &Value) -> Option<u32> {
    match w.get("id")? {
        Value::String(s) => s.parse().ok(),
        Value::Number(n) => n.as_u64().map(|n| n as u32),
        _ => None,
    }
}

async fn query(
    client: &AdoClient,
    repo_id: &str,
    criteria: &[(&str, &str)],
) -> Result<Vec<PrSummary>, AdoError> {
    let mut path = format!("{}?searchCriteria.status=active", pulls_path(repo_id));
    for (k, v) in criteria {
        path.push_str(&format!("&searchCriteria.{k}={}", enc(v)));
    }
    let v: Value = client.get(&path).await?;
    Ok(v["value"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|p| summary_from(p, client.org_url()))
                .collect()
        })
        .unwrap_or_default())
}

/// Active PRs from `source` (a branch name), without work items or thread
/// counts.
pub async fn for_source(
    client: &AdoClient,
    repo_id: &str,
    source: &str,
) -> Result<Vec<PrSummary>, AdoError> {
    let r = format!("refs/heads/{}", strip_heads(source));
    query(client, repo_id, &[("sourceRefName", &r)]).await
}

/// The ids of the work items linked to a PR.
pub async fn work_items(
    client: &AdoClient,
    repo_id: &str,
    pr_id: u32,
) -> Result<Vec<u32>, AdoError> {
    let v: Value = client
        .get(&format!("{}/{pr_id}/workitems", pulls_path(repo_id)))
        .await?;
    Ok(v["value"]
        .as_array()
        .map(|a| a.iter().filter_map(work_item_id).collect())
        .unwrap_or_default())
}

/// Fill in work items and the thread count.
async fn hydrate(client: &AdoClient, repo_id: &str, pr: &mut PrSummary) -> Result<(), AdoError> {
    pr.work_items = work_items(client, repo_id, pr.id).await?;
    pr.threads = threads(client, repo_id, pr.id).await?.len() as u32;
    Ok(())
}

/// My active PRs (created by `mine`) and the active ones I review.
pub async fn list(
    client: &AdoClient,
    repo_id: &str,
    mine: &str,
) -> Result<(Vec<PrSummary>, Vec<PrSummary>), AdoError> {
    let mut created = query(client, repo_id, &[("creatorId", mine)]).await?;
    let mut review = query(client, repo_id, &[("reviewerId", mine)]).await?;
    for pr in created.iter_mut().chain(review.iter_mut()) {
        hydrate(client, repo_id, pr).await?;
    }
    Ok((created, review))
}

/// One PR, hydrated.
pub async fn get(client: &AdoClient, repo_id: &str, pr_id: u32) -> Result<PrSummary, AdoError> {
    let v: Value = client
        .get(&format!("{}/{pr_id}", pulls_path(repo_id)))
        .await?;
    let mut pr = summary_from(&v, client.org_url()).ok_or(AdoError::NotFound)?;
    hydrate(client, repo_id, &mut pr).await?;
    Ok(pr)
}

/// The server's policy evaluations for a PR. While the PR is a draft
/// every row is `NotEvaluated`.
pub async fn policies(
    client: &AdoClient,
    project_id: &str,
    pr_id: u32,
    is_draft: bool,
) -> Result<Vec<PolicyRow>, AdoError> {
    let artifact = format!("vstfs:///CodeReview/CodeReviewId/{project_id}/{pr_id}");
    let v: Value = client
        .get(&format!(
            "/{}/_apis/policy/evaluations?artifactId={}&api-version={POLICY_API_VERSION}",
            enc(project_id),
            enc(&artifact)
        ))
        .await?;
    Ok(v["value"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter(|e| at(e, &["configuration", "isEnabled"]).as_bool() != Some(false))
                .map(|e| {
                    let cfg = &e["configuration"];
                    let mut name = str_at(cfg, &["settings", "displayName"]);
                    if name.is_empty() {
                        name = str_at(cfg, &["type", "displayName"]);
                    }
                    PolicyRow {
                        name,
                        state: if is_draft {
                            PolicyState::NotEvaluated
                        } else {
                            PolicyState::from_status(&str_at(e, &["status"]))
                        },
                        blocking: cfg.get("isBlocking").and_then(Value::as_bool) == Some(true),
                    }
                })
                .collect()
        })
        .unwrap_or_default())
}

/// A PR's comment threads, without system threads (votes, pushes, policy
/// notes) or deleted comments.
pub async fn threads(
    client: &AdoClient,
    repo_id: &str,
    pr_id: u32,
) -> Result<Vec<Thread>, AdoError> {
    let v: Value = client
        .get(&format!("{}/{pr_id}/threads", pulls_path(repo_id)))
        .await?;
    Ok(v["value"]
        .as_array()
        .map(|a| a.iter().filter_map(thread_from).collect())
        .unwrap_or_default())
}

fn thread_from(t: &Value) -> Option<Thread> {
    if t.get("isDeleted").and_then(Value::as_bool) == Some(true) {
        return None;
    }
    if t.get("properties")
        .and_then(|p| p.get("CodeReviewThreadType"))
        .is_some()
    {
        return None;
    }
    let comments: Vec<Comment> = t
        .get("comments")
        .and_then(Value::as_array)?
        .iter()
        .filter(|c| c.get("isDeleted").and_then(Value::as_bool) != Some(true))
        .filter(|c| str_at(c, &["commentType"]) != "system")
        .map(|c| Comment {
            author: str_at(c, &["author", "displayName"]),
            text: str_at(c, &["content"]),
            at: str_at(c, &["publishedDate"]),
        })
        .collect();
    if comments.is_empty() {
        return None;
    }
    let ctx = &t["threadContext"];
    let file = ctx
        .get("filePath")
        .and_then(Value::as_str)
        .map(str::to_string);
    let line = ["rightFileStart", "leftFileStart"]
        .iter()
        .find_map(|k| ctx.get(*k)?.get("line")?.as_u64())
        .map(|l| l as u32);
    Some(Thread {
        id: t.get("id")?.as_u64()? as u32,
        status: t
            .get("status")
            .and_then(Value::as_str)
            .unwrap_or("unknown")
            .to_string(),
        file,
        line,
        comments,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ado::auth::{Credential, TokenSource};
    use httpmock::prelude::*;
    use serde_json::json;

    fn client(s: &MockServer) -> AdoClient {
        AdoClient::new(
            &s.base_url(),
            Arc::new(TokenSource::fixed(Credential::Bearer("tok".into()))),
        )
    }

    fn pr_json(id: u32, draft: bool, votes: &[i64]) -> Value {
        json!({
            "pullRequestId": id,
            "title": format!("PR {id}"),
            "sourceRefName": "refs/heads/ja/fix",
            "targetRefName": "refs/heads/main",
            "isDraft": draft,
            "status": "active",
            "createdBy": {"displayName": "Jay Rich", "id": "me-1"},
            "repository": {"id": "r1", "name": "api", "project": {"id": "p1", "name": "Platform"}},
            "reviewers": votes.iter().enumerate().map(|(i, v)| json!({
                "id": format!("u{i}"), "displayName": format!("User {i}"),
                "vote": v, "isRequired": i == 0
            })).collect::<Vec<_>>()
        })
    }

    fn empty_hydration(s: &MockServer, id: u32) {
        s.mock(|w, t| {
            w.method(GET).path(format!(
                "/_apis/git/repositories/r1/pullrequests/{id}/workitems"
            ));
            t.status(200)
                .json_body(json!({"value":[{"id":"42","url":"x"}]}));
        });
        s.mock(|w, t| {
            w.method(GET).path(format!(
                "/_apis/git/repositories/r1/pullrequests/{id}/threads"
            ));
            t.status(200).json_body(json!({"value":[]}));
        });
    }

    #[tokio::test]
    async fn list_queries_carry_creator_and_reviewer() {
        let s = MockServer::start();
        let mine = s.mock(|w, t| {
            w.method(GET)
                .path("/_apis/git/repositories/r1/pullrequests")
                .query_param("searchCriteria.status", "active")
                .query_param("searchCriteria.creatorId", "me-1")
                .query_param("api-version", "7.1");
            t.status(200)
                .json_body(json!({"value":[pr_json(1, false, &[10])]}));
        });
        let review = s.mock(|w, t| {
            w.method(GET)
                .path("/_apis/git/repositories/r1/pullrequests")
                .query_param("searchCriteria.status", "active")
                .query_param("searchCriteria.reviewerId", "me-1");
            t.status(200)
                .json_body(json!({"value":[pr_json(2, true, &[])]}));
        });
        empty_hydration(&s, 1);
        empty_hydration(&s, 2);
        let (m, r) = list(&client(&s), "r1", "me-1").await.unwrap();
        mine.assert();
        review.assert();
        assert_eq!(m.len(), 1);
        assert_eq!(m[0].id, 1);
        assert_eq!(m[0].source, "ja/fix");
        assert_eq!(m[0].target, "main");
        assert_eq!(m[0].created_by, "Jay Rich");
        assert!(m[0].url.ends_with("/Platform/_git/api/pullrequest/1"));
        assert_eq!(r.len(), 1);
        assert!(r[0].is_draft);
    }

    #[tokio::test]
    async fn work_items_come_from_the_workitems_endpoint() {
        let s = MockServer::start();
        s.mock(|w, t| {
            w.method(GET)
                .path("/_apis/git/repositories/r1/pullrequests/7");
            t.status(200).json_body(pr_json(7, false, &[]));
        });
        let wi = s.mock(|w, t| {
            w.method(GET)
                .path("/_apis/git/repositories/r1/pullrequests/7/workitems");
            t.status(200)
                .json_body(json!({"value":[{"id":"42","url":"x"},{"id":"43","url":"y"}]}));
        });
        s.mock(|w, t| {
            w.method(GET)
                .path("/_apis/git/repositories/r1/pullrequests/7/threads");
            t.status(200).json_body(json!({"value":[]}));
        });
        let pr = get(&client(&s), "r1", 7).await.unwrap();
        wi.assert();
        assert_eq!(pr.work_items, vec![42, 43]);
    }

    #[test]
    fn votes_map_all_five_values() {
        let v = pr_json(3, false, &[10, 5, 0, -5, -10]);
        let pr = summary_from(&v, "https://dev.azure.com/tnt").unwrap();
        let votes: Vec<i8> = pr.reviewers.iter().map(|r| r.vote).collect();
        assert_eq!(votes, vec![10, 5, 0, -5, -10]);
        assert!(pr.reviewers[0].required);
        assert!(!pr.reviewers[1].required);
        let labels: Vec<&str> = votes.iter().map(|v| vote_label(*v)).collect();
        assert_eq!(
            labels,
            vec![
                "approved",
                "approved with suggestions",
                "no vote",
                "waiting for author",
                "rejected"
            ]
        );
    }

    fn evaluations() -> Value {
        let row = |status: &str, name: &str, blocking: bool| {
            json!({
                "status": status,
                "configuration": {
                    "isEnabled": true, "isBlocking": blocking,
                    "type": {"displayName": name}, "settings": {}
                }
            })
        };
        json!({"value": [
            row("approved", "Minimum number of reviewers", true),
            row("rejected", "Work item linking", true),
            row("queued", "Build", true),
            row("running", "Status", false),
            row("notApplicable", "Comment requirements", false),
            {"status":"approved","configuration":{"isEnabled":false,"isBlocking":true,"type":{"displayName":"Off"}}}
        ]})
    }

    fn mock_evaluations(s: &MockServer) -> httpmock::Mock<'_> {
        s.mock(|w, t| {
            w.method(GET)
                .path("/p1/_apis/policy/evaluations")
                .query_param("artifactId", "vstfs:///CodeReview/CodeReviewId/p1/9")
                .query_param("api-version", "7.1-preview.1");
            t.status(200).json_body(evaluations());
        })
    }

    #[tokio::test]
    async fn policy_status_maps_to_state() {
        let s = MockServer::start();
        let m = mock_evaluations(&s);
        let rows = policies(&client(&s), "p1", 9, false).await.unwrap();
        m.assert();
        let states: Vec<PolicyState> = rows.iter().map(|r| r.state).collect();
        assert_eq!(
            states,
            vec![
                PolicyState::Approved,
                PolicyState::Rejected,
                PolicyState::Queued,
                PolicyState::Running,
                PolicyState::NotApplicable,
            ]
        );
        assert_eq!(rows[0].name, "Minimum number of reviewers");
        assert!(rows[0].blocking);
        assert!(!rows[3].blocking);
    }

    #[tokio::test]
    async fn a_draft_is_not_evaluated() {
        let s = MockServer::start();
        mock_evaluations(&s);
        let rows = policies(&client(&s), "p1", 9, true).await.unwrap();
        assert_eq!(rows.len(), 5);
        assert!(rows.iter().all(|r| r.state == PolicyState::NotEvaluated));
    }

    #[tokio::test]
    async fn system_threads_are_dropped() {
        let s = MockServer::start();
        s.mock(|w, t| {
            w.method(GET)
                .path("/_apis/git/repositories/r1/pullrequests/5/threads");
            t.status(200).json_body(json!({"value": [
                {
                    "id": 1, "status": "active",
                    "threadContext": {"filePath": "/src/a.rs", "rightFileStart": {"line": 12, "offset": 1}},
                    "comments": [{"author":{"displayName":"Sam Lee"},"content":"Why?","publishedDate":"2026-10-14T01:00:00Z","commentType":"text"}],
                    "properties": {}
                },
                {
                    "id": 2,
                    "comments": [{"author":{"displayName":"Sam Lee"},"content":"Sam Lee voted 10","commentType":"system"}],
                    "properties": {"CodeReviewThreadType": {"$type":"System.String","$value":"VoteUpdate"}}
                },
                {
                    "id": 3, "status": "fixed",
                    "comments": [{"author":{"displayName":"Jay"},"content":"General note","publishedDate":"2026-10-14T02:00:00Z","commentType":"text"}]
                }
            ]}));
        });
        let ts = threads(&client(&s), "r1", 5).await.unwrap();
        assert_eq!(ts.iter().map(|t| t.id).collect::<Vec<_>>(), vec![1, 3]);
        assert_eq!(ts[0].file.as_deref(), Some("/src/a.rs"));
        assert_eq!(ts[0].line, Some(12));
        assert_eq!(ts[0].comments[0].author, "Sam Lee");
        assert_eq!(ts[1].file, None);
        assert_eq!(ts[1].status, "fixed");
    }

    #[tokio::test]
    async fn my_id_comes_from_connection_data_once() {
        let s = MockServer::start();
        let m = s.mock(|w, t| {
            w.method(GET).path("/_apis/connectionData");
            t.status(200)
                .json_body(json!({"authenticatedUser":{"id":"me-9","providerDisplayName":"Jay"}}));
        });
        let c = client(&s);
        assert_eq!(my_id(&c).await.unwrap(), "me-9");
        assert_eq!(my_id(&c).await.unwrap(), "me-9");
        assert_eq!(m.hits(), 1);
    }

    #[test]
    fn vssps_from_either_org_url() {
        assert_eq!(
            vssps_base("https://dev.azure.com/tnt").as_deref(),
            Some("https://vssps.dev.azure.com/tnt")
        );
        assert_eq!(
            vssps_base("https://tnt.visualstudio.com/").as_deref(),
            Some("https://vssps.dev.azure.com/tnt")
        );
        assert_eq!(vssps_base("https://github.com/x"), None);
    }
}
