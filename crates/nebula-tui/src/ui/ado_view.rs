//! The AZURE DEVOPS OVERLAY's drawing (`crate::ado`): a tab strip of its
//! three lists, the list under it (or a pull request's threads), a line
//! saying what Enter does there with the tab's buttons.

use super::*;
use crate::ado::{AdoButton, AdoTab, AdoView, Load, PrThreads, VOTES};

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
                    let place = match (br.local, br.remote.is_some()) {
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

/// The bottom row: words, then buttons (each `[key label]`, a click its
/// key). Returns each button's rect.
fn foot(
    f: &mut Frame,
    row: Rect,
    words: &str,
    buttons: &[(String, AdoButton)],
    th: Theme,
) -> Vec<(Rect, AdoButton)> {
    let mut spans = Vec::new();
    let mut hits = Vec::new();
    let mut x = row.x + 1;
    spans.push(Span::raw(" "));
    for (label, b) in buttons {
        let text = format!("[{label}]");
        let w = text.chars().count() as u16;
        if x + w > row.right() {
            break;
        }
        hits.push((Rect { x, width: w, ..row }, *b));
        spans.push(Span::styled(text, Style::default().fg(th.accent)));
        spans.push(Span::raw(" "));
        x += w + 1;
    }
    spans.push(Span::styled(
        format!(" {words}"),
        Style::default().fg(th.dim),
    ));
    f.render_widget(Paragraph::new(Line::from(spans)), row);
    hits
}

/// A pull request's threads in place of the list: each thread's place
/// and status (the cursor's lit), its comments under it, and the reply
/// being typed. Returns each thread's header rect.
fn draw_threads(f: &mut Frame, body: Rect, t: &PrThreads, th: Theme) -> Vec<(Rect, usize)> {
    let dim = Style::default().fg(th.dim);
    let width = usize::from(body.width).saturating_sub(4);
    let head = Line::from(vec![
        Span::styled(format!(" !{} ", t.pr), Style::default().fg(th.accent)),
        Span::styled(
            truncate(&t.title, width.saturating_sub(10)),
            Style::default().fg(th.text),
        ),
        Span::styled(if t.is_draft { "  draft" } else { "" }, dim),
    ]);
    f.render_widget(Paragraph::new(head), Rect { height: 1, ..body });
    let list = Rect {
        y: body.y + 1,
        height: body.height.saturating_sub(1),
        ..body
    };
    let threads = match &t.threads {
        Load::Loading => {
            empty_list_row(f, list, " loading the threads…", th);
            return Vec::new();
        }
        Load::Failed(e) => {
            empty_list_row(f, list, &format!(" could not read them: {e}"), th);
            return Vec::new();
        }
        Load::Ready(v) if v.is_empty() => {
            empty_list_row(
                f,
                list,
                " no comments yet: open it in the browser to start one",
                th,
            );
            return Vec::new();
        }
        Load::Ready(v) => v,
    };
    // Each thread is its header, its comments (last few), and under the
    // cursor's the reply box: laid out from the cursor's thread up so it
    // is always on screen.
    let cursor = t.cursor.min(threads.len() - 1);
    let mut lines: Vec<(Line<'static>, Option<usize>)> = Vec::new();
    let mut cursor_at = 0;
    for (i, thread) in threads.iter().enumerate() {
        if i == cursor {
            cursor_at = lines.len();
        }
        let lit = i == cursor;
        let style = if lit {
            Style::default().fg(th.on_accent).bg(th.accent)
        } else {
            Style::default().fg(th.text)
        };
        lines.push((
            Line::from(vec![
                Span::styled(format!(" {} ", thread.place()), style),
                Span::styled(format!("  {}", thread.status), dim),
                Span::styled(format!("  #{}", thread.id), dim),
            ]),
            Some(i),
        ));
        for (who, text) in thread.comments.iter().rev().take(3).rev() {
            let text = text.lines().next().unwrap_or("");
            lines.push((
                Line::from(vec![
                    Span::styled(format!("   {who}: "), Style::default().fg(th.muted)),
                    Span::styled(
                        truncate(text, width.saturating_sub(who.chars().count() + 6)),
                        Style::default().fg(th.text),
                    ),
                ]),
                None,
            ));
        }
        if lit {
            if let Some(reply) = &t.reply {
                let cursor = if t.sending { " sending…" } else { "▏" };
                lines.push((
                    Line::from(vec![
                        Span::styled("   reply › ", Style::default().fg(th.accent)),
                        Span::styled(
                            format!("{}{cursor}", truncate(reply, width.saturating_sub(12))),
                            Style::default().fg(th.text),
                        ),
                    ]),
                    None,
                ));
            }
        }
    }
    let fit = usize::from(list.height);
    let start = cursor_at.saturating_sub(fit.saturating_sub(5).max(1) / 2);
    let start = start.min(lines.len().saturating_sub(fit));
    let mut hits = Vec::new();
    for (row, (line, thread)) in lines.into_iter().skip(start).take(fit).enumerate() {
        let r = Rect {
            y: list.y + row as u16,
            height: 1,
            ..list
        };
        if let Some(i) = thread {
            hits.push((r, i));
        }
        f.render_widget(Paragraph::new(line), r);
    }
    hits
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
    let mut buttons = Vec::new();
    if let Some(row) = row_rect(inner, usize::from(inner.height.saturating_sub(1))) {
        // The PR tab's verbs as buttons naming their keys: a vote's
        // choices while one is picked, the reply's while one is typed.
        let replying = view.threads.as_ref().is_some_and(|t| t.reply.is_some());
        let (words, bs): (String, Vec<(String, AdoButton)>) = if let Some(id) = view.voting {
            let mut bs: Vec<(String, AdoButton)> = VOTES
                .iter()
                .map(|(k, _, label)| (format!("{k} {label}"), AdoButton::Cast(*k)))
                .collect();
            bs.push(("Esc cancel".into(), AdoButton::Back));
            (format!("your vote on !{id}"), bs)
        } else if replying {
            (
                "type the reply".into(),
                vec![
                    ("Enter send".into(), AdoButton::Send),
                    ("Esc cancel".into(), AdoButton::Back),
                ],
            )
        } else if view.threads.is_some() {
            (
                "j/k: thread   r: reload".into(),
                vec![
                    ("c reply".into(), AdoButton::Comments),
                    ("v vote".into(), AdoButton::Vote),
                    ("Esc back".into(), AdoButton::Back),
                ],
            )
        } else if view.tab == AdoTab::PullRequests {
            (
                hint(view.tab).into(),
                vec![
                    ("v vote".into(), AdoButton::Vote),
                    ("c comments".into(), AdoButton::Comments),
                ],
            )
        } else {
            (hint(view.tab).into(), Vec::new())
        };
        buttons = foot(f, row, &words, &bs, th);
    }
    let mut thread_hits = Vec::new();
    if let (AdoTab::PullRequests, Some(t)) = (view.tab, &view.threads) {
        thread_hits = draw_threads(f, body, t, th);
        if let Some(Overlay::Ado(v)) = &mut app.modals.overlay {
            v.area = area;
            v.list_area = Rect::default();
            v.tab_hits = tab_hits;
            v.buttons = buttons;
            v.thread_hits = thread_hits;
        }
        return;
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
        v.buttons = buttons;
        v.thread_hits = thread_hits;
    }
}
