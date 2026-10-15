//! The user's order for projects, worktrees and sessions: the
//! `ClientRequest::Reorder` a drag or a move up / move down sends. The
//! daemon writes it and answers with an upsert per row that moved, which
//! the tree folds in like any other; nothing is reordered here first.
//!
//! The rule every client lays rows out by is `nebula_core::ReorderScope`'s,
//! written once in `nebula_core::order` and read by the desktop too:
//! a scope the user has ordered lays out by `sort_order`; one never
//! ordered keeps each view's own order. A scope has a user order once some
//! row's `sort_order` is above 0 ([`has_user_order`]): the daemon leaves
//! every worktree and session at 0 until a `Reorder` writes 0..n, and
//! numbers projects 0..n as they are added, so with two projects or more
//! the PROJECT TABS are always in the user's order.
//! - worktrees: the root first, then the user's order, or else the most
//!   recently worked in (the BANDS);
//! - sessions: the user's order, or else the grid's recency;
//! - the KANBAN: newest first across checkouts, and the user's order among
//!   the cards of a checkout that has one ([`refill`]).

use crate::app::{App, PendingIntent};
use crate::launcher::{Lane, LauncherRow};
use nebula_core::{AgentId, ClientRequest, ProjectId, ReorderScope, WorktreeId, REORDER_PROTOCOL};
use std::collections::HashSet;

pub use nebula_core::order::has_user_order;

/// Whether the projects are in a user order (the PROJECT TABS, the
/// Projects column).
pub fn projects_ordered(app: &App) -> bool {
    has_user_order(app.tree.projects.iter().map(|p| p.sort_order))
}

/// Whether `project`'s worktrees are.
pub fn worktrees_ordered(app: &App, project: &ProjectId) -> bool {
    has_user_order(
        app.tree
            .worktrees
            .iter()
            .filter(|w| &w.project_id == project)
            .map(|w| w.sort_order),
    )
}

/// The checkouts whose sessions carry a user order, archived ones counted.
pub fn ordered_worktrees(app: &App) -> HashSet<WorktreeId> {
    nebula_core::order::ordered_worktrees(&app.tree.agents)
}

/// Which side of a row a drop lands on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Place {
    Before,
    After,
}

impl Place {
    /// The side of a cell `offset` into a row `len` long: the first half
    /// is before.
    pub fn of(offset: u16, len: u16) -> Place {
        if u32::from(offset) * 2 < u32::from(len) {
            Place::Before
        } else {
            Place::After
        }
    }
}

/// `ids` with `id` moved to sit just before or after `target`; None when
/// that changes nothing (or either is missing).
pub fn place_id<T: PartialEq + Clone>(
    ids: &[T],
    id: &T,
    target: &T,
    place: Place,
) -> Option<Vec<T>> {
    if id == target || !ids.contains(id) || !ids.contains(target) {
        return None;
    }
    let mut rest: Vec<T> = ids.iter().filter(|x| *x != id).cloned().collect();
    let at = rest.iter().position(|x| x == target)? + usize::from(place == Place::After);
    rest.insert(at, id.clone());
    (rest != ids).then_some(rest)
}

/// One step up (`dir` < 0) or down past the next of `peers` (the rows shown
/// beside it, in display order), as a new order of the whole scope `ids`.
/// None at either end.
pub fn step_id<T: PartialEq + Clone>(ids: &[T], peers: &[T], id: &T, dir: i64) -> Option<Vec<T>> {
    let i = peers.iter().position(|x| x == id)? as i64;
    let next = peers.get(usize::try_from(i + dir.signum()).ok()?)?;
    place_id(
        ids,
        id,
        next,
        if dir < 0 { Place::Before } else { Place::After },
    )
}

/// A worktree scope's new order keeps the root first: the daemon refuses
/// one that puts it after a linked worktree.
pub fn root_first(app: &App, ids: Vec<WorktreeId>) -> Vec<WorktreeId> {
    let root = |id: &WorktreeId| app.tree.worktrees.iter().any(|w| &w.id == id && w.is_main);
    let (mut out, rest): (Vec<_>, Vec<_>) = ids.into_iter().partition(|id| root(id));
    out.extend(rest);
    out
}

/// The projects in the order the tabs lay them out once ordered.
pub fn project_ids(app: &App) -> Vec<ProjectId> {
    let mut all: Vec<&nebula_core::Project> = app.tree.projects.iter().collect();
    all.sort_by_key(|p| p.sort_order);
    all.into_iter().map(|p| p.id.clone()).collect()
}

/// `project`'s worktrees as the BANDS lay them out (the root first), empty
/// ones too: the scope a band drag or step reorders.
pub fn worktree_ids(app: &App, project: &ProjectId) -> Vec<WorktreeId> {
    app.worktrees_in_order(project)
        .into_iter()
        .map(|w| w.id.clone())
        .collect()
}

/// A checkout's sessions in the order a drop or a step there starts from:
/// the user's order, or else the order `shown` lists them in (the band's
/// cards, a lane's), so the first move keeps the rest put. Sessions it
/// does not show follow in the daemon's order, which it would append.
pub fn agent_ids(app: &App, worktree: &WorktreeId, shown: &[AgentId]) -> Vec<AgentId> {
    let mut here: Vec<&nebula_core::Agent> = app
        .tree
        .agents
        .iter()
        .filter(|a| &a.worktree_id == worktree)
        .collect();
    if has_user_order(here.iter().map(|a| a.sort_order)) {
        here.sort_by_key(|a| a.sort_order);
        return here.into_iter().map(|a| a.id.clone()).collect();
    }
    let mut out: Vec<AgentId> = shown
        .iter()
        .filter(|id| here.iter().any(|a| &a.id == *id))
        .cloned()
        .collect();
    out.extend(
        here.into_iter()
            .map(|a| a.id.clone())
            .filter(|id| !shown.contains(id)),
    );
    out
}

/// A lane's cards: `cards` in the board's own order (newest first across
/// checkouts), and the user's order among the cards of a checkout that has
/// one. Each such checkout's cards take back the slots they hold, in
/// `sort_order`, so the checkouts stay interleaved by age.
pub fn refill(cards: Vec<LauncherRow>, ordered: &HashSet<WorktreeId>) -> Vec<LauncherRow> {
    nebula_core::order::refill(cards, ordered, |r| {
        (&r.agent.worktree_id, r.agent.sort_order)
    })
}

/// Where a board card is dropped: a lane, or the ARCHIVED zone the board
/// shows while a card is held.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DropLane {
    Lane(Lane),
    Archived,
}

/// What dropping a board card on a lane does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LaneDrop {
    /// The same lane: beside a card of its own checkout, a reorder.
    Reorder,
    /// Unread to Done: the session is marked seen.
    Seen,
    Archive,
    /// A status the agent (or the pull request) owns: refused, with why.
    Refuse(&'static str),
}

/// Only moves the user can cause: Unread to Done marks the session seen,
/// the Archived zone archives it, the same lane reorders. A status the
/// agent (or the pull request) owns is refused with the reason. The
/// desktop's `laneDrop`, word for word.
pub fn lane_drop(from: Lane, to: DropLane) -> LaneDrop {
    let to = match to {
        DropLane::Archived => return LaneDrop::Archive,
        DropLane::Lane(l) => l,
    };
    if from == to {
        return LaneDrop::Reorder;
    }
    if to == Lane::Done {
        return match from {
            Lane::Unseen => LaneDrop::Seen,
            Lane::NeedsInput => LaneDrop::Refuse(
                "Needs you is the agent\u{2019}s: the card leaves it when you answer the session.",
            ),
            Lane::Running => LaneDrop::Refuse(
                "Running is the agent\u{2019}s: the card lands in Done when the turn ends.",
            ),
            Lane::PrOpen => LaneDrop::Refuse(
                "Its pull request is open: the card leaves PR open when the PR merges or closes.",
            ),
            Lane::Done => LaneDrop::Reorder,
        };
    }
    LaneDrop::Refuse(match to {
        Lane::NeedsInput => "Only the agent puts a card in Needs you, by asking.",
        Lane::Running => {
            "Only the agent puts a card in Running, by starting a turn: send it a follow-up (Space)."
        }
        Lane::Unseen => {
            "Unread comes from a turn finishing while you were away; a card cannot be marked unread."
        }
        _ => "PR open follows the worktree\u{2019}s pull request: open one and the card moves.",
    })
}

/// What a move says to a daemon too old to take one.
#[cfg(unix)]
pub const DAEMON_TOO_OLD: &str =
    "the running daemon predates reordering: run `kete reload` (every session keeps running), then reopen kete";
/// Windows daemons can't restart in place: `kete reload` refuses there.
#[cfg(windows)]
pub const DAEMON_TOO_OLD: &str =
    "the running daemon predates reordering: run `kete kill` (stops all sessions), then reopen kete";

/// Can the daemon take `Reorder`? Below [`REORDER_PROTOCOL`] it cannot
/// decode the request and would drop the connection on it. None (no
/// handshake yet: tests) counts as yes, as `daemon_provisions` does.
pub fn daemon_reorders(app: &App) -> bool {
    app.orch
        .daemon_protocol
        .is_none_or(|v| v >= REORDER_PROTOCOL)
}

/// Queue the new order of `scope`, first to last (the whole scope, or
/// just its front: the daemon appends whatever `ids` leaves out). A daemon
/// too old for it gets nothing and the flash says what to run; false then.
pub fn reorder(
    app: &mut App,
    scope: ReorderScope,
    ids: Vec<String>,
    out: &mut Vec<ClientRequest>,
) -> bool {
    if !daemon_reorders(app) {
        app.chrome.flash = Some(DAEMON_TOO_OLD.into());
        app.chrome.dirty = true;
        return false;
    }
    let req_id = app.alloc_req_id(PendingIntent::None);
    out.push(ClientRequest::Reorder { req_id, scope, ids });
    true
}

/// A row the user can put elsewhere in its order: a PROJECT TAB, a BAND's
/// checkout, a session card.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OrderTarget {
    Project(ProjectId),
    Worktree(WorktreeId),
    Agent(AgentId),
}

/// What a step or a drop of the root checkout says: the daemon refuses an
/// order that puts it after a linked worktree.
pub const ROOT_STAYS_FIRST: &str = "the root checkout stays first";

/// What an Unread card let go on Done says.
pub const MARKED_SEEN: &str = "marked read: the card is done";

/// The open PROJECT TABS, in the order the header draws them.
fn tab_ids(app: &App) -> Vec<ProjectId> {
    crate::launcher::project_tabs(app)
        .into_iter()
        .map(|t| t.id)
        .collect()
}

/// The projects a tab drop or step starts from: the user's order, or —
/// from a daemon that left them all at 0 — the tabs as the header shows
/// them, then the projects with no tab, so the first move keeps the rest
/// put.
fn project_base(app: &App) -> Vec<ProjectId> {
    if projects_ordered(app) {
        return project_ids(app);
    }
    let tabs = tab_ids(app);
    let rest: Vec<ProjectId> = project_ids(app)
        .into_iter()
        .filter(|p| !tabs.contains(p))
        .collect();
    tabs.into_iter().chain(rest).collect()
}

/// `worktree`'s sessions as the BAND shows them, first card to last.
pub fn band_shown(app: &App, worktree: &WorktreeId) -> Vec<AgentId> {
    crate::launcher::bands(app)
        .into_iter()
        .filter(|b| &b.worktree == worktree)
        .flat_map(|b| b.cards)
        .filter_map(|c| match c {
            crate::launcher::Card::Session(row) => Some(row.agent.id),
            crate::launcher::Card::Terminal(_) => None,
        })
        .collect()
}

/// `worktree`'s cards on the KANBAN, lane by lane, and the ones beside
/// `id` in its lane: what a step there walks past.
fn board_shown(app: &App, worktree: &WorktreeId, id: &AgentId) -> (Vec<AgentId>, Vec<AgentId>) {
    let lanes = crate::launcher::kanban(app, app.launcher.kanban_all);
    let mine = |lane: &Vec<LauncherRow>| -> Vec<AgentId> {
        lane.iter()
            .filter(|r| &r.agent.worktree_id == worktree)
            .map(|r| r.agent.id.clone())
            .collect()
    };
    let shown = lanes.iter().flat_map(mine).collect();
    let peers = lanes
        .iter()
        .find(|l| l.iter().any(|r| &r.agent.id == id))
        .map(mine)
        .unwrap_or_default();
    (shown, peers)
}

/// An id as the `Reorder` request carries it.
trait RawId {
    fn raw(self) -> String;
}
impl RawId for ProjectId {
    fn raw(self) -> String {
        self.0
    }
}
impl RawId for WorktreeId {
    fn raw(self) -> String {
        self.0
    }
}
impl RawId for AgentId {
    fn raw(self) -> String {
        self.0
    }
}

fn raw<T: RawId>(ids: Vec<T>) -> Vec<String> {
    ids.into_iter().map(RawId::raw).collect()
}

fn is_root(app: &App, id: &WorktreeId) -> bool {
    app.tree.worktrees.iter().any(|w| &w.id == id && w.is_main)
}

/// `⌥↑` / `⌥↓`, and the menus' **Move up** / **Move down**: `target` one
/// place earlier (`dir` < 0) or later among the rows shown beside it — the
/// open tabs, the project's other linked worktrees, the band's cards or on
/// the KANBAN the cards of its checkout in its lane. True when a
/// `Reorder` went out; otherwise the flash says why not.
pub fn step(app: &mut App, target: &OrderTarget, dir: i64, out: &mut Vec<ClientRequest>) -> bool {
    let (scope, ids) = match target {
        OrderTarget::Project(id) => {
            let all = project_base(app);
            let tabs = tab_ids(app);
            let peers = if tabs.contains(id) { tabs } else { all.clone() };
            (
                ReorderScope::Projects,
                step_id(&all, &peers, id, dir).map(raw),
            )
        }
        OrderTarget::Worktree(id) => {
            let Some(project) = app
                .tree
                .worktrees
                .iter()
                .find(|w| &w.id == id)
                .map(|w| w.project_id.clone())
            else {
                return false;
            };
            if is_root(app, id) {
                app.chrome.flash = Some(ROOT_STAYS_FIRST.into());
                return false;
            }
            let all = worktree_ids(app, &project);
            let peers: Vec<WorktreeId> = all.iter().filter(|w| !is_root(app, w)).cloned().collect();
            let ids = step_id(&all, &peers, id, dir).map(|ids| raw(root_first(app, ids)));
            (ReorderScope::Worktrees(project), ids)
        }
        OrderTarget::Agent(id) => {
            let Some(worktree) = app
                .tree
                .agents
                .iter()
                .find(|a| &a.id == id)
                .map(|a| a.worktree_id.clone())
            else {
                return false;
            };
            let (shown, peers) = if app.launcher.launcher_kanban {
                board_shown(app, &worktree, id)
            } else {
                let shown = band_shown(app, &worktree);
                (shown.clone(), shown)
            };
            let all = agent_ids(app, &worktree, &shown);
            (
                ReorderScope::Agents(worktree),
                step_id(&all, &peers, id, dir).map(raw),
            )
        }
    };
    let Some(ids) = ids else {
        app.chrome.flash = Some(
            if dir < 0 {
                "already first"
            } else {
                "already last"
            }
            .into(),
        );
        app.chrome.dirty = true;
        return false;
    };
    reorder(app, scope, ids, out)
}

/// A PROJECT TAB dropped before or after another.
pub fn drop_project(
    app: &mut App,
    id: &ProjectId,
    target: &ProjectId,
    place: Place,
    out: &mut Vec<ClientRequest>,
) -> bool {
    match place_id(&project_base(app), id, target, place) {
        Some(ids) => reorder(app, ReorderScope::Projects, raw(ids), out),
        None => false,
    }
}

/// A BAND dropped before or after another of its project's: never the
/// root, and never above it.
pub fn drop_worktree(
    app: &mut App,
    id: &WorktreeId,
    target: &WorktreeId,
    place: Place,
    out: &mut Vec<ClientRequest>,
) -> bool {
    let Some(project) = app
        .tree
        .worktrees
        .iter()
        .find(|w| &w.id == id)
        .map(|w| w.project_id.clone())
    else {
        return false;
    };
    if is_root(app, id) {
        app.chrome.flash = Some(ROOT_STAYS_FIRST.into());
        return false;
    }
    match place_id(&worktree_ids(app, &project), id, target, place) {
        Some(ids) => {
            let ids = raw(root_first(app, ids));
            reorder(app, ReorderScope::Worktrees(project), ids, out)
        }
        None => false,
    }
}

/// A session card dropped before or after another of its checkout's:
/// `shown` is the order the view showed them in (the band's cards, the
/// board's).
pub fn drop_agent(
    app: &mut App,
    id: &AgentId,
    target: &AgentId,
    place: Place,
    shown: &[AgentId],
    out: &mut Vec<ClientRequest>,
) -> bool {
    let Some(worktree) = app
        .tree
        .agents
        .iter()
        .find(|a| &a.id == id)
        .map(|a| a.worktree_id.clone())
    else {
        return false;
    };
    match place_id(&agent_ids(app, &worktree, shown), id, target, place) {
        Some(ids) => reorder(app, ReorderScope::Agents(worktree), raw(ids), out),
        None => false,
    }
}

/// The KANBAN's cards of `worktree`, lane by lane: what a board drop
/// reorders from.
pub fn board_order(app: &App, worktree: &WorktreeId) -> Vec<AgentId> {
    board_shown(app, worktree, &AgentId(String::new())).0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn place_and_step_move_one_id_and_say_when_nothing_changes() {
        let ids = ["a", "b", "c", "d"];
        assert_eq!(
            place_id(&ids, &"d", &"b", Place::Before),
            Some(vec!["a", "d", "b", "c"])
        );
        assert_eq!(
            place_id(&ids, &"a", &"c", Place::After),
            Some(vec!["b", "c", "a", "d"])
        );
        assert_eq!(place_id(&ids, &"b", &"c", Place::Before), None);
        assert_eq!(place_id(&ids, &"x", &"c", Place::Before), None);
        // Peers are the rows shown beside it: the step skips the rest.
        assert_eq!(
            step_id(&ids, &["b", "d"], &"d", -1),
            Some(vec!["a", "d", "b", "c"])
        );
        assert_eq!(
            step_id(&ids, &["b", "d"], &"b", 1),
            Some(vec!["a", "c", "d", "b"])
        );
        assert_eq!(step_id(&ids, &ids, &"a", -1), None);
        assert_eq!(step_id(&ids, &ids, &"d", 1), None);
        assert_eq!(Place::of(0, 10), Place::Before);
        assert_eq!(Place::of(5, 10), Place::After);
    }

    #[test]
    fn lane_drops_only_do_what_the_user_can_cause() {
        use Lane::*;
        assert_eq!(lane_drop(Unseen, DropLane::Lane(Done)), LaneDrop::Seen);
        assert_eq!(lane_drop(Running, DropLane::Archived), LaneDrop::Archive);
        assert_eq!(lane_drop(Done, DropLane::Lane(Done)), LaneDrop::Reorder);
        for from in [NeedsInput, Running, PrOpen] {
            assert!(matches!(
                lane_drop(from, DropLane::Lane(Done)),
                LaneDrop::Refuse(_)
            ));
        }
        assert!(
            matches!(lane_drop(NeedsInput, DropLane::Lane(Done)), LaneDrop::Refuse(why) if why.contains("Needs you"))
        );
        for to in [NeedsInput, Running, Unseen, PrOpen] {
            assert!(matches!(
                lane_drop(Done, DropLane::Lane(to)),
                LaneDrop::Refuse(_)
            ));
        }
    }

    #[test]
    fn a_reorder_goes_out_only_to_a_daemon_that_takes_it() {
        let mut app = App::new();
        let scope = ReorderScope::Agents(WorktreeId("w".into()));
        let mut out = Vec::new();

        app.orch.daemon_protocol = Some(REORDER_PROTOCOL - 1);
        assert!(!reorder(
            &mut app,
            scope.clone(),
            vec!["a".into()],
            &mut out
        ));
        assert!(out.is_empty(), "an older daemon would drop the connection");
        assert_eq!(app.chrome.flash.as_deref(), Some(DAEMON_TOO_OLD));

        app.orch.daemon_protocol = Some(REORDER_PROTOCOL);
        assert!(reorder(
            &mut app,
            scope.clone(),
            vec!["b".into(), "a".into()],
            &mut out
        ));
        match out.as_slice() {
            [ClientRequest::Reorder {
                scope: sent, ids, ..
            }] => {
                assert_eq!(sent, &scope);
                assert_eq!(ids, &["b".to_string(), "a".to_string()]);
            }
            other => panic!("expected one Reorder, got {other:?}"),
        }
    }
}
