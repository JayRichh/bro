//! The BUTTONS: every clickable affordance that stands for a key — the
//! footer's key hints, the header's tools, the card under the cursor's
//! buttons, a band's `+`, the pane header's diff and menu, the help's rows,
//! a confirm's answers and a prompt's `[Enter]` / `[Esc]` — drawn whole
//! inside the frame at every width, and a click on each doing what its key
//! does.

use super::*;
use crate::app::Focus;
use crate::keymap::Action;
use ratatui::layout::Rect;

/// The widths the buttons are checked at: a narrow split, a laptop, a
/// wide monitor.
const WIDTHS: [u16; 3] = [60, 100, 160];

fn draw_at(app: &mut App, w: u16, h: u16) -> Terminal<TestBackend> {
    let mut terminal = Terminal::new(TestBackend::new(w, h)).unwrap();
    terminal.draw(|f| ui::draw(f, app)).unwrap();
    terminal
}

/// [`seed_tree`] with a second session in its root, the cursor landed on
/// the first card by a click — as a user would — and nothing flashing, so
/// the footer shows its key hints.
fn grid(w: u16) -> App {
    let mut app = App::new();
    seed_tree(&mut app);
    seed_second_agent(&mut app, nebula_core::AgentStatus::Finished);
    draw_at(&mut app, w, 40);
    let card = app
        .chrome
        .hits
        .iter()
        .find(|(_, h)| matches!(h, HitTarget::LauncherCard(_)))
        .map(|(r, _)| *r)
        .expect("a card on the grid");
    click(&mut app, card.x + 2, card.y + 1, &mut Vec::new());
    app.chrome.flash = None;
    draw_at(&mut app, w, 40);
    app
}

fn hits_where(app: &App, keep: impl Fn(&HitTarget) -> bool) -> Vec<(Rect, HitTarget)> {
    app.chrome
        .hits
        .iter()
        .filter(|(_, h)| keep(h))
        .cloned()
        .collect()
}

fn is_button(h: &HitTarget) -> bool {
    matches!(
        h,
        HitTarget::Button(_)
            | HitTarget::FooterHint(_)
            | HitTarget::CardButton(..)
            | HitTarget::CardMenu(_)
            | HitTarget::LauncherBandNew(_)
            | HitTarget::LauncherPaneMenu
            | HitTarget::AccountChip(_)
    )
}

/// Every button lies inside the frame and on no other button.
fn assert_laid_out(app: &App, w: u16, h: u16) {
    let buttons = hits_where(app, is_button);
    for (i, (r, hit)) in buttons.iter().enumerate() {
        assert!(r.width > 0, "{w}: {hit:?} has no width");
        assert!(
            r.x + r.width <= w && r.y < h,
            "{w}: {hit:?} at {r:?} runs off the frame"
        );
        for (other, other_hit) in &buttons[i + 1..] {
            assert!(
                r.intersection(*other).area() == 0,
                "{w}: {hit:?} {r:?} overlaps {other_hit:?} {other:?}"
            );
        }
    }
}

fn center(r: Rect) -> (u16, u16) {
    (r.x + r.width / 2, r.y)
}

fn click_hit(app: &mut App, hit: &HitTarget, out: &mut Vec<ClientRequest>) {
    let rect = app
        .hit_rect(hit)
        .unwrap_or_else(|| panic!("{hit:?} not drawn"));
    let (x, y) = center(rect);
    click(app, x, y, out);
}

#[test]
fn every_button_is_drawn_whole_inside_the_frame_at_every_width() {
    with_default_config(|| {
        for w in WIDTHS {
            let app = grid(w);
            assert_laid_out(&app, w, 40);
            let footer = hits_where(&app, |h| matches!(h, HitTarget::FooterHint(_)));
            // The footer's hints are buttons where the bar has room for them,
            // on its last row; 60 columns of context leave it none.
            assert!(w < 100 || !footer.is_empty(), "{w}: no footer buttons");
            assert!(footer.iter().all(|(r, _)| r.y == 39), "{w}: {footer:?}");
            // Help is always a click away — the header's `?` or the footer's
            // — and every other action is a click on one of its rows.
            assert!(
                app.hit_rect(&HitTarget::Button(Action::Help)).is_some()
                    || app.hit_rect(&HitTarget::FooterHint(Action::Help)).is_some(),
                "{w}: no way to the help"
            );
            // The card under the cursor carries its buttons, the `⋯` always.
            assert!(
                !hits_where(&app, |h| matches!(h, HitTarget::CardMenu(_))).is_empty(),
                "{w}: no card menu"
            );
        }
        // Wide: the header's tools and every card verb.
        let app = grid(160);
        for hit in [
            HitTarget::Button(Action::Settings),
            HitTarget::Button(Action::Help),
            HitTarget::FooterHint(Action::QuickPrompt),
        ] {
            assert!(app.hit_rect(&hit).is_some(), "{hit:?} missing at 160");
        }
        let verbs: Vec<Action> = hits_where(&app, |h| matches!(h, HitTarget::CardButton(..)))
            .into_iter()
            .filter_map(|(_, h)| match h {
                HitTarget::CardButton(_, a) => Some(a),
                _ => None,
            })
            .collect();
        for verb in [
            Action::FollowUp,
            Action::GitDiff,
            Action::Archive,
            Action::Delete,
        ] {
            assert!(verbs.contains(&verb), "{verb:?} not on the card: {verbs:?}");
        }
    });
}

/// A footer hint drawn only in part is not a button: every one registered
/// ends inside the bar, short of the usage readout.
#[test]
fn a_footer_hint_the_bar_has_no_room_for_is_no_button() {
    with_default_config(|| {
        let app = grid(60);
        let all = launcher_hint_count(&app);
        let drawn = hits_where(&app, |h| matches!(h, HitTarget::FooterHint(_))).len();
        assert!(drawn < all, "60 columns hold every one of {all} hints?");
        let usage = app.hit_rect(&HitTarget::FooterUsage);
        for (r, hit) in hits_where(&app, |h| matches!(h, HitTarget::FooterHint(_))) {
            if let Some(u) = usage {
                assert!(r.x + r.width <= u.x, "{hit:?} under the readout");
            }
        }
    });
}

/// How many of the grid's footer entries are buttons at a width that
/// holds them all.
fn launcher_hint_count(_: &App) -> usize {
    let app = grid(400);
    hits_where(&app, |h| matches!(h, HitTarget::FooterHint(_))).len()
}

/// INPUT PARITY: a footer hint, a header tool and a help row each do what
/// their key does.
#[test]
fn a_footer_hint_and_a_header_tool_run_their_action() {
    with_default_config(|| {
        let mut app = grid(160);
        click_hit(
            &mut app,
            &HitTarget::FooterHint(Action::QuickPrompt),
            &mut Vec::new(),
        );
        assert!(
            matches!(&app.modals.overlay, Some(Overlay::Prompt(p)) if matches!(p.kind, PromptKind::QuickPrompt(_)))
        );
        // A bar wide enough for every hint: the jump, the help, the diff.
        let mut app = grid(400);
        assert!(app.hit_rect(&HitTarget::FooterHint(Action::Help)).is_some());
        assert!(app
            .hit_rect(&HitTarget::FooterHint(Action::GitDiff))
            .is_some());
        click_hit(
            &mut app,
            &HitTarget::FooterHint(Action::Palette),
            &mut Vec::new(),
        );
        assert!(matches!(app.modals.overlay, Some(Overlay::Palette(_))));

        let mut app = grid(160);
        click_hit(&mut app, &HitTarget::Button(Action::Help), &mut Vec::new());
        assert!(matches!(app.modals.overlay, Some(Overlay::Help(_))));

        let mut app = grid(160);
        click_hit(
            &mut app,
            &HitTarget::Button(Action::Settings),
            &mut Vec::new(),
        );
        assert!(matches!(app.modals.overlay, Some(Overlay::Settings(_))));
    });
}

/// The pointer on a header button names its key in the footer — the
/// TUI's tooltip — and a footer hint is left as it is.
#[test]
fn hovering_a_button_names_its_key_in_the_footer() {
    with_default_config(|| {
        let mut app = grid(160);
        let (x, y) = center(app.hit_rect(&HitTarget::Button(Action::Settings)).unwrap());
        handle_mouse(&mut app, mev(MouseEventKind::Moved, x, y), &mut Vec::new());
        assert_eq!(
            app.launcher.hover_crumb,
            Some(HitTarget::Button(Action::Settings))
        );
        let term = draw_at(&mut app, 160, 40);
        let footer: String = (0..160)
            .map(|x| term.backend().buffer()[(x, 39)].symbol().to_string())
            .collect();
        let key = crate::ui::key_hint(&app, Action::Settings);
        assert!(footer.contains(&format!("{key}: Settings")), "{footer:?}");
    });
}

/// The card's `archive` is `a`: the same confirm, from the cursor's card —
/// and from a locked pane too, which gives the keys back first.
#[test]
fn a_card_button_is_its_key_even_from_a_locked_pane() {
    with_default_config(|| {
        let by_key = {
            let mut app = grid(160);
            press(
                &mut app,
                KeyCode::Char('a'),
                KeyModifiers::NONE,
                &mut Vec::new(),
            );
            app.modals.overlay.clone()
        };
        let confirm_title = |o: &Option<Overlay>| match o {
            Some(Overlay::Confirm(c)) => Some(c.title.clone()),
            _ => None,
        };
        assert!(confirm_title(&by_key).is_some(), "a asks first");

        let mut app = grid(160);
        app.nav.focus = Focus::Terminal;
        app.pane.term_locked = true;
        let archive = hits_where(&app, |h| {
            matches!(h, HitTarget::CardButton(_, Action::Archive))
        })[0]
            .clone();
        let (x, y) = center(archive.0);
        click(&mut app, x, y, &mut Vec::new());
        assert_eq!(confirm_title(&app.modals.overlay), confirm_title(&by_key));
        assert_eq!(app.nav.focus, Focus::Sessions, "the keys are the cards'");
        assert!(!app.pane.term_locked);
    });
}

/// The card's `⋯` opens the menu a right-click on it opens.
#[test]
fn the_card_menu_button_opens_the_right_click_menu() {
    with_default_config(|| {
        let labels = |app: &App| match &app.modals.overlay {
            Some(Overlay::Menu(m)) => m.items.iter().map(|i| i.label.clone()).collect(),
            _ => Vec::<String>::new(),
        };
        let mut by_right = grid(160);
        let card = by_right
            .hit_rect(&HitTarget::LauncherCard(crate::launcher::CardRef {
                band: 0,
                card: 0,
            }))
            .unwrap();
        handle_mouse(
            &mut by_right,
            mev(
                MouseEventKind::Down(MouseButton::Right),
                card.x + 2,
                card.y + 1,
            ),
            &mut Vec::new(),
        );
        let mut by_button = grid(160);
        let menu = hits_where(&by_button, |h| matches!(h, HitTarget::CardMenu(_)))[0].0;
        let (x, y) = center(menu);
        click(&mut by_button, x, y, &mut Vec::new());
        assert!(!labels(&by_button).is_empty());
        assert_eq!(labels(&by_button), labels(&by_right));
    });
}

/// A band's `+` is `p` on that band: the quick prompt.
#[test]
fn a_bands_plus_opens_the_quick_prompt() {
    with_default_config(|| {
        let mut app = grid(160);
        click_hit(&mut app, &HitTarget::LauncherBandNew(0), &mut Vec::new());
        assert!(
            matches!(&app.modals.overlay, Some(Overlay::Prompt(p)) if matches!(p.kind, PromptKind::QuickPrompt(_))),
            "{:?}",
            app.modals.overlay.as_ref().map(std::mem::discriminant)
        );
    });
}

/// The pane header's `⋯` is the card's menu; its buttons drop on a
/// narrow pane before the full-screen and close buttons do.
#[test]
fn the_pane_header_has_a_menu_and_gives_its_extras_up_first() {
    with_default_config(|| {
        let mut app = grid(160);
        assert!(
            app.hit_rect(&HitTarget::LauncherPaneMenu).is_some(),
            "a pane reading a card has its menu"
        );
        assert!(app.hit_rect(&HitTarget::Button(Action::GitDiff)).is_some());
        click_hit(&mut app, &HitTarget::LauncherPaneMenu, &mut Vec::new());
        assert!(matches!(app.modals.overlay, Some(Overlay::Menu(_))));

        for w in WIDTHS {
            let app = grid(w);
            if app.hit_rect(&HitTarget::Button(Action::GitDiff)).is_some() {
                assert!(app.hit_rect(&HitTarget::LauncherPaneZoom).is_some(), "{w}");
                assert!(app.hit_rect(&HitTarget::LauncherPaneClose).is_some(), "{w}");
            }
        }
    });
}

/// The pane header's account chip — what the session runs on — is MOVE
/// TO ACCOUNT: a click lists the other accounts of its harness, a pick
/// asks first (fresh, same worktree), and yes sends the switch.
#[test]
fn the_pane_headers_account_chip_moves_the_session_to_another_account() {
    with_config_json(
        r#"{"profiles":[{"name":"work","harness":"claude"}]}"#,
        || {
            let mut app = grid(160);
            // The pane reads the card under the cursor.
            let chip = hits_where(&app, |h| matches!(h, HitTarget::AccountChip(_)))
                .first()
                .map(|(_, h)| h.clone())
                .expect("the chip is drawn");
            let mut out = Vec::new();
            click_hit(&mut app, &chip, &mut out);
            let Some(Overlay::Menu(menu)) = &app.modals.overlay else {
                panic!("{:?}", app.modals.overlay);
            };
            assert_eq!(menu.items.len(), 1, "{:?}", menu.items);
            assert_eq!(menu.items[0].label, "Restart on work…");
            press(&mut app, KeyCode::Enter, KeyModifiers::NONE, &mut out);
            let Some(Overlay::Confirm(confirm)) = &app.modals.overlay else {
                panic!("{:?}", app.modals.overlay);
            };
            assert!(
                confirm.message.contains("same worktree"),
                "{}",
                confirm.message
            );
            assert!(out.is_empty(), "nothing before the yes");
            press(&mut app, KeyCode::Enter, KeyModifiers::NONE, &mut out);
            assert!(
                matches!(
                    out.as_slice(),
                    [ClientRequest::SwitchAgentAccount { profile: Some(p), .. }] if p == "work"
                ),
                "{out:?}"
            );
        },
    );
}

/// A help row is a button: the help closes and the action runs.
#[test]
fn a_help_row_runs_its_action() {
    with_default_config(|| {
        let mut app = grid(160);
        press(
            &mut app,
            KeyCode::Char('?'),
            KeyModifiers::NONE,
            &mut Vec::new(),
        );
        drawn_modal_area(&mut app);
        let palette = match &app.modals.overlay {
            Some(Overlay::Help(h)) => h
                .buttons
                .iter()
                .find(|(_, a)| *a == Action::Palette)
                .map(|(r, _)| *r)
                .expect("a row for the palette"),
            _ => panic!("help is open"),
        };
        click(&mut app, palette.x + 1, palette.y, &mut Vec::new());
        assert!(matches!(app.modals.overlay, Some(Overlay::Palette(_))));
    });
}

/// A confirm's answers are its keys: `cancel` is Esc, `confirm` Enter.
#[test]
fn a_confirms_answers_are_buttons() {
    with_default_config(|| {
        let open = || {
            let mut app = grid(160);
            press(
                &mut app,
                KeyCode::Delete,
                KeyModifiers::NONE,
                &mut Vec::new(),
            );
            drawn_modal_area(&mut app);
            app
        };
        let answers = |app: &App| match &app.modals.overlay {
            Some(Overlay::Confirm(c)) => c.answer_rects(),
            _ => panic!("the delete confirm is up"),
        };
        let mut app = open();
        let (cancel, _) = answers(&app)
            .into_iter()
            .find(|(_, a)| a.key == KeyCode::Esc)
            .unwrap();
        click(&mut app, cancel.x + 1, cancel.y, &mut Vec::new());
        assert!(app.modals.overlay.is_none(), "cancel closes it");

        let mut by_key = open();
        let mut key_out = Vec::new();
        press(
            &mut by_key,
            KeyCode::Enter,
            KeyModifiers::NONE,
            &mut key_out,
        );
        let mut app = open();
        let (yes, _) = answers(&app)
            .into_iter()
            .find(|(_, a)| a.key == KeyCode::Enter)
            .unwrap();
        let mut out = Vec::new();
        click(&mut app, yes.x + 1, yes.y, &mut out);
        assert_eq!(format!("{out:?}"), format!("{key_out:?}"));
        assert_eq!(
            app.modals.overlay.is_none(),
            by_key.modals.overlay.is_none()
        );
    });
}

/// A forced delete's one destructive answer is drawn last, and the
/// second click of a double-click never answers a dialog the first put
/// up.
#[test]
fn a_double_click_never_answers_the_dialog_it_put_up() {
    with_default_config(|| {
        let mut app = grid(160);
        app.modals.overlay = Some(Overlay::Confirm(ConfirmDialog {
            title: "Delete anyway?".into(),
            message: "feat has uncommitted changes".into(),
            action: PendingAction::ForceDeleteWorktree(nebula_core::WorktreeId("w1".into())),
            area: ratatui::layout::Rect::default(),
        }));
        drawn_modal_area(&mut app);
        let answers = match &app.modals.overlay {
            Some(Overlay::Confirm(c)) => c.answer_rects(),
            _ => unreachable!(),
        };
        assert!(answers.last().is_some_and(|(_, a)| a.destructive));
        assert!(answers.first().is_some_and(|(_, a)| !a.destructive));
        // The first click of the pair answered the dialog before this one.
        app.modals.button_clicked_at = Some(std::time::Instant::now());
        let (force, _) = *answers.last().unwrap();
        let mut out = Vec::new();
        click(&mut app, force.x + 1, force.y, &mut out);
        assert!(out.is_empty(), "nothing sent: {out:?}");
        assert!(
            matches!(app.modals.overlay, Some(Overlay::Confirm(_))),
            "the dialog waits for a deliberate answer"
        );
    });
}

/// A plain one-line prompt — a rename — has its two buttons too, in the
/// narrower box it is drawn in.
#[test]
fn a_rename_prompt_has_its_buttons() {
    with_default_config(|| {
        let mut app = App::new();
        seed_tree(&mut app);
        open_prompt(
            &mut app,
            PromptKind::RenameAgent {
                id: AgentId("a1".into()),
            },
        );
        drawn_modal_area(&mut app);
        let rects = match &app.modals.overlay {
            Some(Overlay::Prompt(p)) => p.answer_rects(),
            _ => panic!("the rename prompt is up"),
        };
        assert_eq!(rects.len(), 2, "[Enter] ok and [Esc] cancel");
        let esc = rects.iter().find(|(_, k)| *k == KeyCode::Esc).unwrap().0;
        click(&mut app, esc.x + 1, esc.y, &mut Vec::new());
        assert!(app.modals.overlay.is_none(), "cancel closes it");
    });
}

/// A one-line prompt's `[Esc] cancel` closes it and `[Enter]` is Enter.
#[test]
fn a_prompts_enter_and_esc_are_buttons() {
    with_default_config(|| {
        let open = || {
            let mut app = App::new();
            open_prompt(&mut app, PromptKind::AddProject);
            drawn_modal_area(&mut app);
            app
        };
        let rects = |app: &App| match &app.modals.overlay {
            Some(Overlay::Prompt(p)) => p.answer_rects(),
            _ => panic!("the prompt is up"),
        };
        let mut app = open();
        let esc = rects(&app)
            .into_iter()
            .find(|(_, k)| *k == KeyCode::Esc)
            .unwrap()
            .0;
        click(&mut app, esc.x + 1, esc.y, &mut Vec::new());
        assert!(app.modals.overlay.is_none(), "cancel closes it");

        let mut app = open();
        let enter = rects(&app)
            .into_iter()
            .find(|(_, k)| *k == KeyCode::Enter)
            .unwrap()
            .0;
        let mut by_key = open();
        let mut key_out = Vec::new();
        press(
            &mut by_key,
            KeyCode::Enter,
            KeyModifiers::NONE,
            &mut key_out,
        );
        let mut out = Vec::new();
        click(&mut app, enter.x + 1, enter.y, &mut out);
        assert_eq!(format!("{out:?}"), format!("{key_out:?}"));
        assert_eq!(
            app.modals.overlay.is_some(),
            by_key.modals.overlay.is_some()
        );
    });
}
