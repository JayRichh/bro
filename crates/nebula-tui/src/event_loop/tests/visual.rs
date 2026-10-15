//! The 0.48-0.50 screens drawn at the two sizes people run the terminal
//! app at, 80x24 and 120x40: the board with a scope and its hidden counts,
//! the docked review, the ship composer, the merge confirm, the verbs
//! menu, the accounts overlay and the Hotkeys tab. Each names what must
//! stay whole at that size (a title, a key, a count) and that nothing
//! runs past the frame it is drawn in. `KETE_DUMP=1` prints the screens.

use super::board::{agent, bench, bench_at, feat_repo, select_card, with_open_pr};
use super::*;
use crate::keymap::Action;
use nebula_core::{AgentStatus, WorktreeId};

const SIZES: [(u16, u16); 2] = [(80, 24), (120, 40)];

fn draw(app: &mut App, w: u16, h: u16, what: &str) -> Vec<String> {
    let mut terminal = Terminal::new(TestBackend::new(w, h)).unwrap();
    terminal.draw(|f| ui::draw(f, app)).unwrap();
    let text = buffer_text(&terminal);
    if std::env::var_os("KETE_DUMP").is_some() {
        eprintln!("---- {what} at {w}x{h} ----\n{text}");
    }
    let lines: Vec<String> = text.lines().map(str::to_string).collect();
    // The status bar drops a hint whole rather than ending on `Enter:`.
    let bar = lines.last().map_or("", |l| l.trim_end());
    assert!(
        !bar.ends_with(':'),
        "a hint cut at the bar's edge, {what} at {w}x{h}: {bar}"
    );
    lines
}

/// `needle` is on one row, whole.
fn shows(lines: &[String], needle: &str) -> bool {
    lines.iter().any(|l| l.contains(needle))
}

/// A box drawn with `╭…╮` / `╰…╯` (or square corners: the overlays'
/// frames): every row
/// between its corners has its right edge where the corner put it, so no
/// text ran over the frame.
fn frames_close(lines: &[String]) -> bool {
    let cols = |l: &str| l.chars().collect::<Vec<char>>();
    for (top, line) in lines.iter().enumerate() {
        let row = cols(line);
        let Some(left) = row.iter().position(|&c| c == '╭' || c == '┌') else {
            continue;
        };
        let Some(right) = row[left..]
            .iter()
            .position(|&c| c == '╮' || c == '┐')
            .map(|r| r + left)
        else {
            continue;
        };
        for below in &lines[top + 1..] {
            let r = cols(below);
            match r.get(left) {
                Some('╰') | Some('└') => break,
                Some('│') | Some('├') => {
                    if !matches!(r.get(right), Some('│') | Some('┤')) {
                        return false;
                    }
                }
                _ => break,
            }
        }
    }
    true
}

fn scoped_board() -> App {
    let mut app = bench();
    // A running session on the root, outside the scope to come.
    seed_agent_in(&mut app, "a3", &WorktreeId("w1".into()));
    agent(&mut app, "a3").status = AgentStatus::Running;
    agent(&mut app, "a2").profile = Some("work".into());
    app.launcher.scope = nebula_core::scope::Scope {
        repo: Some(nebula_core::ProjectId("p1".into())),
        worktree: Some(WorktreeId("w2".into())),
        ..Default::default()
    };
    app
}

/// The board with a scope: the chip says where, its ✕ stays in view, and
/// each lane's foot counts what the scope leaves out.
#[test]
fn the_scoped_board_draws_whole() {
    with_default_config(|| {
        for (w, h) in SIZES {
            let mut app = scoped_board();
            let lines = draw(&mut app, w, h, "scoped board");
            assert!(shows(&lines, "✕"), "the chip's clear at {w}x{h}");
            assert!(
                shows(&lines, "feat"),
                "the chip names the worktree at {w}x{h}"
            );
            // The count whole, however narrow the lane: never `1 more outsid…`.
            assert!(
                shows(&lines, "1 more outside scope") || shows(&lines, "+1 outside"),
                "the hidden count at {w}x{h}"
            );
            assert!(
                !lines.iter().any(|l| l.contains("outsid…")),
                "a cut count at {w}x{h}"
            );
            assert!(shows(&lines, "polish-"), "the card at {w}x{h}");
        }
    });
}

/// The review unit docked beside the board: its verb row keeps Reply and
/// the PR verb, and the lanes stay beside it.
#[test]
fn the_docked_review_draws_whole() {
    with_default_config(|| {
        for (w, h) in SIZES {
            let dir = tempfile::tempdir().unwrap();
            let mut app = bench_at(feat_repo(&dir));
            note_worktree_changes(&mut app, WorktreeId("w2".into()), Some(2));
            draw(&mut app, w, h, "board");
            select_card(&mut app, "a2");
            press(
                &mut app,
                KeyCode::Enter,
                KeyModifiers::NONE,
                &mut Vec::new(),
            );
            assert!(matches!(&app.modals.overlay, Some(Overlay::Diff(v)) if v.review.is_some()));
            let lines = draw(&mut app, w, h, "docked review");
            assert!(shows(&lines, "reply"), "Reply on the verb row at {w}x{h}");
            assert!(shows(&lines, "PR"), "the PR verb at {w}x{h}");
            // Beside the dock, a lane shows a card's name, or the dock
            // takes the board: never lanes of `● p…`.
            assert!(
                !shows(&lines, "● p…"),
                "lanes too narrow to read at {w}x{h}"
            );
            assert!(frames_close(&lines), "a frame overrun at {w}x{h}");
        }
    });
}

/// The ship composer: its title, the fields and the send key, inside its
/// frame; and its sign-in error, whole.
#[test]
fn the_ship_composer_draws_whole() {
    with_default_config(|| {
        for (w, h) in SIZES {
            let mut app = bench();
            super::super::ship::open_composer(&mut app, &WorktreeId("w2".into()), None, None);
            let lines = draw(&mut app, w, h, "ship composer");
            assert!(shows(&lines, "Pull request: feat"), "the title at {w}x{h}");
            assert!(shows(&lines, "Title"), "the title field at {w}x{h}");
            assert!(frames_close(&lines), "a frame overrun at {w}x{h}");
            if let Some(Overlay::Ship(v)) = &mut app.modals.overlay {
                v.stage = crate::ship_view::Stage::Failed(kete_ado::ship::ShipError {
                    kind: kete_ado::ship::ShipErrorKind::SignIn,
                    forge: Some(kete_ado::ship::ForgeKind::GitHub),
                    message: kete_ado::ship::GH_SIGN_IN.into(),
                    url: None,
                });
            }
            let lines = draw(&mut app, w, h, "ship composer, signed out");
            assert!(shows(&lines, "gh auth login"), "the fix at {w}x{h}");
            assert!(frames_close(&lines), "a frame overrun at {w}x{h}");
        }
    });
}

/// The merge confirm: the number, the methods and the branch choice.
#[test]
fn the_merge_confirm_draws_whole() {
    with_default_config(|| {
        for (w, h) in SIZES {
            let mut app = bench();
            agent(&mut app, "a2").unseen = false;
            with_open_pr(&mut app);
            super::super::ship::open_merge(&mut app, &WorktreeId("w2".into()), None, None);
            let lines = draw(&mut app, w, h, "merge confirm");
            assert!(shows(&lines, "Merge #12: feat"), "the title at {w}x{h}");
            assert!(shows(&lines, "squash"), "the methods at {w}x{h}");
            assert!(shows(&lines, "[Enter] Merge"), "the send key at {w}x{h}");
            assert!(frames_close(&lines), "a frame overrun at {w}x{h}");
        }
    });
}

/// `?` on a card: the verbs menu with its filter line and its keys, and
/// All keys last, in view.
#[test]
fn the_verbs_menu_draws_whole() {
    with_default_config(|| {
        for (w, h) in SIZES {
            let mut app = bench();
            draw(&mut app, w, h, "board");
            select_card(&mut app, "a2");
            run_action(&mut app, Action::Verbs, &mut Vec::new());
            assert!(matches!(app.modals.overlay, Some(Overlay::Menu(_))));
            let lines = draw(&mut app, w, h, "verbs menu");
            assert!(
                shows(&lines, "polish-nav"),
                "the menu names the card at {w}x{h}"
            );
            assert!(shows(&lines, "Archive"), "a verb at {w}x{h}");
            assert!(frames_close(&lines), "a frame overrun at {w}x{h}");
        }
    });
}

/// The accounts overlay: its title and every login's name and state.
#[test]
fn the_accounts_overlay_draws_whole() {
    with_default_config(|| {
        for (w, h) in SIZES {
            let mut app = bench();
            accounts::open(&mut app);
            let lines = draw(&mut app, w, h, "accounts");
            assert!(shows(&lines, "Accounts"), "the title at {w}x{h}");
            assert!(shows(&lines, "claude"), "a login at {w}x{h}");
            // The spread's words whole, or ended on an ellipsis, not cut
            // at the frame.
            assert!(
                shows(&lines, "when none is starred") || shows(&lines, "…│"),
                "the spread's line cut at {w}x{h}"
            );
            assert!(frames_close(&lines), "a frame overrun at {w}x{h}");
        }
    });
}

/// Settings, Hotkeys: the tab strip keeps Hotkeys, a group and a row
/// with its chord.
#[test]
fn the_hotkeys_tab_draws_whole() {
    with_default_config(|| {
        for (w, h) in SIZES {
            let mut app = bench();
            open_settings_on(&mut app, crate::config::hotkeys_tab(), &mut Vec::new());
            let lines = draw(&mut app, w, h, "hotkeys");
            assert!(shows(&lines, "Hotkeys"), "the tab at {w}x{h}");
            assert!(shows(&lines, "NAVIGATE"), "a group at {w}x{h}");
            assert!(frames_close(&lines), "a frame overrun at {w}x{h}");
        }
    });
}
