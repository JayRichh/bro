//! Pick a thing, see its verbs: one dispatch from a registry verb and the
//! thing it acts on to what the verb does there.
//!
//! The menus (a right-click, a card's `⋯`, the project tab's), the VERBS
//! MENU `?` opens, the footer's leading verbs and the COMMAND PALETTE's
//! top rows are all built here, from nebula-config's registry: its
//! [`MENUS`](nebula_config::actions::MENUS) for which verbs a kind's menu
//! lists and in what order, its [`PRIMARY`](nebula_config::actions::PRIMARY)
//! for the few a footer leads with, and its menu labels for the words —
//! the desktop window reads the same tables. A verb on a [`Target`] is
//! [`menu_action`]: the row-addressed [`MenuAction`] its menu row runs
//! (Rename on a session renames that session, whichever card the cursor
//! is on), or, for a verb with no such form, its key run on the selection
//! ([`MenuAction::Run`]) — offered only when the target is the selection,
//! so the key's target and the row's are the same thing. A key bound to
//! one of the menus' own verbs (Restart, Run, Delete worktree…) goes the
//! same way ([`run_on_selection`]).

use super::*;
use crate::keymap::{Action, KeyChord};
use nebula_config::actions::{menu_label, menu_of, primary_of, AppliesTo, Surface, WorksIn};

/// A thing a verb acts on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Target {
    Repo(ProjectId),
    Worktree(WorktreeId),
    Session(AgentId),
    Terminal(TerminalId),
}

impl Target {
    pub(crate) fn kind(&self) -> AppliesTo {
        match self {
            Target::Repo(_) => AppliesTo::Repo,
            Target::Worktree(_) => AppliesTo::Worktree,
            Target::Session(_) => AppliesTo::Session,
            Target::Terminal(_) => AppliesTo::Terminal,
        }
    }
}

/// Where the keys are: the grid of cards, the board, or the pane.
pub(crate) fn place(app: &App) -> WorksIn {
    if app.nav.focus == Focus::Terminal || app.pane.collapsed {
        WorksIn::Pane
    } else if app.launcher.launcher_kanban && app.launcher_active() {
        WorksIn::Board
    } else {
        WorksIn::Grid
    }
}

/// What the keys act on now: the PROJECT TAB under the header's cursor,
/// the band with no card to be on, the card (a board card is a session
/// card), or in the panels the focused column's row. None with nothing
/// under the cursor.
pub(crate) fn selection(app: &App) -> Option<Target> {
    if let Some(p) = &app.launcher.launcher_tab_cursor {
        return Some(Target::Repo(p.clone()));
    }
    let row = || match app.selected_session_row()? {
        SessionRow::Agent(a) => Some(Target::Session(a.id)),
        SessionRow::Terminal(t) => Some(Target::Terminal(t.id)),
        SessionRow::Link(_) => None,
    };
    let worktree = || {
        app.selected_worktree()
            .map(|w| Target::Worktree(w.id.clone()))
    };
    if app.launcher_active() && !app.launcher.launcher_columns {
        if let Some(w) = launcher::empty_band(app).or_else(|| launcher::folded_band(app)) {
            return Some(Target::Worktree(w));
        }
        return row()
            .or_else(worktree)
            .or_else(|| app.selected_project().map(|p| Target::Repo(p.id.clone())));
    }
    match app.nav.focus {
        Focus::Projects => app.selected_project().map(|p| Target::Repo(p.id.clone())),
        Focus::Worktrees => {
            if app.selected_worktree_pr().is_some() || app.selected_worktree_issue().is_some() {
                None
            } else {
                worktree()
            }
        }
        Focus::Sessions | Focus::Terminal => row().or_else(worktree),
    }
}

/// The checkout a target runs in, and the project it is of.
fn worktree_of(app: &App, t: &Target) -> Option<WorktreeId> {
    match t {
        Target::Worktree(w) => Some(w.clone()),
        Target::Session(id) => app
            .tree
            .agents
            .iter()
            .find(|a| &a.id == id)
            .map(|a| a.worktree_id.clone()),
        Target::Terminal(id) => app
            .tree
            .terminals
            .iter()
            .find(|x| &x.id == id)
            .map(|x| x.worktree_id.clone()),
        Target::Repo(_) => None,
    }
}

fn project_of(app: &App, t: &Target) -> Option<ProjectId> {
    match t {
        Target::Repo(p) => Some(p.clone()),
        _ => {
            let w = worktree_of(app, t)?;
            app.tree
                .worktrees
                .iter()
                .find(|x| x.id == w)
                .map(|x| x.project_id.clone())
        }
    }
}

/// Whether a verb's key, run now, would act on `t`: `t` is the selection,
/// or the checkout or project the selection is in.
fn on_selection(app: &App, t: &Target) -> bool {
    let Some(sel) = selection(app) else {
        return false;
    };
    match t {
        Target::Worktree(w) => worktree_of(app, &sel).as_ref() == Some(w),
        Target::Repo(p) => project_of(app, &sel).as_ref() == Some(p),
        _ => sel == *t,
    }
}

/// What verb `action` does to `t`, as a menu row runs it: None where it
/// means nothing there now — Unarchive on a live session, Delete worktree
/// on a root checkout, a key-only verb on a thing the cursor is not on.
pub(crate) fn menu_action(app: &App, action: Action, t: &Target) -> Option<MenuAction> {
    let spec = crate::keymap::spec_of(action)?;
    if !spec.applies_to.contains(&t.kind()) || spec.surface == Surface::Desktop {
        return None;
    }
    let keyed = |app: &App| on_selection(app, t).then_some(MenuAction::Run(action));
    match t {
        Target::Session(id) => {
            let a = app.tree.agents.iter().find(|a| &a.id == id)?;
            // A Cloud row has no terminal to attach or restart: its agent
            // runs in the cloud sandbox, so its open is the browser and
            // its follow-up the message queue.
            let cloud = a.cloud_session_url().filter(|_| !a.archived);
            let local = cloud.is_none() && !a.archived;
            let id = id.clone();
            match action {
                Action::Activate => match cloud {
                    Some(url) => Some(MenuAction::OpenLink(url)),
                    None => (!a.archived).then_some(MenuAction::Attach(SessionRef::Agent(id))),
                },
                Action::FollowUp => match cloud {
                    Some(_) => Some(MenuAction::SendCloudMessage(id)),
                    None => (!a.archived).then_some(MenuAction::FollowUp),
                },
                Action::Restart => local.then_some(MenuAction::RestartAgent(id)),
                Action::DuplicateSession => Some(MenuAction::DuplicateAgent(id)),
                Action::MoveSession => local.then_some(MenuAction::MoveAgentPicker(id)),
                Action::MoveToAccount => local.then_some(MenuAction::SwitchAccountPicker(id)),
                Action::Rename => (!a.archived).then_some(MenuAction::RenameAgent(id)),
                Action::PinOrchestrator if local => Some(if a.role.is_orchestrator() {
                    MenuAction::ClearOrchestrator
                } else {
                    MenuAction::MakeOrchestrator(id)
                }),
                Action::PinOrchestrator => None,
                Action::Archive => (!a.archived).then_some(MenuAction::ArchiveAgent(id)),
                Action::Unarchive => a.archived.then_some(MenuAction::UnarchiveAgent(id)),
                Action::Delete => Some(MenuAction::DeleteAgent(id)),
                // The card's pull request and the issue it was started
                // from, what `⇧V` and `⇧I` open.
                Action::OpenPullRequest => crate::launcher::row(app, &id)
                    .and_then(|row| row.pr)
                    .map(|pr| MenuAction::OpenLink(pr.url)),
                Action::OpenIssue => a.issue_url.clone().map(MenuAction::OpenLink),
                // The checkout's pull request: the composer while it has
                // none, the merge while one is open.
                Action::CreatePullRequest if !a.archived => {
                    ships(app, &a.worktree_id).then(|| MenuAction::CreatePullRequest {
                        worktree: a.worktree_id.clone(),
                        agent: Some(id),
                    })
                }
                Action::MergePullRequest if !a.archived => {
                    merges(app, &a.worktree_id).then(|| MenuAction::MergePullRequest {
                        worktree: a.worktree_id.clone(),
                        agent: Some(id),
                    })
                }
                Action::CreatePullRequest | Action::MergePullRequest => None,
                Action::FocusScope => super::board::scope_for(app, t).map(MenuAction::FocusScope),
                Action::ReorderUp | Action::ReorderDown if !a.archived => {
                    Some(MenuAction::Reorder(
                        crate::order::OrderTarget::Agent(id),
                        if action == Action::ReorderUp { -1 } else { 1 },
                    ))
                }
                Action::ReorderUp | Action::ReorderDown => None,
                _ => keyed(app),
            }
        }
        Target::Terminal(id) => match action {
            Action::Activate => Some(MenuAction::Attach(SessionRef::Terminal(id.clone()))),
            Action::Rename => Some(MenuAction::RenameTerminal(id.clone())),
            Action::Delete => Some(MenuAction::CloseTerminal(id.clone())),
            _ => keyed(app),
        },
        Target::Worktree(id) => {
            let w = app.tree.worktrees.iter().find(|w| &w.id == id)?;
            // A stand-in checkout is not a place the DAEMON knows yet.
            if app.is_placeholder_worktree(id) {
                return None;
            }
            let id = id.clone();
            match action {
                Action::New => Some(MenuAction::NewAgent(id)),
                Action::NewTerminal => Some(MenuAction::NewTerminal(id)),
                Action::ToggleRun => Some(MenuAction::ToggleRun(id)),
                Action::OpenWorktree => Some(MenuAction::OpenWorktree(id)),
                // The ROOT WORKTREE moves between branches; a linked
                // worktree, named for its branch, is deleted instead.
                Action::SwitchBranch => w.is_main.then_some(MenuAction::SwitchBranch(id)),
                Action::DeleteWorktree => (!w.is_main).then_some(MenuAction::DeleteWorktree(id)),
                Action::CreatePullRequest => {
                    ships(app, &id).then_some(MenuAction::CreatePullRequest {
                        worktree: id,
                        agent: None,
                    })
                }
                Action::MergePullRequest => {
                    merges(app, &id).then_some(MenuAction::MergePullRequest {
                        worktree: id,
                        agent: None,
                    })
                }
                Action::FocusScope => super::board::scope_for(app, t).map(MenuAction::FocusScope),
                // The band's place among the project's; the root's stays
                // first.
                Action::ReorderUp | Action::ReorderDown => (!w.is_main).then(|| {
                    MenuAction::Reorder(
                        crate::order::OrderTarget::Worktree(id),
                        if action == Action::ReorderUp { -1 } else { 1 },
                    )
                }),
                // Only with something to delete.
                Action::DeleteAll => {
                    let any =
                        app.tree.agents.iter().any(|a| {
                            a.worktree_id == w.id && a.archived == app.launcher.show_archived
                        }) || app.tree.terminals.iter().any(|x| x.worktree_id == w.id);
                    if any {
                        keyed(app)
                    } else {
                        None
                    }
                }
                _ => keyed(app),
            }
        }
        Target::Repo(id) => {
            // The checkout the grid would launch into: what the project's
            // Run, Open and Delete worktree act on.
            let checkout = || crate::launcher::checkout_for(app, id);
            let is_main =
                |w: &WorktreeId| app.tree.worktrees.iter().any(|x| &x.id == w && x.is_main);
            match action {
                Action::NewWorktree => Some(MenuAction::NewWorktree(id.clone())),
                Action::SwitchBranch => app.root_worktree(id).map(MenuAction::SwitchBranch),
                Action::ToggleRun => checkout().map(MenuAction::ToggleRun),
                Action::OpenWorktree => checkout().map(MenuAction::OpenWorktree),
                Action::DeleteWorktree => checkout()
                    .filter(|w| !is_main(w))
                    .map(MenuAction::DeleteWorktree),
                Action::Rename => Some(MenuAction::RenameProject(id.clone())),
                Action::ReorderUp | Action::ReorderDown => Some(MenuAction::Reorder(
                    crate::order::OrderTarget::Project(id.clone()),
                    if action == Action::ReorderUp { -1 } else { 1 },
                )),
                Action::RemoveProject => Some(MenuAction::RemoveProject(id.clone())),
                Action::FocusScope => super::board::scope_for(app, t).map(MenuAction::FocusScope),
                _ => keyed(app),
            }
        }
    }
}

/// Can checkout `w` open a pull request from here: none open on it, not
/// on its base branch, not a stand-in still being cut.
fn ships(app: &App, w: &WorktreeId) -> bool {
    !app.is_placeholder_worktree(w)
        && !merges(app, w)
        && !app
            .jobs
            .worktree_base
            .get(w)
            .is_some_and(|b| b.on_default_branch)
        && app.tree.worktrees.iter().any(|x| &x.id == w)
}

/// Has checkout `w` an open pull request to merge?
fn merges(app: &App, w: &WorktreeId) -> bool {
    crate::launcher::worktree_pr(app, w).is_some_and(|pr| {
        matches!(
            pr.standing,
            crate::pull_request::Standing::Open | crate::pull_request::Standing::Draft
        )
    })
}

/// The words a row for `action` on `t` says: the registry's menu label,
/// or what it does now where that turns (Run or Stop run, Make or Unpin
/// orchestrator).
fn label(app: &App, action: Action, t: &Target) -> String {
    let id = crate::keymap::spec_of(action).map_or("", |s| s.id);
    match (action, t) {
        (Action::ToggleRun, _) => {
            let running = match t {
                Target::Repo(p) => {
                    crate::launcher::checkout_for(app, p).is_some_and(|w| app.worktree_running(&w))
                }
                _ => worktree_of(app, t).is_some_and(|w| app.worktree_running(&w)),
            };
            if running { "Stop run" } else { "Run" }.into()
        }
        (Action::PinOrchestrator, Target::Session(id))
            if app
                .tree
                .agents
                .iter()
                .any(|a| &a.id == id && a.role.is_orchestrator()) =>
        {
            "Unpin orchestrator".into()
        }
        // A Cloud row's open is the browser and its follow-up the message
        // queue: the row says so.
        (Action::Activate | Action::FollowUp, Target::Session(id))
            if app
                .tree
                .agents
                .iter()
                .any(|a| &a.id == id && !a.archived && a.cloud_session_url().is_some()) =>
        {
            if action == Action::Activate {
                "Open in browser".into()
            } else {
                "Send to cloud session".into()
            }
        }
        _ => menu_label(id, t.kind()).to_string(),
    }
}

/// A menu row for verb `id` on `t`, or None where it means nothing there.
fn row(app: &App, id: &str, t: &Target) -> Option<MenuItem> {
    let spec = nebula_config::actions::spec_by_id(id)?;
    let action = menu_action(app, spec.action, t)?;
    let item = MenuItem::new(label(app, spec.action, t), action).keyed(spec.action);
    Some(if spec.destructive {
        MenuItem {
            destructive: true,
            ..item
        }
    } else {
        item
    })
}

/// The rows of `ids` that mean something on `t`, in that order.
fn rows(app: &App, ids: &[&str], t: &Target) -> Vec<MenuItem> {
    ids.iter().filter_map(|id| row(app, id, t)).collect()
}

/// `t`'s menu: its kind's rows ([`MENUS`](nebula_config::actions::MENUS)),
/// and for a card its checkout's after them — the RUN COMMAND, the
/// checkout opened, cut or deleted — named as the checkout's where the
/// card has a row of the same verb (Move worktree up).
pub(crate) fn menu_for(app: &App, t: &Target) -> Vec<MenuItem> {
    let mut items = rows(app, menu_of(t.kind()), t);
    let card = matches!(t, Target::Session(_) | Target::Terminal(_));
    // An archived card keeps no checkout verbs: it runs in none.
    let live = match t {
        Target::Session(id) => app.tree.agents.iter().any(|a| &a.id == id && !a.archived),
        _ => true,
    };
    if card && live {
        if let Some(w) = worktree_of(app, t) {
            let checkout = Target::Worktree(w);
            for mut item in rows(app, menu_of(AppliesTo::Worktree), &checkout) {
                if items.iter().any(|i| i.verb == item.verb) {
                    item.label = match item.verb {
                        Some(Action::ReorderUp) => "Move worktree up".into(),
                        Some(Action::ReorderDown) => "Move worktree down".into(),
                        _ => continue,
                    };
                }
                items.push(item);
            }
        }
    }
    items
}

/// The verbs on `t` its menu leaves out that still mean something here
/// (`place`), as their keys run them: a pull request commented on, the
/// subagents, a file found. Not the cursor's walks: those are keys, not
/// verbs.
fn more_rows(app: &App, t: &Target, shown: &[MenuItem]) -> Vec<MenuItem> {
    let place = place(app);
    let kinds = match t {
        Target::Session(_) | Target::Terminal(_) => vec![t.kind(), AppliesTo::Worktree],
        _ => vec![t.kind()],
    };
    let mut out = Vec::new();
    for kind in &kinds {
        let on = if *kind == t.kind() {
            t.clone()
        } else {
            match worktree_of(app, t) {
                Some(w) => Target::Worktree(w),
                None => continue,
            }
        };
        for spec in crate::keymap::ACTIONS {
            if spec.group == "NAVIGATE"
                || !spec.applies_to.contains(kind)
                || !spec.works_in.contains(&place)
                || matches!(spec.action, Action::Verbs | Action::Help)
                || shown
                    .iter()
                    .chain(&out)
                    .any(|i: &MenuItem| i.verb == Some(spec.action))
            {
                continue;
            }
            if let Some(item) = row(app, spec.id, &on) {
                out.push(item);
            }
        }
    }
    out
}

/// The app's own verbs, where nothing is selected: the views and the
/// boxes, run as their keys run them.
fn global_rows(app: &App) -> Vec<MenuItem> {
    let place = place(app);
    crate::keymap::ACTIONS
        .iter()
        .filter(|s| {
            s.applies_to.contains(&AppliesTo::Global)
                && s.works_in.contains(&place)
                && s.group != "NAVIGATE"
                && crate::palette::is_command(s.action)
                && !matches!(s.action, Action::Verbs | Action::Help)
        })
        .map(|s| {
            MenuItem::new(
                menu_label(s.id, AppliesTo::Global),
                MenuAction::Run(s.action),
            )
            .keyed(s.action)
        })
        .collect()
}

/// What a verbs menu's title names: the thing its verbs act on.
fn name(app: &App, t: &Target) -> String {
    let t = match t {
        Target::Repo(id) => app
            .tree
            .projects
            .iter()
            .find(|p| &p.id == id)
            .map(|p| p.name.clone()),
        Target::Worktree(id) => app
            .tree
            .worktrees
            .iter()
            .find(|w| &w.id == id)
            .map(|w| w.branch.clone()),
        Target::Session(id) => app
            .tree
            .agents
            .iter()
            .find(|a| &a.id == id)
            .map(|a| a.name.clone()),
        Target::Terminal(id) => app
            .tree
            .terminals
            .iter()
            .find(|x| &x.id == id)
            .map(|x| x.name.clone()),
    };
    t.unwrap_or_default()
}

/// The **All keys…** row every verbs menu ends on: the full help.
fn all_keys() -> MenuItem {
    MenuItem::new("All keys\u{2026}", MenuAction::AllKeys).keyed(Action::Help)
}

/// A verbs menu over `items`: titled, type-ahead, **All keys…** last.
fn verbs_menu(title: String, mut items: Vec<MenuItem>) -> ContextMenu {
    items.push(all_keys());
    ContextMenu {
        title: Some(title),
        filter: Some(MenuFilter {
            query: String::new(),
            all: items.clone(),
        }),
        items,
        at: None,
        hover: 0,
        area: ratatui::layout::Rect::default(),
        parent: None,
    }
}

/// The VERBS MENU for the selection: its menu's rows, then the rest of
/// what applies to it here, then **All keys…**.
pub(crate) fn selection_menu(app: &App) -> ContextMenu {
    match selection(app) {
        Some(t) => {
            let mut items = menu_for(app, &t);
            let more = more_rows(app, &t, &items);
            items.extend(more);
            let what = match t.kind() {
                AppliesTo::Repo => "project",
                AppliesTo::Worktree => "checkout",
                AppliesTo::Terminal => "terminal",
                _ => "session",
            };
            verbs_menu(format!("{what} {}", name(app, &t)), items)
        }
        None => verbs_menu("Verbs".into(), global_rows(app)),
    }
}

/// `?`: the VERBS MENU for the selection, centered over the grid.
pub(crate) fn open(app: &mut App) {
    app.modals.verbs_under = None;
    app.modals.overlay = Some(Overlay::Menu(selection_menu(app)));
    app.chrome.dirty = true;
}

/// The verbs of the selection a footer and the palette lead with: the
/// first `n` of its kind's [`PRIMARY`](nebula_config::actions::PRIMARY)
/// list that mean something on it now — the app's own with nothing
/// selected — with the words the footer prints. `keyed`: only verbs
/// with a key bound, for a footer that names the key.
pub(crate) fn primary(app: &App, n: usize, keyed: bool) -> Vec<(Action, &'static str)> {
    let bound = |a: Action| !keyed || app.chrome.keymap.first(a).is_some();
    let Some(t) = selection(app) else {
        let place = place(app);
        return primary_of(AppliesTo::Global)
            .iter()
            .filter_map(|id| nebula_config::actions::spec_by_id(id))
            .filter(|s| s.works_in.contains(&place) && bound(s.action))
            .take(n)
            .map(|s| (s.action, s.word))
            .collect();
    };
    let mut out = Vec::new();
    let tries = |kind: AppliesTo, on: &Target, out: &mut Vec<(Action, &'static str)>| {
        for id in primary_of(kind) {
            if out.len() >= n {
                return;
            }
            let Some(spec) = nebula_config::actions::spec_by_id(id) else {
                continue;
            };
            if out.iter().any(|(a, _)| *a == spec.action) || !bound(spec.action) {
                continue;
            }
            if menu_action(app, spec.action, on).is_some() {
                out.push((spec.action, spec.word));
            }
        }
    };
    tries(t.kind(), &t, &mut out);
    // A card short of verbs (an archived one) fills from its checkout.
    if let (true, Some(w)) = (out.len() < n, worktree_of(app, &t)) {
        if matches!(t, Target::Session(_) | Target::Terminal(_)) {
            tries(AppliesTo::Worktree, &Target::Worktree(w), &mut out);
        }
    }
    out
}

/// A key bound to one of the menus' own verbs (Restart, Run, Delete
/// worktree…), or the palette's row of one: the verb on the selection —
/// or on the checkout or project it is in, where the verb is theirs —
/// as its menu row would run it.
pub(crate) fn run_on_selection(app: &mut App, action: Action, out: &mut Vec<ClientRequest>) {
    let Some(sel) = selection(app) else {
        flash_nothing(app, action);
        return;
    };
    let mut targets = vec![sel.clone()];
    if let Some(w) = worktree_of(app, &sel).filter(|_| !matches!(sel, Target::Worktree(_))) {
        targets.push(Target::Worktree(w));
    }
    if let Some(p) = project_of(app, &sel).filter(|_| !matches!(sel, Target::Repo(_))) {
        targets.push(Target::Repo(p));
    }
    let found = targets
        .iter()
        .find_map(|t| match menu_action(app, action, t) {
            // The key itself: running it again would come straight back here.
            Some(MenuAction::Run(_)) | None => None,
            Some(m) => Some(m),
        });
    match found {
        Some(m) => run_menu_action(app, m, out),
        None => flash_nothing(app, action),
    }
}

fn flash_nothing(app: &mut App, action: Action) {
    let label = crate::keymap::spec_of(action).map_or("that", |s| s.label);
    app.chrome.flash = Some(format!("{label}: not for what is under the cursor"));
}

// ---- an overlay's verbs ----

/// What `?` lists over an overlay with rows of its own: its keys, as the
/// overlay reads them. Enter puts the overlay back and presses the key.
const DIFF_VERBS: &[(&str, KeyCode, KeyModifiers, bool)] = &[
    (
        "Compare with the base or HEAD",
        KeyCode::Char('g'),
        KeyModifiers::CONTROL,
        false,
    ),
    (
        "Next change",
        KeyCode::Char('n'),
        KeyModifiers::CONTROL,
        false,
    ),
    (
        "Previous change",
        KeyCode::Char('p'),
        KeyModifiers::CONTROL,
        false,
    ),
    (
        "Accept the change (stage it)",
        KeyCode::Char('y'),
        KeyModifiers::CONTROL,
        false,
    ),
    (
        "Reject the change\u{2026}",
        KeyCode::Char('x'),
        KeyModifiers::CONTROL,
        true,
    ),
    (
        "Mark the file reviewed",
        KeyCode::Char('r'),
        KeyModifiers::CONTROL,
        false,
    ),
    (
        "Tree or flat list",
        KeyCode::Char('t'),
        KeyModifiers::CONTROL,
        false,
    ),
    (
        "Next layout",
        KeyCode::Char('s'),
        KeyModifiers::CONTROL,
        false,
    ),
    (
        "Full context or folded",
        KeyCode::Char('o'),
        KeyModifiers::CONTROL,
        false,
    ),
    ("Close the diff", KeyCode::Esc, KeyModifiers::NONE, false),
];

const ACCOUNT_VERBS: &[(&str, KeyCode, KeyModifiers, bool)] = &[
    ("Edit\u{2026}", KeyCode::Enter, KeyModifiers::NONE, false),
    ("Log in", KeyCode::Char('l'), KeyModifiers::NONE, false),
    ("On or off", KeyCode::Char(' '), KeyModifiers::NONE, false),
    (
        "Make default",
        KeyCode::Char('d'),
        KeyModifiers::NONE,
        false,
    ),
    (
        "Use for\u{2026}",
        KeyCode::Char('p'),
        KeyModifiers::NONE,
        false,
    ),
    (
        "Move the session here",
        KeyCode::Char('m'),
        KeyModifiers::NONE,
        false,
    ),
    (
        "Rename\u{2026}",
        KeyCode::Char('r'),
        KeyModifiers::NONE,
        false,
    ),
    (
        "Duplicate\u{2026}",
        KeyCode::Char('c'),
        KeyModifiers::NONE,
        false,
    ),
    (
        "Add an account\u{2026}",
        KeyCode::Char('a'),
        KeyModifiers::NONE,
        false,
    ),
    ("Spread mode", KeyCode::Char('s'), KeyModifiers::NONE, false),
    (
        "Spread threshold",
        KeyCode::Char('t'),
        KeyModifiers::NONE,
        false,
    ),
    (
        "Remove\u{2026}",
        KeyCode::Char('x'),
        KeyModifiers::NONE,
        true,
    ),
    ("Close", KeyCode::Esc, KeyModifiers::NONE, false),
];

/// `?` over the diff (its file list, nothing typed in its filter) or the
/// accounts list: that overlay's verbs, the overlay kept to come back to.
/// False where `?` is the overlay's own (a filter being typed, a form).
pub(crate) fn open_over_overlay(app: &mut App) -> bool {
    let (title, table) = match &app.modals.overlay {
        Some(Overlay::Diff(v)) if v.filter.is_empty() && v.confirm_discard.is_none() => {
            let file = v.selected_path().unwrap_or_default().to_string();
            (format!("diff {file}"), DIFF_VERBS)
        }
        Some(Overlay::Accounts(v)) if matches!(v.mode, crate::accounts_overlay::Mode::List) => {
            let name = v.selected().map(|r| r.key.clone()).unwrap_or_default();
            (format!("account {name}"), ACCOUNT_VERBS)
        }
        _ => return false,
    };
    let items = table
        .iter()
        .map(|(label, code, mods, destructive)| MenuItem {
            label: (*label).into(),
            action: MenuAction::OverlayKey(*code, *mods),
            destructive: *destructive,
            verb: None,
            key: Some(
                KeyChord {
                    code: *code,
                    mods: *mods,
                }
                .display(),
            ),
        })
        .collect();
    let under = app.modals.overlay.take().map(Box::new);
    app.modals.overlay = Some(Overlay::Menu(verbs_menu(title, items)));
    app.modals.verbs_under = under;
    app.chrome.dirty = true;
    true
}

/// The COMMAND PALETTE's first rows: the selection's own menu verbs, by
/// their menu words and the thing's name (`Archive · agent-1`), each run
/// as its key runs on the selection.
pub(crate) fn palette_verbs(app: &App) -> Vec<(Action, String)> {
    let Some(t) = selection(app) else {
        return Vec::new();
    };
    let name = name(app, &t);
    rows(app, menu_of(t.kind()), &t)
        .into_iter()
        .filter_map(|item| Some((item.verb?, format!("{} \u{b7} {name}", item.label))))
        .collect()
}
