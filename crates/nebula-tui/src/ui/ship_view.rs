//! The SHIP OVERLAY's drawing (`crate::ship_view`): the pull request
//! composer or the merge confirm, centered over the board. Every control
//! is a button naming its key, laid down where the row puts it so a click
//! is the key's twin.

use super::{centered_rect, input_spans, multiline_input_lines, truncate};
use crate::app::{App, Overlay};
use crate::ship_view::{
    error_action, method_word, Button, Composer, Field, Form, MergeForm, ShipView, Stage, METHODS,
};
use crate::theme::Theme;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Clear, Paragraph, Wrap};
use ratatui::Frame;

/// The widest the box gets.
const MAX_W: u16 = 78;
/// Rows the body field takes when the screen has them.
const BODY_ROWS: u16 = 7;
/// The width of a field's label column.
const LABEL_W: u16 = 7;

/// One row of spans, each maybe a button: drawn left to right in `r`, the
/// buttons' cells recorded.
fn row(
    f: &mut Frame,
    r: Rect,
    parts: Vec<(Span<'static>, Option<Button>)>,
    buttons: &mut Vec<(Rect, Button)>,
) {
    let mut x = r.x;
    let mut spans = Vec::new();
    for (span, button) in parts {
        let w = (span.width() as u16).min(r.right().saturating_sub(x));
        if let Some(b) = button.filter(|_| w > 0) {
            buttons.push((
                Rect {
                    x,
                    y: r.y,
                    width: w,
                    height: 1,
                },
                b,
            ));
        }
        x = x.saturating_add(w);
        spans.push(span);
    }
    f.render_widget(Paragraph::new(Line::from(spans)), r);
}

/// The i-th row of `inner`, when there is one.
fn line_at(inner: Rect, i: u16) -> Option<Rect> {
    (i < inner.height).then(|| Rect {
        y: inner.y + i,
        height: 1,
        ..inner
    })
}

pub(super) fn draw(f: &mut Frame, app: &mut App, mut view: ShipView) {
    let th = app.chrome.theme;
    let screen = f.area();
    let width = MAX_W.min(screen.width.saturating_sub(2)).max(10);
    let body_rows = BODY_ROWS.min(screen.height.saturating_sub(12)).max(1);
    let rows: u16 = match &view.form {
        // title, body, base, draft, gap, status ×2, buttons
        Form::Compose(_) => 1 + body_rows + 1 + 1 + 1 + 2 + 1,
        // pull request, method, branch, gap, status ×2, buttons
        Form::Merge(_) => 1 + 1 + 1 + 1 + 2 + 1,
    };
    let area = centered_rect(screen, width, (rows + 2).min(screen.height));
    f.render_widget(Clear, area);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(th.accent))
        .title(Span::styled(
            truncate(&view.title(), area.width.saturating_sub(4) as usize),
            Style::default().fg(th.accent).add_modifier(Modifier::BOLD),
        ));
    let inner = block.inner(area);
    f.render_widget(block, area);
    let mut buttons = Vec::new();
    let editing = view.stage == Stage::Editing;
    let next = match &view.form {
        Form::Compose(c) => draw_compose(f, inner, c, body_rows, editing, th, &mut buttons),
        Form::Merge(m) => draw_merge(f, inner, m, editing, th, &mut buttons),
    };
    draw_status(f, inner, next, &view, th, &mut buttons);
    view.area = area;
    view.buttons = buttons;
    if let Some(Overlay::Ship(live)) = &mut app.modals.overlay {
        live.area = view.area;
        live.buttons = view.buttons;
    }
}

/// A field's label, lit while the cursor is in it, as a button that puts
/// the cursor there.
fn label(text: &str, on: bool, th: Theme, field: Field) -> (Span<'static>, Option<Button>) {
    let style = if on {
        Style::default().fg(th.accent).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(th.muted)
    };
    (
        Span::styled(format!("{text:<w$}", w = LABEL_W as usize), style),
        Some(Button::Field(field)),
    )
}

/// The composer's fields; the row after them.
fn draw_compose(
    f: &mut Frame,
    inner: Rect,
    c: &Composer,
    body_rows: u16,
    editing: bool,
    th: Theme,
    buttons: &mut Vec<(Rect, Button)>,
) -> u16 {
    let room = inner.width.saturating_sub(LABEL_W) as usize;
    let caret = |on: bool| if on && editing { th.accent } else { th.dim };
    if let Some(r) = line_at(inner, 0) {
        let on = c.field == Field::Title;
        let mut parts = vec![label("Title", on, th, Field::Title)];
        let field: Vec<Span<'static>> = if c.title.is_empty() && !on {
            vec![Span::styled(
                "a title is needed".to_string(),
                Style::default().fg(th.dim),
            )]
        } else {
            input_spans(&c.title, room, caret(on), th)
        };
        parts.extend(
            field
                .into_iter()
                .map(|s| (s, Some(Button::Field(Field::Title)))),
        );
        row(f, r, parts, buttons);
    }
    // The body: its label on its first row, the text wrapped beside it,
    // scrolled to keep the caret in view.
    let on = c.field == Field::Body;
    let (lines, caret_row) = multiline_input_lines(&c.body, room.max(1), caret(on), th);
    let top = caret_row.saturating_sub(body_rows.saturating_sub(1) as usize);
    for i in 0..body_rows {
        let Some(r) = line_at(inner, 1 + i) else {
            break;
        };
        let head = if i == 0 {
            label("Body", on, th, Field::Body)
        } else {
            (
                Span::raw(" ".repeat(LABEL_W as usize)),
                Some(Button::Field(Field::Body)),
            )
        };
        let mut parts = vec![head];
        let text = if c.body.is_empty() && i == 0 && !on {
            Line::from(Span::styled(
                "what changed and why",
                Style::default().fg(th.dim),
            ))
        } else {
            lines.get(top + i as usize).cloned().unwrap_or_default()
        };
        parts.extend(
            text.spans
                .into_iter()
                .map(|s| (s, Some(Button::Field(Field::Body)))),
        );
        row(f, r, parts, buttons);
    }
    let at = 1 + body_rows;
    if let Some(r) = line_at(inner, at) {
        let on = c.field == Field::Base;
        let mut parts = vec![label("Base", on, th, Field::Base)];
        if c.base.is_empty() && !on {
            parts.push((
                Span::styled(
                    if c.suggesting {
                        "reading the review base…"
                    } else {
                        "the review base"
                    },
                    Style::default().fg(th.dim),
                ),
                Some(Button::Field(Field::Base)),
            ));
        } else {
            parts.extend(
                input_spans(&c.base, room, caret(on), th)
                    .into_iter()
                    .map(|s| (s, Some(Button::Field(Field::Base)))),
            );
        }
        row(f, r, parts, buttons);
    }
    if let Some(r) = line_at(inner, at + 1) {
        let on = c.field == Field::Draft;
        let mark = if c.draft { "[x]" } else { "[ ]" };
        let style = if on {
            Style::default().fg(th.accent).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(th.text)
        };
        row(
            f,
            r,
            vec![
                (
                    Span::styled(format!("{mark} Draft"), style),
                    Some(Button::Field(Field::Draft)),
                ),
                (Span::styled("  Space", Style::default().fg(th.dim)), None),
            ],
            buttons,
        );
    }
    at + 3
}

/// The merge confirm's rows; the row after them.
fn draw_merge(
    f: &mut Frame,
    inner: Rect,
    m: &MergeForm,
    editing: bool,
    th: Theme,
    buttons: &mut Vec<(Rect, Button)>,
) -> u16 {
    let _ = editing;
    if let Some(r) = line_at(inner, 0) {
        let text = truncate(&format!("#{} {}", m.number, m.title), r.width as usize);
        f.render_widget(
            Paragraph::new(Span::styled(text, Style::default().fg(th.text))),
            r,
        );
    }
    if let Some(r) = line_at(inner, 1) {
        let mut parts = vec![(
            Span::styled(
                format!("{:<w$}", "Method", w = LABEL_W as usize),
                Style::default().fg(th.muted),
            ),
            None,
        )];
        for method in METHODS {
            let on = m.method == method;
            let (mark, style) = if on {
                (
                    "(\u{2022})",
                    Style::default().fg(th.accent).add_modifier(Modifier::BOLD),
                )
            } else {
                ("( )", Style::default().fg(th.text))
            };
            parts.push((
                Span::styled(format!("{mark} {} ", method_word(method)), style),
                Some(Button::Method(method)),
            ));
        }
        parts.push((
            Span::styled(" \u{2190}/\u{2192}", Style::default().fg(th.dim)),
            None,
        ));
        row(f, r, parts, buttons);
    }
    if let Some(r) = line_at(inner, 2) {
        let mark = if m.delete_branch { "[x]" } else { "[ ]" };
        row(
            f,
            r,
            vec![
                (
                    Span::styled(
                        format!("{mark} Delete the remote branch (the worktree stays)"),
                        Style::default().fg(th.text),
                    ),
                    Some(Button::DeleteBranch),
                ),
                (Span::styled("  Space", Style::default().fg(th.dim)), None),
            ],
            buttons,
        );
    }
    4
}

/// Where the work stands — reading, working, or what failed and its fix
/// — and the answers along the foot.
fn draw_status(
    f: &mut Frame,
    inner: Rect,
    at: u16,
    view: &ShipView,
    th: Theme,
    buttons: &mut Vec<(Rect, Button)>,
) {
    let foot = inner.height.saturating_sub(1);
    let status = Rect {
        y: inner.y + at.min(foot),
        height: foot.saturating_sub(at).min(2),
        ..inner
    };
    let words = match &view.stage {
        Stage::Working => Some((
            match view.form {
                Form::Compose(_) => "pushing the branch and opening the pull request…",
                Form::Merge(_) => "merging…",
            }
            .to_string(),
            th.warn,
        )),
        Stage::Failed(err) => Some((err.message.clone(), th.err)),
        Stage::Editing => match &view.form {
            Form::Compose(c) if c.suggesting => {
                Some(("reading the commits since the base…".to_string(), th.dim))
            }
            _ => None,
        },
    };
    if let Some((text, color)) = words.filter(|_| status.height > 0) {
        f.render_widget(
            Paragraph::new(Span::styled(text, Style::default().fg(color)))
                .wrap(Wrap { trim: true }),
            status,
        );
    }
    let Some(r) = line_at(inner, foot) else {
        return;
    };
    let key = |label: String, button: Button, color| {
        (
            Span::styled(
                label,
                Style::default().fg(color).add_modifier(Modifier::BOLD),
            ),
            Some(button),
        )
    };
    let gap = || (Span::raw("  "), None);
    let parts = match &view.stage {
        Stage::Failed(err) => match error_action(err) {
            Some((_, words)) => vec![
                key(format!("[Enter] {words}"), Button::Fix, th.accent),
                gap(),
                key("[Esc] back".into(), Button::Dismiss, th.dim),
            ],
            None => vec![key("[Enter/Esc] back".into(), Button::Dismiss, th.dim)],
        },
        Stage::Working => vec![key(
            "[Esc] close (it still lands)".into(),
            Button::Cancel,
            th.dim,
        )],
        Stage::Editing => {
            let go = match view.form {
                Form::Compose(_) => "[Enter] Create",
                Form::Merge(_) => "[Enter] Merge",
            };
            let mut parts = vec![
                key(go.into(), Button::Submit, th.accent),
                gap(),
                key("[Esc] Cancel".into(), Button::Cancel, th.dim),
            ];
            if view.is_compose() {
                parts.push((
                    Span::styled(
                        "  Tab: next field \u{b7} \u{21e7}Enter: new line",
                        Style::default().fg(th.dim),
                    ),
                    None,
                ));
            }
            parts
        }
    };
    row(f, r, parts, buttons);
}
