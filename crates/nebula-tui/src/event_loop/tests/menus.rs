//! The menus a right-click opens (lane/tui-menus): the menu of the card,
//! band, background or file row under the pointer, folded into submenus,
//! hovered and chosen by the left button alone, and the verbs behind the
//! rows (copy, mark seen, archive done).

use super::*;
use crate::accounts_overlay::{AccountRow, AccountsHit, AccountsView};
use crate::event_loop::verbs::{self, Target};
use crate::keymap::Action;
use nebula_config::actions::{AppliesTo, MenuGroup, MENU_GROUPS};
use nebula_core::accounts::LoginState;
use nebula_core::{
    AgentId, AgentStatus, Entity, ProjectId, TerminalId, TerminalTab, Worktree, WorktreeId,
};
use ratatui::layout::{Position, Rect};

// ---- the benches ----

/// A linked worktree `feat` of `demo` with a session of its own, `a3`.
fn add_feat(app: &mut App) {
    hse(
        app,
        ServerEvent::EntityUpserted {
            entity: Entity::Worktree(Worktree {
                id: WorktreeId("w2".into()),
                project_id: ProjectId("p1".into()),
                path: "/tmp/demo-feat".into(),
                branch: "feat".into(),
                is_main: false,
                sort_order: 1,
            }),
        },
    );
    seed_agent_in(app, "a3", &WorktreeId("w2".into()));
}

/// The two bands of a grid, with nothing to crowd a card off the screen:
/// `main` with agent-1 and `feat` with a3.
fn grid_app() -> App {
    let mut app = App::new();
    seed_tree(&mut app);
    add_feat(&mut app);
    app
}

/// [`grid_app`] and what a full menu has to choose from: an archived
/// session and a Cloud one in the root, a terminal there, and an issue on
/// agent-1.
fn bench() -> App {
    let mut app = grid_app();
    seed_agent_in(&mut app, "arch", &WorktreeId("w1".into()));
    seed_agent_in(&mut app, "cloud", &WorktreeId("w1".into()));
    for a in app.tree.agents.iter_mut() {
        match a.id.0.as_str() {
            "arch" => a.archived = true,
            "cloud" => a.cloud_session_id = Some("session_01SQugK2HDyk33coSrfqFJk4".into()),
            "a1" => a.issue_url = Some("https://github.com/o/r/issues/7".into()),
            _ => {}
        }
    }
    hse(
        &mut app,
        ServerEvent::EntityUpserted {
            entity: Entity::Terminal(TerminalTab {
                id: TerminalId("t1".into()),
                worktree_id: WorktreeId("w1".into()),
                name: "shell".into(),
                sort_order: 0,
                alive: true,
                run_command: None,
            }),
        },
    );
    app
}

/// Change session `id` in the tree.
fn tweak(app: &mut App, id: &str, f: impl FnOnce(&mut nebula_core::Agent)) {
    let a = app
        .tree
        .agents
        .iter_mut()
        .find(|a| a.id.0 == id)
        .expect("the session");
    f(a);
}

fn sref(id: &str) -> SessionRef {
    SessionRef::Agent(AgentId(id.into()))
}

/// The cursor on agent-1, the card the root's session rows start on.
fn cursor_on_a1(app: &mut App) {
    let rows = app.visible_session_rows();
    app.nav.sel_session = rows
        .iter()
        .position(|r| r.sref() == Some(sref("a1")))
        .expect("agent-1 has a row");
    assert_eq!(
        verbs::selection(app),
        Some(Target::Session(AgentId("a1".into())))
    );
}

/// Land the cursor on session `id` as a click on its card does.
fn select(app: &mut App, id: &str) {
    let mut out = Vec::new();
    super::super::launcher::select(app, AgentId(id.into()), &mut out);
    assert_eq!(
        verbs::selection(app),
        Some(Target::Session(AgentId(id.into())))
    );
}

// ---- the screen ----

fn draw_at(app: &mut App, w: u16, h: u16) -> Terminal<TestBackend> {
    let mut terminal = Terminal::new(TestBackend::new(w, h)).unwrap();
    terminal.draw(|f| ui::draw(f, app)).unwrap();
    terminal
}

/// One mouse event through the app's own mouse handling.
fn mouse(app: &mut App, kind: MouseEventKind, x: u16, y: u16) -> Vec<ClientRequest> {
    let mut out = Vec::new();
    handle_mouse(app, mev(kind, x, y), &mut out);
    out
}

/// Where `sref`'s card was drawn on the grid.
fn card_rect(app: &App, sref: &SessionRef) -> Rect {
    let bands = crate::launcher::bands(app);
    app.chrome
        .hits
        .iter()
        .find(|(_, hit)| match hit {
            HitTarget::LauncherCard(at) => {
                crate::launcher::card_at(&bands, *at).is_some_and(|c| &c.sref() == sref)
            }
            _ => false,
        })
        .map(|(rect, _)| *rect)
        .unwrap_or_else(|| panic!("{sref:?}'s card was not drawn"))
}

/// A cell inside `rect` the menu `over` does not cover.
fn clear_of(rect: Rect, over: Rect) -> (u16, u16) {
    for y in rect.y + 1..rect.y + rect.height {
        for x in rect.x + 1..rect.x + rect.width {
            if !over.contains(Position::new(x, y)) {
                return (x, y);
            }
        }
    }
    panic!("{rect:?} lies under the menu {over:?}");
}

/// The menu on screen.
fn up(app: &App) -> &ContextMenu {
    match &app.modals.overlay {
        Some(Overlay::Menu(menu)) => menu,
        other => panic!("no menu is up: {other:?}"),
    }
}

/// Put the hover on row `row` of the menu on screen.
fn hover(app: &mut App, row: usize) {
    if let Some(Overlay::Menu(menu)) = &mut app.modals.overlay {
        menu.hover = row;
    }
}

fn texts(menu: &ContextMenu) -> Vec<&str> {
    menu.items.iter().map(|i| i.label.as_str()).collect()
}

fn is_submenu(item: &MenuItem) -> bool {
    matches!(item.action, MenuAction::Submenu { .. })
}

/// The rows of the submenu titled `title` among `items`.
fn submenu_rows<'a>(items: &'a [MenuItem], title: &str) -> Option<&'a [MenuItem]> {
    items.iter().find_map(|i| match &i.action {
        MenuAction::Submenu { title: t, items } if t == title => Some(items.as_slice()),
        _ => None,
    })
}

/// Is `item` one of the verbs group `g` folds?
fn group_has(g: &MenuGroup, item: &MenuItem) -> bool {
    item.verb
        .and_then(crate::keymap::spec_of)
        .is_some_and(|s| g.rows.iter().any(|(id, _)| *id == s.id))
}

// ---- the right button on the grid ----

/// A right-click on a card opens that session's menu where the pointer is,
/// the cursor going to the card, and the menu ends on the card's checkout.
#[test]
fn a_right_click_on_a_card_opens_its_menu_and_moves_the_cursor() {
    with_default_config(|| {
        let mut app = grid_app();
        draw_at(&mut app, 160, 60);
        let a3 = AgentId("a3".into());
        assert_ne!(
            verbs::selection(&app),
            Some(Target::Session(a3.clone())),
            "the cursor starts elsewhere"
        );
        let card = card_rect(&app, &sref("a3"));
        let at = (card.x + 3, card.y + 1);
        mouse(
            &mut app,
            MouseEventKind::Down(MouseButton::Right),
            at.0,
            at.1,
        );

        let menu = up(&app);
        assert!(texts(menu).contains(&"Open"), "{:?}", texts(menu));
        assert!(menu
            .items
            .iter()
            .any(|i| i.action == MenuAction::Attach(sref("a3"))));
        let last = menu.items.last().unwrap();
        assert_eq!(last.label, "Worktree feat");
        assert!(
            matches!(&last.action, MenuAction::Submenu { title, .. } if title == "Worktree feat")
        );
        let checkout = submenu_rows(&menu.items, "Worktree feat").unwrap();
        assert!(checkout
            .iter()
            .any(|i| i.action == MenuAction::NewTerminal(WorktreeId("w2".into()))));
        assert_eq!(menu.at, Some(at), "hung at the pointer");
        assert_eq!(verbs::selection(&app), Some(Target::Session(a3)));
    });
}

/// A right-click on a card while the keys are on the project tabs hands
/// them back to the cards: the tab cursor goes, and the menu is the
/// card's with its keyed rows (Diff) and its checkout, not the project's.
#[test]
fn a_right_click_on_a_card_takes_the_keys_back_from_the_project_tabs() {
    with_default_config(|| {
        let mut app = grid_app();
        draw_at(&mut app, 160, 60);
        app.launcher.launcher_tab_cursor = Some(ProjectId("p1".into()));
        assert_eq!(
            verbs::selection(&app),
            Some(Target::Repo(ProjectId("p1".into())))
        );
        let card = card_rect(&app, &sref("a1"));
        mouse(
            &mut app,
            MouseEventKind::Down(MouseButton::Right),
            card.x + 3,
            card.y + 1,
        );
        assert_eq!(app.launcher.launcher_tab_cursor, None);
        let menu = up(&app);
        assert!(texts(menu).contains(&"Diff"), "{:?}", texts(menu));
        assert_eq!(menu.items.last().unwrap().label, "Checkout main");
        assert_eq!(
            verbs::selection(&app),
            Some(Target::Session(AgentId("a1".into())))
        );
    });
}

/// A right-click on a band's rule opens the menu of its worktree, even
/// with cards in the band: the worktree's rows, no card's.
#[test]
fn a_right_click_on_a_bands_rule_opens_the_worktrees_menu() {
    with_default_config(|| {
        let mut app = grid_app();
        draw_at(&mut app, 160, 60);
        let w2 = WorktreeId("w2".into());
        let bands = crate::launcher::bands(&app);
        let band = bands
            .iter()
            .position(|b| b.worktree == w2)
            .expect("feat has a band");
        assert!(!bands[band].cards.is_empty(), "a band with a card in it");
        let rule = app
            .hit_rect(&HitTarget::LauncherBand(band))
            .expect("the band's rule was drawn");
        mouse(
            &mut app,
            MouseEventKind::Down(MouseButton::Right),
            rule.x + 2,
            rule.y,
        );
        let menu = up(&app);
        for want in [
            MenuAction::NewTerminal(w2.clone()),
            MenuAction::NewAgent(w2.clone()),
            MenuAction::DeleteWorktree(w2.clone()),
        ] {
            assert!(
                menu.items.iter().any(|i| i.action == want),
                "{want:?} in {:?}",
                texts(menu)
            );
        }
        assert!(!menu.items.iter().any(|i| i.verb == Some(Action::FollowUp)));
        assert!(!menu
            .items
            .iter()
            .any(|i| matches!(i.action, MenuAction::Attach(_))));
        assert!(!texts(menu).contains(&"Follow-up prompt\u{2026}"));
    });
}

/// A right-click on a board card opens that session's menu, the cursor on
/// the card.
#[test]
fn a_right_click_on_a_board_card_opens_that_sessions_menu() {
    with_default_config(|| {
        let mut app = super::board::bench();
        draw_at(&mut app, 200, 50);
        let (rect, id) = app
            .chrome
            .hits
            .iter()
            .find_map(|(r, h)| match h {
                HitTarget::KanbanCard(id) => Some((*r, id.clone())),
                _ => None,
            })
            .expect("a card on the board");
        mouse(
            &mut app,
            MouseEventKind::Down(MouseButton::Right),
            rect.x + 2,
            rect.y + 2,
        );
        let menu = up(&app);
        assert!(
            menu.items
                .iter()
                .any(|i| i.action == MenuAction::Attach(SessionRef::Agent(id.clone()))),
            "{:?}",
            texts(menu)
        );
        assert_eq!(app.selected_session().map(|a| a.id), Some(id));
    });
}

/// A right-click on the grid's air is the background's menu: the app's own
/// rows, its views folded into View, and no card's.
#[test]
fn a_right_click_on_the_grids_air_opens_the_background_menu() {
    with_default_config(|| {
        let mut app = grid_app();
        draw_at(&mut app, 160, 60);
        let bg = HitTarget::PanelBg(Focus::Sessions);
        let (area, _) = *app
            .chrome
            .hits
            .iter()
            .find(|(_, h)| *h == bg)
            .expect("the grid registered its background");
        let air = (area.y..area.y + area.height)
            .flat_map(|y| (area.x..area.x + area.width).map(move |x| (x, y)))
            .find(|&(x, y)| app.hit_at(x, y) == Some(bg.clone()))
            .expect("some air on the grid");
        mouse(
            &mut app,
            MouseEventKind::Down(MouseButton::Right),
            air.0,
            air.1,
        );
        let menu = up(&app);
        assert_eq!(menu.title, None);
        for want in [
            "New session\u{2026}",
            "Add project\u{2026}",
            "Command palette",
            "Settings",
        ] {
            assert!(texts(menu).contains(&want), "{want} in {:?}", texts(menu));
        }
        let view = submenu_rows(&menu.items, "View").expect("the views fold into View");
        assert!(
            view.len() >= 2,
            "{:?}",
            view.iter().map(|i| &i.label).collect::<Vec<_>>()
        );
        assert!(!menu
            .items
            .iter()
            .any(|i| matches!(i.action, MenuAction::Attach(_))));
    });
}

/// A right-click off an open menu closes it and opens the menu of the new
/// spot, hung there.
#[test]
fn a_right_click_off_a_menu_reopens_it_for_the_new_spot() {
    with_default_config(|| {
        let mut app = grid_app();
        draw_at(&mut app, 160, 60);
        let a1 = card_rect(&app, &sref("a1"));
        mouse(
            &mut app,
            MouseEventKind::Down(MouseButton::Right),
            a1.x + 3,
            a1.y + 1,
        );
        assert_eq!(
            up(&app).items.last().map(|i| i.label.as_str()),
            Some("Checkout main")
        );

        draw_at(&mut app, 160, 60);
        let over = up(&app).area;
        assert!(over.width > 0, "the menu was drawn");
        let a3 = card_rect(&app, &sref("a3"));
        let (x, y) = clear_of(a3, over);
        mouse(&mut app, MouseEventKind::Down(MouseButton::Right), x, y);

        let menu = up(&app);
        assert_eq!(menu.at, Some((x, y)));
        assert_eq!(
            menu.items.last().map(|i| i.label.as_str()),
            Some("Worktree feat")
        );
        assert!(menu
            .items
            .iter()
            .any(|i| i.action == MenuAction::Attach(sref("a3"))));
        assert!(app.modals.verbs_under.is_none());
    });
}

// ---- the pointer in an open menu ----

/// The pointer lights the row it rests on, and the wheel over the menu
/// walks the hover a row a notch.
#[test]
fn the_pointer_lights_a_row_and_the_wheel_walks_the_hover() {
    with_default_config(|| {
        let mut app = bench();
        let items = verbs::menu_for(&app, &Target::Session(AgentId("a3".into())));
        open_menu(&mut app, items, (10, 5));
        draw_at(&mut app, 160, 60);
        let area = up(&app).area;
        assert!(area.width > 0 && area.height > 3, "{area:?}");
        assert_eq!(up(&app).hover, 0);

        let (x, y) = (area.x + 2, area.y + 2);
        mouse(&mut app, MouseEventKind::Moved, x, y);
        assert_eq!(up(&app).hover, 1, "the second row");
        mouse(&mut app, MouseEventKind::ScrollDown, x, y);
        assert_eq!(up(&app).hover, 2);
        mouse(&mut app, MouseEventKind::ScrollUp, x, y);
        mouse(&mut app, MouseEventKind::ScrollUp, x, y);
        assert_eq!(up(&app).hover, 0);
        // Off the menu the pointer lights nothing.
        mouse(&mut app, MouseEventKind::Moved, 0, 0);
        assert_eq!(up(&app).hover, 0);
    });
}

/// Only the left button runs a row: the right and the middle on one are
/// inert, and the left runs it.
#[test]
fn only_the_left_button_runs_a_menu_row() {
    with_default_config(|| {
        let mut app = bench();
        let a3 = AgentId("a3".into());
        let items = verbs::menu_for(&app, &Target::Session(a3.clone()));
        let rename = items
            .iter()
            .position(|i| i.action == MenuAction::RenameAgent(a3.clone()))
            .expect("a Rename row");
        let rows = items.len();
        open_menu(&mut app, items, (10, 5));
        draw_at(&mut app, 160, 60);
        let area = up(&app).area;
        let (x, y) = (area.x + 2, area.y + 1 + rename as u16);

        for button in [MouseButton::Right, MouseButton::Middle] {
            let out = mouse(&mut app, MouseEventKind::Down(button), x, y);
            assert!(out.is_empty(), "{button:?}: {out:?}");
            assert_eq!(up(&app).items.len(), rows, "{button:?} left the menu up");
        }
        mouse(&mut app, MouseEventKind::Down(MouseButton::Left), x, y);
        assert!(
            matches!(&app.modals.overlay, Some(Overlay::Prompt(p))
                if matches!(&p.kind, PromptKind::RenameAgent { id } if *id == a3)),
            "the left button ran Rename: {:?}",
            app.modals.overlay
        );
    });
}

// ---- submenus ----

/// Enter on a submenu row opens it in place without running anything,
/// titled with the way down; Left and Esc go back one level, and Esc at
/// the top closes; a left click on the row opens it too.
#[test]
fn a_submenu_row_opens_in_place_and_left_or_esc_goes_back() {
    with_default_config(|| {
        let mut app = grid_app();
        tweak(&mut app, "a3", |a| a.status = AgentStatus::Running);
        draw_at(&mut app, 160, 60);
        let card = card_rect(&app, &sref("a3"));
        mouse(
            &mut app,
            MouseEventKind::Down(MouseButton::Right),
            card.x + 3,
            card.y + 1,
        );
        let top = up(&app)
            .title
            .clone()
            .expect("a running card's menu is titled");
        assert_eq!(top, "a3 \u{b7} running");
        let rows = up(&app).items.len();
        let copy = up(&app)
            .items
            .iter()
            .position(|i| matches!(&i.action, MenuAction::Submenu { title, .. } if title == "Copy"))
            .expect("the copies fold into one row");
        hover(&mut app, copy);
        let mut out = Vec::new();

        // Enter opens it and runs nothing.
        press(&mut app, KeyCode::Enter, KeyModifiers::NONE, &mut out);
        assert!(out.is_empty(), "{out:?}");
        let sub = up(&app);
        assert_eq!(
            sub.title.as_deref(),
            Some("a3 \u{b7} running \u{203a} Copy")
        );
        assert!(sub.parent.is_some());
        assert_eq!(texts(sub), ["Path", "Branch name"]);

        // Left goes back to the parent as it was.
        press(&mut app, KeyCode::Left, KeyModifiers::NONE, &mut out);
        let back = up(&app);
        assert_eq!(back.title.as_deref(), Some(top.as_str()));
        assert!(back.parent.is_none());
        assert_eq!(back.items.len(), rows);

        // Esc backs out one level too, and closes only from the top.
        press(&mut app, KeyCode::Enter, KeyModifiers::NONE, &mut out);
        assert!(up(&app).parent.is_some());
        press(&mut app, KeyCode::Esc, KeyModifiers::NONE, &mut out);
        assert_eq!(up(&app).title.as_deref(), Some(top.as_str()));
        press(&mut app, KeyCode::Esc, KeyModifiers::NONE, &mut out);
        assert!(app.modals.overlay.is_none(), "{:?}", app.modals.overlay);
        assert!(out.is_empty(), "{out:?}");

        // A left click on the row opens it as well.
        draw_at(&mut app, 160, 60);
        let card = card_rect(&app, &sref("a3"));
        mouse(
            &mut app,
            MouseEventKind::Down(MouseButton::Right),
            card.x + 3,
            card.y + 1,
        );
        draw_at(&mut app, 160, 60);
        let area = up(&app).area;
        mouse(
            &mut app,
            MouseEventKind::Down(MouseButton::Left),
            area.x + 2,
            area.y + 1 + copy as u16,
        );
        let sub = up(&app);
        assert!(
            sub.title.as_deref().is_some_and(|t| t.contains('\u{203a}')),
            "{:?}",
            sub.title
        );
        assert!(sub.parent.is_some());
        assert_eq!(texts(sub), ["Path", "Branch name"]);
    });
}

/// A card's menu folds each group of the registry into one row, at the
/// place of its first, every row of the group inside; a group with one row
/// left stays a plain row; the last row is the card's checkout.
#[test]
fn a_cards_menu_folds_each_group_into_one_row_and_ends_on_its_checkout() {
    with_default_config(|| {
        let app = bench();
        for (id, checkout) in [("a3", "Worktree feat"), ("a1", "Checkout main")] {
            let menu = verbs::menu_for(&app, &Target::Session(AgentId(id.into())));
            let (last, card) = menu.split_last().expect("a menu");
            assert_eq!(last.label, checkout);
            assert!(
                matches!(&last.action, MenuAction::Submenu { title, .. } if title == checkout),
                "{id}: {:?}",
                last.action
            );
            // The checkout row repeats group verbs of its own (Move
            // worktree up), so the card's rows are counted without it.
            let flat = verbs::flat(card);
            for g in MENU_GROUPS
                .iter()
                .filter(|g| g.on.contains(&AppliesTo::Session))
            {
                let members: Vec<Option<Action>> = flat
                    .iter()
                    .filter(|i| group_has(g, i))
                    .map(|i| i.verb)
                    .collect();
                let tops: Vec<&MenuItem> = card
                    .iter()
                    .filter(|i| {
                        group_has(g, i)
                            || matches!(&i.action, MenuAction::Submenu { title, .. } if title == g.title)
                    })
                    .collect();
                if members.len() >= 2 {
                    assert_eq!(tops.len(), 1, "{id}: one {} row", g.title);
                    let MenuAction::Submenu { title, items } = &tops[0].action else {
                        panic!("{id}: {} stayed a plain row", g.title);
                    };
                    assert_eq!(title, g.title);
                    assert_eq!(
                        items.iter().map(|i| i.verb).collect::<Vec<_>>(),
                        members,
                        "{id}: every {} row is inside it",
                        g.title
                    );
                } else {
                    assert_eq!(tops.len(), members.len(), "{id}: {}", g.title);
                    assert!(
                        tops.iter().all(|i| !is_submenu(i)),
                        "{id}: a lone {} row stays plain",
                        g.title
                    );
                }
            }
        }

        // The copies fold under their short words.
        let menu = verbs::menu_for(&app, &Target::Session(AgentId("a3".into())));
        let copy = submenu_rows(&menu, "Copy").expect("Copy folds");
        assert_eq!(
            copy.iter().map(|i| i.label.as_str()).collect::<Vec<_>>(),
            ["Path", "Branch name"]
        );
        // A terminal card ends on its checkout too.
        let term = verbs::menu_for(&app, &Target::Terminal(TerminalId("t1".into())));
        assert_eq!(term.last().unwrap().label, "Checkout main");
    });
}

/// An archived card keeps only what brings it back or takes it away, and
/// its copy; its menu's title says it is archived.
#[test]
fn an_archived_cards_menu_is_duplicate_unarchive_and_delete() {
    with_default_config(|| {
        let app = bench();
        let t = Target::Session(AgentId("arch".into()));
        let menu = verbs::menu_for(&app, &t);
        assert_eq!(
            menu.iter().map(|i| i.verb).collect::<Vec<_>>(),
            [
                Some(Action::DuplicateSession),
                Some(Action::Unarchive),
                Some(Action::Delete)
            ]
        );
        assert_eq!(
            texts_of(&menu),
            ["Duplicate", "Unarchive", "Delete\u{2026}"]
        );
        let title = verbs::menu_title(&app, &t).expect("archived is a notable state");
        assert!(title.contains("archived"), "{title}");
    });
}

fn texts_of(items: &[MenuItem]) -> Vec<&str> {
    items.iter().map(|i| i.label.as_str()).collect()
}

/// A menu's title names the thing and its state only while it is in a
/// notable one: running, needing you, done and unread, archived.
#[test]
fn a_menus_title_names_the_state_only_when_there_is_one() {
    with_default_config(|| {
        let mut app = bench();
        let title =
            |app: &App, id: &str| verbs::menu_title(app, &Target::Session(AgentId(id.into())));
        assert_eq!(title(&app, "a1"), None, "a plain card has no banner");
        for (status, unseen, want) in [
            (AgentStatus::Running, false, "agent-1 \u{b7} running"),
            (
                AgentStatus::NeedsFeedback,
                false,
                "agent-1 \u{b7} needs you",
            ),
            (
                AgentStatus::Finished,
                true,
                "agent-1 \u{b7} done \u{b7} unread",
            ),
        ] {
            tweak(&mut app, "a1", |a| {
                a.status = status;
                a.unseen = unseen;
            });
            assert_eq!(title(&app, "a1").as_deref(), Some(want));
        }
        tweak(&mut app, "a1", |a| a.unseen = false);
        assert_eq!(title(&app, "a1"), None, "done and read is plain again");
        assert_eq!(title(&app, "arch").as_deref(), Some("arch \u{b7} archived"));
    });
}

/// The pull request rows follow the checkout's pull request: Create only
/// with none at all (a merged or closed one has its own row), Merge only on
/// an open one and not a draft, and a merged one's open row says so.
#[test]
fn the_pull_request_rows_follow_the_checkouts_pull_request() {
    with_default_config(|| {
        let mut app = bench();
        let a3 = Target::Session(AgentId("a3".into()));
        let with_pr = |app: &mut App, state: &str, draft: bool| {
            app.github.pull_requests.insert(
                WorktreeId("w2".into()),
                Some(crate::pull_request::PullRequest {
                    number: 12,
                    url: "https://github.com/o/r/pull/12".into(),
                    title: "Polish the nav".into(),
                    state: state.into(),
                    is_draft: draft,
                    head: "feat".into(),
                    head_sha: String::new(),
                    base: "main".into(),
                    health: Default::default(),
                    activity: Vec::new(),
                }),
            );
        };
        let verbs_of = |app: &App| -> Vec<Option<Action>> {
            verbs::flat(&verbs::menu_for(app, &a3))
                .iter()
                .map(|i| i.verb)
                .collect()
        };

        let none = verbs_of(&app);
        assert!(none.contains(&Some(Action::CreatePullRequest)));
        assert!(!none.contains(&Some(Action::OpenPullRequest)));

        with_pr(&mut app, crate::pull_request::STATE_OPEN, false);
        let open = verbs_of(&app);
        assert!(open.contains(&Some(Action::OpenPullRequest)));
        assert!(open.contains(&Some(Action::MergePullRequest)));
        assert!(!open.contains(&Some(Action::CreatePullRequest)));

        with_pr(&mut app, crate::pull_request::STATE_OPEN, true);
        let draft = verbs_of(&app);
        assert!(draft.contains(&Some(Action::OpenPullRequest)));
        assert!(!draft.contains(&Some(Action::MergePullRequest)), "a draft");
        assert!(!draft.contains(&Some(Action::CreatePullRequest)));

        for (state, want) in [
            (
                crate::pull_request::STATE_MERGED,
                "Open merged pull request",
            ),
            (
                crate::pull_request::STATE_CLOSED,
                "Open closed pull request",
            ),
        ] {
            with_pr(&mut app, state, false);
            let gone = verbs_of(&app);
            assert!(!gone.contains(&Some(Action::CreatePullRequest)), "{state}");
            assert!(!gone.contains(&Some(Action::MergePullRequest)), "{state}");
            // The one row left of its group stands alone in whole words.
            let menu = verbs::menu_for(&app, &a3);
            let open = menu
                .iter()
                .find(|i| i.verb == Some(Action::OpenPullRequest))
                .unwrap_or_else(|| panic!("{state}: a plain open row in {:?}", texts_of(&menu)));
            assert_eq!(open.label, want);
        }
    });
}

// ---- `?` and the keys on the selection ----

/// `?` on a card ends on a More submenu and **All keys…**; a typed query
/// searches inside the submenus, naming each hit by its way down, and Enter
/// on a hit runs it directly.
#[test]
fn question_mark_ends_on_more_and_all_keys_and_a_query_searches_the_submenus() {
    with_default_config(|| {
        let mut app = bench();
        select(&mut app, "a3");
        let mut out = Vec::new();
        run_action(&mut app, Action::Verbs, &mut out);

        let menu = up(&app);
        assert!(
            menu.title
                .as_deref()
                .is_some_and(|t| t.starts_with("session a3")),
            "{:?}",
            menu.title
        );
        assert!(menu.filter.is_some(), "type to narrow");
        let n = menu.items.len();
        assert_eq!(menu.items[n - 1].action, MenuAction::AllKeys);
        assert_eq!(menu.items[n - 1].label, "All keys\u{2026}");
        assert!(
            matches!(&menu.items[n - 2].action, MenuAction::Submenu { title, .. } if title == "More"),
            "{:?}",
            texts(menu)
        );

        let Some(Overlay::Menu(menu)) = &mut app.modals.overlay else {
            unreachable!()
        };
        assert!(menu.type_filter('c'));
        assert!(menu.type_filter('o'));
        assert!(
            menu.items.iter().any(|i| i.label.contains(" \u{203a} ")),
            "{:?}",
            texts(menu)
        );
        assert!(
            !menu.items.iter().any(is_submenu),
            "a query lists the rows inside the submenus, not the submenus"
        );
        assert_eq!(menu.items.last().unwrap().action, MenuAction::AllKeys);

        // Enter on a hit runs it, with no submenu in the way.
        assert!(menu.type_filter('p'));
        assert!(menu.type_filter('y'));
        let hit = menu
            .items
            .iter()
            .position(|i| i.label == "Copy \u{203a} Branch name")
            .unwrap_or_else(|| panic!("the copy hit in {:?}", texts(menu)));
        activate::menu_row(&mut app, hit, &mut out);
        assert!(app.modals.overlay.is_none(), "{:?}", app.modals.overlay);
        assert_eq!(app.chrome.flash.as_deref(), Some("Copied feat"));
    });
}

/// The copy keys copy what the card on the cursor stands for: its branch,
/// its checkout's path, its pull request's link; with none, they say so.
#[test]
fn the_copy_keys_copy_what_the_card_stands_for() {
    with_default_config(|| {
        let mut app = bench();
        select(&mut app, "a3");
        let mut out = Vec::new();

        run_action(&mut app, Action::CopyBranch, &mut out);
        assert_eq!(app.chrome.flash.as_deref(), Some("Copied feat"));

        let path = verbs::native(std::path::Path::new("/tmp/demo-feat"));
        run_action(&mut app, Action::CopyPath, &mut out);
        assert_eq!(app.chrome.flash, Some(format!("Copied {path}")));

        app.chrome.flash = None;
        run_action(&mut app, Action::CopyPrUrl, &mut out);
        let label = crate::keymap::spec_of(Action::CopyPrUrl).unwrap().label;
        assert_eq!(
            app.chrome.flash,
            Some(format!("{label}: not for what is under the cursor"))
        );

        super::board::with_open_pr(&mut app);
        run_action(&mut app, Action::CopyPrUrl, &mut out);
        assert_eq!(
            app.chrome.flash.as_deref(),
            Some("Copied https://github.com/o/r/pull/12")
        );
        assert!(
            out.is_empty(),
            "copying asks nothing of the daemon: {out:?}"
        );
    });
}

/// Mark seen on an unread card tells the daemon and clears it; Mark all seen
/// on a repo clears every unread session there, and with none says so.
#[test]
fn mark_seen_clears_the_card_and_mark_all_seen_every_unread_session() {
    with_default_config(|| {
        let mut app = grid_app();
        cursor_on_a1(&mut app);
        let (a1, a3) = (AgentId("a1".into()), AgentId("a3".into()));
        let unseen = |app: &App, id: &str| {
            app.tree
                .agents
                .iter()
                .find(|a| a.id.0 == id)
                .is_some_and(|a| a.unseen)
        };
        tweak(&mut app, "a1", |a| {
            a.status = AgentStatus::Finished;
            a.unseen = true;
        });
        let mut out = Vec::new();
        run_action(&mut app, Action::MarkSeen, &mut out);
        assert!(
            matches!(out.as_slice(), [ClientRequest::MarkAgentSeen { id }] if *id == a1),
            "{out:?}"
        );
        assert!(!unseen(&app, "a1"));
        assert_eq!(app.chrome.flash.as_deref(), Some("marked agent-1 seen"));

        // Every unread one in the repo.
        for id in ["a1", "a3"] {
            tweak(&mut app, id, |a| {
                a.status = AgentStatus::Finished;
                a.unseen = true;
            });
        }
        out.clear();
        let repo = Target::Repo(ProjectId("p1".into()));
        run_menu_action(
            &mut app,
            MenuAction::Verb {
                action: Action::MarkSeen,
                target: repo.clone(),
            },
            &mut out,
        );
        let sent: Vec<&AgentId> = out
            .iter()
            .filter_map(|r| match r {
                ClientRequest::MarkAgentSeen { id } => Some(id),
                _ => None,
            })
            .collect();
        assert_eq!(sent.len(), 2, "{out:?}");
        assert!(sent.contains(&&a1) && sent.contains(&&a3), "{out:?}");
        assert!(!unseen(&app, "a1") && !unseen(&app, "a3"));
        assert_eq!(app.chrome.flash.as_deref(), Some("marked 2 sessions seen"));

        // Nothing unread: nothing sent.
        out.clear();
        run_menu_action(
            &mut app,
            MenuAction::Verb {
                action: Action::MarkSeen,
                target: repo,
            },
            &mut out,
        );
        assert!(out.is_empty(), "{out:?}");
        assert_eq!(app.chrome.flash.as_deref(), Some("nothing unread here"));
    });
}

/// Archive done sessions asks first, listing the finished sessions already
/// read — never an unread or a running one; the key does the same from a
/// card, and with none to take it says so.
#[test]
fn archive_done_asks_first_and_lists_the_finished_read_sessions() {
    with_default_config(|| {
        let mut app = grid_app();
        cursor_on_a1(&mut app);
        let (p1, a1) = (ProjectId("p1".into()), AgentId("a1".into()));
        tweak(&mut app, "a1", |a| {
            a.status = AgentStatus::Finished;
            a.unseen = false;
        });
        tweak(&mut app, "a3", |a| {
            a.status = AgentStatus::Finished;
            a.unseen = true;
        });
        let listed = PendingAction::ArchiveThread {
            agents: vec![a1.clone()],
            terminals: Vec::new(),
        };
        let mut out = Vec::new();
        run_menu_action(
            &mut app,
            MenuAction::Verb {
                action: Action::ArchiveDone,
                target: Target::Repo(p1.clone()),
            },
            &mut out,
        );
        match &app.modals.overlay {
            Some(Overlay::Confirm(c)) => {
                assert!(c.title.starts_with("Archive done"), "{}", c.title);
                assert!(c.title.contains("1 session"), "{}", c.title);
                assert!(c.message.contains("agent-1"), "{}", c.message);
                assert_eq!(c.action, listed);
            }
            other => panic!("no confirm: {other:?} {:?}", app.chrome.flash),
        }
        assert!(out.is_empty(), "nothing before the yes: {out:?}");

        // The key, from the card on the cursor: its checkout's.
        app.modals.overlay = None;
        run_action(&mut app, Action::ArchiveDone, &mut out);
        match &app.modals.overlay {
            Some(Overlay::Confirm(c)) => assert_eq!(c.action, listed),
            other => panic!("no confirm from the key: {other:?} {:?}", app.chrome.flash),
        }

        // Nothing finished and read: no confirm.
        app.modals.overlay = None;
        tweak(&mut app, "a1", |a| a.status = AgentStatus::Running);
        run_menu_action(
            &mut app,
            MenuAction::Verb {
                action: Action::ArchiveDone,
                target: Target::Repo(p1),
            },
            &mut out,
        );
        assert!(app.modals.overlay.is_none());
        assert_eq!(
            app.chrome.flash.as_deref(),
            Some("no finished, read session to archive here")
        );
    });
}

/// The editor and wake-up keys still say they are coming rather than
/// doing nothing, and send nothing (the critters have landed).
#[test]
fn the_keys_of_the_verbs_still_to_land_say_they_are_coming() {
    with_default_config(|| {
        let mut app = bench();
        cursor_on_a1(&mut app);
        let mut out = Vec::new();
        for action in [
            Action::OpenInVscode,
            Action::OpenInVisualStudio,
            Action::DockEditor,
            Action::RevealInExplorer,
            Action::OpenWithDefault,
            Action::OpenFileInKete,
            Action::WakeAccount,
            Action::WakeSchedule,
        ] {
            app.chrome.flash = None;
            run_action(&mut app, action, &mut out);
            let label = crate::keymap::spec_of(action).unwrap().label;
            assert_eq!(
                app.chrome.flash.as_deref(),
                Some(format!("{label}: coming in this release").as_str()),
                "{action:?}"
            );
        }
        assert!(out.is_empty(), "nothing sent: {out:?}");
    });
}

// ---- the right button in an overlay ----

/// The diff overlay over a bench, drawn at `w` by `h` so its file list has
/// a place, and that list's rect.
fn diff_up(w: u16, h: u16) -> (App, Rect) {
    let mut app = bench();
    app.modals.overlay = Some(Overlay::Diff(fake_diff_view(3)));
    draw_at(&mut app, w, h);
    let list = match &app.modals.overlay {
        Some(Overlay::Diff(v)) => v.list_area,
        other => panic!("no diff is up: {other:?}"),
    };
    assert!(list.height >= 2, "the file list was drawn: {list:?}");
    (app, list)
}

/// The file the diff under the menu has its cursor on.
fn diff_file_under(app: &App) -> String {
    match app.modals.verbs_under.as_deref() {
        Some(Overlay::Diff(v)) => v
            .selected_file()
            .map(|f| f.path.clone())
            .expect("a file is selected"),
        other => panic!("the diff is not waiting under the menu: {other:?}"),
    }
}

/// A right-click on a diff's file row puts the cursor on it and opens the
/// file's menu over the diff — its copies, its staging by what git says of
/// the file, Mark reviewed; Esc puts the diff back, and so does a copy.
#[test]
fn a_right_click_on_a_diff_file_row_opens_the_files_menu_over_the_diff() {
    with_default_config(|| {
        let (mut app, list) = diff_up(100, 30);
        let mut out = Vec::new();

        // The second row, beta.rs: untracked, so Stage and no Unstage.
        mouse(
            &mut app,
            MouseEventKind::Down(MouseButton::Right),
            list.x + 2,
            list.y + 1,
        );
        let menu = up(&app);
        assert_eq!(menu.title.as_deref(), Some("diff beta.rs"));
        for want in [
            "Copy path",
            "Copy relative path",
            "Stage file",
            "Mark reviewed",
        ] {
            assert!(texts(menu).contains(&want), "{want} in {:?}", texts(menu));
        }
        for gone in ["Unstage file", "Discard changes\u{2026}"] {
            assert!(!texts(menu).contains(&gone), "{gone} in {:?}", texts(menu));
        }
        assert_eq!(diff_file_under(&app), "beta.rs");

        // Esc puts the diff back.
        press(&mut app, KeyCode::Esc, KeyModifiers::NONE, &mut out);
        assert!(matches!(app.modals.overlay, Some(Overlay::Diff(_))));
        assert!(app.modals.verbs_under.is_none());

        // The first row, alpha.rs: a staged change, so Unstage.
        mouse(
            &mut app,
            MouseEventKind::Down(MouseButton::Right),
            list.x + 2,
            list.y,
        );
        let menu = up(&app);
        assert_eq!(menu.title.as_deref(), Some("diff alpha.rs"));
        assert!(texts(menu).contains(&"Unstage file"), "{:?}", texts(menu));
        assert!(!texts(menu).contains(&"Stage file"), "{:?}", texts(menu));
        assert_eq!(diff_file_under(&app), "alpha.rs");

        // A copy leaves the user where they were: the diff comes back.
        let copy = menu
            .items
            .iter()
            .position(|i| i.label == "Copy relative path")
            .expect("a Copy relative path row");
        activate::menu_row(&mut app, copy, &mut out);
        assert!(
            matches!(app.modals.overlay, Some(Overlay::Diff(_))),
            "{:?}",
            app.modals.overlay
        );
        assert!(app.modals.verbs_under.is_none());
        assert_eq!(app.chrome.flash.as_deref(), Some("Copied alpha.rs"));
        assert!(out.is_empty(), "{out:?}");
    });
}

/// A right-click on an accounts row opens that account's menu over the
/// overlay; Esc goes back to the accounts.
#[test]
fn a_right_click_on_an_accounts_row_opens_that_accounts_menu() {
    with_default_config(|| {
        let mut app = bench();
        let dir = tempfile::tempdir().unwrap();
        let row = AccountRow {
            key: "@claude".into(),
            harness: "claude".into(),
            profile: None,
            usage: None,
            login: LoginState::LoggedIn,
            expired: false,
            default: false,
            default_off: false,
            problem: None,
            sessions: vec!["agent-1".into()],
        };
        let view = AccountsView::new(
            vec![row],
            dir.path().join("config.json"),
            dir.path().join("config.local.json"),
        );
        app.modals.overlay = Some(Overlay::Accounts(view));
        draw_at(&mut app, 120, 40);
        let rect = match &app.modals.overlay {
            Some(Overlay::Accounts(v)) => v
                .hits
                .iter()
                .find(|(_, h)| *h == AccountsHit::Row(0))
                .map(|(r, _)| *r),
            other => panic!("no accounts overlay: {other:?}"),
        }
        .expect("the account's row was drawn");

        mouse(
            &mut app,
            MouseEventKind::Down(MouseButton::Right),
            rect.x + rect.width / 2,
            rect.y,
        );
        let menu = up(&app);
        assert_eq!(menu.title.as_deref(), Some("account claude (own login)"));
        assert!(
            texts(menu).contains(&"Show its sessions"),
            "{:?}",
            texts(menu)
        );
        assert!(
            matches!(
                app.modals.verbs_under.as_deref(),
                Some(Overlay::Accounts(_))
            ),
            "the menu stands on the accounts"
        );

        let mut out = Vec::new();
        press(&mut app, KeyCode::Esc, KeyModifiers::NONE, &mut out);
        assert!(matches!(app.modals.overlay, Some(Overlay::Accounts(_))));
        assert!(app.modals.verbs_under.is_none());
    });
}

/// A file menu drawn on a small screen fits it: the whole menu is inside
/// 80 by 24 and its rows are legible.
#[test]
fn a_file_menu_fits_an_80_by_24_screen() {
    with_default_config(|| {
        let (mut app, list) = diff_up(100, 30);
        mouse(
            &mut app,
            MouseEventKind::Down(MouseButton::Right),
            list.x + 2,
            list.y + 1,
        );
        let terminal = draw_at(&mut app, 80, 24);
        let area = up(&app).area;
        assert!(area.width > 0 && area.height > 2, "{area:?}");
        assert!(
            area.x + area.width <= 80 && area.y + area.height <= 24,
            "{area:?} runs off an 80x24 screen"
        );
        assert!(buffer_text(&terminal).contains("Copy relative path"));
    });
}
