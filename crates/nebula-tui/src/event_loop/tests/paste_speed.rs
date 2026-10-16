//! A long paste the Windows console typed out, through the loop's paste
//! path the way the loop drives it: the first key from the stream, the
//! rest of what is queued read with `queued_input` (the text in bulk where
//! the host can), held by the PASTE BURST, settled once the keys stop, and
//! handled — one bracketed Input to the agent, its line breaks inside it,
//! and the keys typed after it still keys.

use super::super::host_input::HostInput;
use super::super::loop_run::queued_input;
use super::super::paste_burst::{self, PasteBurst};
use super::*;
use futures::Stream;
use std::collections::VecDeque;
use std::pin::Pin;
use std::task::{Context, Poll};

/// A host with `events` queued: one at a time from the stream, and — when
/// `bulk` — the text keys at the head in one go, as the console reads them.
struct Queued {
    events: VecDeque<Event>,
    bulk: bool,
}

impl Stream for Queued {
    type Item = std::io::Result<Event>;

    fn poll_next(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        match self.get_mut().events.pop_front() {
            Some(event) => Poll::Ready(Some(Ok(event))),
            None => Poll::Pending,
        }
    }
}

impl HostInput for Queued {
    fn read_text(&mut self, out: &mut Vec<Event>, max: usize) -> std::io::Result<usize> {
        if !self.bulk {
            return Ok(0);
        }
        let mut taken = 0;
        while taken < max {
            match self.events.front() {
                Some(Event::Key(key)) if paste_burst::types_text(key) => {
                    out.extend(self.events.pop_front());
                    taken += 1;
                }
                _ => break,
            }
        }
        Ok(taken)
    }
}

/// `text` as the console types a paste out: a press and a release per
/// character, a carriage return an Enter.
fn typed(text: &str) -> Vec<Event> {
    text.chars()
        .flat_map(|c| {
            let code = if c == '\r' {
                KeyCode::Enter
            } else {
                KeyCode::Char(c)
            };
            [KeyEventKind::Press, KeyEventKind::Release]
                .map(|kind| Event::Key(KeyEvent::new_with_kind(code, KeyModifiers::NONE, kind)))
        })
        .collect()
}

fn paste_text(bytes: usize) -> String {
    let mut text = String::new();
    let mut i = 0;
    while text.len() < bytes {
        text.push_str(&format!(
            "    let value_{i} = compute(\"line {i}\", &[1, 2, 3]).unwrap_or_default();\r"
        ));
        i += 1;
    }
    text
}

/// The loop's turns over `input` until it runs dry, as `loop_run` takes
/// them: a turn's first event from the stream, what's queued behind it
/// through `queued_input`, fed to the burst; once nothing more comes the
/// burst's deadline settles it. Returns what was handed on and how many
/// turns that took.
fn drive(input: &mut Queued, burst: &mut PasteBurst) -> (Vec<Event>, usize) {
    use futures::StreamExt;
    let mut handled = Vec::new();
    let mut turns = 0;
    let mut closed = false;
    loop {
        let first = futures::executor::block_on(futures::future::poll_fn(|cx| {
            Poll::Ready(match input.poll_next_unpin(cx) {
                Poll::Ready(Some(Ok(event))) => Some(event),
                _ => None,
            })
        }));
        let Some(event) = first else { break };
        turns += 1;
        if burst.is_holding() || paste_burst::is_text_press(&event) {
            let batch = futures::executor::block_on(queued_input(input, vec![event], &mut closed));
            handled.extend(burst.feed(batch, std::time::Instant::now()));
        } else {
            handled.push(event);
        }
    }
    assert!(!closed);
    handled.extend(burst.settle());
    (handled, turns)
}

fn pasting_app() -> (App, SessionRef) {
    let mut app = App::new();
    seed_tree(&mut app);
    let sref = SessionRef::Agent(AgentId("a1".into()));
    let mut term = AttachedTerm::new(sref.clone(), 80, 24);
    term.apply_output(0, b"\x1b[?2004h");
    app.pane.term = Some(term);
    app.nav.focus = Focus::Terminal;
    app.pane.term_locked = true;
    (app, sref)
}

/// A 110 KB paste read in bulk: a handful of turns, one bracketed Input
/// with every line break inside it — none submitted — and the key typed
/// once it has landed goes to the agent as the key it is.
#[test]
fn a_110_kb_paste_is_one_input_in_a_few_turns() {
    let (mut app, sref) = pasting_app();
    let text = paste_text(110 * 1024);
    let mut input = Queued {
        events: typed(&text).into(),
        bulk: true,
    };
    let mut burst = PasteBurst::default();
    let (handled, turns) = drive(&mut input, &mut burst);
    // 4 here; a 1024-event batch took 220. Room for a slow machine, where
    // the batch's time runs out first.
    assert!(turns <= 32, "read in {turns} turns");
    assert_eq!(handled, vec![Event::Paste(text.clone())]);

    let mut out = Vec::new();
    for event in handled {
        handle_terminal_event(&mut app, event, &mut out);
    }
    let want = [&b"\x1b[200~"[..], text.as_bytes(), b"\x1b[201~"].concat();
    match out.as_slice() {
        [ClientRequest::Input { session, data }] => {
            assert_eq!(session, &sref);
            assert!(data == &want, "the paste arrives whole, bracketed");
        }
        other => panic!("expected one Input request, got {} of them", other.len()),
    }

    // Typed afterwards: a lone key. Straight after, it is held a moment in
    // case it is the paste's tail, and comes back as the key it is (no
    // release with it); after that, never held.
    out.clear();
    let x = typed("x");
    let now = std::time::Instant::now();
    assert!(burst.feed(x[..1].to_vec(), now).is_empty());
    assert_eq!(burst.settle(), x[..1]);
    assert_eq!(
        burst.feed(x[..1].to_vec(), now + std::time::Duration::from_secs(2)),
        x[..1]
    );
    handle_terminal_event(&mut app, x[0].clone(), &mut out);
    assert!(
        matches!(out.as_slice(), [ClientRequest::Input { data, .. }] if data.as_slice() == b"x"),
        "{out:?}"
    );
}

/// Read one at a time (crossterm's stream, where there's no console), the
/// same paste is the same one Input; only the turns differ.
#[test]
fn a_paste_read_one_event_at_a_time_is_the_same_paste() {
    let text = paste_text(8 * 1024);
    let mut input = Queued {
        events: typed(&text).into(),
        bulk: false,
    };
    let (handled, _) = drive(&mut input, &mut PasteBurst::default());
    assert_eq!(handled, vec![Event::Paste(text)]);
}

/// An arrow and a Ctrl+C typed right behind a paste, in the same queue:
/// the bulk read stops at the arrow, the stream hands it over next, and
/// both follow the paste in order — neither swallowed into it.
#[test]
fn keys_queued_behind_a_paste_follow_it() {
    let text = paste_text(4 * 1024);
    let up = Event::Key(KeyEvent::new(KeyCode::Up, KeyModifiers::NONE));
    let ctrl_c = Event::Key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL));
    let mut events = typed(&text);
    events.push(up.clone());
    events.push(ctrl_c.clone());
    let mut input = Queued {
        events: events.into(),
        bulk: true,
    };
    let (handled, _) = drive(&mut input, &mut PasteBurst::default());
    assert_eq!(handled, vec![Event::Paste(text), up, ctrl_c]);
}

/// The status bar counts a burst big enough to mention while it is read,
/// and lets go of it once it settles.
#[test]
fn a_held_burst_is_counted_for_the_status_bar() {
    let mut burst = PasteBurst::default();
    burst.feed(typed("ab\rcd"), std::time::Instant::now());
    assert_eq!(burst.pasting(), None, "a few keys say nothing");
    let text = paste_text(3000);
    burst.feed(typed(&text), std::time::Instant::now());
    assert_eq!(burst.pasting(), Some(5 + text.len()));
    burst.settle();
    assert_eq!(burst.pasting(), None);
}

/// No frame between the turns a burst is read in, but one every
/// `PASTE_FRAME` so the count moves; the pacer decides the rest of the
/// time.
#[test]
fn frames_wait_while_a_burst_is_read() {
    let t0 = tokio::time::Instant::now();
    let next = t0 + Duration::from_millis(2);
    assert_eq!(paste_burst::draw_at(next, t0, false), next);
    assert_eq!(
        paste_burst::draw_at(next, t0, true),
        t0 + paste_burst::PASTE_FRAME
    );
    let late = t0 + Duration::from_secs(1);
    assert_eq!(paste_burst::draw_at(late, t0, true), late);
}

/// The status bar says how much has come, at 80 columns, whole.
#[test]
fn the_status_bar_says_pasting() {
    let (mut app, _) = pasting_app();
    app.chrome.pasting = Some(64 * 1024 + 1);
    let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
    terminal.draw(|f| ui::draw(f, &mut app)).unwrap();
    let text = buffer_text(&terminal);
    let bar = text.lines().last().unwrap_or_default();
    assert!(bar.contains("pasting… 65 KB"), "{bar}");
}

/// PASTE PATH BENCH: the loop's side of a 110 KB paste — `queued_input`,
/// the burst, the settle and the bracketed Input — read in bulk and one
/// event at a time. The console's own read is `console_read_bench`.
///
/// `cargo test -p nebula-tui paste_path_bench -- --ignored --nocapture`
#[test]
#[ignore = "a measurement, not a check"]
fn paste_path_bench() {
    for bulk in [false, true] {
        let (mut app, _) = pasting_app();
        let text = paste_text(110 * 1024);
        let events = typed(&text);
        let count = events.len();
        let mut input = Queued {
            events: events.into(),
            bulk,
        };
        let began = std::time::Instant::now();
        let (handled, turns) = drive(&mut input, &mut PasteBurst::default());
        let read = began.elapsed();
        let mut out = Vec::new();
        for event in handled {
            handle_terminal_event(&mut app, event, &mut out);
        }
        let all = began.elapsed();
        assert_eq!(out.len(), 1);
        println!(
            "{}: {count} events, {turns} turns, read+settled {:.1} ms, to one Input {:.1} ms",
            if bulk { "bulk" } else { "one at a time" },
            read.as_secs_f64() * 1e3,
            all.as_secs_f64() * 1e3
        );
    }
}
