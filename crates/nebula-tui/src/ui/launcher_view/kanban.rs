//! The KANBAN (`⇧K`; Settings → Appearance → **Kanban**): the LAUNCHER
//! VIEW's grid laid out as five lanes by what each session wants from you
//! — Needs you, Running, Unread, Done, PR open (`launcher::Lane`) —
//! under the same PROJECT TABS header, beside the same PANE. Its cards
//! are the grid's own session cards ([`super::draw_any_card`]), stacked
//! down their lane, the most recently touched first; on the board across
//! every project (`Tab`) each card names its project on its top border.
//!
//! The cursor is the grid's own selection: `h`/`l` walk the lanes and
//! `j`/`k` the cards (`event_loop::launcher::kanban_action`), each step
//! landing the selection through the `/` PALETTE's jump, so Enter, Space,
//! `a`, `d`, `g` and every other key that acts on "the card under the
//! cursor" act on the card the board shows lit.

use super::{draw_any_card, draw_head, draw_rule, pad_x, row_rect, truncate, HeadCount};
use crate::app::{App, Focus, HitTarget};
use crate::launcher::{Card, Lane, CARD_H};
use crate::theme::Theme;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

/// Rows a lane's head takes: its title, the rule under it.
const LANE_HEAD_H: u16 = 2;

/// The color a lane is titled in: the STATUS DOT its cards wear.
fn lane_color(lane: Lane, th: Theme) -> Color {
    match lane {
        Lane::NeedsInput => th.err,
        Lane::Running => th.warn,
        Lane::Unseen => th.done,
        Lane::Done => th.ok,
        Lane::PrOpen => th.accent,
    }
}

/// The board, in `body`: the header, then the five lanes side by side.
pub(super) fn draw(f: &mut Frame, app: &mut App, body: Rect) {
    let th = app.chrome.theme;
    let lanes = crate::launcher::kanban(app, app.launcher.kanban_all);
    let total: usize = lanes.iter().map(Vec::len).sum();
    draw_head(
        f,
        app,
        body,
        HeadCount {
            sessions: total,
            terminals: 0,
        },
        crate::launcher::Hidden::default(),
    );
    let g = crate::launcher::bands_layout(body);
    let area = g.area;
    if area.height <= LANE_HEAD_H || area.width < Lane::ALL.len() as u16 * 8 {
        return;
    }
    let cursor = app
        .selected_session()
        .filter(|_| !app.launcher.launcher_unaimed)
        .and_then(|a| crate::launcher::kanban_find(&lanes, &a.id));
    let keys = app.nav.focus != Focus::Terminal && app.launcher.launcher_tab_cursor.is_none();
    let rects = Layout::horizontal(Lane::ALL.map(|_| Constraint::Ratio(1, Lane::ALL.len() as u32)))
        .spacing(1)
        .split(area);
    let mut cfg = None;
    let mut hits = Vec::new();
    for (i, lane) in Lane::ALL.into_iter().enumerate() {
        let r = rects[i];
        let cards = &lanes[i];
        let color = lane_color(lane, th);
        if let Some(head) = row_rect(r, 0) {
            // The count before the title, so a narrow lane clips the
            // word rather than the number.
            let count = format!("{} ", cards.len());
            let room = (head.width as usize).saturating_sub(2 + count.chars().count());
            let title = Line::from(vec![
                Span::styled("● ", Style::default().fg(color)),
                Span::styled(count, Style::default().fg(th.dim)),
                Span::styled(
                    truncate(lane.title(), room),
                    Style::default().fg(color).add_modifier(Modifier::BOLD),
                ),
            ]);
            f.render_widget(Paragraph::new(title), head);
        }
        draw_rule(f, r, 1, if cards.is_empty() { th.edge } else { color });
        let room = r.height.saturating_sub(LANE_HEAD_H);
        let fit = (room / CARD_H).max(1) as usize;
        let at = cursor.filter(|(l, _)| *l == i).map(|(_, row)| row);
        // The lane scrolls under its cursor, the card it is on kept whole
        // on screen; the others show their top.
        let start = at.map_or(0, |row| (row + 1).saturating_sub(fit));
        if cards.is_empty() {
            if let Some(line) = row_rect(r, LANE_HEAD_H as usize) {
                f.render_widget(
                    Paragraph::new(Span::styled("-", Style::default().fg(th.dim))),
                    line,
                );
            }
            continue;
        }
        for (n, row) in cards.iter().enumerate().skip(start).take(fit) {
            let y = r.y + LANE_HEAD_H + ((n - start) as u16) * CARD_H;
            if y + CARD_H > r.bottom() {
                break;
            }
            let rect = Rect {
                x: r.x,
                y,
                width: r.width,
                height: CARD_H,
            };
            let selected = at == Some(n);
            let card = Card::Session(row.clone());
            draw_any_card(
                f.buffer_mut(),
                &*app,
                rect,
                &card,
                selected,
                keys,
                th,
                &mut cfg,
            );
            if app.launcher.kanban_all {
                draw_project_label(f, rect, &row.project, th);
            }
            hits.push((rect, HitTarget::KanbanCard(row.agent.id.clone())));
        }
        let shown = (start + fit).min(cards.len());
        if shown < cards.len() {
            if let Some(line) = row_rect(r, r.height.saturating_sub(1) as usize) {
                f.render_widget(
                    Paragraph::new(Span::styled(
                        format!("↓ {} more", cards.len() - shown),
                        Style::default().fg(th.dim),
                    ))
                    .alignment(ratatui::layout::Alignment::Right),
                    line,
                );
            }
        }
    }
    if total == 0 {
        if let Some(line) = row_rect(area, (area.height / 2) as usize) {
            let what = if app.launcher.kanban_all {
                "no sessions in any project: p starts one"
            } else {
                "no sessions in this project: p starts one, Tab shows every project's"
            };
            f.render_widget(
                Paragraph::new(Span::styled(what, Style::default().fg(th.dim)))
                    .alignment(ratatui::layout::Alignment::Center),
                pad_x(line),
            );
        }
    }
    app.chrome.hits.extend(hits);
    app.chrome
        .hits
        .push((area, HitTarget::PanelBg(Focus::Sessions)));
}

/// On the all-projects board, a card's project on its top border, left,
/// where a band's rule would have named it.
fn draw_project_label(f: &mut Frame, rect: Rect, project: &str, th: Theme) {
    let room = (rect.width as usize).saturating_sub(6);
    if room < 3 {
        return;
    }
    let label = format!(" {} ", truncate(project, room));
    let at = Rect {
        x: rect.x + 2,
        y: rect.y,
        width: label.chars().count() as u16,
        height: 1,
    };
    f.render_widget(
        Paragraph::new(Span::styled(label, Style::default().fg(th.muted))),
        at,
    );
}
