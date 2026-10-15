//! The LAUNCHER VIEW header's ported pieces, laid in the room the PROJECT
//! TABS and the counts leave between them: BACK and FORWARD (`‹ ›`, as
//! `⌥←` / `⌥→`) and the ATTENTION STRIP — `2 need you · 3 running ·
//! 1 unread` across every project (`crate::attention`), each count a
//! button that opens the next session in that state.

use super::*;
use crate::subagents::Tone;

/// One piece of the row: what it draws, and what a click on it means.
type Part = Vec<(Span<'static>, Option<HitTarget>)>;

fn width(part: &Part) -> usize {
    part.iter().map(|(s, _)| s.width()).sum()
}

/// `‹ ›`: lit where there is somewhere to go, dim where not, underlined
/// under the pointer.
fn nav_part(app: &App, th: Theme) -> Part {
    let button = |glyph: &'static str, can: bool, hit: HitTarget| {
        let mut style = Style::default().fg(if can { th.muted } else { th.dim });
        if can && app.launcher.hover_crumb.as_ref() == Some(&hit) {
            style = Style::default()
                .fg(th.accent)
                .add_modifier(Modifier::UNDERLINED);
        }
        (Span::styled(glyph, style), can.then_some(hit))
    };
    vec![
        button("‹", app.desk.history.can_back(), HitTarget::NavBack),
        (Span::raw(" "), None),
        button("›", app.desk.history.can_forward(), HitTarget::NavForward),
    ]
}

/// The strip's counts — every one when `zeros`, else only the lit ones.
/// `failed` only when something failed, as the desktop's does.
fn strip_part(app: &App, th: Theme, zeros: bool) -> Part {
    let counts = crate::attention::tally(app);
    let mut out: Part = Vec::new();
    for tone in crate::attention::KINDS {
        let n = counts.get(&tone).map_or(0, Vec::len);
        if (tone == Tone::Failed || !zeros) && n == 0 {
            continue;
        }
        if !out.is_empty() {
            out.push((Span::styled(" · ", Style::default().fg(th.dim)), None));
        }
        let hot = n > 0 && matches!(tone, Tone::Needs | Tone::Failed);
        let fg = if hot {
            crate::ui::subagents_view::tone_color(tone, th)
        } else if n > 0 {
            th.text
        } else {
            th.dim
        };
        let hit = (n > 0).then_some(HitTarget::Attention(tone));
        let mut style = Style::default().fg(fg);
        if hot {
            style = style.add_modifier(Modifier::BOLD);
        }
        if hit.is_some() && app.launcher.hover_crumb == hit {
            style = style.add_modifier(Modifier::UNDERLINED);
        }
        out.push((
            Span::styled(format!("{n} {}", crate::attention::word(tone, n)), style),
            hit,
        ));
    }
    out
}

/// What the gap `r` holds: `‹ ›`, then the strip with its zeros, or
/// without them, or not at all — the most of it that fits.
pub(super) fn parts(app: &App, room: usize) -> Part {
    let th = app.chrome.theme;
    let nav = nav_part(app, th);
    if width(&nav) > room {
        return Vec::new();
    }
    let gap: Part = vec![(Span::raw("   "), None)];
    for zeros in [true, false] {
        let strip = strip_part(app, th, zeros);
        if strip.is_empty() {
            break;
        }
        if width(&nav) + width(&gap) + width(&strip) <= room {
            return nav.into_iter().chain(gap).chain(strip).collect();
        }
    }
    nav
}

/// Lay [`parts`] into `r`, registering each button's cell.
pub(super) fn draw(f: &mut Frame, app: &mut App, r: Rect) {
    let parts = parts(app, usize::from(r.width));
    let mut x = r.x;
    let mut spans = Vec::with_capacity(parts.len());
    for (span, hit) in parts {
        let w = span.width() as u16;
        if let Some(hit) = hit {
            app.chrome.hits.push((Rect { x, width: w, ..r }, hit));
        }
        x += w;
        spans.push(span);
    }
    f.render_widget(Paragraph::new(Line::from(spans)), r);
}
