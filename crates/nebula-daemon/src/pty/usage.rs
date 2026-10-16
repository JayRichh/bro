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
//! The same lines carry Claude's USAGE-LIMIT WAIT (2.1.234+,
//! `interactive-mode#wait-for-a-usage-limit-to-reset`): the two lines at
//! the bottom of a session waiting out a limit, and what the conversation
//! records when the wait starts or ends ([`LimitLine`]):
//!
//! ```text
//! Usage limit reached · limit resets 3:45pm
//! Continuing automatically at 3:45pm · esc to cancel
//! Usage limit reached · continuing automatically at 3:45pm · esc to cancel
//! Automatic continue cancelled
//! Automatic continue stopped after repeated usage-limit hits · /rate-limit-options to try again
//! ```
//!
//! No hook says when the wait ends, and only the screen says when Claude
//! will carry on: the time is already the user's local clock
//! (`Daemon::on_pty_limit`).
//!
//! The stream is a TUI's: styled runs, cursor moves, line erases. Escape
//! sequences are dropped, and a cursor move or erase — like a line break —
//! ends a line, so the text Claude draws on one screen row reaches the
//! parsers as one line and nothing else. Each reading is reported once,
//! until it changes.

use nebula_core::profiles::Usage;

/// A line longer than this is dropped rather than kept growing: the
/// notice is short, and a TUI repaint without a line break must not make
/// the buffer the whole screen.
const MAX_LINE: usize = 1024;

/// The longest reset time a limit line is believed to print (`3:45pm`,
/// `Oct 20 at 9am`).
const MAX_WHEN: usize = 24;

/// Claude's usage-limit wait, as one line of its screen tells it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LimitLine {
    /// Claude is waiting and will carry the task on by itself at `at`:
    /// the footer's `Continuing automatically at 3:45pm · esc to cancel`,
    /// or the conversation's lower-case record of the same.
    Continuing { at: String },
    /// `Usage limit reached · limit resets 3:45pm`: when the limit resets,
    /// whether or not Claude waits for it.
    Resets { at: String },
    /// The wait is over without carrying on: cancelled, or given up after
    /// repeated limits.
    Ended,
}

impl LimitLine {
    /// The wait one line of a session's screen reports, if any.
    pub fn parse(line: &str) -> Option<LimitLine> {
        // ASCII-only lowering keeps every byte offset valid in `line`.
        let lower = line.to_ascii_lowercase();
        let after = |phrase: &str| {
            lower
                .find(phrase)
                .and_then(|i| when(&line[i + phrase.len()..]))
        };
        if let Some(at) = after("continuing automatically at ") {
            return Some(LimitLine::Continuing { at });
        }
        if let Some(at) = after("limit resets ") {
            return Some(LimitLine::Resets { at });
        }
        if lower.contains("automatic continue cancelled")
            || lower.contains("automatic continue stopped")
        {
            return Some(LimitLine::Ended);
        }
        None
    }
}

/// The reset time at the head of `rest`, up to the next ` · ` or the end.
fn when(rest: &str) -> Option<String> {
    let at = rest
        .split(['\u{b7}', '(', '|', ',', '\r', '\n'])
        .next()
        .unwrap_or("")
        .trim()
        .trim_end_matches('.')
        .trim();
    (!at.is_empty() && at.len() <= MAX_WHEN).then(|| at.to_string())
}

/// What one chunk of output completed.
#[derive(Debug, Default, PartialEq)]
pub struct Scan {
    /// The newest plan-usage notice, when it differs from the last one.
    pub usage: Option<Usage>,
    /// Usage-limit wait lines that are news, in screen order.
    pub limits: Vec<LimitLine>,
}

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
    /// The wait's last reported times, one per kind: the two footer lines
    /// repaint together, so a single "last" would report both every time.
    continuing: Option<String>,
    resets: Option<String>,
    /// An end was reported, and no wait has started since.
    ended: bool,
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
            continuing: None,
            resets: None,
            ended: false,
        }
    }

    /// Feed one chunk of output: the readings it completed that are news.
    pub fn feed(&mut self, bytes: &[u8]) -> Scan {
        let mut scan = Scan::default();
        for &b in bytes {
            match self.esc {
                Esc::None => match b {
                    0x1b => self.esc = Esc::Start,
                    b'\r' | b'\n' => self.end_line(&mut scan),
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
                            self.end_line(&mut scan);
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
        scan
    }

    fn end_line(&mut self, scan: &mut Scan) {
        if self.line.is_empty() {
            return;
        }
        let text = String::from_utf8_lossy(&self.line).into_owned();
        self.line.clear();
        if let Some(usage) = Usage::parse(&text) {
            let same = self.last.as_ref().is_some_and(|last| {
                last.percent == usage.percent
                    && last.window == usage.window
                    && last.resets == usage.resets
            });
            if !same {
                self.last = Some(usage.clone());
                scan.usage = Some(usage);
            }
            return;
        }
        let Some(limit) = LimitLine::parse(&text) else {
            return;
        };
        let news = match &limit {
            LimitLine::Continuing { at } => {
                self.ended = false;
                self.continuing.replace(at.clone()).as_ref() != Some(at)
            }
            LimitLine::Resets { at } => self.resets.replace(at.clone()).as_ref() != Some(at),
            // A later wait at the same time is news again.
            LimitLine::Ended => {
                self.continuing = None;
                !std::mem::replace(&mut self.ended, true)
            }
        };
        if news {
            scan.limits.push(limit);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_styled_notice_reads_once_until_it_changes() {
        let mut scanner = UsageScanner::new();
        let painted = "\x1b[2K\x1b[38;5;214mYou've used \x1b[1m91%\x1b[22m of your weekly limit \u{b7} resets 2pm\x1b[39m\x1b[1B";
        let usage = scanner.feed(painted.as_bytes()).usage.expect("the notice");
        assert_eq!(usage.summary(), "91% weekly \u{b7} resets 2pm");
        assert_eq!(
            scanner.feed(painted.as_bytes()).usage,
            None,
            "the same reading again"
        );
        let usage = scanner
            .feed(b"You've used 92% of your weekly limit\r\n")
            .usage
            .expect("a new reading");
        assert_eq!(usage.percent, 92);
    }

    #[test]
    fn a_notice_split_across_chunks_and_titles_between_read_whole() {
        let mut scanner = UsageScanner::new();
        assert_eq!(
            scanner.feed(b"\x1b]0;\xe2\x9c\xb3 Fix\x07You've used 4"),
            Scan::default()
        );
        let usage = scanner
            .feed(b"0% of your 5-hour limit\r\n")
            .usage
            .expect("the notice");
        assert_eq!((usage.percent, usage.window.as_str()), (40, "5-hour"));
        // Text on the next row after a cursor move is a line of its own.
        assert_eq!(
            scanner.feed(b"used\x1b[3;1H 50% of your weekly limit\n"),
            Scan::default()
        );
    }

    /// The documented lines, word for word: the footer pair, the
    /// conversation's record of the wait starting, a cancel and a give-up.
    #[test]
    fn the_documented_limit_lines_parse() {
        let continuing = |at: &str| Some(LimitLine::Continuing { at: at.to_string() });
        assert_eq!(
            LimitLine::parse("Usage limit reached \u{b7} limit resets 3:45pm"),
            Some(LimitLine::Resets {
                at: "3:45pm".into()
            })
        );
        assert_eq!(
            LimitLine::parse("Continuing automatically at 3:45pm \u{b7} esc to cancel"),
            continuing("3:45pm")
        );
        assert_eq!(
            LimitLine::parse(
                "Continuing automatically at 3:45pm \u{b7} /usage-credits to continue now"
            ),
            continuing("3:45pm")
        );
        assert_eq!(
            LimitLine::parse(
                "  Usage limit reached \u{b7} continuing automatically at 3:45pm \u{b7} esc to cancel"
            ),
            continuing("3:45pm"),
            "the conversation's lower-case record"
        );
        assert_eq!(
            LimitLine::parse(
                "Usage limit reached \u{b7} limit resets Oct 20 at 9am (Pacific/Auckland)"
            ),
            Some(LimitLine::Resets {
                at: "Oct 20 at 9am".into()
            })
        );
        assert_eq!(
            LimitLine::parse("Automatic continue cancelled. Send a prompt to continue."),
            Some(LimitLine::Ended)
        );
        assert_eq!(
            LimitLine::parse(
                "Automatic continue stopped after repeated usage-limit hits \u{b7} /rate-limit-options to try again"
            ),
            Some(LimitLine::Ended)
        );
        for other in [
            "Continuing shortly \u{b7} esc to cancel",
            "Usage limit reset \u{b7} continuing automatically",
            "You've used 91% of your weekly limit \u{b7} resets 2pm",
            "Continuing automatically at  \u{b7} esc to cancel",
            "limit resets whenever the moon is full and the tide is high again",
        ] {
            assert_eq!(LimitLine::parse(other), None, "{other}");
        }
    }

    /// The footer's two lines repaint together, again and again: each is
    /// news once, until its time changes; an end is news once, and a wait
    /// after it is news again even at the same time.
    #[test]
    fn a_repainted_wait_reads_once_until_it_changes() {
        let mut scanner = UsageScanner::new();
        let footer = "\x1b[2K\x1b[38;5;214mUsage limit reached \u{b7} limit resets 3:45pm\x1b[39m\x1b[1B\x1b[2KContinuing automatically at \x1b[1m3:45pm\x1b[22m \u{b7} esc to cancel\x1b[1B";
        assert_eq!(
            scanner.feed(footer.as_bytes()).limits,
            vec![
                LimitLine::Resets {
                    at: "3:45pm".into()
                },
                LimitLine::Continuing {
                    at: "3:45pm".into()
                },
            ]
        );
        assert!(scanner.feed(footer.as_bytes()).limits.is_empty());
        let cancelled = b"Automatic continue cancelled\r\n";
        assert_eq!(scanner.feed(cancelled).limits, vec![LimitLine::Ended]);
        assert!(scanner.feed(cancelled).limits.is_empty());
        assert_eq!(
            scanner.feed(footer.as_bytes()).limits,
            vec![LimitLine::Continuing {
                at: "3:45pm".into()
            }],
            "a new wait"
        );
    }
}
