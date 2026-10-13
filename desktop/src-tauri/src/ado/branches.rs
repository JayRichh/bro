//! Branches: local and remote merged into one row per name, with sync
//! state, the last commit, the linked task and any worktree or session,
//! grouped by the person prefix before the first `/`.

use super::prs::{self, PrSummary};
use super::{naming, remote, AdoContext, AdoState};
use crate::daemon::{Client, Model};
use anyhow::{anyhow, bail, Result};
use nebula_core::{ClientRequest, EntityId, ProjectId, WorktreeId};
use serde::Serialize;
use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tauri::State;

/// The group for branches with no `/` prefix.
pub const OTHER: &str = "other";

/// A branch's active pull request.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PrRef {
    pub id: u32,
    pub status: String,
    pub is_draft: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BranchRow {
    pub name: String,
    /// The prefix before `/`, or `other`.
    pub person: String,
    pub local: bool,
    pub remote: bool,
    pub ahead: u32,
    pub behind: u32,
    /// The upstream was deleted on the remote.
    pub gone: bool,
    /// Seconds since the last commit (the local tip when there is one).
    pub last_commit_age: i64,
    pub last_author: String,
    pub task: Option<u32>,
    pub pr: Option<PrRef>,
    /// Id of the worktree that has this branch checked out.
    pub worktree: Option<String>,
    /// Id of a live (unarchived) agent session in that worktree.
    pub session: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BranchTree {
    pub me: String,
    /// `me` first, then people alphabetically.
    pub people: Vec<(String, Vec<BranchRow>)>,
}

fn git_cmd(repo: &Path) -> Command {
    let mut cmd = Command::new("git");
    cmd.current_dir(repo)
        .env("GIT_TERMINAL_PROMPT", "0")
        .stdin(Stdio::null())
        .args(["-c", "core.quotepath=off"]);
    #[cfg(windows)]
    nebula_core::platform::no_window(&mut cmd);
    cmd
}

/// Run git in `repo`, returning stdout, or an error with git's own words.
fn git(repo: &Path, args: &[&str]) -> Result<String> {
    let out = git_cmd(repo)
        .args(args)
        .output()
        .map_err(|e| anyhow!("could not run git: {e}"))?;
    if !out.status.success() {
        let why = String::from_utf8_lossy(&out.stderr).trim().to_string();
        let what = args.first().copied().unwrap_or("");
        if why.is_empty() {
            bail!("git {what} failed");
        }
        bail!("git {what} failed: {why}");
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

fn person_of(name: &str) -> String {
    match name.split_once('/') {
        Some((p, _)) if !p.is_empty() => p.to_string(),
        _ => OTHER.to_string(),
    }
}

/// `[ahead 1, behind 2]`, `[gone]` or empty, as `%(upstream:track)` gives.
fn parse_track(track: &str) -> (u32, u32, bool) {
    let inner = track.trim().trim_start_matches('[').trim_end_matches(']');
    if inner == "gone" {
        return (0, 0, true);
    }
    let (mut ahead, mut behind) = (0, 0);
    for part in inner.split(',') {
        let mut words = part.split_whitespace();
        match (words.next(), words.next().and_then(|n| n.parse().ok())) {
            (Some("ahead"), Some(n)) => ahead = n,
            (Some("behind"), Some(n)) => behind = n,
            _ => {}
        }
    }
    (ahead, behind, false)
}

fn now_secs() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

struct RefLine {
    name: String,
    upstream: String,
    track: String,
    date: i64,
    author: String,
}

fn for_each_ref(repo: &Path, pattern: &str) -> Result<Vec<RefLine>> {
    let out = git(
        repo,
        &[
            "for-each-ref",
            "--format=%(refname)%00%(upstream)%00%(upstream:track)%00%(committerdate:unix)%00%(authorname)",
            pattern,
        ],
    )?;
    Ok(out
        .lines()
        .filter_map(|line| {
            let mut f = line.split('\0');
            Some(RefLine {
                name: f.next()?.to_string(),
                upstream: f.next()?.to_string(),
                track: f.next()?.to_string(),
                date: f.next()?.trim().parse().unwrap_or(0),
                author: f.next().unwrap_or("").to_string(),
            })
        })
        .collect())
}

/// Every branch in `refs/heads` and `refs/remotes/<remote>`, one row per
/// name, sorted by name. `remote` empty reads local branches only. Task,
/// PR, worktree and session are left for the caller to fill.
pub fn collect(repo: &Path, remote: &str) -> Result<Vec<BranchRow>> {
    let now = now_secs();
    let mut rows: BTreeMap<String, BranchRow> = BTreeMap::new();
    let mut upstreams: HashMap<String, String> = HashMap::new();
    for r in for_each_ref(repo, "refs/heads")? {
        let Some(name) = r.name.strip_prefix("refs/heads/") else {
            continue;
        };
        let (ahead, behind, gone) = parse_track(&r.track);
        upstreams.insert(name.to_string(), r.upstream.clone());
        rows.insert(
            name.to_string(),
            BranchRow {
                name: name.to_string(),
                person: person_of(name),
                local: true,
                remote: false,
                ahead,
                behind,
                gone,
                last_commit_age: (now - r.date).max(0),
                last_author: r.author,
                task: None,
                pr: None,
                worktree: None,
                session: None,
            },
        );
    }
    if !remote.is_empty() {
        let prefix = format!("refs/remotes/{remote}/");
        for r in for_each_ref(repo, &format!("refs/remotes/{remote}"))? {
            let Some(name) = r.name.strip_prefix(&prefix) else {
                continue;
            };
            if name == "HEAD" {
                continue;
            }
            match rows.get_mut(name) {
                Some(row) => {
                    row.remote = true;
                    row.gone = false;
                    // No upstream set: count against the same-named
                    // remote branch so the badge still says something.
                    if upstreams.get(name).is_none_or(|u| u.is_empty()) {
                        let range = format!("refs/heads/{name}...{}", r.name);
                        if let Ok(out) = git(repo, &["rev-list", "--left-right", "--count", &range])
                        {
                            let mut n = out.split_whitespace().map(|x| x.parse().unwrap_or(0));
                            row.ahead = n.next().unwrap_or(0);
                            row.behind = n.next().unwrap_or(0);
                        }
                    }
                }
                None => {
                    rows.insert(
                        name.to_string(),
                        BranchRow {
                            name: name.to_string(),
                            person: person_of(name),
                            local: false,
                            remote: true,
                            ahead: 0,
                            behind: 0,
                            gone: false,
                            last_commit_age: (now - r.date).max(0),
                            last_author: r.author,
                            task: None,
                            pr: None,
                            worktree: None,
                            session: None,
                        },
                    );
                }
            }
        }
    }
    Ok(rows.into_values().collect())
}

/// Group rows by person: `me` first (when it has branches), then everyone
/// else alphabetically, `other` included. Rows keep their order.
pub fn group(rows: Vec<BranchRow>, me: &str) -> BranchTree {
    let mut by: BTreeMap<String, Vec<BranchRow>> = BTreeMap::new();
    for row in rows {
        by.entry(row.person.clone()).or_default().push(row);
    }
    let mut people = Vec::with_capacity(by.len());
    if let Some(mine) = by.remove(me) {
        people.push((me.to_string(), mine));
    }
    people.extend(by);
    BranchTree {
        me: me.to_string(),
        people,
    }
}

/// The task a branch works on: the stored link, then the PR's first work
/// item, then the first standalone 3 to 7 digit number in the name.
pub fn resolve_task(
    name: &str,
    store_map: &BTreeMap<String, u32>,
    pr_work_items: Option<&[u32]>,
) -> Option<u32> {
    if let Some(id) = store_map.get(name) {
        return Some(*id);
    }
    if let Some(id) = pr_work_items.and_then(|w| w.first()) {
        return Some(*id);
    }
    number_in_name(name)
}

/// `\b(\d{3,7})\b`: a whole run of 3 to 7 digits with no word character
/// either side.
fn number_in_name(name: &str) -> Option<u32> {
    let chars: Vec<char> = name.chars().collect();
    let word = |c: char| c.is_alphanumeric() || c == '_';
    let mut i = 0;
    while i < chars.len() {
        if !chars[i].is_ascii_digit() {
            i += 1;
            continue;
        }
        let start = i;
        while i < chars.len() && chars[i].is_ascii_digit() {
            i += 1;
        }
        let before = start == 0 || !word(chars[start - 1]);
        let after = i == chars.len() || !word(chars[i]);
        if before && after && (3..=7).contains(&(i - start)) {
            return chars[start..i].iter().collect::<String>().parse().ok();
        }
    }
    None
}

/// The remote to read: `origin`, else the single ADO remote, else the only
/// remote. Empty when none fits.
pub fn pick_remote(repo: &Path) -> String {
    let names: Vec<String> = git(repo, &["remote"])
        .unwrap_or_default()
        .lines()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(String::from)
        .collect();
    if names.iter().any(|n| n == "origin") {
        return "origin".into();
    }
    let ado: Vec<&String> = names
        .iter()
        .filter(|n| {
            remote_url(repo, n)
                .and_then(|u| remote::parse_remote(&u))
                .is_some()
        })
        .collect();
    match (ado.as_slice(), names.as_slice()) {
        ([one], _) => (*one).clone(),
        (_, [one]) => one.clone(),
        _ => String::new(),
    }
}

fn remote_url(repo: &Path, remote: &str) -> Option<String> {
    git(repo, &["remote", "get-url", remote])
        .ok()
        .map(|s| s.trim().to_string())
}

/// `ado.initials`, else initials of `user.name`, else empty.
pub fn my_initials(repo: &Path) -> String {
    if let Ok(v) = git(repo, &["config", "ado.initials"]) {
        let v = v.trim();
        if !v.is_empty() {
            return v.to_lowercase();
        }
    }
    git(repo, &["config", "user.name"])
        .ok()
        .and_then(|n| naming::initials(n.trim()))
        .unwrap_or_default()
}

/// How long a fetch may take before it is killed, as for the diff's.
const FETCH_TIMEOUT: Duration = Duration::from_secs(60);

/// `git fetch --prune <remote>`, killed after [`FETCH_TIMEOUT`].
pub fn fetch(repo: &Path, remote: &str) -> Result<()> {
    if remote.is_empty() {
        bail!("this repo has no remote to fetch from");
    }
    let mut cmd = git_cmd(repo);
    cmd.args(["fetch", "--prune", remote])
        .stdout(Stdio::null())
        .stderr(Stdio::piped());
    let out = crate::git::output_within(cmd, FETCH_TIMEOUT, "git fetch").map_err(|e| anyhow!(e))?;
    if !out.status.success() {
        let why = String::from_utf8_lossy(&out.stderr).trim().to_string();
        if why.is_empty() {
            bail!("git fetch failed");
        }
        bail!("git fetch failed: {why}");
    }
    Ok(())
}

/// What a local delete came to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Deleted {
    Deleted,
    /// `git branch -d` refused: the branch has commits its upstream (or
    /// HEAD) lacks. A squash-merged branch looks like this. Only an
    /// explicit "Delete anyway" (`force`) removes it.
    Unmerged,
}

/// `git branch -d <branch>`, which refuses an unmerged branch as git does
/// (that is `Unmerged`, not an error); with `force`, `git branch -D`.
pub fn delete_local(repo: &Path, branch: &str, force: bool) -> Result<Deleted> {
    let flag = if force { "-D" } else { "-d" };
    let out = git_cmd(repo)
        // git's own words decide "unmerged", so read them untranslated.
        .env("LC_ALL", "C")
        .args(["branch", flag, "--", branch])
        .output()
        .map_err(|e| anyhow!("could not run git: {e}"))?;
    if out.status.success() {
        return Ok(Deleted::Deleted);
    }
    let why = String::from_utf8_lossy(&out.stderr).trim().to_string();
    if !force && why.contains("not fully merged") {
        return Ok(Deleted::Unmerged);
    }
    if why.is_empty() {
        bail!("git branch failed");
    }
    bail!("git branch failed: {why}")
}

/// Fill worktree and session from the daemon model: a worktree of the
/// same project with this branch checked out, and its first live agent.
fn attach_sessions(rows: &mut [BranchRow], model: &Model, project: &ProjectId) {
    for row in rows {
        let Some(w) = model
            .worktrees
            .iter()
            .find(|w| &w.project_id == project && w.branch == row.name)
        else {
            continue;
        };
        row.worktree = Some(w.id.0.clone());
        row.session = model
            .agents
            .iter()
            .find(|a| a.worktree_id == w.id && !a.archived)
            .map(|a| a.id.0.clone());
    }
}

/// The checkout path and project of a worktree in the daemon model.
fn locate(client: &Client, worktree: &str) -> std::result::Result<(PathBuf, ProjectId), String> {
    client
        .snapshot()
        .worktrees
        .iter()
        .find(|w| w.id.0 == worktree)
        .map(|w| (w.path.clone(), w.project_id.clone()))
        .ok_or_else(|| "that worktree is gone".to_string())
}

/// Fill each row's PR (the first active one from that branch, by
/// `sourceRefName`) and its task, which the PR's work items inform.
pub fn attach_prs(rows: &mut [BranchRow], prs: &[PrSummary], tasks: &BTreeMap<String, u32>) {
    for row in rows {
        let pr = prs.iter().find(|p| p.source == row.name);
        row.pr = pr.map(|p| PrRef {
            id: p.id,
            status: p.status.clone(),
            is_draft: p.is_draft,
        });
        row.task = resolve_task(&row.name, tasks, pr.map(|p| p.work_items.as_slice()));
    }
}

/// How long the branch list waits for the PR list before it shows the
/// branches without PRs.
const PR_LIST_TIMEOUT: Duration = Duration::from_secs(10);
/// The separate budget for the PRs' work items: running out keeps the
/// PRs and whatever work items arrived.
const WORK_ITEMS_TIMEOUT: Duration = Duration::from_secs(5);

/// The repo's active PRs, with work items for branches that have no
/// stored task. Empty when the list fails (offline, not signed in) or
/// takes too long: the branches show without PRs. Slow work items never
/// cost the PRs.
async fn branch_prs(ctx: &AdoContext, tasks: &BTreeMap<String, u32>) -> Vec<PrSummary> {
    branch_prs_within(ctx, tasks, PR_LIST_TIMEOUT, WORK_ITEMS_TIMEOUT).await
}

async fn branch_prs_within(
    ctx: &AdoContext,
    tasks: &BTreeMap<String, u32>,
    list_budget: Duration,
    work_items_budget: Duration,
) -> Vec<PrSummary> {
    let Ok(Ok(mut found)) = tokio::time::timeout(list_budget, prs::active(ctx)).await else {
        return Vec::new();
    };
    let fill = prs::fill_work_items(ctx, &mut found, |source| !tasks.contains_key(source));
    let _ = tokio::time::timeout(work_items_budget, fill).await;
    found
}

/// The remote the branch commands read: the ADO context's, else
/// [`pick_remote`]'s for a repo with no ADO remote.
async fn remote_of(ctx: Option<&AdoContext>, repo: &Path) -> std::result::Result<String, String> {
    match ctx {
        Some(c) => Ok(c.remote_name.clone()),
        None => {
            let repo = repo.to_path_buf();
            blocking(move || Ok(pick_remote(&repo))).await
        }
    }
}

async fn blocking<T: Send + 'static>(
    f: impl FnOnce() -> Result<T> + Send + 'static,
) -> std::result::Result<T, String> {
    tokio::task::spawn_blocking(f)
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())
}

/// Every branch, grouped by person, with its task, PR, worktree and
/// session. The PR lookup uses the silent client, so opening the list
/// never brings up a sign-in window; when it fails the branches come back
/// without PRs.
#[tauri::command]
pub async fn ado_branches(
    client: State<'_, Arc<Client>>,
    ado: State<'_, AdoState>,
    worktree: String,
) -> std::result::Result<BranchTree, String> {
    let (repo, project) = locate(&client, &worktree)?;
    let ctx = ado.silent_context(&repo).await.ok().flatten();
    let remote_name = remote_of(ctx.as_ref(), &repo).await?;
    let tasks = ctx
        .as_ref()
        .map(|c| c.store.read_repo(&c.key, |s| s.branch_tasks.clone()))
        .unwrap_or_default();
    let r = repo.clone();
    let (mut rows, me) =
        blocking(move || Ok((collect(&r, &remote_name)?, my_initials(&r)))).await?;
    let prs = match &ctx {
        Some(c) => branch_prs(c, &tasks).await,
        None => Vec::new(),
    };
    attach_prs(&mut rows, &prs, &tasks);
    attach_sessions(&mut rows, &client.snapshot(), &project);
    Ok(group(rows, &me))
}

#[tauri::command]
pub async fn ado_fetch(
    client: State<'_, Arc<Client>>,
    ado: State<'_, AdoState>,
    worktree: String,
) -> std::result::Result<(), String> {
    let (repo, _) = locate(&client, &worktree)?;
    let ctx = ado.silent_context(&repo).await.ok().flatten();
    let remote_name = remote_of(ctx.as_ref(), &repo).await?;
    blocking(move || fetch(&repo, &remote_name)).await
}

/// Check `branch` out in a worktree through the daemon. A branch already
/// in a worktree returns that worktree's id; a remote-only branch is cut
/// from `<remote>/<branch>` so it tracks it. Returns the worktree id.
#[tauri::command]
pub async fn ado_checkout_worktree(
    client: State<'_, Arc<Client>>,
    ado: State<'_, AdoState>,
    worktree: String,
    branch: String,
) -> std::result::Result<String, String> {
    let (repo, project) = locate(&client, &worktree)?;
    let model = client.snapshot();
    if let Some(w) = model
        .worktrees
        .iter()
        .find(|w| w.project_id == project && w.branch == branch)
    {
        return Ok(w.id.0.clone());
    }
    let ctx = ado.silent_context(&repo).await.ok().flatten();
    let remote_name = remote_of(ctx.as_ref(), &repo).await?;
    let name = branch.clone();
    let base = blocking(move || {
        let local = git(
            &repo,
            &[
                "show-ref",
                "--verify",
                "--quiet",
                &format!("refs/heads/{name}"),
            ],
        )
        .is_ok();
        if local {
            return Ok(None);
        }
        let remote_ref = format!("refs/remotes/{remote_name}/{name}");
        if remote_name.is_empty()
            || git(&repo, &["show-ref", "--verify", "--quiet", &remote_ref]).is_err()
        {
            bail!("there is no branch named {name}");
        }
        Ok(Some(format!("{remote_name}/{name}")))
    })
    .await?;
    let req_id = client.req_id();
    let created = client
        .rpc_within(
            req_id,
            ClientRequest::CreateWorktree {
                req_id,
                project: project.clone(),
                branch: branch.clone(),
                base,
            },
            super::tasks::CREATE_WORKTREE_TIMEOUT,
        )
        .await;
    match created {
        Ok(Some(EntityId::Worktree(WorktreeId(id)))) => Ok(id),
        Ok(_) => Err("the daemon did not say which worktree it made".into()),
        // A big repo's checkout can outlast the wait: look for it.
        Err(e) if e.is::<crate::daemon::RpcTimeout>() => {
            let deadline = tokio::time::Instant::now() + super::tasks::RECONCILE_WAIT;
            loop {
                if let Some(w) = client
                    .snapshot()
                    .worktrees
                    .iter()
                    .find(|w| w.project_id == project && w.branch == branch)
                {
                    return Ok(w.id.0.clone());
                }
                if tokio::time::Instant::now() >= deadline {
                    return Err(format!(
                        "the daemon did not answer in time, and no worktree on {branch} showed up"
                    ));
                }
                tokio::time::sleep(Duration::from_millis(500)).await;
            }
        }
        Err(e) => Err(e.to_string()),
    }
}

/// Delete a local branch. Refused while a worktree has it checked out.
#[tauri::command]
pub async fn ado_delete_local(
    client: State<'_, Arc<Client>>,
    worktree: String,
    branch: String,
    force: Option<bool>,
) -> std::result::Result<Deleted, String> {
    let (repo, project) = locate(&client, &worktree)?;
    if client
        .snapshot()
        .worktrees
        .iter()
        .any(|w| w.project_id == project && w.branch == branch)
    {
        return Err(format!(
            "{branch} is checked out in a worktree. Remove the worktree first."
        ));
    }
    let force = force.unwrap_or(false);
    blocking(move || delete_local(&repo, &branch, force)).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    fn temp_dir() -> PathBuf {
        static N: AtomicUsize = AtomicUsize::new(0);
        let dir = std::env::temp_dir().join(format!(
            "kete-ado-branches-{}-{}",
            std::process::id(),
            N.fetch_add(1, Ordering::SeqCst)
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn run(dir: &Path, args: &[&str]) {
        git(dir, args).unwrap_or_else(|e| panic!("git {args:?}: {e}"));
    }

    fn commit(dir: &Path, msg: &str) {
        run(dir, &["commit", "--allow-empty", "-q", "-m", msg]);
    }

    /// A work repo with `origin` pointing at a bare repo, `main` pushed.
    fn repo_with_remote() -> PathBuf {
        let root = temp_dir();
        let bare = root.join("remote.git");
        let work = root.join("work");
        std::fs::create_dir_all(&bare).unwrap();
        std::fs::create_dir_all(&work).unwrap();
        run(&bare, &["init", "-q", "--bare", "-b", "main"]);
        run(&work, &["init", "-q", "-b", "main"]);
        run(&work, &["config", "user.name", "Jay Richards"]);
        run(&work, &["config", "user.email", "jay@example.com"]);
        run(&work, &["config", "commit.gpgsign", "false"]);
        run(&work, &["remote", "add", "origin", bare.to_str().unwrap()]);
        commit(&work, "init");
        run(&work, &["push", "-q", "-u", "origin", "main"]);
        work
    }

    fn row<'a>(rows: &'a [BranchRow], name: &str) -> &'a BranchRow {
        rows.iter()
            .find(|r| r.name == name)
            .unwrap_or_else(|| panic!("no row {name} in {rows:#?}"))
    }

    fn sample(name: &str) -> BranchRow {
        BranchRow {
            name: name.into(),
            person: person_of(name),
            local: true,
            remote: false,
            ahead: 0,
            behind: 0,
            gone: false,
            last_commit_age: 0,
            last_author: String::new(),
            task: None,
            pr: None,
            worktree: None,
            session: None,
        }
    }

    #[test]
    fn local_only_branch() {
        let work = repo_with_remote();
        run(&work, &["branch", "jari/local"]);
        let rows = collect(&work, "origin").unwrap();
        let r = row(&rows, "jari/local");
        assert!(r.local && !r.remote && !r.gone);
        assert_eq!(r.person, "jari");
        assert_eq!(r.last_author, "Jay Richards");
        let main = row(&rows, "main");
        assert!(main.local && main.remote);
        assert_eq!((main.ahead, main.behind), (0, 0));
        assert!(!rows.iter().any(|r| r.name == "HEAD"));
    }

    #[test]
    fn pushed_then_advanced_is_ahead_one() {
        let work = repo_with_remote();
        run(&work, &["checkout", "-q", "-b", "jari/ahead"]);
        commit(&work, "one");
        run(&work, &["push", "-q", "-u", "origin", "jari/ahead"]);
        commit(&work, "two");
        let rows = collect(&work, "origin").unwrap();
        let r = row(&rows, "jari/ahead");
        assert!(r.local && r.remote);
        assert_eq!((r.ahead, r.behind), (1, 0));
    }

    #[test]
    fn same_named_remote_without_upstream_still_counts() {
        let work = repo_with_remote();
        run(&work, &["checkout", "-q", "-b", "ga/loose"]);
        commit(&work, "one");
        run(&work, &["push", "-q", "origin", "ga/loose"]);
        commit(&work, "two");
        commit(&work, "three");
        let rows = collect(&work, "origin").unwrap();
        let r = row(&rows, "ga/loose");
        assert!(r.local && r.remote);
        assert_eq!((r.ahead, r.behind), (2, 0));
    }

    #[test]
    fn remote_branch_without_local() {
        let work = repo_with_remote();
        run(&work, &["checkout", "-q", "-b", "ga/remote"]);
        commit(&work, "one");
        run(&work, &["push", "-q", "origin", "ga/remote"]);
        run(&work, &["checkout", "-q", "main"]);
        run(&work, &["branch", "-q", "-D", "ga/remote"]);
        let rows = collect(&work, "origin").unwrap();
        let r = row(&rows, "ga/remote");
        assert!(r.remote && !r.local);
        assert_eq!(r.person, "ga");
    }

    #[test]
    fn deleted_upstream_is_gone_after_fetch_prune() {
        let work = repo_with_remote();
        run(&work, &["checkout", "-q", "-b", "jari/gone"]);
        commit(&work, "one");
        run(&work, &["push", "-q", "-u", "origin", "jari/gone"]);
        run(&work, &["checkout", "-q", "main"]);
        let bare = work.parent().unwrap().join("remote.git");
        run(&bare, &["branch", "-q", "-D", "jari/gone"]);
        let before = collect(&work, "origin").unwrap();
        assert!(!row(&before, "jari/gone").gone);
        fetch(&work, "origin").unwrap();
        let rows = collect(&work, "origin").unwrap();
        let r = row(&rows, "jari/gone");
        assert!(r.gone && r.local && !r.remote);
    }

    #[test]
    fn delete_local_refuses_unmerged_and_takes_merged() {
        let work = repo_with_remote();
        run(&work, &["branch", "jari/merged"]);
        run(&work, &["checkout", "-q", "-b", "jari/unmerged"]);
        commit(&work, "one");
        run(&work, &["checkout", "-q", "main"]);
        assert_eq!(
            delete_local(&work, "jari/unmerged", false).unwrap(),
            Deleted::Unmerged
        );
        assert_eq!(
            delete_local(&work, "jari/merged", false).unwrap(),
            Deleted::Deleted
        );
        let rows = collect(&work, "origin").unwrap();
        assert!(!rows.iter().any(|r| r.name == "jari/merged"));
        assert!(rows.iter().any(|r| r.name == "jari/unmerged"));
        // "Delete anyway" takes it.
        assert_eq!(
            delete_local(&work, "jari/unmerged", true).unwrap(),
            Deleted::Deleted
        );
        let rows = collect(&work, "origin").unwrap();
        assert!(!rows.iter().any(|r| r.name == "jari/unmerged"));
        // A branch that is not there is still an error.
        assert!(delete_local(&work, "jari/none", false).is_err());
    }

    #[test]
    fn picks_origin_and_reads_initials() {
        let work = repo_with_remote();
        assert_eq!(pick_remote(&work), "origin");
        assert_eq!(my_initials(&work), "jari");
        run(&work, &["config", "ado.initials", "JR"]);
        assert_eq!(my_initials(&work), "jr");
    }

    #[test]
    fn grouping_pins_me_then_alphabetical_with_main_under_other() {
        let rows = vec![
            sample("zed/x"),
            sample("main"),
            sample("ga/b"),
            sample("jari/two"),
            sample("jari/one"),
            sample("ab/c"),
        ];
        let tree = group(rows, "jari");
        assert_eq!(tree.me, "jari");
        let names: Vec<&str> = tree.people.iter().map(|(p, _)| p.as_str()).collect();
        assert_eq!(names, ["jari", "ab", "ga", "other", "zed"]);
        let other = &tree.people[3].1;
        assert_eq!(other.len(), 1);
        assert_eq!(other[0].name, "main");
        let mine: Vec<&str> = tree.people[0].1.iter().map(|r| r.name.as_str()).collect();
        assert_eq!(mine, ["jari/two", "jari/one"]);
    }

    #[test]
    fn grouping_without_my_branches_is_alphabetical() {
        let tree = group(vec![sample("ga/b"), sample("main")], "jari");
        let names: Vec<&str> = tree.people.iter().map(|(p, _)| p.as_str()).collect();
        assert_eq!(names, ["ga", "other"]);
    }

    #[test]
    fn resolve_task_prefers_store_then_pr_then_name() {
        let mut map = BTreeMap::new();
        map.insert("jari/fix-4567".to_string(), 111);
        assert_eq!(resolve_task("jari/fix-4567", &map, Some(&[222])), Some(111));
        assert_eq!(resolve_task("jari/fix-4567", &map, None), Some(111));
        assert_eq!(resolve_task("jari/fix-890", &map, Some(&[222])), Some(222));
        assert_eq!(resolve_task("jari/fix-890", &map, Some(&[])), Some(890));
        assert_eq!(resolve_task("jari/890-fix", &map, None), Some(890));
        assert_eq!(resolve_task("jari/1234567", &map, None), Some(1234567));
        assert_eq!(resolve_task("jari/12345678", &map, None), None);
        assert_eq!(resolve_task("jari/v12", &map, None), None);
        assert_eq!(resolve_task("jari/abc1234", &map, None), None);
        assert_eq!(resolve_task("jari/x_1234", &map, None), None);
        assert_eq!(resolve_task("jari/no-number", &map, None), None);
        assert_eq!(resolve_task("12/345-x", &map, None), Some(345));
    }

    fn summary(id: u32, source: &str, work_items: Vec<u32>) -> PrSummary {
        PrSummary {
            id,
            title: format!("PR {id}"),
            source: source.into(),
            target: "main".into(),
            is_draft: id.is_multiple_of(2),
            status: "active".into(),
            created_by: "Jay".into(),
            reviewers: Vec::new(),
            work_items,
            threads: 0,
            url: String::new(),
            source_commit: None,
            target_commit: None,
        }
    }

    #[test]
    fn prs_attach_by_source_and_inform_the_task() {
        let mut rows = vec![
            sample("jari/fix-890"),
            sample("jari/stored"),
            sample("ga/no-pr"),
        ];
        let prs = vec![
            summary(4, "jari/fix-890", vec![4321]),
            summary(5, "jari/stored", vec![999]),
            summary(7, "jari/fix-890", vec![]),
        ];
        let mut tasks = BTreeMap::new();
        tasks.insert("jari/stored".to_string(), 111);
        attach_prs(&mut rows, &prs, &tasks);
        assert_eq!(
            rows[0].pr,
            Some(PrRef {
                id: 4,
                status: "active".into(),
                is_draft: true
            })
        );
        // The PR's work item beats the number in the name.
        assert_eq!(rows[0].task, Some(4321));
        // A stored link beats the PR's work item.
        assert_eq!(rows[1].pr.as_ref().map(|p| p.id), Some(5));
        assert_eq!(rows[1].task, Some(111));
        assert_eq!(rows[2].pr, None);
        assert_eq!(rows[2].task, None);
    }

    #[tokio::test]
    async fn branch_prs_take_one_list_call_and_work_items_where_needed() {
        use httpmock::prelude::*;
        use serde_json::json;
        let s = MockServer::start();
        s.mock(|w, t| {
            w.method(GET).path("/Platform/_apis/git/repositories/api");
            t.status(200)
                .json_body(json!({"id": "r1", "project": {"id": "p1"}}));
        });
        let pr = |id: u32, source: &str| {
            json!({
                "pullRequestId": id, "title": "x", "status": "active", "isDraft": false,
                "sourceRefName": format!("refs/heads/{source}"),
                "targetRefName": "refs/heads/main",
                "repository": {"webUrl": "https://x"}
            })
        };
        let list = s.mock(|w, t| {
            w.method(GET)
                .path("/_apis/git/repositories/r1/pullrequests")
                .query_param("searchCriteria.status", "active");
            t.status(200)
                .json_body(json!({"value": [pr(4, "jari/fix-890"), pr(5, "jari/stored")]}));
        });
        let wanted = s.mock(|w, t| {
            w.method(GET)
                .path("/_apis/git/repositories/r1/pullrequests/4/workitems");
            t.status(200).json_body(json!({"value": [{"id": "4321"}]}));
        });
        let unwanted = s.mock(|w, t| {
            w.method(GET)
                .path("/_apis/git/repositories/r1/pullrequests/5/workitems");
            t.status(200).json_body(json!({"value": [{"id": "999"}]}));
        });
        let ctx = crate::ado::testing::context(&s.base_url(), crate::ado::testing::temp_store());
        let mut tasks = BTreeMap::new();
        tasks.insert("jari/stored".to_string(), 111);
        let prs = branch_prs(&ctx, &tasks).await;
        list.assert();
        wanted.assert();
        assert_eq!(unwanted.hits(), 0);
        assert_eq!(prs.len(), 2);
        assert_eq!(prs[0].source, "jari/fix-890");
        assert_eq!(prs[0].work_items, vec![4321]);
    }

    #[tokio::test]
    async fn slow_work_items_keep_the_prs() {
        use httpmock::prelude::*;
        use serde_json::json;
        let s = MockServer::start();
        s.mock(|w, t| {
            w.method(GET).path("/Platform/_apis/git/repositories/api");
            t.status(200)
                .json_body(json!({"id": "r1", "project": {"id": "p1"}}));
        });
        s.mock(|w, t| {
            w.method(GET)
                .path("/_apis/git/repositories/r1/pullrequests")
                .query_param("searchCriteria.status", "active");
            t.status(200).json_body(json!({"value": [{
                "pullRequestId": 4, "status": "active", "isDraft": true,
                "sourceRefName": "refs/heads/jari/fix-890",
                "targetRefName": "refs/heads/main",
                "repository": {"webUrl": "https://x"}
            }]}));
        });
        s.mock(|w, t| {
            w.method(GET)
                .path("/_apis/git/repositories/r1/pullrequests/4/workitems");
            t.status(200)
                .delay(Duration::from_secs(3))
                .json_body(json!({"value": [{"id": "4321"}]}));
        });
        let ctx = crate::ado::testing::context(&s.base_url(), crate::ado::testing::temp_store());
        let tasks = BTreeMap::new();
        let prs = branch_prs_within(
            &ctx,
            &tasks,
            Duration::from_secs(10),
            Duration::from_millis(200),
        )
        .await;
        assert_eq!(prs.len(), 1);
        assert!(prs[0].work_items.is_empty());
        let mut rows = vec![sample("jari/fix-890")];
        attach_prs(&mut rows, &prs, &tasks);
        assert_eq!(rows[0].pr.as_ref().map(|p| p.id), Some(4));
        // No work items arrived, so the task comes from the name.
        assert_eq!(rows[0].task, Some(890));
    }

    #[tokio::test]
    async fn a_failed_pr_lookup_leaves_the_rows_without_prs() {
        use httpmock::prelude::*;
        let s = MockServer::start();
        s.mock(|w, t| {
            w.path_contains("/_apis/");
            t.status(401);
        });
        let ctx = crate::ado::testing::context(&s.base_url(), crate::ado::testing::temp_store());
        let tasks = BTreeMap::new();
        let prs = branch_prs(&ctx, &tasks).await;
        assert!(prs.is_empty());
        let mut rows = vec![sample("jari/fix-890")];
        attach_prs(&mut rows, &prs, &tasks);
        assert_eq!(rows[0].pr, None);
        assert_eq!(rows[0].task, Some(890));
    }

    #[test]
    fn track_parsing() {
        assert_eq!(parse_track(""), (0, 0, false));
        assert_eq!(parse_track("[gone]"), (0, 0, true));
        assert_eq!(parse_track("[ahead 3]"), (3, 0, false));
        assert_eq!(parse_track("[behind 2]"), (0, 2, false));
        assert_eq!(parse_track("[ahead 1, behind 4]"), (1, 4, false));
    }
}
