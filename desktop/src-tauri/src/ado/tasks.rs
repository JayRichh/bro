//! Sprint tasks: the team's iterations, the work items in a scope grouped
//! by parent, and starting a task as a branch, a worktree and an agent.

use crate::ado::client::{AdoError, CmdError, ErrorKind};
use crate::ado::naming::validate_branch_name;
use crate::ado::{enc, git_out, AdoContext, AdoState, Me, WitState};
use crate::daemon::Client;
use crate::NewAgent;
use nebula_core::{ClientRequest, EntityId, ProjectId};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;
use tauri::State;

/// Work items hydrated per `workitemsbatch` call (the API's limit).
pub const BATCH: usize = 200;
/// Most ids a WIQL query returns.
const WIQL_TOP: usize = 500;

/// Which tasks to list. The two sprint "mine" scopes show the whole
/// iteration's worth of my work, Completed items included (Removed ones
/// never); the two "open" scopes leave Completed and Removed out.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TaskScope {
    /// The team's current iteration, my items, Completed included.
    CurrentSprint,
    /// A chosen iteration (by id), my items, Completed included.
    Iteration(String),
    /// Everything open assigned to me, in the team's areas.
    MyOpen,
    /// Everyone's open items in the current iteration.
    SprintAll,
}

impl TaskScope {
    /// The `ado.json` cache entry for this scope's last list.
    fn cache_name(&self, team: Option<&str>) -> String {
        let scope = match self {
            TaskScope::CurrentSprint => "currentSprint".to_string(),
            TaskScope::Iteration(id) => format!("iteration:{id}"),
            TaskScope::MyOpen => "myOpen".to_string(),
            TaskScope::SprintAll => "sprintAll".to_string(),
        };
        format!("tasks:{}:{scope}", team.unwrap_or(""))
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Iteration {
    pub id: String,
    pub name: String,
    pub path: String,
    pub start: Option<String>,
    pub finish: Option<String>,
    pub timeframe: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkItem {
    pub id: u32,
    pub kind: String,
    pub title: String,
    pub state: String,
    pub state_category: String,
    pub assigned_to: Option<String>,
    pub remaining: Option<f64>,
    pub parent: Option<u32>,
    pub board_column: Option<String>,
    pub description_md: String,
    pub acceptance_md: String,
    pub url: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskList {
    pub iteration: Option<Iteration>,
    pub stale: bool,
    pub next: Option<Iteration>,
    pub groups: Vec<TaskGroup>,
    /// Set when ADO could not be reached or signed in to and this is the
    /// last list saved for the scope: when it was saved, in Unix
    /// milliseconds.
    #[serde(default)]
    pub cached_at: Option<i64>,
    /// Why the saved list is shown: `offline`, or `auth` (signed out, so
    /// the window offers "Sign in again").
    #[serde(default)]
    pub cached_because: Option<ErrorKind>,
    /// The failure behind a saved list, in words.
    #[serde(default)]
    pub cached_reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskGroup {
    pub parent: Option<WorkItem>,
    pub items: Vec<WorkItem>,
}

// ---- ADO reply shapes ----

#[derive(Deserialize)]
struct List<T> {
    value: Vec<T>,
}

#[derive(Deserialize)]
struct RawIteration {
    id: String,
    name: String,
    #[serde(default)]
    path: String,
    #[serde(default)]
    attributes: RawIterationAttrs,
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct RawIterationAttrs {
    start_date: Option<String>,
    finish_date: Option<String>,
    time_frame: Option<String>,
}

impl From<RawIteration> for Iteration {
    fn from(r: RawIteration) -> Self {
        Iteration {
            id: r.id,
            name: r.name,
            path: r.path,
            start: r.attributes.start_date,
            finish: r.attributes.finish_date,
            timeframe: r.attributes.time_frame,
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct IterationItems {
    #[serde(default)]
    work_item_relations: Vec<IterationRelation>,
}

#[derive(Deserialize)]
struct IterationRelation {
    source: Option<IdRef>,
    target: Option<IdRef>,
}

#[derive(Deserialize)]
struct IdRef {
    id: u32,
}

#[derive(Deserialize)]
struct RawItem {
    id: u32,
    #[serde(default)]
    fields: serde_json::Map<String, Value>,
    #[serde(default)]
    relations: Option<Vec<RawRelation>>,
}

#[derive(Deserialize)]
struct RawRelation {
    rel: String,
    url: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct WiqlReply {
    #[serde(default)]
    work_items: Vec<IdRef>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct TeamFieldValues {
    field: FieldRef,
    #[serde(default)]
    values: Vec<TeamFieldValue>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct FieldRef {
    reference_name: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct TeamFieldValue {
    value: String,
    #[serde(default)]
    include_children: bool,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawType {
    name: String,
    #[serde(default)]
    is_disabled: bool,
}

/// A hydrated item plus who it is assigned to, for "mine" filters.
#[derive(Debug, Clone)]
struct Hydrated {
    item: WorkItem,
    assignee_id: Option<String>,
    assignee_unique: Option<String>,
}

impl Hydrated {
    fn is_mine(&self, me: &Me) -> bool {
        self.assignee_id.as_deref() == Some(me.id.as_str())
            || (!me.unique_name.is_empty()
                && self
                    .assignee_unique
                    .as_deref()
                    .is_some_and(|u| u.eq_ignore_ascii_case(&me.unique_name)))
    }
}

fn is_closed(category: &str) -> bool {
    category == "Completed" || category == "Removed"
}

// ---- reads ----

/// The project's team names.
pub async fn teams(ctx: &AdoContext) -> Result<Vec<String>, AdoError> {
    #[derive(Deserialize)]
    struct Team {
        name: String,
    }
    let list: List<Team> = ctx
        .client
        .get(&format!(
            "/_apis/projects/{}/teams?$top=500",
            enc(&ctx.remote.project)
        ))
        .await?;
    let mut names: Vec<String> = list.value.into_iter().map(|t| t.name).collect();
    names.sort_by_key(|n| n.to_lowercase());
    Ok(names)
}

/// The team's iterations, by start date (undated ones last).
pub async fn iterations(ctx: &AdoContext) -> Result<Vec<Iteration>, AdoError> {
    let team = ctx.team().await?;
    all_iterations(ctx, &team).await
}

async fn all_iterations(ctx: &AdoContext, team: &str) -> Result<Vec<Iteration>, AdoError> {
    let list: List<RawIteration> = ctx
        .client
        .get(&ctx.team_api(team, "work/teamsettings/iterations"))
        .await?;
    let mut its: Vec<Iteration> = list.value.into_iter().map(Iteration::from).collect();
    its.sort_by(|a, b| match (&a.start, &b.start) {
        (Some(x), Some(y)) => x.cmp(y),
        (Some(_), None) => std::cmp::Ordering::Less,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        (None, None) => a.name.cmp(&b.name),
    });
    Ok(its)
}

async fn current_iteration(ctx: &AdoContext, team: &str) -> Result<Option<Iteration>, AdoError> {
    let list: List<RawIteration> = ctx
        .client
        .get(&ctx.team_api(team, "work/teamsettings/iterations?$timeframe=current"))
        .await?;
    Ok(list.value.into_iter().next().map(Iteration::from))
}

/// `YYYY-MM-DD` of an ADO date.
fn day(date: &str) -> &str {
    date.get(..10).unwrap_or(date)
}

/// Whether `current` has ended before `today` (`YYYY-MM-DD`; the finish
/// day itself is still in the sprint), and the iteration after it by
/// start date.
pub fn stale_and_next(
    current: &Iteration,
    all: &[Iteration],
    today: &str,
) -> (bool, Option<Iteration>) {
    let stale = current.finish.as_deref().is_some_and(|f| day(f) < today);
    let after = current
        .start
        .as_deref()
        .or(current.finish.as_deref())
        .map(day);
    let next = after.and_then(|after| {
        all.iter()
            .filter(|i| i.id != current.id)
            .filter(|i| i.start.as_deref().is_some_and(|s| day(s) > after))
            .min_by(|a, b| a.start.cmp(&b.start))
            .cloned()
    });
    (stale, next)
}

/// Today in UTC as `YYYY-MM-DD`.
pub fn today_utc() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let (y, m, d) = civil_from_days(secs.div_euclid(86_400));
    format!("{y:04}-{m:02}-{d:02}")
}

/// Howard Hinnant's days-since-epoch to (year, month, day).
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

/// The tasks in `scope`. `today` is `YYYY-MM-DD`.
pub async fn tasks(ctx: &AdoContext, scope: &TaskScope, today: &str) -> Result<TaskList, AdoError> {
    let team = ctx.team().await?;
    let mut states: HashMap<String, Vec<WitState>> = HashMap::new();
    if *scope == TaskScope::MyOpen {
        let ids = my_open_ids(ctx, &team).await?;
        let items = hydrate(ctx, &ids, &mut states).await?;
        let shown: Vec<Hydrated> = items
            .into_iter()
            .filter(|h| !is_closed(&h.item.state_category))
            .collect();
        let groups = finish_groups(ctx, shown, &mut states).await?;
        return Ok(TaskList {
            iteration: None,
            stale: false,
            next: None,
            groups,
            cached_at: None,
            cached_because: None,
            cached_reason: None,
        });
    }

    let all = all_iterations(ctx, &team).await?;
    // ADO names no current iteration once the last one has ended (by the
    // org's own date, which can be a day ahead of UTC): that one is shown,
    // stale, whatever today's UTC date says.
    let mut ended = false;
    let iteration = match scope {
        TaskScope::Iteration(id) => all.iter().find(|i| &i.id == id).cloned(),
        _ => match current_iteration(ctx, &team).await? {
            Some(i) => Some(i),
            None => {
                ended = true;
                all.iter()
                    .filter(|i| i.start.as_deref().is_some_and(|s| day(s) <= today))
                    .max_by(|a, b| a.start.cmp(&b.start))
                    .cloned()
            }
        },
    };
    let Some(iteration) = iteration else {
        return Ok(TaskList {
            iteration: None,
            stale: false,
            next: None,
            groups: Vec::new(),
            cached_at: None,
            cached_because: None,
            cached_reason: None,
        });
    };
    let (stale, next) = stale_and_next(&iteration, &all, today);
    let stale = (stale || ended) && !matches!(scope, TaskScope::Iteration(_));

    let reply: IterationItems = ctx
        .client
        .get(&ctx.team_api(
            &team,
            &format!(
                "work/teamsettings/iterations/{}/workitems",
                enc(&iteration.id)
            ),
        ))
        .await?;
    let mut ids = Vec::new();
    let mut seen = HashSet::new();
    for r in &reply.work_item_relations {
        for id in [r.source.as_ref(), r.target.as_ref()].into_iter().flatten() {
            if seen.insert(id.id) {
                ids.push(id.id);
            }
        }
    }
    if ids.is_empty() {
        return Ok(TaskList {
            iteration: Some(iteration),
            stale,
            next,
            groups: Vec::new(),
            cached_at: None,
            cached_because: None,
            cached_reason: None,
        });
    }

    let items = hydrate(ctx, &ids, &mut states).await?;
    let shown: Vec<Hydrated> = if *scope == TaskScope::SprintAll {
        items
            .into_iter()
            .filter(|h| !is_closed(&h.item.state_category))
            .collect()
    } else {
        let me = ctx.me().await?;
        items
            .into_iter()
            .filter(|h| h.item.state_category != "Removed" && h.is_mine(&me))
            .collect()
    };
    let groups = finish_groups(ctx, shown, &mut states).await?;
    Ok(TaskList {
        iteration: Some(iteration),
        stale,
        next,
        groups,
        cached_at: None,
        cached_because: None,
        cached_reason: None,
    })
}

/// Whether `e` means ADO could not be reached or signed in to, when the
/// last saved list beats an error. The list then says which it was.
fn falls_back(e: &AdoError) -> bool {
    matches!(e.kind(), ErrorKind::Offline | ErrorKind::Auth)
}

/// [`tasks`], saving each list in `ado.json`. When ADO cannot be reached
/// or signed in to, the last list saved for the scope comes back instead,
/// with `cached_at` set and `cached_because` saying which (a signed-out
/// list is never passed off as offline).
pub async fn tasks_or_cached(
    ctx: &AdoContext,
    scope: &TaskScope,
    today: &str,
) -> Result<TaskList, AdoError> {
    let team = ctx.store.read_repo(&ctx.key, |r| r.team.clone());
    let name = scope.cache_name(team.as_deref());
    match tasks(ctx, scope, today).await {
        Ok(list) => {
            let value = serde_json::to_value(&list)
                .map_err(|e| AdoError::Local(format!("Could not save the task list: {e}")))?;
            ctx.store.cache_put(&ctx.key, &name, value);
            Ok(list)
        }
        Err(e) if falls_back(&e) => {
            let saved = ctx
                .store
                .read_repo(&ctx.key, |r| r.cache.get(&name).cloned());
            let cached = saved.and_then(|c| {
                serde_json::from_value::<TaskList>(c.value)
                    .ok()
                    .map(|l| (l, c.at))
            });
            match cached {
                Some((mut list, at)) => {
                    list.cached_at = Some(at);
                    list.cached_because = Some(e.kind());
                    list.cached_reason = Some(e.user_message());
                    Ok(list)
                }
                None => Err(e),
            }
        }
        Err(e) => Err(e),
    }
}

/// Remember `team` for this repo's views; empty goes back to the
/// project's default team.
pub fn set_team(ctx: &AdoContext, team: &str) -> Result<(), AdoError> {
    let team = team.trim();
    ctx.store
        .try_with_repo(&ctx.key, |r| {
            r.team = (!team.is_empty()).then(|| team.to_string());
        })
        .map_err(|e| AdoError::Local(format!("Could not save the team: {e}")))
}

/// Work item `id` and its parent, hydrated.
async fn item_and_parent(
    ctx: &AdoContext,
    id: u32,
    states: &mut HashMap<String, Vec<WitState>>,
) -> Result<(Hydrated, Option<WorkItem>), AdoError> {
    let mut found = hydrate(ctx, &[id], states).await?;
    if found.is_empty() {
        return Err(AdoError::NotFound);
    }
    let item = found.remove(0);
    let parent = match item.item.parent {
        Some(p) => hydrate(ctx, &[p], states)
            .await
            .ok()
            .and_then(|mut v| (!v.is_empty()).then(|| v.remove(0).item)),
        None => None,
    };
    Ok((item, parent))
}

/// The prompt Start would give an agent for task `id`.
pub async fn prompt_for(ctx: &AdoContext, id: u32) -> Result<String, AdoError> {
    let (item, parent) = item_and_parent(ctx, id, &mut HashMap::new()).await?;
    Ok(task_prompt(&item.item, parent.as_ref()))
}

/// Fetch the parents `shown` points at that it does not hold, fill each
/// item's empty acceptance criteria from its parent, and group.
async fn finish_groups(
    ctx: &AdoContext,
    shown: Vec<Hydrated>,
    states: &mut HashMap<String, Vec<WitState>>,
) -> Result<Vec<TaskGroup>, AdoError> {
    let mut known: HashMap<u32, WorkItem> =
        shown.iter().map(|h| (h.item.id, h.item.clone())).collect();
    let missing: Vec<u32> = {
        let mut seen = HashSet::new();
        shown
            .iter()
            .filter_map(|h| h.item.parent)
            .filter(|p| !known.contains_key(p) && seen.insert(*p))
            .collect()
    };
    if !missing.is_empty() {
        for h in hydrate(ctx, &missing, states).await? {
            known.insert(h.item.id, h.item);
        }
    }
    let items: Vec<WorkItem> = shown
        .into_iter()
        .map(|h| {
            let mut item = h.item;
            if item.acceptance_md.is_empty() {
                if let Some(p) = item.parent.and_then(|p| known.get(&p)) {
                    item.acceptance_md = p.acceptance_md.clone();
                }
            }
            item
        })
        .collect();
    Ok(group(items, &known))
}

/// Group `items` under their parents, in first-seen order, with the
/// parentless group last. An item that heads a group is not also listed.
fn group(items: Vec<WorkItem>, known: &HashMap<u32, WorkItem>) -> Vec<TaskGroup> {
    let heads: HashSet<u32> = items.iter().filter_map(|i| i.parent).collect();
    let mut order: Vec<Option<u32>> = Vec::new();
    let mut by_parent: HashMap<Option<u32>, Vec<WorkItem>> = HashMap::new();
    for item in items {
        if heads.contains(&item.id) {
            continue;
        }
        let key = item.parent.filter(|p| known.contains_key(p));
        if !by_parent.contains_key(&key) {
            order.push(key);
        }
        by_parent.entry(key).or_default().push(item);
    }
    order.sort_by_key(|k| k.is_none());
    order
        .into_iter()
        .map(|k| TaskGroup {
            parent: k.and_then(|p| known.get(&p).cloned()),
            items: by_parent.remove(&k).unwrap_or_default(),
        })
        .collect()
}

/// `POST wit/workitemsbatch` in batches of [`BATCH`], keeping `ids` order.
async fn hydrate(
    ctx: &AdoContext,
    ids: &[u32],
    states: &mut HashMap<String, Vec<WitState>>,
) -> Result<Vec<Hydrated>, AdoError> {
    let mut raw: HashMap<u32, RawItem> = HashMap::new();
    for chunk in ids.chunks(BATCH) {
        let body = json!({
            "ids": chunk,
            "$expand": "relations",
            "errorPolicy": "omit",
        });
        let reply: List<Option<RawItem>> = ctx
            .client
            .post_json(&ctx.project_api("wit/workitemsbatch"), &body)
            .await?;
        for item in reply.value.into_iter().flatten() {
            raw.insert(item.id, item);
        }
    }
    let mut out = Vec::new();
    for id in ids {
        let Some(r) = raw.remove(id) else { continue };
        let kind = str_field(&r.fields, "System.WorkItemType");
        if !states.contains_key(&kind) {
            // A type whose states cannot be read shows its state with no
            // category rather than failing the list.
            let s = ctx.states(&kind).await.unwrap_or_default();
            states.insert(kind.clone(), s);
        }
        out.push(to_item(ctx, r, &states[&kind]));
    }
    Ok(out)
}

fn str_field(fields: &serde_json::Map<String, Value>, name: &str) -> String {
    fields
        .get(name)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

/// The id at the end of a `.../workItems/{id}` URL.
fn id_from_url(url: &str) -> Option<u32> {
    url.rsplit('/').next()?.parse().ok()
}

fn to_item(ctx: &AdoContext, r: RawItem, states: &[WitState]) -> Hydrated {
    let f = &r.fields;
    let kind = str_field(f, "System.WorkItemType");
    let state = str_field(f, "System.State");
    let state_category = states
        .iter()
        .find(|s| s.name == state)
        .map(|s| s.category.clone())
        .unwrap_or_default();
    let assignee = f.get("System.AssignedTo");
    let parent = f
        .get("System.Parent")
        .and_then(Value::as_u64)
        .map(|p| p as u32)
        .or_else(|| {
            r.relations
                .iter()
                .flatten()
                .find(|rel| rel.rel == "System.LinkTypes.Hierarchy-Reverse")
                .and_then(|rel| id_from_url(&rel.url))
        });
    let mut description = html_to_md(&str_field(f, "System.Description"));
    if description.is_empty() {
        description = html_to_md(&str_field(f, "Microsoft.VSTS.TCM.ReproSteps"));
    }
    Hydrated {
        assignee_id: assignee
            .and_then(|a| a.get("id"))
            .and_then(Value::as_str)
            .map(str::to_string),
        assignee_unique: assignee
            .and_then(|a| a.get("uniqueName"))
            .and_then(Value::as_str)
            .map(str::to_string),
        item: WorkItem {
            id: r.id,
            title: str_field(f, "System.Title"),
            state,
            state_category,
            assigned_to: assignee
                .and_then(|a| a.get("displayName").and_then(Value::as_str).or(a.as_str()))
                .map(str::to_string),
            remaining: f
                .get("Microsoft.VSTS.Scheduling.RemainingWork")
                .and_then(Value::as_f64),
            parent,
            board_column: f
                .get("System.BoardColumn")
                .and_then(Value::as_str)
                .filter(|s| !s.is_empty())
                .map(str::to_string),
            description_md: description,
            acceptance_md: html_to_md(&str_field(f, "Microsoft.VSTS.Common.AcceptanceCriteria")),
            url: ctx.work_item_web_url(r.id),
            kind,
        },
    }
}

/// WIQL string literal.
fn wiql_str(s: &str) -> String {
    format!("'{}'", s.replace('\'', "''"))
}

/// The WIQL for "all my open work": assigned to me, in the team's areas,
/// and not in a Completed or Removed state of its type.
pub fn my_open_wiql(
    area_field: &str,
    areas: &[(String, bool)],
    closed_by_type: &[(String, Vec<String>)],
) -> String {
    let mut q = String::from(
        "SELECT [System.Id] FROM WorkItems WHERE [System.TeamProject] = @project AND [System.AssignedTo] = @Me",
    );
    if !areas.is_empty() {
        let parts: Vec<String> = areas
            .iter()
            .map(|(path, children)| {
                let op = if *children { "UNDER" } else { "=" };
                format!("[{area_field}] {op} {}", wiql_str(path))
            })
            .collect();
        q.push_str(&format!(" AND ({})", parts.join(" OR ")));
    }
    let closed: Vec<String> = closed_by_type
        .iter()
        .filter(|(_, names)| !names.is_empty())
        .map(|(kind, names)| {
            let names: Vec<String> = names.iter().map(|n| wiql_str(n)).collect();
            format!(
                "([System.WorkItemType] = {} AND [System.State] IN ({}))",
                wiql_str(kind),
                names.join(", ")
            )
        })
        .collect();
    if !closed.is_empty() {
        q.push_str(&format!(" AND NOT ({})", closed.join(" OR ")));
    }
    q.push_str(" ORDER BY [System.ChangedDate] DESC");
    q
}

async fn my_open_ids(ctx: &AdoContext, team: &str) -> Result<Vec<u32>, AdoError> {
    let tfv: TeamFieldValues = ctx
        .client
        .get(&ctx.team_api(team, "work/teamsettings/teamfieldvalues"))
        .await?;
    let areas: Vec<(String, bool)> = tfv
        .values
        .into_iter()
        .map(|v| (v.value, v.include_children))
        .collect();
    let types: List<RawType> = ctx
        .client
        .get(&ctx.project_api("wit/workitemtypes"))
        .await?;
    let mut closed_by_type = Vec::new();
    for t in types.value.into_iter().filter(|t| !t.is_disabled) {
        // A type whose states cannot be read gets no exclusion clause
        // rather than failing the list; its closed items are filtered out
        // after hydration if their category is known by then.
        let Ok(states) = ctx.states(&t.name).await else {
            continue;
        };
        let closed: Vec<String> = states
            .into_iter()
            .filter(|s| is_closed(&s.category))
            .map(|s| s.name)
            .collect();
        closed_by_type.push((t.name, closed));
    }
    let query = my_open_wiql(&tfv.field.reference_name, &areas, &closed_by_type);
    let reply: WiqlReply = ctx
        .client
        .post_json(
            &ctx.project_api(&format!("wit/wiql?$top={WIQL_TOP}")),
            &json!({ "query": query }),
        )
        .await?;
    Ok(reply.work_items.into_iter().map(|w| w.id).collect())
}

// ---- start a task ----

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StartTask {
    pub id: u32,
    pub branch: String,
    /// The branch to cut from; None is the daemon's default base (the
    /// configured `worktree_base_branch`, else `origin/HEAD`).
    pub base: Option<String>,
    pub assign_to_me: bool,
    pub activate: bool,
    /// The agent to start in the new worktree. Its `worktree` is filled
    /// in; an empty prompt becomes [`task_prompt`].
    pub spawn: Option<NewAgent>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StartedTask {
    pub worktree_id: String,
    pub agent_id: Option<String>,
    /// Steps after the worktree that did not happen, one each. The
    /// worktree and branch stay either way.
    pub problems: Vec<StepProblem>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StepProblem {
    /// `link`, `read`, `assign`, `activate` or `spawn`.
    pub step: String,
    pub message: String,
}

/// What starting a task needs from git and the daemon.
#[async_trait::async_trait]
pub trait Workspace: Send + Sync {
    /// Branch names already taken: local branches, and branches checked
    /// out in one of the project's worktrees.
    async fn taken_branches(&self) -> Result<Vec<String>, String>;
    /// Cut a worktree on the new `branch` from `base`; its id.
    async fn create_worktree(&self, branch: &str, base: Option<&str>) -> Result<String, String>;
    /// Start an agent; its id.
    async fn create_agent(&self, spec: NewAgent) -> Result<String, String>;
}

/// The agent's opening prompt: the task's title, description and
/// acceptance criteria (the parent's when the task has none).
pub fn task_prompt(item: &WorkItem, parent: Option<&WorkItem>) -> String {
    let mut s = format!(
        "# {} {}: {}\n\n{}\n",
        item.kind, item.id, item.title, item.url
    );
    if let Some(p) = parent {
        s.push_str(&format!("\nParent: {} {}: {}\n", p.kind, p.id, p.title));
    }
    let description = if item.description_md.is_empty() {
        "None given."
    } else {
        &item.description_md
    };
    let acceptance = match (&item.acceptance_md, parent) {
        (a, _) if !a.is_empty() => a.as_str(),
        (_, Some(p)) if !p.acceptance_md.is_empty() => p.acceptance_md.as_str(),
        _ => "None given.",
    };
    s.push_str(&format!(
        "\n## Description\n\n{description}\n\n## Acceptance criteria\n\n{acceptance}\n"
    ));
    s
}

/// Start task `req.id`: a new branch and worktree, the branch-to-task
/// link, then (each optional, each reported on its own) assign it to me,
/// move it to In Progress and start an agent. Only a bad branch name or a
/// failed worktree is an error; nothing after the worktree undoes it.
pub async fn start_task(
    ctx: &AdoContext,
    ws: &dyn Workspace,
    req: StartTask,
) -> Result<StartedTask, CmdError> {
    validate_branch_name(&req.branch, &[])?;
    let taken = ws.taken_branches().await?;
    validate_branch_name(&req.branch, &taken)?;
    let base = match req.base.as_deref().map(str::trim).filter(|b| !b.is_empty()) {
        Some(b) => Some(b.to_string()),
        // The repo's ADO default branch, never whatever origin/HEAD says.
        None => ctx.default_branch_ref().await.map_err(|e| CmdError {
            kind: e.kind(),
            message: format!(
                "Could not read the repo's default branch, so no base to cut from: {}",
                e.user_message()
            ),
        })?,
    };
    let worktree_id = ws.create_worktree(&req.branch, base.as_deref()).await?;
    let mut out = StartedTask {
        worktree_id,
        ..Default::default()
    };
    let mut problem = |step: &str, message: String| {
        out.problems.push(StepProblem {
            step: step.to_string(),
            message,
        })
    };

    if let Err(e) = ctx.store.try_with_repo(&ctx.key, |r| {
        r.branch_tasks.insert(req.branch.clone(), req.id);
    }) {
        problem(
            "link",
            format!("The branch is made, but its link to the task was not saved: {e}"),
        );
    }

    let (item, parent) = match item_and_parent(ctx, req.id, &mut HashMap::new()).await {
        Ok((item, parent)) => (Some(item), parent),
        Err(AdoError::NotFound) => {
            problem("read", format!("Work item {} was not found.", req.id));
            (None, None)
        }
        Err(e) => {
            problem("read", e.user_message());
            (None, None)
        }
    };
    let item_path = ctx.project_api(&format!("wit/workitems/{}", req.id));

    if req.assign_to_me {
        match &item {
            None => problem(
                "assign",
                "Not assigned: the work item could not be read.".into(),
            ),
            Some(h) if h.assignee_id.is_some() || h.assignee_unique.is_some() => {
                match ctx.me().await {
                    Err(e) => problem(
                        "assign",
                        format!(
                            "Could not check who you are signed in as: {}",
                            e.user_message()
                        ),
                    ),
                    Ok(me) if h.is_mine(&me) => {}
                    Ok(_) => {
                        let who = h.item.assigned_to.as_deref().unwrap_or("someone else");
                        problem(
                            "assign",
                            format!("Left as it is: the task is assigned to {who}."),
                        );
                    }
                }
            }
            Some(_) => match ctx.me().await {
                Err(e) => problem("assign", e.user_message()),
                Ok(me) if me.unique_name.is_empty() => problem(
                    "assign",
                    "Not assigned: Azure DevOps did not say who you are signed in as.".into(),
                ),
                Ok(me) => {
                    let ops = json!([{
                        "op": "add",
                        "path": "/fields/System.AssignedTo",
                        "value": me.unique_name,
                    }]);
                    if let Err(e) = ctx.client.patch_json_patch::<Value>(&item_path, &ops).await {
                        problem("assign", e.user_message());
                    }
                }
            },
        }
    }

    if req.activate {
        match &item {
            None => problem(
                "activate",
                "Not moved to In Progress: the work item could not be read.".into(),
            ),
            Some(h) if h.item.state_category == "InProgress" => {}
            Some(h)
                if matches!(
                    h.item.state_category.as_str(),
                    "Resolved" | "Completed" | "Removed"
                ) =>
            {
                problem(
                    "activate",
                    format!("Left as it is: the task is already {}.", h.item.state),
                )
            }
            Some(h) => match ctx.states(&h.item.kind).await {
                Err(e) => problem("activate", e.user_message()),
                Ok(states) => match states.iter().find(|s| s.category == "InProgress") {
                    None => problem(
                        "activate",
                        format!("A {} has no In Progress state to move to.", h.item.kind),
                    ),
                    Some(s) => {
                        let ops = json!([{
                            "op": "add",
                            "path": "/fields/System.State",
                            "value": s.name,
                        }]);
                        if let Err(e) = ctx.client.patch_json_patch::<Value>(&item_path, &ops).await
                        {
                            problem("activate", e.user_message());
                        }
                    }
                },
            },
        }
    }

    if let Some(mut spec) = req.spawn {
        spec.worktree = out.worktree_id.clone();
        if spec.prompt.as_deref().is_none_or(|p| p.trim().is_empty()) {
            spec.prompt = Some(match &item {
                Some(h) => task_prompt(&h.item, parent.as_ref()),
                None => format!(
                    "Work on Azure DevOps work item {}: {}",
                    req.id,
                    ctx.work_item_web_url(req.id)
                ),
            });
        }
        match ws.create_agent(spec).await {
            Ok(id) => out.agent_id = Some(id),
            Err(e) => out.problems.push(StepProblem {
                step: "spawn".into(),
                message: format!("The agent did not start: {e}"),
            }),
        }
    }
    Ok(out)
}

/// Branch names `for-each-ref --format=%(refname)` output takes up: every
/// local branch, and every branch on `remote` (`refs/remotes/origin/x`
/// takes `x`). Other remotes' branches and `HEAD` do not count.
fn taken_from_refs(refs: &str, remote: &str) -> Vec<String> {
    let remote_prefix = format!("refs/remotes/{remote}/");
    refs.lines()
        .map(str::trim)
        .filter_map(|r| {
            r.strip_prefix("refs/heads/")
                .or_else(|| r.strip_prefix(&remote_prefix).filter(|b| *b != "HEAD"))
        })
        .filter(|b| !b.is_empty())
        .map(str::to_string)
        .collect()
}

/// [`Workspace`] over the daemon and the project's repo.
struct DaemonWorkspace {
    client: Arc<Client>,
    project: ProjectId,
    repo: PathBuf,
    remote_name: String,
}

#[async_trait::async_trait]
impl Workspace for DaemonWorkspace {
    async fn taken_branches(&self) -> Result<Vec<String>, String> {
        let refs = git_out(
            &self.repo,
            &[
                "for-each-ref",
                "--format=%(refname)",
                "refs/heads",
                "refs/remotes",
            ],
        )
        .await?;
        let mut taken = taken_from_refs(&refs, &self.remote_name);
        taken.extend(
            self.client
                .snapshot()
                .worktrees
                .iter()
                .filter(|w| w.project_id == self.project)
                .map(|w| w.branch.clone()),
        );
        Ok(taken)
    }

    async fn create_worktree(&self, branch: &str, base: Option<&str>) -> Result<String, String> {
        let req_id = self.client.req_id();
        let created = self
            .client
            .rpc(
                req_id,
                ClientRequest::CreateWorktree {
                    req_id,
                    project: self.project.clone(),
                    branch: branch.to_string(),
                    base: base.map(str::to_string),
                },
            )
            .await
            .map_err(|e| e.to_string())?;
        let id = match created {
            Some(EntityId::Worktree(id)) => id.0,
            _ => return Err("The daemon did not report the new worktree.".into()),
        };
        // The daemon cuts from an already-qualified `origin/main` with
        // tracking on, which would make main the task branch's upstream.
        // The branch is new work: it gets its own upstream when pushed.
        if base.is_some_and(|b| b.starts_with(&format!("{}/", self.remote_name))) {
            let _ = git_out(&self.repo, &["branch", "--unset-upstream", branch]).await;
        }
        Ok(id)
    }

    async fn create_agent(&self, spec: NewAgent) -> Result<String, String> {
        crate::spawn_agent(&self.client, spec).await
    }
}

// ---- commands ----

/// The project's teams, for the team picker.
#[tauri::command]
pub async fn ado_teams(
    client: State<'_, Arc<Client>>,
    ado: State<'_, AdoState>,
    worktree: String,
) -> Result<Vec<String>, CmdError> {
    let ctx = ado.context_for(&client, &worktree).await?;
    teams(&ctx).await.map_err(CmdError::from)
}

/// The team's iterations, by start date.
#[tauri::command]
pub async fn ado_iterations(
    client: State<'_, Arc<Client>>,
    ado: State<'_, AdoState>,
    worktree: String,
) -> Result<Vec<Iteration>, CmdError> {
    let ctx = ado.context_for(&client, &worktree).await?;
    iterations(&ctx).await.map_err(CmdError::from)
}

/// The tasks in `scope`, grouped by parent.
#[tauri::command]
pub async fn ado_tasks(
    client: State<'_, Arc<Client>>,
    ado: State<'_, AdoState>,
    worktree: String,
    scope: TaskScope,
) -> Result<TaskList, CmdError> {
    let ctx = ado.context_for(&client, &worktree).await?;
    tasks_or_cached(&ctx, &scope, &today_utc())
        .await
        .map_err(CmdError::from)
}

/// Remember the team this repo's views use (empty: the default team).
#[tauri::command]
pub async fn ado_set_team(
    client: State<'_, Arc<Client>>,
    ado: State<'_, AdoState>,
    worktree: String,
    team: String,
) -> Result<(), CmdError> {
    let ctx = ado.context_for(&client, &worktree).await?;
    set_team(&ctx, &team).map_err(CmdError::from)
}

/// The prompt Start would give an agent for task `id`, for the dialog.
#[tauri::command]
pub async fn ado_task_prompt(
    client: State<'_, Arc<Client>>,
    ado: State<'_, AdoState>,
    worktree: String,
    id: u32,
) -> Result<String, CmdError> {
    let ctx = ado.context_for(&client, &worktree).await?;
    prompt_for(&ctx, id).await.map_err(CmdError::from)
}

/// Start a task: branch, worktree, task link, and optionally assign,
/// activate and an agent. `worktree` is any checkout of the repo.
#[tauri::command]
pub async fn ado_start_task(
    client: State<'_, Arc<Client>>,
    ado: State<'_, AdoState>,
    worktree: String,
    req: StartTask,
) -> Result<StartedTask, CmdError> {
    // Refuse a bad name before any git or HTTP.
    validate_branch_name(&req.branch, &[])?;
    let row = client
        .snapshot()
        .worktrees
        .into_iter()
        .find(|w| w.id.0 == worktree || w.path == std::path::Path::new(&worktree))
        .ok_or("That worktree is gone.")?;
    let ctx = ado.context(&row.path).await?;
    let ws = DaemonWorkspace {
        client: client.inner().clone(),
        project: row.project_id,
        repo: row.path,
        remote_name: ctx.remote_name.clone(),
    };
    Ok(start_task(&ctx, &ws, req).await?)
}

// ---- HTML to markdown ----

/// ADO's rich text as plain markdown. Keeps `p`, `br`, `ul`/`ol`/`li`,
/// `b`/`strong`, `i`/`em`, `a` and `code`; block tags (`div`, headings,
/// rows) become line breaks; every other tag is dropped, its text kept.
pub fn html_to_md(html: &str) -> String {
    let mut md = Md::default();
    let mut rest = html;
    while let Some(lt) = rest.find('<') {
        md.text(&rest[..lt]);
        let after = &rest[lt + 1..];
        if let Some(body) = after.strip_prefix("!--") {
            rest = body.find("-->").map_or("", |e| &body[e + 3..]);
            continue;
        }
        let Some(gt) = after.find('>') else {
            md.text(&rest[lt..]);
            rest = "";
            break;
        };
        md.tag(&after[..gt]);
        rest = &after[gt + 1..];
    }
    md.text(rest);
    md.finish()
}

#[derive(Default)]
struct Md {
    out: String,
    /// Open lists: `None` for `ul`, `Some(next number)` for `ol`.
    lists: Vec<Option<u32>>,
    /// Open links: where the text starts and the href.
    links: Vec<(usize, Option<String>)>,
    /// Inside `script` or `style`.
    skip: u32,
}

impl Md {
    fn text(&mut self, raw: &str) {
        if self.skip > 0 || raw.is_empty() {
            return;
        }
        for c in decode_entities(raw).chars() {
            if c.is_whitespace() {
                if !(self.out.is_empty() || self.out.ends_with([' ', '\n'])) {
                    self.out.push(' ');
                }
            } else {
                self.out.push(c);
            }
        }
    }

    fn trim_spaces(&mut self) {
        let keep = self.out.trim_end_matches(' ').len();
        self.out.truncate(keep);
    }

    fn newline(&mut self) {
        self.trim_spaces();
        if !self.out.is_empty() && !self.out.ends_with('\n') {
            self.out.push('\n');
        }
    }

    fn blank_line(&mut self) {
        self.newline();
        if !self.out.is_empty() && !self.out.ends_with("\n\n") {
            self.out.push('\n');
        }
    }

    fn tag(&mut self, raw: &str) {
        let raw = raw.trim();
        let closing = raw.starts_with('/');
        let body = raw.trim_start_matches('/');
        let name: String = body
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric())
            .collect::<String>()
            .to_ascii_lowercase();
        match name.as_str() {
            "script" | "style" => {
                if closing {
                    self.skip = self.skip.saturating_sub(1);
                } else if !body.ends_with('/') {
                    self.skip += 1;
                }
            }
            _ if self.skip > 0 => {}
            "p" => self.blank_line(),
            "div" | "tr" | "h1" | "h2" | "h3" | "h4" | "h5" | "h6" | "blockquote" | "pre" => {
                self.newline()
            }
            "br" => {
                self.trim_spaces();
                self.out.push('\n');
            }
            "ul" | "ol" => {
                if closing {
                    self.lists.pop();
                    if self.lists.is_empty() {
                        self.blank_line();
                    } else {
                        self.newline();
                    }
                } else {
                    if self.lists.is_empty() {
                        self.blank_line();
                    } else {
                        self.newline();
                    }
                    self.lists.push((name == "ol").then_some(1));
                }
            }
            "li" => {
                if !closing {
                    self.newline();
                    let depth = self.lists.len().max(1);
                    self.out.push_str(&"  ".repeat(depth - 1));
                    match self.lists.last_mut() {
                        Some(Some(n)) => {
                            self.out.push_str(&format!("{n}. "));
                            *n += 1;
                        }
                        _ => self.out.push_str("- "),
                    }
                }
            }
            "b" | "strong" => self.out.push_str("**"),
            "i" | "em" => self.out.push('*'),
            "code" => self.out.push('`'),
            "a" => {
                if closing {
                    if let Some((start, href)) = self.links.pop() {
                        let text = self.out[start..].trim().to_string();
                        if let Some(href) = href.filter(|h| !h.is_empty() && *h != text) {
                            self.out.insert(start, '[');
                            self.out.push_str(&format!("]({href})"));
                        }
                    }
                } else {
                    let href = attr(body, "href").map(|h| decode_entities(&h));
                    self.links.push((self.out.len(), href));
                }
            }
            _ => {}
        }
    }

    fn finish(self) -> String {
        let mut out = String::new();
        let mut blank = 0;
        for line in self.out.lines() {
            let line = line.trim_end();
            if line.is_empty() {
                blank += 1;
                continue;
            }
            if !out.is_empty() {
                out.push_str(if blank > 0 { "\n\n" } else { "\n" });
            }
            blank = 0;
            out.push_str(line);
        }
        out
    }
}

/// An attribute's value from a tag body.
fn attr(tag: &str, name: &str) -> Option<String> {
    let lower = tag.to_ascii_lowercase();
    let mut from = 0;
    while let Some(i) = lower[from..].find(name) {
        let at = from + i;
        from = at + name.len();
        let before_ok = at == 0 || lower.as_bytes()[at - 1].is_ascii_whitespace();
        let rest = lower[from..].trim_start();
        if !before_ok || !rest.starts_with('=') {
            continue;
        }
        let vstart = tag.len() - rest.len() + 1;
        let v = tag[vstart..].trim_start();
        return Some(match v.chars().next() {
            Some(q @ ('"' | '\'')) => v[1..].split(q).next().unwrap_or("").to_string(),
            _ => v.split_whitespace().next().unwrap_or("").to_string(),
        });
    }
    None
}

fn decode_entities(s: &str) -> String {
    if !s.contains('&') {
        return s.to_string();
    }
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(amp) = rest.find('&') {
        out.push_str(&rest[..amp]);
        let tail = &rest[amp..];
        let decoded = tail.find(';').filter(|&e| e <= 10).and_then(|e| {
            let ent = &tail[1..e];
            let c = match ent {
                "amp" => Some('&'),
                "lt" => Some('<'),
                "gt" => Some('>'),
                "quot" => Some('"'),
                "apos" => Some('\''),
                "nbsp" => Some(' '),
                _ => ent
                    .strip_prefix("#x")
                    .or_else(|| ent.strip_prefix("#X"))
                    .and_then(|h| u32::from_str_radix(h, 16).ok())
                    .or_else(|| ent.strip_prefix('#').and_then(|d| d.parse().ok()))
                    .and_then(char::from_u32),
            }?;
            Some((c, e + 1))
        });
        match decoded {
            Some((c, len)) => {
                out.push(c);
                rest = &tail[len..];
            }
            None => {
                out.push('&');
                rest = &tail[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ado::testing;
    use httpmock::prelude::*;
    use httpmock::Mock;

    const PROJECT: &str = include_str!("../../tests/fixtures/ado/project.json");
    const TEAMS: &str = include_str!("../../tests/fixtures/ado/teams.json");
    const IT_CURRENT: &str = include_str!("../../tests/fixtures/ado/iterations_current.json");
    const IT_ALL: &str = include_str!("../../tests/fixtures/ado/iterations_all.json");
    const IT_ITEMS: &str = include_str!("../../tests/fixtures/ado/iteration_workitems.json");
    const IT_EMPTY: &str = include_str!("../../tests/fixtures/ado/iteration_workitems_empty.json");
    const BATCH_MAIN: &str = include_str!("../../tests/fixtures/ado/workitemsbatch.json");
    const BATCH_PARENT: &str = include_str!("../../tests/fixtures/ado/workitemsbatch_parent.json");
    const CONN: &str = include_str!("../../tests/fixtures/ado/connection_data.json");
    const TFV: &str = include_str!("../../tests/fixtures/ado/teamfieldvalues.json");
    const WIQL: &str = include_str!("../../tests/fixtures/ado/wiql.json");
    const TYPES: &str = include_str!("../../tests/fixtures/ado/workitemtypes.json");
    const ST_TASK: &str = include_str!("../../tests/fixtures/ado/states_task.json");
    const ST_STORY: &str = include_str!("../../tests/fixtures/ado/states_user_story.json");
    const ST_BUG: &str = include_str!("../../tests/fixtures/ado/states_bug.json");
    const ST_FEATURE: &str = include_str!("../../tests/fixtures/ado/states_feature.json");

    const TEAM_BASE: &str = "/Platform/Platform%20Team/_apis/work/teamsettings";
    const SPRINT_42: &str = "a589a806-bf11-4d4f-a031-c19813331553";

    fn get<'a>(s: &'a MockServer, path: &str, body: &str) -> Mock<'a> {
        let (path, body) = (path.to_string(), body.to_string());
        s.mock(move |w, t| {
            w.method(GET).path(path);
            t.status(200)
                .header("content-type", "application/json")
                .body(body);
        })
    }

    const REPO: &str = include_str!("../../tests/fixtures/ado/repository.json");

    /// Project (default team), repo metadata, states for every type, me.
    fn common(s: &MockServer) {
        common_except(s, &[]);
    }

    /// [`common`], with the named parts (`me`, or a type's URL name)
    /// answering 500.
    fn common_except(s: &MockServer, failing: &[&str]) {
        get(s, "/_apis/projects/Platform", PROJECT);
        get(s, "/Platform/_apis/git/repositories/api", REPO);
        let fail = |s: &MockServer, path: String| {
            s.mock(move |w, t| {
                w.path(path);
                t.status(500).body(r#"{"message":"boom"}"#);
            });
        };
        if failing.contains(&"me") {
            fail(s, "/_apis/connectionData".into());
        } else {
            get(s, "/_apis/connectionData", CONN);
        }
        for (kind, body) in [
            ("Task", ST_TASK),
            ("User%20Story", ST_STORY),
            ("Bug", ST_BUG),
            ("Feature", ST_FEATURE),
        ] {
            let path = format!("/Platform/_apis/wit/workitemtypes/{kind}/states");
            if failing.contains(&kind) {
                fail(s, path);
            } else {
                get(s, &path, body);
            }
        }
    }

    fn sprint_mocks(s: &MockServer, items: &str) {
        s.mock(|w, t| {
            w.method(GET)
                .path(format!("{TEAM_BASE}/iterations"))
                .query_param("$timeframe", "current");
            t.status(200).body(IT_CURRENT);
        });
        s.mock(|w, t| {
            w.method(GET)
                .path(format!("{TEAM_BASE}/iterations"))
                .matches(|req| {
                    !req.query_params
                        .iter()
                        .flatten()
                        .any(|(k, _)| k == "$timeframe")
                });
            t.status(200).body(IT_ALL);
        });
        get(
            s,
            &format!("{TEAM_BASE}/iterations/{SPRINT_42}/workitems"),
            items,
        );
    }

    fn batch_mocks(s: &MockServer) -> (Mock<'_>, Mock<'_>) {
        let main = s.mock(|w, t| {
            w.method(POST)
                .path("/Platform/_apis/wit/workitemsbatch")
                .body_contains("\"ids\":[100,")
                .body_contains("\"$expand\":\"relations\"");
            t.status(200).body(BATCH_MAIN);
        });
        let parent = s.mock(|w, t| {
            w.method(POST)
                .path("/Platform/_apis/wit/workitemsbatch")
                .body_contains("\"ids\":[300]");
            t.status(200).body(BATCH_PARENT);
        });
        (main, parent)
    }

    fn ids(g: &TaskGroup) -> Vec<u32> {
        g.items.iter().map(|i| i.id).collect()
    }

    #[tokio::test]
    async fn current_sprint_is_mine_hydrated_and_grouped_by_parent() {
        let s = MockServer::start();
        common(&s);
        sprint_mocks(&s, IT_ITEMS);
        let (main, parent) = batch_mocks(&s);
        let ctx = testing::context(&s.base_url(), testing::temp_store());
        let list = tasks(&ctx, &TaskScope::CurrentSprint, "2026-10-10")
            .await
            .unwrap();
        main.assert();
        parent.assert();
        assert_eq!(list.iteration.as_ref().unwrap().name, "Sprint 42");
        assert!(!list.stale);
        // Mine: 100 (story), 103 and 200. 101 is unassigned, 102 is Sam's.
        assert_eq!(list.groups.len(), 2);
        let tax = &list.groups[0];
        assert_eq!(tax.parent.as_ref().unwrap().id, 300);
        assert_eq!(ids(tax), vec![100, 103]);
        assert_eq!(list.groups[1].parent, None);
        assert_eq!(ids(&list.groups[1]), vec![200]);
        let story = &tax.items[0];
        assert_eq!(story.kind, "User Story");
        assert_eq!(story.state_category, "InProgress");
        assert_eq!(story.assigned_to.as_deref(), Some("Jay Richardson"));
        assert_eq!(story.board_column.as_deref(), Some("Doing"));
        assert_eq!(
            story.url,
            format!("{}/Platform/_workitems/edit/100", s.base_url())
        );
        let bug = &list.groups[1].items[0];
        assert_eq!(bug.description_md, "1. Reset password\n2. Sign in");
    }

    #[tokio::test]
    async fn sprint_all_lists_open_work_with_parents_as_headers() {
        let s = MockServer::start();
        common(&s);
        sprint_mocks(&s, IT_ITEMS);
        batch_mocks(&s);
        let ctx = testing::context(&s.base_url(), testing::temp_store());
        let list = tasks(&ctx, &TaskScope::SprintAll, "2026-10-10")
            .await
            .unwrap();
        // 102 is Done; 100 heads 101's group instead of being listed.
        let shape: Vec<(Option<u32>, Vec<u32>)> = list
            .groups
            .iter()
            .map(|g| (g.parent.as_ref().map(|p| p.id), ids(g)))
            .collect();
        assert_eq!(
            shape,
            vec![
                (Some(100), vec![101]),
                (Some(300), vec![103]),
                (None, vec![200]),
            ]
        );
        let fix = &list.groups[0].items[0];
        assert_eq!(fix.description_md, "a\n\n- b");
        assert_eq!(fix.remaining, Some(3.0));
        assert_eq!(fix.parent, Some(100));
        assert_eq!(fix.assigned_to, None);
        // Acceptance falls back to the parent's.
        assert_eq!(
            fix.acceptance_md,
            "- Totals match the ledger\n- Rounding is per invoice"
        );
        let docs = &list.groups[1].items[0];
        assert_eq!(docs.acceptance_md, "IRD sign-off");
        assert_eq!(
            docs.description_md,
            "See [the wiki](https://wiki.tnt.co.nz/tax) & **update** it."
        );
    }

    #[tokio::test]
    async fn batch_splits_at_200() {
        let s = MockServer::start();
        common(&s);
        let rels: Vec<Value> = (1..=250)
            .map(|i| json!({"rel": null, "source": null, "target": {"id": i, "url": ""}}))
            .collect();
        let items = json!({ "workItemRelations": rels }).to_string();
        sprint_mocks(&s, &items);
        let first = s.mock(|w, t| {
            w.method(POST)
                .path("/Platform/_apis/wit/workitemsbatch")
                .body_contains("\"ids\":[1,2,")
                .body_contains(",200]");
            t.status(200).body(r#"{"count":0,"value":[]}"#);
        });
        let second = s.mock(|w, t| {
            w.method(POST)
                .path("/Platform/_apis/wit/workitemsbatch")
                .body_contains("\"ids\":[201,")
                .body_contains(",250]");
            t.status(200).body(r#"{"count":0,"value":[]}"#);
        });
        let ctx = testing::context(&s.base_url(), testing::temp_store());
        let list = tasks(&ctx, &TaskScope::SprintAll, "2026-10-10")
            .await
            .unwrap();
        first.assert_hits(1);
        second.assert_hits(1);
        assert!(list.groups.is_empty());
    }

    #[tokio::test]
    async fn ended_sprint_is_stale_and_offers_the_next() {
        let s = MockServer::start();
        common(&s);
        sprint_mocks(&s, IT_EMPTY);
        let ctx = testing::context(&s.base_url(), testing::temp_store());
        let list = tasks(&ctx, &TaskScope::CurrentSprint, "2026-10-17")
            .await
            .unwrap();
        assert!(list.stale);
        assert_eq!(list.next.unwrap().name, "Sprint 43");
        // The finish day is still in the sprint.
        let list = tasks(&ctx, &TaskScope::CurrentSprint, "2026-10-14")
            .await
            .unwrap();
        assert!(!list.stale);
    }

    /// ADO's date can be ahead of UTC (NZ): on the finish day in UTC it
    /// may already name no current iteration.
    #[tokio::test]
    async fn no_current_iteration_means_the_last_one_ended() {
        let s = MockServer::start();
        common(&s);
        s.mock(|w, t| {
            w.method(GET)
                .path(format!("{TEAM_BASE}/iterations"))
                .query_param("$timeframe", "current");
            t.status(200).body(r#"{"count":0,"value":[]}"#);
        });
        s.mock(|w, t| {
            w.method(GET)
                .path(format!("{TEAM_BASE}/iterations"))
                .matches(|req| {
                    !req.query_params
                        .iter()
                        .flatten()
                        .any(|(k, _)| k == "$timeframe")
                });
            t.status(200).body(IT_ALL);
        });
        get(
            &s,
            &format!("{TEAM_BASE}/iterations/{SPRINT_42}/workitems"),
            IT_EMPTY,
        );
        let ctx = testing::context(&s.base_url(), testing::temp_store());
        let list = tasks(&ctx, &TaskScope::CurrentSprint, "2026-10-14")
            .await
            .unwrap();
        assert_eq!(list.iteration.unwrap().name, "Sprint 42");
        assert!(list.stale);
        assert_eq!(list.next.unwrap().name, "Sprint 43");
    }

    #[test]
    fn next_is_by_start_date_not_list_order() {
        let it = |id: &str, start: Option<&str>, finish: Option<&str>| Iteration {
            id: id.into(),
            name: id.into(),
            path: String::new(),
            start: start.map(str::to_string),
            finish: finish.map(str::to_string),
            timeframe: None,
        };
        let cur = it(
            "42",
            Some("2026-10-01T00:00:00Z"),
            Some("2026-10-14T00:00:00Z"),
        );
        let all = vec![
            it("44", Some("2026-10-29T00:00:00Z"), None),
            it("41", Some("2026-09-17T00:00:00Z"), None),
            cur.clone(),
            it("43", Some("2026-10-15T00:00:00Z"), None),
            it("backlog", None, None),
        ];
        let (stale, next) = stale_and_next(&cur, &all, "2026-10-15");
        assert!(stale);
        assert_eq!(next.unwrap().id, "43");
        let undated = it("x", None, None);
        assert_eq!(stale_and_next(&undated, &all, "2026-10-15"), (false, None));
    }

    #[tokio::test]
    async fn empty_iteration_gives_empty_groups() {
        let s = MockServer::start();
        common(&s);
        sprint_mocks(&s, IT_EMPTY);
        let batch = s.mock(|w, t| {
            w.method(POST).path("/Platform/_apis/wit/workitemsbatch");
            t.status(500);
        });
        let ctx = testing::context(&s.base_url(), testing::temp_store());
        let list = tasks(&ctx, &TaskScope::CurrentSprint, "2026-10-10")
            .await
            .unwrap();
        assert!(list.groups.is_empty());
        assert_eq!(list.iteration.unwrap().name, "Sprint 42");
        assert_eq!(batch.hits(), 0);
    }

    #[tokio::test]
    async fn my_open_queries_wiql_without_closed_states() {
        let s = MockServer::start();
        common(&s);
        get(&s, &format!("{TEAM_BASE}/teamfieldvalues"), TFV);
        get(&s, "/Platform/_apis/wit/workitemtypes", TYPES);
        let wiql = s.mock(|w, t| {
            w.method(POST)
                .path("/Platform/_apis/wit/wiql")
                .body_contains("[System.AssignedTo] = @Me")
                .body_contains("[System.AreaPath] UNDER 'Platform\\\\Payments'")
                .body_contains("[System.AreaPath] = 'Platform\\\\O''Brien Squad'")
                .body_contains(
                    "([System.WorkItemType] = 'Task' AND [System.State] IN ('Done', 'Removed'))",
                )
                .body_contains(
                    "([System.WorkItemType] = 'User Story' AND [System.State] IN ('Closed', 'Removed'))",
                );
            t.status(200).body(WIQL);
        });
        let batch = s.mock(|w, t| {
            w.method(POST)
                .path("/Platform/_apis/wit/workitemsbatch")
                .body_contains("\"ids\":[101,200]");
            t.status(200).body(BATCH_MAIN);
        });
        let parent = s.mock(|w, t| {
            w.method(POST)
                .path("/Platform/_apis/wit/workitemsbatch")
                .body_contains("\"ids\":[100]");
            t.status(200).body(BATCH_MAIN);
        });
        let ctx = testing::context(&s.base_url(), testing::temp_store());
        let list = tasks(&ctx, &TaskScope::MyOpen, "2026-10-10").await.unwrap();
        wiql.assert();
        batch.assert();
        parent.assert();
        assert!(list.iteration.is_none());
        let shape: Vec<(Option<u32>, Vec<u32>)> = list
            .groups
            .iter()
            .map(|g| (g.parent.as_ref().map(|p| p.id), ids(g)))
            .collect();
        assert_eq!(shape, vec![(Some(100), vec![101]), (None, vec![200])]);
    }

    #[test]
    fn wiql_without_areas_or_closed_states() {
        assert_eq!(
            my_open_wiql("System.AreaPath", &[], &[("Epic".into(), vec![])]),
            "SELECT [System.Id] FROM WorkItems WHERE [System.TeamProject] = @project AND [System.AssignedTo] = @Me ORDER BY [System.ChangedDate] DESC"
        );
    }

    #[tokio::test]
    async fn teams_and_iterations() {
        let s = MockServer::start();
        common(&s);
        get(&s, "/_apis/projects/Platform/teams", TEAMS);
        sprint_mocks(&s, IT_EMPTY);
        let store = testing::temp_store();
        let ctx = testing::context(&s.base_url(), store.clone());
        assert_eq!(teams(&ctx).await.unwrap(), vec!["Billing", "Platform Team"]);
        let its = iterations(&ctx).await.unwrap();
        let names: Vec<&str> = its.iter().map(|i| i.name.as_str()).collect();
        assert_eq!(
            names,
            vec![
                "Sprint 41",
                "Sprint 42",
                "Sprint 43",
                "Sprint 44",
                "Backlog"
            ]
        );
        assert_eq!(its[1].finish.as_deref(), Some("2026-10-14T00:00:00Z"));
        assert_eq!(its[1].timeframe.as_deref(), Some("current"));
        assert_eq!(its[4].start, None);
    }

    #[tokio::test]
    async fn a_remembered_team_is_used() {
        let s = MockServer::start();
        let billing = s.mock(|w, t| {
            w.method(GET)
                .path("/Platform/Billing/_apis/work/teamsettings/iterations");
            t.status(200).body(r#"{"count":0,"value":[]}"#);
        });
        let store = testing::temp_store();
        let ctx = testing::context(&s.base_url(), store.clone());
        store.with_repo(&ctx.key, |r| r.team = Some("Billing".into()));
        assert!(iterations(&ctx).await.unwrap().is_empty());
        billing.assert();
    }

    #[test]
    fn html_to_markdown() {
        assert_eq!(html_to_md("<p>a</p><ul><li>b</li></ul>"), "a\n\n- b");
        assert_eq!(html_to_md(""), "");
        assert_eq!(
            html_to_md("<div>one</div><div>two<br>three</div>"),
            "one\ntwo\nthree"
        );
        assert_eq!(
            html_to_md("<ol><li>x<ul><li>y</li></ul></li><li>z</li></ol>"),
            "1. x\n  - y\n2. z"
        );
        assert_eq!(
            html_to_md("<p><strong>Bold</strong> <em>it</em> <code>x()</code></p>"),
            "**Bold** *it* `x()`"
        );
        assert_eq!(
            html_to_md(
                "<span style=\"color:red\">a &lt;b&gt; &#39;c&#x27;&nbsp;d</span><img src=x>"
            ),
            "a <b> 'c' d"
        );
        assert_eq!(
            html_to_md("<a href=\"https://x.nz\">https://x.nz</a>"),
            "https://x.nz"
        );
        assert_eq!(
            html_to_md("<style>p{}</style><!-- note --><p>kept</p><script>bad()</script>"),
            "kept"
        );
    }

    // ---- start a task ----

    #[derive(Default)]
    struct FakeWs {
        taken: Vec<String>,
        calls: std::sync::Mutex<Vec<String>>,
        agents: std::sync::Mutex<Vec<NewAgent>>,
        fail_agent: bool,
    }

    #[async_trait::async_trait]
    impl Workspace for FakeWs {
        async fn taken_branches(&self) -> Result<Vec<String>, String> {
            self.calls.lock().unwrap().push("branches".into());
            Ok(self.taken.clone())
        }
        async fn create_worktree(
            &self,
            branch: &str,
            base: Option<&str>,
        ) -> Result<String, String> {
            self.calls
                .lock()
                .unwrap()
                .push(format!("worktree {branch} {}", base.unwrap_or("-")));
            Ok("wt-9".into())
        }
        async fn create_agent(&self, spec: NewAgent) -> Result<String, String> {
            self.calls.lock().unwrap().push("agent".into());
            self.agents.lock().unwrap().push(spec);
            if self.fail_agent {
                Err("no such harness".into())
            } else {
                Ok("ag-1".into())
            }
        }
    }

    fn start(id: u32, branch: &str) -> StartTask {
        StartTask {
            id,
            branch: branch.into(),
            base: None,
            assign_to_me: true,
            activate: true,
            spawn: None,
        }
    }

    fn spawn_spec() -> NewAgent {
        NewAgent {
            worktree: String::new(),
            kind: "claude".into(),
            profile: None,
            prompt: None,
        }
    }

    fn item(id: u32, title: &str, desc: &str, acc: &str) -> WorkItem {
        WorkItem {
            id,
            kind: "Task".into(),
            title: title.into(),
            state: "To Do".into(),
            state_category: "Proposed".into(),
            assigned_to: None,
            remaining: None,
            parent: None,
            board_column: None,
            description_md: desc.into(),
            acceptance_md: acc.into(),
            url: format!("https://dev.azure.com/tnt/Platform/_workitems/edit/{id}"),
        }
    }

    #[test]
    fn task_prompt_has_title_description_and_acceptance() {
        let mut task = item(101, "Fix GST rounding", "a\n\n- b", "");
        task.parent = Some(100);
        let mut story = item(100, "GST rounding on invoices", "", "- Totals match");
        story.kind = "User Story".into();
        let p = task_prompt(&task, Some(&story));
        assert_eq!(
            p,
            "# Task 101: Fix GST rounding\n\n\
             https://dev.azure.com/tnt/Platform/_workitems/edit/101\n\n\
             Parent: User Story 100: GST rounding on invoices\n\n\
             ## Description\n\na\n\n- b\n\n\
             ## Acceptance criteria\n\n- Totals match\n"
        );
        let bare = task_prompt(&item(7, "Tidy", "", ""), None);
        assert!(bare.contains("## Description\n\nNone given."));
        assert!(bare.contains("## Acceptance criteria\n\nNone given."));
        assert!(!bare.contains("Parent:"));
        assert!(!bare.contains('\u{2014}'));
    }

    #[tokio::test]
    async fn an_invalid_branch_is_refused_before_any_git_or_http() {
        let s = MockServer::start();
        let any = s.mock(|w, t| {
            w.path_contains("/");
            t.status(500);
        });
        let ctx = testing::context(&s.base_url(), testing::temp_store());
        let ws = FakeWs::default();
        for bad in ["jari/fix gst", "jari/..x", "-x", "jari/"] {
            let e = start_task(&ctx, &ws, start(101, bad)).await.unwrap_err();
            assert!(e.message.contains("not a valid branch name"), "{e}");
        }
        assert!(ws.calls.lock().unwrap().is_empty());
        assert_eq!(any.hits(), 0);
    }

    #[tokio::test]
    async fn a_taken_branch_is_refused_before_the_worktree() {
        let s = MockServer::start();
        let any = s.mock(|w, t| {
            w.path_contains("/");
            t.status(500);
        });
        let ctx = testing::context(&s.base_url(), testing::temp_store());
        let ws = FakeWs {
            taken: vec!["jari/fix-gst".into()],
            ..Default::default()
        };
        let e = start_task(&ctx, &ws, start(101, "jari/fix-gst"))
            .await
            .unwrap_err();
        assert!(e.message.contains("already exists"), "{e}");
        assert_eq!(*ws.calls.lock().unwrap(), vec!["branches"]);
        assert_eq!(any.hits(), 0);
    }

    /// Item 101 (unassigned, To Do) under story 100.
    fn start_mocks(s: &MockServer, patch_status: u16) -> (Mock<'_>, Mock<'_>) {
        common(s);
        s.mock(|w, t| {
            w.method(POST).path("/Platform/_apis/wit/workitemsbatch");
            t.status(200).body(BATCH_MAIN);
        });
        let assign = s.mock(|w, t| {
            w.method(httpmock::Method::PATCH)
                .path("/Platform/_apis/wit/workitems/101")
                .header("content-type", "application/json-patch+json")
                .json_body(json!([{
                    "op": "add",
                    "path": "/fields/System.AssignedTo",
                    "value": "jay@tnt.co.nz"
                }]));
            t.status(patch_status).body(r#"{"id":101}"#);
        });
        let activate = s.mock(|w, t| {
            w.method(httpmock::Method::PATCH)
                .path("/Platform/_apis/wit/workitems/101")
                .header("content-type", "application/json-patch+json")
                .json_body(json!([{
                    "op": "add",
                    "path": "/fields/System.State",
                    "value": "In Progress"
                }]));
            t.status(patch_status).body(r#"{"id":101}"#);
        });
        (assign, activate)
    }

    #[tokio::test]
    async fn start_patches_assign_and_activate_records_the_link_and_spawns() {
        let s = MockServer::start();
        let (assign, activate) = start_mocks(&s, 200);
        let store = testing::temp_store();
        let ctx = testing::context(&s.base_url(), store.clone());
        let ws = FakeWs::default();
        let mut req = start(101, "jari/fix-gst-rounding");
        req.base = Some("main".into());
        req.spawn = Some(spawn_spec());
        let out = start_task(&ctx, &ws, req).await.unwrap();
        assign.assert();
        activate.assert();
        assert_eq!(out.worktree_id, "wt-9");
        assert_eq!(out.agent_id.as_deref(), Some("ag-1"));
        assert!(out.problems.is_empty(), "{:?}", out.problems);
        assert_eq!(
            *ws.calls.lock().unwrap(),
            vec!["branches", "worktree jari/fix-gst-rounding main", "agent"]
        );
        // The store records the branch-to-task link.
        assert_eq!(
            store.read_repo(&ctx.key, |r| r
                .branch_tasks
                .get("jari/fix-gst-rounding")
                .copied()),
            Some(101)
        );
        let agents = ws.agents.lock().unwrap();
        assert_eq!(agents[0].worktree, "wt-9");
        let prompt = agents[0].prompt.as_deref().unwrap();
        assert!(prompt.starts_with("# Task 101: Fix GST rounding"));
        assert!(prompt.contains("Parent: User Story 100: GST rounding on invoices"));
        assert!(prompt.contains("## Acceptance criteria\n\n- Totals match the ledger"));
    }

    #[tokio::test]
    async fn a_failed_patch_keeps_the_branch_and_is_reported_per_step() {
        let s = MockServer::start();
        let (assign, activate) = start_mocks(&s, 500);
        let store = testing::temp_store();
        let ctx = testing::context(&s.base_url(), store.clone());
        let ws = FakeWs {
            fail_agent: true,
            ..Default::default()
        };
        let mut req = start(101, "jari/fix-gst");
        let mut spec = spawn_spec();
        spec.prompt = Some("Use my words".into());
        req.spawn = Some(spec);
        let out = start_task(&ctx, &ws, req).await.unwrap();
        assign.assert();
        activate.assert();
        assert_eq!(out.worktree_id, "wt-9");
        assert_eq!(out.agent_id, None);
        let steps: Vec<&str> = out.problems.iter().map(|p| p.step.as_str()).collect();
        assert_eq!(steps, vec!["assign", "activate", "spawn"]);
        assert_eq!(
            store.read_repo(&ctx.key, |r| r.branch_tasks.get("jari/fix-gst").copied()),
            Some(101)
        );
        assert_eq!(
            ws.agents.lock().unwrap()[0].prompt.as_deref(),
            Some("Use my words")
        );
    }

    #[tokio::test]
    async fn someone_elses_task_is_not_reassigned_and_active_is_not_moved() {
        let s = MockServer::start();
        common(&s);
        s.mock(|w, t| {
            w.method(POST).path("/Platform/_apis/wit/workitemsbatch");
            t.status(200).body(BATCH_MAIN);
        });
        let patch = s.mock(|w, t| {
            w.method(httpmock::Method::PATCH);
            t.status(200).body("{}");
        });
        let ctx = testing::context(&s.base_url(), testing::temp_store());
        let ws = FakeWs::default();
        // 102 is Sam's, and Done.
        let out = start_task(&ctx, &ws, start(102, "jari/tests"))
            .await
            .unwrap();
        let steps: Vec<&str> = out.problems.iter().map(|p| p.step.as_str()).collect();
        assert_eq!(steps, vec!["assign", "activate"]);
        assert!(out.problems[0].message.contains("Sam Lee"));
        assert!(out.problems[1].message.contains("already Done"));
        // 100 is mine and already Active.
        let out = start_task(&ctx, &ws, start(100, "jari/gst")).await.unwrap();
        assert!(out.problems.is_empty(), "{:?}", out.problems);
        assert_eq!(patch.hits(), 0);
    }

    #[test]
    fn start_task_reads_camel_case_and_a_spawn_without_worktree() {
        let req: StartTask = serde_json::from_value(json!({
            "id": 101,
            "branch": "jari/fix",
            "base": null,
            "assignToMe": true,
            "activate": false,
            "spawn": {"kind": "claude", "profile": null, "prompt": null}
        }))
        .unwrap();
        assert!(req.assign_to_me && !req.activate);
        assert_eq!(req.spawn.unwrap().worktree, "");
    }

    // ---- fix round 1 ----

    #[tokio::test]
    async fn start_cuts_from_the_ado_default_branch_when_no_base_is_given() {
        let s = MockServer::start();
        start_mocks(&s, 200);
        let ctx = testing::context(&s.base_url(), testing::temp_store());
        let ws = FakeWs::default();
        start_task(&ctx, &ws, start(101, "jari/fix")).await.unwrap();
        assert_eq!(ws.calls.lock().unwrap()[1], "worktree jari/fix origin/main");
    }

    #[tokio::test]
    async fn an_unreadable_default_branch_stops_before_the_worktree() {
        let s = MockServer::start();
        s.mock(|w, t| {
            w.path("/Platform/_apis/git/repositories/api");
            t.status(500);
        });
        let ctx = testing::context(&s.base_url(), testing::temp_store());
        let ws = FakeWs::default();
        let e = start_task(&ctx, &ws, start(101, "jari/fix"))
            .await
            .unwrap_err();
        assert!(e.message.contains("default branch"), "{e}");
        assert_eq!(*ws.calls.lock().unwrap(), vec!["branches"]);
    }

    #[test]
    fn remote_branches_on_the_repo_remote_are_taken() {
        let refs = "refs/heads/main\nrefs/heads/jari/a\nrefs/remotes/origin/HEAD\n\
                    refs/remotes/origin/ga/b\nrefs/remotes/fork/jari/c\n";
        assert_eq!(
            taken_from_refs(refs, "origin"),
            vec!["main", "jari/a", "ga/b"]
        );
    }

    #[tokio::test]
    async fn a_failed_identity_check_is_reported_as_such() {
        let s = MockServer::start();
        common_except(&s, &["me"]);
        s.mock(|w, t| {
            w.method(POST).path("/Platform/_apis/wit/workitemsbatch");
            t.status(200).body(BATCH_MAIN);
        });
        let ctx = testing::context(&s.base_url(), testing::temp_store());
        let ws = FakeWs::default();
        let mut req = start(102, "jari/tests");
        req.activate = false;
        let out = start_task(&ctx, &ws, req).await.unwrap();
        assert_eq!(out.problems.len(), 1);
        assert!(
            out.problems[0].message.starts_with("Could not check who"),
            "{:?}",
            out.problems
        );
        assert!(!out.problems[0].message.contains("Sam Lee"));
    }

    #[tokio::test]
    async fn task_prompt_command_hydrates_item_and_parent() {
        let s = MockServer::start();
        common(&s);
        s.mock(|w, t| {
            w.method(POST).path("/Platform/_apis/wit/workitemsbatch");
            t.status(200).body(BATCH_MAIN);
        });
        let ctx = testing::context(&s.base_url(), testing::temp_store());
        let p = prompt_for(&ctx, 101).await.unwrap();
        assert!(p.starts_with("# Task 101: Fix GST rounding"));
        assert!(p.contains("Parent: User Story 100: GST rounding on invoices"));
        assert!(p.contains("## Description\n\na\n\n- b"));
        assert_eq!(prompt_for(&ctx, 999).await, Err(AdoError::NotFound));
    }

    #[tokio::test]
    async fn set_team_persists_and_empty_clears() {
        let store = testing::temp_store();
        let ctx = testing::context("http://127.0.0.1:1", store.clone());
        set_team(&ctx, " Billing ").unwrap();
        assert_eq!(
            store.read_repo(&ctx.key, |r| r.team.clone()).as_deref(),
            Some("Billing")
        );
        // Read back from the file, not just memory.
        let again = crate::ado::store::AdoStore::open(store.path());
        assert_eq!(
            again.read_repo(&ctx.key, |r| r.team.clone()).as_deref(),
            Some("Billing")
        );
        assert_eq!(ctx.team().await.unwrap(), "Billing");
        set_team(&ctx, "").unwrap();
        assert_eq!(store.read_repo(&ctx.key, |r| r.team.clone()), None);
    }

    #[tokio::test]
    async fn no_default_team_asks_for_one() {
        let s = MockServer::start();
        get(
            &s,
            "/_apis/projects/Platform",
            r#"{"id":"p","name":"Platform"}"#,
        );
        let ctx = testing::context(&s.base_url(), testing::temp_store());
        assert_eq!(
            ctx.team().await,
            Err(AdoError::Local("Choose a team".into()))
        );
    }

    #[tokio::test]
    async fn a_saved_list_comes_back_offline_with_its_age() {
        let s = MockServer::start();
        common(&s);
        sprint_mocks(&s, IT_ITEMS);
        batch_mocks(&s);
        let store = testing::temp_store();
        let online = testing::context(&s.base_url(), store.clone());
        let fresh = tasks_or_cached(&online, &TaskScope::SprintAll, "2026-10-10")
            .await
            .unwrap();
        assert_eq!(fresh.cached_at, None);
        assert_eq!(fresh.groups.len(), 3);

        // Unreachable: the saved list, stamped.
        let offline = testing::context("http://127.0.0.1:1", store.clone());
        let cached = tasks_or_cached(&offline, &TaskScope::SprintAll, "2026-10-10")
            .await
            .unwrap();
        assert!(cached.cached_at.is_some());
        assert_eq!(cached.cached_because, Some(ErrorKind::Offline));
        assert_eq!(cached.groups, fresh.groups);
        // Another scope has nothing saved: the error stands.
        let e = tasks_or_cached(&offline, &TaskScope::MyOpen, "2026-10-10")
            .await
            .unwrap_err();
        assert!(matches!(e, AdoError::Network(_)), "{e:?}");

        // Signed out (401): the saved list too.
        let denied = MockServer::start();
        denied.mock(|w, t| {
            w.path_contains("/");
            t.status(401);
        });
        let signed_out = testing::context(&denied.base_url(), store.clone());
        let cached = tasks_or_cached(&signed_out, &TaskScope::SprintAll, "2026-10-10")
            .await
            .unwrap();
        assert!(cached.cached_at.is_some());
        // ...saying it is shown because of the sign-in, not as offline.
        assert_eq!(cached.cached_because, Some(ErrorKind::Auth));
        assert!(cached.cached_reason.is_some_and(|r| r.contains("sign")));
        // A 403 is not offline: it stays an error.
        let forbidden = MockServer::start();
        forbidden.mock(|w, t| {
            w.path_contains("/");
            t.status(403);
        });
        let ctx = testing::context(&forbidden.base_url(), store);
        assert!(tasks_or_cached(&ctx, &TaskScope::SprintAll, "2026-10-10")
            .await
            .is_err());
    }

    #[tokio::test]
    async fn one_unreadable_type_does_not_fail_my_open() {
        let s = MockServer::start();
        common_except(&s, &["Bug"]);
        get(&s, &format!("{TEAM_BASE}/teamfieldvalues"), TFV);
        get(&s, "/Platform/_apis/wit/workitemtypes", TYPES);
        let wiql = s.mock(|w, t| {
            w.method(POST)
                .path("/Platform/_apis/wit/wiql")
                .body_contains("([System.WorkItemType] = 'Task'")
                .matches(|req| {
                    let body = String::from_utf8_lossy(req.body.as_deref().unwrap_or_default());
                    !body.contains("[System.WorkItemType] = 'Bug'")
                });
            t.status(200).body(WIQL);
        });
        s.mock(|w, t| {
            w.method(POST).path("/Platform/_apis/wit/workitemsbatch");
            t.status(200).body(BATCH_MAIN);
        });
        let ctx = testing::context(&s.base_url(), testing::temp_store());
        let list = tasks(&ctx, &TaskScope::MyOpen, "2026-10-10").await.unwrap();
        wiql.assert();
        let all: Vec<u32> = list
            .groups
            .iter()
            .flat_map(|g| g.items.iter().map(|i| i.id))
            .collect();
        assert_eq!(all, vec![101, 200]);
    }

    #[test]
    fn dates() {
        assert_eq!(civil_from_days(0), (1970, 1, 1));
        assert_eq!(civil_from_days(20_740), (2026, 10, 14));
        assert_eq!(today_utc().len(), 10);
    }
}
