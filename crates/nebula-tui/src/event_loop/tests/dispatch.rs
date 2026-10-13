//! Every default binding, pressed through the real key path: no action in
//! the catalogue may fall through to an `unreachable!` (F10 used to).

use super::*;
use crate::app::Focus;
use crossterm::event::{KeyEvent, KeyModifiers};

/// Actions whose default chord, on a seeded tree, would reach outside the
/// test: a browser, a terminal emulator, the project's open command.
fn launches_something(action: crate::keymap::Action) -> bool {
    use crate::keymap::Action;
    matches!(
        action,
        Action::OpenRepo
            | Action::OpenInBrowser
            | Action::Screenshot
            | Action::OpenGhosttyTab
            | Action::OpenWorktree
            | Action::OpenPullRequest
            | Action::OpenIssue
    )
}

/// Press every default chord of every action in a fresh `App` each time,
/// and name every one that panicked.
fn press_every_default(seed: bool, focus: Option<Focus>) -> Vec<String> {
    let mut failed = Vec::new();
    for spec in crate::keymap::ACTIONS {
        if seed && launches_something(spec.action) {
            continue;
        }
        for raw in spec.defaults {
            let Some(chord) = crate::keymap::KeyChord::parse(raw) else {
                continue;
            };
            let pressed = with_default_config(|| {
                std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    let mut app = App::new();
                    if seed {
                        seed_tree(&mut app);
                    }
                    if let Some(focus) = focus {
                        app.nav.focus = focus;
                    }
                    let mut out = Vec::new();
                    handle_key(&mut app, KeyEvent::new(chord.code, chord.mods), &mut out);
                }))
            });
            if pressed.is_err() {
                failed.push(format!("{} ({raw}) focus={focus:?} seed={seed}", spec.id));
            }
        }
    }
    failed
}

#[test]
fn every_default_binding_dispatches_without_panicking_on_a_fresh_app() {
    let failed = press_every_default(false, None);
    assert!(failed.is_empty(), "panicked: {failed:#?}");
}

#[test]
fn every_default_binding_dispatches_without_panicking_on_a_seeded_app() {
    let mut failed = press_every_default(true, None);
    for focus in [
        Focus::Projects,
        Focus::Worktrees,
        Focus::Sessions,
        Focus::Terminal,
    ] {
        failed.extend(press_every_default(true, Some(focus)));
    }
    assert!(failed.is_empty(), "panicked: {failed:#?}");
}

#[test]
fn f10_never_panics() {
    with_default_config(|| {
        let mut app = App::new();
        seed_tree(&mut app);
        let mut out = Vec::new();
        handle_key(
            &mut app,
            KeyEvent::new(KeyCode::F(10), KeyModifiers::NONE),
            &mut out,
        );
    });
}
