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
use crate::app::{App, Focus, HitTarget, OrderItem, OrderOver};
use crate::launcher::{Card, Lane, CARD_H};
use crate::order::{lane_drop, DropLane, LaneDrop, Place};
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
    let mut area = g.area;
    if area.height <= LANE_HEAD_H || area.width < Lane::ALL.len() as u16 * 8 {
        return;
    }
    // A card held: the ARCHIVED zone takes the board's foot, under the
    // lanes rather than beside them, so no lane moves under the pointer.
    let held = held_card(app);
    let archive = (held.is_some() && area.height > LANE_HEAD_H + CARD_H + ARCHIVE_H).then(|| {
        let zone = Rect {
            y: area.bottom() - ARCHIVE_H,
            height: ARCHIVE_H,
            ..area
        };
        area.height -= ARCHIVE_H + 1;
        zone
    });
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
    let mut lane_hits = Vec::new();
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
        // The lane a held card is over: its rule in the accent for a drop
        // that acts, the error's for one it refuses.
        let rule = match lane_mark(app, lane) {
            Some(true) => th.accent,
            Some(false) => th.err,
            None if cards.is_empty() => th.edge,
            None => color,
        };
        draw_rule(f, r, 1, rule);
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
            lane_hits.push((r, HitTarget::KanbanLane(i)));
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
            let placed = crate::launcher::Placed {
                rect,
                cut_top: 0,
                cut_bottom: 0,
            };
            // The card under the cursor's buttons, its diff first, giving
            // way to the SUBAGENT CHIP at the left end of the same edge;
            // both registered ahead of the card, so they win the click.
            if selected && !app.launcher.launcher_unaimed {
                let reserve = crate::ui::subagents_view::card_chip_width(app, &card);
                let buttons = super::card_buttons(
                    &card,
                    &super::ButtonsOn::Board(row.agent.id.clone()),
                    placed,
                    reserve,
                );
                super::draw_card_buttons(f.buffer_mut(), app, &buttons);
                hits.extend(buttons.into_iter().map(|(rect, hit, _)| (rect, hit)));
            }
            hits.extend(crate::ui::subagents_view::card_chip_hit(app, &card, placed));
            draw_beside_mark(f, app, rect, &row.agent.id);
            hits.push((rect, HitTarget::KanbanCard(row.agent.id.clone())));
        }
        lane_hits.push((r, HitTarget::KanbanLane(i)));
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
    if let Some(zone) = archive {
        draw_archive_zone(f, app, zone);
        hits.push((zone, HitTarget::KanbanArchive));
    }
    // The lanes after their cards, so a card wins the click; a held card
    // let go on a lane's air is let go on the lane.
    hits.extend(lane_hits);
    app.chrome.hits.extend(hits);
    app.chrome
        .hits
        .push((area, HitTarget::PanelBg(Focus::Sessions)));
}

/// Rows the ARCHIVED zone takes: its box, a line of words inside.
const ARCHIVE_H: u16 = 3;

/// The board card held under the button, once the drag has started.
fn held_card(app: &App) -> Option<&crate::app::OrderDrag> {
    app.launcher
        .order_drag
        .as_ref()
        .filter(|d| d.active && matches!(d.item, OrderItem::Board { .. }))
}

/// Whether a held card is over `lane` (and not beside a card of it):
/// Some(true) when letting go there acts, Some(false) when it is refused.
fn lane_mark(app: &App, lane: Lane) -> Option<bool> {
    let drag = held_card(app)?;
    let OrderItem::Board { lane: from, .. } = &drag.item else {
        return None;
    };
    match &drag.over {
        Some(OrderOver::Lane(DropLane::Lane(to), None)) if *to == lane && *from != lane => Some(
            !matches!(lane_drop(*from, DropLane::Lane(lane)), LaneDrop::Refuse(_)),
        ),
        _ => None,
    }
}

/// The insert mark of a held card about to land beside `id`: an accent
/// rule along the card's top edge (before it) or its bottom (after).
fn draw_beside_mark(f: &mut Frame, app: &App, rect: Rect, id: &nebula_core::AgentId) {
    let Some(OrderOver::Lane(_, Some((target, place)))) =
        held_card(app).and_then(|d| d.over.as_ref())
    else {
        return;
    };
    if target != id {
        return;
    }
    let y = match place {
        Place::Before => rect.y,
        Place::After => rect.bottom().saturating_sub(1),
    };
    let th = app.chrome.theme;
    f.buffer_mut().set_stringn(
        rect.x,
        y,
        "━".repeat(rect.width as usize),
        rect.width as usize,
        Style::default().fg(th.accent),
    );
}

/// The ARCHIVED zone along the board's foot while a card is held: a box
/// that says what letting go there does, lit in the accent once the card
/// is over it.
fn draw_archive_zone(f: &mut Frame, app: &App, zone: Rect) {
    let th = app.chrome.theme;
    let over = held_card(app)
        .is_some_and(|d| matches!(d.over, Some(OrderOver::Lane(DropLane::Archived, _))));
    let edge = if over { th.accent } else { th.edge };
    let block = ratatui::widgets::Block::default()
        .borders(ratatui::widgets::Borders::ALL)
        .border_style(Style::default().fg(edge))
        .title(Span::styled(
            " Archived ",
            Style::default().fg(if over { th.accent } else { th.muted }),
        ));
    let inner = block.inner(zone);
    f.render_widget(block, zone);
    f.render_widget(
        Paragraph::new(Span::styled(
            "let go here to archive the session (u brings it back)",
            Style::default().fg(if over { th.text } else { th.dim }),
        ))
        .alignment(ratatui::layout::Alignment::Center),
        inner,
    );
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
