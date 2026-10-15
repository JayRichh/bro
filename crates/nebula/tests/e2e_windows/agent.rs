//! The fake agent: what the daemon launches as a session's CLI in this
//! suite. It reports what it does through the hooks the daemon installed
//! in the worktree's `.claude/settings.local.json` — the same
//! `kete _hook claude <event>` commands Claude Code runs, run the way it
//! runs them (a shell line, the JSON payload on stdin) — so Running, Needs
//! you and Finished come from the daemon's real hook path, not from
//! anything test-only.
//!
//! Like Claude it fires `SessionStart` as it boots, takes its prompt off
//! the trailing argument (a warm spare, booted with none, waits for one to
//! be typed), fires `UserPromptSubmit` and prints `working on: <prompt>`.
//! Then it runs its script: [`SCRIPT`]`_<FIRST WORD OF THE PROMPT>` when
//! that is set, else [`SCRIPT`]. Steps, `;`-separated:
//! - `gate:<name>` — waits until the suite creates `<GATES>/<name>`, so a
//!   state the suite has to see holds still until it has.
//! - `ask` — an `AskUserQuestion` (`PreToolUse`): prints the question and
//!   waits for a reply line, then its `PostToolUse`.
//! - `edit:<path>:<text>` — writes the file (relative to the worktree).
//! - `commit:<message>` — `git add -A` and `git commit`.
//! - `stop` — `Stop`: the turn is over.
//! - `say:<text>` — prints a line.
//! - `echo` — the idle prompt: turns bracketed paste on and logs every
//!   byte it is sent to [`ECHO_LOG`], printing `paste <n> bytes` for each
//!   bracketed paste and `submit` for each Enter outside one.
//!
//! After the script it stays up, as a CLI at its prompt does, until its
//! console goes away.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

/// Set (to anything) on the TUI, and so on its daemon and the sessions it
/// spawns: run as the fake agent, not the suite.
pub const ROLE: &str = "KETE_E2E_FAKE_AGENT";
/// The agent's script; see the module docs.
pub const SCRIPT: &str = "KETE_E2E_SCRIPT";
/// Where the agent logs each step and hook it ran, one line each.
pub const LOG: &str = "KETE_E2E_AGENT_LOG";
/// Where the `echo` step writes the raw bytes it is sent.
pub const ECHO_LOG: &str = "KETE_E2E_ECHO_LOG";
/// The directory `gate:` steps wait on.
pub const GATES: &str = "KETE_E2E_GATES";

/// The flags Kete's Claude spawn passes that take a value: everything
/// else that is not a flag is the prompt.
const VALUE_FLAGS: &[&str] = &[
    "--settings",
    "--model",
    "--effort",
    "--append-system-prompt",
    "--resume",
    "--session-id",
    "--permission-mode",
    "--mcp-config",
];

pub fn run() {
    raw_console();
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut agent = Agent {
        session_id: format!("fake-{}", std::process::id()),
        cwd: std::env::current_dir().unwrap_or_default(),
        input: std::io::stdin(),
    };
    agent.log(&format!("args {args:?}"));
    print_line("fake agent up");
    agent.hook("SessionStart", serde_json::json!({ "source": "startup" }));
    let prompt = match prompt_of(&args) {
        Some(prompt) => prompt,
        None => {
            print_line("> ");
            agent.read_line()
        }
    };
    agent.log(&format!("prompt {prompt:?}"));
    agent.hook("UserPromptSubmit", serde_json::json!({ "prompt": prompt }));
    print_line(&format!("working on: {prompt}"));
    let word = prompt
        .split_whitespace()
        .next()
        .unwrap_or_default()
        .to_ascii_uppercase();
    let script = std::env::var(format!("{SCRIPT}_{word}"))
        .or_else(|_| std::env::var(SCRIPT))
        .unwrap_or_else(|_| "stop".into());
    for step in script.split(';').map(str::trim).filter(|s| !s.is_empty()) {
        agent.log(&format!("step {step}"));
        let (verb, rest) = step.split_once(':').unwrap_or((step, ""));
        match verb {
            "gate" => {
                let gate = PathBuf::from(std::env::var_os(GATES).unwrap_or_default()).join(rest);
                while !gate.exists() {
                    std::thread::sleep(Duration::from_millis(50));
                }
            }
            "ask" => {
                let tool = serde_json::json!({
                    "tool_name": "AskUserQuestion",
                    "tool_input": { "questions": [{ "question": "Ship it?" }] },
                });
                agent.hook("PreToolUse", tool.clone());
                print_line("QUESTION: ship it? (reply and Enter)");
                let reply = agent.read_line();
                agent.log(&format!("reply {reply:?}"));
                print_line(&format!("got reply: {}", reply.trim()));
                agent.hook("PostToolUse", tool);
            }
            "edit" => {
                let (path, text) = rest.split_once(':').unwrap_or((rest, ""));
                let file = agent.cwd.join(path);
                if let Some(dir) = file.parent() {
                    let _ = std::fs::create_dir_all(dir);
                }
                std::fs::write(&file, format!("{text}\n")).unwrap();
                print_line(&format!("edited {path}"));
            }
            "commit" => {
                git(&agent.cwd, &["add", "-A"]);
                git(&agent.cwd, &["commit", "-q", "-m", rest]);
                agent.hook(
                    "PostToolUse",
                    serde_json::json!({ "tool_name": "Bash", "tool_input": { "command": "git commit" } }),
                );
                print_line(&format!("committed: {rest}"));
            }
            "stop" => {
                agent.hook("Stop", serde_json::json!({}));
                print_line("fake agent finished");
            }
            "say" => print_line(rest),
            "echo" => agent.echo(),
            other => agent.log(&format!("unknown step {other}")),
        }
    }
    let mut buf = [0u8; 1024];
    while matches!(agent.input.read(&mut buf), Ok(n) if n > 0) {}
}


/// The prompt: the last argument that is neither a flag nor a flag's
/// value.
fn prompt_of(args: &[String]) -> Option<String> {
    let mut prompt = None;
    let mut i = 0;
    while i < args.len() {
        let arg = &args[i];
        if VALUE_FLAGS.contains(&arg.as_str()) {
            i += 2;
            continue;
        }
        if !arg.starts_with("--") {
            prompt = Some(arg.clone());
        }
        i += 1;
    }
    prompt
}

struct Agent {
    session_id: String,
    cwd: PathBuf,
    input: std::io::Stdin,
}

impl Agent {
    fn log(&self, line: &str) {
        let Some(path) = std::env::var_os(LOG) else {
            return;
        };
        if let Ok(mut file) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
        {
            let _ = writeln!(file, "[{}] {line}", std::process::id());
        }
    }

    /// Run every command the installed settings file hooks `event` to,
    /// under cmd the way a CLI's hook runner would, with the payload on
    /// stdin.
    fn hook(&self, event: &str, extra: serde_json::Value) {
        let mut payload = serde_json::json!({
            "session_id": self.session_id,
            "hook_event_name": event,
            "cwd": self.cwd.display().to_string(),
        });
        if let (Some(into), Some(from)) = (payload.as_object_mut(), extra.as_object()) {
            for (k, v) in from {
                into.insert(k.clone(), v.clone());
            }
        }
        let commands = hook_commands(&self.cwd, event);
        if commands.is_empty() {
            self.log(&format!("hook {event}: no command installed"));
        }
        for command in commands {
            use std::os::windows::process::CommandExt;
            let child = Command::new("cmd")
                .raw_arg(format!("/D /C {command}"))
                .current_dir(&self.cwd)
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn();
            let Ok(mut child) = child else {
                self.log(&format!("hook {event}: spawn failed"));
                continue;
            };
            if let Some(mut stdin) = child.stdin.take() {
                let _ = stdin.write_all(payload.to_string().as_bytes());
            }
            let out = child.wait_with_output();
            self.log(&format!(
                "hook {event}: {:?}",
                out.as_ref().map(|o| o.status.code())
            ));
        }
    }

    /// One line of input: what is typed up to Enter, with a bracketed
    /// paste's own line breaks kept inside it.
    fn read_line(&mut self) -> String {
        let mut line = Vec::new();
        let mut in_paste = false;
        let mut buf = [0u8; 4096];
        loop {
            let n = match self.input.read(&mut buf) {
                Ok(0) | Err(_) => return String::from_utf8_lossy(&line).into_owned(),
                Ok(n) => n,
            };
            for &b in &buf[..n] {
                line.push(b);
                if line.ends_with(b"\x1b[200~") {
                    in_paste = true;
                } else if line.ends_with(b"\x1b[201~") {
                    in_paste = false;
                } else if (b == b'\r' || b == b'\n') && !in_paste {
                    line.pop();
                    return String::from_utf8_lossy(&line).into_owned();
                }
            }
        }
    }

    /// The idle prompt: log what arrives, and say on screen what it was.
    fn echo(&mut self) {
        let mut out = std::io::stdout();
        let _ = out.write_all(b"\x1b[?2004h");
        let _ = out.flush();
        print_line("echo ready");
        let log = std::env::var_os(ECHO_LOG).map(PathBuf::from);
        let mut seen: Vec<u8> = Vec::new();
        let mut paste_from: Option<usize> = None;
        let mut buf = [0u8; 8192];
        loop {
            let n = match self.input.read(&mut buf) {
                Ok(0) | Err(_) => return,
                Ok(n) => n,
            };
            if let Some(log) = &log {
                if let Ok(mut file) = std::fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(log)
                {
                    let _ = file.write_all(&buf[..n]);
                }
            }
            for &b in &buf[..n] {
                seen.push(b);
                if seen.ends_with(b"\x1b[200~") {
                    paste_from = Some(seen.len());
                } else if seen.ends_with(b"\x1b[201~") {
                    if let Some(from) = paste_from.take() {
                        let bytes = seen.len() - from - b"\x1b[201~".len();
                        print_line(&format!("paste {bytes} bytes"));
                    }
                } else if b == b'\r' && paste_from.is_none() {
                    print_line("submit");
                }
            }
        }
    }
}

/// The commands the worktree's Claude settings file hooks `event` to.
fn hook_commands(cwd: &Path, event: &str) -> Vec<String> {
    let path = cwd.join(".claude").join("settings.local.json");
    let Ok(text) = std::fs::read_to_string(path) else {
        return Vec::new();
    };
    let Ok(settings) = serde_json::from_str::<serde_json::Value>(&text) else {
        return Vec::new();
    };
    settings["hooks"][event]
        .as_array()
        .into_iter()
        .flatten()
        .flat_map(|group| group["hooks"].as_array().cloned().unwrap_or_default())
        .filter_map(|hook| hook["command"].as_str().map(str::to_string))
        .collect()
}

fn git(cwd: &Path, args: &[&str]) {
    let _ = Command::new("git")
        .arg("-C")
        .arg(cwd)
        .args(args)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
}

fn print_line(text: &str) {
    let mut out = std::io::stdout();
    let _ = write!(out, "{text}\r\n");
    let _ = out.flush();
}

/// Raw console input with VT sequences passed through (so a bracketed
/// paste's markers arrive as bytes), and VT output (so the bracketed-paste
/// switch reaches the terminal).
fn raw_console() {
    use windows_sys::Win32::System::Console::{
        GetStdHandle, SetConsoleMode, DISABLE_NEWLINE_AUTO_RETURN, ENABLE_PROCESSED_OUTPUT,
        ENABLE_VIRTUAL_TERMINAL_INPUT, ENABLE_VIRTUAL_TERMINAL_PROCESSING, STD_INPUT_HANDLE,
        STD_OUTPUT_HANDLE,
    };
    // SAFETY: the standard handles of this process, and plain mode flags.
    unsafe {
        SetConsoleMode(GetStdHandle(STD_INPUT_HANDLE), ENABLE_VIRTUAL_TERMINAL_INPUT);
        SetConsoleMode(
            GetStdHandle(STD_OUTPUT_HANDLE),
            ENABLE_PROCESSED_OUTPUT
                | ENABLE_VIRTUAL_TERMINAL_PROCESSING
                | DISABLE_NEWLINE_AUTO_RETURN,
        );
    }
}
