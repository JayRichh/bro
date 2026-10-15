//! CONSOLE INPUT (Windows): the console's input records, the text keys
//! among them read in bulk.
//!
//! [`ConsoleEvents`] owns every read of the console from the moment it is
//! opened. The text keys at the head of the input buffer — what a paste is
//! made of — are peeked thousands at a time, translated here exactly as
//! crossterm would translate them, and taken off with one
//! `ReadConsoleInputW`; the first record that isn't one is left where it
//! is and read through crossterm, so a chord, an arrow, a click or a
//! resize means what crossterm says it means, in the order it came.
//!
//! crossterm's own `EventStream` can't share the console: its waker
//! thread reads a record into crossterm's private queue whenever one
//! arrives, so a bulk read beside it could take the records behind one it
//! had already queued. Here the thread only waits — on the console handle
//! and a stop event — and wakes the loop; the reading is all the loop's.

use super::super::paste_burst::types_text;
use super::HostInput;
use crossterm::event::{Event, EventStream, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use futures::Stream;
use std::collections::VecDeque;
use std::io;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::task::{Context, Poll, Waker};
use std::time::Duration;
use windows_sys::Win32::Foundation::{
    CloseHandle, GENERIC_READ, GENERIC_WRITE, HANDLE, INVALID_HANDLE_VALUE, WAIT_FAILED,
};
use windows_sys::Win32::Storage::FileSystem::{
    CreateFileW, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
};
use windows_sys::Win32::System::Console::{
    GetNumberOfConsoleInputEvents, PeekConsoleInputW, ReadConsoleInputW, INPUT_RECORD, KEY_EVENT,
    KEY_EVENT_RECORD, LEFT_ALT_PRESSED, LEFT_CTRL_PRESSED, RIGHT_ALT_PRESSED, RIGHT_CTRL_PRESSED,
    SHIFT_PRESSED,
};
use windows_sys::Win32::System::Threading::{CreateEventW, SetEvent, WaitForMultipleObjects};

/// Records looked at per peek: a 2 KB stretch of a paste.
const PEEK: usize = 4096;

/// The loop's input on Windows: the console, or crossterm's own stream
/// when there's no console to open (input redirected, a detached test).
pub(in crate::event_loop) enum Input {
    Console(ConsoleEvents),
    Stream(EventStream),
}

impl Input {
    pub(in crate::event_loop) fn open() -> Self {
        match ConsoleEvents::open() {
            Ok(console) => Self::Console(console),
            Err(e) => {
                tracing::warn!(error = %e, "console input unavailable, reading through crossterm");
                Self::Stream(EventStream::new())
            }
        }
    }
}

impl Stream for Input {
    type Item = io::Result<Event>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        match self.get_mut() {
            Self::Console(console) => Pin::new(console).poll_next(cx),
            Self::Stream(stream) => Pin::new(stream).poll_next(cx),
        }
    }
}

impl HostInput for Input {
    fn read_text(&mut self, out: &mut Vec<Event>, max: usize) -> io::Result<usize> {
        match self {
            Self::Console(console) => console.read_text(out, max),
            Self::Stream(_) => Ok(0),
        }
    }
}

/// A handle closed when the last owner lets go.
struct OwnedHandle(HANDLE);

// SAFETY: a console handle and an event handle may be used from any
// thread; nothing here hands out the raw pointer's pointee.
unsafe impl Send for OwnedHandle {}
unsafe impl Sync for OwnedHandle {}

impl Drop for OwnedHandle {
    fn drop(&mut self) {
        // SAFETY: the handle was opened by this module and is closed once.
        unsafe { CloseHandle(self.0) };
    }
}

/// What the waiting thread and the stream share.
struct Wake {
    /// The task to wake once the console has something.
    waker: Mutex<Option<Waker>>,
    /// The stream found the console empty: wait on it again. The handle is
    /// signalled for as long as anything is queued, so the thread waits
    /// only when asked, or it would spin on a record nobody has read yet.
    armed: Mutex<bool>,
    rearm: Condvar,
    stop: AtomicBool,
    stop_event: OwnedHandle,
}

pub(in crate::event_loop) struct ConsoleEvents {
    conin: Arc<OwnedHandle>,
    wake: Arc<Wake>,
    /// The peek buffer, kept between reads.
    peeked: Vec<INPUT_RECORD>,
    /// Events translated for `poll_next` beyond the one it returned (the
    /// two halves of an emoji's press and release).
    pending: VecDeque<Event>,
}

impl ConsoleEvents {
    pub(in crate::event_loop) fn open() -> io::Result<Self> {
        let name: Vec<u16> = "CONIN$\0".encode_utf16().collect();
        // SAFETY: a NUL-terminated name; no security attributes, no template.
        let conin = unsafe {
            CreateFileW(
                name.as_ptr(),
                GENERIC_READ | GENERIC_WRITE,
                FILE_SHARE_READ | FILE_SHARE_WRITE,
                std::ptr::null(),
                OPEN_EXISTING,
                0,
                std::ptr::null_mut(),
            )
        };
        if conin == INVALID_HANDLE_VALUE || conin.is_null() {
            return Err(io::Error::last_os_error());
        }
        let conin = Arc::new(OwnedHandle(conin));
        // A manual-reset event, unset: set once, it stays set for the
        // thread's last wait.
        // SAFETY: default security, unnamed.
        let stop_event = unsafe { CreateEventW(std::ptr::null(), 1, 0, std::ptr::null()) };
        if stop_event.is_null() {
            return Err(io::Error::last_os_error());
        }
        let wake = Arc::new(Wake {
            waker: Mutex::new(None),
            armed: Mutex::new(false),
            rearm: Condvar::new(),
            stop: AtomicBool::new(false),
            stop_event: OwnedHandle(stop_event),
        });
        let (thread_conin, thread_wake) = (conin.clone(), wake.clone());
        std::thread::Builder::new()
            .name("console-input".into())
            .stack_size(64 * 1024)
            .spawn(move || wait_for_input(&thread_conin, &thread_wake))?;
        Ok(Self {
            conin,
            wake,
            peeked: Vec::with_capacity(PEEK),
            pending: VecDeque::new(),
        })
    }

    fn queued(&self) -> io::Result<u32> {
        let mut count = 0;
        // SAFETY: a live console handle and a valid out pointer.
        if unsafe { GetNumberOfConsoleInputEvents(self.conin.0, &mut count) } == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(count)
    }

    /// Up to `max` text-key events off the head of the buffer, with one
    /// peek and one read; how many records it took.
    fn take_text(&mut self, out: &mut Vec<Event>, max: usize) -> io::Result<usize> {
        let queued = self.queued()? as usize;
        if queued == 0 || max == 0 {
            return Ok(0);
        }
        let want = queued.min(PEEK);
        self.peeked.clear();
        self.peeked.resize(want, INPUT_RECORD::default());
        let mut got = 0;
        // SAFETY: the buffer holds `want` records.
        let ok = unsafe {
            PeekConsoleInputW(
                self.conin.0,
                self.peeked.as_mut_ptr(),
                want as u32,
                &mut got,
            )
        };
        if ok == 0 {
            return Err(io::Error::last_os_error());
        }
        let used = translate_run(&self.peeked[..got as usize], max, out);
        // Take exactly the records translated, which are still the head
        // of the buffer: nothing else reads it.
        let mut taken = 0;
        while taken < used {
            let mut read = 0;
            // SAFETY: at least `used - taken` records are queued (the peek
            // saw them), so this never blocks; the buffer has room.
            let ok = unsafe {
                ReadConsoleInputW(
                    self.conin.0,
                    self.peeked.as_mut_ptr(),
                    (used - taken) as u32,
                    &mut read,
                )
            };
            if ok == 0 {
                return Err(io::Error::last_os_error());
            }
            taken += read as usize;
        }
        Ok(used)
    }

    /// See [`HostInput::read_text`].
    fn read_text(&mut self, out: &mut Vec<Event>, max: usize) -> io::Result<usize> {
        let start = out.len();
        while out.len() - start < max {
            let Some(event) = self.pending.pop_front() else {
                break;
            };
            out.push(event);
        }
        loop {
            let room = max.saturating_sub(out.len() - start);
            if room == 0 || self.take_text(out, room)? == 0 {
                break;
            }
        }
        Ok(out.len() - start)
    }

    /// The next event already queued, or None when the console is empty.
    fn next_event(&mut self) -> io::Result<Option<Event>> {
        if let Some(event) = self.pending.pop_front() {
            return Ok(Some(event));
        }
        let mut text = Vec::new();
        self.take_text(&mut text, 1)?;
        if !text.is_empty() {
            let mut text = text.into_iter();
            let first = text.next();
            self.pending.extend(text);
            return Ok(first);
        }
        // Not text: crossterm's to read. Its poll reads one record and says
        // whether it made an event; one that makes none (a lone Shift, a
        // mouse report that changes nothing) is gone, and the next is
        // tried. Bounded, so a console that keeps a record it won't hand
        // over can't hold the loop.
        let mut tries = self.queued()?;
        while tries > 0 {
            if crossterm::event::poll(Duration::ZERO)? {
                return crossterm::event::read().map(Some);
            }
            tries -= 1;
            if self.queued()? == 0 {
                break;
            }
        }
        Ok(None)
    }
}

impl Stream for ConsoleEvents {
    type Item = io::Result<Event>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let this = self.get_mut();
        // The waker goes in before the console is looked at: input landing
        // between the look and the arm still wakes this task.
        *this.wake.waker.lock().unwrap_or_else(|e| e.into_inner()) = Some(cx.waker().clone());
        match this.next_event() {
            Ok(Some(event)) => Poll::Ready(Some(Ok(event))),
            Ok(None) => {
                *this.wake.armed.lock().unwrap_or_else(|e| e.into_inner()) = true;
                this.wake.rearm.notify_one();
                Poll::Pending
            }
            Err(e) => Poll::Ready(Some(Err(e))),
        }
    }
}

impl Drop for ConsoleEvents {
    fn drop(&mut self) {
        self.wake.stop.store(true, Ordering::SeqCst);
        // SAFETY: the event outlives this call (the thread holds it too).
        unsafe { SetEvent(self.wake.stop_event.0) };
        self.wake.rearm.notify_one();
    }
}

/// The waiting thread: once armed, wait for the console to have input (or
/// for the stop), wake the loop, and wait to be armed again.
fn wait_for_input(conin: &OwnedHandle, wake: &Wake) {
    let handles = [conin.0, wake.stop_event.0];
    loop {
        {
            let mut armed = wake.armed.lock().unwrap_or_else(|e| e.into_inner());
            while !*armed && !wake.stop.load(Ordering::SeqCst) {
                armed = wake.rearm.wait(armed).unwrap_or_else(|e| e.into_inner());
            }
            *armed = false;
        }
        if wake.stop.load(Ordering::SeqCst) {
            return;
        }
        // SAFETY: both handles live as long as this thread's Arcs.
        let woke = unsafe { WaitForMultipleObjects(2, handles.as_ptr(), 0, u32::MAX) };
        if wake.stop.load(Ordering::SeqCst) {
            return;
        }
        if woke == WAIT_FAILED {
            // Don't spin on a handle that has gone bad; the loop still
            // reads (and sees the error) on its next turn.
            std::thread::sleep(Duration::from_millis(50));
        }
        if let Some(waker) = wake.waker.lock().unwrap_or_else(|e| e.into_inner()).take() {
            waker.wake();
        }
    }
}

const VK_BACK: u16 = 0x08;
const VK_TAB: u16 = 0x09;
const VK_RETURN: u16 = 0x0D;
const VK_SHIFT: u16 = 0x10;
const VK_CONTROL: u16 = 0x11;
const VK_MENU: u16 = 0x12;
const VK_ESCAPE: u16 = 0x1B;

/// Keys crossterm names rather than reading their character: page and
/// cursor keys, Insert and Delete, the function keys, Backspace, Escape —
/// and the numpad digits, which Alt turns into an Alt code.
fn named_key(vk: u16) -> bool {
    matches!(vk, VK_BACK | VK_ESCAPE | 0x21..=0x28 | 0x2D | 0x2E | 0x60..=0x69 | 0x70..=0x87)
}

/// The modifiers crossterm reads off a record's control-key state.
fn modifiers(state: u32) -> KeyModifiers {
    let mut mods = KeyModifiers::empty();
    if state & SHIFT_PRESSED != 0 {
        mods |= KeyModifiers::SHIFT;
    }
    if state & (LEFT_CTRL_PRESSED | RIGHT_CTRL_PRESSED) != 0 {
        mods |= KeyModifiers::CONTROL;
    }
    if state & (LEFT_ALT_PRESSED | RIGHT_ALT_PRESSED) != 0 {
        mods |= KeyModifiers::ALT;
    }
    mods
}

fn key_record(record: &INPUT_RECORD) -> Option<KEY_EVENT_RECORD> {
    // SAFETY: the union holds a key record when the type says so.
    (u32::from(record.EventType) == KEY_EVENT).then(|| unsafe { record.Event.KeyEvent })
}

fn unit(key: &KEY_EVENT_RECORD) -> u16 {
    // SAFETY: every bit pattern is a valid u16.
    unsafe { key.uChar.UnicodeChar }
}

fn kind(key: &KEY_EVENT_RECORD) -> KeyEventKind {
    if key.bKeyDown != 0 {
        KeyEventKind::Press
    } else {
        KeyEventKind::Release
    }
}

/// One record, as the bulk read sees it.
enum Step {
    /// A text key, as crossterm would have made it.
    Text(Event),
    /// A record crossterm makes nothing of (a bare Shift or Ctrl going
    /// down or up around a capital): taken and dropped, as crossterm does.
    Skip,
    /// The first half of an emoji: see [`surrogate_pair`].
    High,
    /// An Alt going down: perhaps an Alt code, see [`alt_code`].
    Alt,
    /// Anything else: crossterm's to read.
    Stop,
}

fn step(key: &KEY_EVENT_RECORD) -> Step {
    let mods = modifiers(key.dwControlKeyState);
    let code = match key.wVirtualKeyCode {
        VK_MENU if key.bKeyDown != 0 && unit(key) == 0 => return Step::Alt,
        // An Alt code's character comes on the Alt's release.
        VK_MENU => return Step::Stop,
        VK_SHIFT | VK_CONTROL => return Step::Skip,
        VK_RETURN => KeyCode::Enter,
        VK_TAB if mods.contains(KeyModifiers::SHIFT) => return Step::Stop,
        VK_TAB => KeyCode::Tab,
        vk if named_key(vk) => return Step::Stop,
        _ => match unit(key) {
            // A control character crossterm looks up on the keyboard
            // layout; a lone low surrogate it pairs with what it holds.
            0..=0x1f | 0xDC00..=0xDFFF => return Step::Stop,
            0xD800..=0xDBFF => return Step::High,
            unit => match char::from_u32(u32::from(unit)) {
                Some(c) => KeyCode::Char(c),
                None => return Step::Stop,
            },
        },
    };
    let event = KeyEvent::new_with_kind(code, mods, kind(key));
    if types_text(&event) {
        Step::Text(Event::Key(event))
    } else {
        Step::Stop
    }
}

/// An emoji at the head of `records`: its press and release, and how many
/// records they took. The console writes a character outside the BMP as a
/// key per UTF-16 half — high down, high up, low down, low up — or with
/// the halves of each side together; crossterm pairs whichever two halves
/// come in a row, so the first order reached it as two broken halves and
/// the paste lost the emoji. Either order makes it whole here; anything
/// else, or a pair cut off by the end of the peek, is crossterm's.
fn surrogate_pair(records: &[INPUT_RECORD]) -> Option<(Vec<Event>, usize)> {
    let keys: Vec<KEY_EVENT_RECORD> = records.iter().take(4).map_while(key_record).collect();
    let is_high = |k: &KEY_EVENT_RECORD| (0xD800..=0xDBFF).contains(&unit(k));
    let is_low = |k: &KEY_EVENT_RECORD| (0xDC00..=0xDFFF).contains(&unit(k));
    let join = |high: &KEY_EVENT_RECORD, low: &KEY_EVENT_RECORD| {
        let c = char::decode_utf16([unit(high), unit(low)]).next()?.ok()?;
        let event = KeyEvent::new_with_kind(
            KeyCode::Char(c),
            modifiers(high.dwControlKeyState),
            kind(high),
        );
        types_text(&event).then_some(Event::Key(event))
    };
    match keys.as_slice() {
        [h, l, ..] if is_low(l) && h.bKeyDown == l.bKeyDown => {
            let first = join(h, l)?;
            // The release's halves usually follow the same way.
            if let [_, _, h2, l2] = keys.as_slice() {
                if is_high(h2) && is_low(l2) && h2.bKeyDown == l2.bKeyDown {
                    if let Some(second) = join(h2, l2) {
                        return Some((vec![first, second], 4));
                    }
                }
            }
            Some((vec![first], 2))
        }
        [h1, h2, l1, l2]
            if is_high(h2)
                && is_low(l1)
                && is_low(l2)
                && unit(h1) == unit(h2)
                && unit(l1) == unit(l2)
                && h1.bKeyDown == l1.bKeyDown
                && h2.bKeyDown == l2.bKeyDown =>
        {
            Some((vec![join(h1, l1)?, join(h2, l2)?], 4))
        }
        _ => None,
    }
}

/// A character typed as an Alt code at the head of `records`: its press
/// and release, and how many records it took. The console types out a
/// pasted character that isn't on the keyboard (`—`, `“`) as one — Alt
/// down, a few numpad keys with Alt held, and the character on the Alt's
/// release — and crossterm hands over only that release, which a paste
/// skips like every other: the character was lost. Anything that isn't a
/// whole Alt code (an Alt held for a chord, one cut off by the end of the
/// peek) is crossterm's.
fn alt_code(records: &[INPUT_RECORD]) -> Option<(Vec<Event>, usize)> {
    let alt = LEFT_ALT_PRESSED | RIGHT_ALT_PRESSED;
    for (i, record) in records.iter().enumerate().skip(1) {
        let key = key_record(record)?;
        match key.wVirtualKeyCode {
            0x60..=0x69 if key.dwControlKeyState & alt != 0 => continue,
            VK_MENU if key.bKeyDown == 0 => {
                let c = char::from_u32(u32::from(unit(&key))).filter(|&c| c >= ' ')?;
                let mods = modifiers(key.dwControlKeyState);
                let press = KeyEvent::new_with_kind(KeyCode::Char(c), mods, KeyEventKind::Press);
                if !types_text(&press) {
                    return None;
                }
                let release =
                    KeyEvent::new_with_kind(KeyCode::Char(c), mods, KeyEventKind::Release);
                return Some((vec![Event::Key(press), Event::Key(release)], i + 1));
            }
            _ => return None,
        }
    }
    None
}

/// Translate the run of text keys at the head of `records` into `out`, up
/// to about `max` events (an emoji's two may go one over); how many
/// records the run took. Stops at the first record crossterm should read.
fn translate_run(records: &[INPUT_RECORD], max: usize, out: &mut Vec<Event>) -> usize {
    let start = out.len();
    let mut used = 0;
    while used < records.len() && out.len() - start < max {
        let Some(key) = key_record(&records[used]) else {
            break;
        };
        let group = match step(&key) {
            Step::Text(event) => {
                out.push(event);
                used += 1;
                continue;
            }
            Step::Skip => {
                used += 1;
                continue;
            }
            Step::High => surrogate_pair(&records[used..]),
            Step::Alt => alt_code(&records[used..]),
            Step::Stop => break,
        };
        let Some((events, taken)) = group else { break };
        out.extend(events);
        used += taken;
    }
    used
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(vk: u16, unit: u16, down: bool, state: u32) -> INPUT_RECORD {
        let mut record = INPUT_RECORD {
            EventType: KEY_EVENT as u16,
            ..Default::default()
        };
        record.Event.KeyEvent = KEY_EVENT_RECORD {
            bKeyDown: i32::from(down),
            wRepeatCount: 1,
            wVirtualKeyCode: vk,
            wVirtualScanCode: 0,
            uChar: windows_sys::Win32::System::Console::KEY_EVENT_RECORD_0 { UnicodeChar: unit },
            dwControlKeyState: state,
        };
        record
    }

    /// `text` as the console hands a paste over: a press and a release per
    /// UTF-16 unit, capitals inside a Shift's press and release, `\r` an
    /// Enter, `\n` a Ctrl+Enter.
    pub(super) fn paste_records(text: &str) -> Vec<INPUT_RECORD> {
        let mut out = Vec::new();
        for c in text.chars() {
            let (vk, state) = match c {
                '\r' => (VK_RETURN, 0),
                '\n' => (VK_RETURN, LEFT_CTRL_PRESSED),
                '\t' => (VK_TAB, 0),
                'A'..='Z' => (c as u16, SHIFT_PRESSED),
                'a'..='z' => (c.to_ascii_uppercase() as u16, 0),
                '0'..='9' | ' ' => (c as u16, 0),
                _ => (0, 0),
            };
            let shifted = state & SHIFT_PRESSED != 0;
            if shifted {
                out.push(record(VK_SHIFT, 0, true, SHIFT_PRESSED));
            }
            let mut units = [0u16; 2];
            let units = c.encode_utf16(&mut units);
            if units.len() == 2 {
                // As conhost writes one: each half down, then up.
                for &u in units.iter() {
                    out.push(record(0, u, true, state));
                    out.push(record(0, u, false, state));
                }
            } else {
                out.push(record(vk, units[0], true, state));
                out.push(record(vk, units[0], false, state));
            }
            if shifted {
                out.push(record(VK_SHIFT, 0, false, 0));
            }
        }
        out
    }

    fn key(code: KeyCode, mods: KeyModifiers, kind: KeyEventKind) -> Event {
        Event::Key(KeyEvent::new_with_kind(code, mods, kind))
    }

    fn both(code: KeyCode, mods: KeyModifiers) -> [Event; 2] {
        [
            key(code, mods, KeyEventKind::Press),
            key(code, mods, KeyEventKind::Release),
        ]
    }

    /// What crossterm's parser makes of the same records (its
    /// `parse_key_event_record`): characters off `uChar`, Enter and Tab by
    /// their keys, modifiers off the control-key state, Shift's own
    /// records nothing at all.
    #[test]
    fn text_translates_as_crossterm_reads_it() {
        let records = paste_records("aB é—\t1\r\n");
        let mut out = Vec::new();
        assert_eq!(translate_run(&records, usize::MAX, &mut out), records.len());
        let none = KeyModifiers::NONE;
        let mut want = Vec::new();
        want.extend(both(KeyCode::Char('a'), none));
        want.extend(both(KeyCode::Char('B'), KeyModifiers::SHIFT));
        want.extend(both(KeyCode::Char(' '), none));
        want.extend(both(KeyCode::Char('é'), none));
        want.extend(both(KeyCode::Char('—'), none));
        want.extend(both(KeyCode::Tab, none));
        want.extend(both(KeyCode::Char('1'), none));
        want.extend(both(KeyCode::Enter, none));
        want.extend(both(KeyCode::Enter, KeyModifiers::CONTROL));
        assert_eq!(out, want);
    }

    /// AltGr's characters come as Ctrl+Alt and are text; Ctrl alone on a
    /// letter is a chord, and the run stops in front of it.
    #[test]
    fn altgr_is_text_and_a_chord_stops_the_run() {
        let altgr = LEFT_CTRL_PRESSED | RIGHT_ALT_PRESSED;
        let records = [
            record(0x51, '@' as u16, true, altgr),
            record(0x56, 0x16, true, LEFT_CTRL_PRESSED),
        ];
        let mut out = Vec::new();
        assert_eq!(translate_run(&records, usize::MAX, &mut out), 1);
        assert_eq!(
            out,
            vec![key(
                KeyCode::Char('@'),
                KeyModifiers::CONTROL | KeyModifiers::ALT,
                KeyEventKind::Press
            )]
        );
    }

    /// A paste with an arrow and a Ctrl+C typed behind it: the paste is
    /// read in bulk and the run stops at the arrow, which crossterm reads.
    #[test]
    fn keys_behind_a_paste_are_left_for_crossterm() {
        let mut records = paste_records("one\rtwo");
        let text = records.len();
        records.push(record(0x26, 0, true, 0)); // Up
        records.push(record(0x43, 0x03, true, LEFT_CTRL_PRESSED)); // Ctrl+C
        records.extend(paste_records("x"));
        let mut out = Vec::new();
        assert_eq!(translate_run(&records, usize::MAX, &mut out), text);
        assert_eq!(out.len(), text);
    }

    #[test]
    fn named_keys_and_shift_tab_stop_the_run() {
        for (vk, unit, state) in [
            (0x08, 0x08, 0),       // Backspace
            (0x1B, 0x1B, 0),       // Escape
            (0x25, 0, 0),          // Left
            (0x2E, 0, 0),          // Delete
            (0x70, 0, 0),          // F1
            (0x61, '1' as u16, 0), // numpad 1
            (VK_TAB, 0x09, SHIFT_PRESSED),
            (VK_MENU, 'é' as u16, 0), // an Alt code's release
        ] {
            let mut out = Vec::new();
            let records = [record(vk, unit, true, state)];
            assert_eq!(translate_run(&records, usize::MAX, &mut out), 0, "{vk:#x}");
        }
    }

    /// An emoji in either order the console writes one comes back whole,
    /// pressed and released.
    #[test]
    fn an_emoji_comes_back_whole() {
        let [h, l] = [0xD83D, 0xDE00];
        let laughing = KeyCode::Char('😀');
        let none = KeyModifiers::NONE;
        let halves_apart = [
            record(0, h, true, 0),
            record(0, h, false, 0),
            record(0, l, true, 0),
            record(0, l, false, 0),
        ];
        let halves_together = [
            record(0, h, true, 0),
            record(0, l, true, 0),
            record(0, h, false, 0),
            record(0, l, false, 0),
        ];
        for records in [halves_apart, halves_together] {
            let mut out = Vec::new();
            assert_eq!(translate_run(&records, usize::MAX, &mut out), 4);
            assert_eq!(out, both(laughing, none).to_vec());
        }
        // Cut off by the end of the peek: left for the next read.
        let mut out = Vec::new();
        assert_eq!(translate_run(&halves_apart[..2], usize::MAX, &mut out), 0);
    }

    /// `—` as ConPTY types it out of a paste (recorded under a
    /// pseudoconsole): an Alt code. It comes back pressed and released, and
    /// an Alt held for a chord stops the run as before.
    #[test]
    fn an_alt_code_is_its_character() {
        let em_dash = [
            record(VK_MENU, 0, true, LEFT_ALT_PRESSED),
            record(0x64, 0, true, LEFT_ALT_PRESSED),
            record(0x64, 0, false, LEFT_ALT_PRESSED),
            record(0x65, 0, true, LEFT_ALT_PRESSED),
            record(0x65, 0, false, LEFT_ALT_PRESSED),
            record(VK_MENU, 0x2014, false, 0),
        ];
        let mut out = Vec::new();
        assert_eq!(translate_run(&em_dash, usize::MAX, &mut out), 6);
        assert_eq!(out, both(KeyCode::Char('—'), KeyModifiers::NONE).to_vec());
        // Cut off before the release: crossterm's.
        let mut out = Vec::new();
        assert_eq!(translate_run(&em_dash[..5], usize::MAX, &mut out), 0);
        // Alt+x: a chord.
        let chord = [
            record(VK_MENU, 0, true, LEFT_ALT_PRESSED),
            record(0x58, 'x' as u16, true, LEFT_ALT_PRESSED),
        ];
        assert_eq!(translate_run(&chord, usize::MAX, &mut out), 0);
    }

    /// The run stops at `max` events, so a long paste still gives the loop
    /// its turn.
    #[test]
    fn the_run_stops_at_max() {
        let records = paste_records("abcdef");
        let mut out = Vec::new();
        assert_eq!(translate_run(&records, 5, &mut out), 5);
        assert_eq!(out.len(), 5);
    }

    /// CONSOLE READ BENCH: the old read (crossterm's `EventStream`, a
    /// record per call) against [`ConsoleEvents`] on a 110 KB paste
    /// written into a real console host, and the two compared event for
    /// event. Spawns this test binary again under a console of its own
    /// with no window, so it never touches the console it was run from.
    ///
    /// `cargo test -p nebula-tui console_read_bench -- --ignored --nocapture`
    #[test]
    #[ignore = "spawns a console host; a measurement, not a check"]
    fn console_read_bench() {
        use std::os::windows::process::CommandExt;
        const OUT: &str = "NEBULA_CONSOLE_BENCH_OUT";
        if let Ok(path) = std::env::var(OUT) {
            let report = bench_in_console();
            std::fs::write(path, report).unwrap();
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("bench.txt");
        let status = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "event_loop::host_input::console::tests::console_read_bench",
                "--exact",
                "--ignored",
                "--test-threads=1",
            ])
            .env(OUT, &path)
            .stdin(std::process::Stdio::null())
            .creation_flags(windows_sys::Win32::System::Threading::CREATE_NO_WINDOW)
            .status()
            .unwrap();
        assert!(status.success(), "bench child failed");
        println!("{}", std::fs::read_to_string(&path).unwrap());
    }

    /// CONPTY PASTE BENCH: a 110 KB bracketed paste written into a
    /// pseudoconsole, as Windows Terminal writes one, and read back the way
    /// the loop reads it — the first key, what is queued behind it through
    /// `queued_input`, the PASTE BURST settling it [`QUIET`] after the last
    /// key — once through crossterm's stream (0.48 to 0.50) and once in
    /// bulk. Times the first key to the settled paste.
    ///
    /// `cargo test -p nebula-tui conpty_paste_bench -- --ignored --nocapture`
    ///
    /// [`QUIET`]: super::super::super::paste_burst::QUIET
    #[test]
    #[ignore = "spawns a pseudoconsole; a measurement, not a check"]
    fn conpty_paste_bench() {
        use portable_pty::{native_pty_system, CommandBuilder, PtySize};
        use std::io::{Read, Write};
        const MODE: &str = "NEBULA_CONPTY_BENCH_MODE";
        const OUT: &str = "NEBULA_CONPTY_BENCH_OUT";
        if let (Ok(mode), Ok(path)) = (std::env::var(MODE), std::env::var(OUT)) {
            let report = read_paste_in_conpty(mode == "bulk");
            std::fs::write(path, report).unwrap();
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        let text = paste_text(110 * 1024);
        for mode in ["crossterm", "bulk"] {
            let path = dir.path().join(format!("{mode}.txt"));
            let pair = native_pty_system()
                .openpty(PtySize {
                    rows: 40,
                    cols: 120,
                    pixel_width: 0,
                    pixel_height: 0,
                })
                .unwrap();
            let mut cmd = CommandBuilder::new(std::env::current_exe().unwrap());
            cmd.args([
                "event_loop::host_input::console::tests::conpty_paste_bench",
                "--exact",
                "--ignored",
                "--test-threads=1",
                "--nocapture",
            ]);
            cmd.env(MODE, mode);
            cmd.env(OUT, &path);
            let mut child = pair.slave.spawn_command(cmd).unwrap();
            drop(pair.slave);
            // Drain the child's screen: answer the pseudoconsole's opening
            // cursor-position query (it waits for one), and say when the
            // child is ready to read.
            let (said_tx, said_rx) = std::sync::mpsc::channel();
            let mut reader = pair.master.try_clone_reader().unwrap();
            std::thread::spawn(move || {
                let mut seen = Vec::new();
                let mut buf = [0u8; 8192];
                let (mut asked, mut ready) = (false, false);
                while let Ok(n) = reader.read(&mut buf) {
                    if n == 0 {
                        break;
                    }
                    seen.extend_from_slice(&buf[..n]);
                    if !asked && seen.windows(4).any(|w| w == b"\x1b[6n") {
                        asked = true;
                        let _ = said_tx.send(false);
                    }
                    if !ready && seen.windows(5).any(|w| w == b"READY") {
                        ready = true;
                        let _ = said_tx.send(true);
                    }
                }
            });
            let mut writer = pair.master.take_writer().unwrap();
            loop {
                let ready = said_rx
                    .recv_timeout(std::time::Duration::from_secs(30))
                    .expect("the child never got ready");
                if ready {
                    break;
                }
                writer.write_all(b"\x1b[1;1R").unwrap();
                writer.flush().unwrap();
            }
            let paste = [&b"\x1b[200~"[..], text.as_bytes(), b"\x1b[201~"].concat();
            let began = std::time::Instant::now();
            writer.write_all(&paste).unwrap();
            writer.flush().unwrap();
            let wrote = began.elapsed();
            let status = child.wait().unwrap();
            assert!(status.success(), "{mode} child failed");
            println!(
                "{mode}: write into ConPTY returned after {:.3} s; {}",
                wrote.as_secs_f64(),
                std::fs::read_to_string(&path).unwrap()
            );
        }
    }

    /// The child's half: read the paste the parent writes, as the loop
    /// would, and say how long it took.
    fn read_paste_in_conpty(bulk: bool) -> String {
        use super::super::super::loop_run::queued_input;
        use super::super::super::paste_burst::{self, PasteBurst};
        use futures::StreamExt;
        use std::io::Write;
        let expected = paste_text(110 * 1024);
        crossterm::terminal::enable_raw_mode().unwrap();
        let mut stdout = std::io::stdout();
        let _ = stdout.write_all(b"\x1b[?2004hREADY\r\n");
        let _ = stdout.flush();
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let report = runtime.block_on(async {
            let mut input: Input = if bulk {
                Input::open()
            } else {
                Input::Stream(EventStream::new())
            };
            let mut burst = PasteBurst::default();
            let mut closed = false;
            let mut first = None;
            let mut events = 0usize;
            let mut turns = 0usize;
            // Keys handed on as keys, outside the paste: typed into the
            // agent ahead of it.
            let mut loose = 0usize;
            // The longest the loop was held reading.
            let mut longest = Duration::ZERO;
            let give_up = tokio::time::Instant::now() + Duration::from_secs(120);
            loop {
                let deadline = burst
                    .deadline()
                    .map_or(give_up, tokio::time::Instant::from_std);
                let next = tokio::time::timeout_at(deadline, input.next()).await;
                let turn = std::time::Instant::now();
                let batch = match next {
                    Ok(Some(Ok(event))) => {
                        first.get_or_insert(turn);
                        queued_input(&mut input, vec![event], &mut closed).await
                    }
                    Ok(_) => return "the console closed".to_string(),
                    Err(_) if !burst.is_holding() => return "no paste arrived".to_string(),
                    Err(_) => queued_input(&mut input, Vec::new(), &mut closed).await,
                };
                longest = longest.max(turn.elapsed());
                turns += 1;
                events += batch.len();
                let now = std::time::Instant::now();
                let mut out = burst.feed(batch, now);
                if burst.deadline().is_some_and(|d| d <= now) {
                    out.extend(burst.settle());
                }
                loose += out.iter().filter(|e| !matches!(e, Event::Paste(_))).count();
                if let Some(Event::Paste(text)) =
                    out.iter().find(|e| matches!(e, Event::Paste(_)))
                {
                    let took = first.map_or(Duration::ZERO, |f| f.elapsed());
                    let got: Vec<char> = text.chars().collect();
                    let want: Vec<char> = expected.chars().collect();
                    let differs = got.iter().zip(&want).position(|(a, b)| a != b);
                    let diff = match differs {
                        None if got.len() == want.len() => "none".to_string(),
                        None => format!("lengths {} and {}", got.len(), want.len()),
                        Some(at) => format!(
                            "at char {at}: got {:?}, want {:?}",
                            got[at..(at + 12).min(got.len())].iter().collect::<String>(),
                            want[at..(at + 12).min(want.len())].iter().collect::<String>()
                        ),
                    };
                    return format!(
                        "read {events} events in {turns} turns (the longest {:.0} ms), first key to one paste {:.3} s \
                         (the last {} ms of it the burst's quiet), {} bytes, {loose} key \
                         events handed on outside it; difference from what was written: {diff}",
                        longest.as_secs_f64() * 1e3,
                        took.as_secs_f64(),
                        paste_burst::QUIET.as_millis(),
                        text.len(),
                    );
                }
            }
        });
        let _ = crossterm::terminal::disable_raw_mode();
        report
    }

    /// 110 KB of something like code: capitals, punctuation, an `é` and a
    /// `—`, a CR at each line's end as Windows Terminal sends a paste.
    pub(super) fn paste_text(bytes: usize) -> String {
        let mut text = String::new();
        let mut i = 0;
        while text.len() < bytes {
            text.push_str(&format!(
                "    let Value_{i} = compute(\"café — {i}\", &[1, 2, 3]).unwrap_or_default();\r"
            ));
            i += 1;
        }
        text
    }

    fn write_records(conin: HANDLE, records: &[INPUT_RECORD]) {
        use windows_sys::Win32::System::Console::WriteConsoleInputW;
        for chunk in records.chunks(4096) {
            let mut written = 0;
            // SAFETY: a live console handle, a valid slice.
            let ok = unsafe {
                WriteConsoleInputW(conin, chunk.as_ptr(), chunk.len() as u32, &mut written)
            };
            assert!(ok != 0 && written as usize == chunk.len(), "write failed");
        }
    }

    fn bench_in_console() -> String {
        use futures::StreamExt;
        use std::fmt::Write as _;
        use std::time::Instant;
        let text = paste_text(110 * 1024);
        let records = paste_records(&text);
        let mut report = String::new();
        let _ = writeln!(
            report,
            "paste: {} bytes, {} chars, {} input records",
            text.len(),
            text.chars().count(),
            records.len()
        );
        let mut reference = Vec::new();
        assert_eq!(
            translate_run(&records, usize::MAX, &mut reference),
            records.len()
        );
        let expected = reference.len();

        // Old: crossterm's stream, a record per poll (what `queued_input`
        // did with `EventStream`).
        let console = ConsoleEvents::open().expect("a console");
        write_records(console.conin.0, &records);
        let began = Instant::now();
        let old: Vec<Event> = futures::executor::block_on(async {
            let mut stream = EventStream::new();
            let mut got = Vec::with_capacity(expected);
            while got.len() < expected {
                got.push(stream.next().await.unwrap().unwrap());
            }
            got
        });
        let old_took = began.elapsed();
        let _ = writeln!(
            report,
            "crossterm EventStream: {} events in {:.3} s ({:.1} us/record)",
            old.len(),
            old_took.as_secs_f64(),
            old_took.as_secs_f64() * 1e6 / records.len() as f64
        );

        // New: bulk reads of the text run, crossterm for the rest.
        let mut console = console;
        write_records(console.conin.0, &records);
        let began = Instant::now();
        let mut new = Vec::with_capacity(expected);
        let mut reads = 0;
        while new.len() < expected {
            reads += 1;
            if console.read_text(&mut new, usize::MAX).unwrap() == 0 {
                match console.next_event().unwrap() {
                    Some(event) => new.push(event),
                    None => break,
                }
            }
        }
        let new_took = began.elapsed();
        let _ = writeln!(
            report,
            "bulk ConsoleEvents:    {} events in {:.3} s ({:.2} us/record, {reads} read_text calls)",
            new.len(),
            new_took.as_secs_f64(),
            new_took.as_secs_f64() * 1e6 / records.len() as f64
        );
        let _ = writeln!(
            report,
            "same events as crossterm: {}",
            old == new && new == reference
        );
        assert_eq!(old, reference, "crossterm read the records differently");
        assert_eq!(new, reference, "the bulk read differs from crossterm");
        report
    }
}
