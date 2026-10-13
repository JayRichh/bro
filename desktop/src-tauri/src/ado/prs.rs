//! Azure DevOps pull requests: lists, votes, policies and threads.

use crate::ado::auth::{AuthError, TokenSource};
use crate::ado::client::{AdoClient, AdoError};
use crate::ado::remote::{parse_remote, AdoRemote};
use crate::ado::store::{AdoStore, RepoKey, Reviewer};
use crate::attribution::strip_ai_trailers;
use serde::{Deserialize, Serialize};
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
            .map_err(|e| AdoError::Local(e.to_string()))?
            .map_err(AdoError::Local)?;
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
        Self::build(path, remote_name, remote, maker(tokens, false)).await
    }

    /// As [`from_remote`](Self::from_remote), but its client never opens a
    /// sign-in window: for the background PR poll.
    pub async fn from_remote_silent(
        path: &Path,
        remote_name: String,
        remote: AdoRemote,
        tokens: Arc<TokenSource>,
    ) -> Result<PrCtx, AdoError> {
        Self::build(path, remote_name, remote, maker(tokens, true)).await
    }

    async fn build(
        path: &Path,
        remote_name: String,
        remote: AdoRemote,
        make: impl FnOnce(&str) -> AdoClient,
    ) -> Result<PrCtx, AdoError> {
        let client = make(&remote.org_url);
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
        let p = path.to_path_buf();
        let found = tokio::task::spawn_blocking(move || find_ado_remote(&p))
            .await
            .map_err(|e| e.to_string())??;
        let Some((name, remote)) = found else {
            return Err("This checkout has no Azure DevOps remote.".into());
        };
        let ctx = PrCtx::from_remote(path, name, remote, self.tokens.clone())
            .await
            .map_err(|e| e.user_message())?;
        self.resume(&ctx.org_url);
        Ok(ctx)
    }
}

/// Whether an error means "sign in", which the background poll must not
/// keep asking for.
pub fn is_auth_error(e: &AdoError) -> bool {
    matches!(e, AdoError::Auth(_) | AdoError::Unauthorized)
}

fn maker(tokens: Arc<TokenSource>, silent: bool) -> impl FnOnce(&str) -> AdoClient {
    move |org_url| {
        let c = AdoClient::new(org_url, tokens);
        if silent {
            c.silent()
        } else {
            c
        }
    }
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
        return Err(AdoError::Auth(AuthError::NoCredential));
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
/// Fill in work items and the thread count, handing back the threads.
async fn hydrate(
    client: &AdoClient,
    repo_id: &str,
    pr: &mut PrSummary,
) -> Result<Vec<Thread>, AdoError> {
    pr.work_items = work_items(client, repo_id, pr.id).await?;
    let ts = threads(client, repo_id, pr.id).await?;
    pr.threads = ts.len() as u32;
    Ok(ts)
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

/// One PR, hydrated, with its threads (fetched once).
pub async fn get(
    client: &AdoClient,
    repo_id: &str,
    pr_id: u32,
) -> Result<(PrSummary, Vec<Thread>), AdoError> {
    let v: Value = client
        .get(&format!("{}/{pr_id}", pulls_path(repo_id)))
        .await?;
    let mut pr = summary_from(&v, client.org_url()).ok_or(AdoError::NotFound)?;
    let ts = hydrate(client, repo_id, &mut pr).await?;
    Ok((pr, ts))
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

// ---------------------------------------------------------------------------
// Write side
// ---------------------------------------------------------------------------

/// What the composer sends.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NewPr {
    pub source: String,
    pub target: String,
    pub title: String,
    pub description: String,
    pub draft: bool,
    pub reviewers: Vec<Reviewer>,
    pub work_item: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CreateOutcome {
    Created(u32),
    /// The PR exists but the task link failed; the reason is for the UI's
    /// "Created, task not linked" line with Retry.
    CreatedTaskNotLinked(u32, String),
    /// An active PR from that branch already existed; nothing was created.
    Existing(u32),
}

/// What [`create`] hands the composer: the outcome and, when commits
/// already on the server carry co-author lines, a one-line note saying
/// they were left alone.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CreateResult {
    pub outcome: CreateOutcome,
    pub note: Option<String>,
}

/// The note for co-author lines in commits that were already pushed.
pub const PUSHED_TRAILERS_NOTE: &str =
    "Some commits on the server still carry co-author lines; they were left as they are.";

impl Serialize for CreateResult {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeStruct;
        let (kind, id, reason) = match &self.outcome {
            CreateOutcome::Created(id) => ("created", id, None),
            CreateOutcome::CreatedTaskNotLinked(id, why) => {
                ("createdTaskNotLinked", id, Some(why.as_str()))
            }
            CreateOutcome::Existing(id) => ("existing", id, None),
        };
        let mut st = s.serialize_struct("CreateResult", 4)?;
        st.serialize_field("kind", kind)?;
        st.serialize_field("id", id)?;
        st.serialize_field("reason", &reason)?;
        st.serialize_field("note", &self.note)?;
        st.end()
    }
}

/// How long the fetch of the PR target may take before the clean goes
/// ahead without it.
const FETCH_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(60);

/// Fetch `target` from `remote` into its remote-tracking ref, never
/// asking for a sign-in, and return that ref when it exists afterwards.
/// A failed or slow fetch is not an error: an earlier fetch may have left
/// the ref, and without it the clean is bounded by the remotes alone.
async fn fetch_target(repo: &Path, remote: &str, target: &str) -> Option<String> {
    let tracking = format!("refs/remotes/{remote}/{target}");
    let refspec = format!("+refs/heads/{target}:{tracking}");
    let args = [
        "-c",
        "credential.interactive=never",
        "fetch",
        "--quiet",
        "--no-tags",
        "--end-of-options",
        remote,
        &refspec,
    ];
    let _ = tokio::time::timeout(FETCH_TIMEOUT, crate::ado::git_out(repo, &args)).await;
    crate::ado::git_out(
        repo,
        &[
            "rev-parse",
            "--verify",
            "--quiet",
            &format!("{tracking}^{{commit}}"),
        ],
    )
    .await
    .ok()
    .map(|_| tracking)
}

/// The note when commits between `base` and the pushed `source` still
/// carry co-author lines. After the clean, any that do were already on
/// the server before this push.
fn pushed_trailers_note(repo: &Path, base: &str, remote: &str, source: &str) -> Option<String> {
    let range = format!("{base}..refs/remotes/{remote}/{source}");
    match crate::attribution::find_ai_commits(repo, &range) {
        Ok(found) if !found.is_empty() => Some(PUSHED_TRAILERS_NOTE.to_string()),
        _ => None,
    }
}

/// `git push --set-upstream <remote> <source>` in `repo`.
fn push(repo: &Path, remote: &str, source: &str) -> Result<(), AdoError> {
    let out = git_command(repo)
        .args(["push", "--set-upstream", remote, source])
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .output()
        .map_err(|e| AdoError::Local(format!("Could not run git: {e}")))?;
    if out.status.success() {
        return Ok(());
    }
    let stderr = String::from_utf8_lossy(&out.stderr);
    let why = stderr
        .lines()
        .map(str::trim)
        .find(|l| l.starts_with("error:") || l.starts_with("fatal:") || l.starts_with('!'))
        .or_else(|| stderr.lines().map(str::trim).find(|l| !l.is_empty()))
        .unwrap_or("unknown error");
    Err(AdoError::Local(format!(
        "The git push of {source} to {remote} failed: {why}"
    )))
}

/// A create whose outcome is unknown: the PR may exist anyway.
fn ambiguous(e: &AdoError) -> bool {
    matches!(e, AdoError::Network(_) | AdoError::Conflict(_))
        || matches!(e, AdoError::Http(code, _) if *code >= 500)
}

fn pr_artifact(ctx: &PrCtx, pr_id: u32) -> String {
    format!(
        "vstfs:///Git/PullRequestId/{}%2F{}%2F{pr_id}",
        ctx.project_id, ctx.repo_id
    )
}

/// Link work item `task` to a PR by its artifact id.
async fn link_artifact(ctx: &PrCtx, task: u32, artifact: &str) -> Result<(), AdoError> {
    let body = serde_json::json!([{
        "op": "add",
        "path": "/relations/-",
        "value": {
            "rel": "ArtifactLink",
            "url": artifact,
            "attributes": {"name": "Pull Request"}
        }
    }]);
    let _: Value = ctx
        .client
        .patch_json_patch(&format!("/_apis/wit/workitems/{task}"), &body)
        .await?;
    Ok(())
}

/// Link `task` to PR `pr_id` (the Retry for "Created, task not linked").
pub async fn link_task(ctx: &PrCtx, pr_id: u32, task: u32) -> Result<(), AdoError> {
    if work_items(&ctx.client, &ctx.repo_id, pr_id)
        .await?
        .contains(&task)
    {
        return Ok(());
    }
    let v: Value = ctx
        .client
        .get(&format!("{}/{pr_id}", pulls_path(&ctx.repo_id)))
        .await?;
    let artifact = v
        .get("artifactId")
        .and_then(Value::as_str)
        .map(str::to_string)
        .unwrap_or_else(|| pr_artifact(ctx, pr_id));
    link_artifact(ctx, task, &artifact).await
}

/// Push `pr.source` and open a pull request for it, never twice:
/// 1. fetch the target, then clean co-author lines and footers from the
///    branch's unpushed commits (bounded by the target when it is there,
///    and by the remote-tracking refs either way),
/// 2. `git push --set-upstream`; pushed commits that still carry
///    co-author lines give the result its note,
/// 3. an active PR from the branch already there is returned as `Existing`,
/// 4. POST the PR with reviewers and the task link,
/// 5. a create that may have landed (timeout, 409, 5xx) is reconciled by
///    listing again, never by posting again,
/// 6. a task the reply does not show as linked is linked with a PATCH; if
///    that fails the PR is still returned, as `CreatedTaskNotLinked`.
pub async fn create(ctx: &PrCtx, repo_path: &Path, pr: NewPr) -> Result<CreateResult, AdoError> {
    let source = strip_heads(pr.source.trim());
    let target = strip_heads(pr.target.trim());
    if source.is_empty() || target.is_empty() {
        return Err(AdoError::Local(
            "A pull request needs a source and a target branch.".into(),
        ));
    }
    if pr.title.trim().is_empty() {
        return Err(AdoError::Local("Add a title".into()));
    }

    let base = fetch_target(repo_path, &ctx.remote_name, &target).await;
    let (repo, remote, src) = (
        repo_path.to_path_buf(),
        ctx.remote_name.clone(),
        source.clone(),
    );
    let note = tokio::task::spawn_blocking(move || -> Result<Option<String>, AdoError> {
        crate::attribution::rewrite_unpushed(&repo, base.as_deref(), &src)
            .map_err(|e| AdoError::Local(format!("Could not clean the commit messages: {e}")))?;
        push(&repo, &remote, &src)?;
        // Without the target there is no telling the branch's own commits
        // from the target's history, so no note.
        Ok(base.and_then(|b| pushed_trailers_note(&repo, &b, &remote, &src)))
    })
    .await
    .map_err(|e| AdoError::Local(e.to_string()))??;
    let outcome = open(ctx, &source, &target, &pr).await?;
    Ok(CreateResult { outcome, note })
}

/// Steps 3 to 6 of [`create`], once the branch is on the server.
async fn open(
    ctx: &PrCtx,
    source: &str,
    target: &str,
    pr: &NewPr,
) -> Result<CreateOutcome, AdoError> {
    if let Some(p) = for_source(&ctx.client, &ctx.repo_id, source).await?.first() {
        return Ok(CreateOutcome::Existing(p.id));
    }

    let (description, _) = strip_ai_trailers(&pr.description);
    let mut body = serde_json::json!({
        "sourceRefName": format!("refs/heads/{source}"),
        "targetRefName": format!("refs/heads/{target}"),
        "title": pr.title.trim(),
        "description": description,
        "isDraft": pr.draft,
        "reviewers": pr.reviewers.iter().map(|r| serde_json::json!({"id": r.id})).collect::<Vec<_>>(),
    });
    if let Some(task) = pr.work_item {
        body["workItemRefs"] = serde_json::json!([{ "id": task.to_string() }]);
    }

    let reply: Value = match ctx.client.post_json(&pulls_path(&ctx.repo_id), &body).await {
        Ok(v) => v,
        Err(e) if ambiguous(&e) => {
            return match for_source(&ctx.client, &ctx.repo_id, source).await {
                Ok(found) if !found.is_empty() => Ok(CreateOutcome::Existing(found[0].id)),
                _ => Err(e),
            };
        }
        Err(e) => return Err(e),
    };
    let id = reply
        .get("pullRequestId")
        .and_then(Value::as_u64)
        .ok_or_else(|| AdoError::Http(200, "the reply had no pull request id".into()))?
        as u32;

    if let Some(task) = pr.work_item {
        let in_reply = reply
            .get("workItemRefs")
            .and_then(Value::as_array)
            .is_some_and(|w| w.iter().filter_map(work_item_id).any(|w| w == task));
        // The create reply may not echo workItemRefs; ask before linking,
        // so an existing link is never added twice.
        let linked = if in_reply {
            true
        } else {
            match work_items(&ctx.client, &ctx.repo_id, id).await {
                Ok(w) => w.contains(&task),
                // Unknown: linking blind could add the link twice, so
                // leave it to Retry.
                Err(e) => {
                    return Ok(CreateOutcome::CreatedTaskNotLinked(
                        id,
                        format!("Could not check the task link: {}", e.user_message()),
                    ))
                }
            }
        };
        if !linked {
            let artifact = reply
                .get("artifactId")
                .and_then(Value::as_str)
                .map(str::to_string)
                .unwrap_or_else(|| pr_artifact(ctx, id));
            if let Err(e) = link_artifact(ctx, task, &artifact).await {
                return Ok(CreateOutcome::CreatedTaskNotLinked(id, e.user_message()));
            }
        }
    }
    Ok(CreateOutcome::Created(id))
}

/// Take a draft out of draft.
pub async fn publish(ctx: &PrCtx, id: u32) -> Result<(), AdoError> {
    let _: Value = ctx
        .client
        .patch_json(
            &format!("{}/{id}", pulls_path(&ctx.repo_id)),
            &serde_json::json!({"isDraft": false}),
        )
        .await?;
    Ok(())
}

/// Vote on a PR as me: 10, 5, 0, -5 or -10.
pub async fn vote(ctx: &PrCtx, id: u32, vote: i8) -> Result<(), AdoError> {
    if ![10, 5, 0, -5, -10].contains(&vote) {
        return Err(AdoError::Local(format!("{vote} is not a vote")));
    }
    let pr: Value = ctx
        .client
        .get(&format!("{}/{id}", pulls_path(&ctx.repo_id)))
        .await?;
    if pr.get("isDraft").and_then(Value::as_bool) == Some(true) {
        return Err(AdoError::Local(
            "Voting opens when the PR is published".into(),
        ));
    }
    let me = my_id(&ctx.client).await?;
    let _: Value = ctx
        .client
        .put_json(
            &format!("{}/{id}/reviewers/{}", pulls_path(&ctx.repo_id), enc(&me)),
            &serde_json::json!({"vote": vote}),
        )
        .await?;
    Ok(())
}

/// Add a comment to a thread.
pub async fn reply(ctx: &PrCtx, id: u32, thread: u32, text: &str) -> Result<Comment, AdoError> {
    let (text, _) = strip_ai_trailers(text);
    let text = text.trim_end();
    if text.is_empty() {
        return Err(AdoError::Local("The reply is empty".into()));
    }
    let v: Value = ctx
        .client
        .post_json(
            &format!(
                "{}/{id}/threads/{thread}/comments",
                pulls_path(&ctx.repo_id)
            ),
            &serde_json::json!({"content": text, "commentType": 1}),
        )
        .await?;
    Ok(Comment {
        author: str_at(&v, &["author", "displayName"]),
        text: str_at(&v, &["content"]),
        at: str_at(&v, &["publishedDate"]),
    })
}

/// The identity search URL under a vssps base.
pub fn identities_url(vssps_base: &str, q: &str) -> String {
    format!(
        "{}/_apis/identities?searchFilter=General&filterValue={}",
        vssps_base.trim_end_matches('/'),
        enc(q)
    )
}

/// People (and groups) matching `q`, for the reviewer picker. Email is
/// included so look-alike names can be told apart.
pub async fn search_identities(ctx: &PrCtx, q: &str) -> Result<Vec<Reviewer>, AdoError> {
    let q = q.trim();
    if q.is_empty() {
        return Ok(Vec::new());
    }
    let v: Value = ctx.client.get(&identities_url(&ctx.vssps_base, q)).await?;
    Ok(v["value"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter(|i| i.get("isActive").and_then(Value::as_bool) != Some(false))
                .filter_map(|i| {
                    let id = str_at(i, &["id"]);
                    if id.is_empty() {
                        return None;
                    }
                    let mut name = str_at(i, &["providerDisplayName"]);
                    if name.is_empty() {
                        name = str_at(i, &["customDisplayName"]);
                    }
                    let mut email = str_at(i, &["properties", "Mail", "$value"]);
                    if email.is_empty() {
                        email = str_at(i, &["properties", "Account", "$value"]);
                    }
                    Some(Reviewer { id, name, email })
                })
                .collect()
        })
        .unwrap_or_default())
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
        let (pr, _) = get(&client(&s), "r1", 7).await.unwrap();
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

    // --- write side -------------------------------------------------------

    /// A work repo on branch `ja/fix`, one commit ahead of `main`, whose
    /// `origin` is a bare repo holding `main`.
    struct TempRepo {
        root: PathBuf,
        work: PathBuf,
        bare: PathBuf,
    }

    impl Drop for TempRepo {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.root);
        }
    }

    fn git(cwd: &Path, args: &[&str]) -> String {
        let out = Command::new("git")
            .current_dir(cwd)
            .args([
                "-c",
                "user.name=Jay Rich",
                "-c",
                "user.email=jay@example.com",
                "-c",
                "commit.gpgsign=false",
                "-c",
                "core.hooksPath=",
                "-c",
                "core.autocrlf=false",
                "-c",
                "init.defaultBranch=main",
            ])
            .args(args)
            .env("GIT_TERMINAL_PROMPT", "0")
            .output()
            .expect("git");
        assert!(
            out.status.success(),
            "git {args:?}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8_lossy(&out.stdout).trim().to_string()
    }

    fn temp_repo() -> TempRepo {
        static N: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
        let n = N.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let root = std::env::temp_dir().join(format!("kete-prs-{}-{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let (work, bare) = (root.join("work"), root.join("remote.git"));
        std::fs::create_dir_all(&work).unwrap();
        std::fs::create_dir_all(&bare).unwrap();
        git(&bare, &["init", "--bare", "-q"]);
        git(&work, &["init", "-q"]);
        std::fs::write(work.join("a.txt"), "one\n").unwrap();
        git(&work, &["add", "."]);
        git(&work, &["commit", "-q", "-m", "base"]);
        git(&work, &["remote", "add", "origin", bare.to_str().unwrap()]);
        git(&work, &["push", "-q", "origin", "main"]);
        git(&work, &["checkout", "-q", "-b", "ja/fix"]);
        std::fs::write(work.join("a.txt"), "two\n").unwrap();
        git(&work, &["commit", "-q", "-am", "Fix rounding"]);
        TempRepo { root, work, bare }
    }

    fn remote_has(bare: &Path, branch: &str) -> bool {
        Command::new("git")
            .current_dir(bare)
            .args([
                "rev-parse",
                "--verify",
                "-q",
                &format!("refs/heads/{branch}"),
            ])
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }

    fn ctx_for(base: &str, repo: &Path) -> PrCtx {
        PrCtx {
            client: AdoClient::new(
                base,
                Arc::new(TokenSource::fixed(Credential::Bearer("tok".into()))),
            ),
            repo_id: "r1".into(),
            project_id: "p1".into(),
            org_url: base.into(),
            remote_name: "origin".into(),
            repo_path: repo.to_path_buf(),
            vssps_base: "/vssps/tnt".into(),
            key: RepoKey {
                org: "tnt".into(),
                project: "Platform".into(),
                repo: "api".into(),
            },
        }
    }

    fn new_pr(work_item: Option<u32>, description: &str) -> NewPr {
        NewPr {
            source: "ja/fix".into(),
            target: "main".into(),
            title: "Fix rounding".into(),
            description: description.into(),
            draft: true,
            reviewers: vec![Reviewer {
                id: "u1".into(),
                name: "Sam Lee".into(),
                email: "sam@example.com".into(),
            }],
            work_item,
        }
    }

    fn mock_no_existing(s: &MockServer) -> httpmock::Mock<'_> {
        s.mock(|w, t| {
            w.method(GET)
                .path("/_apis/git/repositories/r1/pullrequests")
                .query_param("searchCriteria.status", "active")
                .query_param("searchCriteria.sourceRefName", "refs/heads/ja/fix");
            t.status(200).json_body(json!({"value":[]}));
        })
    }

    #[tokio::test]
    async fn create_pushes_then_posts() {
        let r = temp_repo();
        let s = MockServer::start();
        let list = mock_no_existing(&s);
        let post = s.mock(|w, t| {
            w.method(POST)
                .path("/_apis/git/repositories/r1/pullrequests")
                .json_body_partial(
                    r#"{"sourceRefName":"refs/heads/ja/fix","targetRefName":"refs/heads/main","isDraft":true,"reviewers":[{"id":"u1"}]}"#,
                );
            t.status(201).json_body(json!({"pullRequestId": 5}));
        });
        assert!(!remote_has(&r.bare, "ja/fix"));
        let out = create(&ctx_for(&s.base_url(), &r.work), &r.work, new_pr(None, "x"))
            .await
            .unwrap();
        assert_eq!(
            out,
            CreateResult {
                outcome: CreateOutcome::Created(5),
                note: None
            }
        );
        assert!(remote_has(&r.bare, "ja/fix"));
        assert_eq!(
            git(&r.work, &["rev-parse", "--abbrev-ref", "ja/fix@{upstream}"]),
            "origin/ja/fix"
        );
        list.assert();
        post.assert();
    }

    #[tokio::test]
    async fn a_failed_push_never_posts() {
        let r = temp_repo();
        let s = MockServer::start();
        let list = mock_no_existing(&s);
        let post = s.mock(|w, t| {
            w.method(POST)
                .path("/_apis/git/repositories/r1/pullrequests");
            t.status(201).json_body(json!({"pullRequestId": 5}));
        });
        let mut ctx = ctx_for(&s.base_url(), &r.work);
        ctx.remote_name = "nowhere".into();
        let e = create(&ctx, &r.work, new_pr(None, "x")).await.unwrap_err();
        // The push itself failed: the fetch of the target could not run
        // either, so the clean went ahead on the remote refs alone.
        assert!(
            matches!(&e, AdoError::Local(m) if m.starts_with("The git push of ja/fix to nowhere failed")),
            "{e}"
        );
        assert!(!e.user_message().contains("Azure DevOps"), "{e}");
        assert_eq!(list.hits(), 0);
        assert_eq!(post.hits(), 0);
    }

    #[tokio::test]
    async fn an_existing_pr_short_circuits() {
        let r = temp_repo();
        let s = MockServer::start();
        s.mock(|w, t| {
            w.method(GET)
                .path("/_apis/git/repositories/r1/pullrequests")
                .query_param("searchCriteria.sourceRefName", "refs/heads/ja/fix");
            t.status(200)
                .json_body(json!({"value":[pr_json(77, true, &[])]}));
        });
        let post = s.mock(|w, t| {
            w.method(POST)
                .path("/_apis/git/repositories/r1/pullrequests");
            t.status(201).json_body(json!({"pullRequestId": 5}));
        });
        let out = create(&ctx_for(&s.base_url(), &r.work), &r.work, new_pr(None, "x"))
            .await
            .unwrap();
        assert_eq!(out.outcome, CreateOutcome::Existing(77));
        assert_eq!(post.hits(), 0);
    }

    /// Serves `script` in order, one connection per request: `Some(json)`
    /// answers 200, `None` hangs up without answering. Returns the base
    /// URL and the request lines seen.
    async fn scripted(
        script: Vec<Option<Value>>,
    ) -> (String, tokio::task::JoinHandle<Vec<String>>) {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let handle = tokio::spawn(async move {
            let mut seen = Vec::new();
            for step in script {
                let (mut sock, _) = listener.accept().await.unwrap();
                let mut buf = Vec::new();
                let mut chunk = [0u8; 4096];
                loop {
                    let n = sock.read(&mut chunk).await.unwrap();
                    buf.extend_from_slice(&chunk[..n]);
                    let text = String::from_utf8_lossy(&buf).to_string();
                    if let Some(end) = text.find("\r\n\r\n") {
                        let len = text[..end]
                            .lines()
                            .find_map(|l| {
                                let (k, v) = l.split_once(':')?;
                                k.eq_ignore_ascii_case("content-length")
                                    .then(|| v.trim().parse::<usize>().ok())?
                            })
                            .unwrap_or(0);
                        if buf.len() >= end + 4 + len || n == 0 {
                            seen.push(text.lines().next().unwrap_or("").to_string());
                            break;
                        }
                    }
                    if n == 0 {
                        break;
                    }
                }
                match step {
                    Some(body) => {
                        let body = body.to_string();
                        let reply = format!(
                            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                            body.len()
                        );
                        sock.write_all(reply.as_bytes()).await.unwrap();
                        sock.shutdown().await.unwrap();
                    }
                    None => drop(sock),
                }
            }
            seen
        });
        (base, handle)
    }

    #[tokio::test]
    async fn an_ambiguous_create_is_reconciled_by_listing() {
        let r = temp_repo();
        let (base, server) = scripted(vec![
            Some(json!({"value": []})),
            None,
            Some(json!({"value": [pr_json(88, true, &[])]})),
        ])
        .await;
        let out = create(&ctx_for(&base, &r.work), &r.work, new_pr(None, "x"))
            .await
            .unwrap();
        assert_eq!(out.outcome, CreateOutcome::Existing(88));
        let seen = server.await.unwrap();
        assert_eq!(seen.len(), 3);
        assert_eq!(seen.iter().filter(|l| l.starts_with("POST ")).count(), 1);
        assert!(seen[2].starts_with("GET "));
    }

    #[tokio::test]
    async fn work_item_refs_are_sent() {
        let r = temp_repo();
        let s = MockServer::start();
        mock_no_existing(&s);
        let post = s.mock(|w, t| {
            w.method(POST)
                .path("/_apis/git/repositories/r1/pullrequests")
                .json_body_partial(r#"{"workItemRefs":[{"id":"42"}]}"#);
            t.status(201)
                .json_body(json!({"pullRequestId": 6, "workItemRefs": [{"id": "42"}]}));
        });
        let patch = s.mock(|w, t| {
            w.method(httpmock::Method::PATCH)
                .path("/_apis/wit/workitems/42");
            t.status(200).json_body(json!({}));
        });
        let out = create(
            &ctx_for(&s.base_url(), &r.work),
            &r.work,
            new_pr(Some(42), "x"),
        )
        .await
        .unwrap();
        assert_eq!(out.outcome, CreateOutcome::Created(6));
        post.assert();
        assert_eq!(patch.hits(), 0);
    }

    #[tokio::test]
    async fn a_failed_link_is_created_task_not_linked() {
        let r = temp_repo();
        let s = MockServer::start();
        mock_no_existing(&s);
        s.mock(|w, t| {
            w.method(POST)
                .path("/_apis/git/repositories/r1/pullrequests");
            t.status(201).json_body(json!({
                "pullRequestId": 9,
                "artifactId": "vstfs:///Git/PullRequestId/p1%2Fr1%2F9"
            }));
        });
        s.mock(|w, t| {
            w.method(GET)
                .path("/_apis/git/repositories/r1/pullrequests/9/workitems");
            t.status(200).json_body(json!({"value": []}));
        });
        let patch = s.mock(|w, t| {
            w.method(httpmock::Method::PATCH)
                .path("/_apis/wit/workitems/42")
                .header("content-type", "application/json-patch+json")
                .json_body(json!([{
                    "op": "add",
                    "path": "/relations/-",
                    "value": {
                        "rel": "ArtifactLink",
                        "url": "vstfs:///Git/PullRequestId/p1%2Fr1%2F9",
                        "attributes": {"name": "Pull Request"}
                    }
                }]));
            t.status(400).json_body(json!({"message": "link refused"}));
        });
        let out = create(
            &ctx_for(&s.base_url(), &r.work),
            &r.work,
            new_pr(Some(42), "x"),
        )
        .await
        .unwrap();
        patch.assert();
        assert!(
            matches!(&out.outcome, CreateOutcome::CreatedTaskNotLinked(9, why) if why.contains("link refused")),
            "{out:?}"
        );
    }

    #[tokio::test]
    async fn the_description_loses_its_ai_footer() {
        let r = temp_repo();
        let s = MockServer::start();
        mock_no_existing(&s);
        let post = s.mock(|w, t| {
            w.method(POST)
                .path("/_apis/git/repositories/r1/pullrequests")
                .json_body_partial(r#"{"description":"Summary\n"}"#);
            t.status(201).json_body(json!({"pullRequestId": 3}));
        });
        let desc =
            "Summary\n\n\u{1f916} Generated with [Claude Code](https://claude.com/claude-code)\n";
        create(
            &ctx_for(&s.base_url(), &r.work),
            &r.work,
            new_pr(None, desc),
        )
        .await
        .unwrap();
        post.assert();
    }

    #[test]
    fn create_result_serialises_with_a_kind() {
        assert_eq!(
            serde_json::to_value(CreateResult {
                outcome: CreateOutcome::CreatedTaskNotLinked(4, "no".into()),
                note: None
            })
            .unwrap(),
            json!({"kind": "createdTaskNotLinked", "id": 4, "reason": "no", "note": null})
        );
        assert_eq!(
            serde_json::to_value(CreateResult {
                outcome: CreateOutcome::Existing(2),
                note: Some(PUSHED_TRAILERS_NOTE.into())
            })
            .unwrap(),
            json!({"kind": "existing", "id": 2, "reason": null, "note": PUSHED_TRAILERS_NOTE})
        );
    }

    fn mock_post(s: &MockServer, id: u32) -> httpmock::Mock<'_> {
        s.mock(|w, t| {
            w.method(POST)
                .path("/_apis/git/repositories/r1/pullrequests");
            t.status(201).json_body(json!({ "pullRequestId": id }));
        })
    }

    #[tokio::test]
    async fn create_fetches_the_target_before_the_clean() {
        let r = temp_repo();
        // The target moved on the server and was never fetched here; the
        // local tracking ref is gone too.
        git(&r.work, &["update-ref", "-d", "refs/remotes/origin/main"]);
        let other = r.root.join("other");
        git(
            &r.root,
            &[
                "clone",
                "-q",
                r.bare.to_str().unwrap(),
                other.to_str().unwrap(),
            ],
        );
        std::fs::write(other.join("b.txt"), "b\n").unwrap();
        git(&other, &["add", "."]);
        git(&other, &["commit", "-q", "-m", "Elsewhere"]);
        git(&other, &["push", "-q", "origin", "main"]);
        let server_main = git(&r.bare, &["rev-parse", "main"]);
        git(
            &r.work,
            &[
                "commit",
                "-q",
                "--allow-empty",
                "-m",
                "More\n\nCo-authored-by: Sam Lee <sam@tnt.co.nz>",
            ],
        );

        let s = MockServer::start();
        mock_no_existing(&s);
        let post = mock_post(&s, 5);
        let out = create(&ctx_for(&s.base_url(), &r.work), &r.work, new_pr(None, "x"))
            .await
            .unwrap();
        post.assert();
        assert_eq!(out.outcome, CreateOutcome::Created(5));
        assert_eq!(out.note, None);
        assert_eq!(git(&r.work, &["rev-parse", "origin/main"]), server_main);
        assert_eq!(
            git(&r.bare, &["log", "-1", "--format=%B", "ja/fix"]),
            "More"
        );
    }

    #[tokio::test]
    async fn a_target_found_nowhere_still_pushes_and_posts() {
        let r = temp_repo();
        let s = MockServer::start();
        mock_no_existing(&s);
        let post = mock_post(&s, 6);
        let mut pr = new_pr(None, "x");
        pr.target = "release/never".into();
        let out = create(&ctx_for(&s.base_url(), &r.work), &r.work, pr)
            .await
            .unwrap();
        post.assert();
        assert_eq!(out.outcome, CreateOutcome::Created(6));
        assert_eq!(out.note, None);
        assert!(remote_has(&r.bare, "ja/fix"));
    }

    #[tokio::test]
    async fn co_author_lines_already_pushed_are_left_with_a_note() {
        let r = temp_repo();
        // A commit with a co-author line reached the server before Kete
        // saw the branch; a plain one follows locally.
        git(
            &r.work,
            &[
                "commit",
                "-q",
                "--allow-empty",
                "-m",
                "Early\n\nCo-authored-by: Sam Lee <sam@tnt.co.nz>",
            ],
        );
        git(&r.work, &["push", "-q", "-u", "origin", "ja/fix"]);
        let pushed = git(&r.bare, &["rev-parse", "ja/fix"]);
        git(&r.work, &["commit", "-q", "--allow-empty", "-m", "Later"]);

        let s = MockServer::start();
        mock_no_existing(&s);
        mock_post(&s, 7);
        let out = create(&ctx_for(&s.base_url(), &r.work), &r.work, new_pr(None, "x"))
            .await
            .unwrap();
        assert_eq!(out.outcome, CreateOutcome::Created(7));
        assert_eq!(out.note.as_deref(), Some(PUSHED_TRAILERS_NOTE));
        assert_eq!(
            PUSHED_TRAILERS_NOTE,
            "Some commits on the server still carry co-author lines; they were left as they are."
        );
        // The pushed commit is exactly as it was, under the new tip.
        assert_eq!(git(&r.bare, &["rev-parse", "ja/fix~1"]), pushed);
        assert_eq!(
            git(&r.bare, &["log", "-1", "--format=%B", "ja/fix"]),
            "Later"
        );
    }

    #[tokio::test]
    async fn vote_puts_my_vote() {
        let s = MockServer::start();
        s.mock(|w, t| {
            w.method(GET).path("/_apis/connectionData");
            t.status(200)
                .json_body(json!({"authenticatedUser": {"id": "me-1"}}));
        });
        let put = s.mock(|w, t| {
            w.method(PUT)
                .path("/_apis/git/repositories/r1/pullrequests/5/reviewers/me-1")
                .json_body(json!({"vote": 10}));
            t.status(200).json_body(json!({"vote": 10}));
        });
        s.mock(|w, t| {
            w.method(GET)
                .path("/_apis/git/repositories/r1/pullrequests/5");
            t.status(200).json_body(pr_json(5, false, &[]));
        });
        let ctx = ctx_for(&s.base_url(), Path::new("."));
        vote(&ctx, 5, 10).await.unwrap();
        put.assert();
        assert!(matches!(vote(&ctx, 5, 7).await, Err(AdoError::Local(_))));
    }

    #[tokio::test]
    async fn publish_patches_is_draft_false() {
        let s = MockServer::start();
        let m = s.mock(|w, t| {
            w.method(httpmock::Method::PATCH)
                .path("/_apis/git/repositories/r1/pullrequests/5")
                .json_body(json!({"isDraft": false}));
            t.status(200).json_body(json!({"pullRequestId": 5}));
        });
        publish(&ctx_for(&s.base_url(), Path::new(".")), 5)
            .await
            .unwrap();
        m.assert();
    }

    #[tokio::test]
    async fn reply_posts_to_the_thread() {
        let s = MockServer::start();
        let m = s.mock(|w, t| {
            w.method(POST)
                .path("/_apis/git/repositories/r1/pullrequests/5/threads/3/comments")
                .json_body(json!({"content": "Done", "commentType": 1}));
            t.status(200).json_body(json!({
                "content": "Done", "author": {"displayName": "Jay"},
                "publishedDate": "2026-10-14T03:00:00Z"
            }));
        });
        let c = reply(&ctx_for(&s.base_url(), Path::new(".")), 5, 3, "Done")
            .await
            .unwrap();
        m.assert();
        assert_eq!(c.author, "Jay");
    }

    #[test]
    fn identity_search_url() {
        let base = vssps_base("https://tnt.visualstudio.com").unwrap();
        assert_eq!(
            identities_url(&base, "sam"),
            "https://vssps.dev.azure.com/tnt/_apis/identities?searchFilter=General&filterValue=sam"
        );
    }

    #[tokio::test]
    async fn identity_search_hits_vssps() {
        let s = MockServer::start();
        let m = s.mock(|w, t| {
            w.method(GET)
                .path("/vssps/tnt/_apis/identities")
                .query_param("searchFilter", "General")
                .query_param("filterValue", "sam")
                .query_param("api-version", "7.1");
            t.status(200).json_body(json!({"value": [
                {"id": "u1", "providerDisplayName": "Sam Lee", "isActive": true,
                 "properties": {"Mail": {"$type": "System.String", "$value": "sam@tnt.co.nz"}}},
                {"id": "u2", "providerDisplayName": "Sam Gone", "isActive": false}
            ]}));
        });
        let found = search_identities(&ctx_for(&s.base_url(), Path::new(".")), "sam")
            .await
            .unwrap();
        m.assert();
        assert_eq!(
            found,
            vec![Reviewer {
                id: "u1".into(),
                name: "Sam Lee".into(),
                email: "sam@tnt.co.nz".into()
            }]
        );
    }

    #[tokio::test]
    async fn an_empty_title_is_refused_locally() {
        let r = temp_repo();
        let s = MockServer::start();
        let list = mock_no_existing(&s);
        let mut pr = new_pr(None, "x");
        pr.title = "  ".into();
        let e = create(&ctx_for(&s.base_url(), &r.work), &r.work, pr)
            .await
            .unwrap_err();
        assert_eq!(e, AdoError::Local("Add a title".into()));
        assert_eq!(e.user_message(), "Add a title");
        assert_eq!(list.hits(), 0);
        assert!(!remote_has(&r.bare, "ja/fix"));
    }

    #[tokio::test]
    async fn an_unknown_link_state_is_not_linked_without_a_patch() {
        let r = temp_repo();
        let s = MockServer::start();
        mock_no_existing(&s);
        s.mock(|w, t| {
            w.method(POST)
                .path("/_apis/git/repositories/r1/pullrequests");
            t.status(201).json_body(json!({"pullRequestId": 11}));
        });
        s.mock(|w, t| {
            w.method(GET)
                .path("/_apis/git/repositories/r1/pullrequests/11/workitems");
            t.status(500).json_body(json!({"message": "busy"}));
        });
        let patch = s.mock(|w, t| {
            w.method(httpmock::Method::PATCH)
                .path("/_apis/wit/workitems/42");
            t.status(200).json_body(json!({}));
        });
        let out = create(
            &ctx_for(&s.base_url(), &r.work),
            &r.work,
            new_pr(Some(42), "x"),
        )
        .await
        .unwrap();
        assert!(
            matches!(&out.outcome, CreateOutcome::CreatedTaskNotLinked(11, why) if why.contains("busy")),
            "{out:?}"
        );
        assert_eq!(patch.hits(), 0);
    }

    #[tokio::test]
    async fn voting_on_a_draft_is_refused() {
        let s = MockServer::start();
        s.mock(|w, t| {
            w.method(GET).path("/_apis/connectionData");
            t.status(200)
                .json_body(json!({"authenticatedUser": {"id": "me-1"}}));
        });
        s.mock(|w, t| {
            w.method(GET)
                .path("/_apis/git/repositories/r1/pullrequests/5");
            t.status(200).json_body(pr_json(5, true, &[]));
        });
        let put = s.mock(|w, t| {
            w.method(PUT)
                .path("/_apis/git/repositories/r1/pullrequests/5/reviewers/me-1");
            t.status(200).json_body(json!({}));
        });
        let e = vote(&ctx_for(&s.base_url(), Path::new(".")), 5, 10)
            .await
            .unwrap_err();
        assert_eq!(
            e,
            AdoError::Local("Voting opens when the PR is published".into())
        );
        assert_eq!(put.hits(), 0);
    }

    #[tokio::test]
    async fn no_signed_in_user_is_an_auth_error() {
        let s = MockServer::start();
        s.mock(|w, t| {
            w.method(GET).path("/_apis/connectionData");
            t.status(200).json_body(json!({"authenticatedUser": {}}));
        });
        assert_eq!(
            my_id(&client(&s)).await.unwrap_err(),
            AdoError::Auth(AuthError::NoCredential)
        );
    }

    #[tokio::test]
    async fn an_empty_reply_is_refused_locally() {
        let s = MockServer::start();
        let e = reply(&ctx_for(&s.base_url(), Path::new(".")), 5, 3, " \n")
            .await
            .unwrap_err();
        assert!(matches!(e, AdoError::Local(_)));
    }
}
