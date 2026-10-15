//! LEARN IN PLACE, the terminal app's half: the FIRST-USE TIP on the
//! footer's padding row and the WHAT'S NEW page's trigger. The copy, the
//! rules (shown once, `show_tips`) and the seen-state are nebula-config's
//! ([`nebula_config::learn`]), shared with the desktop window: a tip seen
//! in one app is not shown again in the other.
//!
//! A tip shows the first time its concept is on screen — a selection
//! (the `?` menu), a narrowed scope or a board over several repos (the
//! scope chip), a card in Review (the lanes), a board of two cards or more
//! (drag), the review unit — one at a time, and is marked seen the moment
//! it shows. It stays until its `got it` or `no tips` is clicked, or a key
//! is pressed once it has been up for [`DWELL`] (the key still does what
//! it does). Settings → General → Show tips turns them all off.

use std::collections::BTreeSet;
use std::time::{Duration, Instant};

use nebula_config::learn::{self, TIP_DRAG, TIP_LANES, TIP_REVIEW, TIP_SCOPE, TIP_VERBS};
use ratatui::layout::Rect;

use crate::app::{App, Focus, Overlay};

/// How long a tip is up before a key press may dismiss it: long enough to
/// read a line, so the key that selected the card doesn't wipe the tip
/// that card brought up.
pub const DWELL: Duration = Duration::from_secs(3);

/// The two buttons on a tip's line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TipButton {
    /// Hide this tip (it is already seen).
    GotIt,
    /// Turn Show tips off.
    Off,
}

/// The tip on screen.
#[derive(Debug, Clone)]
pub struct ShownTip {
    pub id: &'static str,
    pub since: Instant,
}

/// The app's tip state. Unarmed (the default, an `App` a test builds) it
/// shows nothing and writes nothing; the event loop arms it from the
/// config at startup.
#[derive(Debug, Default)]
pub struct Learn {
    pub armed: bool,
    /// Write the seen-state to config.json. Off for a test that arms.
    pub persist: bool,
    pub show_tips: bool,
    pub seen: BTreeSet<String>,
    pub tip: Option<ShownTip>,
}

impl Learn {
    /// What [`nebula_config::learn::tip_due`] reads, from what is mirrored.
    fn due(&self, id: &str) -> bool {
        self.armed
            && self.show_tips
            && self.tip.is_none()
            && !self.seen.contains(id)
            && learn::tip(id).is_some()
    }
}

/// Arm the tips from `cfg` and say whether the WHAT'S NEW page is due:
/// `fresh` is no settings file before this run (a first install, which
/// records this release as seen instead). A due page is recorded as seen
/// here; the caller opens it.
pub fn arm(app: &mut App, cfg: &crate::config::Config, fresh: bool, persist: bool) -> bool {
    app.learn.armed = true;
    app.learn.persist = persist;
    sync(app, cfg);
    let current = learn::current_version();
    let due = learn::whats_new_due(&cfg.whats_new_seen, &current, fresh);
    if (due || fresh) && persist {
        let _ = crate::config::Config::update(|c| c.whats_new_seen = current.clone());
    }
    due
}

/// Mirror `show_tips` and the seen-state after the config changed (the
/// settings overlay, or the window wrote it). Off hides the tip up now.
pub fn sync(app: &mut App, cfg: &crate::config::Config) {
    if !app.learn.armed {
        return;
    }
    app.learn.show_tips = cfg.show_tips;
    app.learn.seen.extend(cfg.tips_seen.iter().cloned());
    if !cfg.show_tips {
        app.learn.tip = None;
    }
}

/// The concept on screen now that has a tip, most specific first.
fn concept(app: &App) -> Option<&'static str> {
    let l = &app.learn;
    if let Some(Overlay::Diff(v)) = &app.modals.overlay {
        return v.review.is_some().then_some(TIP_REVIEW);
    }
    if app.modals.overlay.is_some()
        || !app.launcher_grid()
        || app.nav.focus == Focus::Terminal
        || app.launcher.launcher_tab_cursor.is_some()
    {
        return None;
    }
    if app.launcher.launcher_kanban {
        if !l.seen.contains(TIP_LANES) || !l.seen.contains(TIP_DRAG) {
            let lanes = crate::launcher::kanban(app, app.launcher.kanban_all);
            let review = crate::launcher::Lane::ALL
                .iter()
                .position(|x| *x == crate::launcher::Lane::Review);
            if review.is_some_and(|i| !lanes[i].is_empty()) && !l.seen.contains(TIP_LANES) {
                return Some(TIP_LANES);
            }
            let total: usize = lanes.iter().map(Vec::len).sum();
            if total >= 2 && !l.seen.contains(TIP_DRAG) {
                return Some(TIP_DRAG);
            }
        }
        if app.tree.projects.len() > 1 && !l.seen.contains(TIP_SCOPE) {
            return Some(TIP_SCOPE);
        }
    }
    if !app.launcher.scope.is_everything() && !l.seen.contains(TIP_SCOPE) {
        return Some(TIP_SCOPE);
    }
    if crate::event_loop::verbs::selection(app).is_some() && !l.seen.contains(TIP_VERBS) {
        return Some(TIP_VERBS);
    }
    None
}

/// Before a draw: put up the tip for what is on screen, if one is due.
/// Returns the id it showed (already marked seen, and written when
/// persisting).
pub fn observe(app: &mut App) -> Option<&'static str> {
    if !app.learn.armed || !app.learn.show_tips || app.learn.tip.is_some() {
        return None;
    }
    let id = concept(app)?;
    if !app.learn.due(id) {
        return None;
    }
    app.learn.seen.insert(id.to_string());
    app.learn.tip = Some(ShownTip {
        id,
        since: Instant::now(),
    });
    app.chrome.dirty = true;
    if app.learn.persist {
        let _ = crate::config::Config::update(|c| {
            c.tips_seen.insert(id.to_string());
        });
    }
    Some(id)
}

/// A key press: it dismisses a tip that has been up for [`DWELL`].
pub fn on_key(app: &mut App, now: Instant) {
    if app
        .learn
        .tip
        .as_ref()
        .is_some_and(|t| now.duration_since(t.since) >= DWELL)
    {
        app.learn.tip = None;
        app.chrome.dirty = true;
    }
}

/// A click on one of the tip's buttons.
pub fn click(app: &mut App, button: TipButton) {
    app.learn.tip = None;
    app.chrome.dirty = true;
    if button == TipButton::Off {
        app.learn.show_tips = false;
        if app.learn.persist {
            let _ = crate::config::Config::update(|c| c.show_tips = false);
        }
    }
}

/// The tip's line, its `{verb}`s filled with the user's own keys.
pub fn text(app: &App, id: &str) -> String {
    let Some(tip) = learn::tip(id) else {
        return String::new();
    };
    learn::fill(tip.text, |verb| {
        let spec = nebula_config::actions::spec_by_id(verb)?;
        app.chrome
            .keymap
            .shown_first(spec.action)
            .map(|c| c.display().to_string())
    })
}

/// An empty state's line with its verb's key: `No sessions here yet. p:
/// new session`. An unbound verb names no key.
pub fn empty_line(app: &App, id: &str) -> String {
    let Some(e) = learn::empty(id) else {
        return String::new();
    };
    let Some(spec) = e.verb.and_then(nebula_config::actions::spec_by_id) else {
        return e.text.to_string();
    };
    match app.chrome.keymap.shown_first(spec.action) {
        Some(c) => format!("{}  {}: {}", e.text, c.display(), spec.word),
        None => e.text.to_string(),
    }
}

/// The rects the tip's buttons were drawn in, for the hit list: laid out
/// right-aligned on `row`, `got it` then `no tips`.
pub fn button_rects(row: Rect) -> [(Rect, TipButton); 2] {
    let off_w = NO_TIPS.chars().count() as u16;
    let got_w = GOT_IT.chars().count() as u16;
    let off = Rect {
        x: row.right().saturating_sub(off_w + 1),
        width: off_w,
        height: 1,
        ..row
    };
    let got = Rect {
        x: off.x.saturating_sub(got_w + 1),
        width: got_w,
        height: 1,
        ..row
    };
    [(got, TipButton::GotIt), (off, TipButton::Off)]
}

/// The buttons' labels.
pub const GOT_IT: &str = "[got it]";
pub const NO_TIPS: &str = "[no tips]";

/// Open the WHAT'S NEW page: the Help overlay's other page.
pub fn open_whats_new(app: &mut App) {
    app.modals.overlay = Some(Overlay::Help(crate::app::HelpView {
        whats_new: true,
        ..Default::default()
    }));
    app.chrome.dirty = true;
}

/// The WHAT'S NEW page as lines for a box `width` wide: each release's
/// title and items, then the keys that changed. The box wraps them.
pub fn whats_new_lines(width: u16, th: crate::theme::Theme) -> Vec<ratatui::text::Line<'static>> {
    use ratatui::style::{Modifier, Style};
    use ratatui::text::{Line, Span};
    let mut lines = Vec::new();
    for (i, r) in learn::WHATS_NEW.iter().enumerate() {
        if i > 0 {
            lines.push(Line::from(""));
        }
        lines.push(Line::from(vec![
            Span::styled(
                format!(" {}  ", r.version),
                Style::default().fg(th.accent).add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                r.title,
                Style::default().fg(th.text).add_modifier(Modifier::BOLD),
            ),
        ]));
        for item in r.items {
            // Wrapped here, so a long item's next rows hang under its words.
            let rows = wrap_words(item, usize::from(width).saturating_sub(4));
            for (n, row) in rows.into_iter().enumerate() {
                let lead = if n == 0 { " · " } else { "   " };
                lines.push(Line::from(vec![
                    Span::styled(lead, Style::default().fg(th.dim)),
                    Span::styled(row, Style::default().fg(th.text)),
                ]));
            }
        }
    }
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        " Keys changed in 0.49 (yours, if rebound, kept)",
        Style::default().fg(th.muted).add_modifier(Modifier::BOLD),
    )));
    // Three columns: the verb, then the two apps' keys, each cut to fit.
    let w = usize::from(width).saturating_sub(2);
    let verb_w = 18.min(w / 3);
    let key_w = w.saturating_sub(verb_w + 2) / 2;
    let cell = |s: &str, n: usize| format!("{:<n$}", crate::ui::truncate(s, n));
    lines.push(Line::from(Span::styled(
        format!(
            " {} {} {}",
            cell("Verb", verb_w),
            cell("Terminal app", key_w),
            cell("Desktop", key_w)
        ),
        Style::default().fg(th.dim),
    )));
    for k in learn::KEYS_CHANGED {
        lines.push(Line::from(vec![
            Span::styled(
                format!(" {} ", cell(k.verb, verb_w)),
                Style::default().fg(th.text),
            ),
            Span::styled(
                format!("{} ", cell(k.tui, key_w)),
                Style::default().fg(th.accent),
            ),
            Span::styled(cell(k.desktop, key_w), Style::default().fg(th.accent)),
        ]));
    }
    lines
}

/// `text` in rows of at most `width` columns, broken between words (a
/// word longer than a row gets one to itself).
fn wrap_words(text: &str, width: usize) -> Vec<String> {
    let width = width.max(8);
    let mut rows = Vec::new();
    let mut row = String::new();
    for word in text.split_whitespace() {
        let fits = row.chars().count() + 1 + word.chars().count() <= width;
        if !row.is_empty() && !fits {
            rows.push(std::mem::take(&mut row));
        }
        if !row.is_empty() {
            row.push(' ');
        }
        row.push_str(word);
    }
    if !row.is_empty() {
        rows.push(row);
    }
    rows
}

#[cfg(test)]
mod tests {
    use super::wrap_words;

    #[test]
    fn items_wrap_between_words() {
        assert_eq!(
            wrap_words("one two three four", 9),
            vec!["one two", "three", "four"]
        );
        assert_eq!(wrap_words("", 9), Vec::<String>::new());
    }
}
