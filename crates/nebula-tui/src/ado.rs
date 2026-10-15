//! The AZURE DEVOPS OVERLAY (`⇧B`): a project's sprint TASKS, its active
//! PULL REQUESTS and its BRANCHES, read-only, the desktop's ADO views in
//! the terminal. Only a project whose remote is on Azure DevOps has one
//! ([`is_ado_remote`]); the header shows its `ado` button only there.
//!
//! The lists come from the `kete ado … --json` commands this binary
//! already ships (`crates/nebula/src/ado.rs`), run off the loop: the same
//! sign-in (Git Credential Manager, `az`), the same cache when offline,
//! the same team, no second Azure DevOps client in the TUI. Starting a
//! task is `kete ado start-task`, which cuts the branch and worktree and
//! starts the agent through the daemon like any other launch; checking a
//! branch out is the daemon's own `CreateWorktree`, as the desktop's
//! Branches view sends it. The parsers here are pure and tested on
//! fixtures; `event_loop::ado` runs them and takes the keys,
//! `ui::ado_view` draws.

use nebula_core::ProjectId;
use ratatui::layout::Rect;
use serde::Deserialize;
use std::path::Path;

/// Whether `url` is an Azure DevOps remote: `dev.azure.com` (https or
/// ssh) or an old `*.visualstudio.com` one.
pub fn is_ado_remote(url: &str) -> bool {
    let url = url.trim().to_ascii_lowercase();
    url.contains("dev.azure.com") || url.contains(".visualstudio.com")
}

/// The fetch remote of `repo`: `origin`, else the first one listed.
/// Blocking: a `git`, run off the loop.
pub fn remote_url(repo: &Path) -> Option<String> {
    let git = |args: &[&str]| {
        let mut cmd = std::process::Command::new("git");
        cmd.arg("-C").arg(repo).args(args);
        #[cfg(windows)]
        nebula_core::platform::no_window(&mut cmd);
        cmd.output()
            .ok()
            .filter(|o| o.status.success())
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
            .filter(|s| !s.is_empty())
    };
    git(&["remote", "get-url", "origin"]).or_else(|| {
        let first = git(&["remote"])?;
        let name = first.lines().next()?.to_string();
        git(&["remote", "get-url", &name])
    })
}

/// The overlay's three lists, in tab order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AdoTab {
    #[default]
    Tasks,
    PullRequests,
    Branches,
}

impl AdoTab {
    pub const ALL: [AdoTab; 3] = [AdoTab::Tasks, AdoTab::PullRequests, AdoTab::Branches];

    pub fn label(self) -> &'static str {
        match self {
            AdoTab::Tasks => "Tasks",
            AdoTab::PullRequests => "Pull requests",
            AdoTab::Branches => "Branches",
        }
    }

    pub fn index(self) -> usize {
        Self::ALL.iter().position(|t| *t == self).unwrap_or(0)
    }
}

/// One work item in the sprint.
#[derive(Debug, Clone, PartialEq)]
pub struct AdoTask {
    pub id: u32,
    pub kind: String,
    pub title: String,
    pub state: String,
    pub assigned_to: Option<String>,
    pub url: String,
    /// The title of the item it sits under (a story, a bug), if any.
    pub parent: Option<String>,
}

/// What `kete ado tasks --json` says: the sprint's name (or why a saved
/// list is shown), and the items.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct TaskRead {
    pub header: String,
    pub tasks: Vec<AdoTask>,
}

/// One active pull request: yours (`mine`) or one waiting on your review.
#[derive(Debug, Clone, PartialEq)]
pub struct AdoPr {
    pub id: u32,
    pub title: String,
    pub source: String,
    pub target: String,
    pub is_draft: bool,
    pub created_by: String,
    pub url: String,
    pub mine: bool,
}

/// One branch: on this machine, on the remote, or both.
#[derive(Debug, Clone, PartialEq)]
pub struct AdoBranch {
    pub name: String,
    pub local: bool,
    /// `<remote>/<name>` when the remote has it: what a checkout of a
    /// branch only the remote has is cut from, so it tracks it.
    pub remote: Option<String>,
}

// ---- the CLI's JSON (`crates/nebula/src/ado.rs`, `kete-ado` shapes) ----

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
struct TaskListJson {
    iteration: Option<NamedJson>,
    groups: Vec<GroupJson>,
    cached_at: Option<i64>,
    cached_reason: Option<String>,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct NamedJson {
    name: String,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct GroupJson {
    parent: Option<ItemJson>,
    items: Vec<ItemJson>,
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
struct ItemJson {
    id: u32,
    kind: String,
    title: String,
    state: String,
    assigned_to: Option<String>,
    url: String,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct PrsJson {
    mine: Vec<PrJson>,
    review: Vec<PrJson>,
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
struct PrJson {
    id: u32,
    title: String,
    source: String,
    target: String,
    is_draft: bool,
    created_by: String,
    url: String,
}

/// `kete ado tasks --json`.
pub fn parse_tasks(json: &str) -> Result<TaskRead, String> {
    let list: TaskListJson =
        serde_json::from_str(json).map_err(|e| format!("unreadable task list: {e}"))?;
    let mut header = list
        .iteration
        .map(|i| i.name)
        .filter(|n| !n.is_empty())
        .unwrap_or_else(|| "no current sprint".into());
    if list.cached_at.is_some() {
        header.push_str(&format!(
            " · saved list ({})",
            list.cached_reason.as_deref().unwrap_or("offline")
        ));
    }
    let tasks = list
        .groups
        .into_iter()
        .flat_map(|g| {
            let parent = g.parent.map(|p| p.title);
            g.items.into_iter().map(move |i| AdoTask {
                id: i.id,
                kind: i.kind,
                title: i.title,
                state: i.state,
                assigned_to: i.assigned_to,
                url: i.url,
                parent: parent.clone(),
            })
        })
        .collect();
    Ok(TaskRead { header, tasks })
}

/// `kete ado prs --json`: yours first, then those waiting on your review.
pub fn parse_prs(json: &str) -> Result<Vec<AdoPr>, String> {
    let prs: PrsJson =
        serde_json::from_str(json).map_err(|e| format!("unreadable pull requests: {e}"))?;
    let row = |p: PrJson, mine: bool| AdoPr {
        id: p.id,
        title: p.title,
        source: p.source,
        target: p.target,
        is_draft: p.is_draft,
        created_by: p.created_by,
        url: p.url,
        mine,
    };
    Ok(prs
        .mine
        .into_iter()
        .map(|p| row(p, true))
        .chain(prs.review.into_iter().map(|p| row(p, false)))
        .collect())
}

/// `git for-each-ref --format=%(refname) refs/heads refs/remotes/<remote>`
/// as branches: local ones first (by name), then the remote's that have
/// no local branch. The remote's `HEAD` is not a branch.
pub fn parse_branches(refs: &str, remote: &str) -> Vec<AdoBranch> {
    let mut out: Vec<AdoBranch> = Vec::new();
    let remote_prefix = format!("refs/remotes/{remote}/");
    for line in refs.lines().map(str::trim) {
        if let Some(name) = line.strip_prefix("refs/heads/") {
            match out.iter_mut().find(|b| b.name == name) {
                Some(b) => b.local = true,
                None => out.push(AdoBranch {
                    name: name.into(),
                    local: true,
                    remote: None,
                }),
            }
        } else if let Some(name) = line.strip_prefix(&remote_prefix) {
            if name == "HEAD" {
                continue;
            }
            let at = Some(format!("{remote}/{name}"));
            match out.iter_mut().find(|b| b.name == name) {
                Some(b) => b.remote = at,
                None => out.push(AdoBranch {
                    name: name.into(),
                    local: false,
                    remote: at,
                }),
            }
        }
    }
    out.sort_by(|a, b| b.local.cmp(&a.local).then_with(|| a.name.cmp(&b.name)));
    out
}

/// Where a tab's list stands.
#[derive(Debug, Clone, PartialEq, Default)]
pub enum Load<T> {
    #[default]
    Loading,
    Ready(T),
    Failed(String),
}

/// The overlay.
#[derive(Debug, Clone, Default)]
pub struct AdoView {
    pub project: Option<ProjectId>,
    pub tab: AdoTab,
    pub tasks: Load<TaskRead>,
    pub prs: Load<Vec<AdoPr>>,
    pub branches: Load<Vec<AdoBranch>>,
    /// Cursor per tab, by [`AdoTab::index`].
    pub cursor: [usize; 3],
    /// The task a first `s` armed: a second `s` on it starts it.
    pub armed: Option<u32>,
    pub area: Rect,
    pub list_area: Rect,
    /// The tab strip's cells, by tab, written back during draw.
    pub tab_hits: Vec<(Rect, AdoTab)>,
    pub scroll: usize,
}

impl AdoView {
    /// Rows the current tab lists.
    pub fn len(&self) -> usize {
        match self.tab {
            AdoTab::Tasks => match &self.tasks {
                Load::Ready(r) => r.tasks.len(),
                _ => 0,
            },
            AdoTab::PullRequests => match &self.prs {
                Load::Ready(p) => p.len(),
                _ => 0,
            },
            AdoTab::Branches => match &self.branches {
                Load::Ready(b) => b.len(),
                _ => 0,
            },
        }
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn cursor(&self) -> usize {
        self.cursor[self.tab.index()].min(self.len().saturating_sub(1))
    }

    pub fn step(&mut self, delta: i64) {
        let at = crate::app::clamp_selection(self.cursor() as i64 + delta, self.len());
        self.cursor[self.tab.index()] = at;
        self.armed = None;
    }

    pub fn select(&mut self, row: usize) {
        self.cursor[self.tab.index()] = row.min(self.len().saturating_sub(1));
        self.armed = None;
    }

    pub fn set_tab(&mut self, tab: AdoTab) {
        self.tab = tab;
        self.armed = None;
        self.scroll = 0;
    }

    pub fn task(&self) -> Option<&AdoTask> {
        match (&self.tab, &self.tasks) {
            (AdoTab::Tasks, Load::Ready(r)) => r.tasks.get(self.cursor()),
            _ => None,
        }
    }

    pub fn pr(&self) -> Option<&AdoPr> {
        match (&self.tab, &self.prs) {
            (AdoTab::PullRequests, Load::Ready(p)) => p.get(self.cursor()),
            _ => None,
        }
    }

    pub fn branch(&self) -> Option<&AdoBranch> {
        match (&self.tab, &self.branches) {
            (AdoTab::Branches, Load::Ready(b)) => b.get(self.cursor()),
            _ => None,
        }
    }
}

/// What a read off the loop hands back (`view_jobs::Answer::Ado`).
#[derive(Debug)]
pub enum AdoAnswer {
    /// Whether `project`'s remote is on Azure DevOps.
    Remote { project: ProjectId, ado: bool },
    Tasks {
        project: ProjectId,
        result: Result<TaskRead, String>,
    },
    Prs {
        project: ProjectId,
        result: Result<Vec<AdoPr>, String>,
    },
    Branches {
        project: ProjectId,
        result: Result<Vec<AdoBranch>, String>,
    },
    /// `kete ado start-task` finished: what to say in the footer.
    Started(Result<String, String>),
}

/// `kete ado --project <project> <args…>` with this very binary, its
/// stdout on success, else the last line it wrote to stderr. Blocking.
pub fn run_cli(project: &ProjectId, args: &[&str]) -> Result<String, String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let mut cmd = std::process::Command::new(exe);
    cmd.arg("ado")
        .arg("--project")
        .arg(&project.0)
        .args(args)
        .stdin(std::process::Stdio::null());
    #[cfg(windows)]
    nebula_core::platform::no_window(&mut cmd);
    let out = cmd.output().map_err(|e| e.to_string())?;
    if out.status.success() {
        return Ok(String::from_utf8_lossy(&out.stdout).into_owned());
    }
    let err = String::from_utf8_lossy(&out.stderr);
    Err(err
        .lines()
        .rev()
        .map(str::trim)
        .find(|l| !l.is_empty())
        .unwrap_or("kete ado failed")
        .trim_start_matches("Error: ")
        .to_string())
}

/// `git for-each-ref` of `repo`'s branches and its remote's. Blocking.
pub fn read_branches(repo: &Path) -> Result<Vec<AdoBranch>, String> {
    let remote = {
        let mut cmd = std::process::Command::new("git");
        cmd.arg("-C").arg(repo).arg("remote");
        #[cfg(windows)]
        nebula_core::platform::no_window(&mut cmd);
        cmd.output()
            .ok()
            .map(|o| String::from_utf8_lossy(&o.stdout).to_string())
            .and_then(|s| {
                let names: Vec<String> = s.lines().map(|l| l.trim().to_string()).collect();
                names
                    .iter()
                    .find(|n| *n == "origin")
                    .or(names.first())
                    .cloned()
            })
            .unwrap_or_default()
    };
    let mut cmd = std::process::Command::new("git");
    cmd.arg("-C")
        .arg(repo)
        .args(["for-each-ref", "--format=%(refname)", "refs/heads"]);
    if !remote.is_empty() {
        cmd.arg(format!("refs/remotes/{remote}"));
    }
    #[cfg(windows)]
    nebula_core::platform::no_window(&mut cmd);
    let out = cmd.output().map_err(|e| e.to_string())?;
    if !out.status.success() {
        return Err(String::from_utf8_lossy(&out.stderr).trim().to_string());
    }
    Ok(parse_branches(
        &String::from_utf8_lossy(&out.stdout),
        &remote,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ado_remotes_are_told_from_the_rest() {
        assert!(is_ado_remote("https://dev.azure.com/org/proj/_git/repo"));
        assert!(is_ado_remote("git@ssh.dev.azure.com:v3/org/proj/repo"));
        assert!(is_ado_remote("https://org.visualstudio.com/proj/_git/repo"));
        assert!(!is_ado_remote("git@github.com:o/r.git"));
    }

    #[test]
    fn a_task_list_reads_its_sprint_and_its_items_under_their_parents() {
        let json = r#"{
            "iteration": {"id": "i", "name": "Sprint 12", "path": "p"},
            "stale": false,
            "groups": [
                {"parent": {"id": 1, "kind": "User Story", "title": "Login", "state": "Active",
                            "stateCategory": "InProgress", "url": "u1", "descriptionMd": "", "acceptanceMd": ""},
                 "items": [{"id": 2, "kind": "Task", "title": "Fix redirect", "state": "New",
                            "stateCategory": "Proposed", "assignedTo": "Jay", "url": "u2",
                            "descriptionMd": "", "acceptanceMd": ""}]},
                {"parent": null,
                 "items": [{"id": 3, "kind": "Bug", "title": "Crash", "state": "Active",
                            "stateCategory": "InProgress", "url": "u3",
                            "descriptionMd": "", "acceptanceMd": ""}]}
            ]
        }"#;
        let read = parse_tasks(json).unwrap();
        assert_eq!(read.header, "Sprint 12");
        assert_eq!(read.tasks.len(), 2);
        assert_eq!(read.tasks[0].parent.as_deref(), Some("Login"));
        assert_eq!(read.tasks[0].assigned_to.as_deref(), Some("Jay"));
        assert_eq!(read.tasks[1].parent, None);

        let saved = parse_tasks(
            r#"{"iteration": null, "groups": [], "cachedAt": 5, "cachedReason": "offline"}"#,
        )
        .unwrap();
        assert_eq!(saved.header, "no current sprint · saved list (offline)");
        assert!(parse_tasks("not json").is_err());
    }

    #[test]
    fn pull_requests_read_yours_then_your_reviews() {
        let json = r#"{"mine": [{"id": 7, "title": "Mine", "source": "jr/a", "target": "main",
                        "isDraft": true, "createdBy": "Jay", "url": "u7"}],
                      "review": [{"id": 9, "title": "Theirs", "source": "x/b", "target": "main",
                        "isDraft": false, "createdBy": "Sam", "url": "u9"}]}"#;
        let prs = parse_prs(json).unwrap();
        assert_eq!(prs.len(), 2);
        assert!(prs[0].mine && prs[0].is_draft);
        assert!(!prs[1].mine);
        assert_eq!(prs[1].created_by, "Sam");
    }

    #[test]
    fn branches_merge_local_and_remote_and_skip_head() {
        let refs = "refs/heads/main\nrefs/heads/jr/fix\nrefs/remotes/origin/HEAD\nrefs/remotes/origin/main\nrefs/remotes/origin/sam/feat\n";
        let b = parse_branches(&refs.replace("origin", "upstream"), "upstream");
        let names: Vec<(&str, bool, Option<&str>)> = b
            .iter()
            .map(|b| (b.name.as_str(), b.local, b.remote.as_deref()))
            .collect();
        assert_eq!(
            names,
            vec![
                ("jr/fix", true, None),
                ("main", true, Some("upstream/main")),
                ("sam/feat", false, Some("upstream/sam/feat"))
            ]
        );
    }
}
