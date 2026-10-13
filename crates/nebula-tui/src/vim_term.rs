//! Embedded editor modal: a local PTY child (vim) rendered inside the TUI.
//!
//! Unlike agent/terminal sessions (daemon-owned PTYs reached over IPC), the
//! editor is spawned in-process: it's a short-lived affordance of the
//! find-in-files overlay, needs no persistence or reattach, and dies with
//! the client. Output flows reader thread → mpsc → the main loop, which
//! feeds the vt100 parser here (the daemon's `PtySession` shape, minus the
//! ring buffer and broadcast).

use portable_pty::{native_pty_system, ChildKiller, CommandBuilder, MasterPty, PtySize};
use ratatui::layout::Rect;
use std::io::{Read, Write};
use std::path::Path;
use tokio::sync::mpsc::UnboundedSender;

/// Reader-thread → main-loop messages. `generation` stamps which spawn they
/// belong to, so bytes from a closed editor can't bleed into a new one.
#[derive(Debug)]
pub enum VimEvent {
    Output { generation: u64, data: Vec<u8> },
    Exited { generation: u64 },
}

#[cfg(unix)]
type Master = Box<dyn MasterPty + Send>;
/// Windows: shared with the exit watcher in [`VimTerm::spawn_cmd`], which
/// drops the pseudoconsole once the child exits.
#[cfg(windows)]
type Master = std::sync::Arc<std::sync::Mutex<Option<Box<dyn MasterPty + Send>>>>;

pub struct VimTerm {
    pub generation: u64,
    pub parser: vt100::Parser,
    /// Size the parser (and PTY) currently uses.
    pub cols: u16,
    pub rows: u16,
    /// "path:line" for the modal title.
    pub title: String,
    /// Rendered inside the open overlay's preview pane — the TREE BROWSER's,
    /// or the FILE TABS' body — instead of the centered modal (set by that
    /// overlay's Enter).
    pub embedded: bool,
    /// Inner rect from the last draw; `sync_vim_size` resizes to it.
    pub area: Rect,
    master: Master,
    writer: Box<dyn Write + Send>,
    killer: Box<dyn ChildKiller + Send + Sync>,
    /// How much of a cursor-position query (`CSI 6n`) the last chunk ended
    /// on, so one split across reads is still answered.
    #[cfg(windows)]
    dsr_matched: usize,
}

impl VimTerm {
    /// Spawn `editor +<line> <file>` in the checkout. `Err` is a user-facing
    /// flash message.
    #[allow(clippy::too_many_arguments)]
    pub fn spawn_editor(
        editor: &str,
        root: &Path,
        file: &str,
        line: u64,
        cols: u16,
        rows: u16,
        generation: u64,
        tx: UnboundedSender<VimEvent>,
    ) -> Result<Self, String> {
        let title = format!("{file}:{line}");
        Self::spawn_cmd(
            editor,
            &[format!("+{line}"), file.to_string()],
            root,
            title,
            cols,
            rows,
            generation,
            tx,
        )
    }

    /// Editor-agnostic spawn (tests use a shell here).
    #[allow(clippy::too_many_arguments)]
    pub fn spawn_cmd(
        program: &str,
        args: &[String],
        cwd: &Path,
        title: String,
        cols: u16,
        rows: u16,
        generation: u64,
        tx: UnboundedSender<VimEvent>,
    ) -> Result<Self, String> {
        let cols = cols.max(2);
        let rows = rows.max(2);
        let pair = native_pty_system()
            .openpty(PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|e| format!("openpty failed: {e}"))?;

        let mut cmd = CommandBuilder::new(program);
        cmd.args(args);
        cmd.cwd(cwd);
        // The modal is drawn on nebula's grid, the same truecolor terminal
        // every session pane is (see `nebula_core::env::PANE_TERM`).
        cmd.env("TERM", nebula_core::env::PANE_TERM);
        cmd.env("COLORTERM", nebula_core::env::PANE_COLORTERM);

        let mut child = pair
            .slave
            .spawn_command(cmd)
            .map_err(|e| format!("failed to launch {program}: {e}"))?;
        drop(pair.slave);

        let killer = child.clone_killer();
        let mut reader = pair
            .master
            .try_clone_reader()
            .map_err(|e| format!("pty reader: {e}"))?;
        let writer = pair
            .master
            .take_writer()
            .map_err(|e| format!("pty writer: {e}"))?;

        std::thread::spawn(move || {
            let mut buf = [0u8; 8192];
            loop {
                match reader.read(&mut buf) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => {
                        let data = buf[..n].to_vec();
                        if tx.send(VimEvent::Output { generation, data }).is_err() {
                            break; // main loop gone
                        }
                    }
                }
            }
            #[cfg(unix)]
            let _ = child.wait(); // reap
            let _ = tx.send(VimEvent::Exited { generation });
        });

        #[cfg(unix)]
        let master = pair.master;
        // ConPTY keeps the output pipe open after the child exits, for as
        // long as the pseudoconsole lives, so EOF alone never comes: a
        // watcher waits for the child, then drops the pseudoconsole, and the
        // reader above reaches EOF and reports the exit.
        #[cfg(windows)]
        let master = {
            let master = std::sync::Arc::new(std::sync::Mutex::new(Some(pair.master)));
            let closer = master.clone();
            std::thread::spawn(move || {
                let _ = child.wait();
                // Taken out of the lock first: dropping it may block until
                // the reader has drained what the console still had to say.
                let console = closer.lock().ok().and_then(|mut m| m.take());
                drop(console);
            });
            master
        };

        Ok(Self {
            generation,
            parser: vt100::Parser::new(rows, cols, 0),
            cols,
            rows,
            title,
            embedded: false,
            area: Rect::default(),
            master,
            writer,
            killer,
            #[cfg(windows)]
            dsr_matched: 0,
        })
    }

    /// Feed reader-thread output into the emulator.
    #[cfg(unix)]
    pub fn process(&mut self, data: &[u8]) {
        self.parser.process(data);
    }

    /// Windows: ConPTY opens with a cursor-position query (`CSI 6n`) and
    /// holds the child's output until it is answered (docs/PORT.md), so the
    /// emulator answers it, from where its cursor stands just past the query.
    #[cfg(windows)]
    pub fn process(&mut self, data: &[u8]) {
        const DSR: &[u8] = b"\x1b[6n";
        let mut start = 0;
        for (i, &byte) in data.iter().enumerate() {
            self.dsr_matched = if byte == DSR[self.dsr_matched] {
                self.dsr_matched + 1
            } else {
                usize::from(byte == DSR[0])
            };
            if self.dsr_matched == DSR.len() {
                self.dsr_matched = 0;
                self.parser.process(&data[start..=i]);
                start = i + 1;
                let (row, col) = self.parser.screen().cursor_position();
                self.input(format!("\x1b[{};{}R", row + 1, col + 1).as_bytes());
            }
        }
        self.parser.process(&data[start..]);
    }

    /// Encoded keystrokes (and bracketed pastes) go straight to the child.
    pub fn input(&mut self, data: &[u8]) {
        // A write error means the child died; the Exited event closes us.
        let _ = self
            .writer
            .write_all(data)
            .and_then(|_| self.writer.flush());
    }

    /// Keep the PTY and parser sized to the drawn modal.
    pub fn resize(&mut self, cols: u16, rows: u16) {
        let (cols, rows) = (cols.max(2), rows.max(2));
        if (self.cols, self.rows) == (cols, rows) {
            return;
        }
        self.cols = cols;
        self.rows = rows;
        self.parser.screen_mut().set_size(rows, cols);
        let size = PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        };
        #[cfg(unix)]
        let _ = self.master.resize(size);
        #[cfg(windows)]
        if let Some(master) = self.master.lock().ok().as_deref().and_then(Option::as_ref) {
            let _ = master.resize(size);
        }
    }

    /// Force-close (the Ctrl+Q hatch); the reader thread reaps the child.
    pub fn kill(&mut self) {
        let _ = self.killer.kill();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    /// A child that prints `VIM_MODAL_TEST` and stays up.
    #[cfg(unix)]
    const PRINTER: (&str, [&str; 2]) = ("/bin/sh", ["-c", "printf 'VIM_MODAL_TEST'; sleep 30"]);
    #[cfg(windows)]
    const PRINTER: (&str, [&str; 3]) = (
        "cmd.exe",
        ["/d", "/c", "echo VIM_MODAL_TEST& ping -n 30 127.0.0.1 >nul"],
    );

    /// A child that reads one line, prints it back as `GOT:<line>`, and exits.
    #[cfg(unix)]
    const ECHO_ONE_LINE: (&str, [&str; 2]) = ("/bin/sh", ["-c", "read line; printf \"GOT:$line\""]);
    #[cfg(windows)]
    const ECHO_ONE_LINE: (&str, [&str; 4]) = (
        "cmd.exe",
        ["/d", "/v:on", "/c", "set /p line=& echo GOT:!line!"],
    );

    async fn recv_until(
        rx: &mut tokio::sync::mpsc::UnboundedReceiver<VimEvent>,
        term: &mut VimTerm,
        mut done: impl FnMut(&VimTerm, &VimEvent) -> bool,
    ) {
        let ok = tokio::time::timeout(Duration::from_secs(10), async {
            while let Some(ev) = rx.recv().await {
                if let VimEvent::Output { data, .. } = &ev {
                    term.process(data);
                }
                if done(term, &ev) {
                    return;
                }
            }
            panic!("event channel closed early");
        })
        .await;
        assert!(
            ok.is_ok(),
            "timed out; screen:\n{}",
            term.parser.screen().contents()
        );
    }

    #[tokio::test]
    async fn output_reaches_parser_and_kill_exits() {
        let dir = tempfile::tempdir().unwrap();
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        let (program, args) = PRINTER;
        let mut term = VimTerm::spawn_cmd(
            program,
            &args.map(String::from),
            dir.path(),
            "test".into(),
            80,
            24,
            1,
            tx,
        )
        .unwrap();

        recv_until(&mut rx, &mut term, |t, _| {
            t.parser.screen().contents().contains("VIM_MODAL_TEST")
        })
        .await;

        term.kill();
        recv_until(&mut rx, &mut term, |_, ev| {
            matches!(ev, VimEvent::Exited { generation: 1 })
        })
        .await;
    }

    #[tokio::test]
    async fn input_reaches_the_child() {
        let dir = tempfile::tempdir().unwrap();
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        let (program, args) = ECHO_ONE_LINE;
        let mut term = VimTerm::spawn_cmd(
            program,
            &args.map(String::from),
            dir.path(),
            "test".into(),
            80,
            24,
            7,
            tx,
        )
        .unwrap();

        term.input(b"hello\r");
        recv_until(&mut rx, &mut term, |t, _| {
            t.parser.screen().contents().contains("GOT:hello")
        })
        .await;
        // The script ends after one line: the exit must be reported with the
        // spawn's generation stamp.
        recv_until(&mut rx, &mut term, |_, ev| {
            matches!(ev, VimEvent::Exited { generation: 7 })
        })
        .await;
    }

    #[tokio::test]
    async fn spawn_failure_is_a_message_not_a_panic() {
        let dir = tempfile::tempdir().unwrap();
        let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
        let err = VimTerm::spawn_cmd(
            "/nonexistent-editor-binary",
            &[],
            dir.path(),
            "test".into(),
            80,
            24,
            1,
            tx,
        )
        .map(|_| ())
        .unwrap_err();
        assert!(err.contains("failed to launch"), "{err}");
    }
}
