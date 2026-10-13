//! `kete desktop`: launch the desktop window (`kete-desktop`), a separate
//! binary built from `desktop/` in this repository. It is looked for beside
//! this executable first, so a build that ships both finds its own, then (on
//! Windows) where the desktop installer puts it, then on PATH. The window
//! outlives this command: it is started and left running, like any GUI
//! launched from a shell.

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

#[cfg(windows)]
const DESKTOP_EXE: &str = "kete-desktop.exe";
#[cfg(not(windows))]
const DESKTOP_EXE: &str = "kete-desktop";

/// Shown when no `kete-desktop` is installed.
#[cfg(windows)]
const NOT_FOUND: &str = "\
kete: the desktop window isn't installed.

`kete desktop` runs kete-desktop.exe, which it looks for next to this kete,
then in %LOCALAPPDATA%\\Programs\\Kete (where the desktop installer puts it),
then on PATH. Run the Kete installer again to get it, or build it from a
checkout of the repository:

  cd desktop
  npm ci
  npm run tauri build

then copy desktop\\target\\release\\kete-desktop.exe next to kete, or onto PATH.";

#[cfg(not(windows))]
const NOT_FOUND: &str = "\
kete: the desktop window isn't installed.

`kete desktop` runs `kete-desktop`, which it looks for next to this kete and
then on PATH. Build it from a checkout of the repository:

  cd desktop
  npm ci
  npm run tauri build -- --no-bundle

then copy desktop/target/release/kete-desktop next to kete, or onto PATH.";

/// Where the per-user NSIS installer puts the desktop window on Windows:
/// `%LOCALAPPDATA%\Programs\Kete`. Never `%LOCALAPPDATA%\Kete`, which is
/// Kete's own data root.
#[cfg(windows)]
fn installed_dir() -> Option<PathBuf> {
    std::env::var_os("LOCALAPPDATA").map(|d| PathBuf::from(d).join("Programs").join("Kete"))
}

#[cfg(not(windows))]
fn installed_dir() -> Option<PathBuf> {
    None
}

/// Where `kete-desktop` lives: the first of `dirs` that has it (beside this
/// kete, then the installer's dir), else the first match on `path`.
fn find_in(dirs: &[PathBuf], path: Option<&std::ffi::OsStr>) -> Option<PathBuf> {
    if let Some(found) = dirs
        .iter()
        .map(|d| d.join(DESKTOP_EXE))
        .find(|p| p.is_file())
    {
        return Some(found);
    }
    std::env::split_paths(path?)
        .map(|d| d.join(DESKTOP_EXE))
        .find(|p| p.is_file())
}

pub fn run_desktop(args: &[String]) -> Result<()> {
    let exe = std::env::current_exe().ok();
    let dirs: Vec<PathBuf> = exe
        .as_deref()
        .and_then(Path::parent)
        .map(Path::to_path_buf)
        .into_iter()
        .chain(installed_dir())
        .collect();
    let found = find_in(&dirs, std::env::var_os("PATH").as_deref());
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
    fn the_copy_beside_kete_wins_over_path() {
        let beside = tempfile::tempdir().unwrap();
        let on_path = tempfile::tempdir().unwrap();
        std::fs::write(beside.path().join(DESKTOP_EXE), b"").unwrap();
        std::fs::write(on_path.path().join(DESKTOP_EXE), b"").unwrap();
        let path = std::env::join_paths([on_path.path()]).unwrap();
        assert_eq!(
            find_in(&[beside.path().to_path_buf()], Some(&path)),
            Some(beside.path().join(DESKTOP_EXE))
        );
        let empty = tempfile::tempdir().unwrap();
        assert_eq!(
            find_in(&[empty.path().to_path_buf()], Some(&path)),
            Some(on_path.path().join(DESKTOP_EXE))
        );
        assert_eq!(find_in(&[empty.path().to_path_buf()], None), None);
    }

    #[test]
    fn the_installer_dir_comes_after_beside_and_before_path() {
        let beside = tempfile::tempdir().unwrap();
        let installed = tempfile::tempdir().unwrap();
        let on_path = tempfile::tempdir().unwrap();
        std::fs::write(installed.path().join(DESKTOP_EXE), b"").unwrap();
        std::fs::write(on_path.path().join(DESKTOP_EXE), b"").unwrap();
        let path = std::env::join_paths([on_path.path()]).unwrap();
        let dirs = [beside.path().to_path_buf(), installed.path().to_path_buf()];
        assert_eq!(
            find_in(&dirs, Some(&path)),
            Some(installed.path().join(DESKTOP_EXE))
        );
        std::fs::write(beside.path().join(DESKTOP_EXE), b"").unwrap();
        assert_eq!(
            find_in(&dirs, Some(&path)),
            Some(beside.path().join(DESKTOP_EXE))
        );
    }

    #[cfg(windows)]
    #[test]
    fn the_installer_dir_is_under_programs_not_the_data_root() {
        let dir = installed_dir().expect("LOCALAPPDATA is set on Windows");
        assert!(dir.ends_with(Path::new("Programs").join("Kete")));
    }
}
