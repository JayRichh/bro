//! HOST INPUT: where the loop reads the host's events from.
//!
//! Everywhere but Windows this is crossterm's `EventStream`, which reads
//! the bytes a terminal sends and parses a paste's brackets itself.
//!
//! On Windows crossterm reads the console's input records one at a time,
//! and each one costs it five calls into the console host: it opens
//! `CONIN$` afresh, waits on it, counts what is queued, reads a single
//! record and closes the handle again. A paste arrives as two records per
//! character (PASTE BURSTS, paste_burst.rs), so 110 KB was 220 000 of
//! those round trips — measured against a real console host (the
//! `console_read_bench` test in `host_input/console.rs`), the read alone
//! took longer than everything else in the paste's path put together.
//! There the loop reads through [`console::ConsoleEvents`] instead: the
//! text keys queued at the head of the console's buffer are read in bulk,
//! thousands to a call, and everything else still goes through crossterm.

use crossterm::event::{Event, EventStream};
use futures::Stream;

#[cfg(windows)]
pub(super) mod console;

/// A stream of the host's events that can also hand over the text keys
/// already queued, in bulk, without waiting.
pub(super) trait HostInput: Stream<Item = std::io::Result<Event>> + Unpin {
    /// Append up to `max` text-key events that are already queued, in the
    /// order they came, and return how many. Stops at the first event that
    /// isn't one (a chord, an arrow, a click), which the stream yields
    /// next. Never waits.
    fn read_text(&mut self, _out: &mut Vec<Event>, _max: usize) -> std::io::Result<usize> {
        Ok(0)
    }
}

impl HostInput for EventStream {}

/// The loop's input: the console read in bulk on Windows (crossterm's
/// stream where there's no console to open), crossterm's stream elsewhere.
#[cfg(windows)]
pub(super) fn open() -> console::Input {
    console::Input::open()
}

#[cfg(not(windows))]
pub(super) fn open() -> EventStream {
    EventStream::new()
}
