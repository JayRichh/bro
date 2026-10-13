//! The ORCHESTRATOR VIEW (F10), its start prompt and strip, the REVIEW
//! MODAL's tabs, and the daemon's ReviewOpened.

use super::*;
use crate::app::Focus;
use crossterm::event::{KeyEvent, KeyModifiers};
use nebula_core::{Agent, AgentRole, AgentStatus, Entity, ReviewTabKind, WorktreeId};

fn agent(id: &str, role: AgentRole, status: AgentStatus) -> Agent {
    Agent {
        parent: None,
        profile: None,
        id: AgentId(id.into()),
        worktree_id: WorktreeId("w1".into()),
        name: id.into(),
        status,
        archived: false,
        archived_at: 0,
        unseen: false,
        kind: nebula_core::AgentKind::Claude,
        custom_harness: None,
        model: None,
        effort: None,
        session_id: None,
        cloud_session_id: None,
        sort_order: 0,
        status_changed_at: 0,
        alive: true,
        issue_url: None,
        role,
        recent_prompts: Vec::new(),
    }
}

fn upsert(app: &mut App, a: Agent) {
    hse(
        app,
        ServerEvent::EntityUpserted {
            entity: Entity::Agent(a),
        },
    );
}

/// The seeded tree (worker `a1`), an orchestrator `orch`, a worker `ask`
/// waiting on the user and one `run` mid-turn.
fn app_with_orchestrator() -> App {
    let mut app = App::new();
    seed_tree(&mut app);
    upsert(
        &mut app,
        agent("orch", AgentRole::Orchestrator, AgentStatus::Fresh),
    );
    upsert(
        &mut app,
        agent("ask", AgentRole::Worker, AgentStatus::NeedsFeedback),
    );
    upsert(
        &mut app,
        agent("run", AgentRole::Worker, AgentStatus::Running),
    );
    app
}

fn key(app: &mut App, code: KeyCode, out: &mut Vec<ClientRequest>) {
    handle_key(app, KeyEvent::new(code, KeyModifiers::NONE), out);
    orchestrator::settle_review(app, out);
}

fn pane_on(app: &App) -> Option<SessionRef> {
    app.pane.term.as_ref().map(|t| t.sref.clone())
}

fn orch() -> SessionRef {
    SessionRef::Agent(AgentId("orch".into()))
}

fn screen(app: &mut App, w: u16, h: u16) -> String {
    let mut terminal = Terminal::new(TestBackend::new(w, h)).unwrap();
    terminal.draw(|f| ui::draw(f, app)).unwrap();
    terminal
        .backend()
        .buffer()
        .content()
        .iter()
        .map(|c| c.symbol())
        .collect()
}

#[test]
fn f10_with_no_orchestrator_opens_the_start_prompt_and_enter_starts_one() {
    with_default_config(|| {
        let mut app = App::new();
        seed_tree(&mut app);
        let mut out = Vec::new();
        key(&mut app, KeyCode::F(10), &mut out);
        assert!(
            matches!(app.modals.overlay, Some(Overlay::OrchestratorStart(_))),
            "{:?}",
            app.modals.overlay
        );
        // The next harness, then Enter.
        key(&mut app, KeyCode::Right, &mut out);
        key(&mut app, KeyCode::Enter, &mut out);
        assert!(app.modals.overlay.is_none());
        let started = out.iter().find_map(|r| match r {
            ClientRequest::StartOrchestrator {
                req_id,
                kind,
                profile,
                project,
                replace,
                ..
            } => Some((*req_id, *kind, profile.clone(), project.clone(), *replace)),
            _ => None,
        });
        let (req_id, kind, profile, project, replace) = started.expect("StartOrchestrator sent");
        assert_eq!(kind, nebula_core::AgentKind::Codex);
        assert_eq!((profile, project, replace), (None, None, false));

        // The Ack opens the view on the created orchestrator.
        out.clear();
        hse(
            &mut app,
            ServerEvent::Ack {
                req_id,
                created: Some(nebula_core::EntityId::Agent(AgentId("orch".into()))),
            },
        );
        upsert(
            &mut app,
            agent("orch", AgentRole::Orchestrator, AgentStatus::Fresh),
        );
        assert!(crate::orchestrator_view::active(&app));
        assert_eq!(pane_on(&app), Some(orch()));
        assert!(app.pane.term_locked && app.pane.collapsed);
    });
}

/// A protocol-53 daemon (the one `kete upgrade` leaves running while
/// sessions are live) cannot decode the provisioning requests and would drop
/// the connection: F10 and the card menu say so and send nothing.
#[test]
fn an_older_daemon_is_never_sent_a_provisioning_request() {
    with_default_config(|| {
        let mut app = App::new();
        seed_tree(&mut app);
        app.orch.daemon_protocol = Some(53);
        let mut out = Vec::new();
        key(&mut app, KeyCode::F(10), &mut out);
        assert!(app.modals.overlay.is_none(), "no start prompt");
        assert_eq!(
            app.chrome.flash.as_deref(),
            Some(crate::orchestrator_view::DAEMON_TOO_OLD)
        );
        run_menu_action(
            &mut app,
            MenuAction::MakeOrchestrator(AgentId("a1".into())),
            &mut out,
        );
        run_menu_action(&mut app, MenuAction::ClearOrchestrator, &mut out);
        assert!(
            !out.iter().any(|r| matches!(
                r,
                ClientRequest::StartOrchestrator { .. }
                    | ClientRequest::SetOrchestrator { .. }
                    | ClientRequest::ClearOrchestrator { .. }
            )),
            "{out:?}"
        );

        // On 54 the prompt opens.
        app.orch.daemon_protocol = Some(54);
        key(&mut app, KeyCode::F(10), &mut out);
        assert!(matches!(
            app.modals.overlay,
            Some(Overlay::OrchestratorStart(_))
        ));
    });
}

#[test]
fn f10_opens_the_view_on_the_orchestrator_and_again_from_inside_it_leaves() {
    with_default_config(|| {
        let mut app = app_with_orchestrator();
        let mut out = Vec::new();
        key(&mut app, KeyCode::F(10), &mut out);
        assert!(crate::orchestrator_view::active(&app));
        assert_eq!(pane_on(&app), Some(orch()));
        assert_eq!(app.nav.focus, Focus::Terminal);
        assert!(app.pane.term_locked, "typing goes to the orchestrator");

        // F10 reaches through the locked pane.
        key(&mut app, KeyCode::F(10), &mut out);
        assert!(!crate::orchestrator_view::active(&app));
        assert!(!app.pane.collapsed, "back to the grid");
    });
}

/// In a locked TERMINAL tab F10 is the shell program's (htop's and mc's
/// quit), never the orchestrator view's.
#[test]
fn f10_in_a_locked_terminal_tab_goes_to_the_shell() {
    with_default_config(|| {
        let mut app = app_with_orchestrator();
        let mut out = Vec::new();
        let term = SessionRef::Terminal(nebula_core::TerminalId("t1".into()));
        app.pane.term = Some(AttachedTerm::new(term.clone(), 80, 24));
        app.nav.focus = Focus::Terminal;
        app.pane.term_locked = true;
        key(&mut app, KeyCode::F(10), &mut out);
        assert!(!crate::orchestrator_view::active(&app));
        assert!(app.modals.overlay.is_none());
        assert_eq!(pane_on(&app), Some(term.clone()));
        assert!(
            out.iter()
                .any(|r| matches!(r, ClientRequest::Input { session, .. } if *session == term)),
            "{out:?}"
        );
    });
}

#[test]
fn the_strip_shows_every_other_session_in_attention_order_and_collapses_when_narrow() {
    with_default_config(|| {
        let mut app = app_with_orchestrator();
        let mut out = Vec::new();
        key(&mut app, KeyCode::F(10), &mut out);
        let names: Vec<String> = crate::orchestrator_view::chips(&app)
            .into_iter()
            .map(|c| c.name)
            .collect();
        assert_eq!(
            names,
            ["ask", "run", "agent-1"],
            "the orchestrator is not a chip"
        );

        let wide = screen(&mut app, 140, 30);
        assert!(wide.contains(" orchestrator "), "{wide}");
        let at = |s: &str| wide.find(s).unwrap_or_else(|| panic!("{s} missing"));
        assert!(at(" ask ") < at(" run ") && at(" run ") < at(" agent-1 "));
        let chips: Vec<_> = app
            .chrome
            .hits
            .iter()
            .filter_map(|(_, h)| match h {
                HitTarget::OrchestratorChip(s) => Some(s.clone()),
                _ => None,
            })
            .collect();
        assert_eq!(chips.len(), 3, "a click target per chip");

        let narrow = screen(&mut app, 50, 30);
        assert!(narrow.contains("3 sessions · 1 need you"), "{narrow}");
    });
}

#[test]
fn a_chip_click_reviews_that_session_and_closing_puts_the_orchestrator_back() {
    with_default_config(|| {
        let mut app = app_with_orchestrator();
        let mut out = Vec::new();
        key(&mut app, KeyCode::F(10), &mut out);
        screen(&mut app, 140, 30);
        let (rect, _) = app
            .chrome
            .hits
            .iter()
            .find(|(_, h)| {
                *h == HitTarget::OrchestratorChip(SessionRef::Agent(AgentId("run".into())))
            })
            .cloned()
            .expect("run's chip");
        handle_mouse(
            &mut app,
            MouseEvent {
                kind: MouseEventKind::Down(MouseButton::Left),
                column: rect.x,
                row: rect.y,
                modifiers: KeyModifiers::NONE,
            },
            &mut out,
        );
        orchestrator::settle_review(&mut app, &mut out);
        let Some(Overlay::Review(review)) = &app.modals.overlay else {
            panic!("no review: {:?}", app.modals.overlay);
        };
        assert_eq!(review.sessions, [SessionRef::Agent(AgentId("run".into()))]);
        assert_eq!(review.opener, None);
        assert_eq!(
            pane_on(&app),
            Some(SessionRef::Agent(AgentId("run".into()))),
            "the Terminal tab is run's own pane"
        );
        let shown = screen(&mut app, 140, 30);
        assert!(shown.contains("Review · run"), "named, not an id: {shown}");

        key(&mut app, KeyCode::Esc, &mut out);
        assert!(app.modals.overlay.is_none());
        assert_eq!(
            pane_on(&app),
            Some(orch()),
            "the pane is back on the orchestrator"
        );
    });
}

#[test]
fn the_unlock_hatch_steps_out_to_the_chips_and_enter_reviews_one() {
    with_default_config(|| {
        let mut app = app_with_orchestrator();
        let mut out = Vec::new();
        key(&mut app, KeyCode::F(10), &mut out);
        handle_key(
            &mut app,
            KeyEvent::new(KeyCode::Char('q'), KeyModifiers::CONTROL),
            &mut out,
        );
        assert!(crate::orchestrator_view::active(&app), "still full-screen");
        assert!(!app.pane.term_locked);
        key(&mut app, KeyCode::Right, &mut out);
        assert_eq!(app.orch.chip, 1);
        key(&mut app, KeyCode::Enter, &mut out);
        let Some(Overlay::Review(review)) = &app.modals.overlay else {
            panic!("no review");
        };
        assert_eq!(review.sessions, [SessionRef::Agent(AgentId("run".into()))]);
    });
}

#[test]
fn review_opened_while_typing_into_another_session_waits_for_f10() {
    with_default_config(|| {
        let mut app = app_with_orchestrator();
        let mut out = Vec::new();
        // Typing into a1, not the orchestrator.
        attach_now(&mut app, SessionRef::Agent(AgentId("a1".into())), &mut out);
        app.nav.focus = Focus::Terminal;
        app.pane.term_locked = true;
        hse(
            &mut app,
            ServerEvent::ReviewOpened {
                opener: AgentId("orch".into()),
                sessions: vec![SessionRef::Agent(AgentId("ask".into()))],
                tabs: vec![ReviewTabKind::Diff],
            },
        );
        assert!(app.modals.overlay.is_none(), "focus is not stolen");
        assert!(app
            .chrome
            .flash
            .as_deref()
            .is_some_and(|f| f.contains("F10")));
        assert_eq!(pane_on(&app), Some(SessionRef::Agent(AgentId("a1".into()))));

        key(&mut app, KeyCode::F(10), &mut out);
        let Some(Overlay::Review(review)) = &app.modals.overlay else {
            panic!("F10 shows the held review");
        };
        assert_eq!(review.opener, Some(AgentId("orch".into())));
        assert_eq!(review.selected_tab(), Some(ReviewTabKind::Diff));
        assert!(app.orch.pending_review.is_none());
    });
}

#[test]
fn review_opened_while_typing_to_the_orchestrator_waits_too_and_f10_shows_it() {
    with_default_config(|| {
        let mut app = app_with_orchestrator();
        let mut out = Vec::new();
        key(&mut app, KeyCode::F(10), &mut out);
        hse(
            &mut app,
            ServerEvent::ReviewOpened {
                opener: AgentId("orch".into()),
                sessions: vec![
                    SessionRef::Agent(AgentId("ask".into())),
                    SessionRef::Agent(AgentId("run".into())),
                ],
                tabs: Vec::new(),
            },
        );
        assert!(
            app.modals.overlay.is_none(),
            "keys stay with the orchestrator"
        );
        assert_eq!(pane_on(&app), Some(orch()));
        assert_eq!(
            app.chrome.flash.as_deref(),
            Some(orchestrator::PENDING_REVIEW)
        );
        key(&mut app, KeyCode::F(10), &mut out);
        let Some(Overlay::Review(review)) = &app.modals.overlay else {
            panic!("no review: {:?}", app.modals.overlay);
        };
        assert_eq!(review.sessions.len(), 2);
        assert_eq!(review.tabs.len(), 4, "every tab by default");
        key(&mut app, KeyCode::Right, &mut out);
        assert_eq!(
            pane_on(&app),
            Some(SessionRef::Agent(AgentId("run".into())))
        );
    });
}

/// A review held while typing into a terminal tab, where F10 is the
/// shell's: the toast says how to get there.
#[test]
fn a_held_review_from_a_terminal_tab_says_ctrl_q_first() {
    with_default_config(|| {
        let mut app = app_with_orchestrator();
        app.pane.term = Some(AttachedTerm::new(
            SessionRef::Terminal(nebula_core::TerminalId("t1".into())),
            80,
            24,
        ));
        app.nav.focus = Focus::Terminal;
        app.pane.term_locked = true;
        hse(
            &mut app,
            ServerEvent::ReviewOpened {
                opener: AgentId("orch".into()),
                sessions: vec![SessionRef::Agent(AgentId("ask".into()))],
                tabs: Vec::new(),
            },
        );
        assert_eq!(
            app.chrome.flash.as_deref(),
            Some(orchestrator::PENDING_REVIEW_FROM_TERMINAL)
        );
    });
}

/// An unsolicited review (from the daemon, the user on the cards) ignores
/// Enter and a click on the screen for a moment, so a keystroke meant for
/// something else does not jump into a worker.
#[test]
fn an_unsolicited_review_ignores_an_immediate_enter() {
    with_default_config(|| {
        let mut app = app_with_orchestrator();
        let mut out = Vec::new();
        hse(
            &mut app,
            ServerEvent::ReviewOpened {
                opener: AgentId("orch".into()),
                sessions: vec![SessionRef::Agent(AgentId("ask".into()))],
                tabs: Vec::new(),
            },
        );
        assert!(matches!(app.modals.overlay, Some(Overlay::Review(_))));
        key(&mut app, KeyCode::Enter, &mut out);
        assert!(
            matches!(app.modals.overlay, Some(Overlay::Review(_))),
            "Enter right after an unsolicited open does not jump"
        );
        if let Some(Overlay::Review(review)) = &mut app.modals.overlay {
            review.guard_until = None;
        }
        key(&mut app, KeyCode::Enter, &mut out);
        assert!(app.modals.overlay.is_none(), "after the guard, Enter jumps");
        assert_eq!(
            pane_on(&app),
            Some(SessionRef::Agent(AgentId("ask".into())))
        );
    });
}

/// History asked for one session, then another before the first answer:
/// both answers land, neither tab stays on Loading.
#[test]
fn a_superseded_history_request_still_lands() {
    with_default_config(|| {
        let mut app = app_with_orchestrator();
        let mut out = Vec::new();
        orchestrator::open_review(
            &mut app,
            None,
            vec![
                SessionRef::Agent(AgentId("ask".into())),
                SessionRef::Agent(AgentId("run".into())),
            ],
            vec![ReviewTabKind::History],
            &mut out,
        );
        key(&mut app, KeyCode::Right, &mut out);
        let asked: Vec<(u64, String)> = out
            .iter()
            .filter_map(|r| match r {
                ClientRequest::ReadSession { req_id, target, .. } => {
                    Some((*req_id, target.clone()))
                }
                _ => None,
            })
            .collect();
        assert_eq!(asked.len(), 2, "{asked:?}");
        for (req_id, target) in asked.iter().rev() {
            hse(
                &mut app,
                ServerEvent::SessionText {
                    req_id: *req_id,
                    session: SessionRef::Agent(AgentId(target.clone())),
                    text: format!("history of {target}"),
                },
            );
        }
        let Some(Overlay::Review(review)) = &app.modals.overlay else {
            panic!("no review");
        };
        for id in ["ask", "run"] {
            assert_eq!(
                review.histories.get(&SessionRef::Agent(AgentId(id.into()))),
                Some(&crate::review_modal::Loaded::Ready(format!(
                    "history of {id}"
                )))
            );
        }
    });
}

/// A review opened over an empty pane leaves it empty again on close.
#[test]
fn closing_a_review_over_an_empty_pane_empties_it_again() {
    with_default_config(|| {
        let mut app = app_with_orchestrator();
        let mut out = Vec::new();
        assert!(app.pane.term.is_none());
        orchestrator::open_review(
            &mut app,
            None,
            vec![SessionRef::Agent(AgentId("ask".into()))],
            Vec::new(),
            &mut out,
        );
        assert_eq!(
            pane_on(&app),
            Some(SessionRef::Agent(AgentId("ask".into())))
        );
        key(&mut app, KeyCode::Esc, &mut out);
        assert!(app.pane.term.is_none(), "{:?}", pane_on(&app));
    });
}

/// "Make orchestrator" says so when the daemon has done it, not before.
#[test]
fn make_orchestrator_flashes_on_the_ack() {
    with_default_config(|| {
        let mut app = app_with_orchestrator();
        let mut out = Vec::new();
        app.chrome.flash = None;
        run_menu_action(
            &mut app,
            MenuAction::MakeOrchestrator(AgentId("run".into())),
            &mut out,
        );
        assert!(app.chrome.flash.is_none(), "{:?}", app.chrome.flash);
        let req_id = out
            .iter()
            .find_map(|r| match r {
                ClientRequest::SetOrchestrator { req_id, .. } => Some(*req_id),
                _ => None,
            })
            .unwrap();
        hse(
            &mut app,
            ServerEvent::Ack {
                req_id,
                created: None,
            },
        );
        assert!(app
            .chrome
            .flash
            .as_deref()
            .is_some_and(|f| f.contains("run is the orchestrator")));
    });
}

/// The start prompt launches on the harness's configured model and effort.
#[test]
fn the_start_prompt_applies_the_harness_defaults() {
    with_config_json(
        r#"{"claude_model": "sonnet", "claude_effort": "max"}"#,
        || {
            let mut app = App::new();
            seed_tree(&mut app);
            let mut out = Vec::new();
            key(&mut app, KeyCode::F(10), &mut out);
            key(&mut app, KeyCode::Enter, &mut out);
            let sent = out.iter().find_map(|r| match r {
                ClientRequest::StartOrchestrator { model, effort, .. } => {
                    Some((model.clone(), effort.clone()))
                }
                _ => None,
            });
            assert_eq!(sent, Some((Some("sonnet".into()), Some("max".into()))));
        },
    );
}

#[test]
fn the_history_tab_asks_for_the_transcript_and_shows_the_answer() {
    with_default_config(|| {
        let mut app = app_with_orchestrator();
        let mut out = Vec::new();
        orchestrator::open_review(
            &mut app,
            None,
            vec![SessionRef::Agent(AgentId("ask".into()))],
            Vec::new(),
            &mut out,
        );
        out.clear();
        key(&mut app, KeyCode::Char('3'), &mut out);
        let req_id = out
            .iter()
            .find_map(|r| match r {
                ClientRequest::ReadSession { req_id, target, .. } if target == "ask" => {
                    Some(*req_id)
                }
                _ => None,
            })
            .expect("ReadSession for ask");
        hse(
            &mut app,
            ServerEvent::SessionText {
                req_id,
                session: SessionRef::Agent(AgentId("ask".into())),
                text: "user: fix it\nassistant: done".into(),
            },
        );
        let shown = screen(&mut app, 120, 30);
        assert!(shown.contains("assistant: done"), "{shown}");
        // Asked once: a second visit reads what landed.
        out.clear();
        key(&mut app, KeyCode::Char('1'), &mut out);
        key(&mut app, KeyCode::Char('3'), &mut out);
        assert!(!out
            .iter()
            .any(|r| matches!(r, ClientRequest::ReadSession { .. })));
    });
}

#[test]
fn the_diff_tab_reads_the_sessions_worktree_diff() {
    let _env = crate::test_env::spawning();
    with_default_config(|| {
        let dir = tempfile::tempdir().unwrap();
        let git = |args: &[&str]| {
            let ok = std::process::Command::new("git")
                .args(args)
                .current_dir(dir.path())
                .env("GIT_AUTHOR_NAME", "t")
                .env("GIT_AUTHOR_EMAIL", "t@t")
                .env("GIT_COMMITTER_NAME", "t")
                .env("GIT_COMMITTER_EMAIL", "t@t")
                .output()
                .unwrap()
                .status
                .success();
            assert!(ok, "git {args:?}");
        };
        git(&["init", "-q", "-b", "main"]);
        std::fs::write(dir.path().join("notes.txt"), "old line\n").unwrap();
        git(&["add", "."]);
        git(&["commit", "-q", "-m", "init"]);
        std::fs::write(dir.path().join("notes.txt"), "new line\n").unwrap();

        let mut app = App::new();
        seed_worktree_at(&mut app, dir.path());
        upsert(
            &mut app,
            agent("fix", AgentRole::Worker, AgentStatus::Finished),
        );
        let mut out = Vec::new();
        orchestrator::open_review(
            &mut app,
            None,
            vec![SessionRef::Agent(AgentId("fix".into()))],
            vec![ReviewTabKind::Diff],
            &mut out,
        );
        let shown = screen(&mut app, 120, 30);
        assert!(shown.contains("+new line"), "{shown}");
        assert!(shown.contains("-old line"), "{shown}");
    });
}

#[test]
fn the_pr_tab_says_when_the_checkout_has_no_pull_request() {
    with_default_config(|| {
        let mut app = app_with_orchestrator();
        let mut out = Vec::new();
        orchestrator::open_review(
            &mut app,
            None,
            vec![SessionRef::Agent(AgentId("ask".into()))],
            vec![ReviewTabKind::PullRequest],
            &mut out,
        );
        let shown = screen(&mut app, 120, 30);
        assert!(shown.contains("No PR on main"), "{shown}");
    });
}

#[test]
fn cards_offer_make_orchestrator_and_the_orchestrator_card_wears_the_badge() {
    with_default_config(|| {
        let mut app = app_with_orchestrator();
        let worker = app
            .tree
            .agents
            .iter()
            .find(|a| a.id.0 == "run")
            .unwrap()
            .clone();
        let pinned = app
            .tree
            .agents
            .iter()
            .find(|a| a.id.0 == "orch")
            .unwrap()
            .clone();
        let labels = |a: &Agent| -> Vec<String> {
            menu_items_for_session(a)
                .into_iter()
                .map(|i| i.label.to_string())
                .collect()
        };
        assert!(labels(&worker).contains(&"Make orchestrator".to_string()));
        assert!(labels(&pinned).contains(&"Unpin orchestrator".to_string()));

        let mut out = Vec::new();
        run_menu_action(
            &mut app,
            MenuAction::MakeOrchestrator(worker.id.clone()),
            &mut out,
        );
        assert!(out.iter().any(
            |r| matches!(r, ClientRequest::SetOrchestrator { agent, .. } if *agent == worker.id)
        ));
        out.clear();
        run_menu_action(&mut app, MenuAction::ClearOrchestrator, &mut out);
        assert!(out
            .iter()
            .any(|r| matches!(r, ClientRequest::ClearOrchestrator { .. })));

        let grid = screen(&mut app, 160, 40);
        assert!(grid.contains("◆ orchestrator"), "{grid}");
    });
}
