//! How long a large paste takes to reach a child through a real ConPTY:
//! written through the session's `InputQueue`, read by a child that takes
//! its console input the way node (claude) does: raw, VT input on,
//! `ReadConsoleInputW` in bulk. The child is this test binary again,
//! running [`child`]. Ignored; run it with
//!
//! ```text
//! cargo test -p nebula-daemon --lib pty::paste_bench::bench -- --ignored --nocapture
//! ```
//!
//! Knobs, each an environment variable:
//! - `KETE_PASTE_BENCH_KB`: the paste's size in KB (110).
//! - `KETE_PASTE_BENCH_CHUNK`: queue it in writes of this many bytes
//!   rather than whole (0, whole).
//! - `KETE_PASTE_BENCH_LATE`: the child waits this many ms before its
//!   first read, so the writer's time is the pseudoconsole's alone (0).
//! - `KETE_PASTE_BENCH_PLAIN`: set, the paste goes without its brackets.
//! - `KETE_PASTE_BENCH_NOCR`: set, one long line with no carriage returns.
//!
//! What it showed on Windows 11 26200 (inbox conhost 10.0.26100), debug
//! build: about 23 KB/s however the paste is cut or whatever it holds:
//! 10 KB in ~450 ms, 40 KB in ~1.75 s, 110 KB in ~4.5 s. The child reads
//! all 225 000 key records in under 10 ms once they are there, and a child
//! that reads nothing for 8 s leaves the writer just as slow, so the time is
//! the pseudoconsole turning the bytes into input records, not the daemon
//! and not the child.

use super::input::InputQueue;
use portable_pty::{native_pty_system, CommandBuilder, PtySize};
use std::io::{Read, Write};
use std::sync::mpsc;
use std::time::{Duration, Instant};

const CHILD_ENV: &str = "KETE_PASTE_BENCH_CHILD";
const LATE_ENV: &str = "KETE_PASTE_BENCH_LATE";

fn knob(name: &str) -> Option<usize> {
    std::env::var(name).ok().and_then(|v| v.parse().ok())
}

fn flag(name: &str) -> bool {
    std::env::var_os(name).is_some()
}

/// A paste of `kb` KB as the client sends one: bracketed, lines of text
/// with a carriage return after each. Returns it and the letters in it
/// (what the child counts; the brackets and returns it does not).
fn paste(kb: usize) -> (Vec<u8>, usize) {
    let mut body = Vec::with_capacity(kb * 1024);
    let mut letters = 0;
    while body.len() < kb * 1024 {
        body.extend((0..79u8).map(|i| b'a' + i % 26));
        letters += 79;
        if !flag("KETE_PASTE_BENCH_NOCR") {
            body.push(b'\r');
        }
    }
    let text = if flag("KETE_PASTE_BENCH_PLAIN") {
        body
    } else {
        [&b"\x1b[200~"[..], &body, b"\x1b[201~"].concat()
    };
    (text, letters)
}

enum Seen {
    Ready,
    /// The child's own count: its reads, and the records they took.
    Done(String),
    Query,
}

struct Run {
    /// Until the writer had handed the pseudoconsole the last byte.
    written: Duration,
    /// Until the child had read the last letter.
    read: Duration,
    child: String,
}

/// One paste through a fresh pseudoconsole.
fn run_once(kb: usize, write: &dyn Fn(&InputQueue, &[u8])) -> Run {
    let (text, letters) = paste(kb);
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
        "pty::paste_bench::child",
        "--exact",
        "--ignored",
        "--nocapture",
        "--test-threads=1",
    ]);
    cmd.env(CHILD_ENV, letters.to_string());
    if let Some(late) = knob(LATE_ENV) {
        cmd.env(LATE_ENV, late.to_string());
    }
    let mut child = pair.slave.spawn_command(cmd).unwrap();
    drop(pair.slave);
    let mut reader = pair.master.try_clone_reader().unwrap();
    let (seen_tx, seen) = mpsc::channel();
    std::thread::spawn(move || {
        let mut buf = vec![0u8; 64 * 1024];
        let mut all = Vec::new();
        let (mut ready, mut queries) = (false, 0);
        while let Ok(n) = reader.read(&mut buf) {
            if n == 0 {
                break;
            }
            all.extend_from_slice(&buf[..n]);
            let text = String::from_utf8_lossy(&all);
            // The pseudoconsole asks where the cursor is and waits.
            let asked = text.matches("\x1b[6n").count();
            for _ in queries..asked {
                let _ = seen_tx.send(Seen::Query);
            }
            queries = asked;
            if !ready && text.contains("PASTE-BENCH-READY") {
                ready = true;
                let _ = seen_tx.send(Seen::Ready);
            }
            if let Some(i) = text.find("PASTE-BENCH-DONE") {
                if let Some(end) = text[i..].find(" ms.") {
                    let stats = &text[i + "PASTE-BENCH-DONE ".len()..i + end + " ms".len()];
                    let _ = seen_tx.send(Seen::Done(stats.to_string()));
                    break;
                }
            }
        }
    });
    // The writer on its own thread, as in a session, telling the bench
    // when the last byte went in.
    let (wrote_tx, wrote) = mpsc::channel();
    let queue = InputQueue::spawn(Box::new(Signal {
        inner: pair.master.take_writer().unwrap(),
        total: text.len(),
        seen: 0,
        done: wrote_tx,
    }));
    let deadline = Duration::from_secs(120);
    loop {
        match seen
            .recv_timeout(deadline)
            .expect("the child never got ready")
        {
            Seen::Query => queue.write(b"\x1b[1;1R").unwrap(),
            Seen::Ready => break,
            Seen::Done(_) => panic!("done before the paste"),
        }
    }
    let began = Instant::now();
    write(&queue, &text);
    let mut written = None;
    let (read, stats) = loop {
        if written.is_none() && wrote.try_recv().is_ok() {
            written = Some(began.elapsed());
        }
        match seen.recv_timeout(Duration::from_millis(1)) {
            Ok(Seen::Done(stats)) => break (began.elapsed(), stats),
            Ok(Seen::Query) => queue.write(b"\x1b[1;1R").unwrap(),
            Ok(Seen::Ready) | Err(mpsc::RecvTimeoutError::Timeout) => {
                assert!(began.elapsed() < deadline, "the child never read it all");
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => panic!("the child's output ended"),
        }
    };
    let written = written.unwrap_or_else(|| {
        wrote.recv_timeout(deadline).unwrap();
        began.elapsed()
    });
    let _ = child.kill();
    Run {
        written,
        read,
        child: stats,
    }
}

/// The pty writer, telling the bench when the paste's last byte went in.
struct Signal {
    inner: Box<dyn Write + Send>,
    total: usize,
    seen: usize,
    done: mpsc::Sender<()>,
}

impl Write for Signal {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let n = self.inner.write(buf)?;
        let before = self.seen;
        self.seen += n;
        if before < self.total && self.seen >= self.total {
            let _ = self.done.send(());
        }
        Ok(n)
    }
    fn flush(&mut self) -> std::io::Result<()> {
        self.inner.flush()
    }
}

#[test]
#[ignore = "a benchmark: spawns real pseudoconsoles; run with --ignored --nocapture"]
fn bench() {
    let kb = knob("KETE_PASTE_BENCH_KB").unwrap_or(110);
    let chunk = knob("KETE_PASTE_BENCH_CHUNK").unwrap_or(0);
    let write = move |q: &InputQueue, text: &[u8]| match chunk {
        0 => q.write(text).unwrap(),
        n => text.chunks(n).for_each(|c| q.write(c).unwrap()),
    };
    // A warm-up, so the first row doesn't pay for starting conhost cold.
    run_once(4, &write);
    let ms = |d: Duration| d.as_secs_f64() * 1000.0;
    for i in 0..3 {
        let run = run_once(kb, &write);
        eprintln!(
            "paste {kb} KB run {i}: written {:>7.1} ms, read by the child {:>7.1} ms \
             ({:.0} KB/s; child: {})",
            ms(run.written),
            ms(run.read),
            kb as f64 / run.read.as_secs_f64(),
            run.child,
        );
    }
}

/// The bench's child: raw console input with VT input on, as node sets it
/// for a TTY in raw mode, read with `ReadConsoleInputW` in bulk until
/// `KETE_PASTE_BENCH_CHILD` letters have come. A no-op outside the bench.
#[test]
#[ignore = "the paste bench's child"]
fn child() {
    use windows_sys::Win32::System::Console::{
        GetStdHandle, ReadConsoleInputW, SetConsoleMode, ENABLE_VIRTUAL_TERMINAL_INPUT,
        ENABLE_WINDOW_INPUT, INPUT_RECORD, KEY_EVENT, STD_INPUT_HANDLE,
    };
    let Some(want) = knob(CHILD_ENV) else {
        return;
    };
    let late = knob(LATE_ENV).unwrap_or(0) as u64;
    let mut out = std::io::stdout();
    let (mut got, mut reads, mut records_read) = (0usize, 0usize, 0usize);
    let mut first = None;
    // SAFETY: plain console calls on this process's own input handle, into
    // a buffer of the length passed.
    unsafe {
        let input = GetStdHandle(STD_INPUT_HANDLE);
        SetConsoleMode(input, ENABLE_WINDOW_INPUT | ENABLE_VIRTUAL_TERMINAL_INPUT);
        write!(out, "PASTE-BENCH-READY\r\n").unwrap();
        out.flush().unwrap();
        std::thread::sleep(Duration::from_millis(late));
        let mut records: Vec<INPUT_RECORD> = vec![std::mem::zeroed(); 8192];
        while got < want {
            let mut n = 0u32;
            let ok = ReadConsoleInputW(input, records.as_mut_ptr(), records.len() as u32, &mut n);
            assert!(ok != 0, "ReadConsoleInputW failed");
            first.get_or_insert_with(Instant::now);
            reads += 1;
            records_read += n as usize;
            for r in &records[..n as usize] {
                if u32::from(r.EventType) != KEY_EVENT {
                    continue;
                }
                let key = r.Event.KeyEvent;
                let c = key.uChar.UnicodeChar;
                if key.bKeyDown != 0 && (u16::from(b'a')..=u16::from(b'z')).contains(&c) {
                    got += usize::from(key.wRepeatCount.max(1));
                }
            }
        }
    }
    let took = first.map(|f: Instant| f.elapsed()).unwrap_or_default();
    write!(
        out,
        "PASTE-BENCH-DONE {reads} reads, {records_read} records in {:.1} ms.\r\n",
        took.as_secs_f64() * 1000.0
    )
    .unwrap();
    out.flush().unwrap();
}
