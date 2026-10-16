//! The editor verbs in the menus and on the keys (`crate::editors`): which
//! rows a card, a checkout and a project list and in what order, what each
//! one runs, the keys, the file rows' actions and the glyph before an
//! editor's row. Nothing here starts an editor, Explorer or a process: the
//! module's `test` hooks answer for the machine's installs and record what
//! would have been launched, so every test names its installs first.

use super::*;
use crate::editors::test::{install, installs, launched, window};
use crate::editors::EditorAct;
use crate::event_loop::verbs::{self, Target};
use crate::icons::IconStyle;
use crate::keymap::Action;
use nebula_core::editors::Family;
use nebula_core::{AgentId, Entity, ProjectId, Worktree, WorktreeId};
use std::path::Path;

const VSCODE: &str = "Open in VS Code";
const VS: &str = "Open in Visual Studio";
const EXPLORER: &str = "Show in Explorer";
const DOCK: &str = "Dock editor beside Kete";
const EDITOR_ROWS: [&str; 4] = [VSCODE, VS, EXPLORER, DOCK];

/// VS Code and Visual Studio 2022, as a typical machine has them.
fn typical() {
    installs(Some(vec![
        install("code", Family::Code, "VS Code"),
        install("vs-2022", Family::VisualStudio, "Visual Studio 2022"),
    ]));
}

/// A checkout folder holding empty files of these names. A fresh one per
/// case: the solutions are cached per path.
fn checkout(files: &[&str]) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    for name in files {
        std::fs::write(dir.path().join(name), "").unwrap();
    }
    dir
}

/// Project `p1` whose main checkout `w1` is at `dir`, with three sessions
/// in it: `a1` (live), `arch` (archived) and `cloud` (a Claude Cloud one).
/// Local, typical installs, no window answer, nothing launched yet.
fn rig(dir: &Path) -> App {
    let mut app = App::new();
    app.chrome.is_remote = false;
    seed_repo_tree(&mut app, dir);
    let w1 = WorktreeId("w1".into());
    for id in ["a1", "arch", "cloud"] {
        seed_agent_in(&mut app, id, &w1);
    }
    for a in app.tree.agents.iter_mut() {
        match a.id.0.as_str() {
            "arch" => a.archived = true,
            "cloud" => a.cloud_session_id = Some("session_01SQugK2HDyk33coSrfqFJk4".into()),
            _ => {}
        }
    }
    typical();
    window(None);
    launched();
    app
}

fn card(id: &str) -> Target {
    Target::Session(AgentId(id.into()))
}

fn checkout_of(id: &str) -> Target {
    Target::Worktree(WorktreeId(id.into()))
}

fn repo() -> Target {
    Target::Repo(ProjectId("p1".into()))
}

/// The cursor on card `id`, as the keys see it.
fn select(app: &mut App, id: &str) {
    let rows = app.visible_session_rows();
    app.nav.sel_session = rows
        .iter()
        .position(|r| r.sref() == Some(SessionRef::Agent(AgentId(id.into()))))
        .unwrap_or_else(|| panic!("{id} has no row"));
    assert_eq!(verbs::selection(app), Some(card(id)));
}

/// A row inside `title ▸` in the words it has standing alone: the
/// matrix folds the editors into `Open in ▸`, where each says only its
/// install ("VS Code", "Cursor", "Visual Studio 2022…").
fn whole(title: &str, item: &MenuItem) -> String {
    if title != "Open in" {
        return item.label.clone();
    }
    match item.verb {
        Some(Action::RevealInExplorer) => EXPLORER.into(),
        Some(Action::DockEditor) => DOCK.into(),
        // The Open command: "Open command" inside, the registry's words alone.
        Some(Action::OpenWorktree) => nebula_config::actions::menu_label(
            "open_worktree",
            nebula_config::actions::AppliesTo::Worktree,
        )
        .to_string(),
        Some(Action::OpenInVscode | Action::OpenInVisualStudio) => {
            format!("Open in {}", item.label)
        }
        _ => item.label.clone(),
    }
}

/// `items` with each submenu's rows in its place, in their whole words.
fn flat_whole(items: Vec<MenuItem>) -> Vec<MenuItem> {
    let mut out = Vec::new();
    for i in items {
        match &i.action {
            MenuAction::Submenu { title, items } => out.extend(items.iter().map(|k| MenuItem {
                label: whole(title, k),
                ..k.clone()
            })),
            _ => out.push(i),
        }
    }
    out
}

/// `t`'s menu, its submenus opened in place.
fn rows_of(app: &App, t: &Target) -> Vec<MenuItem> {
    flat_whole(verbs::menu_for(app, t))
}

fn labels_of(app: &App, t: &Target) -> Vec<String> {
    rows_of(app, t).into_iter().map(|i| i.label).collect()
}

fn has(labels: &[String], label: &str) -> bool {
    labels.iter().any(|l| l == label)
}

fn count(labels: &[String], label: &str) -> usize {
    labels.iter().filter(|l| *l == label).count()
}

/// Where `label` sits in `labels`.
fn at(labels: &[String], label: &str) -> usize {
    labels
        .iter()
        .position(|l| l == label)
        .unwrap_or_else(|| panic!("{label:?} is not in {labels:?}"))
}

/// `t`'s menu row labelled `label`.
fn item(app: &App, t: &Target, label: &str) -> MenuItem {
    rows_of(app, t)
        .into_iter()
        .find(|i| i.label == label)
        .unwrap_or_else(|| panic!("no {label:?} row in {:?}", labels_of(app, t)))
}

/// What a menu row runs, the way a click on it does.
fn run(app: &mut App, row: MenuItem) -> Vec<nebula_core::ClientRequest> {
    let mut out = Vec::new();
    run_menu_action(app, row.action, &mut out);
    out
}

fn flash(app: &App) -> String {
    app.chrome.flash.clone().unwrap_or_default()
}

/// What a key says when it has nothing to act on.
fn not_for(action: Action) -> String {
    let label = crate::keymap::spec_of(action).unwrap().label;
    format!("{label}: not for what is under the cursor")
}

// ---- the rows on a card ----

/// A live local card lists the editors after its pull request rows and
/// before Restart, in the matrix's order; its checkout's rows after them
/// don't repeat any.
#[test]
fn a_live_local_card_lists_the_editor_rows_between_follow_up_and_restart() {
    let dir = checkout(&["Api.sln"]);
    let mut app = rig(dir.path());
    select(&mut app, "a1");
    let shown = labels_of(&app, &card("a1"));
    let order = [
        "Follow-up prompt\u{2026}",
        VSCODE,
        VS,
        EXPLORER,
        DOCK,
        "Restart agent",
    ];
    let places: Vec<usize> = order.iter().map(|l| at(&shown, l)).collect();
    assert!(places.windows(2).all(|w| w[0] < w[1]), "{shown:?}");
    for l in EDITOR_ROWS {
        assert_eq!(count(&shown, l), 1, "{l} once: {shown:?}");
    }
}

/// An archived card runs in no checkout, so none of the editor rows
/// belong to it, whatever is installed.
#[test]
fn an_archived_card_lists_no_editor_rows() {
    let dir = checkout(&["Api.sln"]);
    let app = rig(dir.path());
    let with_editors = labels_of(&app, &card("arch"));
    for l in EDITOR_ROWS {
        assert!(!has(&with_editors, l), "{l}: {with_editors:?}");
    }
    installs(Some(vec![]));
    assert_eq!(
        with_editors,
        labels_of(&app, &card("arch")),
        "the installs change nothing on it"
    );
}

/// A Cloud card's agent runs in the cloud sandbox: the matrix hides its
/// editor rows (§5.1 row 8).
#[test]
fn a_cloud_card_lists_no_editor_rows() {
    let dir = checkout(&["Api.sln"]);
    let app = rig(dir.path());
    let with_editors = labels_of(&app, &card("cloud"));
    for l in EDITOR_ROWS {
        assert!(!has(&with_editors, l), "{l}: {with_editors:?}");
    }
    installs(Some(vec![]));
    assert_eq!(
        with_editors,
        labels_of(&app, &card("cloud")),
        "the installs change nothing on it"
    );
}

/// Over ssh an editor would open on the far machine's screen: no menu
/// lists an editor row, and nothing else in them moves.
#[test]
fn over_ssh_no_menu_lists_an_editor_row_and_the_others_stay() {
    let dir = checkout(&["Api.sln"]);
    let mut app = rig(dir.path());
    select(&mut app, "a1");
    let targets = [card("a1"), checkout_of("w1"), repo()];
    let home: Vec<Vec<String>> = targets.iter().map(|t| labels_of(&app, t)).collect();
    assert!(has(&home[0], VSCODE), "the same card lists them at home");
    app.chrome.is_remote = true;
    for (t, before) in targets.iter().zip(&home) {
        let now = labels_of(&app, t);
        for l in EDITOR_ROWS {
            assert!(!has(&now, l), "{t:?} lists {l} over ssh: {now:?}");
        }
        let rest: Vec<&String> = before
            .iter()
            .filter(|l| !EDITOR_ROWS.contains(&l.as_str()))
            .collect();
        assert_eq!(
            now.iter().collect::<Vec<_>>(),
            rest,
            "{t:?}: only the editor rows go"
        );
    }
    let card_rows = labels_of(&app, &card("a1"));
    assert!(
        has(&card_rows, "Path") && has(&card_rows, "Branch name"),
        "the copy rows (Copy ▸) stay: {card_rows:?}"
    );
    // `?` lists the menu's rows and the rest of what applies.
    let verbs_menu = |app: &App| -> Vec<String> {
        flat_whole(verbs::selection_menu(app).items)
            .into_iter()
            .map(|i| i.label)
            .collect()
    };
    let over_ssh = verbs_menu(&app);
    for l in EDITOR_ROWS {
        assert!(!has(&over_ssh, l), "`?` lists {l} over ssh: {over_ssh:?}");
    }
    app.chrome.is_remote = false;
    assert!(has(&verbs_menu(&app), VSCODE), "`?` lists them at home");
}

// ---- what is installed ----

/// VS Code is assumed until the scan answers, and so is a window to dock;
/// Visual Studio is not, with or without a solution.
#[test]
fn before_the_scan_answers_vs_code_and_dock_are_offered_and_visual_studio_is_not() {
    let dir = checkout(&["Api.sln"]);
    let app = rig(dir.path());
    installs(None);
    let shown = labels_of(&app, &card("a1"));
    assert!(has(&shown, VSCODE), "{shown:?}");
    assert!(has(&shown, DOCK), "{shown:?}");
    assert!(has(&shown, EXPLORER), "{shown:?}");
    assert!(!has(&shown, VS), "{shown:?}");
}

/// A scan that found nothing: no editor, so no Visual Studio, no VS Code
/// and nothing to dock; Explorer is the machine's own.
#[test]
fn a_scan_that_found_nothing_leaves_only_explorer() {
    let dir = checkout(&["Api.sln"]);
    let app = rig(dir.path());
    installs(Some(vec![]));
    let shown = labels_of(&app, &card("a1"));
    for l in [VSCODE, VS, DOCK] {
        assert!(!has(&shown, l), "{l}: {shown:?}");
    }
    assert!(has(&shown, EXPLORER), "{shown:?}");
}

/// A fork alone says its own name; beside VS Code its row follows VS
/// Code's, once.
#[test]
fn cursor_alone_is_named_and_beside_vs_code_it_follows_it() {
    let dir = checkout(&[]);
    let mut app = rig(dir.path());
    installs(Some(vec![install("cursor", Family::Code, "Cursor")]));
    let shown = labels_of(&app, &card("a1"));
    assert!(has(&shown, "Open in Cursor"), "{shown:?}");
    assert!(!has(&shown, VSCODE), "{shown:?}");

    installs(Some(vec![
        install("code", Family::Code, "VS Code"),
        install("cursor", Family::Code, "Cursor"),
    ]));
    let shown = labels_of(&app, &card("a1"));
    assert_eq!(
        at(&shown, "Open in Cursor"),
        at(&shown, VSCODE) + 1,
        "{shown:?}"
    );
    assert_eq!(count(&shown, "Open in Cursor"), 1, "{shown:?}");

    let cursor = item(&app, &card("a1"), "Open in Cursor");
    let out = run(&mut app, cursor);
    let log = launched();
    assert!(log.iter().any(|l| l.starts_with("cursor ")), "{log:?}");
    assert!(out.is_empty(), "nothing sent: {out:?}");
}

// ---- Visual Studio ----

/// Visual Studio opens a solution or a project: a checkout with neither
/// has no row for it.
#[test]
fn visual_studio_is_offered_only_where_the_checkout_has_a_solution() {
    let bare = checkout(&["README.md"]);
    let app = rig(bare.path());
    for t in [card("a1"), checkout_of("w1"), repo()] {
        let shown = labels_of(&app, &t);
        assert!(!has(&shown, VS), "{t:?}: {shown:?}");
        assert!(has(&shown, VSCODE), "{t:?}: {shown:?}");
    }
}

/// One solution opens at once, in the install that is installed.
#[test]
fn a_checkout_with_one_solution_opens_it_at_once() {
    with_default_config(|| {
        let dir = checkout(&["Api.sln"]);
        let mut app = rig(dir.path());
        let vs = item(&app, &card("a1"), VS);
        assert_eq!(
            vs.action,
            MenuAction::Editor(EditorAct::Solution {
                editor: None,
                root: dir.path().to_path_buf()
            })
        );
        let out = run(&mut app, vs);
        assert!(app.modals.overlay.is_none(), "no pick for one solution");
        let log = launched();
        assert_eq!(log.len(), 1, "{log:?}");
        assert!(
            log[0].starts_with("vs-2022 Solution(") && log[0].contains("Api.sln"),
            "{log:?}"
        );
        assert!(out.is_empty(), "nothing sent: {out:?}");
    });
}

/// Several solutions: the row says there is a pick (`…`), the pick is a
/// menu with one row per solution, and a row opens that solution.
#[test]
fn several_solutions_are_picked_from_a_menu() {
    with_default_config(|| {
        let dir = checkout(&["Api.sln", "Web.sln"]);
        let mut app = rig(dir.path());
        let ask = "Open in Visual Studio\u{2026}";
        let shown = labels_of(&app, &card("a1"));
        assert!(has(&shown, ask), "{shown:?}");
        assert!(!has(&shown, VS), "{shown:?}");
        let vs = item(&app, &card("a1"), ask);
        run(&mut app, vs);
        assert!(launched().is_empty(), "a pick comes first");
        let Some(Overlay::Menu(menu)) = &app.modals.overlay else {
            panic!("no solution pick: {:?} {}", app.modals.overlay, flash(&app));
        };
        assert_eq!(menu.title.as_deref(), Some("Open in Visual Studio"));
        let rels: Vec<&str> = menu.items.iter().map(|i| i.label.as_str()).collect();
        assert_eq!(rels, ["Api.sln", "Web.sln"]);

        let mut out = Vec::new();
        activate::menu_row(&mut app, 1, &mut out);
        let log = launched();
        assert_eq!(log.len(), 1, "{log:?}");
        assert!(
            log[0].starts_with("vs-2022 Solution(") && log[0].contains("Web.sln"),
            "{log:?}"
        );
        assert!(out.is_empty(), "nothing sent: {out:?}");
    });
}

/// Two Visual Studios: each year its own row, the newest first, and the
/// bare "Open in Visual Studio" gives way to them.
#[test]
fn two_visual_studios_get_a_row_each_by_year() {
    let dir = checkout(&["Api.sln"]);
    let app = rig(dir.path());
    installs(Some(vec![
        install("code", Family::Code, "VS Code"),
        install("vs-2022", Family::VisualStudio, "Visual Studio 2022"),
        install("vs-2019", Family::VisualStudio, "Visual Studio 2019"),
    ]));
    let shown = labels_of(&app, &card("a1"));
    assert!(!has(&shown, VS), "{shown:?}");
    assert_eq!(
        at(&shown, "Open in Visual Studio 2019"),
        at(&shown, "Open in Visual Studio 2022") + 1,
        "{shown:?}"
    );
    assert_eq!(count(&shown, "Open in Visual Studio 2019"), 1, "{shown:?}");
}

/// The older Visual Studio's row, over several solutions, picks one and
/// opens it in that install.
#[test]
fn the_second_visual_studio_opens_the_picked_solution() {
    with_default_config(|| {
        let dir = checkout(&["Api.sln", "Web.sln"]);
        let mut app = rig(dir.path());
        installs(Some(vec![
            install("vs-2022", Family::VisualStudio, "Visual Studio 2022"),
            install("vs-2019", Family::VisualStudio, "Visual Studio 2019"),
        ]));
        let shown = labels_of(&app, &card("a1"));
        assert!(
            has(&shown, "Open in Visual Studio 2022\u{2026}"),
            "{shown:?}"
        );
        let old = item(&app, &card("a1"), "Open in Visual Studio 2019\u{2026}");
        run(&mut app, old);
        let mut out = Vec::new();
        activate::menu_row(&mut app, 0, &mut out);
        let log = launched();
        assert_eq!(log.len(), 1, "{log:?}");
        assert!(
            log[0].starts_with("vs-2019 Solution(") && log[0].contains("Api.sln"),
            "{log:?}"
        );
    });
}

/// A `.slnx` waits for the Visual Studio that opens it (17.14 on): an
/// older one says so and starts nothing.
#[test]
fn an_slnx_solution_waits_for_a_visual_studio_that_opens_it() {
    with_default_config(|| {
        let dir = checkout(&["Api.slnx"]);
        let mut app = rig(dir.path());
        let mut old = install("vs-2022", Family::VisualStudio, "Visual Studio 2022");
        old.version = "17.13.1".into();
        installs(Some(vec![old]));
        let vs = item(&app, &card("a1"), VS);
        run(&mut app, vs);
        assert!(launched().is_empty());
        assert_eq!(
            flash(&app),
            "Visual Studio 2022 opens .slnx solutions from 17.14 on; this one is 17.13.1"
        );
    });
}

// ---- Dock ----

/// Dock follows the window check: gone once no editor window is open on
/// the checkout, offered while one is or when that isn't known; running it
/// docks the checkout.
#[test]
fn dock_follows_the_window_check() {
    let dir = checkout(&[]);
    let mut app = rig(dir.path());
    window(Some(false));
    assert!(!has(&labels_of(&app, &card("a1")), DOCK));
    for answer in [Some(true), None] {
        window(answer);
        assert!(has(&labels_of(&app, &card("a1")), DOCK), "{answer:?}");
    }
    let dock = item(&app, &card("a1"), DOCK);
    let out = run(&mut app, dock);
    assert_eq!(launched(), vec![format!("dock {}", dir.path().display())]);
    assert_eq!(flash(&app), "docked the editor beside Kete");
    assert!(out.is_empty(), "nothing sent: {out:?}");
}

// ---- the other targets ----

/// A checkout's menu and a repo's carry the same editor rows. A repo's
/// act on its main checkout, a linked checkout's on its own folder.
#[test]
fn a_checkouts_menu_and_a_repos_carry_the_same_editor_rows() {
    let main = checkout(&["Api.sln"]);
    let feat = checkout(&["Api.sln"]);
    let mut app = rig(main.path());
    hse(
        &mut app,
        ServerEvent::EntityUpserted {
            entity: Entity::Worktree(Worktree {
                id: WorktreeId("w2".into()),
                project_id: ProjectId("p1".into()),
                path: feat.path().to_path_buf(),
                branch: "feat".into(),
                is_main: false,
                sort_order: 1,
            }),
        },
    );
    let rows = |t: &Target| -> Vec<MenuItem> {
        rows_of(&app, t)
            .into_iter()
            .filter(|i| EDITOR_ROWS.contains(&i.label.as_str()))
            .collect()
    };
    let labels = |t: &Target| -> Vec<String> { rows(t).into_iter().map(|i| i.label).collect() };
    let actions =
        |t: &Target| -> Vec<MenuAction> { rows(t).into_iter().map(|i| i.action).collect() };
    let on = |p: &Path| {
        vec![
            MenuAction::Editor(EditorAct::Open {
                family: Family::Code,
                editor: None,
                path: p.to_path_buf(),
                root: Some(p.to_path_buf()),
                line: None,
            }),
            MenuAction::Editor(EditorAct::Solution {
                editor: None,
                root: p.to_path_buf(),
            }),
            MenuAction::Editor(EditorAct::Reveal {
                path: p.to_path_buf(),
                folder: true,
            }),
            MenuAction::Editor(EditorAct::Dock(p.to_path_buf())),
        ]
    };
    for t in [checkout_of("w1"), checkout_of("w2"), repo()] {
        assert_eq!(labels(&t), EDITOR_ROWS, "{t:?}");
    }
    assert_eq!(actions(&checkout_of("w1")), on(main.path()));
    assert_eq!(actions(&checkout_of("w2")), on(feat.path()));
    assert_eq!(
        actions(&repo()),
        on(main.path()),
        "a repo's rows act on its main checkout"
    );
}

// ---- the keys ----

/// An editor verb on a key the user gave it acts on the selected card's
/// checkout as its menu row does, and sends the daemon nothing.
#[test]
fn an_editor_key_acts_on_the_selected_cards_checkout_and_sends_nothing() {
    with_default_config(|| {
        let dir = checkout(&["Api.sln"]);
        let mut app = rig(dir.path());
        select(&mut app, "a1");
        let mut out = Vec::new();

        run_action(&mut app, Action::OpenInVscode, &mut out);
        let log = launched();
        assert_eq!(log.len(), 1, "{log:?}");
        assert!(log[0].starts_with("code Folder("), "{log:?}");

        run_action(&mut app, Action::RevealInExplorer, &mut out);
        assert_eq!(
            launched(),
            vec![format!("reveal {} true", dir.path().display())]
        );

        run_action(&mut app, Action::DockEditor, &mut out);
        assert_eq!(launched(), vec![format!("dock {}", dir.path().display())]);

        run_action(&mut app, Action::OpenInVisualStudio, &mut out);
        let log = launched();
        assert_eq!(log.len(), 1, "{log:?}");
        assert!(log[0].starts_with("vs-2022 Solution("), "{log:?}");

        assert!(out.is_empty(), "nothing sent: {out:?}");
    });
}

/// With nothing under the cursor an editor key says so, in the verb's own
/// words, and starts nothing.
#[test]
fn an_editor_key_with_nothing_under_the_cursor_says_so() {
    installs(None);
    let mut app = App::new();
    app.chrome.is_remote = false;
    let mut out = Vec::new();
    for action in [
        Action::OpenInVscode,
        Action::RevealInExplorer,
        Action::DockEditor,
    ] {
        app.chrome.flash = None;
        run_action(&mut app, action, &mut out);
        assert_eq!(flash(&app), not_for(action));
    }
    assert!(launched().is_empty());
    assert!(out.is_empty(), "nothing sent: {out:?}");
}

/// A verb for files only, or Visual Studio on a checkout with no solution,
/// has nothing to act on at a card.
#[test]
fn an_editor_key_for_what_the_card_cannot_give_says_so() {
    let dir = checkout(&["README.md"]);
    let mut app = rig(dir.path());
    select(&mut app, "a1");
    let mut out = Vec::new();
    for action in [
        Action::OpenWithDefault,
        Action::OpenFileInKete,
        Action::OpenInVisualStudio,
    ] {
        app.chrome.flash = None;
        run_action(&mut app, action, &mut out);
        assert_eq!(flash(&app), not_for(action));
    }
    assert!(launched().is_empty());
    assert!(out.is_empty(), "nothing sent: {out:?}");
}

/// Over ssh no editor key reaches the far machine's screen.
#[test]
fn over_ssh_an_editor_key_says_so_and_starts_nothing() {
    let dir = checkout(&["Api.sln"]);
    let mut app = rig(dir.path());
    select(&mut app, "a1");
    app.chrome.is_remote = true;
    let mut out = Vec::new();
    for action in [
        Action::OpenInVscode,
        Action::OpenInVisualStudio,
        Action::RevealInExplorer,
        Action::DockEditor,
    ] {
        app.chrome.flash = None;
        run_action(&mut app, action, &mut out);
        assert_eq!(flash(&app), not_for(action));
    }
    assert!(launched().is_empty());
    assert!(out.is_empty(), "nothing sent: {out:?}");
}

// ---- a file's verbs ----

/// A file's rows, as the file menus ask for them: a program or a script
/// never goes to its default app, Explorer selects the file in its folder,
/// Visual Studio needs a solution in the root, and over ssh only Kete's
/// own viewer is left.
#[test]
fn a_files_verbs_follow_what_can_open_it() {
    let dir = checkout(&["Api.sln"]);
    let mut app = rig(dir.path());
    let root = dir.path();
    let file = |name: &str| root.join(name);
    let act = |app: &App, action: Action, name: &str| {
        crate::editors::file_action(app, action, root, &file(name), None)
    };
    let editor = |act: EditorAct| Some(MenuAction::Editor(act));

    assert_eq!(act(&app, Action::OpenWithDefault, "setup.exe"), None);
    assert_eq!(act(&app, Action::OpenWithDefault, "run.ps1"), None);
    assert_eq!(
        act(&app, Action::OpenWithDefault, "notes.md"),
        editor(EditorAct::OpenDefault(file("notes.md")))
    );
    assert_eq!(
        act(&app, Action::RevealInExplorer, "notes.md"),
        editor(EditorAct::Reveal {
            path: file("notes.md"),
            folder: false
        })
    );
    assert_eq!(
        act(&app, Action::OpenFileInKete, "notes.md"),
        editor(EditorAct::View {
            root: root.to_path_buf(),
            path: file("notes.md")
        })
    );
    assert_eq!(
        crate::editors::file_action(&app, Action::OpenInVscode, root, &file("a.cs"), Some(12)),
        editor(EditorAct::Open {
            family: Family::Code,
            editor: None,
            path: file("a.cs"),
            root: Some(root.to_path_buf()),
            line: Some(12)
        })
    );
    assert_eq!(
        act(&app, Action::OpenInVisualStudio, "a.cs"),
        editor(EditorAct::Open {
            family: Family::VisualStudio,
            editor: None,
            path: file("a.cs"),
            root: Some(root.to_path_buf()),
            line: None
        })
    );

    // Visual Studio: installed, and a solution in the root.
    let bare = checkout(&[]);
    assert_eq!(
        crate::editors::file_action(
            &app,
            Action::OpenInVisualStudio,
            bare.path(),
            &bare.path().join("a.cs"),
            None
        ),
        None,
        "no solution in the root"
    );
    installs(Some(vec![install("code", Family::Code, "VS Code")]));
    assert_eq!(
        act(&app, Action::OpenInVisualStudio, "a.cs"),
        None,
        "not installed"
    );
    installs(Some(vec![]));
    assert_eq!(
        act(&app, Action::OpenInVscode, "a.cs"),
        None,
        "no VS Code found"
    );
    assert_eq!(
        act(&app, Action::OpenFileInKete, "a.cs"),
        editor(EditorAct::View {
            root: root.to_path_buf(),
            path: file("a.cs")
        }),
        "the viewer needs no install"
    );

    // Over ssh: Kete's viewer alone.
    typical();
    app.chrome.is_remote = true;
    for action in [
        Action::OpenInVscode,
        Action::OpenInVisualStudio,
        Action::OpenWithDefault,
        Action::RevealInExplorer,
    ] {
        assert_eq!(act(&app, action, "notes.md"), None, "{action:?}");
    }
    assert_eq!(
        act(&app, Action::OpenFileInKete, "notes.md"),
        editor(EditorAct::View {
            root: root.to_path_buf(),
            path: file("notes.md")
        })
    );
    assert!(launched().is_empty());
}

/// Running Open with default app on a program says why it won't and starts
/// nothing; a document is handed to the system.
#[test]
fn open_with_default_app_refuses_a_program_and_hands_a_document_over() {
    installs(None);
    let dir = checkout(&[]);
    let mut app = App::new();
    crate::editors::run(
        &mut app,
        EditorAct::OpenDefault(dir.path().join("setup.exe")),
    );
    assert_eq!(
        flash(&app),
        "Kete doesn't run programs: use Show in Explorer"
    );
    assert!(launched().is_empty());

    let notes = dir.path().join("notes.md");
    crate::editors::run(&mut app, EditorAct::OpenDefault(notes.clone()));
    assert_eq!(launched(), vec![format!("default {}", notes.display())]);
    assert_eq!(flash(&app), "opening notes.md");
}

// ---- the glyphs ----

/// A menu of `items` drawn at (2, 2) of an 80x24 screen in icon `style`.
fn drawn(app: &mut App, items: Vec<MenuItem>, style: IconStyle) -> Terminal<TestBackend> {
    app.chrome.icons = style;
    app.modals.overlay = Some(Overlay::Menu(ContextMenu {
        title: None,
        items,
        at: Some((2, 2)),
        hover: 0,
        area: ratatui::layout::Rect::default(),
        scroll: 0,
        parent: None,
        filter: None,
    }));
    let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
    terminal.draw(|f| ui::draw(f, app)).unwrap();
    terminal
}

/// The screen row holding `needle`.
fn screen_row(terminal: &Terminal<TestBackend>, needle: &str) -> String {
    let text = buffer_text(terminal);
    text.lines()
        .find(|l| l.contains(needle))
        .unwrap_or_else(|| panic!("{needle:?} is not on screen"))
        .to_string()
}

/// The cell column `needle` starts at in `terminal`'s row holding it.
fn column(terminal: &Terminal<TestBackend>, needle: &str) -> usize {
    let line = screen_row(terminal, needle);
    let byte = line.find(needle).unwrap();
    line[..byte].chars().count()
}

/// An editor row leads with its editor's glyph: Nerd Font marks, `VSC` and
/// `VS` in the ascii style, nothing with the icons off. A row that is no
/// editor's has none, and no label but the editors' moves.
#[test]
fn an_editor_rows_glyph_leads_its_label_and_no_other_label_moves() {
    let dir = checkout(&["Api.sln"]);
    let mut app = rig(dir.path());
    let w1 = checkout_of("w1");
    let rows = vec![
        item(&app, &w1, VSCODE),
        item(&app, &w1, VS),
        item(&app, &w1, EXPLORER),
        MenuItem::new(
            "Rename\u{2026}",
            MenuAction::RenameAgent(AgentId("a1".into())),
        ),
    ];
    let off = drawn(&mut app, rows.clone(), IconStyle::Off);
    let nerd = drawn(&mut app, rows.clone(), IconStyle::Nerd);
    let ascii = drawn(&mut app, rows, IconStyle::Ascii);

    // Where a label starts with no glyph before it.
    let base = column(&off, "Rename\u{2026}");
    for t in [&off, &nerd, &ascii] {
        assert_eq!(column(t, "Rename\u{2026}"), base);
        assert_eq!(column(t, EXPLORER), base, "Explorer is no editor's");
    }
    assert_eq!(column(&off, VSCODE), base);
    assert_eq!(column(&off, VS), base);
    // The glyph and a space, or the tag padded to three and a space.
    for label in [VSCODE, VS] {
        assert_eq!(column(&nerd, label), base + 2, "{label}");
        assert_eq!(column(&ascii, label), base + 4, "{label}");
    }

    let line = screen_row(&nerd, VSCODE);
    assert!(line.contains('\u{f0a1e}'), "{line:?}");
    assert!(line.find('\u{f0a1e}') < line.find(VSCODE), "{line:?}");
    let line = screen_row(&nerd, VS);
    assert!(line.contains('\u{f0610}'), "{line:?}");
    assert!(line.find('\u{f0610}') < line.find(VS), "{line:?}");
    for plain in [EXPLORER, "Rename\u{2026}"] {
        let line = screen_row(&nerd, plain);
        assert!(
            !line.contains('\u{f0a1e}') && !line.contains('\u{f0610}'),
            "{line:?}"
        );
        let line = screen_row(&ascii, plain);
        assert!(!line.contains("VSC") && !line.contains("VS "), "{line:?}");
    }

    let line = screen_row(&ascii, VSCODE);
    assert!(line.contains("VSC Open in VS Code"), "{line:?}");
    let line = screen_row(&ascii, VS);
    assert!(line.contains("VS  Open in Visual Studio"), "{line:?}");
    assert!(!line.contains('\u{f0610}'), "{line:?}");

    for label in [VSCODE, VS] {
        let line = screen_row(&off, label);
        assert!(
            !line.contains('\u{f0a1e}') && !line.contains('\u{f0610}') && !line.contains("VSC"),
            "{line:?}"
        );
        assert!(!line.contains("VS  "), "{line:?}");
    }
}

/// `?` over a Cloud card adds no editor row for the checkout it sits
/// under: the menu decides those, and it offers none there.
#[test]
fn the_verbs_menu_over_a_cloud_card_adds_no_editor_row() {
    let dir = checkout(&["Api.sln"]);
    let mut app = rig(dir.path());
    select(&mut app, "cloud");
    let rows: Vec<String> = verbs::selection_menu(&app)
        .items
        .into_iter()
        .map(|i| i.label)
        .collect();
    for l in EDITOR_ROWS {
        assert!(!has(&rows, l), "`?` lists {l}: {rows:?}");
    }
}
