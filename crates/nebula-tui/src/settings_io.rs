//! SETTINGS EXPORT and IMPORT from the Settings overlay (`⇧E`, `⇧I`) and
//! the palette: `kete config export` / `kete config import` without
//! leaving the TUI, through the same `crate::bundle` code.
//!
//! Export asks for a path, prefilled with `nebula-settings.json` in the
//! data dir; a folder gets that file name inside it. Import asks for a
//! file or folder, reads it and runs the merge with nothing written
//! (`bundle::preview`), and puts up a confirm saying what would change;
//! only its yes writes (`bundle::import`, which reads and merges again
//! under the settings locks).

use crate::bundle::{self, Paths};
use std::path::{Path, PathBuf};

#[cfg(test)]
thread_local! {
    /// A test's data dir: the settings files and the default export live
    /// there instead of the real one.
    static TEST_DIR: std::cell::RefCell<Option<PathBuf>> = const { std::cell::RefCell::new(None) };
}

/// Run `f` with the settings files in `dir` (tests only).
#[cfg(test)]
pub fn with_dir<T>(dir: &Path, f: impl FnOnce() -> T) -> T {
    TEST_DIR.with(|d| *d.borrow_mut() = Some(dir.to_path_buf()));
    let out = f();
    TEST_DIR.with(|d| *d.borrow_mut() = None);
    out
}

fn data_dir() -> PathBuf {
    #[cfg(test)]
    if let Some(dir) = TEST_DIR.with(|d| d.borrow().clone()) {
        return dir;
    }
    nebula_core::paths::data_dir()
}

/// The settings files an export reads and an import merges into.
pub fn paths() -> Paths {
    #[cfg(test)]
    if let Some(dir) = TEST_DIR.with(|d| d.borrow().clone()) {
        return Paths {
            config: dir.join(nebula_core::paths::CONFIG_FILE_NAME),
            local: dir.join(nebula_core::paths::CONFIG_LOCAL_FILE_NAME),
            presets: dir.join(nebula_core::paths::PRESETS_FILE_NAME),
            hosts: dir.join(nebula_core::paths::SSH_HOSTS_FILE_NAME),
        };
    }
    Paths::current()
}

/// What both prompts start on: `nebula-settings.json` in the data dir.
pub fn default_file() -> String {
    data_dir().join(bundle::FILE_NAME).display().to_string()
}

/// `~/x` as the home's `x`.
fn expand(typed: &str) -> PathBuf {
    let typed = typed.trim();
    match (typed.strip_prefix("~/"), nebula_core::env::home_dir()) {
        (Some(rest), Some(home)) => home.join(rest),
        _ if typed == "~" => nebula_core::env::home_dir().unwrap_or_else(|| PathBuf::from("~")),
        _ => PathBuf::from(typed),
    }
}

/// Write the settings to `typed` (a folder gets [`bundle::FILE_NAME`]
/// inside it), as `kete config export <path>` does: the footer's line.
pub fn export_to(typed: &str) -> Result<String, String> {
    let dest = expand(typed);
    let file = if dest.is_dir() {
        dest.join(bundle::FILE_NAME)
    } else {
        dest
    };
    let (value, warnings) = bundle::export(&paths(), bundle::Scope::Backup);
    let mut text = serde_json::to_string_pretty(&value).map_err(|e| e.to_string())?;
    text.push('\n');
    if let Some(parent) = file.parent().filter(|p| !p.as_os_str().is_empty()) {
        std::fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
    }
    nebula_core::settings::write_atomic(&file, text.as_bytes())
        .map_err(|e| format!("writing {}: {e}", file.display()))?;
    let mut said = format!(
        "settings written to {} ({})",
        file.display(),
        match bundle::section_count(&value) {
            0 => "no settings files yet".to_string(),
            1 => "1 file".to_string(),
            n => format!("{n} files"),
        }
    );
    if let Some(w) = warnings.first() {
        said.push_str(&format!(" · {w}"));
    }
    Ok(said)
}

/// What importing `typed` would change, nothing written: the confirm's
/// lines, or why it can't be imported.
pub fn preview(typed: &str) -> Result<Vec<String>, String> {
    let source = expand(typed);
    let value = read(&source)?;
    let report = bundle::preview(&paths(), &value).map_err(|e| format!("{e:#}"))?;
    if !report.changed() {
        return Err(format!(
            "{} changes nothing here: {}",
            source.display(),
            report.summary()
        ));
    }
    Ok(report.details())
}

/// Import `typed` for real: the footer's line.
pub fn import(typed: &str) -> Result<String, String> {
    let value = read(&expand(typed))?;
    let report = bundle::import(&paths(), &value).map_err(|e| format!("{e:#}"))?;
    Ok(format!("settings imported: {}", report.summary()))
}

fn read(source: &Path) -> Result<serde_json::Value, String> {
    bundle::read_source(&source.display().to_string()).map_err(|e| format!("{e:#}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn an_export_lands_where_it_was_told_and_reads_back_as_an_import() {
        let home = tempfile::tempdir().unwrap();
        let other = tempfile::tempdir().unwrap();
        std::fs::write(
            home.path().join("config.json"),
            json!({"theme": "ocean", "editor": "hx"}).to_string(),
        )
        .unwrap();
        let file = with_dir(home.path(), || {
            assert!(default_file().starts_with(&home.path().display().to_string()));
            let said = export_to(&other.path().display().to_string()).unwrap();
            assert!(said.contains("1 file"), "{said}");
            other.path().join(bundle::FILE_NAME)
        });
        assert!(file.is_file(), "a folder gets the bundle's name inside it");

        // Another machine: one key differs, so the preview names it and
        // writes nothing; the import then does.
        let there = tempfile::tempdir().unwrap();
        std::fs::write(
            there.path().join("config.json"),
            json!({"theme": "ocean", "editor": "vim"}).to_string(),
        )
        .unwrap();
        with_dir(there.path(), || {
            let lines = preview(&file.display().to_string()).unwrap();
            assert_eq!(lines[0], "config: 1 key changed");
            assert_eq!(lines[1], "config.json: editor");
            let stored = std::fs::read_to_string(there.path().join("config.json")).unwrap();
            assert!(stored.contains("vim"), "a preview writes nothing");
            let said = import(&file.display().to_string()).unwrap();
            assert!(said.contains("1 key changed"), "{said}");
            let stored = std::fs::read_to_string(there.path().join("config.json")).unwrap();
            assert!(stored.contains("hx"));
            let again = preview(&file.display().to_string()).unwrap_err();
            assert!(again.contains("changes nothing"), "{again}");
        });
    }

    #[test]
    fn a_missing_or_foreign_file_says_why() {
        let dir = tempfile::tempdir().unwrap();
        with_dir(dir.path(), || {
            let gone = dir.path().join("nope.json");
            assert!(preview(&gone.display().to_string()).is_err());
            let list = dir.path().join("list.json");
            std::fs::write(&list, "[1, 2]").unwrap();
            let why = preview(&list.display().to_string()).unwrap_err();
            assert!(why.contains("presets or ssh hosts"), "{why}");
        });
    }
}
