//! PASTE BURSTS: a paste the host handed over as keystrokes, put back
//! together into the one paste it was.
//!
//! On Windows crossterm reads the console's input records, not the bytes a
//! terminal sends, and has no `Event::Paste` there. Windows Terminal wraps
//! a paste in `CSI 200~` / `CSI 201~` as asked, but the console host
//! between it and nebula takes the markers off and hands the text over as
//! a key press and a key release per character, each line break an Enter
//! (a 5 KB, 200-line paste under ConPTY: 10 580 key events, 200 Enters,
//! no paste). Forwarded as they came, every character was an `Input` of
//! its own and every line break submitted the prompt — claude ran the
//! first line as a turn while the rest was still arriving, one repaint per
//! character, where a terminal shows `[Pasted text #1 +199 lines]`.
//!
//! A pasted character's press and release are written together, a few
//! dozen at a time; a typed key's release comes up a keystroke later. So
//! a text key with more already queued behind it starts a BURST, which
//! holds the keys while they keep coming and is settled once they stop
//! for [`QUIET`]: a burst that holds a line break, or more characters than
//! anyone types in one frame, goes on as one `Event::Paste` — and from
//! there wherever a paste goes (bracketed into a pane that asked for it,
//! into an overlay's field, nowhere near the grid's hotkeys). Anything
//! shorter is replayed key for key, exactly as it arrived. A lone key —
//! everything typed — is never held at all.
//!
//! A burst is read as fast as the host hands it over: the Windows console's
//! text keys in bulk (HOST INPUT, host_input.rs), tens of thousands of
//! events a turn, and no frame painted between turns while it holds but
//! the one in [`PASTE_FRAME`] that says how much has come — a 110 KB
//! paste is two hundred thousand events, and the status bar says
//! `pasting… 64 KB` rather than looking hung.

use crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use std::time::{Duration, Instant};

/// Whether the loop gathers bursts at all: only where the host can't
/// deliver a paste as one. Everywhere else crossterm parses the brackets.
pub(super) const ENABLED: bool = cfg!(windows);

/// How long a burst waits for its next key before it is settled. The
/// console hands a paste over in batches well under a millisecond apart;
/// this is room for a loop busy painting between two of them, so a paste
/// isn't cut in two and its tail typed — where a lone trailing Enter
/// would submit. Only the end of a paste pays it.
pub(super) const QUIET: Duration = Duration::from_millis(20);

/// The most events read in one go before the loop gets a turn again, so
/// a long paste still lets output and the clock through: 32 KB of text,
/// read in a few milliseconds in bulk.
pub(super) const MAX_BATCH: usize = 64 * 1024;

/// The most text keys read in bulk between two looks at the time.
pub(super) const READ_CHUNK: usize = 4096;

/// …or the longest one read may take, where the host hands its events
/// over one at a time (crossterm's stream: tens of microseconds each).
pub(super) const BATCH_TIME: Duration = Duration::from_millis(50);

/// How often a frame is painted while a burst holds: often enough for the
/// status bar's count to move, rarely enough that the paste isn't waiting
/// on frames. Any other time the FRAME PACING decides.
pub(super) const PASTE_FRAME: Duration = Duration::from_millis(100);

/// A burst this big says `pasting…` in the status bar while it is read.
const SHOW_BYTES: usize = 1024;

/// When the loop may paint next: as the pacer says, held back to
/// [`PASTE_FRAME`] after the last frame while a burst is being read.
pub(super) fn draw_at(
    next_draw: tokio::time::Instant,
    last_frame: tokio::time::Instant,
    holding: bool,
) -> tokio::time::Instant {
    if holding {
        next_draw.max(last_frame + PASTE_FRAME)
    } else {
        next_draw
    }
}

/// More characters than this in one burst is a paste even on one line:
/// a slow frame can queue a few typed keys, never this many.
const MIN_PASTE_CHARS: usize = 16;

/// Text keys held since a burst began, waiting to be settled.
#[derive(Debug, Default)]
pub(crate) struct PasteBurst {
    held: Vec<Event>,
    /// When the last key joined: the burst settles [`QUIET`] after it.
    last: Option<Instant>,
    /// The text the held presses type, in bytes: the status bar's count.
    bytes: usize,
}

impl PasteBurst {
    pub(super) fn is_holding(&self) -> bool {
        !self.held.is_empty()
    }

    /// How much a burst big enough to mention has gathered, in bytes.
    pub(super) fn pasting(&self) -> Option<usize> {
        (self.bytes >= SHOW_BYTES).then_some(self.bytes)
    }

    /// When the burst is settled if nothing more joins it.
    pub(super) fn deadline(&self) -> Option<Instant> {
        self.last.map(|last| last + QUIET)
    }

    /// Take `batch` — an event and every one already queued behind it —
    /// and return what is to be handled now, in order. A batch with no
    /// burst in it (one key: anything typed) comes straight back.
    pub(super) fn feed(&mut self, batch: Vec<Event>, now: Instant) -> Vec<Event> {
        let starts = batch.iter().filter(|e| is_text_press(e)).count() >= 2;
        if self.held.is_empty() && !starts {
            return batch;
        }
        let mut out = Vec::new();
        for event in batch {
            if is_text_key(&event) {
                self.bytes += text_len(&event);
                self.held.push(event);
                self.last = Some(now);
            } else {
                // Anything else ends the burst, after it.
                out.extend(self.settle());
                out.push(event);
            }
        }
        out
    }

    /// Settle the burst: one paste, or its keys as they came.
    pub(super) fn settle(&mut self) -> Vec<Event> {
        self.last = None;
        self.bytes = 0;
        let held = std::mem::take(&mut self.held);
        match pasted_text(&held) {
            Some(text) => vec![Event::Paste(text)],
            None => held,
        }
    }
}

/// The text a burst spells, when it reads as a paste.
fn pasted_text(held: &[Event]) -> Option<String> {
    let mut text = String::new();
    let mut chars = 0;
    let mut releases = 0;
    let mut line_break = false;
    for event in held {
        let Event::Key(key) = event else { continue };
        if key.kind == KeyEventKind::Release {
            releases += 1;
            continue;
        }
        chars += 1;
        match key.code {
            KeyCode::Char(c) => text.push(c),
            // The console spells a carriage return Enter and a line feed
            // Ctrl+Enter (a `\r\n` is both), so the text comes back as it
            // was copied; `pasted` and the fields take any of the three.
            KeyCode::Enter => {
                let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
                text.push(if ctrl { '\n' } else { '\r' });
                line_break = true;
            }
            KeyCode::Tab => text.push('\t'),
            _ => return None,
        }
    }
    // A paste comes up as it goes down, key for key. A held key's repeats
    // are presses with one release at the end: `j` held through a slow
    // frame is navigation, never a paste the grid would drop.
    let paired = releases * 2 >= chars;
    (paired && chars >= 2 && (line_break || chars > MIN_PASTE_CHARS)).then_some(text)
}

/// The bytes a held key adds to the text: a press's character, nothing
/// for a release.
fn text_len(event: &Event) -> usize {
    match event {
        Event::Key(key) if key.kind != KeyEventKind::Release => match key.code {
            KeyCode::Char(c) => c.len_utf8(),
            _ => 1,
        },
        _ => 0,
    }
}

/// A key press that types text: what a paste is made of.
pub(super) fn is_text_press(event: &Event) -> bool {
    matches!(event, Event::Key(key) if key.kind != KeyEventKind::Release && types_text(key))
}

/// A press or release of a key that types text.
fn is_text_key(event: &Event) -> bool {
    matches!(event, Event::Key(key) if types_text(key))
}

/// Does `key` type its text? A character, shifted or not — or with
/// Ctrl+Alt, which is AltGr on Windows (`@` on a German layout) — a plain
/// Tab, and Enter plain or with Ctrl (the console's line feed). Ctrl or
/// Alt alone on a character is a chord, never text; a lone Ctrl+Enter
/// typed is never in a burst, and passes through as the chord it is.
pub(super) fn types_text(key: &KeyEvent) -> bool {
    let mods = key.modifiers - KeyModifiers::SHIFT;
    match key.code {
        KeyCode::Char(_) => mods.is_empty() || mods == KeyModifiers::CONTROL | KeyModifiers::ALT,
        KeyCode::Enter => mods.is_empty() || mods == KeyModifiers::CONTROL,
        KeyCode::Tab => mods.is_empty(),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyEventKind, MouseEvent, MouseEventKind};

    fn key(code: KeyCode, kind: KeyEventKind) -> Event {
        Event::Key(KeyEvent::new_with_kind(code, KeyModifiers::NONE, kind))
    }

    /// `text` as the Windows console hands a paste over (seen under
    /// ConPTY): a press and a release per character, `\r` an Enter and
    /// `\n` a Ctrl+Enter.
    fn typed(text: &str) -> Vec<Event> {
        text.chars()
            .flat_map(|c| {
                let (code, mods) = match c {
                    '\r' => (KeyCode::Enter, KeyModifiers::NONE),
                    '\n' => (KeyCode::Enter, KeyModifiers::CONTROL),
                    '\t' => (KeyCode::Tab, KeyModifiers::NONE),
                    c => (KeyCode::Char(c), KeyModifiers::NONE),
                };
                [KeyEventKind::Press, KeyEventKind::Release]
                    .map(|kind| Event::Key(KeyEvent::new_with_kind(code, mods, kind)))
            })
            .collect()
    }

    /// Every line ending a clipboard holds comes back as it was copied.
    #[test]
    fn line_endings_come_back_as_copied() {
        for text in ["a\rb\r", "a\nb\n", "a\r\nb\r\n"] {
            let mut burst = PasteBurst::default();
            burst.feed(typed(text), Instant::now());
            assert_eq!(burst.settle(), vec![Event::Paste(text.into())], "{text:?}");
        }
    }

    fn click() -> Event {
        Event::Mouse(MouseEvent {
            kind: MouseEventKind::Down(crossterm::event::MouseButton::Left),
            column: 1,
            row: 1,
            modifiers: KeyModifiers::NONE,
        })
    }

    #[test]
    fn a_multi_line_burst_is_one_paste() {
        let mut burst = PasteBurst::default();
        let now = Instant::now();
        assert!(burst.feed(typed("ab\ncd"), now).is_empty(), "held");
        assert_eq!(burst.deadline(), Some(now + QUIET));
        assert_eq!(burst.settle(), vec![Event::Paste("ab\ncd".into())]);
        assert!(!burst.is_holding());
        assert_eq!(burst.deadline(), None);
    }

    /// The console hands a long paste over a batch at a time: the burst
    /// holds across them, and stays one paste.
    #[test]
    fn a_burst_spans_batches() {
        let mut burst = PasteBurst::default();
        let t0 = Instant::now();
        let text: String = (0..50).map(|i| format!("line {i}\n")).collect();
        let events = typed(&text);
        let mut later = t0;
        for chunk in events.chunks(52) {
            assert!(burst.feed(chunk.to_vec(), later).is_empty());
            later += Duration::from_micros(500);
        }
        assert_eq!(burst.settle(), vec![Event::Paste(text)]);
    }

    /// Everything typed: one key, nothing behind it. Never held — the
    /// keystroke reaches the pane at once.
    #[test]
    fn a_lone_key_passes_straight_through() {
        let mut burst = PasteBurst::default();
        let press = typed("a")[..1].to_vec();
        assert_eq!(burst.feed(press.clone(), Instant::now()), press);
        assert!(!burst.is_holding());
        let both = typed("\n");
        assert_eq!(burst.feed(both.clone(), Instant::now()), both, "Enter too");
    }

    /// A few keys a slow frame queued, no line break among them: replayed
    /// as typed, so claude's `/` and `!` still open what they open.
    #[test]
    fn a_short_one_line_burst_replays_as_keys() {
        let mut burst = PasteBurst::default();
        let events = typed("/help");
        assert!(burst.feed(events.clone(), Instant::now()).is_empty());
        assert_eq!(burst.settle(), events);
    }

    #[test]
    fn a_long_one_line_burst_is_a_paste() {
        let mut burst = PasteBurst::default();
        let line = "https://example.com/a/rather/long/link";
        burst.feed(typed(line), Instant::now());
        assert_eq!(burst.settle(), vec![Event::Paste(line.into())]);
    }

    /// An arrow or a chord isn't text: the keys before it settle first —
    /// here too few to be a paste — and it follows them, in order.
    #[test]
    fn a_key_that_types_nothing_ends_the_burst() {
        let mut burst = PasteBurst::default();
        let mut batch = typed("ok");
        batch.push(key(KeyCode::Up, KeyEventKind::Press));
        let ctrl_c = Event::Key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL));
        batch.push(ctrl_c.clone());
        let mut want = typed("ok");
        want.push(key(KeyCode::Up, KeyEventKind::Press));
        want.push(ctrl_c);
        assert_eq!(burst.feed(batch, Instant::now()), want);
        assert!(!burst.is_holding());
    }

    #[test]
    fn a_click_behind_a_paste_lands_after_it() {
        let mut burst = PasteBurst::default();
        let mut batch = typed("one\ntwo");
        batch.push(click());
        assert_eq!(
            burst.feed(batch, Instant::now()),
            vec![Event::Paste("one\ntwo".into()), click()]
        );
    }

    /// Uppercase arrives shifted and AltGr's characters as Ctrl+Alt: both
    /// are text. Ctrl alone is a chord.
    #[test]
    fn shift_and_altgr_type_text() {
        let shifted = KeyEvent::new(KeyCode::Char('A'), KeyModifiers::SHIFT);
        let altgr = KeyEvent::new(
            KeyCode::Char('@'),
            KeyModifiers::CONTROL | KeyModifiers::ALT,
        );
        let ctrl = KeyEvent::new(KeyCode::Char('v'), KeyModifiers::CONTROL);
        assert!(types_text(&shifted));
        assert!(types_text(&altgr));
        assert!(!types_text(&ctrl));
        let mut burst = PasteBurst::default();
        let mut batch = vec![Event::Key(shifted), Event::Key(altgr)];
        batch.extend(typed("\nx"));
        burst.feed(batch, Instant::now());
        assert_eq!(burst.settle(), vec![Event::Paste("A@\nx".into())]);
    }

    /// A held `j`'s repeats queued up behind a slow frame — presses, no
    /// releases — are replayed as the navigation they are.
    #[test]
    fn a_held_keys_repeats_are_not_a_paste() {
        let mut burst = PasteBurst::default();
        let repeats: Vec<Event> = (0..40)
            .map(|_| key(KeyCode::Char('j'), KeyEventKind::Press))
            .collect();
        assert!(burst.feed(repeats.clone(), Instant::now()).is_empty());
        assert_eq!(burst.settle(), repeats);
    }

    /// A lone release (a typed key coming up) is never a burst.
    #[test]
    fn a_release_alone_passes_through() {
        let mut burst = PasteBurst::default();
        let up = vec![key(KeyCode::Char('a'), KeyEventKind::Release)];
        assert_eq!(burst.feed(up.clone(), Instant::now()), up);
    }
}
