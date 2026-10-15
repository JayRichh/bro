//! The REVIEW UNIT: Enter on a Review card of the board docks the git
//! diff of its worktree, read against the base (the merge-base with
//! **Worktree base branch**, else the default branch), beside the board —
//! the board on the left, the diff on the right — with a verb row under
//! it: Space replies to the session, then the pull request (opened, or
//! the composer for one), its merge, the archive and the worktree's
//! delete. Closing it (Esc, a click on the board, `⇧K`) lands the cursor
//! back on the card it was opened from, by the card's id.
//!
//! The dock is the ordinary DIFF VIEWER (`Overlay::Diff`) with a
//! [`ReviewDock`] on it: every key of the diff works in it, and its bare
//! letters still feed the file filter, so the verbs are Space (with the
//! filter empty) and `⌥` chords.

use super::*;
use crate::app::{ReviewDock, ReviewVerb};
use nebula_core::board::Lane;

/// The verbs in the order the row lists them, with the key each answers
/// to and its word.
pub(crate) const VERBS: [(ReviewVerb, &str, &str); 5] = [
    (ReviewVerb::Reply, "Space", "reply"),
    (ReviewVerb::Pr, "\u{2325}p", "PR"),
    (ReviewVerb::Merge, "\u{2325}m", "merge"),
    (ReviewVerb::Archive, "\u{2325}a", "archive"),
    (ReviewVerb::DeleteWorktree, "\u{2325}w", "delete worktree"),
];

/// The PR verb's word: `view PR` with one open, `open PR` without.
pub(crate) fn pr_word(app: &App, worktree: &WorktreeId) -> &'static str {
    if crate::launcher::worktree_pr(app, worktree).is_some_and(|pr| {
        matches!(
            pr.standing,
            crate::pull_request::Standing::Open | crate::pull_request::Standing::Draft
        )
    }) {
        "view PR"
    } else {
        "open PR"
    }
}

/// Is the card under the board's cursor in Review?
pub(super) fn on_review_card(app: &App) -> bool {
    if !(app.launcher.launcher_kanban && app.launcher_active()) || app.launcher.launcher_unaimed {
        return false;
    }
    let Some(a) = app.selected_session() else {
        return false;
    };
    crate::launcher::row(app, &a.id)
        .is_some_and(|row| crate::launcher::lane_of(app, &row) == Lane::Review)
}

/// Enter on a Review card: its worktree's diff against the base, docked.
pub(crate) fn open(app: &mut App, out: &mut Vec<ClientRequest>) {
    let Some(a) = app.selected_session() else {
        return;
    };
    let (agent, worktree) = (a.id.clone(), a.worktree_id.clone());
    // Always against the base, whatever the diff was last left on: the
    // review reads what the branch did since it forked.
    let remembered = app.modals.diff_against;
    app.modals.diff_against = Some(nebula_core::review_base::Against::Base);
    open_diff_view(app);
    app.modals.diff_against = remembered;
    match &mut app.modals.overlay {
        Some(Overlay::Diff(view)) => {
            view.review = Some(Box::new(ReviewDock {
                agent: agent.clone(),
                worktree,
                verbs: Vec::new(),
                dock: ratatui::layout::Rect::default(),
            }));
            view.from_card = Some(agent.clone());
        }
        _ => {
            if app.chrome.flash.is_none() {
                app.chrome.flash = Some("nothing to review in this worktree".into());
            }
            app.chrome.dirty = true;
            return;
        }
    }
    // Its snapshots, for the "this session" filter: only a 59+ daemon
    // takes the request (None, no handshake yet, is a test).
    if app
        .orch
        .daemon_protocol
        .is_none_or(|v| v >= nebula_core::SESSION_CHANGES_PROTOCOL)
    {
        let req_id = app.alloc_req_id(crate::app::PendingIntent::None);
        out.push(ClientRequest::SessionChanges { req_id, agent });
    }
    app.chrome.dirty = true;
}

/// The verb a key on the docked diff is, if it is one.
pub(super) fn verb_of(view: &crate::app::DiffView, key: &KeyEvent) -> Option<ReviewVerb> {
    view.review.as_ref()?;
    let alt = key.modifiers.contains(KeyModifiers::ALT);
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
    match key.code {
        KeyCode::Char(' ') if view.filter.is_empty() && !alt && !ctrl => Some(ReviewVerb::Reply),
        KeyCode::Char('p') if alt => Some(ReviewVerb::Pr),
        KeyCode::Char('m') if alt => Some(ReviewVerb::Merge),
        KeyCode::Char('a') if alt => Some(ReviewVerb::Archive),
        KeyCode::Char('w') if alt => Some(ReviewVerb::DeleteWorktree),
        _ => None,
    }
}

/// Close a diff opened from a board card, onto that card.
pub(crate) fn close_diff(app: &mut App, out: &mut Vec<ClientRequest>) {
    let Some(Overlay::Diff(view)) = app.modals.overlay.take() else {
        return;
    };
    super::board::return_to_card(app, view.from_card.as_ref(), out);
    app.chrome.dirty = true;
}

/// Run a verb of the REVIEW UNIT on its card.
pub(crate) fn run_verb(app: &mut App, verb: ReviewVerb, out: &mut Vec<ClientRequest>) {
    let Some(Overlay::Diff(view)) = &app.modals.overlay else {
        return;
    };
    let Some(dock) = view.review.clone() else {
        return;
    };
    app.chrome.dirty = true;
    match verb {
        ReviewVerb::Reply => {
            close_diff(app, out);
            launcher::follow_up(app);
        }
        ReviewVerb::Pr => match crate::launcher::worktree_pr(app, &dock.worktree) {
            Some(pr)
                if matches!(
                    pr.standing,
                    crate::pull_request::Standing::Open | crate::pull_request::Standing::Draft
                ) =>
            {
                open_link(app, &pr.url, out);
            }
            _ => {
                let Some(Overlay::Diff(back)) = app.modals.overlay.take() else {
                    return;
                };
                super::ship::open_composer(
                    app,
                    &dock.worktree,
                    Some(dock.agent.clone()),
                    Some(Box::new(back.clone())),
                );
                // Refused before any form opened: the review stays.
                if app.modals.overlay.is_none() {
                    app.modals.overlay = Some(Overlay::Diff(back));
                }
            }
        },
        ReviewVerb::Merge => {
            let Some(Overlay::Diff(back)) = app.modals.overlay.take() else {
                return;
            };
            super::ship::open_merge(
                app,
                &dock.worktree,
                Some(dock.agent.clone()),
                Some(Box::new(back.clone())),
            );
            if app.modals.overlay.is_none() {
                app.modals.overlay = Some(Overlay::Diff(back));
            }
        }
        ReviewVerb::Archive => {
            app.modals.overlay = None;
            super::archive_agent(app, dock.agent, out);
        }
        ReviewVerb::DeleteWorktree => {
            let main = app
                .tree
                .worktrees
                .iter()
                .any(|w| w.id == dock.worktree && w.is_main);
            if main {
                app.chrome.flash =
                    Some("the project's own checkout is never deleted from here".into());
                return;
            }
            app.modals.overlay = None;
            super::activate::delete_worktree(app, &dock.worktree);
        }
    }
}

/// A click while the REVIEW UNIT is up: a verb on its row runs; a click
/// outside the dock (on the board beside it) closes it onto its card and
/// goes on to whatever it landed on. True when the click was the dock's.
pub(super) fn handle_click(
    app: &mut App,
    mouse: MouseEvent,
    pos: ratatui::layout::Position,
    out: &mut Vec<ClientRequest>,
) -> Option<bool> {
    let Some(Overlay::Diff(view)) = &app.modals.overlay else {
        return None;
    };
    let dock = view.review.as_ref()?;
    if !matches!(mouse.kind, MouseEventKind::Down(MouseButton::Left)) {
        return None;
    }
    if let Some((_, verb)) = dock.verbs.iter().find(|(r, _)| r.contains(pos)) {
        let verb = *verb;
        run_verb(app, verb, out);
        return Some(true);
    }
    if view.area.contains(pos) {
        return None;
    }
    close_diff(app, out);
    Some(false)
}
