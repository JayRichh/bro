//! PLAN USAGE scanner — Claude's own word on how much of an account's
//! limit is gone, read straight off the PTY stream:
//!
//! ```text
//! You've used 91% of your weekly limit · resets 2pm
//! ```
//!
//! Claude prints no hook for it, and the line is the only place a
//! subscription's headroom shows. Read here, it is filed under the
//! session's PROFILE (`Daemon::on_pty_usage`), so the QUICK PROMPT's
//! profile field can name the fuller account before you launch on it.
//!
//! The stream is a TUI's: styled runs, cursor moves, line erases. Escape
//! sequences are dropped, and a cursor move or erase — like a line break —
//! ends a line, so the text Claude draws on one screen row reaches
//! [`nebula_core::profiles::Usage::parse`] as one line and nothing else.
//! Each reading is reported once, until it changes.

use nebula_core::profiles::Usage;

/// A line longer than this is dropped rather than kept growing: the
/// notice is short, and a TUI repaint without a line break must not make
/// the buffer the whole screen.
const MAX_LINE: usize = 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Esc {
    /// Plain text.
    None,
    /// Just past ESC.
    Start,
    /// Inside `ESC [ … final`.
    Csi,
    /// Inside `ESC ] … BEL` (or `ESC \`); `true` just past an ESC in it.
    Osc(bool),
}

#[derive(Debug)]
pub struct UsageScanner {
    line: Vec<u8>,
    esc: Esc,
    last: Option<Usage>,
}

impl Default for UsageScanner {
    fn default() -> Self {
        Self::new()
    }
}

impl UsageScanner {
    pub fn new() -> Self {
        Self {
            line: Vec::new(),
            esc: Esc::None,
            last: None,
        }
    }

    /// Feed one chunk of output; the newest reading it completed, when it
    /// differs from the last one reported.
    pub fn feed(&mut self, bytes: &[u8]) -> Option<Usage> {
        let mut found = None;
        for &b in bytes {
            match self.esc {
                Esc::None => match b {
                    0x1b => self.esc = Esc::Start,
                    b'\r' | b'\n' => found = self.end_line().or(found),
                    _ if b < 0x20 => {}
                    _ => {
                        if self.line.len() < MAX_LINE {
                            self.line.push(b);
                        } else {
                            self.line.clear();
                        }
                    }
                },
                Esc::Start => {
                    self.esc = match b {
                        b'[' => Esc::Csi,
                        b']' => Esc::Osc(false),
                        _ => Esc::None,
                    }
                }
                Esc::Csi => {
                    if (0x40..=0x7e).contains(&b) {
                        self.esc = Esc::None;
                        // Cursor moves and erases start a new row of text.
                        if matches!(b, b'A'..=b'H' | b'J' | b'K' | b'f' | b'd') {
                            found = self.end_line().or(found);
                        }
                    }
                }
                Esc::Osc(after_esc) => {
                    self.esc = match (b, after_esc) {
                        (0x07, _) | (b'\\', true) => Esc::None,
                        (0x1b, _) => Esc::Osc(true),
                        _ => Esc::Osc(false),
                    }
                }
            }
        }
        found
    }

    fn end_line(&mut self) -> Option<Usage> {
        if self.line.is_empty() {
            return None;
        }
        let text = String::from_utf8_lossy(&self.line).into_owned();
        self.line.clear();
        let usage = Usage::parse(&text)?;
        if self.last.as_ref().is_some_and(|last| {
            last.percent == usage.percent
                && last.window == usage.window
                && last.resets == usage.resets
        }) {
            return None;
        }
        self.last = Some(usage.clone());
        Some(usage)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_styled_notice_reads_once_until_it_changes() {
        let mut scanner = UsageScanner::new();
        let painted = "\x1b[2K\x1b[38;5;214mYou've used \x1b[1m91%\x1b[22m of your weekly limit \u{b7} resets 2pm\x1b[39m\x1b[1B";
        let usage = scanner.feed(painted.as_bytes()).expect("the notice");
        assert_eq!(usage.summary(), "91% weekly \u{b7} resets 2pm");
        assert_eq!(
            scanner.feed(painted.as_bytes()),
            None,
            "the same reading again"
        );
        let usage = scanner
            .feed(b"You've used 92% of your weekly limit\r\n")
            .expect("a new reading");
        assert_eq!(usage.percent, 92);
    }

    #[test]
    fn a_notice_split_across_chunks_and_titles_between_read_whole() {
        let mut scanner = UsageScanner::new();
        assert_eq!(
            scanner.feed(b"\x1b]0;\xe2\x9c\xb3 Fix\x07You've used 4"),
            None
        );
        let usage = scanner
            .feed(b"0% of your 5-hour limit\r\n")
            .expect("the notice");
        assert_eq!((usage.percent, usage.window.as_str()), (40, "5-hour"));
        // Text on the next row after a cursor move is a line of its own.
        assert_eq!(
            scanner.feed(b"used\x1b[3;1H 50% of your weekly limit\n"),
            None
        );
    }
}
