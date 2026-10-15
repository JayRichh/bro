//! The AZURE DEVOPS OVERLAY's reads, keys and clicks (`crate::ado` is the
//! model, `ui::ado_view` the drawing).

use super::*;
use crate::ado::{AdoAnswer, AdoTab, AdoView, Load};

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

/// j/k walk, Tab / 1–3 switch lists, Enter opens, `s` starts a task, `r`
/// reads the list again, Esc / q close.
pub(crate) fn handle_key(app: &mut App, key: KeyEvent, out: &mut Vec<ClientRequest>) {
    app.chrome.dirty = true;
    match key.code {
        KeyCode::Enter => return activate(app, out),
        KeyCode::Char('s') => return start_task(app),
        KeyCode::Char('r') => return reload(app),
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
    use crate::ado::{AdoBranch, AdoPr, AdoTask, TaskRead};
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
}
