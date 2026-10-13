//! The REVIEW MODAL's drawing, and the ORCHESTRATOR start prompt's.

use super::*;
use crate::review_modal::{Loaded, ReviewView};
use nebula_core::ReviewTabKind;

/// The name a session goes by in the review: its card's name.
fn session_name(app: &App, sref: &SessionRef) -> String {
    match sref {
        SessionRef::Agent(id) => app
            .tree
            .agents
            .iter()
            .find(|a| &a.id == id)
            .map(|a| a.name.clone())
            .unwrap_or_else(|| id.to_string()),
        SessionRef::Terminal(id) => app
            .tree
            .terminals
            .iter()
            .find(|t| &t.id == id)
            .map(|t| format!("terminal {}", t.name))
            .unwrap_or_else(|| format!("terminal {id}")),
    }
}

fn session_worktree<'a>(app: &'a App, sref: &SessionRef) -> Option<&'a nebula_core::Worktree> {
    let wid = match sref {
        SessionRef::Agent(id) => &app.tree.agents.iter().find(|a| &a.id == id)?.worktree_id,
        SessionRef::Terminal(id) => &app.tree.terminals.iter().find(|t| &t.id == id)?.worktree_id,
    };
    app.tree.worktrees.iter().find(|w| &w.id == wid)
}

pub(super) fn draw(f: &mut Frame, app: &mut App, mut view: ReviewView) {
    let th = app.chrome.theme;
    let area = centered_rect_pct(f.area(), 98, 96);
    let selected = view.selected_session().cloned();
    let title = match &selected {
        Some(sref) if view.sessions.len() > 1 => format!(
            " Review · {} ({}/{}) ",
            session_name(app, sref),
            view.session + 1,
            view.sessions.len()
        ),
        Some(sref) => format!(" Review · {} ", session_name(app, sref)),
        None => " Review ".to_string(),
    };
    let inner = render_modal_frame(f, area, title, th);
    let (strip, hits) = tab_strip(
        inner.x,
        view.tabs
            .iter()
            .map(|tab| (crate::review_modal::tab_label(*tab), None)),
        view.tab,
        true,
        th,
    );
    if let Some(row) = row_rect(inner, 0) {
        f.render_widget(Paragraph::new(Line::from(strip)), row);
    }
    // The sessions under review, the selected one lit; ←/→ walk them.
    let mut session_hits = Vec::new();
    if let Some(row) = row_rect(inner, 1) {
        let mut spans = Vec::new();
        let mut x = row.x;
        for (i, sref) in view.sessions.iter().enumerate() {
            let label = format!(" {} ", fit_to_width(&session_name(app, sref), 24));
            let w = Span::raw(label.as_str()).width() as u16;
            if x + w + 1 > row.right() {
                break;
            }
            spans.push(Span::raw(" "));
            x += 1;
            let style = if i == view.session {
                Style::default().fg(th.on_accent).bg(th.accent)
            } else {
                Style::default().fg(th.muted)
            };
            spans.push(Span::styled(label, style));
            session_hits.push((x, x + w));
            x += w;
        }
        if view.sessions.len() > 1 {
            spans.push(Span::styled("   ←/→ sessions", Style::default().fg(th.dim)));
        }
        f.render_widget(Paragraph::new(Line::from(spans)), row);
    }
    let body = Rect {
        y: inner.y.saturating_add(3),
        height: inner.height.saturating_sub(4),
        ..inner
    };
    let legend = Rect {
        y: inner.bottom().saturating_sub(1),
        height: 1,
        ..inner
    };
    view.pr_url = None;
    match (view.selected_tab(), &selected) {
        (_, None) => dim_text(f, body, "No session selected.", th),
        (Some(ReviewTabKind::Terminal), Some(sref)) => draw_terminal_tab(f, app, sref, body),
        (Some(ReviewTabKind::Diff), Some(sref)) => {
            let branch = session_worktree(app, sref)
                .map(|w| w.branch.clone())
                .unwrap_or_default();
            match view.diffs.get(sref) {
                Some(Loaded::Ready(text)) if text.is_empty() => {
                    dim_text(f, body, &format!("No uncommitted changes in {branch}."), th)
                }
                Some(Loaded::Ready(text)) => {
                    let lines = text.lines().map(|l| diff_line(l, th)).collect();
                    scrolled(f, body, lines, &mut view.scroll);
                }
                Some(Loaded::Failed(why)) => dim_text(f, body, why, th),
                _ => dim_text(f, body, "Reading the diff…", th),
            }
        }
        (Some(ReviewTabKind::History), Some(sref)) => match view.histories.get(sref) {
            Some(Loaded::Ready(text)) if text.trim().is_empty() => {
                dim_text(f, body, "Nothing in this session's history yet.", th)
            }
            Some(Loaded::Ready(text)) => {
                let lines = text
                    .lines()
                    .map(|l| Line::from(Span::styled(l.to_string(), Style::default().fg(th.text))))
                    .collect();
                scrolled(f, body, lines, &mut view.scroll);
            }
            Some(Loaded::Failed(why)) => dim_text(f, body, why, th),
            _ => dim_text(f, body, "Reading the history…", th),
        },
        (Some(ReviewTabKind::PullRequest), Some(sref)) => {
            let lines = pr_lines(app, sref, body.width as usize, &mut view.pr_url);
            scrolled(f, body, lines, &mut view.scroll);
        }
        (None, _) => {}
    }
    let enter = match view.selected_tab() {
        Some(ReviewTabKind::Terminal) => "Enter type here · ",
        Some(ReviewTabKind::PullRequest) if view.pr_url.is_some() => "Enter open in browser · ",
        _ => "",
    };
    f.render_widget(
        Paragraph::new(Span::styled(
            format!(" Tab tabs · ←/→ sessions · ↑/↓ scroll · {enter}Esc close"),
            Style::default().fg(th.dim),
        )),
        legend,
    );
    if let Some(Overlay::Review(v)) = &mut app.modals.overlay {
        view.area = area;
        view.body_area = body;
        view.tab_hits = hits;
        view.session_hits = session_hits;
        *v = view;
    }
}

/// The Terminal tab: the session's own screen, which the review attached
/// the pane to (`settle_review`).
fn draw_terminal_tab(f: &mut Frame, app: &mut App, sref: &SessionRef, body: Rect) {
    let th = app.chrome.theme;
    match &app.pane.term {
        Some(term) if &term.sref == sref && term.refused.is_some() => {
            dim_text(f, body, term.refused.as_deref().unwrap_or_default(), th)
        }
        Some(term) if &term.sref == sref && term.booting && !term.exited => {
            dim_text(f, body, "Starting the session…", th)
        }
        Some(term) if &term.sref == sref => {
            let widget = tui_term::widget::PseudoTerminal::new(term.parser.screen());
            f.render_widget(widget, body);
        }
        _ => dim_text(f, body, "Attaching…", th),
    }
}

/// The PR tab: the pull request on the session's checkout — its whole
/// conversation when it has been read, else its headline — or none.
fn pr_lines(
    app: &App,
    sref: &SessionRef,
    width: usize,
    url_out: &mut Option<String>,
) -> Vec<Line<'static>> {
    let th = app.chrome.theme;
    let Some(worktree) = session_worktree(app, sref) else {
        return vec![dim_line("The session's checkout is not known.", th)];
    };
    let tracked = app
        .github
        .pull_requests
        .get(&worktree.id)
        .and_then(|pr| pr.as_ref())
        .map(|pr| {
            (
                pr.number,
                pr.title.clone(),
                pr.url.clone(),
                pr.standing().label(),
            )
        });
    let listed = || {
        let list = app.github.open_prs.get(&worktree.project_id)?;
        let pr = list.list.iter().find(|pr| pr.head == worktree.branch)?;
        Some((
            pr.number,
            pr.title.clone(),
            pr.url.clone(),
            pr.standing().label(),
        ))
    };
    let Some((number, title, url, state)) = tracked.or_else(listed) else {
        return vec![dim_line(
            &format!(
                "No PR on {}: ⇧R on the grid reloads from GitHub.",
                worktree.branch
            ),
            th,
        )];
    };
    *url_out = Some(url.clone());
    if let Some(detail) = app.github.pr_detail.get(&url) {
        return crate::pr_preview::lines(detail, width, th);
    }
    vec![
        Line::from(Span::styled(
            format!("#{number} {title}"),
            Style::default().fg(th.text).add_modifier(Modifier::BOLD),
        )),
        Line::from(Span::styled(
            state.to_string(),
            Style::default().fg(th.muted),
        )),
        Line::from(Span::styled(url, Style::default().fg(th.accent))),
        Line::from(""),
        dim_line(
            "Its conversation shows here once read (v on the grid); Enter opens it in the browser.",
            th,
        ),
    ]
}

fn diff_line(line: &str, th: Theme) -> Line<'static> {
    let color = match classify_diff_line(line) {
        DiffLineKind::Add => th.ok,
        DiffLineKind::Remove => th.err,
        DiffLineKind::Hunk => th.accent,
        DiffLineKind::Header => th.muted,
        DiffLineKind::Context => th.text,
    };
    Line::from(Span::styled(line.to_string(), Style::default().fg(color)))
}

/// `lines` from `scroll` down, the scroll clamped so the last line can
/// reach the bottom and no further.
fn scrolled(f: &mut Frame, body: Rect, lines: Vec<Line<'static>>, scroll: &mut u16) {
    let max = lines.len().saturating_sub(body.height as usize) as u16;
    *scroll = (*scroll).min(max);
    let shown: Vec<Line> = lines
        .into_iter()
        .skip(*scroll as usize)
        .take(body.height as usize)
        .collect();
    f.render_widget(Paragraph::new(shown), body);
}

fn dim_line(text: &str, th: Theme) -> Line<'static> {
    Line::from(Span::styled(text.to_string(), Style::default().fg(th.dim)))
}

fn dim_text(f: &mut Frame, body: Rect, text: &str, th: Theme) {
    f.render_widget(
        Paragraph::new(dim_line(text, th)).wrap(Wrap { trim: false }),
        body,
    );
}

/// F10's start prompt.
pub(super) fn draw_start(
    f: &mut Frame,
    app: &mut App,
    mut view: crate::orchestrator_view::StartView,
) {
    let th = app.chrome.theme;
    let area = centered_rect(f.area(), 58, 9);
    let inner = render_modal_frame(f, area, " Start the orchestrator ", th);
    let row = |label: &str, value: String, focused: bool| {
        let style = if focused {
            Style::default().fg(th.on_accent).bg(th.accent)
        } else {
            Style::default().fg(th.text)
        };
        Line::from(vec![
            Span::styled(format!(" {label:<9}"), Style::default().fg(th.muted)),
            Span::styled(format!(" ‹ {value} › "), style),
        ])
    };
    let lines = vec![
        dim_line(" One session that coordinates every other one.", th),
        Line::from(""),
        row("Harness", view.kind().as_str().to_string(), view.row == 0),
        row("Profile", view.profile_label(), view.row == 1),
        Line::from(""),
        dim_line(" ↑/↓ row · ←/→ change · Enter start · Esc cancel", th),
    ];
    f.render_widget(Paragraph::new(lines), inner);
    view.area = area;
    if let Some(Overlay::OrchestratorStart(v)) = &mut app.modals.overlay {
        *v = view;
    }
}
