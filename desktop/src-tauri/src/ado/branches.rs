//! Branches: local and remote merged into one row per name, with sync
//! state, the last commit, the linked task and any worktree or session,
//! grouped by the person prefix before the first `/`.

use super::{naming, remote, store};
use crate::daemon::{Client, Model};
use anyhow::{anyhow, bail, Result};
use nebula_core::{ClientRequest, EntityId, ProjectId, WorktreeId};
use serde::Serialize;
use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::State;

/// The group for branches with no `/` prefix.
pub const OTHER: &str = "other";

/// A branch's pull request. Filled by the PR module; `None` until then.
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

/// `git fetch --prune <remote>`.
pub fn fetch(repo: &Path, remote: &str) -> Result<()> {
    if remote.is_empty() {
        bail!("this repo has no remote to fetch from");
    }
    git(repo, &["fetch", "--prune", remote]).map(|_| ())
}

/// `git branch -d <branch>`: refuses an unmerged branch, as git does.
pub fn delete_local(repo: &Path, branch: &str) -> Result<()> {
    git(repo, &["branch", "-d", "--", branch]).map(|_| ())
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

fn branch_tree(repo: &Path, model: &Model, project: &ProjectId) -> Result<BranchTree> {
    let remote_name = pick_remote(repo);
    let mut rows = collect(repo, &remote_name)?;
    let tasks = remote_url(repo, &remote_name)
        .and_then(|u| remote::parse_remote(&u))
        .map(|r| {
            let key = store::RepoKey {
                org: r.org,
                project: r.project,
                repo: r.repo,
            };
            store::AdoStore::open(store::AdoStore::default_path())
                .read_repo(&key, |s| s.branch_tasks.clone())
        })
        .unwrap_or_default();
    for row in &mut rows {
        row.task = resolve_task(&row.name, &tasks, None);
    }
    attach_sessions(&mut rows, model, project);
    Ok(group(rows, &my_initials(repo)))
}

async fn blocking<T: Send + 'static>(
    f: impl FnOnce() -> Result<T> + Send + 'static,
) -> std::result::Result<T, String> {
    tokio::task::spawn_blocking(f)
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn ado_branches(
    client: State<'_, Arc<Client>>,
    worktree: String,
) -> std::result::Result<BranchTree, String> {
    let (repo, project) = locate(&client, &worktree)?;
    let model = client.snapshot();
    blocking(move || branch_tree(&repo, &model, &project)).await
}

#[tauri::command]
pub async fn ado_fetch(
    client: State<'_, Arc<Client>>,
    worktree: String,
) -> std::result::Result<(), String> {
    let (repo, _) = locate(&client, &worktree)?;
    blocking(move || {
        let remote_name = pick_remote(&repo);
        fetch(&repo, &remote_name)
    })
    .await
}

/// Check `branch` out in a worktree through the daemon. A branch already
/// in a worktree returns that worktree's id; a remote-only branch is cut
/// from `<remote>/<branch>` so it tracks it. Returns the worktree id.
#[tauri::command]
pub async fn ado_checkout_worktree(
    client: State<'_, Arc<Client>>,
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
        let remote_name = pick_remote(&repo);
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
        .rpc(
            req_id,
            ClientRequest::CreateWorktree {
                req_id,
                project,
                branch,
                base,
            },
        )
        .await
        .map_err(|e| e.to_string())?;
    match created {
        Some(EntityId::Worktree(WorktreeId(id))) => Ok(id),
        _ => Err("the daemon did not say which worktree it made".into()),
    }
}

/// Delete a local branch. Refused while a worktree has it checked out.
#[tauri::command]
pub async fn ado_delete_local(
    client: State<'_, Arc<Client>>,
    worktree: String,
    branch: String,
) -> std::result::Result<(), String> {
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
    blocking(move || delete_local(&repo, &branch)).await
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
        assert!(delete_local(&work, "jari/unmerged").is_err());
        delete_local(&work, "jari/merged").unwrap();
        let rows = collect(&work, "origin").unwrap();
        assert!(!rows.iter().any(|r| r.name == "jari/merged"));
        assert!(rows.iter().any(|r| r.name == "jari/unmerged"));
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

    #[test]
    fn track_parsing() {
        assert_eq!(parse_track(""), (0, 0, false));
        assert_eq!(parse_track("[gone]"), (0, 0, true));
        assert_eq!(parse_track("[ahead 3]"), (3, 0, false));
        assert_eq!(parse_track("[behind 2]"), (0, 2, false));
        assert_eq!(parse_track("[ahead 1, behind 4]"), (1, 4, false));
    }
}
