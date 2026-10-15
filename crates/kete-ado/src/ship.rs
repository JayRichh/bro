//! Ship: a worktree's branch on to a pull request and on to merged, on
//! whichever forge its remote is. GitHub goes through `gh` (its own
//! sign-in), Azure DevOps through this crate's client and PR create. One
//! implementation for the terminal app, `kete ship` and the desktop window.
//!
//! Nothing here types a credential or opens a prompt: `gh` runs with its
//! prompts off, git with terminal prompts off, ADO through the token
//! sources the rest of the crate uses. A missing sign-in is a
//! [`ShipErrorKind::SignIn`] the UI shows with a button.
//!
//! The pieces that decide something are pure (forge detection, `gh`'s
//! arguments, the PR it reports, the title and body suggestion) and every
//! process this module starts itself goes through a [`Runner`], so tests
//! never reach a real `gh`, `az` or remote.

use crate::client::{AdoError, ErrorKind};
use crate::prs::{self, PrCtx};
use crate::remote::{parse_remote, AdoRemote};
use crate::{AdoContext, AdoState};
use async_trait::async_trait;
use nebula_core::review_base::{self, CommitLine};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

// ---------------------------------------------------------------------------
// Forges
// ---------------------------------------------------------------------------

/// Which forge a worktree ships to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ForgeKind {
    #[serde(rename = "github")]
    GitHub,
    Ado,
}

/// A GitHub repo, as named by its git remote.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitHubRepo {
    /// `github.com`, or an Enterprise host.
    pub host: String,
    pub owner: String,
    pub repo: String,
}

impl GitHubRepo {
    /// `gh --repo`'s `[HOST/]OWNER/REPO`: the host only off github.com.
    pub fn gh_repo(&self) -> String {
        if self.host == "github.com" {
            format!("{}/{}", self.owner, self.repo)
        } else {
            format!("{}/{}/{}", self.host, self.owner, self.repo)
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Forge {
    GitHub(GitHubRepo),
    Ado(AdoRemote),
}

impl Forge {
    pub fn kind(&self) -> ForgeKind {
        match self {
            Forge::GitHub(_) => ForgeKind::GitHub,
            Forge::Ado(_) => ForgeKind::Ado,
        }
    }
}

/// The forge a remote URL points at: Azure DevOps first (its own parser),
/// then a GitHub host (`github.com` or a host with `github` in its name,
/// as Enterprise ones are). None for anything else.
pub fn detect_forge(url: &str) -> Option<Forge> {
    if let Some(r) = parse_remote(url) {
        return Some(Forge::Ado(r));
    }
    parse_github(url).map(Forge::GitHub)
}

/// `https://[user@]host/owner/repo[.git]`, `ssh://git@host[:port]/owner/repo`
/// and `git@host:owner/repo[.git]`, for a GitHub host.
pub fn parse_github(url: &str) -> Option<GitHubRepo> {
    let url = url.trim();
    let (authority, path) = if let Some(rest) = ["https://", "http://", "ssh://", "git://"]
        .iter()
        .find_map(|p| url.strip_prefix(p))
    {
        rest.split_once('/')?
    } else {
        // scp-like: [user@]host:owner/repo
        let (authority, path) = url.split_once(':')?;
        if authority.contains('/') {
            return None;
        }
        (authority, path)
    };
    let host = authority.rsplit_once('@').map_or(authority, |(_, h)| h);
    let host = host.split(':').next()?.to_ascii_lowercase();
    if host != "github.com" && !host.contains("github") {
        return None;
    }
    let mut segs = path.trim_matches('/').split('/');
    let owner = segs.next().filter(|s| !s.is_empty())?;
    let repo = segs.next()?.trim_end_matches(".git");
    if repo.is_empty() || segs.next().is_some() {
        return None;
    }
    Some(GitHubRepo {
        host,
        owner: owner.to_string(),
        repo: repo.to_string(),
    })
}

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ShipErrorKind {
    /// Not signed in to the forge: `gh auth login`, or the ADO sign-in.
    SignIn,
    /// `gh` is not installed.
    NoTool,
    /// No remote on a forge this knows.
    NoForge,
    /// HEAD is not on a branch, or the branch cannot be shipped (the base
    /// itself, the default branch).
    NoBranch,
    /// No pull request to act on.
    NoPr,
    /// Azure DevOps would not complete the PR here (policies, a draft):
    /// finish it on the PR's page, `url`.
    MergeInAdo,
    /// The forge could not be reached.
    Offline,
    Other,
}

/// A ship failure as the UI gets it: `{ kind, forge, message, url }`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ShipError {
    pub kind: ShipErrorKind,
    pub forge: Option<ForgeKind>,
    pub message: String,
    /// The page to finish on, when there is one.
    pub url: Option<String>,
}

impl ShipError {
    pub fn new(kind: ShipErrorKind, forge: Option<ForgeKind>, message: impl Into<String>) -> Self {
        Self {
            kind,
            forge,
            message: message.into(),
            url: None,
        }
    }

    fn other(forge: Option<ForgeKind>, message: impl Into<String>) -> Self {
        Self::new(ShipErrorKind::Other, forge, message)
    }

    fn with_url(mut self, url: &str) -> Self {
        self.url = Some(url.to_string()).filter(|u| !u.is_empty());
        self
    }

    fn from_ado(e: AdoError) -> Self {
        let kind = match e.kind() {
            ErrorKind::Auth => ShipErrorKind::SignIn,
            ErrorKind::Offline => ShipErrorKind::Offline,
            _ => ShipErrorKind::Other,
        };
        Self::new(kind, Some(ForgeKind::Ado), e.user_message())
    }
}

impl fmt::Display for ShipError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for ShipError {}

/// What to tell someone `gh` refused for want of a sign-in.
pub const GH_SIGN_IN: &str = "GitHub sign-in needed: run `gh auth login`, then try again.";

/// `gh`'s ways of saying it has no usable sign-in.
pub fn gh_needs_sign_in(stderr: &str) -> bool {
    let s = stderr.to_ascii_lowercase();
    s.contains("gh auth login")
        || s.contains("not logged in")
        || s.contains("authentication required")
        || s.contains("bad credentials")
        || s.contains("http 401")
}

/// The line of `gh`'s stderr worth showing.
fn first_line(stderr: &str) -> &str {
    stderr
        .lines()
        .map(str::trim)
        .find(|l| !l.is_empty())
        .unwrap_or("unknown error")
}

fn gh_error(out: &RunOut, what: &str) -> ShipError {
    let gh = Some(ForgeKind::GitHub);
    if gh_needs_sign_in(&out.stderr) {
        return ShipError::new(ShipErrorKind::SignIn, gh, GH_SIGN_IN);
    }
    let line = first_line(&out.stderr);
    let lower = line.to_ascii_lowercase();
    if lower.contains("could not resolve host") || lower.contains("connection refused") {
        return ShipError::new(
            ShipErrorKind::Offline,
            gh,
            format!("Could not reach GitHub: {line}"),
        );
    }
    ShipError::other(gh, format!("{what} failed: {line}"))
}

// ---------------------------------------------------------------------------
// Running processes
// ---------------------------------------------------------------------------

/// A finished process: did it exit 0, and what it printed.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RunOut {
    pub ok: bool,
    pub stdout: String,
    pub stderr: String,
}

/// Runs `gh` and the few git commands this module starts itself. The real
/// one spawns the process; tests answer from a script.
#[async_trait]
pub trait Runner: Send + Sync {
    /// `program args` in `cwd`, `stdin` fed in. An `Err` only when it could
    /// not run at all (`NotFound`: not installed).
    async fn run(
        &self,
        program: &str,
        args: &[String],
        cwd: &Path,
        stdin: Option<&str>,
    ) -> std::io::Result<RunOut>;
}

/// How long one `gh` or git call may take.
const RUN_TIMEOUT: Duration = Duration::from_secs(120);

/// Spawns the process with no console window, every prompt off, and kills
/// it after [`RUN_TIMEOUT`].
pub struct ProcessRunner;

#[async_trait]
impl Runner for ProcessRunner {
    async fn run(
        &self,
        program: &str,
        args: &[String],
        cwd: &Path,
        stdin: Option<&str>,
    ) -> std::io::Result<RunOut> {
        use std::process::Stdio;
        use tokio::io::AsyncWriteExt;
        let mut cmd = tokio::process::Command::new(program);
        cmd.args(args)
            .current_dir(cwd)
            .stdin(if stdin.is_some() {
                Stdio::piped()
            } else {
                Stdio::null()
            })
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .env("GH_PROMPT_DISABLED", "1")
            .env("GH_NO_UPDATE_NOTIFIER", "1")
            .env("NO_COLOR", "1")
            .env("GIT_TERMINAL_PROMPT", "0")
            .env("GCM_INTERACTIVE", "never")
            .kill_on_drop(true);
        #[cfg(windows)]
        cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
        let mut child = cmd.spawn()?;
        if let (Some(input), Some(mut pipe)) = (stdin, child.stdin.take()) {
            pipe.write_all(input.as_bytes()).await?;
            drop(pipe);
        }
        let out = tokio::time::timeout(RUN_TIMEOUT, child.wait_with_output())
            .await
            .map_err(|_| {
                std::io::Error::new(
                    std::io::ErrorKind::TimedOut,
                    format!("{program} took longer than {} s", RUN_TIMEOUT.as_secs()),
                )
            })??;
        Ok(RunOut {
            ok: out.status.success(),
            stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
        })
    }
}

// ---------------------------------------------------------------------------
// What the UI gets
// ---------------------------------------------------------------------------

/// A pull request, either forge's.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PrInfo {
    pub forge: ForgeKind,
    /// GitHub's number, or ADO's id.
    pub number: u64,
    pub url: String,
    pub title: String,
    /// `open`, `merged` or `closed`.
    pub state: String,
    pub draft: bool,
    /// Its source branch.
    pub head: String,
    /// The branch it merges into.
    pub base: String,
    /// The source commit it is at, when the forge said.
    pub head_sha: Option<String>,
}

impl PrInfo {
    pub fn is_open(&self) -> bool {
        self.state == "open"
    }
}

/// A title and description for a new pull request.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Suggestion {
    pub title: String,
    pub body: String,
}

/// Where a worktree stands for shipping.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ShipStatus {
    /// None: no remote on a forge this knows.
    pub forge: Option<ForgeKind>,
    /// The remote it pushes to.
    pub remote: Option<String>,
    /// The checked-out branch; None on a detached HEAD.
    pub branch: Option<String>,
    /// The branch a new PR merges into: the review base.
    pub base: Option<String>,
    /// Commits on HEAD no remote-tracking branch has; None with no remote.
    pub unpushed: Option<u32>,
    /// The branch's pull request, open first, else its newest.
    pub pr: Option<PrInfo>,
    /// Why `pr` could not be read (a sign-in, no `gh`): the rest stands.
    pub error: Option<ShipError>,
}

/// What `create_pr` is asked for.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NewPr {
    pub title: String,
    #[serde(default)]
    pub body: String,
    #[serde(default)]
    pub draft: bool,
    /// The branch it merges into; None is the review base.
    #[serde(default)]
    pub base: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Created {
    pub pr: PrInfo,
    /// An open PR from the branch was already there: the push updated it
    /// and nothing new was opened.
    pub existing: bool,
    /// A note for the UI (pushed commits that still carry co-author lines).
    pub note: Option<String>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MergeMethod {
    #[default]
    Squash,
    Merge,
    Rebase,
}

impl MergeMethod {
    pub fn parse(word: &str) -> Option<Self> {
        match word.trim().to_ascii_lowercase().as_str() {
            "squash" => Some(Self::Squash),
            "merge" => Some(Self::Merge),
            "rebase" => Some(Self::Rebase),
            _ => None,
        }
    }

    /// `gh pr merge`'s flag.
    fn gh_flag(self) -> &'static str {
        match self {
            Self::Squash => "--squash",
            Self::Merge => "--merge",
            Self::Rebase => "--rebase",
        }
    }

    /// ADO's `completionOptions.mergeStrategy`.
    fn ado_strategy(self) -> &'static str {
        match self {
            Self::Squash => "squash",
            Self::Merge => "noFastForward",
            Self::Rebase => "rebase",
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MergeOpts {
    #[serde(default)]
    pub method: MergeMethod,
    /// Delete the branch on the remote once merged. The local branch and
    /// its worktree stay: removing them is the worktree delete's job.
    #[serde(default)]
    pub delete_branch: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Merged {
    pub pr: PrInfo,
    /// `merged`, or `queued` when the forge took it behind its checks.
    pub state: String,
    pub branch_deleted: bool,
    pub note: Option<String>,
}

// ---------------------------------------------------------------------------
// The title and body
// ---------------------------------------------------------------------------

/// The longest suggested title.
const TITLE_MAX: usize = 72;
/// The most of the prompt the Summary keeps.
const SUMMARY_MAX: usize = 1200;

/// A title and description from what the session and the branch say:
/// - title: the session's name unless it is a default `agent-N`, else a
///   lone commit's subject, else the prompt's first line, else the
///   branch's words;
/// - body: `## Summary` (the prompt) and `## Changes` (the commits since
///   the base, oldest first), each only when there is something to say.
///
/// `commits` come newest first, as [`review_base::commits_since`] lists them.
pub fn suggest_title_body(
    session_name: Option<&str>,
    last_prompt: Option<&str>,
    branch: Option<&str>,
    commits: &[CommitLine],
) -> Suggestion {
    let named = session_name
        .map(str::trim)
        .filter(|n| !n.is_empty() && !is_default_name(n));
    let prompt = last_prompt
        .map(|p| {
            crate::attribution::strip_ai_trailers(p)
                .0
                .trim()
                .to_string()
        })
        .filter(|p| !p.is_empty());
    let prompt = prompt.as_deref();
    let first_line = prompt
        .and_then(|p| p.lines().map(str::trim).find(|l| !l.is_empty()))
        .map(|l| clip(l, TITLE_MAX));
    let title = named
        .map(str::to_string)
        .or_else(|| match commits {
            [one] if !one.subject.trim().is_empty() => Some(one.subject.trim().to_string()),
            _ => None,
        })
        .or(first_line)
        .or_else(|| branch.map(branch_words).filter(|w| !w.is_empty()))
        .unwrap_or_else(|| "Changes".to_string());
    let mut parts = Vec::new();
    if let Some(p) = prompt {
        parts.push(format!("## Summary\n{}", clip(p, SUMMARY_MAX)));
    }
    if !commits.is_empty() {
        let lines: Vec<String> = commits
            .iter()
            .rev()
            .map(|c| format!("- {}", c.subject.trim()))
            .collect();
        parts.push(format!("## Changes\n{}", lines.join("\n")));
    }
    Suggestion {
        title,
        body: parts.join("\n\n"),
    }
}

/// `agent-3`: the name a session has until it is titled.
fn is_default_name(name: &str) -> bool {
    name.strip_prefix("agent-")
        .is_some_and(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()))
}

/// `text` cut to at most `max` characters, at a word when one ends close.
fn clip(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_string();
    }
    let cut: String = text.chars().take(max).collect();
    let at = cut
        .rfind(char::is_whitespace)
        .filter(|i| *i > max / 2)
        .unwrap_or(cut.len());
    format!("{}…", cut[..at].trim_end())
}

/// `jr/fix-login_redirect` → `Fix login redirect`.
fn branch_words(branch: &str) -> String {
    let leaf = branch.rsplit('/').next().unwrap_or(branch);
    let words: Vec<&str> = leaf
        .split(['-', '_', '.'])
        .filter(|w| !w.is_empty())
        .collect();
    let mut out = words.join(" ");
    if let Some(first) = out.get(..1) {
        out = first.to_uppercase() + &out[1..];
    }
    out
}

// ---------------------------------------------------------------------------
// gh: the arguments and what it says
// ---------------------------------------------------------------------------

/// The fields read from `gh pr list`.
pub const GH_PR_FIELDS: &str =
    "number,url,title,state,isDraft,headRefName,headRefOid,baseRefName,isCrossRepository";

fn strings(args: &[&str]) -> Vec<String> {
    args.iter().map(|a| a.to_string()).collect()
}

/// `gh pr list` for the PRs whose head is `branch`, in any state.
pub fn gh_list_args(repo: &GitHubRepo, branch: &str) -> Vec<String> {
    strings(&[
        "pr",
        "list",
        "--repo",
        &repo.gh_repo(),
        "--head",
        branch,
        "--state",
        "all",
        "--limit",
        "5",
        "--json",
        GH_PR_FIELDS,
    ])
}

/// `gh pr create`; the body comes on stdin (`--body-file -`), so a long or
/// many-line one never meets the command line's quoting.
pub fn gh_create_args(
    repo: &GitHubRepo,
    head: &str,
    base: &str,
    title: &str,
    draft: bool,
) -> Vec<String> {
    let mut args = strings(&[
        "pr",
        "create",
        "--repo",
        &repo.gh_repo(),
        "--head",
        head,
        "--base",
        base,
        "--title",
        title,
        "--body-file",
        "-",
    ]);
    if draft {
        args.push("--draft".into());
    }
    args
}

/// `gh pr merge`, always with its method (gh would ask otherwise) and,
/// when known, pinned to the head commit that was looked at. Never
/// `--delete-branch`: that also deletes the local branch, which a linked
/// worktree has checked out.
pub fn gh_merge_args(
    repo: &GitHubRepo,
    number: u64,
    method: MergeMethod,
    head_sha: Option<&str>,
) -> Vec<String> {
    let mut args = strings(&[
        "pr",
        "merge",
        &number.to_string(),
        "--repo",
        &repo.gh_repo(),
        method.gh_flag(),
    ]);
    if let Some(sha) = head_sha.filter(|s| !s.is_empty()) {
        args.push("--match-head-commit".into());
        args.push(sha.to_string());
    }
    args
}

/// The PR `branch` shows, from `gh pr list --json` output: its head is
/// `branch` in this repository (a fork's PR from its own branch of that
/// name is not this one), an open one before any other (a branch reopened
/// after an old merge), else the newest. gh's states become `open`,
/// `merged` or `closed`; one it adds later reads as open.
pub fn pick_gh_pr(json: &str, branch: &str) -> Option<PrInfo> {
    let list: Vec<Value> = serde_json::from_str(json).ok()?;
    let state = |v: &Value| match v.get("state").and_then(Value::as_str) {
        Some("MERGED") => "merged",
        Some("CLOSED") => "closed",
        _ => "open",
    };
    let number = |v: &Value| v.get("number").and_then(Value::as_u64).unwrap_or(0);
    let s = |v: &Value, k: &str| {
        v.get(k)
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string()
    };
    let v = list
        .iter()
        .filter(|v| v.get("headRefName").and_then(Value::as_str) == Some(branch))
        .filter(|v| v.get("isCrossRepository").and_then(Value::as_bool) != Some(true))
        .min_by_key(|v| (state(v) != "open", std::cmp::Reverse(number(v))))?;
    Some(PrInfo {
        forge: ForgeKind::GitHub,
        number: v.get("number")?.as_u64()?,
        url: v.get("url")?.as_str()?.to_string(),
        title: s(v, "title"),
        state: state(v).to_string(),
        draft: v.get("isDraft").and_then(Value::as_bool).unwrap_or(false),
        head: branch.to_string(),
        base: s(v, "baseRefName"),
        head_sha: Some(s(v, "headRefOid")).filter(|h| !h.is_empty()),
    })
}

/// The PR number at the end of the URL `gh pr create` prints.
pub fn number_from_url(url: &str) -> Option<u64> {
    url.trim()
        .trim_end_matches('/')
        .rsplit('/')
        .next()?
        .parse()
        .ok()
}

// ---------------------------------------------------------------------------
// The worktree
// ---------------------------------------------------------------------------

/// Where shipping acts: a checkout, and the `worktree_base_branch` setting
/// its review base is read with.
#[derive(Debug, Clone)]
pub struct Place {
    pub path: PathBuf,
    pub base_setting: String,
}

impl Place {
    pub fn new(path: impl Into<PathBuf>, base_setting: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            base_setting: base_setting.into(),
        }
    }

    /// The checked-out branch; None on a detached HEAD.
    pub async fn branch(&self) -> Option<String> {
        crate::git_out(&self.path, &["symbolic-ref", "--short", "-q", "HEAD"])
            .await
            .ok()
            .map(|b| b.trim().to_string())
            .filter(|b| !b.is_empty())
    }

    /// The review base branch (`main`, `develop`), as both apps' diffs read it.
    pub async fn base_branch(&self) -> Option<String> {
        let (path, setting) = (self.path.clone(), self.base_setting.clone());
        tokio::task::spawn_blocking(move || review_base::resolve(&path, &setting))
            .await
            .ok()
            .flatten()
            .map(|b| b.branch)
    }

    /// The commits since the review base, newest first.
    pub async fn commits(&self) -> Vec<CommitLine> {
        let (path, setting) = (self.path.clone(), self.base_setting.clone());
        tokio::task::spawn_blocking(move || {
            review_base::resolve(&path, &setting)
                .map(|b| review_base::commits_since(&path, &b.sha, review_base::MAX_COMMITS))
                .unwrap_or_default()
        })
        .await
        .unwrap_or_default()
    }

    async fn unpushed(&self) -> Option<u32> {
        crate::git_out(
            &self.path,
            &["rev-list", "--count", "HEAD", "--not", "--remotes"],
        )
        .await
        .ok()
        .and_then(|n| n.trim().parse().ok())
    }

    /// [`suggest_title_body`] for this checkout: its branch and the commits
    /// since its base, with the session's name and last prompt.
    pub async fn suggest(
        &self,
        session_name: Option<&str>,
        last_prompt: Option<&str>,
    ) -> Suggestion {
        let branch = self.branch().await;
        let commits = self.commits().await;
        suggest_title_body(session_name, last_prompt, branch.as_deref(), &commits)
    }
}

/// The checkout's forge and the remote it is on: the ADO remote when there
/// is one (as the rest of this crate picks it), else `origin`, else the
/// only remote, when that is on GitHub.
async fn find_forge(path: &Path) -> Result<(String, Forge), ShipError> {
    if let Ok(Some((name, remote))) = crate::ado_remote(path).await {
        return Ok((name, Forge::Ado(remote)));
    }
    let p = path.to_path_buf();
    let name = tokio::task::spawn_blocking(move || crate::branches::pick_remote(&p))
        .await
        .unwrap_or_default();
    let none = || {
        ShipError::new(
            ShipErrorKind::NoForge,
            None,
            "This repo has no GitHub or Azure DevOps remote to ship to.",
        )
    };
    if name.is_empty() {
        return Err(none());
    }
    let url = crate::git_out(path, &["remote", "get-url", &name])
        .await
        .map_err(|_| none())?;
    match detect_forge(&url) {
        Some(forge) => Ok((name, forge)),
        None => Err(none()),
    }
}

// ---------------------------------------------------------------------------
// Shipping
// ---------------------------------------------------------------------------

/// The shipping commands over one runner and the app's ADO state.
pub struct Ship<'a> {
    pub runner: Arc<dyn Runner>,
    pub ado: &'a AdoState,
    /// Background or command-line use: ADO never opens a sign-in window
    /// (a missing sign-in is the error), as `kete ado` works.
    pub silent: bool,
}

impl<'a> Ship<'a> {
    pub fn new(ado: &'a AdoState, silent: bool) -> Self {
        Self {
            runner: Arc::new(ProcessRunner),
            ado,
            silent,
        }
    }

    pub fn with_runner(mut self, runner: Arc<dyn Runner>) -> Self {
        self.runner = runner;
        self
    }

    async fn gh(
        &self,
        cwd: &Path,
        args: Vec<String>,
        stdin: Option<&str>,
    ) -> Result<RunOut, ShipError> {
        self.runner.run("gh", &args, cwd, stdin).await.map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                ShipError::new(
                    ShipErrorKind::NoTool,
                    Some(ForgeKind::GitHub),
                    "The GitHub CLI (gh) is not installed: install it and run `gh auth login`.",
                )
            } else {
                ShipError::other(Some(ForgeKind::GitHub), format!("gh: {e}"))
            }
        })
    }

    async fn ado_context(&self, path: &Path) -> Result<AdoContext, ShipError> {
        let missing = |e: String| ShipError::new(ShipErrorKind::NoForge, Some(ForgeKind::Ado), e);
        if self.silent {
            self.ado
                .silent_context(path)
                .await
                .map_err(missing)?
                .ok_or_else(|| missing("This repo has no Azure DevOps remote.".into()))
        } else {
            self.ado.context(path).await.map_err(missing)
        }
    }

    async fn ado_pr_ctx(&self, path: &Path) -> Result<PrCtx, ShipError> {
        let ado = self.ado_context(path).await?;
        PrCtx::new(ado).await.map_err(ShipError::from_ado)
    }

    /// The branch, or why there is none to ship.
    async fn branch(place: &Place, forge: ForgeKind) -> Result<String, ShipError> {
        place.branch().await.ok_or_else(|| {
            ShipError::new(
                ShipErrorKind::NoBranch,
                Some(forge),
                "HEAD is not on a branch here: check a branch out to ship it.",
            )
        })
    }

    /// Where the worktree stands: forge, branch, base, unpushed commits and
    /// its pull request. Reading the PR can fail (a sign-in, no `gh`)
    /// without failing the rest: that goes in `error`.
    pub async fn status(&self, place: &Place) -> ShipStatus {
        let mut out = ShipStatus {
            branch: place.branch().await,
            base: place.base_branch().await,
            unpushed: place.unpushed().await,
            ..ShipStatus::default()
        };
        let (remote, forge) = match find_forge(&place.path).await {
            Ok(found) => found,
            Err(e) => {
                out.error = Some(e);
                return out;
            }
        };
        out.forge = Some(forge.kind());
        out.remote = Some(remote);
        if let Some(branch) = out.branch.clone() {
            match self.pr_for(&place.path, &forge, &branch).await {
                Ok(pr) => out.pr = pr,
                Err(e) => out.error = Some(e),
            }
        }
        out
    }

    /// The branch's pull request: open first, else the newest (GitHub);
    /// the active one (ADO lists only those).
    pub async fn pr_status(&self, place: &Place) -> Result<Option<PrInfo>, ShipError> {
        let (_, forge) = find_forge(&place.path).await?;
        let branch = Self::branch(place, forge.kind()).await?;
        self.pr_for(&place.path, &forge, &branch).await
    }

    async fn pr_for(
        &self,
        path: &Path,
        forge: &Forge,
        branch: &str,
    ) -> Result<Option<PrInfo>, ShipError> {
        match forge {
            Forge::GitHub(repo) => {
                let out = self.gh(path, gh_list_args(repo, branch), None).await?;
                if !out.ok {
                    return Err(gh_error(&out, "gh pr list"));
                }
                Ok(pick_gh_pr(&out.stdout, branch))
            }
            Forge::Ado(_) => {
                let ctx = self.ado_pr_ctx(path).await?;
                let found = prs::for_source(&ctx.ado.client, &ctx.repo_id, branch)
                    .await
                    .map_err(ShipError::from_ado)?;
                Ok(found.into_iter().next().map(|p| ado_info(&p)))
            }
        }
    }

    /// The base a new PR goes into: `asked`, else the review base, else
    /// (ADO) the repo's default branch.
    async fn base_for(
        &self,
        place: &Place,
        forge: ForgeKind,
        asked: Option<&str>,
    ) -> Result<String, ShipError> {
        if let Some(b) = asked.map(str::trim).filter(|b| !b.is_empty()) {
            return Ok(b.strip_prefix("refs/heads/").unwrap_or(b).to_string());
        }
        if let Some(b) = place.base_branch().await {
            return Ok(b);
        }
        if forge == ForgeKind::Ado {
            let ado = self.ado_context(&place.path).await?;
            if let Ok(Some(b)) = ado.default_branch_ref().await {
                let b = b
                    .strip_prefix(&format!("{}/", ado.remote_name))
                    .unwrap_or(&b)
                    .to_string();
                return Ok(b);
            }
        }
        Err(ShipError::new(
            ShipErrorKind::NoBranch,
            Some(forge),
            "No base branch was found: name the branch the pull request goes into.",
        ))
    }

    /// Push the branch, co-author lines cleaned from its unpushed commits
    /// first, with its upstream set. The note when commits already on the
    /// remote still carry co-author lines.
    pub async fn push_branch(&self, place: &Place) -> Result<Option<String>, ShipError> {
        let (remote, forge) = find_forge(&place.path).await?;
        let branch = Self::branch(place, forge.kind()).await?;
        let base = self.base_for(place, forge.kind(), None).await?;
        refuse_base_branch(&branch, &base, forge.kind())?;
        prs::clean_and_push(&place.path, &remote, &branch, &base)
            .await
            .map_err(|e| ShipError::from_ado(e).with_forge(forge.kind()))
    }

    /// Push the branch and open its pull request. An open PR from the
    /// branch is not opened twice: the push updates it and it comes back
    /// as `existing`.
    pub async fn create_pr(&self, place: &Place, pr: NewPr) -> Result<Created, ShipError> {
        let (remote, forge) = find_forge(&place.path).await?;
        self.create_on(place, &remote, &forge, pr).await
    }

    async fn create_on(
        &self,
        place: &Place,
        remote: &str,
        forge: &Forge,
        pr: NewPr,
    ) -> Result<Created, ShipError> {
        let kind = forge.kind();
        let title = pr.title.trim().to_string();
        if title.is_empty() {
            return Err(ShipError::other(Some(kind), "Add a title."));
        }
        let branch = Self::branch(place, kind).await?;
        let base = self.base_for(place, kind, pr.base.as_deref()).await?;
        refuse_base_branch(&branch, &base, kind)?;
        let body = crate::attribution::strip_ai_trailers(&pr.body)
            .0
            .trim_end()
            .to_string();
        match forge {
            Forge::GitHub(repo) => {
                let note = prs::clean_and_push(&place.path, remote, &branch, &base)
                    .await
                    .map_err(|e| ShipError::from_ado(e).with_forge(kind))?;
                if let Some(open) = self
                    .pr_for(&place.path, forge, &branch)
                    .await?
                    .filter(PrInfo::is_open)
                {
                    return Ok(Created {
                        pr: open,
                        existing: true,
                        note,
                    });
                }
                let args = gh_create_args(repo, &branch, &base, &title, pr.draft);
                let out = self.gh(&place.path, args, Some(&body)).await?;
                if !out.ok {
                    return Err(gh_error(&out, "gh pr create"));
                }
                let url = out
                    .stdout
                    .lines()
                    .map(str::trim)
                    .rfind(|l| l.starts_with("http"))
                    .unwrap_or_default()
                    .to_string();
                // Read back as gh reports it; failing that, what was sent.
                let made = self
                    .pr_for(&place.path, forge, &branch)
                    .await
                    .ok()
                    .flatten();
                let pr = made.unwrap_or_else(|| PrInfo {
                    forge: kind,
                    number: number_from_url(&url).unwrap_or(0),
                    url,
                    title,
                    state: "open".into(),
                    draft: pr.draft,
                    head: branch,
                    base,
                    head_sha: None,
                });
                Ok(Created {
                    pr,
                    existing: false,
                    note,
                })
            }
            Forge::Ado(remote) => {
                let ctx = self.ado_pr_ctx(&place.path).await?;
                let new = prs::NewPr {
                    source: branch.clone(),
                    target: base.clone(),
                    title: title.clone(),
                    description: body,
                    draft: pr.draft,
                    reviewers: Vec::new(),
                    work_item: None,
                };
                let out = prs::create(&ctx, new).await.map_err(ShipError::from_ado)?;
                let (id, existing) = match out.outcome {
                    prs::CreateOutcome::Created(id)
                    | prs::CreateOutcome::CreatedTaskNotLinked(id, _) => (id, false),
                    prs::CreateOutcome::Existing(id) => (id, true),
                };
                Ok(Created {
                    pr: PrInfo {
                        forge: kind,
                        number: id as u64,
                        url: ado_pr_url(remote, id),
                        title,
                        state: "open".into(),
                        draft: pr.draft,
                        head: branch,
                        base,
                        head_sha: None,
                    },
                    existing,
                    note: out.note,
                })
            }
        }
    }

    /// Merge the branch's open pull request with `opts.method`, then (when
    /// asked) delete the branch on the remote. Azure DevOps completes it
    /// through its API; one its policies or state keep open comes back as
    /// [`ShipErrorKind::MergeInAdo`] with the PR's page.
    pub async fn merge_pr(&self, place: &Place, opts: MergeOpts) -> Result<Merged, ShipError> {
        let (remote, forge) = find_forge(&place.path).await?;
        self.merge_on(place, &remote, &forge, opts).await
    }

    async fn merge_on(
        &self,
        place: &Place,
        remote: &str,
        forge: &Forge,
        opts: MergeOpts,
    ) -> Result<Merged, ShipError> {
        let kind = forge.kind();
        let branch = Self::branch(place, kind).await?;
        let pr = self
            .pr_for(&place.path, forge, &branch)
            .await?
            .filter(PrInfo::is_open)
            .ok_or_else(|| {
                ShipError::new(
                    ShipErrorKind::NoPr,
                    Some(kind),
                    format!("{branch} has no open pull request to merge."),
                )
            })?;
        match forge {
            Forge::GitHub(repo) => {
                if pr.draft {
                    return Err(ShipError::other(
                        Some(kind),
                        format!("#{} is a draft: mark it ready for review first.", pr.number),
                    )
                    .with_url(&pr.url));
                }
                let args = gh_merge_args(repo, pr.number, opts.method, pr.head_sha.as_deref());
                let out = self.gh(&place.path, args, None).await?;
                if !out.ok {
                    return Err(gh_error(&out, "gh pr merge").with_url(&pr.url));
                }
                let (branch_deleted, note) = if opts.delete_branch {
                    self.delete_remote_branch(&place.path, remote, &branch)
                        .await
                } else {
                    (false, None)
                };
                Ok(Merged {
                    pr,
                    state: "merged".into(),
                    branch_deleted,
                    note,
                })
            }
            Forge::Ado(_) => {
                let ctx = self.ado_pr_ctx(&place.path).await?;
                let status = prs::complete(
                    &ctx,
                    pr.number as u32,
                    opts.method.ado_strategy(),
                    opts.delete_branch,
                )
                .await
                .map_err(|e| {
                    let e = ShipError::from_ado(e);
                    match e.kind {
                        ShipErrorKind::SignIn | ShipErrorKind::Offline => e,
                        _ => ShipError::new(
                            ShipErrorKind::MergeInAdo,
                            Some(kind),
                            format!("{} Finish it in Azure DevOps.", e.message),
                        ),
                    }
                    .with_url(&pr.url)
                })?;
                let done = status == "completed";
                Ok(Merged {
                    pr,
                    state: if done { "merged" } else { "queued" }.into(),
                    branch_deleted: done && opts.delete_branch,
                    note: (!done).then(|| {
                        "Azure DevOps queued the completion behind the PR's policies.".to_string()
                    }),
                })
            }
        }
    }

    /// `git push <remote> --delete <branch>`: whether it went, and why not.
    async fn delete_remote_branch(
        &self,
        path: &Path,
        remote: &str,
        branch: &str,
    ) -> (bool, Option<String>) {
        let args = strings(&["push", remote, "--delete", branch]);
        match self.runner.run("git", &args, path, None).await {
            Ok(out) if out.ok => (true, None),
            Ok(out) => (
                false,
                Some(format!(
                    "Merged; {branch} stayed on {remote}: {}",
                    first_line(&out.stderr)
                )),
            ),
            Err(e) => (
                false,
                Some(format!("Merged; {branch} stayed on {remote}: {e}")),
            ),
        }
    }
}

impl ShipError {
    fn with_forge(mut self, forge: ForgeKind) -> Self {
        self.forge = Some(forge);
        self
    }
}

/// A branch that is its own base cannot open a PR, and shipping pushes
/// it: refused before anything runs.
fn refuse_base_branch(branch: &str, base: &str, forge: ForgeKind) -> Result<(), ShipError> {
    if branch.eq_ignore_ascii_case(base) {
        return Err(ShipError::new(
            ShipErrorKind::NoBranch,
            Some(forge),
            format!("{branch} is the base branch: ship from a work branch."),
        ));
    }
    Ok(())
}

fn ado_pr_url(remote: &AdoRemote, id: u32) -> String {
    format!(
        "{}/{}/_git/{}/pullrequest/{id}",
        remote.org_url.trim_end_matches('/'),
        crate::enc(&remote.project),
        crate::enc(&remote.repo)
    )
}

fn ado_info(p: &prs::PrSummary) -> PrInfo {
    PrInfo {
        forge: ForgeKind::Ado,
        number: p.id as u64,
        url: p.url.clone(),
        title: p.title.clone(),
        state: match p.status.as_str() {
            "completed" => "merged",
            "abandoned" => "closed",
            _ => "open",
        }
        .into(),
        draft: p.is_draft,
        head: p.source.clone(),
        base: p.target.clone(),
        head_sha: p.source_commit.clone(),
    }
}

#[cfg(test)]
mod tests;
