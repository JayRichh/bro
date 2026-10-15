//! The KANBAN (`⇧K`; Settings → Appearance → **Kanban**): the LAUNCHER
//! VIEW's grid laid out as five lanes by what each session wants from you
//! — Running, Needs you, Review, Ship, Done (`launcher::Lane`) —
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
        Lane::Review => th.done,
        Lane::Done => th.ok,
        Lane::Ship => th.accent,
    }
}

/// The empty-state copy ([`nebula_config::learn::EMPTY`]) a lane shows
/// with no cards.
fn lane_empty_id(lane: Lane) -> &'static str {
    match lane {
        Lane::Running => "lane_running",
        Lane::NeedsInput => "lane_needs",
        Lane::Review => "lane_review",
        Lane::Ship => "lane_ship",
        Lane::Done => "lane_done",
    }
}

/// The narrowest the lanes go beside a docked review: five of 14 columns
/// and their gaps, so a card still shows its name.
const DOCKED_LANES_W: u16 = Lane::ALL.len() as u16 * 15 - 1;
/// The narrowest docked review: its file list and the diff beside it.
const DOCK_MIN_W: u16 = 72;

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
    // The REVIEW UNIT docks its diff on the board's right: the lanes keep
    // the left, where they still fit five abreast with a card's name
    // legible; a board too narrow for both (lanes and a diff of its two
    // panes) gives the diff the whole of it.
    app.launcher.review_dock = Rect::default();
    if review_docked(app) {
        let lanes_w = (area.width * 2 / 5).max(DOCKED_LANES_W);
        if area.width >= lanes_w + 1 + DOCK_MIN_W {
            app.launcher.review_dock = Rect {
                x: area.x + lanes_w + 1,
                width: area.width - lanes_w - 1,
                ..area
            };
            area.width = lanes_w;
        } else {
            app.launcher.review_dock = area;
        }
    }
    if area.height <= LANE_HEAD_H || area.width < Lane::ALL.len() as u16 * 8 {
        return;
    }
    let hidden = crate::launcher::kanban_hidden(app, app.launcher.kanban_all);
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
            // A held card over a lane that refuses it: the title says why,
            // dimmed, until the card moves on (the drop flashes it too).
            let title = match refusal(app, lane) {
                Some(why) => Line::from(Span::styled(
                    truncate(why, head.width as usize),
                    Style::default().fg(th.dim).add_modifier(Modifier::ITALIC),
                )),
                None => Line::from(vec![
                    Span::styled("● ", Style::default().fg(color)),
                    Span::styled(count, Style::default().fg(th.dim)),
                    Span::styled(
                        truncate(lane.title(), room),
                        Style::default().fg(color).add_modifier(Modifier::BOLD),
                    ),
                ]),
            };
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
        // The lane's foot says what the scope leaves out of it.
        let outside = hidden[i];
        let foot = u16::from(outside > 0);
        if outside > 0 {
            if let Some(line) = row_rect(r, r.height.saturating_sub(1) as usize)
                .filter(|_| r.height > LANE_HEAD_H + 1)
            {
                // The count whole: a narrow lane says it shorter.
                let long = format!("{outside} more outside scope");
                let said = if long.chars().count() <= line.width as usize {
                    long
                } else {
                    truncate(&format!("+{outside} outside"), line.width as usize)
                };
                f.render_widget(
                    Paragraph::new(Span::styled(said, Style::default().fg(th.dim))),
                    line,
                );
            }
        }
        let r = Rect {
            height: r.height.saturating_sub(foot),
            ..r
        };
        let room = r.height.saturating_sub(LANE_HEAD_H);
        let fit = (room / CARD_H).max(1) as usize;
        let at = cursor.filter(|(l, _)| *l == i).map(|(_, row)| row);
        // The lane scrolls under its cursor, the card it is on kept whole
        // on screen; the others show their top.
        let start = at.map_or(0, |row| (row + 1).saturating_sub(fit));
        if cards.is_empty() {
            // An empty lane says what lands in it (and, for Running, the
            // key that starts something), wrapped to the lane.
            let copy = Rect {
                y: r.y + LANE_HEAD_H,
                height: r.height.saturating_sub(LANE_HEAD_H).min(4),
                ..r
            };
            if copy.height > 0 {
                f.render_widget(
                    Paragraph::new(Span::styled(
                        crate::learn::empty_line(app, lane_empty_id(lane)),
                        Style::default().fg(th.dim),
                    ))
                    .wrap(ratatui::widgets::Wrap { trim: true }),
                    copy,
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
            let label = app.launcher.kanban_all
                || app.launcher.scope.repo.is_some()
                || app.launcher.scope.worktree.is_some();
            let taken = if label {
                draw_project_label(f, rect, &row.project, th)
            } else {
                0
            };
            draw_card_facts(f, app, rect, row, taken, th);
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
            let start = crate::learn::empty_line(app, "board");
            let what = if app.launcher.kanban_all {
                start
            } else {
                format!(
                    "{start}  {}: every project",
                    crate::ui::key_hint(app, crate::keymap::Action::FocusNext)
                )
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
    // let go on a lane's air is let go on the lane. Only while a card is
    // pressed: otherwise the air is the board's, its menu and its wheel.
    let pressed = app
        .launcher
        .order_drag
        .as_ref()
        .is_some_and(|d| matches!(d.item, OrderItem::Board { .. }));
    if pressed {
        hits.extend(lane_hits);
    }
    app.chrome.hits.extend(hits);
    app.chrome
        .hits
        .push((area, HitTarget::PanelBg(Focus::Sessions)));
}

/// Is the REVIEW UNIT's diff docked over the board?
fn review_docked(app: &App) -> bool {
    matches!(
        &app.modals.overlay,
        Some(crate::app::Overlay::Diff(v)) if v.review.is_some()
    )
}

/// Why letting the held card go on `lane` is refused, while it is over
/// that lane.
fn refusal(app: &App, lane: Lane) -> Option<&'static str> {
    let drag = held_card(app)?;
    let OrderItem::Board {
        agent, lane: from, ..
    } = &drag.item
    else {
        return None;
    };
    match &drag.over {
        Some(OrderOver::Lane(DropLane::Lane(to), None)) if *to == lane && *from != lane => {
            match lane_drop(app, agent, *from, DropLane::Lane(lane)) {
                LaneDrop::Refuse(why) => Some(why),
                _ => None,
            }
        }
        _ => None,
    }
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
    let OrderItem::Board {
        agent, lane: from, ..
    } = &drag.item
    else {
        return None;
    };
    match &drag.over {
        Some(OrderOver::Lane(DropLane::Lane(to), None)) if *to == lane && *from != lane => {
            Some(!matches!(
                lane_drop(app, agent, *from, DropLane::Lane(lane)),
                LaneDrop::Refuse(_)
            ))
        }
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
        "─".repeat(rect.width as usize),
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

/// What a board card says about its checkout on its top border, right:
/// the commits since its base (`3 commits`) and its pull request in the
/// PR rows' own colors (`#12 ready`) — what a band's rule says on the
/// grid, which the board has no rule for. `taken` columns at the left
/// are the project's label; what does not fit after it is left off,
/// the pull request first kept.
fn draw_card_facts(
    f: &mut Frame,
    app: &App,
    rect: Rect,
    row: &crate::launcher::LauncherRow,
    taken: u16,
    th: Theme,
) {
    let facts = crate::launcher::card_facts(app, row);
    let mut parts: Vec<Span<'static>> = Vec::new();
    if let Some(pr) = &row.pr {
        let look = crate::pr_row::look(pr.standing, pr.trouble, th);
        parts.push(Span::styled(
            format!(" #{} {} ", pr.number, pr.badge()),
            Style::default().fg(look.badge),
        ));
    }
    if facts.ahead_of_base > 0 {
        let n = facts.ahead_of_base;
        parts.insert(
            0,
            Span::styled(
                format!(" {n} commit{} ", if n == 1 { "" } else { "s" }),
                Style::default().fg(th.muted),
            ),
        );
    }
    // Two corners and a dash of rule either side; the label's room.
    let room = rect.width.saturating_sub(4 + taken + u16::from(taken > 0)) as usize;
    while parts.iter().map(Span::width).sum::<usize>() > room && !parts.is_empty() {
        parts.remove(0);
    }
    let w = parts.iter().map(Span::width).sum::<usize>() as u16;
    if w == 0 {
        return;
    }
    let at = Rect {
        x: rect.right().saturating_sub(2 + w),
        y: rect.y,
        width: w,
        height: 1,
    };
    f.render_widget(Paragraph::new(Line::from(parts)), at);
}

/// On the all-projects board, a card's project on its top border, left,
/// where a band's rule would have named it. The columns it took.
fn draw_project_label(f: &mut Frame, rect: Rect, project: &str, th: Theme) -> u16 {
    let room = (rect.width as usize).saturating_sub(6);
    if room < 3 {
        return 0;
    }
    // Short enough to leave the right of the border to the card's facts.
    let label = format!(" {} ", truncate(project, room.min(PROJECT_LABEL_MAX)));
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
    at.width
}

/// The most of a project's name a board card's border spells out.
const PROJECT_LABEL_MAX: usize = 14;
