//! The ACCOUNTS OVERLAY's keys, and the usage the daemon pushes
//! (`ServerEvent::AccountUsage` / `SessionContexts`). State and rows live
//! in `crate::accounts_overlay`.

use super::*;
use crate::accounts_overlay::{
    build_rows, AccountForm, AccountsHit, AccountsView, Field, FormKind, Mode, Outcome,
};

/// The accounts to show: the pushed list, or the kept files when the
/// daemon is too old to push one.
fn readings(app: &mut App) -> Vec<nebula_core::usage::AccountUsage> {
    if !app.launcher.usage_pushed {
        app.launcher.account_usage = crate::accounts_overlay::kept_readings();
    }
    app.launcher.account_usage.clone()
}

/// The rows, and every setting the overlay mirrors, read again: the
/// profiles, the spread, each project's default accounts.
fn rows(app: &mut App) -> Vec<crate::accounts_overlay::AccountRow> {
    let cfg = crate::config::Config::load();
    crate::quick_prompt::mirror_accounts(app, &cfg);
    let usage = readings(app);
    build_rows(
        &cfg.profiles,
        &usage,
        &app.tree.agents,
        crate::accounts_overlay::login_lookup,
    )
}

/// Fill the view's mirrors from the app and the config.
fn mirror(app: &App, view: &mut AccountsView) {
    let cfg = crate::config::Config::load();
    view.harnesses = cfg
        .harness_registry()
        .iter()
        .map(|h| h.id.clone())
        .collect();
    view.spread = cfg.account_spread.clone();
    view.threshold = cfg.account_spread_threshold.clone();
    view.projects = app
        .tree
        .projects
        .iter()
        .map(|p| (p.name.clone(), p.repo_path.clone()))
        .collect();
    view.project_accounts = app.launcher.project_accounts.clone();
}

/// `⇧U`: open the overlay.
pub(crate) fn open(app: &mut App) {
    let rows = rows(app);
    let (config, local) = crate::config::Config::layer_paths();
    let mut view = AccountsView::new(rows, config, local);
    mirror(app, &mut view);
    app.modals.overlay = Some(Overlay::Accounts(view));
    app.chrome.dirty = true;
}

/// Rebuild the open overlay's rows (a push, an edit), keeping the cursor.
pub(crate) fn refresh(app: &mut App) {
    if !matches!(app.modals.overlay, Some(Overlay::Accounts(_))) {
        return;
    }
    let rows = rows(app);
    let Some(Overlay::Accounts(mut view)) = app.modals.overlay.take() else {
        return;
    };
    view.set_rows(rows);
    mirror(app, &mut view);
    app.modals.overlay = Some(Overlay::Accounts(view));
}

/// What a key did: a message for the footer, an account to sign in.
type Done = Option<Result<Outcome, String>>;

/// The list's keys: j/k move, Space/e switch a profile on or off, d/*
/// make it the default, a add, Enter edit, c duplicate, x/Delete remove,
/// l sign in, p use for a repo, m move the grid's session onto it, s/t
/// the spread and its threshold, Esc (or q) closes.
fn list_key(app: &mut App, key: KeyEvent, out: &mut Vec<ClientRequest>) -> Done {
    let agents = app.tree.agents.clone();
    let Some(Overlay::Accounts(view)) = &mut app.modals.overlay else {
        return None;
    };
    let said = |r: Result<String, String>| {
        Some(r.map(|m| Outcome {
            message: m,
            sign_in: None,
        }))
    };
    match key.code {
        KeyCode::Esc | KeyCode::Char('q') => {
            app.modals.overlay = None;
            None
        }
        KeyCode::Down | KeyCode::Char('j') => {
            view.step(1);
            None
        }
        KeyCode::Up | KeyCode::Char('k') => {
            view.step(-1);
            None
        }
        KeyCode::Char(' ') | KeyCode::Char('e') => said(view.toggle_enabled()),
        KeyCode::Char('d') | KeyCode::Char('*') => said(view.set_default()),
        KeyCode::Char('a') => {
            view.open_add();
            None
        }
        KeyCode::Enter => view.open_edit().err().map(Err),
        KeyCode::Char('c') => view.open_duplicate().err().map(Err),
        KeyCode::Char('x') | KeyCode::Delete => view.open_remove(&agents).err().map(Err),
        KeyCode::Char('p') => view.open_use_for().err().map(Err),
        KeyCode::Char('s') => Some(view.cycle_spread()),
        KeyCode::Char('t') => Some(view.cycle_threshold()),
        KeyCode::Char('l') => {
            let key = view.selected().map(|r| r.key.clone());
            if let Some(key) = key {
                sign_in(app, &key, out);
            }
            None
        }
        KeyCode::Char('m') => {
            move_session_here(app);
            None
        }
        _ => None,
    }
}

/// A form's keys: Tab / ↓ / ↑ walk the fields, ←/→ (or Space) step the
/// harness and flip a box, typing fills a text field, Enter saves, Esc
/// goes back to the list.
fn form_key(app: &mut App, key: KeyEvent) -> Done {
    let Some(Overlay::Accounts(view)) = &mut app.modals.overlay else {
        return None;
    };
    let taken: Vec<String> = view
        .rows
        .iter()
        .filter_map(|r| r.profile.as_ref().map(|p| p.name.trim().to_string()))
        .collect();
    let harnesses = view.harnesses.clone();
    if key.code == KeyCode::Enter {
        return Some(view.submit_form());
    }
    let Mode::Form(form) = &mut view.mode else {
        return None;
    };
    let n = form.fields().len();
    let field = form.current();
    match key.code {
        KeyCode::Esc => view.mode = Mode::List,
        KeyCode::Tab | KeyCode::Down => form.field = (form.field + 1) % n,
        KeyCode::BackTab | KeyCode::Up => form.field = (form.field + n - 1) % n,
        KeyCode::Left | KeyCode::Right | KeyCode::Char(' ') if field == Field::Harness => {
            if !harnesses.is_empty() {
                let at = harnesses
                    .iter()
                    .position(|h| *h == form.harness)
                    .unwrap_or(0);
                let next = if key.code == KeyCode::Left {
                    (at + harnesses.len() - 1) % harnesses.len()
                } else {
                    (at + 1) % harnesses.len()
                };
                form.harness = harnesses[next].clone();
                if form.kind == FormKind::Add && !form.touched {
                    form.name = crate::accounts_overlay::suggest_name(&form.harness, &taken);
                }
            }
        }
        KeyCode::Left | KeyCode::Right | KeyCode::Char(' ') if field == Field::Adopt => {
            form.adopt = !form.adopt;
        }
        KeyCode::Left | KeyCode::Right | KeyCode::Char(' ') if field == Field::Default => {
            form.make_default = !form.make_default;
        }
        KeyCode::Backspace if AccountForm::is_text(field) => {
            let text = if field == Field::Name {
                &mut form.name
            } else {
                &mut form.env
            };
            text.pop();
            form.touched = true;
        }
        KeyCode::Char(c)
            if AccountForm::is_text(field)
                && !key
                    .modifiers
                    .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) =>
        {
            if field == Field::Name {
                form.name.push(c);
            } else {
                form.env.push(c);
            }
            form.touched = true;
        }
        _ => {}
    }
    if let Mode::Form(form) = &mut view.mode {
        form.error = None;
    }
    None
}

/// The remove's keys: y / Enter remove, n / Esc keep.
fn remove_key(app: &mut App, key: KeyEvent) -> Done {
    let Some(Overlay::Accounts(view)) = &mut app.modals.overlay else {
        return None;
    };
    match key.code {
        KeyCode::Char('y') | KeyCode::Enter => Some(view.confirm_remove()),
        KeyCode::Char('n') | KeyCode::Esc => {
            view.mode = Mode::List;
            None
        }
        _ => None,
    }
}

/// "Use for repo"'s keys: j/k walk the repos, Enter sets or clears, Esc
/// goes back.
fn use_for_key(app: &mut App, key: KeyEvent) -> Done {
    let Some(Overlay::Accounts(view)) = &mut app.modals.overlay else {
        return None;
    };
    let n = view.projects.len();
    let Mode::UseFor { cursor } = &mut view.mode else {
        return None;
    };
    match key.code {
        KeyCode::Esc | KeyCode::Char('q') => view.mode = Mode::List,
        KeyCode::Down | KeyCode::Char('j') => *cursor = (*cursor + 1).min(n.saturating_sub(1)),
        KeyCode::Up | KeyCode::Char('k') => *cursor = cursor.saturating_sub(1),
        KeyCode::Enter | KeyCode::Char(' ') => return Some(view.toggle_use_for()),
        _ => {}
    }
    None
}

pub(crate) fn handle_key(app: &mut App, key: KeyEvent, out: &mut Vec<ClientRequest>) {
    let Some(Overlay::Accounts(view)) = &app.modals.overlay else {
        return;
    };
    let done = match view.mode {
        Mode::List => list_key(app, key, out),
        Mode::Form(_) => form_key(app, key),
        Mode::Remove { .. } => remove_key(app, key),
        Mode::UseFor { .. } => use_for_key(app, key),
    };
    match done {
        Some(Ok(outcome)) => {
            app.chrome.flash = Some(outcome.message);
            refresh(app);
            if let Some(account) = outcome.sign_in {
                sign_in(app, &account, out);
            }
        }
        Some(Err(message)) => app.chrome.flash = Some(message),
        None => {}
    }
    app.chrome.dirty = true;
}

/// A click: a button is its key, pressed; a row puts the cursor there; a
/// form's field takes the form's cursor; a repo in "use for" is picked
/// (a second click sets it). The wheel walks the rows.
pub(crate) fn handle_mouse(app: &mut App, mouse: MouseEvent, out: &mut Vec<ClientRequest>) -> bool {
    let Some(Overlay::Accounts(view)) = &mut app.modals.overlay else {
        return false;
    };
    app.chrome.dirty = true;
    let pos = ratatui::layout::Position::new(mouse.column, mouse.row);
    match mouse.kind {
        MouseEventKind::ScrollDown if view.mode == Mode::List => view.step(1),
        MouseEventKind::ScrollUp if view.mode == Mode::List => view.step(-1),
        MouseEventKind::Down(MouseButton::Left) => {
            let Some(hit) = view
                .hits
                .iter()
                .find(|(r, _)| r.contains(pos))
                .map(|(_, h)| *h)
            else {
                return true;
            };
            match hit {
                AccountsHit::Key(code) => {
                    handle_key(app, KeyEvent::new(code, KeyModifiers::NONE), out);
                }
                AccountsHit::Row(i) => view.cursor = i.min(view.rows.len().saturating_sub(1)),
                AccountsHit::Field(i) => {
                    if let Mode::Form(form) = &mut view.mode {
                        form.field = i.min(form.fields().len().saturating_sub(1));
                    }
                }
                AccountsHit::Project(i) => {
                    if let Mode::UseFor { cursor } = &mut view.mode {
                        if *cursor == i {
                            handle_key(app, KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE), out);
                        } else {
                            *cursor = i;
                        }
                    }
                }
                AccountsHit::Spread(i) => {
                    let word = nebula_core::accounts::SPREADS[i.min(2)];
                    let done = view.set_spread(word);
                    match done {
                        Ok(o) => {
                            app.chrome.flash = Some(o.message);
                            refresh(app);
                        }
                        Err(e) => app.chrome.flash = Some(e),
                    }
                }
            }
        }
        _ => {}
    }
    true
}

/// The checkout a login terminal opens in: the selected one, else the
/// first project's root.
fn login_worktree(app: &App) -> Option<WorktreeId> {
    app.selected_worktree()
        .map(|w| w.id.clone())
        .filter(|id| !app.is_placeholder_worktree(id))
        .or_else(|| {
            app.tree
                .projects
                .first()
                .and_then(|p| app.root_worktree(&p.id))
        })
}

/// Sign the account `key` in: its login command (looked up here, never
/// typed by anyone) run in a new terminal session, which the pane shows.
/// With no checkout to open one in, the footer says the command instead.
pub(crate) fn sign_in(app: &mut App, key: &str, out: &mut Vec<ClientRequest>) {
    let row = match &app.modals.overlay {
        Some(Overlay::Accounts(view)) => view.rows.iter().find(|r| r.key == key).cloned(),
        _ => None,
    };
    let Some(row) = row else {
        app.chrome.flash = Some(format!("no account `{key}`"));
        return;
    };
    if let Some(problem) = &row.problem {
        app.chrome.flash = Some(format!("{}: {problem}", row.label()));
        return;
    }
    let Some(line) = nebula_core::accounts::login_command(row.profile.as_ref(), &row.harness)
    else {
        app.chrome.flash = Some(format!(
            "kete knows no sign-in for {}: sign in from a session under it",
            row.harness
        ));
        return;
    };
    let Some(worktree) = login_worktree(app) else {
        app.chrome.flash = Some(format!("add a repo first, or run it yourself: {line}"));
        return;
    };
    app.modals.overlay = None;
    app.chrome.flash = Some(format!(
        "signing {} in: finish in the terminal, then ⇧U shows it signed in",
        row.label()
    ));
    send_with(app, out, PendingIntent::SignIn { line }, |req_id| {
        ClientRequest::CreateTerminal {
            req_id,
            worktree,
            name: Some(format!("sign in {}", row.key.trim_start_matches('@'))),
        }
    });
}

/// `m`: the session under the grid's cursor moved onto the selected
/// account, through MOVE TO ACCOUNT's confirm.
fn move_session_here(app: &mut App) {
    let Some(Overlay::Accounts(view)) = &app.modals.overlay else {
        return;
    };
    let Some(row) = view.selected().cloned() else {
        return;
    };
    let session = match app.selected_session_row() {
        Some(crate::app::SessionRow::Agent(a)) => a,
        _ => {
            app.chrome.flash = Some("no session under the cursor to move".into());
            return;
        }
    };
    let harness = nebula_core::accounts::session_harness(&session);
    if harness != row.harness {
        app.chrome.flash = Some(format!(
            "{} is a {} account: {} runs {harness}",
            row.label(),
            row.harness,
            session.name
        ));
        return;
    }
    if nebula_core::accounts::session_account(&session) == row.key {
        app.chrome.flash = Some(format!("{} already runs on {}", session.name, row.label()));
        return;
    }
    let profile = row.profile.as_ref().map(|p| p.name.trim().to_string());
    crate::account_switch::confirm(app, session.id.clone(), profile);
}

/// A 55+ daemon's account list: replaces what was known.
pub(crate) fn on_account_usage(app: &mut App, accounts: Vec<nebula_core::usage::AccountUsage>) {
    app.launcher.account_usage = accounts;
    app.launcher.usage_pushed = true;
    refresh(app);
    app.chrome.dirty = true;
}

/// Session contexts: folded in by agent.
pub(crate) fn on_session_contexts(
    app: &mut App,
    contexts: Vec<nebula_core::usage::SessionContext>,
) {
    for c in contexts {
        app.launcher.session_contexts.insert(c.agent, c.context);
    }
    app.chrome.dirty = true;
}

#[cfg(test)]
mod tests {
    use super::super::tests::{buffer_text, press, seed_tree, with_config_json};
    use super::*;
    use nebula_core::usage::{
        AccountUsage, ContextUsage, SessionContext, UsageSource, UsageWindow, WindowKind,
    };
    use ratatui::backend::TestBackend;

    fn own_login(percent: f32) -> AccountUsage {
        own_login_read(percent, nebula_core::clock::now_secs() as i64)
    }

    fn own_login_read(percent: f32, read_at: i64) -> AccountUsage {
        AccountUsage {
            account: "@claude".into(),
            harness: "claude".into(),
            windows: vec![
                UsageWindow {
                    kind: WindowKind::Session,
                    label: None,
                    percent,
                    resets_at: None,
                    read_at,
                },
                UsageWindow {
                    kind: WindowKind::Weekly,
                    label: None,
                    percent: 7.0,
                    resets_at: None,
                    read_at,
                },
            ],
            source: UsageSource::Statusline,
            at: 0,
            plan: None,
        }
    }

    /// The daemon's pushes land: the list replaces, contexts fold by id,
    /// and an open overlay follows the push.
    #[test]
    fn pushed_usage_and_contexts_fold_into_the_app_and_the_overlay() {
        with_config_json("{}", || {
            let mut app = App::new();
            let mut out = Vec::new();
            press(&mut app, KeyCode::Char('U'), KeyModifiers::SHIFT, &mut out);
            assert!(matches!(app.modals.overlay, Some(Overlay::Accounts(_))));
            handle_server_event(
                &mut app,
                ServerEvent::AccountUsage {
                    accounts: vec![own_login(21.0)],
                },
                &mut out,
            );
            assert!(app.launcher.usage_pushed);
            let Some(Overlay::Accounts(view)) = &app.modals.overlay else {
                panic!("closed");
            };
            assert_eq!(view.rows.len(), 1, "the own login shows with no profiles");
            assert_eq!(view.rows[0].key, "@claude");
            let ctx = |p: f32| ContextUsage {
                percent: Some(p),
                tokens: None,
                window: None,
                compactions: 0,
                last_compact_at: None,
                source: UsageSource::Statusline,
                at: 0,
            };
            for p in [10.0, 30.0] {
                handle_server_event(
                    &mut app,
                    ServerEvent::SessionContexts {
                        contexts: vec![SessionContext {
                            agent: AgentId("a1".into()),
                            context: ctx(p),
                        }],
                    },
                    &mut out,
                );
            }
            assert_eq!(app.launcher.session_contexts.len(), 1);
            assert_eq!(
                app.launcher.session_contexts[&AgentId("a1".into())].percent,
                Some(30.0)
            );
            assert!(out.is_empty(), "nothing goes to the daemon");
        });
    }

    /// The overlay draws the own-login account with its two bars, and the
    /// footer names its keys; Esc closes.
    #[test]
    fn the_overlay_draws_bars_and_closes_on_esc() {
        with_config_json("{}", || {
            let mut app = App::new();
            seed_tree(&mut app);
            let mut out = Vec::new();
            press(&mut app, KeyCode::Char('U'), KeyModifiers::SHIFT, &mut out);
            on_account_usage(&mut app, vec![own_login(83.0)]);
            let mut terminal = ratatui::Terminal::new(TestBackend::new(110, 30)).unwrap();
            terminal.draw(|f| crate::ui::draw(f, &mut app)).unwrap();
            let text = buffer_text(&terminal);
            assert!(text.contains("Accounts"), "{text}");
            assert!(text.contains("claude (own login)"), "{text}");
            assert!(text.contains(" 83%"), "{text}");
            assert!(text.contains("\u{25b0}"), "{text}");
            assert!(text.contains("Space: on/off"), "{text}");
            press(&mut app, KeyCode::Char(' '), KeyModifiers::NONE, &mut out);
            assert!(
                app.chrome
                    .flash
                    .as_deref()
                    .unwrap_or("")
                    .contains("own login"),
                "an own login cannot be switched off: {:?}",
                app.chrome.flash
            );
            press(&mut app, KeyCode::Esc, KeyModifiers::NONE, &mut out);
            assert!(app.modals.overlay.is_none());
        });
    }

    /// A session reading four hours old is drawn dim with its age inline;
    /// one past its window reads "—".
    #[test]
    fn a_stale_reading_shows_its_age_and_an_aged_out_one_reads_unknown() {
        with_config_json("{}", || {
            let now = nebula_core::clock::now_secs() as i64;
            let mut app = App::new();
            seed_tree(&mut app);
            let mut out = Vec::new();
            press(&mut app, KeyCode::Char('U'), KeyModifiers::SHIFT, &mut out);
            on_account_usage(&mut app, vec![own_login_read(83.0, now - 4 * 3600)]);
            let mut terminal = ratatui::Terminal::new(TestBackend::new(110, 30)).unwrap();
            terminal.draw(|f| crate::ui::draw(f, &mut app)).unwrap();
            let text = buffer_text(&terminal);
            assert!(text.contains(" 83%"), "{text}");
            assert!(text.contains("read 4h ago"), "{text}");
            on_account_usage(&mut app, vec![own_login_read(83.0, now - 6 * 3600)]);
            terminal.draw(|f| crate::ui::draw(f, &mut app)).unwrap();
            let text = buffer_text(&terminal);
            assert!(!text.contains(" 83%"), "aged out: {text}");
            assert!(text.contains("\u{2014}"), "{text}");
        });
    }
    /// A terminal shorter than the overlay's minimum still draws: the
    /// height clamp never has its floor above its ceiling.
    #[test]
    fn the_overlay_draws_in_a_tiny_terminal() {
        with_config_json("{}", || {
            let mut app = App::new();
            seed_tree(&mut app);
            let mut out = Vec::new();
            press(&mut app, KeyCode::Char('U'), KeyModifiers::SHIFT, &mut out);
            on_account_usage(&mut app, vec![own_login(40.0)]);
            for h in [1u16, 2, 5, 9, 10] {
                let mut terminal = ratatui::Terminal::new(TestBackend::new(60, h)).unwrap();
                terminal.draw(|f| crate::ui::draw(f, &mut app)).unwrap();
            }
        });
    }

    /// The settings the launch-order tests stand on: `main` the harness
    /// default, `proj` the seeded project's default account, `spare` a third.
    fn launch_json(spread: &str, proj_disabled: bool) -> String {
        format!(
            r#"{{"account_spread":"{spread}","profiles":[{{"name":"main","harness":"claude","default":true}},{{"name":"spare","harness":"claude"}},{{"name":"proj","harness":"claude","disabled":{proj_disabled}}}],"projects":{{"/tmp/demo":{{"default_profiles":{{"claude":"proj"}}}}}}}}"#
        )
    }

    fn box_launch(app: &App) -> crate::quick_prompt::QuickLaunch {
        match &app.modals.overlay {
            Some(Overlay::Prompt(p)) => match &p.kind {
                PromptKind::QuickPrompt(launch) => launch.clone(),
                other => panic!("{other:?}"),
            },
            other => panic!("{other:?}"),
        }
    }

    /// The box and the `n` picker preselect what the daemon would pick for
    /// a launch naming none: the project's default account ahead of the
    /// harness default (named under `manual`, "auto" under a spread), and
    /// the harness default again once the project's is switched off.
    #[test]
    fn a_launch_preselects_the_project_default_account() {
        let worktree = || crate::quick_prompt::QuickTarget::Worktree(WorktreeId("w1".into()));
        with_config_json(&launch_json("manual", false), || {
            let mut app = App::new();
            seed_tree(&mut app);
            crate::quick_prompt::open_for(&mut app, worktree());
            let launch = box_launch(&app);
            assert_eq!(launch.profile.as_deref(), Some("proj"));
            let label = crate::quick_prompt::profile_choice_label(
                launch.profile.as_deref(),
                &app.launcher.profiles,
                "claude",
                app.launcher.spread,
                &[],
                crate::quick_prompt::project_default(&app, &launch.target, "claude").as_deref(),
            );
            assert_eq!(label, "proj (project default)");
            assert_eq!(
                launch.profile_choices(&app.launcher.profiles, app.launcher.spread, Some("proj")),
                vec![
                    Some("proj".to_string()),
                    Some("main".to_string()),
                    Some("spare".to_string())
                ],
                "the project's leads; no \"none\" while a default stands"
            );
            // `n`: the Claude rows lead with the project's account.
            app.modals.overlay = None;
            super::super::tests::open_picker(&mut app);
            let Some(Overlay::Menu(menu)) = &app.modals.overlay else {
                panic!("{:?}", app.modals.overlay);
            };
            let first = menu
                .items
                .iter()
                .find_map(|i| match &i.action {
                    MenuAction::NewAgentOfKind {
                        kind: nebula_core::AgentKind::Claude,
                        custom: None,
                        profile,
                        ..
                    } => Some((i.label.clone(), profile.clone())),
                    _ => None,
                })
                .unwrap();
            assert_eq!(first.1.as_deref(), Some("proj"), "{first:?}");
            assert!(first.0.contains("proj (project default)"), "{first:?}");
        });
        with_config_json(&launch_json("least_used", false), || {
            let mut app = App::new();
            seed_tree(&mut app);
            crate::quick_prompt::open_for(&mut app, worktree());
            let launch = box_launch(&app);
            assert_eq!(launch.profile, None, "auto: the daemon picks");
            let label = crate::quick_prompt::profile_choice_label(
                None,
                &app.launcher.profiles,
                "claude",
                app.launcher.spread,
                &[],
                Some("proj"),
            );
            assert_eq!(label, "auto → proj (project default)");
        });
        with_config_json(&launch_json("manual", true), || {
            let mut app = App::new();
            seed_tree(&mut app);
            crate::quick_prompt::open_for(&mut app, worktree());
            assert_eq!(
                box_launch(&app).profile.as_deref(),
                Some("main"),
                "a switched-off project default falls through to the harness default"
            );
        });
    }

    /// The overlay edits the layers the test override (or a real override)
    /// routes to, never the data dir's config.json behind its back.
    #[test]
    fn the_overlay_edits_the_routed_config_layers() {
        with_config_json("{}", || {
            let mut app = App::new();
            let mut out = Vec::new();
            press(&mut app, KeyCode::Char('U'), KeyModifiers::SHIFT, &mut out);
            let Some(Overlay::Accounts(view)) = &app.modals.overlay else {
                panic!("closed");
            };
            let (config, local) = crate::config::Config::layer_paths();
            assert_eq!((&view.config, &view.local), (&config, &local));
            assert_ne!(view.config, nebula_core::paths::config_path());
        });
    }
}
