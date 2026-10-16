//! `kete-w`: kete with no window, the way `pythonw` is python with none.
//!
//! `kete.exe` is a console program, so a Windows scheduled task that runs
//! it opens a console window for as long as it runs — up to a couple of
//! minutes for a WHAKAOHO wake-up, and it may take the focus from whatever
//! the user is typing in. `kete-w.exe` is built for the Windows subsystem:
//! it never gets a console, starts the `kete.exe` beside it with the same
//! arguments and no window (`CREATE_NO_WINDOW`), waits, and exits with its
//! code. The scheduled task runs `kete-w.exe wake --all`
//! (`nebula_core::wake::task::program_for`). Elsewhere it is a plain
//! passthrough to the `kete` beside it.

// The tests run as a console program, so their output still shows.
#![cfg_attr(all(windows, not(test)), windows_subsystem = "windows")]

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Command;

/// What `kete-w` exits with when it could not start `kete` at all.
const NOT_STARTED: i32 = 127;

/// The `kete` beside the launcher at `launcher`.
fn sibling(launcher: &Path) -> PathBuf {
    launcher.with_file_name(if cfg!(windows) { "kete.exe" } else { "kete" })
}

/// Start `kete` beside `launcher` with `args` through `spawn` (which runs
/// it and waits) and say what to exit with: its code; 1 when it ended with
/// none (killed); [`NOT_STARTED`] when it could not be started.
fn run(
    launcher: &Path,
    args: &[OsString],
    spawn: impl FnOnce(&Path, &[OsString]) -> std::io::Result<Option<i32>>,
) -> i32 {
    match spawn(&sibling(launcher), args) {
        Ok(code) => code.unwrap_or(1),
        Err(_) => NOT_STARTED,
    }
}

/// Run `exe args…` with nothing on screen and wait for its exit code.
fn spawn_hidden(exe: &Path, args: &[OsString]) -> std::io::Result<Option<i32>> {
    let mut command = Command::new(exe);
    command
        .args(args)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // No console window for the console program it starts.
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    command.status().map(|status| status.code())
}

fn main() {
    let launcher = std::env::current_exe().unwrap_or_else(|_| PathBuf::from("kete-w"));
    let args: Vec<OsString> = std::env::args_os().skip(1).collect();
    std::process::exit(run(&launcher, &args, spawn_hidden));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_runs_the_kete_beside_it_with_the_same_arguments() {
        let launcher = Path::new(if cfg!(windows) {
            r"C:\kete\bin\kete-w.exe"
        } else {
            "/opt/kete/bin/kete-w"
        });
        let args: Vec<OsString> = ["wake", "--all"].iter().map(OsString::from).collect();
        let mut seen = None;
        let code = run(launcher, &args, |exe, got| {
            seen = Some((exe.to_path_buf(), got.to_vec()));
            Ok(Some(0))
        });
        assert_eq!(code, 0);
        let (exe, got) = seen.unwrap();
        assert_eq!(exe, sibling(launcher));
        assert_eq!(
            exe.file_name().unwrap(),
            if cfg!(windows) { "kete.exe" } else { "kete" }
        );
        assert_eq!(exe.parent(), launcher.parent());
        assert_eq!(got, args);
    }

    #[test]
    fn it_exits_with_kete_s_code() {
        let launcher = Path::new("kete-w");
        assert_eq!(run(launcher, &[], |_, _| Ok(Some(3))), 3);
        assert_eq!(run(launcher, &[], |_, _| Ok(None)), 1, "killed: no code");
        assert_eq!(
            run(launcher, &[], |_, _| Err(std::io::Error::other("missing"))),
            NOT_STARTED
        );
    }

    #[test]
    fn a_real_child_s_exit_code_comes_back() {
        // Any program that exits with a known code stands in for kete.
        let (program, args): (&str, &[&str]) = if cfg!(windows) {
            ("cmd", &["/D", "/C", "exit 7"])
        } else {
            ("sh", &["-c", "exit 7"])
        };
        let args: Vec<OsString> = args.iter().map(OsString::from).collect();
        assert_eq!(spawn_hidden(Path::new(program), &args).unwrap(), Some(7));
    }
}
