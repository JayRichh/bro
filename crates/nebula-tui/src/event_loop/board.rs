//! The board's verbs beyond the cursor (`event_loop::launcher` walks it):
//! what letting a card go on a lane does — the mouse's drop and
//! `Shift+←`/`Shift+→` alike, both through core's `drop_verb` — and the
//! bench's SCOPE, the header's scope chip.

use super::*;
use crate::order::{lane_drop, DropLane, LaneDrop, Place};
use nebula_core::board::Lane;
use nebula_core::scope::Scope;

/// What letting `agent`'s card, picked up in `from`, go on `to` does —
/// beside `beside` when it was let go over a card of its own checkout in
/// its own lane. The one function behind the drop and the move keys.
pub(super) fn run_lane_drop(
    app: &mut App,
    agent: AgentId,
    worktree: WorktreeId,
    from: Lane,
    to: DropLane,
    beside: Option<(AgentId, Place)>,
    out: &mut Vec<ClientRequest>,
) {
    app.chrome.dirty = true;
    match lane_drop(app, &agent, from, to) {
        LaneDrop::Refuse(why) => app.chrome.flash = Some(why.into()),
        LaneDrop::MarkSeen => {
            super::mark_agent_seen(app, &agent, out);
            app.chrome.flash = Some(
                if to == DropLane::Lane(Lane::Ship) {
                    crate::order::MARKED_SEEN_SHIP
                } else {
                    crate::order::MARKED_SEEN
                }
                .into(),
            );
        }
        LaneDrop::Archive => {
            super::archive_agent(app, agent, out);
        }
        LaneDrop::ArchiveAndOfferWorktreeDelete => archive_and_offer(app, agent, out),
        LaneDrop::OpenPr => super::ship::open_composer(app, &worktree, Some(agent), None),
        LaneDrop::MergePr => super::ship::open_merge(app, &worktree, Some(agent), None),
        LaneDrop::Reorder => match beside {
            Some((target, place)) => {
                let shown = crate::order::board_order(app, &worktree);
                crate::order::drop_agent(app, &agent, &target, place, &shown, out);
            }
            None if to != DropLane::Archived => {
                app.chrome.flash = Some(
                    "cards reorder beside cards of their own worktree; m moves the session to another"
                        .into(),
                );
            }
            None => {}
        },
    }
}

/// What the archive of a Done card asks: archive it, and — when it is the
/// last live thing in a linked worktree — whether its checkout goes too,
/// by the SAFE delete (unforced: the daemon keeps a checkout with
/// uncommitted changes and says so). Never the project's own checkout.
/// The board asks whatever **Confirm on archive** and the emptied-band
/// settings say: the delete is the point of the drop. Anywhere else it is
/// the plain archive.
pub(crate) fn archive_and_offer(app: &mut App, id: AgentId, out: &mut Vec<ClientRequest>) {
    let Some(a) = app.tree.agents.iter().find(|a| a.id == id).cloned() else {
        return;
    };
    let offer = app
        .tree
        .worktrees
        .iter()
        .find(|w| w.id == a.worktree_id)
        .filter(|w| {
            !w.is_main
                && !app.is_placeholder_worktree(&w.id)
                && !a.archived
                && live_cards_in(app, &w.id) == 1
        })
        .cloned();
    let Some(w) = offer else {
        super::archive_agent(app, id, out);
        return;
    };
    app.modals.overlay = Some(Overlay::Confirm(ConfirmDialog {
        title: "Archive session".into(),
        message: format!(
            "Archive '{}'? u brings it back.\nNothing else is live in worktree '{}': delete it from disk too?\nA checkout with uncommitted changes is kept, and says so.",
            a.name, w.branch
        ),
        action: PendingAction::ThenDeleteWorktree {
            first: Box::new(PendingAction::ArchiveAgent(id)),
            worktree: w.id,
            offered: true,
        },
        area: ratatui::layout::Rect::default(),
    }));
    app.chrome.dirty = true;
}

/// `Shift+←` / `Shift+→` on the board: the card under the cursor let go on
/// the next lane over, as a drop there; past Done, the Archived zone.
pub(crate) fn move_card(app: &mut App, dx: i64, out: &mut Vec<ClientRequest>) {
    app.chrome.dirty = true;
    if !(app.launcher.launcher_kanban && app.launcher_active()) {
        app.chrome.flash = Some("cards move between lanes on the board: \u{21e7}K".into());
        return;
    }
    let lanes = crate::launcher::kanban(app, app.launcher.kanban_all);
    let Some((l, row)) = app
        .selected_session()
        .filter(|_| !app.launcher.launcher_unaimed)
        .and_then(|a| crate::launcher::kanban_find(&lanes, &a.id))
    else {
        app.chrome.flash = Some("no card under the cursor to move".into());
        return;
    };
    let card = &lanes[l][row];
    let (agent, worktree) = (card.agent.id.clone(), card.agent.worktree_id.clone());
    let to = l as i64 + dx;
    let to = match usize::try_from(to) {
        Ok(i) if i < Lane::ALL.len() => DropLane::Lane(Lane::ALL[i]),
        Ok(_) => DropLane::Archived,
        Err(_) => {
            app.chrome.flash = Some("no lane left of Running".into());
            return;
        }
    };
    run_lane_drop(app, agent, worktree, Lane::ALL[l], to, None, out);
}

/// Back onto `agent`'s card, when it is still on the board: what closing
/// the REVIEW UNIT, the ship forms or a diff opened from a card lands on,
/// by the card's id — never by its old lane and row, since reading it may
/// have moved it — rather than the first card of the first lane.
pub(crate) fn return_to_card(app: &mut App, agent: Option<&AgentId>, out: &mut Vec<ClientRequest>) {
    let Some(id) = agent else {
        return;
    };
    let live = app.tree.agents.iter().any(|a| &a.id == id && !a.archived);
    if live && app.launcher_active() {
        launcher::select(app, id.clone(), out);
    }
}

// ---- the SCOPE ----

/// What FOCUS HERE on `t` narrows the scope chip to: that repo, or that
/// worktree (a card's is its checkout's) in its repo; the account kept.
pub(crate) fn scope_for(app: &App, t: &super::verbs::Target) -> Option<Scope> {
    use super::verbs::Target;
    let worktree = match t {
        Target::Repo(p) => {
            return Some(Scope {
                repo: Some(p.clone()),
                account: app.launcher.scope.account.clone(),
                ..Scope::default()
            })
        }
        Target::Worktree(w) => w.clone(),
        Target::Session(id) => app
            .tree
            .agents
            .iter()
            .find(|a| &a.id == id)?
            .worktree_id
            .clone(),
        Target::Terminal(id) => app
            .tree
            .terminals
            .iter()
            .find(|x| &x.id == id)?
            .worktree_id
            .clone(),
        // A file, a folder, an account and the background are no place on
        // the bench to narrow to.
        Target::File { .. } | Target::Folder { .. } | Target::Account(_) | Target::Global => {
            return None
        }
    };
    let w = app.tree.worktrees.iter().find(|x| x.id == worktree)?;
    Some(Scope {
        repo: Some(w.project_id.clone()),
        worktree: Some(w.id.clone()),
        session: None,
        account: app.launcher.scope.account.clone(),
    })
}

/// Set the scope chip to `scope`, and say so.
pub(crate) fn set_scope(app: &mut App, scope: Scope) {
    app.launcher.scope = scope;
    app.chrome.flash = Some(format!(
        "scope: {} · the chip's \u{2715} or Esc widens it again",
        scope_words(app).join(" \u{25b8} ")
    ));
    app.chrome.dirty = true;
}

/// Widen the scope to one segment of the chip: `keep` segments of it
/// (0 is everything).
pub(crate) fn widen_scope(app: &mut App, keep: usize) {
    let s = &mut app.launcher.scope;
    if keep < 3 {
        s.session = None;
    }
    if keep < 2 {
        s.worktree = None;
    }
    if keep < 1 {
        *s = Scope::default();
        // The board's "this project" is a scope too: everything is every
        // project's cards.
        if app.launcher.kanban_all {
            app.chrome.flash = Some("scope: everything".into());
        } else {
            set_board_all(app, true);
        }
    }
    app.chrome.dirty = true;
}

/// The board's breadth, saved as **Kanban scope**, as `Tab` flips it.
fn set_board_all(app: &mut App, all: bool) {
    let mut cfg = crate::config::Config::load();
    cfg.kanban_scope = if all { "all" } else { "project" }.into();
    if super::save_config(app, &cfg) {
        super::apply_config(app, &cfg);
    }
    app.launcher.kanban_all = all;
    app.chrome.flash = Some(
        if all {
            "scope: everything"
        } else {
            "scope: this project"
        }
        .into(),
    );
}

/// `Tab` on the board (kept from before the chip, for the hands that
/// know it): a scope narrowed by FOCUS HERE widens back to everything;
/// otherwise the board flips between the project on screen and every
/// project, saved as **Kanban scope**, as it always did.
pub(crate) fn flip_board_scope(app: &mut App) {
    if !app.launcher.scope.is_everything() {
        app.launcher.scope = Scope::default();
        set_board_all(app, true);
    } else {
        set_board_all(app, !app.launcher.kanban_all);
    }
    app.chrome.dirty = true;
}

/// A tab opened (a digit, `[`/`]`, a click) under a scope on another
/// repo: the scope follows the tab, as "this project" does.
pub(crate) fn scope_follows_tab(app: &mut App, project: &nebula_core::ProjectId) {
    if app
        .launcher
        .scope
        .repo
        .as_ref()
        .is_some_and(|r| r != project)
    {
        app.launcher.scope = Scope {
            repo: Some(project.clone()),
            account: app.launcher.scope.account.clone(),
            ..Scope::default()
        };
    }
}

/// The chip's segments after `all`, as it reads them: the repo, the
/// worktree's branch, the session, the account (`@name`).
pub(crate) fn scope_words(app: &App) -> Vec<String> {
    crate::launcher::scope_segments(app)
        .into_iter()
        .map(|(word, _)| word)
        .collect()
}
