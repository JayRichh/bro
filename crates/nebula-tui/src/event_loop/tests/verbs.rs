//! The menus built from the verb registry (`event_loop::verbs`): every
//! verb the hand-built menus had is still a row, running the same
//! [`MenuAction`]; each row shows its key; `?` opens the selection's
//! verbs; the footer leads with them.

use super::*;
use crate::event_loop::verbs::{self, Target};
use crate::keymap::Action;
use nebula_core::{AgentId, Entity, ProjectId, TerminalId, TerminalTab, Worktree, WorktreeId};

/// [`seed_tree`] (`demo`, its root `main` and agent-1), and a linked
/// worktree `feat` with a session of its own, a terminal in the root, an
/// archived session there and a Cloud one.
fn bench() -> App {
    let mut app = App::new();
    seed_tree(&mut app);
    hse(
        &mut app,
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
    seed_agent_in(&mut app, "a3", &WorktreeId("w2".into()));
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

fn agent(app: &App, id: &str) -> nebula_core::Agent {
    app.tree
        .agents
        .iter()
        .find(|a| a.id.0 == id)
        .cloned()
        .unwrap()
}

fn worktree(app: &App, id: &str) -> Worktree {
    app.tree
        .worktrees
        .iter()
        .find(|w| w.id.0 == id)
        .cloned()
        .unwrap()
}

// ---- the menus as they were built by hand, before the registry ----

fn legacy_session(a: &nebula_core::Agent) -> Vec<MenuAction> {
    let id = || a.id.clone();
    if a.cloud_session_url().filter(|_| !a.archived).is_some() {
        return vec![
            MenuAction::OpenLink(a.cloud_session_url().unwrap()),
            MenuAction::SendCloudMessage(id()),
            MenuAction::DuplicateAgent(id()),
            MenuAction::RenameAgent(id()),
            MenuAction::ArchiveAgent(id()),
            MenuAction::DeleteAgent(id()),
        ];
    }
    if a.archived {
        return vec![
            MenuAction::UnarchiveAgent(id()),
            MenuAction::DuplicateAgent(id()),
            MenuAction::DeleteAgent(id()),
        ];
    }
    vec![
        MenuAction::Attach(SessionRef::Agent(id())),
        MenuAction::FollowUp,
        MenuAction::RestartAgent(id()),
        MenuAction::DuplicateAgent(id()),
        MenuAction::MoveAgentPicker(id()),
        MenuAction::SwitchAccountPicker(id()),
        MenuAction::RenameAgent(id()),
        if a.role.is_orchestrator() {
            MenuAction::ClearOrchestrator
        } else {
            MenuAction::MakeOrchestrator(id())
        },
        MenuAction::ArchiveAgent(id()),
        MenuAction::DeleteAgent(id()),
    ]
}

fn legacy_session_in(app: &App, a: &nebula_core::Agent) -> Vec<MenuAction> {
    let mut items = legacy_session(a);
    if let Some(url) = a.issue_url.clone() {
        items.push(MenuAction::OpenLink(url));
    }
    if a.archived {
        return items;
    }
    let w = worktree(app, &a.worktree_id.0);
    items.push(MenuAction::ToggleRun(w.id.clone()));
    items.push(MenuAction::OpenWorktree(w.id.clone()));
    items.push(MenuAction::Reorder(
        crate::order::OrderTarget::Agent(a.id.clone()),
        -1,
    ));
    items.push(MenuAction::Reorder(
        crate::order::OrderTarget::Agent(a.id.clone()),
        1,
    ));
    if w.is_main {
        items.push(MenuAction::SwitchBranch(w.id.clone()));
    } else {
        items.push(MenuAction::Reorder(
            crate::order::OrderTarget::Worktree(w.id.clone()),
            -1,
        ));
        items.push(MenuAction::Reorder(
            crate::order::OrderTarget::Worktree(w.id.clone()),
            1,
        ));
        items.push(MenuAction::DeleteWorktree(w.id.clone()));
    }
    items
}

fn legacy_worktree(w: &Worktree) -> Vec<MenuAction> {
    let mut items = vec![
        MenuAction::NewAgent(w.id.clone()),
        MenuAction::NewTerminal(w.id.clone()),
        MenuAction::ToggleRun(w.id.clone()),
        MenuAction::OpenWorktree(w.id.clone()),
    ];
    if w.is_main {
        items.push(MenuAction::SwitchBranch(w.id.clone()));
    } else {
        items.push(MenuAction::Reorder(
            crate::order::OrderTarget::Worktree(w.id.clone()),
            -1,
        ));
        items.push(MenuAction::Reorder(
            crate::order::OrderTarget::Worktree(w.id.clone()),
            1,
        ));
        items.push(MenuAction::DeleteWorktree(w.id.clone()));
    }
    items
}

fn legacy_repo(app: &App, p: &ProjectId) -> Vec<MenuAction> {
    let mut items = vec![MenuAction::NewWorktree(p.clone())];
    if let Some(w) = crate::launcher::checkout_for(app, p) {
        items.push(MenuAction::ToggleRun(w.clone()));
        items.push(MenuAction::OpenWorktree(w.clone()));
        if !worktree(app, &w.0).is_main {
            items.push(MenuAction::DeleteWorktree(w));
        }
    }
    items.push(MenuAction::RenameProject(p.clone()));
    items.push(MenuAction::Reorder(
        crate::order::OrderTarget::Project(p.clone()),
        -1,
    ));
    items.push(MenuAction::Reorder(
        crate::order::OrderTarget::Project(p.clone()),
        1,
    ));
    items.push(MenuAction::RemoveProject(p.clone()));
    items
}

/// Every row of `legacy` is still a row of `t`'s menu, at its top or in
/// one of its submenus.
fn assert_kept(app: &App, t: &Target, legacy: Vec<MenuAction>) {
    let menu = verbs::flat(&verbs::menu_for(app, t));
    for want in legacy {
        assert!(
            menu.iter().any(|i| i.action == want),
            "{t:?}: {want:?} is gone from {:?}",
            menu.iter().map(|i| &i.label).collect::<Vec<_>>()
        );
    }
}

/// Every verb the hand-built menus offered on a live, an archived and a
/// Cloud session, a terminal, a root and a linked checkout and a project
/// is a row of its registry-built menu, running the same thing.
#[test]
fn every_menu_verb_is_still_there() {
    let app = bench();
    for id in ["a1", "a3", "arch", "cloud"] {
        let a = agent(&app, id);
        assert_kept(
            &app,
            &Target::Session(a.id.clone()),
            legacy_session_in(&app, &a),
        );
    }
    assert_kept(
        &app,
        &Target::Terminal(TerminalId("t1".into())),
        vec![
            MenuAction::Attach(SessionRef::Terminal(TerminalId("t1".into()))),
            MenuAction::RenameTerminal(TerminalId("t1".into())),
            MenuAction::CloseTerminal(TerminalId("t1".into())),
        ],
    );
    for w in ["w1", "w2"] {
        let w = worktree(&app, w);
        assert_kept(&app, &Target::Worktree(w.id.clone()), legacy_worktree(&w));
    }
    let p = ProjectId("p1".into());
    assert_kept(&app, &Target::Repo(p.clone()), legacy_repo(&app, &p));
}

/// A menu means what its target is now: no Unarchive on a live session,
/// no Restart on an archived or Cloud one, no Delete worktree on the
/// root, no Switch branch on a linked worktree.
#[test]
fn menus_leave_out_what_means_nothing_there() {
    let app = bench();
    let verbs_of = |t: Target| -> Vec<Option<Action>> {
        verbs::menu_for(&app, &t).iter().map(|i| i.verb).collect()
    };
    let live = verbs_of(Target::Session(AgentId("a1".into())));
    assert!(!live.contains(&Some(Action::Unarchive)));
    assert!(live.contains(&Some(Action::Archive)));
    let archived = verbs_of(Target::Session(AgentId("arch".into())));
    assert!(archived.contains(&Some(Action::Unarchive)));
    assert!(!archived.contains(&Some(Action::Restart)));
    assert!(
        !archived.contains(&Some(Action::ToggleRun)),
        "no checkout verbs"
    );
    let cloud = verbs_of(Target::Session(AgentId("cloud".into())));
    assert!(!cloud.contains(&Some(Action::Restart)));
    assert!(!cloud.contains(&Some(Action::PinOrchestrator)));
    let root = verbs_of(Target::Worktree(WorktreeId("w1".into())));
    assert!(root.contains(&Some(Action::SwitchBranch)));
    assert!(!root.contains(&Some(Action::DeleteWorktree)));
    let linked = verbs_of(Target::Worktree(WorktreeId("w2".into())));
    assert!(!linked.contains(&Some(Action::SwitchBranch)));
    assert!(linked.contains(&Some(Action::DeleteWorktree)));
}

/// The same verb says the same words in both apps: the registry's menu
/// labels, the card's checkout rows (in its `Worktree feat ▸`) named as
/// the checkout's where the card has its own.
#[test]
fn menu_rows_take_the_registrys_words_and_show_their_keys() {
    let app = bench();
    let menu = verbs::flat(&verbs::menu_for(
        &app,
        &Target::Session(AgentId("a3".into())),
    ));
    let label = |a: MenuAction| {
        menu.iter()
            .find(|i| i.action == a)
            .map(|i| i.label.clone())
            .unwrap()
    };
    assert_eq!(
        label(MenuAction::Attach(SessionRef::Agent(AgentId("a3".into())))),
        "Open"
    );
    assert_eq!(
        label(MenuAction::OpenWorktree(WorktreeId("w2".into()))),
        "Open in editor"
    );
    assert_eq!(
        label(MenuAction::Reorder(
            crate::order::OrderTarget::Worktree(WorktreeId("w2".into())),
            -1
        )),
        "Move worktree up"
    );
    assert_eq!(
        label(MenuAction::DeleteAgent(AgentId("a3".into()))),
        "Delete\u{2026}"
    );
    // A terminal: Open, Copy path, Rename…, Close terminal…, its checkout.
    let term = verbs::menu_for(&app, &Target::Terminal(TerminalId("t1".into())));
    assert_eq!(term[3].label, "Close terminal\u{2026}");
    assert!(term[3].destructive);
    assert_eq!(term[4].label, "Checkout main");
    let repo = verbs::menu_for(&app, &Target::Repo(ProjectId("p1".into())));
    assert!(repo.iter().any(|i| i.label == "Remove from Kete\u{2026}"));
    // Every registry row carries its verb, so the draw shows its key.
    assert!(menu.iter().all(|i| i.verb.is_some()));
    let rename = menu
        .iter()
        .find(|i| i.verb == Some(Action::Rename))
        .unwrap();
    assert_eq!(
        app.chrome.keymap.first(Action::Rename).unwrap().display(),
        "r"
    );
    let mut terminal = Terminal::new(TestBackend::new(100, 40)).unwrap();
    let mut app = app;
    app.modals.overlay = Some(Overlay::Menu(ContextMenu {
        title: None,
        items: vec![rename.clone()],
        at: Some((2, 2)),
        hover: 0,
        area: ratatui::layout::Rect::default(),
        scroll: 0,
        parent: None,
        filter: None,
    }));
    terminal.draw(|f| ui::draw(f, &mut app)).unwrap();
    let text = buffer_text(&terminal);
    let line = text.lines().find(|l| l.contains("Rename")).unwrap();
    assert!(
        line.split("Rename…")
            .nth(1)
            .and_then(|rest| rest.split('│').next())
            .map(str::trim)
            == Some("r"),
        "the key at the row's right: {line:?}"
    );
}

/// `?` opens the verbs of whatever the cursor is on — a card, a band, a
/// project tab — searchable, keys shown, **All keys…** last; the full
/// help is F1 and that row.
#[test]
fn question_mark_opens_the_selections_verbs() {
    let mut app = bench();
    let mut out = Vec::new();
    // The cursor on a card.
    let rows = app.visible_session_rows();
    let at = rows
        .iter()
        .position(|r| r.sref() == Some(SessionRef::Agent(AgentId("a1".into()))))
        .unwrap();
    app.nav.sel_session = at;
    assert_eq!(
        verbs::selection(&app),
        Some(Target::Session(AgentId("a1".into())))
    );
    run_action(&mut app, Action::Verbs, &mut out);
    let Some(Overlay::Menu(menu)) = &app.modals.overlay else {
        panic!("no verbs menu");
    };
    assert!(menu.title.as_deref().unwrap().contains("agent-1"));
    assert!(menu.filter.is_some(), "type to narrow");
    assert!(menu.items.iter().any(|i| i.verb == Some(Action::Archive)));
    // The rest of what applies, past the card's menu, under one `More ▸`
    // ahead of All keys: a comment on its pull request, its subagents.
    let more = &menu.items[menu.items.len() - 2];
    assert_eq!(more.label, "More");
    assert!(verbs::flat(std::slice::from_ref(more))
        .iter()
        .any(|i| i.action == MenuAction::Run(Action::Subagents)));
    assert_eq!(menu.items.last().unwrap().action, MenuAction::AllKeys);
    // Typing narrows; All keys stays last.
    let Some(Overlay::Menu(menu)) = &mut app.modals.overlay else {
        unreachable!()
    };
    assert!(menu.type_filter('a'));
    assert!(menu.type_filter('r'));
    assert_eq!(menu.items[0].verb, Some(Action::Archive));
    assert_eq!(menu.items.last().unwrap().action, MenuAction::AllKeys);
    // Enter on All keys: the full help.
    let last = menu.items.len() - 1;
    activate::menu_row(&mut app, last, &mut out);
    assert!(matches!(app.modals.overlay, Some(Overlay::Help(_))));
    app.modals.overlay = None;
    // F1 is the help's own key now.
    assert_eq!(app.chrome.keymap.label(Action::Help), "F1");
    assert_eq!(app.chrome.keymap.label(Action::Verbs), "?");

    // A project tab under the header's cursor: the project's verbs.
    app.launcher.launcher_tab_cursor = Some(ProjectId("p1".into()));
    assert_eq!(
        verbs::selection(&app),
        Some(Target::Repo(ProjectId("p1".into())))
    );
    let menu = verbs::selection_menu(&app);
    assert!(menu
        .items
        .iter()
        .any(|i| i.action == MenuAction::RenameProject(ProjectId("p1".into()))));
    app.launcher.launcher_tab_cursor = None;

    // A terminal card.
    let rows = app.visible_session_rows();
    if let Some(at) = rows
        .iter()
        .position(|r| r.sref() == Some(SessionRef::Terminal(TerminalId("t1".into()))))
    {
        app.nav.sel_session = at;
        let menu = verbs::selection_menu(&app);
        assert!(menu.title.as_deref().unwrap().starts_with("terminal"));
        assert!(menu
            .items
            .iter()
            .any(|i| i.action == MenuAction::CloseTerminal(TerminalId("t1".into()))));
    }
}

/// `?` over the accounts' list: that overlay's own verbs, with their
/// keys; Esc goes back to it, and a row puts it back and presses its key.
#[test]
fn question_mark_over_the_accounts_lists_their_verbs() {
    let mut app = bench();
    let mut out = Vec::new();
    accounts::open(&mut app);
    assert!(matches!(app.modals.overlay, Some(Overlay::Accounts(_))));
    handle_overlay_key(
        &mut app,
        KeyEvent::new(KeyCode::Char('?'), KeyModifiers::SHIFT),
        &mut out,
    );
    let Some(Overlay::Menu(menu)) = &app.modals.overlay else {
        panic!("no verbs menu over the accounts");
    };
    let close = menu.items.iter().position(|i| i.label == "Close").unwrap();
    assert_eq!(menu.items[close].key.as_deref(), Some("Esc"));
    assert_eq!(menu.items.last().unwrap().action, MenuAction::AllKeys);
    // Esc: back to the accounts.
    handle_overlay_key(
        &mut app,
        KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE),
        &mut out,
    );
    assert!(matches!(app.modals.overlay, Some(Overlay::Accounts(_))));
    // A row: the accounts back, and its key pressed there (Close).
    handle_overlay_key(
        &mut app,
        KeyEvent::new(KeyCode::Char('?'), KeyModifiers::NONE),
        &mut out,
    );
    activate::menu_row(&mut app, close, &mut out);
    assert!(app.modals.overlay.is_none(), "Esc closed the accounts");
    assert!(app.modals.verbs_under.is_none());
}

/// A key the user gives one of the menus' own verbs runs it on the
/// selection, as its row would: Run on a card runs its checkout's.
#[test]
fn a_menu_verbs_key_acts_on_the_selection() {
    let mut app = bench();
    let rows = app.visible_session_rows();
    app.nav.sel_session = rows
        .iter()
        .position(|r| r.sref() == Some(SessionRef::Agent(AgentId("a1".into()))))
        .unwrap();
    let mut out = Vec::new();
    run_action(&mut app, Action::RemoveProject, &mut out);
    match &app.modals.overlay {
        Some(Overlay::Confirm(c)) => assert!(matches!(c.action, PendingAction::RemoveProject(_))),
        other => panic!("no remove confirm: {other:?}"),
    }
}

/// The footer leads with the selection's verbs, in the registry's words,
/// and `?` for the rest.
#[test]
fn the_footer_leads_with_the_selections_verbs() {
    let mut app = bench();
    let rows = app.visible_session_rows();
    app.nav.sel_session = rows
        .iter()
        .position(|r| r.sref() == Some(SessionRef::Agent(AgentId("a1".into()))))
        .unwrap();
    let top = verbs::primary(&app, 4, true);
    assert_eq!(
        top,
        vec![
            (Action::Activate, "open"),
            (Action::FollowUp, "reply"),
            (Action::GitDiff, "diff"),
            (Action::Archive, "archive"),
        ]
    );
    app.nav.sel_session = rows
        .iter()
        .position(|r| r.sref() == Some(SessionRef::Agent(AgentId("arch".into()))))
        .unwrap_or(app.nav.sel_session);
    if verbs::selection(&app) == Some(Target::Session(AgentId("arch".into()))) {
        let top = verbs::primary(&app, 4, true);
        assert_eq!(top[0], (Action::Unarchive, "unarchive"));
    }
}

/// `^K` leads with the selection's verbs, by their menu words and its
/// name, ahead of every session and command; Enter runs one as its key.
#[test]
fn the_command_palette_leads_with_the_selections_verbs() {
    let mut app = bench();
    let rows = app.visible_session_rows();
    app.nav.sel_session = rows
        .iter()
        .position(|r| r.sref() == Some(SessionRef::Agent(AgentId("a1".into()))))
        .unwrap();
    let mut out = Vec::new();
    run_action(&mut app, Action::CommandPalette, &mut out);
    let Some(Overlay::Palette(p)) = &app.modals.overlay else {
        panic!("no palette");
    };
    let first = &p.list.items[p.list.matches[0].item];
    assert!(first.pinned);
    assert_eq!(first.text, "Open \u{b7} agent-1");
    assert!(p
        .list
        .matches
        .iter()
        .take_while(|m| p.list.items[m.item].pinned)
        .any(|m| p.list.items[m.item].text == "Archive \u{b7} agent-1"));
}

/// `?` over the diff's file list: the diff's own verbs, the diff kept
/// beneath; a row puts it back and presses the row's key there.
#[test]
fn question_mark_over_the_diff_lists_its_verbs() {
    let mut app = bench();
    let mut out = Vec::new();
    app.modals.overlay = Some(Overlay::Diff(fake_diff_view(3)));
    handle_overlay_key(
        &mut app,
        KeyEvent::new(KeyCode::Char('?'), KeyModifiers::NONE),
        &mut out,
    );
    let Some(Overlay::Menu(menu)) = &app.modals.overlay else {
        panic!("no verbs menu over the diff");
    };
    assert!(menu.title.as_deref().unwrap().starts_with("diff"));
    let next = menu
        .items
        .iter()
        .position(|i| i.label == "Next change")
        .unwrap();
    assert_eq!(menu.items[next].key.as_deref(), Some("^n"));
    activate::menu_row(&mut app, next, &mut out);
    assert!(matches!(app.modals.overlay, Some(Overlay::Diff(_))));
}

/// The grid's footer: `? more` first, then the walk and the card's
/// leading verbs as buttons.
#[test]
fn the_grid_footer_shows_more_and_the_cards_verbs() {
    let mut app = App::new();
    seed_tree(&mut app);
    seed_second_agent(&mut app, nebula_core::AgentStatus::Finished);
    let mut terminal = Terminal::new(TestBackend::new(200, 40)).unwrap();
    terminal.draw(|f| ui::draw(f, &mut app)).unwrap();
    app.chrome.flash = None;
    terminal.draw(|f| ui::draw(f, &mut app)).unwrap();
    let hint = |a: Action| app.hit_rect(&HitTarget::FooterHint(a));
    let more = hint(Action::Verbs).expect("? more");
    for a in [
        Action::Activate,
        Action::FollowUp,
        Action::GitDiff,
        Action::Archive,
    ] {
        let at = hint(a).unwrap_or_else(|| panic!("{a:?} on the footer"));
        assert!(more.x < at.x, "? more leads");
    }
    let text = buffer_text(&terminal);
    let footer = text.lines().last().unwrap();
    assert!(footer.contains("?: more"), "{footer}");
    assert!(footer.contains("reply"), "{footer}");
}

/// A verbs menu over the accounts put away some other way (a click off
/// it, the unlock) leaves nothing waiting: a later menu's Esc closes to
/// the grid, never back into the accounts.
#[test]
fn the_overlay_under_a_verbs_menu_never_comes_back_late() {
    let mut app = bench();
    let mut out = Vec::new();
    let q = || KeyEvent::new(KeyCode::Char('?'), KeyModifiers::NONE);
    let esc = || KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE);
    // The unlock: onto the grid, nothing kept.
    accounts::open(&mut app);
    handle_overlay_key(&mut app, q(), &mut out);
    assert!(crate::overlay_close::force_close(&mut app));
    assert!(app.modals.overlay.is_none());
    assert!(app.modals.verbs_under.is_none());
    // A click off it: back to the accounts, as Esc goes.
    accounts::open(&mut app);
    handle_overlay_key(&mut app, q(), &mut out);
    crate::overlay_close::click_outside(&mut app, &mut out);
    assert!(matches!(app.modals.overlay, Some(Overlay::Accounts(_))));
    // `?` again: put away, the same way.
    handle_overlay_key(&mut app, q(), &mut out);
    handle_overlay_key(&mut app, q(), &mut out);
    assert!(matches!(app.modals.overlay, Some(Overlay::Accounts(_))));
    // Left waiting by hand: a card's menu Esc still closes to the grid.
    app.modals.overlay = None;
    app.modals.verbs_under = Some(Box::new(Overlay::Help(HelpView::default())));
    let items = verbs::menu_for(&app, &Target::Session(AgentId("a1".into())));
    open_menu(&mut app, items, (2, 2));
    handle_overlay_key(&mut app, esc(), &mut out);
    assert!(app.modals.overlay.is_none(), "{:?}", app.modals.overlay);
    // A link's menu shows its keys too.
    let row = LinkRow::Saved {
        link: nebula_core::Link {
            id: nebula_core::LinkId("l1".into()),
            worktree_id: WorktreeId("w1".into()),
            url: "https://example.dev".into(),
            sort_order: 0,
        },
        pr: None,
    };
    let link = menu_items_for_link(&row);
    assert_eq!(link[0].verb, Some(Action::Activate));
    assert!(link.iter().any(|i| i.verb == Some(Action::Delete)));
}

/// The project menu's **Delete merged & clean worktrees…**: the linked
/// checkouts with nothing in them and their commits on a remote are
/// listed in one confirm; one with uncommitted work, or a session, stays
/// off it; the yes deletes each unforced and bulk.
#[test]
fn the_sweep_lists_the_clean_worktrees_and_deletes_them_unforced() {
    let dir = tempfile::tempdir().unwrap();
    let origin = test_repo(&dir);
    let clone = dir.path().join("clone");
    let out = std::process::Command::new("git")
        .arg("clone")
        .arg(&origin)
        .arg(&clone)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let (clean, dirty, busy) = (
        dir.path().join("clean"),
        dir.path().join("dirty"),
        dir.path().join("busy"),
    );
    for (path, branch) in [(&clean, "clean"), (&dirty, "dirty"), (&busy, "busy")] {
        run_git(
            &clone,
            &["worktree", "add", "-b", branch, &path.display().to_string()],
        );
    }
    std::fs::write(dirty.join("new.txt"), "x").unwrap();

    let mut app = App::new();
    seed_repo_tree(&mut app, &clone);
    for (i, (path, branch)) in [(&clean, "clean"), (&dirty, "dirty"), (&busy, "busy")]
        .into_iter()
        .enumerate()
    {
        hse(
            &mut app,
            ServerEvent::EntityUpserted {
                entity: Entity::Worktree(Worktree {
                    id: WorktreeId(format!("w-{branch}")),
                    project_id: ProjectId("p1".into()),
                    path: path.clone(),
                    branch: branch.into(),
                    is_main: false,
                    sort_order: i as i64 + 1,
                }),
            },
        );
    }
    seed_agent_in(&mut app, "worker", &WorktreeId("w-busy".into()));
    let p1 = ProjectId("p1".into());
    let menu = verbs::menu_for(&app, &Target::Repo(p1.clone()));
    let row = menu
        .iter()
        .find(|i| i.action == MenuAction::SweepWorktrees(p1.clone()))
        .expect("the project menu offers the sweep");
    assert_eq!(row.label, "Delete merged & clean worktrees\u{2026}");

    let mut out = Vec::new();
    run_menu_action(&mut app, MenuAction::SweepWorktrees(p1), &mut out);
    let Some(Overlay::Confirm(c)) = app.modals.overlay.clone() else {
        panic!(
            "no sweep confirm: {:?} {:?}",
            app.modals.overlay, app.chrome.flash
        );
    };
    assert_eq!(c.title, "Delete 1 worktree");
    assert!(c.message.contains("  clean · clean"), "{}", c.message);
    assert!(!c.message.contains("dirty") && !c.message.contains("busy"));
    app.modals.overlay = None;
    run_pending_action(&mut app, c.action, &mut out);
    let deletes: Vec<_> = out
        .iter()
        .filter_map(|r| match r {
            ClientRequest::DeleteWorktree { id, force, .. } => Some((id.0.clone(), *force)),
            _ => None,
        })
        .collect();
    assert_eq!(deletes, vec![("w-clean".to_string(), false)]);
}

/// The panels' footer is the registry's too: `?` first, then the
/// selection's leading verbs in their one word, as the grid's; a link
/// row, which no verb is about, says what its own keys do.
#[test]
fn the_panels_footer_leads_with_the_registrys_verbs() {
    with_default_config(|| {
        let mut app = bench();
        app.pane.collapsed = true;
        app.nav.focus = Focus::Sessions;
        let rows = app.visible_session_rows();
        app.nav.sel_session = rows
            .iter()
            .position(|r| r.sref() == Some(SessionRef::Agent(AgentId("a1".into()))))
            .unwrap();
        let footer = |app: &mut App| {
            let mut terminal =
                ratatui::Terminal::new(ratatui::backend::TestBackend::new(200, 30)).unwrap();
            terminal.draw(|f| crate::ui::draw(f, app)).unwrap();
            let buf = terminal.backend().buffer().clone();
            (0..buf.area.width)
                .map(|x| buf[(x, buf.area.height - 1)].symbol().to_string())
                .collect::<String>()
        };
        app.chrome.flash = None;
        let text = footer(&mut app);
        let at = |w: &str| {
            text.find(w)
                .unwrap_or_else(|| panic!("{w} missing: {text}"))
        };
        assert!(at("more") < at("open"), "{text}");
        assert!(
            at("open") < at("reply") && at("reply") < at("diff"),
            "{text}"
        );
        assert!(!text.contains("del  "), "no hand-written list: {text}");
    });
}

/// The 0.54/0.55 verbs ahead of their lanes: the editor rows sit in a
/// card's `Open in ▸` after its own verbs, the copies in `Copy ▸` before
/// its changes; an editor's, wake-up's and the critters' (a key, a row, the
/// palette) still say they are coming, while the menus' own verbs and the
/// copies now act.
#[test]
fn the_new_verbs_have_rows_and_say_they_are_coming() {
    let mut app = bench();
    let rows = app.visible_session_rows();
    let at = rows
        .iter()
        .position(|r| r.sref() == Some(SessionRef::Agent(AgentId("a1".into()))))
        .unwrap();
    app.nav.sel_session = at;
    let menu = verbs::menu_for(&app, &Target::Session(AgentId("a1".into())));
    let labels: Vec<&str> = menu.iter().map(|i| i.label.as_str()).collect();
    let pos = |l: &str| {
        labels
            .iter()
            .position(|x| *x == l)
            .unwrap_or_else(|| panic!("{l}: {labels:?}"))
    };
    assert!(pos("Follow-up prompt\u{2026}") < pos("Open in"));
    assert!(pos("Open in") < pos("Copy"));
    assert!(pos("Copy") < pos("Restart agent"));
    let flat = verbs::flat(&menu);
    let words: Vec<&str> = flat.iter().map(|i| i.label.as_str()).collect();
    assert!(words.contains(&"VS Code"), "{words:?}");
    assert_eq!(
        flat.iter()
            .filter(|i| i.verb == Some(Action::CopyPath))
            .count(),
        1
    );
    let mut out = Vec::new();
    for action in [Action::OpenInVscode, Action::WakeSchedule] {
        app.chrome.flash = None;
        run_action(&mut app, action, &mut out);
        let label = crate::keymap::spec_of(action).unwrap().label;
        assert_eq!(
            app.chrome.flash.as_deref(),
            Some(format!("{label}: coming in this release").as_str())
        );
    }
    // The copy is the card's checkout's folder.
    app.chrome.flash = None;
    run_action(&mut app, Action::CopyPath, &mut out);
    let path = verbs::native(&worktree(&app, "w1").path);
    assert_eq!(app.chrome.flash, Some(format!("Copied {path}")));
    assert!(out.is_empty(), "nothing sent: {out:?}");
}

/// The menu matrix's rows show only where they can act: Mark seen on an
/// unread card, a pull request's rows with one, the dev server while it
/// listens, a repo's remote lists by host (its submenu titled by it),
/// Archive done with a finished session already read.
#[test]
fn the_matrix_rows_follow_their_state() {
    let mut app = bench();
    let a1 = AgentId("a1".into());
    let rows = app.visible_session_rows();
    app.nav.sel_session = rows
        .iter()
        .position(|r| r.sref() == Some(SessionRef::Agent(a1.clone())))
        .unwrap();
    let labels = |app: &App, t: &Target| -> Vec<String> {
        verbs::menu_for(app, t)
            .into_iter()
            .map(|i| i.label)
            .collect()
    };
    let has = |app: &App, t: &Target, l: &str| labels(app, t).iter().any(|x| x == l);
    // A verb anywhere in the menu, folded or not: the card's own rows,
    // not its checkout's submenu, for a card.
    let has_verb = |app: &App, t: &Target, v: Action| {
        let menu = verbs::menu_for(app, t);
        let own: Vec<MenuItem> = match t {
            Target::Session(_) => menu[..menu.len() - 1].to_vec(),
            _ => menu,
        };
        verbs::flat(&own).iter().any(|i| i.verb == Some(v))
    };
    let card = Target::Session(a1.clone());
    let root = Target::Worktree(WorktreeId("w1".into()));
    let repo = Target::Repo(ProjectId("p1".into()));

    // Read: no Mark seen; unread: the card's, its checkout's, its repo's.
    let set = |app: &mut App, unseen: bool| {
        for a in app.tree.agents.iter_mut().filter(|a| a.id == a1) {
            a.status = nebula_core::AgentStatus::Finished;
            a.unseen = unseen;
        }
    };
    set(&mut app, false);
    assert!(!has_verb(&app, &card, Action::MarkSeen));
    assert!(!has(&app, &root, "Mark all seen"));
    assert!(
        has(&app, &root, "Archive done sessions\u{2026}"),
        "{:?}",
        labels(&app, &root)
    );
    set(&mut app, true);
    assert!(has(&app, &card, "Mark seen"));
    assert!(has(&app, &root, "Mark all seen"));
    assert!(has(&app, &repo, "Mark all seen"));

    // No pull request on the checkout: no link to copy, nothing to
    // comment on, no Open pull request in its menu.
    for t in [&card, &root] {
        assert!(!has_verb(&app, t, Action::CopyPrUrl), "{t:?}");
        assert!(!has_verb(&app, t, Action::CommentPullRequest), "{t:?}");
    }
    assert!(!has_verb(&app, &root, Action::OpenPullRequest));

    // The dev server only while something listens, by its port.
    assert!(!has_verb(&app, &card, Action::OpenInBrowser));
    app.tree.ports.insert(
        WorktreeId("w1".into()),
        nebula_core::ports::WorktreePorts {
            worktree: WorktreeId("w1".into()),
            block: None,
            live: vec![5173],
        },
    );
    assert!(has_verb(&app, &card, Action::OpenInBrowser));
    assert!(has_verb(&app, &root, Action::OpenInBrowser));
    let browser = verbs::flat(&verbs::menu_for(&app, &root))
        .into_iter()
        .find(|i| i.verb == Some(Action::OpenInBrowser))
        .unwrap();
    assert_eq!(browser.label, "Browser \u{b7} localhost:5173");

    // A repo's lists by host, under its host's name.
    app.desk.ado_remote.insert(ProjectId("p1".into()), false);
    assert!(!has_verb(&app, &repo, Action::Ado));
    assert!(has_verb(&app, &repo, Action::PullRequests));
    assert!(has(&app, &repo, "GitHub"), "{:?}", labels(&app, &repo));
    app.desk.ado_remote.insert(ProjectId("p1".into()), true);
    assert!(has_verb(&app, &repo, Action::Ado));
    assert!(!has_verb(&app, &repo, Action::Issues));
    assert!(
        has(&app, &repo, "Azure DevOps"),
        "{:?}",
        labels(&app, &repo)
    );
}

/// The card's own rows, without its checkout's `Worktree ‹branch› ▸`,
/// flattened out of their submenus.
fn card_rows(app: &App, id: &str) -> Vec<MenuItem> {
    let menu = verbs::menu_for(app, &Target::Session(AgentId(id.into())));
    verbs::flat(&menu[..menu.len() - 1])
}

fn pr_on_feat(state: &str, draft: bool) -> crate::pull_request::PullRequest {
    crate::pull_request::PullRequest {
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
    }
}

/// A checkout's pull request by its state (matrix §5.1): none, Create
/// standing alone; open, the browser, the merge and a comment in `Pull
/// request ▸`; a draft, no merge until it is published; merged or closed,
/// only its own row, in words that say so, and never Create beside it.
#[test]
fn pull_request_rows_follow_its_state() {
    use crate::pull_request::{STATE_CLOSED, STATE_MERGED, STATE_OPEN};
    let mut app = bench();
    let has = |app: &App, a: Action| card_rows(app, "a3").iter().any(|i| i.verb == Some(a));
    let top = |app: &App| -> Vec<String> {
        verbs::menu_for(app, &Target::Session(AgentId("a3".into())))
            .into_iter()
            .map(|i| i.label)
            .collect()
    };
    let feat = WorktreeId("w2".into());
    assert!(top(&app).contains(&"Create pull request\u{2026}".to_string()));

    app.github
        .pull_requests
        .insert(feat.clone(), Some(pr_on_feat(STATE_OPEN, false)));
    assert!(!has(&app, Action::CreatePullRequest));
    assert!(has(&app, Action::MergePullRequest));
    assert!(has(&app, Action::OpenPullRequest));
    assert!(
        top(&app).contains(&"Pull request".to_string()),
        "{:?}",
        top(&app)
    );

    app.github
        .pull_requests
        .insert(feat.clone(), Some(pr_on_feat(STATE_OPEN, true)));
    assert!(
        !has(&app, Action::MergePullRequest),
        "a draft is published first"
    );
    assert!(has(&app, Action::OpenPullRequest));

    for (state, words) in [
        (STATE_MERGED, "Open merged pull request"),
        (STATE_CLOSED, "Open closed pull request"),
    ] {
        app.github
            .pull_requests
            .insert(feat.clone(), Some(pr_on_feat(state, false)));
        assert!(top(&app).contains(&words.to_string()), "{:?}", top(&app));
        for gone in [
            Action::CreatePullRequest,
            Action::MergePullRequest,
            Action::CommentPullRequest,
        ] {
            assert!(!has(&app, gone), "{state}: {gone:?}");
        }
    }
    // A merged checkout says so on its delete and in its banner.
    app.github
        .pull_requests
        .insert(feat.clone(), Some(pr_on_feat(STATE_MERGED, false)));
    let band: Vec<String> = verbs::menu_for(&app, &Target::Worktree(feat.clone()))
        .into_iter()
        .map(|i| i.label)
        .collect();
    assert!(
        band.contains(&"Delete worktree (pull request merged)\u{2026}".to_string()),
        "{band:?}"
    );
    assert_eq!(
        verbs::banner(&app, &Target::Worktree(feat)).as_deref(),
        Some("merged")
    );
}

/// A repo's menu reaches its linked worktrees (`Worktrees ▸`, each landing
/// on its band) after its remote's lists and ahead of what opens another
/// app; a checkout's reaches its live run terminal, on the band and in a
/// card's `Worktree ‹branch› ▸`.
#[test]
fn the_go_rows_connect_a_repo_and_a_checkout_to_their_parts() {
    let mut app = bench();
    let rows = app.visible_session_rows();
    app.nav.sel_session = rows
        .iter()
        .position(|r| r.sref() == Some(SessionRef::Agent(AgentId("a1".into()))))
        .unwrap();
    let repo = verbs::menu_for(&app, &Target::Repo(ProjectId("p1".into())));
    let labels: Vec<&str> = repo.iter().map(|i| i.label.as_str()).collect();
    let pos = |l: &str| {
        labels
            .iter()
            .position(|x| *x == l)
            .unwrap_or_else(|| panic!("{l}: {labels:?}"))
    };
    assert!(pos("Remote") < pos("Worktrees"));
    assert!(pos("Worktrees") < pos("Open in"));
    let MenuAction::Submenu { items, .. } = &repo[pos("Worktrees")].action else {
        panic!("Worktrees is a submenu");
    };
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].label, "feat");
    assert_eq!(
        items[0].action,
        MenuAction::Jump(crate::palette::PaletteTarget::Worktree(WorktreeId(
            "w2".into()
        )))
    );

    hse(
        &mut app,
        ServerEvent::EntityUpserted {
            entity: Entity::Terminal(TerminalTab {
                id: TerminalId("dev".into()),
                worktree_id: WorktreeId("w2".into()),
                name: "dev".into(),
                sort_order: 1,
                alive: true,
                run_command: Some("npm run dev".into()),
            }),
        },
    );
    let show = MenuAction::Attach(SessionRef::Terminal(TerminalId("dev".into())));
    let band = verbs::menu_for(&app, &Target::Worktree(WorktreeId("w2".into())));
    assert!(band
        .iter()
        .any(|i| i.label == "Show run terminal" && i.action == show));
    let card = verbs::menu_for(&app, &Target::Session(AgentId("a3".into())));
    assert!(verbs::flat(&card[card.len() - 1..])
        .iter()
        .any(|i| i.action == show));
}

/// Diff this file (from the tree browser or a finder) opens the
/// checkout's diff on that file; a file with nothing changed says so.
#[test]
fn diff_this_file_lands_the_diff_on_it() {
    let dir = tempfile::tempdir().unwrap();
    let repo = test_repo(&dir);
    std::fs::write(repo.join("a.txt"), "changed\n").unwrap();
    std::fs::write(repo.join("z.txt"), "fresh\n").unwrap();
    let mut app = App::new();
    seed_repo_tree(&mut app, &repo);
    let mut out = Vec::new();
    run_menu_action(&mut app, MenuAction::DiffThisFile("z.txt".into()), &mut out);
    match &app.modals.overlay {
        Some(Overlay::Diff(v)) => assert_eq!(v.selected_path(), Some("z.txt")),
        other => panic!("no diff: {other:?}"),
    }
    assert!(app.modals.diff_land_on.is_none());
    app.modals.overlay = None;
    run_menu_action(
        &mut app,
        MenuAction::DiffThisFile("untouched.txt".into()),
        &mut out,
    );
    assert_eq!(
        app.chrome.flash.as_deref(),
        Some("no changes in untouched.txt")
    );
}

/// `?` over Azure DevOps lists the pull request under its cursor's menu
/// (the browser, its threads, the worktree on its branch, its copies, a
/// vote, the merge), then the overlay's own keys; a vote presses `v` and
/// its letter in the overlay put back.
#[test]
fn azure_devops_rows_have_their_own_menus() {
    use crate::ado::{AdoBranch, AdoPr, AdoTab, AdoView, Load};
    let mut app = bench();
    app.modals.overlay = Some(Overlay::Ado(AdoView {
        project: Some(ProjectId("p1".into())),
        tab: AdoTab::PullRequests,
        prs: Load::Ready(vec![AdoPr {
            id: 7,
            title: "Polish".into(),
            source: "refs/heads/feat".into(),
            target: "refs/heads/main".into(),
            is_draft: false,
            created_by: "someone".into(),
            url: "https://dev.azure.com/o/p/_git/r/pullrequest/7".into(),
            mine: false,
        }]),
        branches: Load::Ready(vec![AdoBranch {
            name: "feat".into(),
            local: true,
            remote: None,
        }]),
        ..AdoView::default()
    }));
    let mut out = Vec::new();
    handle_overlay_key(
        &mut app,
        KeyEvent::new(KeyCode::Char('?'), KeyModifiers::NONE),
        &mut out,
    );
    let Some(Overlay::Menu(menu)) = &app.modals.overlay else {
        panic!("no menu over Azure DevOps: {:?}", app.modals.overlay);
    };
    assert_eq!(menu.title.as_deref(), Some("pull request !7"));
    let labels: Vec<String> = menu.items.iter().map(|i| i.label.clone()).collect();
    for want in [
        "Open in browser",
        "Review comments\u{2026}",
        "Go to worktree feat",
        "Copy",
        "Vote",
        "Merge\u{2026}",
        "Refresh",
        "Branches",
        "All keys\u{2026}",
    ] {
        assert!(labels.iter().any(|l| l == want), "{want}: {labels:?}");
    }
    assert!(matches!(
        app.modals.verbs_under.as_deref(),
        Some(Overlay::Ado(_))
    ));
    let vote = labels.iter().position(|l| l == "Vote").unwrap();
    activate::menu_row(&mut app, vote, &mut out);
    let Some(Overlay::Menu(sub)) = &app.modals.overlay else {
        panic!("the vote submenu");
    };
    assert_eq!(sub.title.as_deref(), Some("pull request !7 \u{203a} Vote"));
    assert_eq!(sub.items[0].label, "Approve");
    activate::menu_row(&mut app, 0, &mut out);
    assert!(matches!(app.modals.overlay, Some(Overlay::Ado(_))));
    assert_eq!(
        app.chrome.flash.as_deref(),
        Some("voting approve on !7\u{2026}")
    );
}
