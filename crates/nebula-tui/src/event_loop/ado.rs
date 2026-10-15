//! The AZURE DEVOPS OVERLAY's reads, keys and clicks (`crate::ado` is the
//! model, `ui::ado_view` the drawing).

use super::*;
use crate::ado::{AdoAnswer, AdoButton, AdoTab, AdoView, Load, PrThreads, VOTES};

/// Ask, off the loop, whether each project's remote is on Azure DevOps —
/// the projects not asked about yet. The answer lights the header's `ado`
/// button. Nothing without a runtime (the unit tests).
pub(crate) fn probe(app: &mut App) {
    let Some(jobs) = app.jobs.view_jobs.clone() else {
        return;
    };
    for p in &app.tree.projects {
        if app.desk.ado_remote.contains_key(&p.id) || !app.desk.ado_probing.insert(p.id.clone()) {
            continue;
        }
        let (project, repo) = (p.id.clone(), p.repo_path.clone());
        jobs.run(move || {
            let ado = crate::ado::remote_url(&repo).is_some_and(|u| crate::ado::is_ado_remote(&u));
            Some(crate::view_jobs::Answer::Ado(AdoAnswer::Remote {
                project,
                ado,
            }))
        });
    }
}

/// Whether the project in front of you is on Azure DevOps, asked now if
/// no probe has answered yet.
fn on_ado(app: &mut App, project: &ProjectId) -> bool {
    if let Some(known) = app.desk.ado_remote.get(project) {
        return *known;
    }
    let ado = app
        .tree
        .projects
        .iter()
        .find(|p| &p.id == project)
        .and_then(|p| crate::ado::remote_url(&p.repo_path))
        .is_some_and(|u| crate::ado::is_ado_remote(&u));
    app.desk.ado_remote.insert(project.clone(), ado);
    ado
}

/// `⇧B`, or the header's `ado` button: the overlay on the project in front
/// of you, its three lists asked for at once.
pub(crate) fn open(app: &mut App) {
    let Some(project) = app.selected_project().map(|p| p.id.clone()) else {
        app.chrome.flash = Some("no project: Azure DevOps is a project's".into());
        return;
    };
    if !on_ado(app, &project) {
        app.chrome.flash = Some("this project's remote is not on Azure DevOps".into());
        return;
    }
    app.modals.overlay = Some(Overlay::Ado(AdoView {
        project: Some(project.clone()),
        ..AdoView::default()
    }));
    for tab in AdoTab::ALL {
        fetch(app, &project, tab);
    }
    app.chrome.dirty = true;
}

/// Read one list off the loop: the CLI's JSON for tasks and pull
/// requests, `git for-each-ref` for branches.
fn fetch(app: &mut App, project: &ProjectId, tab: AdoTab) {
    let Some(jobs) = app.jobs.view_jobs.clone() else {
        return;
    };
    let Some(repo) = app
        .tree
        .projects
        .iter()
        .find(|p| &p.id == project)
        .map(|p| p.repo_path.clone())
    else {
        return;
    };
    let project = project.clone();
    jobs.run(move || {
        let answer = match tab {
            AdoTab::Tasks => AdoAnswer::Tasks {
                result: crate::ado::run_cli(&project, &["tasks", "--json"])
                    .and_then(|json| crate::ado::parse_tasks(&json)),
                project,
            },
            AdoTab::PullRequests => AdoAnswer::Prs {
                result: crate::ado::run_cli(&project, &["prs", "--json"])
                    .and_then(|json| crate::ado::parse_prs(&json)),
                project,
            },
            AdoTab::Branches => AdoAnswer::Branches {
                result: crate::ado::read_branches(&repo),
                project,
            },
        };
        Some(crate::view_jobs::Answer::Ado(answer))
    });
}

fn load<T>(result: Result<T, String>) -> Load<T> {
    match result {
        Ok(v) => Load::Ready(v),
        Err(e) => Load::Failed(e),
    }
}

/// A read landed: into the open overlay when it is still that project's.
pub(crate) fn land(app: &mut App, answer: AdoAnswer) {
    let ours = |app: &App, project: &ProjectId| matches!(&app.modals.overlay, Some(Overlay::Ado(v)) if v.project.as_ref() == Some(project));
    match answer {
        AdoAnswer::Remote { project, ado } => {
            app.desk.ado_probing.remove(&project);
            app.desk.ado_remote.insert(project, ado);
        }
        AdoAnswer::Tasks { project, result } if ours(app, &project) => {
            if let Some(Overlay::Ado(v)) = &mut app.modals.overlay {
                v.tasks = load(result);
            }
        }
        AdoAnswer::Prs { project, result } if ours(app, &project) => {
            if let Some(Overlay::Ado(v)) = &mut app.modals.overlay {
                v.prs = load(result);
            }
        }
        AdoAnswer::Branches { project, result } if ours(app, &project) => {
            if let Some(Overlay::Ado(v)) = &mut app.modals.overlay {
                v.branches = load(result);
            }
        }
        AdoAnswer::Threads {
            project,
            pr,
            result,
        } if ours(app, &project) => {
            if let Some(Overlay::Ado(v)) = &mut app.modals.overlay {
                if let Some(t) = v.threads.as_mut().filter(|t| t.pr == pr) {
                    t.threads = load(result);
                }
            }
        }
        AdoAnswer::Voted(result) => {
            app.chrome.flash = Some(match result {
                Ok(said) => said.trim().to_string(),
                Err(e) => format!("vote failed: {e}"),
            });
        }
        AdoAnswer::Replied {
            project,
            pr,
            result,
        } => {
            let ok = result.is_ok();
            app.chrome.flash = Some(match result {
                Ok(said) => said.trim().to_string(),
                Err(e) => format!("reply failed: {e}"),
            });
            if let Some(Overlay::Ado(v)) = &mut app.modals.overlay {
                if let Some(t) = v.threads.as_mut().filter(|t| t.pr == pr) {
                    t.sending = false;
                    // Sent: the box goes and the threads are read again;
                    // failed, the text stays to send again.
                    if ok {
                        t.reply = None;
                    }
                }
            }
            if ok && ours(app, &project) {
                fetch_threads(app, &project, pr);
            }
        }
        AdoAnswer::Started(result) => {
            app.chrome.flash = Some(match result {
                Ok(said) => said
                    .lines()
                    .map(str::trim)
                    .find(|l| !l.is_empty())
                    .unwrap_or("task started")
                    .to_string(),
                Err(e) => format!("start task failed: {e}"),
            });
        }
        _ => {}
    }
    app.chrome.dirty = true;
}

/// Enter on the cursor's row: a task or pull request in the browser, a
/// branch checked out in a worktree of its own — or, already in one,
/// that worktree.
fn activate(app: &mut App, out: &mut Vec<ClientRequest>) {
    let Some(Overlay::Ado(view)) = &app.modals.overlay else {
        return;
    };
    let url = view
        .task()
        .map(|t| t.url.clone())
        .or_else(|| view.pr().map(|p| p.url.clone()));
    if let Some(url) = url {
        open_link(app, &url, out);
        return;
    }
    let (Some(branch), Some(project)) = (view.branch().cloned(), view.project.clone()) else {
        return;
    };
    app.modals.overlay = None;
    if let Some(w) = app
        .tree
        .worktrees
        .iter()
        .find(|w| w.project_id == project && w.branch == branch.name)
    {
        let id = w.id.clone();
        jump_to_target(app, PaletteTarget::Worktree(id), Landing::FocusOnly, out);
        return;
    }
    // A remote-only branch is cut from the remote's, so it tracks it.
    let base = if branch.local {
        None
    } else {
        branch.remote.clone()
    };
    let focus = app.nav.focus;
    app.bring_tab_forward(&project);
    let placeholder = placeholder::stage_worktree(app, project.clone(), branch.name.clone(), out);
    send_with(
        app,
        out,
        PendingIntent::SelectCreatedWorktree {
            placeholder,
            focus,
            prompt: PromptKind::NewWorktree {
                project: project.clone(),
                suggestion: branch.name.clone(),
            },
            text: branch.name.clone(),
            launch: None,
        },
        |req_id| ClientRequest::CreateWorktree {
            req_id,
            project,
            branch: branch.name,
            base,
        },
    );
}

/// `s` on a task: armed by the first press, started by the second —
/// `kete ado start-task`, which cuts the branch and worktree, links and
/// assigns the task and starts an agent on it.
fn start_task(app: &mut App) {
    let Some(Overlay::Ado(view)) = &mut app.modals.overlay else {
        return;
    };
    let (Some(task), Some(project)) = (view.task().cloned(), view.project.clone()) else {
        return;
    };
    if view.armed != Some(task.id) {
        view.armed = Some(task.id);
        app.chrome.flash = Some(format!(
            "s again starts #{}: a branch, a worktree and an agent on it",
            task.id
        ));
        return;
    }
    view.armed = None;
    let Some(jobs) = app.jobs.view_jobs.clone() else {
        return;
    };
    app.chrome.flash = Some(format!("starting #{}…", task.id));
    jobs.run(move || {
        let id = task.id.to_string();
        Some(crate::view_jobs::Answer::Ado(AdoAnswer::Started(
            crate::ado::run_cli(&project, &["start-task", &id]),
        )))
    });
}

/// The pull request under the cursor, on the PR tab.
fn pr_here(app: &App) -> Option<(ProjectId, crate::ado::AdoPr)> {
    let Some(Overlay::Ado(view)) = &app.modals.overlay else {
        return None;
    };
    Some((view.project.clone()?, view.pr()?.clone()))
}

/// `v` on a pull request: its vote choices, as the desktop's Review
/// buttons. A draft takes no votes until it is published.
fn start_vote(app: &mut App) {
    let pr = match &app.modals.overlay {
        Some(Overlay::Ado(view)) => view
            .threads
            .as_ref()
            .map(|t| (t.pr, t.is_draft))
            .or_else(|| view.pr().map(|p| (p.id, p.is_draft))),
        _ => None,
    };
    let Some((id, draft)) = pr else {
        return;
    };
    if draft {
        app.chrome.flash = Some(format!(
            "!{id} is a draft: nobody can vote until it is published"
        ));
        return;
    }
    if let Some(Overlay::Ado(view)) = &mut app.modals.overlay {
        view.voting = Some(id);
    }
}

/// A vote's key: `kete ado vote <id> <word>`, off the loop.
fn cast(app: &mut App, key: char) {
    let Some(Overlay::Ado(view)) = &mut app.modals.overlay else {
        return;
    };
    let (Some(id), Some(project)) = (view.voting.take(), view.project.clone()) else {
        return;
    };
    let Some((_, word, label)) = VOTES.iter().find(|v| v.0 == key).copied() else {
        return;
    };
    app.chrome.flash = Some(format!("voting {label} on !{id}…"));
    let Some(jobs) = app.jobs.view_jobs.clone() else {
        return;
    };
    jobs.run(move || {
        let id = id.to_string();
        Some(crate::view_jobs::Answer::Ado(AdoAnswer::Voted(
            crate::ado::run_cli(&project, &["vote", &id, word]),
        )))
    });
}

/// `c` on a pull request: its threads in place of the list, read now.
fn open_threads(app: &mut App) {
    let Some((project, pr)) = pr_here(app) else {
        return;
    };
    if let Some(Overlay::Ado(view)) = &mut app.modals.overlay {
        view.voting = None;
        view.threads = Some(PrThreads {
            pr: pr.id,
            title: pr.title.clone(),
            is_draft: pr.is_draft,
            threads: Load::Loading,
            cursor: 0,
            reply: None,
            sending: false,
        });
    }
    fetch_threads(app, &project, pr.id);
}

/// `kete ado pr <id> --json`'s threads, off the loop.
fn fetch_threads(app: &mut App, project: &ProjectId, pr: u32) {
    let Some(jobs) = app.jobs.view_jobs.clone() else {
        return;
    };
    let project = project.clone();
    jobs.run(move || {
        let id = pr.to_string();
        Some(crate::view_jobs::Answer::Ado(AdoAnswer::Threads {
            result: crate::ado::run_cli(&project, &["pr", &id, "--json"])
                .and_then(|json| crate::ado::parse_threads(&json)),
            project,
            pr,
        }))
    });
}

/// Enter on a typed reply: `kete ado reply <id> <thread> -- <text>`,
/// off the loop; the `--` keeps a reply that starts with `-` text.
fn send_reply(app: &mut App) {
    let Some(Overlay::Ado(view)) = &mut app.modals.overlay else {
        return;
    };
    let project = view.project.clone();
    let Some(t) = view.threads.as_mut() else {
        return;
    };
    let text = t.reply.as_deref().unwrap_or("").trim().to_string();
    let (Some(project), Some(thread)) = (project, t.thread().map(|x| x.id)) else {
        return;
    };
    if text.is_empty() || t.sending {
        return;
    }
    t.sending = true;
    let pr = t.pr;
    app.chrome.flash = Some(format!("replying on !{pr}…"));
    let Some(jobs) = app.jobs.view_jobs.clone() else {
        return;
    };
    jobs.run(move || {
        let (id, thread) = (pr.to_string(), thread.to_string());
        Some(crate::view_jobs::Answer::Ado(AdoAnswer::Replied {
            result: crate::ado::run_cli(&project, &["reply", &id, &thread, "--", &text]),
            project,
            pr,
        }))
    });
}

/// The keys while a vote is being picked, a reply typed, or a PR's
/// threads are up; false when none of them is.
fn handle_pr_key(app: &mut App, key: KeyEvent) -> bool {
    let Some(Overlay::Ado(view)) = &mut app.modals.overlay else {
        return false;
    };
    let plain = !key
        .modifiers
        .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT);
    if let Some(t) = view.threads.as_mut().filter(|t| t.reply.is_some()) {
        match key.code {
            KeyCode::Esc => t.reply = None,
            KeyCode::Enter => send_reply(app),
            KeyCode::Backspace => {
                t.reply.as_mut().map(String::pop);
            }
            KeyCode::Char(c) if plain && !t.sending => {
                t.reply.as_mut().map(|r| r.push(c));
            }
            _ => {}
        }
        return true;
    }
    if view.voting.is_some() {
        match key.code {
            KeyCode::Char(c) if VOTES.iter().any(|v| v.0 == c) => cast(app, c),
            KeyCode::Esc | KeyCode::Char('q') | KeyCode::Char('v') => view.voting = None,
            _ => {}
        }
        return true;
    }
    let Some(t) = view.threads.as_mut() else {
        return false;
    };
    match key.code {
        KeyCode::Esc | KeyCode::Char('q') => view.threads = None,
        KeyCode::Down | KeyCode::Char('j') => t.step(1),
        KeyCode::Up | KeyCode::Char('k') => t.step(-1),
        KeyCode::Enter | KeyCode::Char('c') if t.thread().is_some() => {
            t.reply = Some(String::new())
        }
        KeyCode::Char('v') => start_vote(app),
        KeyCode::Char('r') => {
            t.threads = Load::Loading;
            let (pr, project) = (t.pr, view.project.clone());
            if let Some(project) = project {
                fetch_threads(app, &project, pr);
            }
        }
        _ => {}
    }
    true
}

/// j/k walk, Tab / 1–3 switch lists, Enter opens, `s` starts a task, `v`
/// votes on a pull request and `c` reads its threads to reply, `r`
/// reads the list again, Esc / q close.
pub(crate) fn handle_key(app: &mut App, key: KeyEvent, out: &mut Vec<ClientRequest>) {
    app.chrome.dirty = true;
    if handle_pr_key(app, key) {
        return;
    }
    match key.code {
        KeyCode::Enter => return activate(app, out),
        KeyCode::Char('s') => return start_task(app),
        KeyCode::Char('r') => return reload(app),
        KeyCode::Char('v') => return start_vote(app),
        KeyCode::Char('c') => return open_threads(app),
        _ => {}
    }
    let Some(Overlay::Ado(view)) = &mut app.modals.overlay else {
        return;
    };
    let tab_at = |i: usize| AdoTab::ALL[i % AdoTab::ALL.len()];
    match key.code {
        KeyCode::Esc | KeyCode::Char('q') => app.modals.overlay = None,
        KeyCode::Down | KeyCode::Char('j') => view.step(1),
        KeyCode::Up | KeyCode::Char('k') => view.step(-1),
        KeyCode::Home => view.select(0),
        KeyCode::End => view.select(usize::MAX),
        KeyCode::Tab => view.set_tab(tab_at(view.tab.index() + 1)),
        KeyCode::BackTab => view.set_tab(tab_at(view.tab.index() + AdoTab::ALL.len() - 1)),
        KeyCode::Char(c @ '1'..='3') => view.set_tab(tab_at(c as usize - '1' as usize)),
        _ => {}
    }
}

/// `r`: read the tab's list again.
fn reload(app: &mut App) {
    let Some(Overlay::Ado(view)) = &mut app.modals.overlay else {
        return;
    };
    let (tab, project) = (view.tab, view.project.clone());
    match tab {
        AdoTab::Tasks => view.tasks = Load::Loading,
        AdoTab::PullRequests => view.prs = Load::Loading,
        AdoTab::Branches => view.branches = Load::Loading,
    }
    if let Some(project) = project {
        fetch(app, &project, tab);
    }
}

/// A click on a tab switches to it; on a row puts the cursor there, and
/// on the cursor's row is Enter. The wheel walks the rows.
pub(crate) fn handle_mouse(app: &mut App, mouse: MouseEvent, out: &mut Vec<ClientRequest>) -> bool {
    let Some(Overlay::Ado(view)) = &mut app.modals.overlay else {
        return false;
    };
    app.chrome.dirty = true;
    let pos = ratatui::layout::Position::new(mouse.column, mouse.row);
    // A bottom-row button is its key, pressed.
    if matches!(mouse.kind, MouseEventKind::Down(MouseButton::Left)) {
        if let Some(b) = view
            .buttons
            .iter()
            .find(|(r, _)| r.contains(pos))
            .map(|(_, b)| *b)
        {
            let code = match b {
                AdoButton::Vote => KeyCode::Char('v'),
                AdoButton::Comments => KeyCode::Char('c'),
                AdoButton::Cast(c) => KeyCode::Char(c),
                AdoButton::Send => KeyCode::Enter,
                AdoButton::Back => KeyCode::Esc,
            };
            handle_key(app, KeyEvent::new(code, KeyModifiers::NONE), out);
            return true;
        }
        // A thread: the cursor there; on the cursor's, a reply.
        if let Some(i) = view
            .thread_hits
            .iter()
            .find(|(r, _)| r.contains(pos))
            .map(|(_, i)| *i)
        {
            if let Some(t) = view.threads.as_mut().filter(|t| t.reply.is_none()) {
                if t.cursor == i {
                    t.reply = Some(String::new());
                } else {
                    t.cursor = i;
                }
            }
            return true;
        }
    }
    if let Some(t) = view.threads.as_mut() {
        match mouse.kind {
            MouseEventKind::ScrollDown => t.step(1),
            MouseEventKind::ScrollUp => t.step(-1),
            _ => {}
        }
        return true;
    }
    match mouse.kind {
        MouseEventKind::ScrollDown => view.step(1),
        MouseEventKind::ScrollUp => view.step(-1),
        MouseEventKind::Down(MouseButton::Left) => {
            if let Some(tab) = view
                .tab_hits
                .iter()
                .find(|(r, _)| r.contains(pos))
                .map(|(_, t)| *t)
            {
                view.set_tab(tab);
            } else if view.list_area.contains(pos) {
                let row = view.scroll + usize::from(mouse.row - view.list_area.y);
                if row < view.len() {
                    if row == view.cursor() {
                        activate(app, out);
                    } else {
                        view.select(row);
                    }
                }
            }
        }
        _ => {}
    }
    true
}
#[cfg(test)]
mod tests {
    use super::super::tests::{buffer_text, press, seed_tree, with_default_config};
    use super::*;
    use crate::ado::{AdoBranch, AdoPr, AdoTask, AdoThread, TaskRead};
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    fn ado_app() -> App {
        let mut app = App::new();
        app.launcher.launcher_pane_at = crate::launcher::PaneSide::Bottom;
        seed_tree(&mut app);
        app.desk.ado_remote.insert(ProjectId("p1".into()), true);
        app
    }

    fn draw(app: &mut App) -> String {
        let mut terminal = Terminal::new(TestBackend::new(140, 34)).unwrap();
        terminal.draw(|f| crate::ui::draw(f, app)).unwrap();
        buffer_text(&terminal)
    }

    fn view(app: &App) -> &AdoView {
        match &app.modals.overlay {
            Some(Overlay::Ado(v)) => v,
            other => panic!("no ado overlay: {other:?}"),
        }
    }

    /// `⇧B` on a project that is not on Azure DevOps says so and opens
    /// nothing; on one that is, the overlay opens loading its lists, and
    /// the header carries the `ado` button.
    #[test]
    fn the_overlay_opens_only_on_an_ado_project() {
        with_default_config(|| {
            let mut app = App::new();
            seed_tree(&mut app);
            app.desk.ado_remote.insert(ProjectId("p1".into()), false);
            let mut out = Vec::new();
            press(&mut app, KeyCode::Char('B'), KeyModifiers::SHIFT, &mut out);
            assert!(app.modals.overlay.is_none());
            assert!(!draw(&mut app).contains(" ado "));

            let mut app = ado_app();
            assert!(draw(&mut app).contains("ado ⇧B"));
            press(&mut app, KeyCode::Char('B'), KeyModifiers::SHIFT, &mut out);
            assert_eq!(view(&app).tab, AdoTab::Tasks);
            let text = draw(&mut app);
            assert!(text.contains("Azure DevOps · demo"), "{text}");
            assert!(text.contains("loading"), "{text}");
        });
    }

    /// Landed lists draw in their tabs; Tab and the digits switch; a
    /// branch already in a worktree is that worktree on Enter.
    #[test]
    fn the_lists_land_and_the_tabs_switch() {
        with_default_config(|| {
            let mut app = ado_app();
            let mut out = Vec::new();
            press(&mut app, KeyCode::Char('B'), KeyModifiers::SHIFT, &mut out);
            let p1 = ProjectId("p1".into());
            land(
                &mut app,
                AdoAnswer::Tasks {
                    project: p1.clone(),
                    result: Ok(TaskRead {
                        header: "Sprint 12".into(),
                        tasks: vec![AdoTask {
                            id: 42,
                            kind: "Task".into(),
                            title: "Fix the redirect".into(),
                            state: "Active".into(),
                            assigned_to: Some("Jay".into()),
                            url: "https://dev.azure.com/o/p/_workitems/edit/42".into(),
                            parent: Some("Login".into()),
                        }],
                    }),
                },
            );
            land(
                &mut app,
                AdoAnswer::Prs {
                    project: p1.clone(),
                    result: Ok(vec![AdoPr {
                        id: 7,
                        title: "Attach links".into(),
                        source: "jr/links".into(),
                        target: "main".into(),
                        is_draft: true,
                        created_by: "Jay".into(),
                        url: "u".into(),
                        mine: true,
                    }]),
                },
            );
            land(
                &mut app,
                AdoAnswer::Branches {
                    project: p1,
                    result: Ok(vec![AdoBranch {
                        name: "main".into(),
                        local: true,
                        remote: Some("origin/main".into()),
                    }]),
                },
            );
            let text = draw(&mut app);
            assert!(text.contains("#42"), "{text}");
            assert!(text.contains("Fix the redirect"), "{text}");
            assert!(text.contains("Sprint 12"), "{text}");
            press(&mut app, KeyCode::Tab, KeyModifiers::NONE, &mut out);
            let text = draw(&mut app);
            assert!(text.contains("!7") && text.contains("draft"), "{text}");
            press(&mut app, KeyCode::Char('3'), KeyModifiers::NONE, &mut out);
            let text = draw(&mut app);
            assert!(text.contains("main") && text.contains("worktree"), "{text}");
            press(&mut app, KeyCode::Enter, KeyModifiers::NONE, &mut out);
            assert!(app.modals.overlay.is_none());
            assert_eq!(
                app.selected_worktree().map(|w| w.branch.clone()),
                Some("main".into())
            );
        });
    }

    /// A list that failed says why; `s` on a task arms before it starts.
    #[test]
    fn a_failure_reads_out_and_start_task_asks_twice() {
        with_default_config(|| {
            let mut app = ado_app();
            let mut out = Vec::new();
            press(&mut app, KeyCode::Char('B'), KeyModifiers::SHIFT, &mut out);
            land(
                &mut app,
                AdoAnswer::Tasks {
                    project: ProjectId("p1".into()),
                    result: Ok(TaskRead {
                        header: "Sprint 1".into(),
                        tasks: vec![AdoTask {
                            id: 5,
                            kind: "Task".into(),
                            title: "t".into(),
                            state: "New".into(),
                            assigned_to: None,
                            url: "u".into(),
                            parent: None,
                        }],
                    }),
                },
            );
            land(
                &mut app,
                AdoAnswer::Prs {
                    project: ProjectId("p1".into()),
                    result: Err("Sign in to Azure DevOps".into()),
                },
            );
            press(&mut app, KeyCode::Char('s'), KeyModifiers::NONE, &mut out);
            assert_eq!(view(&app).armed, Some(5));
            assert!(app
                .chrome
                .flash
                .as_deref()
                .unwrap_or("")
                .contains("s again"));
            press(&mut app, KeyCode::Char('2'), KeyModifiers::NONE, &mut out);
            assert_eq!(view(&app).armed, None, "a tab change disarms");
            let text = draw(&mut app);
            assert!(text.contains("Sign in to Azure DevOps"), "{text}");
        });
    }
    fn pr(id: u32, draft: bool) -> AdoPr {
        AdoPr {
            id,
            title: format!("PR {id}"),
            source: "jr/x".into(),
            target: "main".into(),
            is_draft: draft,
            created_by: "Sam".into(),
            url: "u".into(),
            mine: false,
        }
    }

    /// On a pull request `v` puts up the vote choices (buttons naming
    /// their keys) and a draft takes none; `c` reads its threads, a
    /// thread takes a typed reply, and a sent reply closes the box.
    #[test]
    fn a_pull_request_takes_a_vote_and_a_reply() {
        with_default_config(|| {
            let mut app = ado_app();
            let mut out = Vec::new();
            let p1 = ProjectId("p1".into());
            press(&mut app, KeyCode::Char('B'), KeyModifiers::SHIFT, &mut out);
            land(
                &mut app,
                AdoAnswer::Prs {
                    project: p1.clone(),
                    result: Ok(vec![pr(7, false), pr(8, true)]),
                },
            );
            press(&mut app, KeyCode::Char('2'), KeyModifiers::NONE, &mut out);
            let text = draw(&mut app);
            assert!(
                text.contains("[v vote]") && text.contains("[c comments]"),
                "{text}"
            );

            press(&mut app, KeyCode::Char('v'), KeyModifiers::NONE, &mut out);
            assert_eq!(view(&app).voting, Some(7));
            let text = draw(&mut app);
            assert!(
                text.contains("[a approve]") && text.contains("[x reject]"),
                "{text}"
            );
            press(&mut app, KeyCode::Esc, KeyModifiers::NONE, &mut out);
            assert_eq!(view(&app).voting, None);
            assert!(
                app.modals.overlay.is_some(),
                "Esc leaves the votes, not the overlay"
            );
            draw(&mut app);
            // A click on the button is its key.
            let (rect, _) = view(&app)
                .buttons
                .iter()
                .find(|(_, b)| *b == AdoButton::Vote)
                .copied()
                .unwrap();
            handle_mouse(
                &mut app,
                MouseEvent {
                    kind: MouseEventKind::Down(MouseButton::Left),
                    column: rect.x + 1,
                    row: rect.y,
                    modifiers: KeyModifiers::NONE,
                },
                &mut out,
            );
            assert_eq!(view(&app).voting, Some(7));
            press(&mut app, KeyCode::Char('a'), KeyModifiers::NONE, &mut out);
            assert_eq!(view(&app).voting, None);
            assert!(app
                .chrome
                .flash
                .as_deref()
                .unwrap_or("")
                .contains("approve on !7"));

            press(&mut app, KeyCode::Char('j'), KeyModifiers::NONE, &mut out);
            press(&mut app, KeyCode::Char('v'), KeyModifiers::NONE, &mut out);
            assert_eq!(view(&app).voting, None, "a draft takes no votes");
            assert!(app.chrome.flash.as_deref().unwrap_or("").contains("draft"));
            press(&mut app, KeyCode::Char('k'), KeyModifiers::NONE, &mut out);

            press(&mut app, KeyCode::Char('c'), KeyModifiers::NONE, &mut out);
            assert!(draw(&mut app).contains("loading the threads"));
            land(
                &mut app,
                AdoAnswer::Threads {
                    project: p1.clone(),
                    pr: 7,
                    result: Ok(vec![AdoThread {
                        id: 3,
                        status: "active".into(),
                        file: Some("/src/a.rs".into()),
                        line: Some(12),
                        comments: vec![("Sam".into(), "why this?".into())],
                    }]),
                },
            );
            let text = draw(&mut app);
            assert!(
                text.contains("src/a.rs:12") && text.contains("Sam: why this?"),
                "{text}"
            );
            press(&mut app, KeyCode::Char('c'), KeyModifiers::NONE, &mut out);
            for c in "-q jk".chars() {
                press(&mut app, KeyCode::Char(c), KeyModifiers::NONE, &mut out);
            }
            let t = view(&app).threads.clone().unwrap();
            assert_eq!(t.reply.as_deref(), Some("-q jk"), "the letters type");
            assert!(draw(&mut app).contains("reply › -q jk"));
            press(&mut app, KeyCode::Enter, KeyModifiers::NONE, &mut out);
            assert!(view(&app).threads.as_ref().unwrap().sending);
            land(
                &mut app,
                AdoAnswer::Replied {
                    project: p1,
                    pr: 7,
                    result: Ok("Replied on !7, thread 3.".into()),
                },
            );
            let t = view(&app).threads.clone().unwrap();
            assert_eq!((t.reply, t.sending), (None, false));
            assert_eq!(
                app.chrome.flash.as_deref(),
                Some("Replied on !7, thread 3.")
            );
            press(&mut app, KeyCode::Esc, KeyModifiers::NONE, &mut out);
            assert!(view(&app).threads.is_none(), "Esc: back to the list");
        });
    }
}
