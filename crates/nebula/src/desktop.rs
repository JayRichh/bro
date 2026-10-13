//! `bro desktop`: launch the desktop window (`bro-desktop`), a separate
//! binary built from `desktop/` in this repository. It is looked for beside
//! this executable first, so an install that ships both finds its own,
//! then on PATH. The window outlives this command: it is started and left
//! running, like any GUI launched from a shell.

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

#[cfg(windows)]
const DESKTOP_EXE: &str = "bro-desktop.exe";
#[cfg(not(windows))]
const DESKTOP_EXE: &str = "bro-desktop";

/// Shown when no `bro-desktop` is installed.
const NOT_FOUND: &str = "\
bro: the desktop window isn't installed.

`bro desktop` runs `bro-desktop`, which it looks for next to this bro and
then on PATH. Build it from a checkout of the repository:

  cd desktop
  npm ci
  npm run tauri build

then copy desktop/target/release/bro-desktop(.exe) next to bro, or onto PATH.";

/// Where `bro-desktop` lives: beside `exe`, else the first match on `path`.
fn find_in(exe_dir: Option<&Path>, path: Option<&std::ffi::OsStr>) -> Option<PathBuf> {
    if let Some(dir) = exe_dir {
        let beside = dir.join(DESKTOP_EXE);
        if beside.is_file() {
            return Some(beside);
        }
    }
    std::env::split_paths(path?)
        .map(|d| d.join(DESKTOP_EXE))
        .find(|p| p.is_file())
}

pub fn run_desktop(args: &[String]) -> Result<()> {
    let exe = std::env::current_exe().ok();
    let found = find_in(
        exe.as_deref().and_then(Path::parent),
        std::env::var_os("PATH").as_deref(),
    );
    let Some(app) = found else {
        println!("{NOT_FOUND}");
        return Ok(());
    };
    let mut cmd = Command::new(&app);
    cmd.args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    #[cfg(windows)]
    nebula_core::platform::no_window(&mut cmd);
    cmd.spawn()
        .with_context(|| format!("start {}", app.display()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_copy_beside_bro_wins_over_path() {
        let beside = tempfile::tempdir().unwrap();
        let on_path = tempfile::tempdir().unwrap();
        std::fs::write(beside.path().join(DESKTOP_EXE), b"").unwrap();
        std::fs::write(on_path.path().join(DESKTOP_EXE), b"").unwrap();
        let path = std::env::join_paths([on_path.path()]).unwrap();
        assert_eq!(
            find_in(Some(beside.path()), Some(&path)),
            Some(beside.path().join(DESKTOP_EXE))
        );
        let empty = tempfile::tempdir().unwrap();
        assert_eq!(
            find_in(Some(empty.path()), Some(&path)),
            Some(on_path.path().join(DESKTOP_EXE))
        );
        assert_eq!(find_in(Some(empty.path()), None), None);
    }
}
