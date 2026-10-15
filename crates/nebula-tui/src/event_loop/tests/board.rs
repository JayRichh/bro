//! Storyboard B on the terminal board: the SCOPE CHIP, drops and the
//! move keys through core's drop verbs, the SHIP OVERLAY over a fake
//! ship runner (the queued jobs, answered by the test — never `gh` or a
//! push), the archive's worktree offer, the REVIEW UNIT and the cards'
//! facts.

use super::*;
use crate::app::{OrderItem, ReviewVerb};
use crate::event_loop::verbs;
use crate::launcher::Lane;
use crate::ship_view::{Form, ShipAnswer, ShipJob, ShipOutcome, Stage};
use kete_ado::ship::{
    Created, ForgeKind, MergeMethod, Merged, PrInfo, ShipError, ShipErrorKind, Suggestion,
};
use nebula_core::{AgentStatus, Entity, ProjectId, Worktree, WorktreeId};
use ratatui::layout::Rect;

/// `demo` with its root `main` (agent-1, read and done) and a linked
/// worktree `feat` at `feat_path` with `polish-nav` in it, finished and
/// unread: a Review card. On the board, every project.
fn bench_at(feat_path: std::path::PathBuf) -> App {
    let mut app = App::new();
    seed_tree(&mut app);
    hse(
        &mut app,
        ServerEvent::EntityUpserted {
            entity: Entity::Worktree(Worktree {
                id: WorktreeId("w2".into()),
                project_id: ProjectId("p1".into()),
                path: feat_path,
                branch: "feat".into(),
                is_main: false,
                sort_order: 1,
            }),
        },
    );
    seed_agent_in(&mut app, "a2", &WorktreeId("w2".into()));
    for a in app.tree.agents.iter_mut() {
        a.status = AgentStatus::Finished;
        a.status_changed_at = crate::app::now_ms() - 1_000;
        if a.id.0 == "a2" {
            a.name = "polish-nav".into();
            a.unseen = true;
        }
    }
    app.launcher.launcher_pane_at = crate::launcher::PaneSide::Bottom;
    app.launcher.launcher_kanban = true;
    app.launcher.kanban_all = true;
    app
}

fn bench() -> App {
    bench_at("/tmp/demo-feat".into())
}

fn agent<'a>(app: &'a mut App, id: &str) -> &'a mut nebula_core::Agent {
    app.tree
        .agents
        .iter_mut()
        .find(|a| a.id.0 == id)
        .expect("the session")
}

fn lane(app: &App, id: &str) -> Lane {
    let row = crate::launcher::row(app, &AgentId(id.into())).expect("a row");
    crate::launcher::lane_of(app, &row)
}

fn draw_at(app: &mut App, w: u16, h: u16) -> String {
    let mut terminal = Terminal::new(TestBackend::new(w, h)).unwrap();
    terminal.draw(|f| ui::draw(f, app)).unwrap();
    buffer_text(&terminal)
}

fn key(app: &mut App, code: KeyCode, mods: KeyModifiers) -> Vec<ClientRequest> {
    let mut out = Vec::new();
    press(app, code, mods, &mut out);
    out
}

fn mouse_at(app: &mut App, kind: MouseEventKind, x: u16, y: u16) -> Vec<ClientRequest> {
    let mut out = Vec::new();
    handle_terminal_event(
        app,
        crossterm::event::Event::Mouse(MouseEvent {
            kind,
            column: x,
            row: y,
            modifiers: KeyModifiers::NONE,
        }),
        &mut out,
    );
    out
}

fn rect_where(app: &App, f: impl Fn(&HitTarget) -> bool) -> Rect {
    app.chrome
        .hits
        .iter()
        .find_map(|(r, h)| f(h).then_some(*r))
        .expect("drawn")
}

fn card(app: &App, id: &str) -> Rect {
    rect_where(app, |h| matches!(h, HitTarget::KanbanCard(a) if a.0 == id))
}

/// The cursor onto board card `id`, as a step there lands it.
fn select_card(app: &mut App, id: &str) {
    let mut out = Vec::new();
    super::super::launcher::select(app, AgentId(id.into()), &mut out);
}

/// Hold board card `id` and let it go over lane `to`'s air.
fn drag_card_to_lane(app: &mut App, id: &str, to: Lane) -> Vec<ClientRequest> {
    let (w, h) = (200, 50);
    draw_at(app, w, h);
    let r = card(app, id);
    let mut out = mouse_at(
        app,
        MouseEventKind::Down(MouseButton::Left),
        r.x + 2,
        r.y + 2,
    );
    draw_at(app, w, h);
    let lane = rect_where(app, |h| *h == HitTarget::KanbanLane(to.index()));
    let air = (lane.x + 2, lane.bottom() - 2);
    mouse_at(app, MouseEventKind::Drag(MouseButton::Left), air.0, air.1);
    out.extend(mouse_at(
        app,
        MouseEventKind::Up(MouseButton::Left),
        air.0,
        air.1,
    ));
    out
}

fn ship_view(app: &App) -> &crate::ship_view::ShipView {
    match &app.modals.overlay {
        Some(Overlay::Ship(v)) => v,
        other => panic!("the ship overlay, got {other:?}"),
    }
}

/// The last job the overlay queued, taken off the queue.
fn take_job(app: &mut App) -> (u64, WorktreeId, ShipJob) {
    app.github.ship_queued.pop().expect("a ship job queued")
}

fn pr_info(number: u64, state: &str) -> PrInfo {
    PrInfo {
        forge: ForgeKind::GitHub,
        number,
        url: format!("https://github.com/o/r/pull/{number}"),
        title: "Polish the nav".into(),
        state: state.into(),
        draft: false,
        head: "feat".into(),
        base: "main".into(),
        head_sha: None,
    }
}

/// `feat` carrying an open pull request, #12.
fn with_open_pr(app: &mut App) {
    app.github.pull_requests.insert(
        WorktreeId("w2".into()),
        Some(crate::pull_request::PullRequest {
            number: 12,
            url: "https://github.com/o/r/pull/12".into(),
            title: "Polish the nav".into(),
            state: crate::pull_request::STATE_OPEN.into(),
            is_draft: false,
            head: "feat".into(),
            head_sha: String::new(),
            base: "main".into(),
            health: Default::default(),
            activity: Vec::new(),
        }),
    );
}

// ---- the SCOPE CHIP ----

/// FOCUS HERE on a card narrows the board to its worktree: the chip says
/// `all ▸ demo ▸ feat`, each lane's foot counts what it leaves out, the
/// ATTENTION STRIP and `.` keep to it; the chip's `✕` widens it back, Esc
/// clears it once the cursor has let go of the card, and `Tab` (the old
/// project/all switch) clears a narrowed scope before it flips anything.
#[test]
fn focus_here_narrows_the_board_and_the_chip_widens_it() {
    with_default_config(|| {
        let mut app = bench();
        // The board as Settings has it, so a save of the config keeps it.
        let mut cfg = crate::config::Config::load();
        cfg.kanban = true;
        cfg.kanban_scope = "all".into();
        assert!(save_config(&mut app, &cfg));
        // A running session on the root too, outside the scope to come.
        seed_agent_in(&mut app, "a3", &WorktreeId("w1".into()));
        agent(&mut app, "a3").status = AgentStatus::Running;
        draw_at(&mut app, 200, 50);
        select_card(&mut app, "a2");
        let mut out = Vec::new();
        verbs::run_on_selection(&mut app, crate::keymap::Action::FocusScope, &mut out);
        assert_eq!(
            app.launcher.scope,
            nebula_core::scope::Scope {
                repo: Some(ProjectId("p1".into())),
                worktree: Some(WorktreeId("w2".into())),
                ..Default::default()
            }
        );
        let lanes = crate::launcher::kanban(&app, app.launcher.kanban_all);
        let shown: Vec<&str> = lanes
            .iter()
            .flatten()
            .map(|r| r.agent.name.as_str())
            .collect();
        assert_eq!(shown, ["polish-nav"]);
        let text = draw_at(&mut app, 200, 50);
        assert!(text.contains("all ▸ demo ▸ feat @ ✕"), "{text}");
        assert!(text.contains("1 more outside scope"), "{text}");
        let running = crate::attention::tally(&app)
            .get(&crate::subagents::Tone::Running)
            .map_or(0, Vec::len);
        assert_eq!(running, 0, "a3 runs outside the scope");

        // The ✕: everything again.
        let x = rect_where(&app, |h| *h == HitTarget::ScopeClear);
        mouse_at(&mut app, MouseEventKind::Down(MouseButton::Left), x.x, x.y);
        assert!(app.launcher.scope.is_everything());
        assert!(app.launcher.kanban_all);

        // Esc: the aim first, then the scope.
        select_card(&mut app, "a2");
        verbs::run_on_selection(&mut app, crate::keymap::Action::FocusScope, &mut out);
        key(&mut app, KeyCode::Esc, KeyModifiers::NONE);
        assert!(
            !app.launcher.scope.is_everything(),
            "the first Esc lets the card go"
        );
        key(&mut app, KeyCode::Esc, KeyModifiers::NONE);
        assert!(
            app.launcher.scope.is_everything(),
            "the next clears the scope"
        );

        // Tab clears a narrowed scope before it flips the board.
        select_card(&mut app, "a2");
        verbs::run_on_selection(&mut app, crate::keymap::Action::FocusScope, &mut out);
        key(&mut app, KeyCode::Tab, KeyModifiers::NONE);
        assert!(app.launcher.scope.is_everything());
        assert!(app.launcher.kanban_all);
        key(&mut app, KeyCode::Tab, KeyModifiers::NONE);
        assert!(
            !app.launcher.kanban_all,
            "then this project, as before: {:?} {:?}",
            app.chrome.flash, app.launcher.scope
        );
        assert!(crate::config::Config::load().kanban_scope == "project");
        let text = draw_at(&mut app, 200, 50);
        assert!(text.contains("all ▸ demo @ ✕"), "the chip says it: {text}");
        // Esc clears that too, once the card is let go of.
        key(&mut app, KeyCode::Esc, KeyModifiers::NONE);
        key(&mut app, KeyCode::Esc, KeyModifiers::NONE);
        assert!(app.launcher.kanban_all, "everything again");
    });
}

/// A scope on one repo follows the PROJECT TAB to another, the way the
/// board's "this project" does, and a segment of the chip widens it to
/// that segment.
#[test]
fn a_scoped_repo_follows_the_tab_and_a_segment_widens_to_it() {
    with_default_config(|| {
        let mut app = bench();
        seed_other_project(&mut app);
        app.launcher.scope = nebula_core::scope::Scope {
            repo: Some(ProjectId("p1".into())),
            worktree: Some(WorktreeId("w2".into())),
            ..Default::default()
        };
        draw_at(&mut app, 200, 50);
        let demo = rect_where(&app, |h| *h == HitTarget::ScopeChip(1));
        mouse_at(
            &mut app,
            MouseEventKind::Down(MouseButton::Left),
            demo.x,
            demo.y,
        );
        assert_eq!(app.launcher.scope.repo, Some(ProjectId("p1".into())));
        assert_eq!(app.launcher.scope.worktree, None, "widened to the repo");
        let mut out = Vec::new();
        super::super::launcher::open_tab(&mut app, &ProjectId("p9".into()), &mut out);
        assert_eq!(app.launcher.scope.repo, Some(ProjectId("p9".into())));
    });
}

/// The quick prompt aims at the scope's worktree with nothing under the
/// cursor, before the root.
#[test]
fn the_quick_prompt_aims_at_the_scoped_worktree() {
    let mut app = bench();
    app.launcher.launcher_unaimed = true;
    let p1 = ProjectId("p1".into());
    assert_eq!(
        crate::launcher::launch_checkout(&app, &p1),
        Some(WorktreeId("w1".into()))
    );
    app.launcher.scope.worktree = Some(WorktreeId("w2".into()));
    assert_eq!(
        crate::launcher::launch_checkout(&app, &p1),
        Some(WorktreeId("w2".into()))
    );
}

// ---- drops, the move keys and the SHIP OVERLAY ----

/// A Review card let go on Ship opens the PULL REQUEST COMPOSER, prefilled
/// at once and from the commits once read; Enter ships it off the loop.
/// A sign-in failure stays on the form with its fix — a terminal running
/// `gh auth login` — and the composer opened again by `Shift+→` ships;
/// what lands moves the card to Ship once read.
#[test]
fn a_review_card_dropped_on_ship_opens_the_composer_and_ships() {
    with_default_config(|| {
        let mut app = bench();
        // Its checkout has work no pull request carries.
        note_worktree_changes(&mut app, WorktreeId("w2".into()), Some(2));
        assert_eq!(lane(&app, "a2"), Lane::Review);
        // Pressing the card reads it (marked seen, attached); letting it
        // go on Ship opens the composer, judged as it was picked up.
        let out = drag_card_to_lane(&mut app, "a2", Lane::Ship);
        assert!(
            !out.iter()
                .any(|r| matches!(r, ClientRequest::CreateTerminal { .. })),
            "{out:?}"
        );
        let view = ship_view(&app);
        assert!(view.is_compose());
        assert_eq!(view.worktree, WorktreeId("w2".into()));
        let (ticket, _, job) = take_job(&mut app);
        assert!(matches!(job, ShipJob::Suggest { .. }), "{job:?}");
        let Form::Compose(c) = &ship_view(&app).form else {
            unreachable!()
        };
        assert_eq!(c.title.as_str(), "polish-nav", "the session's name at once");

        // The read of the commits lands: it fills the form.
        let mut out = Vec::new();
        ship::land_ship(
            &mut app,
            ShipAnswer {
                ticket,
                worktree: WorktreeId("w2".into()),
                outcome: ShipOutcome::Suggested {
                    suggestion: Suggestion {
                        title: "Polish the nav".into(),
                        body: "## Changes\n- polish".into(),
                    },
                    base: Some("main".into()),
                },
            },
            &mut out,
        );
        let Form::Compose(c) = &ship_view(&app).form else {
            unreachable!()
        };
        assert_eq!(c.title.as_str(), "Polish the nav");
        assert_eq!(c.base.as_str(), "main");
        assert!(draw_at(&mut app, 120, 40).contains("Pull request: feat"));

        // Enter: the push and create, off the loop.
        key(&mut app, KeyCode::Enter, KeyModifiers::NONE);
        assert_eq!(ship_view(&app).stage, Stage::Working);
        let (ticket, worktree, job) = take_job(&mut app);
        assert_eq!(worktree, WorktreeId("w2".into()));
        let ShipJob::Create { pr, .. } = job else {
            panic!("a create, got {job:?}");
        };
        assert_eq!(pr.title, "Polish the nav");
        assert_eq!(pr.base.as_deref(), Some("main"));
        assert!(app.github.ship_inflight.contains(&worktree));

        // Not signed in: the form says so and offers the sign-in.
        ship::land_ship(
            &mut app,
            ShipAnswer {
                ticket,
                worktree: worktree.clone(),
                outcome: ShipOutcome::Created(Err(ShipError {
                    kind: ShipErrorKind::SignIn,
                    forge: Some(ForgeKind::GitHub),
                    message: kete_ado::ship::GH_SIGN_IN.into(),
                    url: None,
                })),
            },
            &mut out,
        );
        assert!(matches!(ship_view(&app).stage, Stage::Failed(_)));
        assert!(!app.github.ship_inflight.contains(&worktree));
        let text = draw_at(&mut app, 120, 40);
        assert!(text.contains("gh auth login"), "{text}");
        let mut out = key(&mut app, KeyCode::Enter, KeyModifiers::NONE);
        assert!(app.modals.overlay.is_none());
        assert!(
            out.iter().any(|r| matches!(
                r,
                ClientRequest::CreateTerminal { worktree, .. } if worktree.0 == "w2"
            )),
            "{out:?}"
        );
        assert!(app
            .requests
            .pending
            .values()
            .any(|p| matches!(p, PendingIntent::SignIn { line } if line == "gh auth login")));

        // The move key is the drop's twin.
        draw_at(&mut app, 200, 50);
        select_card(&mut app, "a2");
        agent(&mut app, "a2").unseen = true;
        key(&mut app, KeyCode::Right, KeyModifiers::SHIFT);
        assert!(ship_view(&app).is_compose());
        let _suggest = take_job(&mut app);
        key(&mut app, KeyCode::Enter, KeyModifiers::NONE);
        let (ticket, worktree, _) = take_job(&mut app);
        // A stale answer, for a form long gone, only says itself.
        ship::land_ship(
            &mut app,
            ShipAnswer {
                ticket: ticket - 1,
                worktree: worktree.clone(),
                outcome: ShipOutcome::Suggested {
                    suggestion: Suggestion::default(),
                    base: None,
                },
            },
            &mut out,
        );
        assert_eq!(ship_view(&app).stage, Stage::Working);
        ship::land_ship(
            &mut app,
            ShipAnswer {
                ticket,
                worktree: worktree.clone(),
                outcome: ShipOutcome::Created(Ok(Created {
                    pr: pr_info(12, "open"),
                    existing: false,
                    note: None,
                })),
            },
            &mut out,
        );
        assert!(app.modals.overlay.is_none(), "{:?}", app.modals.overlay);
        assert_eq!(
            app.chrome.flash.as_deref(),
            Some("opened #12 Polish the nav")
        );
        assert_eq!(
            app.selected_session().map(|a| a.id.0),
            Some("a2".into()),
            "back on the card"
        );
        agent(&mut app, "a2").unseen = false;
        assert_eq!(lane(&app, "a2"), Lane::Ship, "its pull request holds it");
        // The card says so on its border.
        let text = draw_at(&mut app, 200, 50);
        assert!(text.contains("#12 ready"), "{text}");
    });
}

/// A Ship card let go on Done asks to merge: the method and the remote
/// branch picked, Enter merges off the loop, and what lands moves the card
/// to Done. Azure DevOps keeping the merge for its page says so, with the
/// page as the fix.
#[test]
fn a_ship_card_dropped_on_done_merges_after_its_confirm() {
    with_default_config(|| {
        let mut app = bench();
        agent(&mut app, "a2").unseen = false;
        with_open_pr(&mut app);
        assert_eq!(lane(&app, "a2"), Lane::Ship);
        drag_card_to_lane(&mut app, "a2", Lane::Done);
        let view = ship_view(&app);
        let Form::Merge(m) = &view.form else {
            panic!("the merge confirm, got {:?}", view.form);
        };
        assert_eq!(m.number, 12);
        assert!(
            app.github.ship_queued.is_empty(),
            "nothing runs before Enter"
        );
        let text = draw_at(&mut app, 120, 40);
        assert!(text.contains("Merge #12: feat"), "{text}");
        key(&mut app, KeyCode::Right, KeyModifiers::NONE);
        key(&mut app, KeyCode::Char(' '), KeyModifiers::NONE);
        key(&mut app, KeyCode::Enter, KeyModifiers::NONE);
        let (ticket, worktree, job) = take_job(&mut app);
        let ShipJob::Merge { opts, .. } = job else {
            panic!("a merge, got {job:?}");
        };
        assert_eq!(opts.method, MergeMethod::Merge);
        assert!(!opts.delete_branch);

        // Azure DevOps keeps it: the form says so, the page is the fix.
        let mut out = Vec::new();
        ship::land_ship(
            &mut app,
            ShipAnswer {
                ticket,
                worktree: worktree.clone(),
                outcome: ShipOutcome::Merged(Err(ShipError {
                    kind: ShipErrorKind::MergeInAdo,
                    forge: Some(ForgeKind::Ado),
                    message: "Policies keep it".into(),
                    url: Some("https://dev.azure.com/o/p/_git/r/pullrequest/12".into()),
                })),
            },
            &mut out,
        );
        assert!(matches!(ship_view(&app).stage, Stage::Failed(_)));
        assert!(draw_at(&mut app, 120, 40).contains("finish it on the pull request's page"));
        key(&mut app, KeyCode::Esc, KeyModifiers::NONE);
        assert_eq!(ship_view(&app).stage, Stage::Editing, "back to the form");
        key(&mut app, KeyCode::Enter, KeyModifiers::NONE);
        let (ticket, worktree, _) = take_job(&mut app);
        ship::land_ship(
            &mut app,
            ShipAnswer {
                ticket,
                worktree,
                outcome: ShipOutcome::Merged(Ok(Merged {
                    pr: pr_info(12, "merged"),
                    state: "merged".into(),
                    branch_deleted: false,
                    note: None,
                })),
            },
            &mut out,
        );
        assert!(app.modals.overlay.is_none());
        assert_eq!(
            app.chrome.flash.as_deref(),
            Some("merged #12 Polish the nav")
        );
        assert_eq!(lane(&app, "a2"), Lane::Done);
    });
}

/// A drop the agent owns is refused: while the card is held over the lane
/// its title says why, dimmed, and letting go says it again.
#[test]
fn a_refused_lane_names_why_while_the_card_is_held() {
    with_default_config(|| {
        let mut app = bench();
        agent(&mut app, "a2").status = AgentStatus::Running;
        let (w, h) = (200, 50);
        draw_at(&mut app, w, h);
        let r = card(&app, "a2");
        mouse_at(
            &mut app,
            MouseEventKind::Down(MouseButton::Left),
            r.x + 2,
            r.y + 2,
        );
        draw_at(&mut app, w, h);
        let done = rect_where(&app, |h| *h == HitTarget::KanbanLane(Lane::Done.index()));
        let air = (done.x + 2, done.bottom() - 2);
        mouse_at(
            &mut app,
            MouseEventKind::Drag(MouseButton::Left),
            air.0,
            air.1,
        );
        assert!(matches!(
            app.launcher.order_drag.as_ref().map(|d| &d.item),
            Some(OrderItem::Board { .. })
        ));
        let text = draw_at(&mut app, w, h);
        assert!(text.contains("Running is the agent"), "{text}");
        let out = mouse_at(
            &mut app,
            MouseEventKind::Up(MouseButton::Left),
            air.0,
            air.1,
        );
        assert!(out.is_empty());
        assert!(app
            .chrome
            .flash
            .as_deref()
            .is_some_and(|f| f.contains("Running is the agent")));
        // The keys refuse the same way; nothing is left of Running.
        select_card(&mut app, "a2");
        key(&mut app, KeyCode::Right, KeyModifiers::SHIFT);
        assert!(app.modals.overlay.is_none());
        assert!(app
            .chrome
            .flash
            .as_deref()
            .is_some_and(|f| f.contains("Only the agent")));
        key(&mut app, KeyCode::Left, KeyModifiers::SHIFT);
        assert_eq!(app.chrome.flash.as_deref(), Some("no lane left of Running"));
    });
}

/// Past Done, a card is archived — and the last live card of a linked
/// worktree offers that worktree's safe delete in the same dialog. The
/// root checkout is never offered.
#[test]
fn archiving_a_done_card_offers_its_linked_worktree_only() {
    with_default_config(|| {
        let mut app = bench();
        agent(&mut app, "a2").unseen = false;
        assert_eq!(lane(&app, "a2"), Lane::Done);
        draw_at(&mut app, 200, 50);
        select_card(&mut app, "a2");
        key(&mut app, KeyCode::Right, KeyModifiers::SHIFT);
        let Some(Overlay::Confirm(c)) = &app.modals.overlay else {
            panic!("the archive's confirm, got {:?}", app.modals.overlay);
        };
        assert_eq!(
            c.action,
            PendingAction::ThenDeleteWorktree {
                first: Box::new(PendingAction::ArchiveAgent(AgentId("a2".into()))),
                worktree: WorktreeId("w2".into()),
                offered: true,
            }
        );
        assert!(c.message.contains("worktree 'feat'"), "{}", c.message);
        app.modals.overlay = None;

        // agent-1 on the root: the archive alone.
        draw_at(&mut app, 200, 50);
        select_card(&mut app, "a1");
        assert_eq!(lane(&app, "a1"), Lane::Done);
        key(&mut app, KeyCode::Right, KeyModifiers::SHIFT);
        assert!(
            !matches!(
                &app.modals.overlay,
                Some(Overlay::Confirm(c)) if matches!(c.action, PendingAction::ThenDeleteWorktree { .. })
            ),
            "{:?}",
            app.modals.overlay
        );
    });
}

/// The registry's verbs are on a card's menu where they mean something:
/// Create pull request without one open, Merge with one, Focus here
/// always; `?` lists them too.
#[test]
fn the_ship_verbs_are_on_the_cards_menus() {
    with_default_config(|| {
        let mut app = bench();
        let t = verbs::Target::Session(AgentId("a2".into()));
        let labels = |app: &App| -> Vec<String> {
            verbs::menu_for(app, &t)
                .into_iter()
                .map(|i| i.label)
                .collect()
        };
        let l = labels(&app);
        assert!(l.iter().any(|s| s == "Create pull request…"), "{l:?}");
        assert!(!l.iter().any(|s| s == "Merge pull request…"), "{l:?}");
        assert!(l.iter().any(|s| s == "Focus here"), "{l:?}");
        with_open_pr(&mut app);
        let l = labels(&app);
        assert!(!l.iter().any(|s| s == "Create pull request…"), "{l:?}");
        assert!(l.iter().any(|s| s == "Merge pull request…"), "{l:?}");
        // The menu row runs the verb.
        let item = verbs::menu_for(&app, &t)
            .into_iter()
            .find(|i| i.label == "Merge pull request…")
            .unwrap();
        let mut out = Vec::new();
        run_menu_action(&mut app, item.action, &mut out);
        assert!(matches!(ship_view(&app).form, Form::Merge(_)));
    });
}

// ---- the REVIEW UNIT ----

/// A git checkout on `feat`, one commit past `main`, with an edit on top.
fn feat_repo(dir: &tempfile::TempDir) -> std::path::PathBuf {
    let repo = test_repo(dir);
    let git = |args: &[&str]| {
        let out = std::process::Command::new("git")
            .arg("-C")
            .arg(&repo)
            .args(args)
            .output()
            .expect("git");
        assert!(out.status.success(), "{args:?}");
    };
    git(&["checkout", "-q", "-b", "feat"]);
    std::fs::write(repo.join("b.txt"), "new\n").unwrap();
    git(&["add", "."]);
    git(&["commit", "-q", "-m", "add b"]);
    std::fs::write(repo.join("a.txt"), "edited\n").unwrap();
    repo
}

/// Enter on a Review card docks its worktree's diff beside the board with
/// the verb row; Esc lands back on the card; the PR verb opens the composer
/// over the dock and its Esc puts the dock back; a click on the board
/// beside it closes it onto the card.
#[test]
fn enter_on_a_review_card_docks_its_diff_beside_the_board() {
    with_default_config(|| {
        let dir = tempfile::tempdir().unwrap();
        let mut app = bench_at(feat_repo(&dir));
        // The poll found its edit: read or not, the card is in Review.
        note_worktree_changes(&mut app, WorktreeId("w2".into()), Some(2));
        draw_at(&mut app, 200, 50);
        select_card(&mut app, "a2");
        assert!(super::super::review::on_review_card(&app));
        key(&mut app, KeyCode::Enter, KeyModifiers::NONE);
        let Some(Overlay::Diff(view)) = &app.modals.overlay else {
            panic!("the docked diff, got {:?}", app.modals.overlay);
        };
        assert_eq!(view.review.as_ref().map(|r| r.agent.0.as_str()), Some("a2"));
        let text = draw_at(&mut app, 200, 50);
        let dock = app.launcher.review_dock;
        assert!(dock.width > 0 && dock.x > 0, "{dock:?}");
        assert!(text.contains("Space reply"), "{text}");
        assert!(text.contains("open PR"), "{text}");
        assert!(text.contains("Review"), "the lanes stay beside it: {text}");
        let Some(Overlay::Diff(view)) = &app.modals.overlay else {
            unreachable!()
        };
        assert!(view.area.x >= dock.x, "docked, not centered");
        let verbs: Vec<ReviewVerb> = view
            .review
            .as_ref()
            .unwrap()
            .verbs
            .iter()
            .map(|v| v.1)
            .collect();
        assert_eq!(verbs[0], ReviewVerb::Reply);

        // Esc: the board, on the card.
        key(&mut app, KeyCode::Esc, KeyModifiers::NONE);
        assert!(app.modals.overlay.is_none());
        assert_eq!(app.selected_session().map(|a| a.id.0), Some("a2".into()));

        // ⌥p with no pull request: the composer over the dock.
        key(&mut app, KeyCode::Enter, KeyModifiers::NONE);
        key(&mut app, KeyCode::Char('p'), KeyModifiers::ALT);
        let view = ship_view(&app);
        assert!(view.back.is_some(), "it stands over the review");
        key(&mut app, KeyCode::Esc, KeyModifiers::NONE);
        assert!(
            matches!(&app.modals.overlay, Some(Overlay::Diff(v)) if v.review.is_some()),
            "the dock comes back"
        );

        // A click on a verb of the row is that verb: the PR, the composer.
        draw_at(&mut app, 200, 50);
        let Some(Overlay::Diff(view)) = &app.modals.overlay else {
            unreachable!()
        };
        let pr = view
            .review
            .as_ref()
            .and_then(|r| r.verbs.iter().find(|v| v.1 == ReviewVerb::Pr))
            .map(|v| v.0)
            .expect("the PR verb on the row");
        mouse_at(
            &mut app,
            MouseEventKind::Down(MouseButton::Left),
            pr.x + 1,
            pr.y,
        );
        assert!(
            ship_view(&app).back.is_some(),
            "the composer, over the dock"
        );
        key(&mut app, KeyCode::Esc, KeyModifiers::NONE);

        // A click on the board beside the dock closes it onto the card.
        draw_at(&mut app, 200, 50);
        mouse_at(&mut app, MouseEventKind::Down(MouseButton::Left), 2, 20);
        assert!(app.modals.overlay.is_none());
        assert_eq!(app.selected_session().map(|a| a.id.0), Some("a2".into()));
    });
}

/// `⇧K` from a diff opened on a board card lands back on that card, not
/// the board's first: the position is the card's, kept by its id.
#[test]
fn shift_k_from_a_cards_diff_keeps_the_boards_place() {
    with_default_config(|| {
        let dir = tempfile::tempdir().unwrap();
        let mut app = bench_at(feat_repo(&dir));
        agent(&mut app, "a2").unseen = false;
        // agent-1 is the board's first card: Running, the leftmost lane.
        agent(&mut app, "a1").status = AgentStatus::Running;
        draw_at(&mut app, 200, 50);
        select_card(&mut app, "a2");
        key(&mut app, KeyCode::Char('g'), KeyModifiers::NONE);
        assert!(
            matches!(&app.modals.overlay, Some(Overlay::Diff(v)) if v.from_card.is_some()),
            "{:?}",
            app.modals.overlay
        );
        key(&mut app, KeyCode::Char('K'), KeyModifiers::SHIFT);
        assert!(app.modals.overlay.is_none());
        assert!(app.launcher.launcher_kanban);
        assert_eq!(app.selected_session().map(|a| a.id.0), Some("a2".into()));
    });
}

// ---- drawing ----

/// The board with a narrowed scope, the docked review and the ship forms
/// draw at any size without a panic, the chip giving way first.
#[test]
fn the_board_pieces_draw_at_small_sizes() {
    with_default_config(|| {
        for (w, h) in [(24, 6), (40, 10), (60, 16), (90, 24)] {
            let mut app = bench();
            with_open_pr(&mut app);
            app.launcher.scope.worktree = Some(WorktreeId("w2".into()));
            draw_at(&mut app, w, h);
            let mut view =
                crate::app::DiffView::new("/nonexistent".into(), "feat".into(), Vec::new(), true);
            view.review = Some(Box::new(crate::app::ReviewDock {
                agent: AgentId("a2".into()),
                worktree: WorktreeId("w2".into()),
                verbs: Vec::new(),
                dock: Rect::default(),
            }));
            app.modals.overlay = Some(Overlay::Diff(view));
            draw_at(&mut app, w, h);
            super::super::ship::open_merge(&mut app, &WorktreeId("w2".into()), None, None);
            draw_at(&mut app, w, h);
            app.modals.overlay = None;
            app.github.pull_requests.clear();
            super::super::ship::open_composer(&mut app, &WorktreeId("w2".into()), None, None);
            draw_at(&mut app, w, h);
            if let Some(Overlay::Ship(v)) = &mut app.modals.overlay {
                v.stage = Stage::Failed(ShipError {
                    kind: ShipErrorKind::NoTool,
                    forge: Some(ForgeKind::GitHub),
                    message:
                        "The GitHub CLI (gh) is not installed: install it and run `gh auth login`."
                            .into(),
                    url: None,
                });
            }
            draw_at(&mut app, w, h);
        }
    });
}

/// A board card names the commits its branch has since the base.
#[test]
fn a_board_card_names_its_commits_since_the_base() {
    use nebula_core::review_base::{BaseRef, BaseStanding};
    let mut app = bench();
    agent(&mut app, "a2").unseen = false;
    let w = app
        .tree
        .worktrees
        .iter()
        .find(|w| w.id.0 == "w2")
        .cloned()
        .unwrap();
    note_worktree_base(
        &mut app,
        &w.id,
        Some((
            w.branch.clone(),
            BaseRef::Base("origin/main".into()),
            BaseStanding {
                ahead_of_base: 3,
                on_default_branch: false,
            },
        )),
    );
    let text = draw_at(&mut app, 200, 50);
    assert!(text.contains("3 commits"), "{text}");
}

/// The chip's `@`: a picker of the accounts the board's sessions run on;
/// a pick narrows the scope to that account, the place kept, and the
/// chip names it; Any account widens it back.
#[test]
fn the_chip_picks_an_account() {
    with_default_config(|| {
        let mut app = bench();
        agent(&mut app, "a2").profile = Some("work".into());
        let text = draw_at(&mut app, 200, 50);
        assert!(text.contains("all @"), "{text}");
        let at = rect_where(&app, |h| *h == HitTarget::ScopeAccount);
        mouse_at(
            &mut app,
            MouseEventKind::Down(MouseButton::Left),
            at.x,
            at.y,
        );
        let items = match &app.modals.overlay {
            Some(Overlay::Menu(m)) => m.items.clone(),
            other => panic!("no picker: {other:?}"),
        };
        let labels: Vec<&str> = items.iter().map(|i| i.label.trim()).collect();
        assert_eq!(labels[0], "\u{2713} Any account");
        assert!(labels.contains(&"@work"), "{labels:?}");
        assert_eq!(labels.len(), 3, "one row per account in use: {labels:?}");
        let work = items.iter().find(|i| i.label.trim() == "@work").unwrap();
        let mut out = Vec::new();
        let pick = work.action.clone();
        app.modals.overlay = None;
        crate::event_loop::commands::run_menu_action(&mut app, pick, &mut out);
        assert_eq!(app.launcher.scope.account.as_deref(), Some("work"));
        let lanes = crate::launcher::kanban(&app, app.launcher.kanban_all);
        let shown: Vec<&str> = lanes
            .iter()
            .flatten()
            .map(|r| r.agent.name.as_str())
            .collect();
        assert_eq!(shown, ["polish-nav"]);
        let text = draw_at(&mut app, 200, 50);
        assert!(text.contains("all ▸ @work ✕"), "{text}");

        // The account's segment opens the picker again; Any account
        // takes the account off.
        let at = rect_where(&app, |h| *h == HitTarget::ScopeAccount);
        mouse_at(
            &mut app,
            MouseEventKind::Down(MouseButton::Left),
            at.x,
            at.y,
        );
        let any = match &app.modals.overlay {
            Some(Overlay::Menu(m)) => m.items[0].action.clone(),
            other => panic!("no picker: {other:?}"),
        };
        app.modals.overlay = None;
        crate::event_loop::commands::run_menu_action(&mut app, any, &mut out);
        assert!(app.launcher.scope.account.is_none());

        // The palette's row does what the click does.
        run_action(&mut app, crate::keymap::Action::ScopeAccount, &mut out);
        assert!(matches!(app.modals.overlay, Some(Overlay::Menu(_))));

        // Scoped to the root checkout, only its sessions' accounts.
        app.modals.overlay = None;
        app.launcher.scope.worktree = Some(WorktreeId("w1".into()));
        run_action(&mut app, crate::keymap::Action::ScopeAccount, &mut out);
        let labels: Vec<String> = match &app.modals.overlay {
            Some(Overlay::Menu(m)) => m.items.iter().map(|i| i.label.trim().to_string()).collect(),
            other => panic!("no picker: {other:?}"),
        };
        assert_eq!(labels.len(), 2, "{labels:?}");
        assert!(!labels.contains(&"@work".to_string()), "{labels:?}");
    });
}
