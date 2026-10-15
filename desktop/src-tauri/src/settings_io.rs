//! Settings export and import from the Settings view: the window's way to
//! `kete config export` and `kete config import`, run through the `kete`
//! binary beside it (`daemon::find_kete`) so the bundle is written and
//! merged by the very code the CLI and the TUI use. The file is picked in
//! the system's save and open dialogs; an import is read first with
//! `--dry-run`, which writes nothing, so the window can say what would
//! change and ask before the real one.

use crate::Res;
use serde::Serialize;
use std::path::{Path, PathBuf};
use tauri_plugin_dialog::DialogExt;

/// What `kete config export` names the file a folder gets.
const FILE_NAME: &str = "nebula-settings.json";

/// What an import would change, for the window's confirm.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ImportPreview {
    pub path: String,
    /// `kete config import --dry-run`'s lines: the summary first.
    pub lines: Vec<String>,
    /// Whether anything would change at all.
    pub changes: bool,
}

/// `kete config <args…>`: its stdout, else the last line it wrote to
/// stderr. Blocking.
fn run_config(args: &[&std::ffi::OsStr]) -> Res<String> {
    let kete = crate::daemon::find_kete()
        .ok_or("The kete program was not found beside this window or on PATH.")?;
    run_with(&kete, args)
}

fn run_with(kete: &Path, args: &[&std::ffi::OsStr]) -> Res<String> {
    let mut cmd = std::process::Command::new(kete);
    cmd.arg("config")
        .args(args)
        .stdin(std::process::Stdio::null());
    #[cfg(windows)]
    nebula_core::platform::no_window(&mut cmd);
    let out = cmd
        .output()
        .map_err(|e| format!("{}: {e}", kete.display()))?;
    if out.status.success() {
        return Ok(String::from_utf8_lossy(&out.stdout).trim().to_string());
    }
    Err(last_line(&String::from_utf8_lossy(&out.stderr))
        .unwrap_or("kete config failed")
        .trim_start_matches("Error: ")
        .to_string())
}

fn last_line(text: &str) -> Option<&str> {
    text.lines().rev().map(str::trim).find(|l| !l.is_empty())
}

/// The dry run's lines as the confirm shows them; nothing changes when
/// its summary says so.
pub fn preview_of(path: &str, stdout: &str) -> ImportPreview {
    let lines: Vec<String> = stdout
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(str::to_string)
        .collect();
    let changes = lines
        .first()
        .is_some_and(|l| !l.starts_with("nothing changed"));
    ImportPreview {
        path: path.to_string(),
        lines,
        changes,
    }
}

/// Where the dialogs start: the data dir, where the TUI's export goes.
fn start_dir() -> PathBuf {
    nebula_core::paths::data_dir()
}

/// Pick a file in the save dialog and write the settings to it; None when
/// the dialog was cancelled, else what `kete config export` said.
#[tauri::command]
pub async fn settings_export(app: tauri::AppHandle) -> Res<Option<String>> {
    tauri::async_runtime::spawn_blocking(move || {
        let Some(picked) = app
            .dialog()
            .file()
            .set_title("Export kete settings")
            .set_directory(start_dir())
            .set_file_name(FILE_NAME)
            .add_filter("JSON", &["json"])
            .blocking_save_file()
        else {
            return Ok(None);
        };
        let path = picked.into_path().map_err(|e| e.to_string())?;
        run_config(&["export".as_ref(), path.as_os_str()]).map(Some)
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Pick a settings file in the open dialog and read what importing it
/// would change, nothing written; None when the dialog was cancelled.
#[tauri::command]
pub async fn settings_import_preview(app: tauri::AppHandle) -> Res<Option<ImportPreview>> {
    tauri::async_runtime::spawn_blocking(move || {
        let Some(picked) = app
            .dialog()
            .file()
            .set_title("Import kete settings")
            .set_directory(start_dir())
            .add_filter("JSON", &["json"])
            .blocking_pick_file()
        else {
            return Ok(None);
        };
        let path = picked.into_path().map_err(|e| e.to_string())?;
        let said = run_config(&["import".as_ref(), "--dry-run".as_ref(), path.as_os_str()])?;
        Ok(Some(preview_of(&path.display().to_string(), &said)))
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Import the file a preview read: `kete config import`'s summary.
#[tauri::command]
pub async fn settings_import(path: String) -> Res<String> {
    tauri::async_runtime::spawn_blocking(move || {
        run_config(&["import".as_ref(), std::ffi::OsStr::new(&path)])
    })
    .await
    .map_err(|e| e.to_string())?
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_dry_run_reads_as_the_confirms_lines() {
        let p = preview_of(
            "/x/nebula-settings.json",
            "config: 2 keys changed · presets: 1 added, 0 replaced\nconfig.json: theme, editor\npresets: review\n",
        );
        assert!(p.changes);
        assert_eq!(p.lines.len(), 3);
        assert_eq!(p.lines[1], "config.json: theme, editor");
        let none = preview_of("/x", "nothing changed\n");
        assert!(!none.changes);
        assert!(!preview_of("/x", "").changes);
    }

    #[test]
    fn a_missing_program_says_so() {
        let dir = tempfile::tempdir().unwrap();
        let err = run_with(&dir.path().join("no-kete"), &["export".as_ref()]).unwrap_err();
        assert!(err.contains("no-kete"), "{err}");
        assert_eq!(last_line("a\n\nError: b\n  \n"), Some("Error: b"));
    }
}
