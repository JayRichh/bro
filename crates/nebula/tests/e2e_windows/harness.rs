//! The Windows TUI harness: `kete` in a ConPTY of its own, driven with
//! literal key bytes (ConPTY turns them into the console key records the
//! TUI reads) and read back through vt100.
//!
//! Every harness owns a temp root: the runtime and data dirs (never the
//! user's), a home, the repos, and the daemon the TUI starts there.

use super::agent;
use portable_pty::{native_pty_system, CommandBuilder, PtySize};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

pub const COLS: u16 = 160;
pub const ROWS: u16 = 45;
/// How long one screen state may take to come up: generous, as a cold
/// CI runner starts a daemon, git worktrees and agents.
pub const WAIT: Duration = Duration::from_secs(30);
const POLL_STEP: Duration = Duration::from_millis(50);

pub const ENTER: &[u8] = b"\r";
pub const ESC: &[u8] = &[0x1b];

type Writer = Arc<Mutex<Box<dyn Write + Send>>>;

pub struct Tui {
    writer: Writer,
    parser: Arc<Mutex<vt100::Parser>>,
    child: Box<dyn portable_pty::Child + Send + Sync>,
    pub runtime_dir: PathBuf,
    pub data_dir: PathBuf,
    pub repos: PathBuf,
    pub agent_log: PathBuf,
    pub echo_log: PathBuf,
    _root: tempfile::TempDir,
}

pub struct Options {
    /// The fake agent's script (see `agent`).
    pub script: String,
    /// Directories dropped from PATH, for a TUI that must not find a tool.
    pub strip_path: Vec<String>,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            script: "start;stop;echo".into(),
            strip_path: Vec::new(),
        }
    }
}

impl Tui {
    pub fn spawn(options: Options) -> Self {
        let root = tempfile::Builder::new().prefix("kete-e2e-").tempdir().unwrap();
        let base = root.path().canonicalize().unwrap();
        let base = PathBuf::from(
            base.display()
                .to_string()
                .trim_start_matches(r"\\?\")
                .to_string(),
        );
        let runtime_dir = base.join("rt");
        let data_dir = base.join("data");
        let home = base.join("home");
        let repos = base.join("repos");
        for dir in [&runtime_dir, &data_dir, &home, &repos] {
            std::fs::create_dir_all(dir).unwrap();
        }
        // The look the assertions read (the indexed default theme, no file
        // icons), and the fake agent as the `claude` harness's program:
        // the quick prompt launches Claude, and the daemon installs
        // Claude's hooks for it and passes it Claude's argv.
        let me = std::env::current_exe().unwrap();
        std::fs::write(
            data_dir.join(nebula_core::paths::CONFIG_FILE_NAME),
            serde_json::json!({
                "theme": "default",
                "icons": "off",
                "harnesses": { "claude": { "program": me.display().to_string() } },
            })
            .to_string(),
        )
        .unwrap();

        let pty = native_pty_system()
            .openpty(PtySize {
                rows: ROWS,
                cols: COLS,
                pixel_width: 0,
                pixel_height: 0,
            })
            .unwrap();
        let mut cmd = CommandBuilder::new(env!("CARGO_BIN_EXE_kete"));
        cmd.env(nebula_core::env::RUNTIME_DIR, &runtime_dir);
        cmd.env(nebula_core::env::DATA_DIR, &data_dir);
        cmd.env(nebula_core::env::WORKTREE_SYNC_MS, "100");
        cmd.env(nebula_core::env::UPDATE_CHECK_SECS, "0");
        cmd.env(nebula_core::env::LOG, "debug");
        cmd.env("HOME", &home);
        cmd.env("GIT_TERMINAL_PROMPT", "0");
        cmd.env("GCM_INTERACTIVE", "never");
        cmd.env_remove("NO_COLOR");
        cmd.env_remove("FORCE_COLOR");
        for var in nebula_core::env::AGENT_SESSION_VARS {
            cmd.env_remove(var);
        }
        let agent_log = base.join("agent.log");
        let echo_log = base.join("echo.log");
        cmd.env(agent::ROLE, "1");
        cmd.env(agent::SCRIPT, &options.script);
        cmd.env(agent::LOG, &agent_log);
        cmd.env(agent::ECHO_LOG, &echo_log);
        if !options.strip_path.is_empty() {
            let path = std::env::var("PATH").unwrap_or_default();
            let kept: Vec<&str> = path
                .split(';')
                .filter(|dir| {
                    !options
                        .strip_path
                        .iter()
                        .any(|gone| dir.trim_end_matches('\\').eq_ignore_ascii_case(gone))
                })
                .collect();
            cmd.env("PATH", kept.join(";"));
        }
        cmd.cwd(&repos);
        let child = pty.slave.spawn_command(cmd).unwrap();
        drop(pty.slave);

        let mut reader = pty.master.try_clone_reader().unwrap();
        let writer: Writer = Arc::new(Mutex::new(pty.master.take_writer().unwrap()));
        // Dropping the master closes the pseudoconsole under the TUI.
        std::mem::forget(pty.master);

        let parser = Arc::new(Mutex::new(vt100::Parser::new(ROWS, COLS, 0)));
        {
            let parser = parser.clone();
            let writer = writer.clone();
            std::thread::spawn(move || {
                let mut buf = [0u8; 16384];
                let mut tail: Vec<u8> = Vec::new();
                while let Ok(n) = reader.read(&mut buf) {
                    if n == 0 {
                        break;
                    }
                    let mut parser = parser.lock().unwrap();
                    parser.process(&buf[..n]);
                    // ConPTY asks where the cursor is before it draws a
                    // thing (and the TUI may ask again): answer as a
                    // terminal would, or it waits.
                    tail.extend_from_slice(&buf[..n]);
                    let asks = tail.windows(4).filter(|w| w == b"\x1b[6n").count();
                    if asks > 0 {
                        let (row, col) = parser.screen().cursor_position();
                        let reply = format!("\x1b[{};{}R", row + 1, col + 1);
                        let mut w = writer.lock().unwrap();
                        for _ in 0..asks {
                            let _ = w.write_all(reply.as_bytes());
                        }
                        let _ = w.flush();
                        tail.clear();
                    } else if tail.len() > 3 {
                        tail.drain(..tail.len() - 3);
                    }
                }
            });
        }

        Self {
            writer,
            parser,
            child,
            runtime_dir,
            data_dir,
            repos,
            agent_log,
            echo_log,
            _root: root,
        }
    }

    /// A committed repo named `name` with a bare repo beside it as its
    /// `origin` (its `main` pushed), so the base is `origin/main`.
    pub fn make_repo(&self, name: &str) -> PathBuf {
        let repo = self.repos.join(name);
        let bare = self.repos.join(format!("{name}-origin.git"));
        std::fs::create_dir_all(&repo).unwrap();
        git(&self.repos, &["init", "-q", "--bare", "-b", "main", &bare.display().to_string()]);
        git(&repo, &["init", "-q", "-b", "main"]);
        git(&repo, &["config", "user.email", "e2e@kete.dev"]);
        git(&repo, &["config", "user.name", "kete-e2e"]);
        git(&repo, &["config", "core.autocrlf", "false"]);
        std::fs::write(repo.join("README.md"), "e2e\n").unwrap();
        git(&repo, &["add", "."]);
        git(&repo, &["commit", "-q", "-m", "init"]);
        git(&repo, &["remote", "add", "origin", &bare.display().to_string()]);
        git(&repo, &["push", "-q", "-u", "origin", "main"]);
        git(&repo, &["remote", "set-head", "origin", "main"]);
        repo
    }

    pub fn send(&self, bytes: &[u8]) {
        let mut w = self.writer.lock().unwrap();
        w.write_all(bytes).unwrap();
        w.flush().unwrap();
    }

    pub fn type_str(&self, s: &str) {
        self.send(s.as_bytes());
    }

    pub fn screen_text(&self) -> String {
        screen_to_text(self.parser.lock().unwrap().screen())
    }

    pub fn wait_for(&self, what: &str, pred: impl Fn(&vt100::Screen) -> bool) {
        let deadline = Instant::now() + WAIT;
        loop {
            if pred(self.parser.lock().unwrap().screen()) {
                return;
            }
            if Instant::now() > deadline {
                panic!(
                    "timed out waiting for: {what}\n--- screen ---\n{}\n--- tui.log tail ---\n{}\n--- agent.log ---\n{}",
                    self.screen_text(),
                    tail(&self.data_dir.join("state").join("tui.log"), 40),
                    tail(&self.agent_log, 40),
                );
            }
            std::thread::sleep(POLL_STEP);
        }
    }

    pub fn wait_for_text(&self, needle: &str) {
        self.wait_for(&format!("text {needle:?}"), |s| {
            screen_to_text(s).contains(needle)
        });
    }

    pub fn wait_for_gone(&self, needle: &str) {
        self.wait_for(&format!("text {needle:?} to disappear"), |s| {
            !screen_to_text(s).contains(needle)
        });
    }

    pub fn try_wait_for_text(&self, needle: &str, window: Duration) -> bool {
        let deadline = Instant::now() + window;
        loop {
            if self.screen_text().contains(needle) {
                return true;
            }
            if Instant::now() > deadline {
                return false;
            }
            std::thread::sleep(POLL_STEP);
        }
    }

    /// Wait until the agent's log has a line containing `needle`.
    pub fn wait_for_agent(&self, needle: &str) {
        self.wait_for(&format!("agent log {needle:?}"), |_| {
            std::fs::read_to_string(&self.agent_log).is_ok_and(|t| t.contains(needle))
        });
    }

    /// Where `needle` first appears: (row, col) in cells.
    pub fn find(&self, needle: &str) -> Option<(u16, u16)> {
        screen_to_text(self.parser.lock().unwrap().screen())
            .lines()
            .enumerate()
            .find_map(|(row, line)| {
                let at = line.find(needle)?;
                Some((row as u16, line[..at].chars().count() as u16))
            })
    }

    /// A left click on the first place `needle` shows.
    pub fn click(&self, needle: &str) {
        let (row, col) = self
            .find(needle)
            .unwrap_or_else(|| panic!("no {needle:?} to click:\n{}", self.screen_text()));
        self.send(&sgr_mouse(0, col, row, false));
        self.send(&sgr_mouse(0, col, row, true));
    }

    /// Open the project at `path` with `o`, and wait for its name.
    pub fn add_project(&self, path: &Path, name: &str) {
        self.send(b"o");
        self.wait_for_text("Open project");
        self.type_str(&path.display().to_string());
        self.send(ENTER);
        self.wait_for_gone("Open project");
        self.wait_for_text(name);
    }
}

impl Drop for Tui {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = std::process::Command::new(env!("CARGO_BIN_EXE_kete"))
            .arg("kill")
            .env(nebula_core::env::RUNTIME_DIR, &self.runtime_dir)
            .env(nebula_core::env::DATA_DIR, &self.data_dir)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status();
        // The daemon and its agents let go of the dirs a moment after.
        std::thread::sleep(Duration::from_millis(300));
    }
}

pub fn screen_to_text(screen: &vt100::Screen) -> String {
    let (rows, cols) = screen.size();
    let mut out = String::new();
    for row in 0..rows {
        for col in 0..cols {
            match screen.cell(row, col) {
                Some(cell) if !cell.contents().is_empty() => out.push_str(cell.contents()),
                Some(cell) if cell.is_wide_continuation() => {}
                _ => out.push(' '),
            }
        }
        out.push('\n');
    }
    out
}

/// An SGR mouse report at 0-based `col`,`row`.
pub fn sgr_mouse(button: u16, col: u16, row: u16, release: bool) -> Vec<u8> {
    format!(
        "\x1b[<{button};{};{}{}",
        col + 1,
        row + 1,
        if release { 'm' } else { 'M' }
    )
    .into_bytes()
}

pub fn git(cwd: &Path, args: &[&str]) -> String {
    let out = std::process::Command::new("git")
        .arg("-C")
        .arg(cwd)
        .args(args)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "git {args:?} failed in {}: {}",
        cwd.display(),
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).into_owned()
}

fn tail(path: &Path, lines: usize) -> String {
    let text = std::fs::read_to_string(path).unwrap_or_default();
    let all: Vec<&str> = text.lines().collect();
    all[all.len().saturating_sub(lines)..].join("\n")
}
