//! The AZURE DEVOPS OVERLAY's drawing (`crate::ado`): a tab strip of its
//! three lists, the list under it, a line saying what Enter does there.

use super::*;
use crate::ado::{AdoTab, AdoView, Load};

const WIDTH: u16 = 104;
const HEIGHT: u16 = 26;

/// The list's rows as spans, or the line that stands in for them.
fn rows(app: &App, view: &AdoView, width: usize) -> Result<Vec<Vec<Span<'static>>>, String> {
    let th = app.chrome.theme;
    let dim = Style::default().fg(th.dim);
    let fit = |s: &str, w: usize| truncate(s, w);
    let failed = |e: &str| Err(format!("could not read it: {e}"));
    match view.tab {
        AdoTab::Tasks => match &view.tasks {
            Load::Loading => Err("loading the sprint…".into()),
            Load::Failed(e) => failed(e),
            Load::Ready(r) if r.tasks.is_empty() => Err(format!("{}: nothing for you", r.header)),
            Load::Ready(r) => Ok(r
                .tasks
                .iter()
                .map(|t| {
                    let state_fg = match t.state.as_str() {
                        "Active" | "In Progress" | "Committed" => th.warn,
                        "Done" | "Closed" | "Resolved" => th.ok,
                        _ => th.muted,
                    };
                    let parent = t
                        .parent
                        .as_deref()
                        .map(|p| format!("{} › ", fit(p, width / 4)))
                        .unwrap_or_default();
                    let who = t.assigned_to.clone().unwrap_or_default();
                    let fixed = 8 + parent.chars().count() + 14 + who.chars().count() + 4;
                    vec![
                        Span::styled(format!("#{:<6} ", t.id), Style::default().fg(th.accent)),
                        Span::styled(parent, dim),
                        Span::styled(
                            fit(&t.title, width.saturating_sub(fixed).max(10)),
                            Style::default().fg(th.text),
                        ),
                        Span::styled(format!("  {}", t.state), Style::default().fg(state_fg)),
                        Span::styled(format!("  {}", t.kind), dim),
                        Span::styled(format!("  {who}"), dim),
                    ]
                })
                .collect()),
        },
        AdoTab::PullRequests => match &view.prs {
            Load::Loading => Err("loading the pull requests…".into()),
            Load::Failed(e) => failed(e),
            Load::Ready(p) if p.is_empty() => {
                Err("no active pull requests of yours or for your review".into())
            }
            Load::Ready(p) => Ok(p
                .iter()
                .map(|pr| {
                    let whose = if pr.mine { "yours" } else { "review" };
                    let draft = if pr.is_draft { "  draft" } else { "" };
                    let branch = format!("  {} → {}", pr.source, pr.target);
                    let fixed = 8 + 8 + draft.len() + branch.chars().count() + 2;
                    vec![
                        Span::styled(format!("!{:<6} ", pr.id), Style::default().fg(th.accent)),
                        Span::styled(
                            format!("{whose:<7} "),
                            Style::default().fg(if pr.mine { th.muted } else { th.warn }),
                        ),
                        Span::styled(
                            fit(&pr.title, width.saturating_sub(fixed).max(10)),
                            Style::default().fg(if pr.is_draft { th.muted } else { th.text }),
                        ),
                        Span::styled(draft, dim),
                        Span::styled(branch, dim),
                    ]
                })
                .collect()),
        },
        AdoTab::Branches => match &view.branches {
            Load::Loading => Err("reading the branches…".into()),
            Load::Failed(e) => failed(e),
            Load::Ready(b) if b.is_empty() => Err("no branches".into()),
            Load::Ready(b) => Ok(b
                .iter()
                .map(|br| {
                    let checked_out = app.tree.worktrees.iter().any(|w| {
                        Some(&w.project_id) == view.project.as_ref() && w.branch == br.name
                    });
                    let place = match (br.local, br.remote) {
                        (true, true) => "local · remote",
                        (true, false) => "local",
                        _ => "remote",
                    };
                    let mut spans = vec![
                        Span::styled(
                            fit(&br.name, width.saturating_sub(30).max(10)),
                            Style::default().fg(if checked_out { th.worktree } else { th.text }),
                        ),
                        Span::styled(format!("  {place}"), dim),
                    ];
                    if checked_out {
                        spans.push(Span::styled("  in a worktree", Style::default().fg(th.ok)));
                    }
                    spans
                })
                .collect()),
        },
    }
}

/// What Enter (and the tab's other key) does on this tab.
fn hint(tab: AdoTab) -> &'static str {
    match tab {
        AdoTab::Tasks => {
            "Enter: open in browser   s s: start task (branch, worktree, agent)   r: reload"
        }
        AdoTab::PullRequests => "Enter: open in browser   r: reload",
        AdoTab::Branches => "Enter: check out in a worktree (or go to it)   r: reload",
    }
}

pub(super) fn draw(f: &mut Frame, app: &mut App, view: AdoView) {
    let th = app.chrome.theme;
    let area = centered_rect(
        f.area(),
        WIDTH,
        HEIGHT.min(f.area().height.saturating_sub(2)),
    );
    let name = view
        .project
        .as_ref()
        .and_then(|id| app.tree.projects.iter().find(|p| &p.id == id))
        .map_or_else(String::new, |p| p.name.clone());
    let inner = render_modal_frame(f, area, format!(" Azure DevOps · {name} "), th);
    // The tab strip: the lit tab as the settings overlay's is, the others
    // buttons; `1 2 3` are their keys.
    let mut tab_hits = Vec::new();
    if let Some(strip) = row_rect(inner, 0) {
        let mut x = strip.x + 1;
        let mut spans = vec![Span::raw(" ")];
        for (i, tab) in AdoTab::ALL.into_iter().enumerate() {
            let text = format!(" {} {} ", i + 1, tab.label());
            let w = text.chars().count() as u16;
            let style = if tab == view.tab {
                Style::default()
                    .fg(th.accent)
                    .bg(th.sel_bg)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(th.muted)
            };
            tab_hits.push((
                Rect {
                    x,
                    width: w,
                    ..strip
                },
                tab,
            ));
            spans.push(Span::styled(text, style));
            spans.push(Span::raw(" "));
            x += w + 1;
        }
        if let (AdoTab::Tasks, Load::Ready(r)) = (view.tab, &view.tasks) {
            spans.push(Span::styled(
                format!("  {}", r.header),
                Style::default().fg(th.dim),
            ));
        }
        f.render_widget(Paragraph::new(Line::from(spans)), strip);
    }
    let body = Rect {
        y: inner.y + 2,
        height: inner.height.saturating_sub(3),
        ..inner
    };
    if let Some(foot) = row_rect(inner, usize::from(inner.height.saturating_sub(1))) {
        f.render_widget(
            Paragraph::new(Span::styled(
                format!(" {}", hint(view.tab)),
                Style::default().fg(th.dim),
            )),
            foot,
        );
    }
    let width = usize::from(body.width).saturating_sub(2);
    let fit = usize::from(body.height.max(1));
    let cursor = view.cursor();
    let mut start = view.scroll.min(cursor);
    if cursor >= start + fit {
        start = cursor + 1 - fit;
    }
    match rows(app, &view, width) {
        Err(line) => empty_list_row(f, body, &format!(" {line}"), th),
        Ok(rows) => {
            for (i, spans) in rows.into_iter().enumerate().skip(start).take(fit) {
                if let Some(r) = row_rect(body, i - start) {
                    render_row(f, r, spans, i == cursor, true, th);
                }
            }
        }
    }
    if let Some(Overlay::Ado(v)) = &mut app.modals.overlay {
        v.area = area;
        v.list_area = body;
        v.tab_hits = tab_hits;
        v.scroll = start;
    }
}
