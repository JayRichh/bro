//! Starting an editor on something: the argv each family takes, the
//! environment it must not inherit from Kete, and the detached spawn.
//!
//! The editor's own exe is started directly, never through `code.cmd` or a
//! shell: no console flashes, no `%` / `"` rules from `shell.rs`, and none
//! of `code.cmd`'s `ELECTRON_RUN_AS_NODE`. VS Code itself then decides the
//! window: a folder already open in one is focused there, so "focus" is a
//! relaunch.

use super::{Editor, Family};
use std::ffi::{OsStr, OsString};
use std::path::Path;
use std::process::Command;

/// What to open.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Open<'a> {
    /// A checkout or a folder.
    Folder(&'a Path),
    /// A file, at a line (and column) when given. `folder`: the checkout
    /// it is in, so VS Code puts it in that checkout's window rather than
    /// whichever window was last active.
    File {
        folder: Option<&'a Path>,
        file: &'a Path,
        line: Option<u32>,
        column: Option<u32>,
    },
    /// A Visual Studio solution or project.
    Solution(&'a Path),
}

/// How the user wants editors opened.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Options {
    /// VS Code's `-r`: a folder replaces what the last active window
    /// shows instead of opening a window of its own.
    pub reuse_window: bool,
}

/// The arguments `editor`'s exe takes to open `what`. Err when the
/// editor can't open that kind of thing: VS Code a solution file on its
/// own, Visual Studio a folder.
pub fn argv(editor: &Editor, what: &Open, opts: &Options) -> Result<Vec<OsString>, String> {
    let mut out: Vec<OsString> = Vec::new();
    match (editor.family, *what) {
        (Family::Code, Open::Folder(folder)) => {
            if opts.reuse_window {
                out.push("-r".into());
            }
            out.push(folder.into());
        }
        (
            Family::Code,
            Open::File {
                folder,
                file,
                line,
                column,
            },
        ) => {
            if opts.reuse_window {
                out.push("-r".into());
            }
            if let Some(folder) = folder {
                out.push(folder.into());
            }
            match line {
                Some(line) => {
                    let mut at = file.as_os_str().to_owned();
                    at.push(format!(":{}", line.max(1)));
                    if let Some(col) = column {
                        at.push(format!(":{}", col.max(1)));
                    }
                    out.push("-g".into());
                    out.push(at);
                }
                None => out.push(file.into()),
            }
        }
        (Family::Code, Open::Solution(_)) => {
            return Err(format!("{} opens a folder, not a solution", editor.short))
        }
        (Family::VisualStudio, Open::Solution(sln)) => out.push(sln.into()),
        // `/Edit` sends the file to the most recent Visual Studio, which
        // focuses; a line needs DTE over COM, later.
        (Family::VisualStudio, Open::File { file, .. }) => {
            out.push("/Edit".into());
            out.push(file.into());
        }
        (Family::VisualStudio, Open::Folder(_)) => {
            return Err(
                "Visual Studio opens a solution or a project, and this folder has none".into(),
            )
        }
    }
    Ok(out)
}

/// Whether a variable is Kete's own and stays out of a launched editor:
/// a cold-started VS Code hands its environment to every terminal it
/// opens, so a Kete variable here would follow the user into their own
/// shells. `profiles_root` is Kete's PROFILE HOME folder: a profile's
/// config-dir variable (`CLAUDE_CONFIG_DIR`, `CODEX_HOME`, `COPILOT_HOME`,
/// `GH_CONFIG_DIR`) is stripped when it points there, and kept when the
/// user set it to a folder of their own.
pub fn strips(name: &OsStr, value: &OsStr, profiles_root: &Path) -> bool {
    let Some(name) = name.to_str() else {
        return false;
    };
    let upper = name.to_ascii_uppercase();
    // Kete's own (and its old names'), Electron's (`ELECTRON_RUN_AS_NODE`
    // makes Code.exe a bare Node), and a parent VS Code's terminal's
    // (`VSCODE_IPC_HOOK_CLI`, the git askpass handles): an editor started
    // from Kete is the user's, not a child of whatever Kete ran in.
    const PREFIXES: &[&str] = &["NEBULA_", "KETE_", "BRO_", "ELECTRON_", "VSCODE_"];
    if PREFIXES.iter().any(|p| upper.starts_with(p)) {
        return true;
    }
    if crate::env::AGENT_SESSION_VARS.contains(&name)
        || crate::env::HOST_CLAUDE_SESSION_VARS.contains(&name)
    {
        return true;
    }
    let profile_dir = ["claude", "codex", "copilot"]
        .iter()
        .flat_map(|h| crate::profiles::home_dirs(h))
        .any(|d| d.var == name);
    profile_dir && under(Path::new(value), profiles_root)
}

/// `path` is `root` or inside it, compared the way the OS compares
/// paths (case-blind on Windows).
fn under(path: &Path, root: &Path) -> bool {
    if root.as_os_str().is_empty() {
        return false;
    }
    let norm = |p: &Path| {
        let s = p.to_string_lossy().replace('\\', "/");
        let s = s.trim_end_matches('/').to_string();
        if cfg!(windows) {
            s.to_ascii_lowercase()
        } else {
            s
        }
    };
    let (p, r) = (norm(path), norm(root));
    p == r || p.starts_with(&format!("{r}/"))
}

/// The names in `vars` a launched editor must not inherit ([`strips`]).
pub fn launch_env_removals(
    vars: impl IntoIterator<Item = (OsString, OsString)>,
    profiles_root: &Path,
) -> Vec<OsString> {
    vars.into_iter()
        .filter(|(n, v)| strips(n, v, profiles_root))
        .map(|(n, _)| n)
        .collect()
}

/// The command that opens `what` in `editor`, built but not started:
/// the exe, its argv, the stripped environment (`vars`, this process's,
/// less [`strips`]) and a working folder. What [`launch`] spawns; the
/// tests read it back.
pub fn command(
    editor: &Editor,
    what: &Open,
    opts: &Options,
    vars: impl IntoIterator<Item = (OsString, OsString)>,
    profiles_root: &Path,
) -> Result<Command, String> {
    let args = argv(editor, what, opts)?;
    let mut cmd = Command::new(&editor.exe);
    cmd.args(&args);
    for name in launch_env_removals(vars, profiles_root) {
        cmd.env_remove(name);
    }
    let cwd = match *what {
        Open::Folder(f) => Some(f),
        Open::File { folder, file, .. } => folder.or_else(|| file.parent()),
        Open::Solution(s) => s.parent(),
    };
    if let Some(cwd) = cwd.filter(|c| c.is_dir()) {
        cmd.current_dir(cwd);
    }
    cmd.stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    Ok(cmd)
}

/// Open `what` in `editor`: started detached, with nothing attached and
/// Kete's environment stripped, then let it take the foreground Kete has.
/// The editor's pid, or the line to show.
pub fn launch(editor: &Editor, what: &Open, opts: &Options) -> Result<u32, String> {
    if !editor.exe.as_os_str().is_empty() && editor.exe.is_absolute() && !editor.exe.is_file() {
        return Err(format!(
            "{} is no longer at {}: re-scan the editors in Settings",
            editor.short,
            editor.exe.display()
        ));
    }
    let mut cmd = command(
        editor,
        what,
        opts,
        std::env::vars_os(),
        &crate::profiles::profiles_root(),
    )?;
    let child =
        spawn_detached(&mut cmd).map_err(|e| format!("Could not start {}: {e}", editor.short))?;
    let pid = child.id();
    super::allow_foreground(pid);
    reap(child);
    Ok(pid)
}

/// Start `cmd` out of Kete's console, process group and (where the job
/// lets it) job, as the desktop starts the daemon: closing Kete never
/// takes the editor with it.
#[cfg(windows)]
fn spawn_detached(cmd: &mut Command) -> std::io::Result<std::process::Child> {
    use std::os::windows::process::CommandExt;
    use windows_sys::Win32::System::Threading::{
        CREATE_BREAKAWAY_FROM_JOB, CREATE_NEW_PROCESS_GROUP, DETACHED_PROCESS,
    };
    let flags = CREATE_NEW_PROCESS_GROUP | DETACHED_PROCESS;
    cmd.creation_flags(flags | CREATE_BREAKAWAY_FROM_JOB);
    cmd.spawn().or_else(|_| {
        cmd.creation_flags(flags);
        cmd.spawn()
    })
}

#[cfg(unix)]
fn spawn_detached(cmd: &mut Command) -> std::io::Result<std::process::Child> {
    use std::os::unix::process::CommandExt;
    cmd.process_group(0).spawn()
}

/// Wait on the child off-thread so it never lingers as a zombie (unix);
/// on Windows dropping it closes the handle and the editor runs on.
fn reap(child: std::process::Child) {
    #[cfg(unix)]
    {
        let mut child = child;
        std::thread::spawn(move || {
            let _ = child.wait();
        });
    }
    #[cfg(not(unix))]
    drop(child);
}
