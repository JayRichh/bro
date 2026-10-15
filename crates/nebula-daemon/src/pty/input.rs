//! The PTY's input side: every write to the child goes through one queue
//! and one thread of its own, in the order it was asked for.
//!
//! A write to a PTY blocks for as long as the child is slow to read, and
//! a pseudoconsole reads slowly: one 110 KB paste written to ConPTY took
//! three seconds to go in. Written where it was asked for, that blocked
//! the client's whole request loop behind the paste (its next key, its
//! resize, its detach) — and the pump, which answers the child's terminal
//! queries from a tokio worker, would block on the same writer, stop
//! draining the child's output and leave it stuck writing, unable to read
//! the rest of the paste. Queued, a write returns at once; a paste waits
//! only on itself.
//!
//! Each message goes in whole and in order — a query's answer never lands
//! inside a paste's brackets — and whatever is queued behind a write goes
//! with the next one.

use anyhow::{anyhow, Result};
use std::io::Write;
use std::sync::mpsc;

pub(super) struct InputQueue {
    tx: mpsc::Sender<Vec<u8>>,
}

impl InputQueue {
    /// Start the thread that owns `writer`. It ends when the queue is
    /// dropped (the session with it) or a write fails (the child is gone).
    pub(super) fn spawn(mut writer: Box<dyn Write + Send>) -> Self {
        let (tx, rx) = mpsc::channel::<Vec<u8>>();
        std::thread::Builder::new()
            .name("pty-input".into())
            .stack_size(256 * 1024)
            .spawn(move || {
                while let Ok(mut data) = rx.recv() {
                    while let Ok(more) = rx.try_recv() {
                        data.extend_from_slice(&more);
                    }
                    if let Err(e) = writer.write_all(&data).and_then(|()| writer.flush()) {
                        tracing::warn!(error = %e, "pty write failed");
                        break;
                    }
                }
            })
            .expect("spawn pty input thread");
        Self { tx }
    }

    /// Queue `data` for the child. Never blocks; fails only once the
    /// writer has given up.
    pub(super) fn write(&self, data: &[u8]) -> Result<()> {
        if data.is_empty() {
            return Ok(());
        }
        self.tx
            .send(data.to_vec())
            .map_err(|_| anyhow!("pty input closed"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};
    use std::time::{Duration, Instant};

    /// A writer that blocks its first write until let go, the way ConPTY
    /// holds a large paste, and records everything written.
    struct Stuck {
        gate: mpsc::Receiver<()>,
        held: bool,
        out: Arc<Mutex<Vec<u8>>>,
    }

    impl Write for Stuck {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            if !self.held {
                self.held = true;
                let _ = self.gate.recv();
            }
            self.out.lock().unwrap().extend_from_slice(buf);
            Ok(buf.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    fn wait_for(out: &Arc<Mutex<Vec<u8>>>, len: usize) -> Vec<u8> {
        let deadline = Instant::now() + Duration::from_secs(5);
        while out.lock().unwrap().len() < len {
            assert!(Instant::now() < deadline, "the writer never caught up");
            std::thread::sleep(Duration::from_millis(5));
        }
        out.lock().unwrap().clone()
    }

    /// A paste the child is slow to take holds nothing up: the paste's
    /// write returns at once, and so does a query's answer queued behind
    /// it — which goes in after the paste, never inside its brackets.
    #[test]
    fn a_slow_write_blocks_no_caller_and_keeps_the_order() {
        let (release, gate) = mpsc::channel();
        let out = Arc::new(Mutex::new(Vec::new()));
        let queue = InputQueue::spawn(Box::new(Stuck {
            gate,
            held: false,
            out: out.clone(),
        }));
        let paste = [&b"\x1b[200~"[..], &[b'x'; 64 * 1024], b"\x1b[201~"].concat();
        let began = Instant::now();
        queue.write(&paste).unwrap();
        queue.write(b"\x1b[1;1R").unwrap();
        queue.write(b"\r").unwrap();
        assert!(
            began.elapsed() < Duration::from_millis(500),
            "a caller waited on the child: {:?}",
            began.elapsed()
        );
        assert!(out.lock().unwrap().is_empty(), "the child hasn't read yet");
        release.send(()).unwrap();
        let want = [&paste[..], b"\x1b[1;1R", b"\r"].concat();
        assert_eq!(wait_for(&out, want.len()), want);
    }

    /// A writer that failed (the child is gone) is reported to the next
    /// caller instead of queueing into nothing.
    #[test]
    fn a_failed_writer_is_reported() {
        struct Broken;
        impl Write for Broken {
            fn write(&mut self, _: &[u8]) -> std::io::Result<usize> {
                Err(std::io::ErrorKind::BrokenPipe.into())
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }
        let queue = InputQueue::spawn(Box::new(Broken));
        queue.write(b"a").unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        while queue.write(b"b").is_ok() {
            assert!(Instant::now() < deadline, "the failure never surfaced");
            std::thread::sleep(Duration::from_millis(5));
        }
    }
}
