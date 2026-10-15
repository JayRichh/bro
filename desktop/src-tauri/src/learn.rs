//! LEARN IN PLACE, the window's half: the first-use tips' seen-state and
//! the WHAT'S NEW trigger, read from and written to the config both apps
//! share (`show_tips`, `tips_seen`, `whats_new_seen`). The copy and the
//! rules are nebula-config's (`learn.rs`); the window's own tip bar and
//! page read the copy from `actions.gen.ts`.

use std::path::Path;
use std::sync::OnceLock;

use nebula_config::learn;
use nebula_config::Config;
use serde::Serialize;

type Res<T> = Result<T, String>;

/// Whether this run is a first install, not an upgrade: no settings file
/// and no daemon database (`learn::fresh_install`). Read once, at startup,
/// before the daemon is reached.
static FRESH: OnceLock<bool> = OnceLock::new();

/// Note whether this is a first install. Called first thing in `main`.
pub fn note_startup() {
    let _ = FRESH.set(learn::fresh_install());
}

#[derive(Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct LearnState {
    pub show_tips: bool,
    /// The tips already shown, in either app.
    pub seen: Vec<String>,
    /// Show the What's new page now (it is recorded as seen here).
    pub whats_new: bool,
    /// This build's release, major.minor.
    pub version: String,
}

/// The tips' state, and whether What's new is due — which, when it is,
/// is recorded as shown, as is this release on a first install.
#[tauri::command]
pub fn learn_state() -> Res<LearnState> {
    let (path, local) = Config::layer_paths();
    state_at(
        &path,
        &local,
        *FRESH.get().unwrap_or(&false),
        &learn::current_version(),
    )
}

pub fn state_at(path: &Path, local: &Path, fresh: bool, current: &str) -> Res<LearnState> {
    let cfg = Config::load_at(path, local);
    let due = learn::whats_new_due(&cfg.whats_new_seen, current, fresh);
    if (due || fresh) && cfg.whats_new_seen != current {
        Config::update_in(path, local, |c| c.whats_new_seen = current.to_string())
            .map_err(|e| e.to_string())?;
    }
    Ok(LearnState {
        show_tips: cfg.show_tips,
        seen: cfg.tips_seen.iter().cloned().collect(),
        whats_new: due,
        version: current.to_string(),
    })
}

/// A tip was shown: never again, in either app.
#[tauri::command]
pub fn learn_seen(id: String) -> Res<()> {
    let (path, local) = Config::layer_paths();
    seen_at(&path, &local, &id)
}

pub fn seen_at(path: &Path, local: &Path, id: &str) -> Res<()> {
    if learn::tip(id).is_none() {
        return Err(format!("no tip {id}"));
    }
    Config::update_in(path, local, |c| {
        c.tips_seen.insert(id.to_string());
    })
    .map(|_| ())
    .map_err(|e| e.to_string())
}

/// "Don't show tips" (or Show tips back on).
#[tauri::command]
pub fn learn_tips(on: bool) -> Res<()> {
    let (path, local) = Config::layer_paths();
    Config::update_in(&path, &local, |c| c.show_tips = on)
        .map(|_| ())
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn files() -> (tempfile::TempDir, std::path::PathBuf, std::path::PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.json");
        let local = dir.path().join("config.local.json");
        (dir, path, local)
    }

    #[test]
    fn whats_new_is_due_once_after_an_upgrade() {
        let (_d, path, local) = files();
        std::fs::write(&path, r#"{"whats_new_seen": "0.49"}"#).unwrap();
        let first = state_at(&path, &local, false, "0.52").unwrap();
        assert!(first.whats_new);
        let again = state_at(&path, &local, false, "0.52").unwrap();
        assert!(!again.whats_new, "recorded as shown");
    }

    #[test]
    fn a_first_install_records_the_release_and_shows_nothing() {
        let (_d, path, local) = files();
        let s = state_at(&path, &local, true, "0.52").unwrap();
        assert!(!s.whats_new);
        assert!(s.show_tips);
        assert_eq!(Config::load_at(&path, &local).whats_new_seen, "0.52");
    }

    #[test]
    fn a_seen_tip_is_written_beside_the_terminal_apps() {
        let (_d, path, local) = files();
        std::fs::write(&path, r#"{"theme": "dark"}"#).unwrap();
        seen_at(&path, &local, learn::TIP_SCOPE).unwrap();
        let cfg = Config::load_at(&path, &local);
        assert!(cfg.tips_seen.contains(learn::TIP_SCOPE));
        assert_eq!(cfg.theme, "dark", "the rest kept");
        assert!(seen_at(&path, &local, "nope").is_err());
        let s = state_at(&path, &local, false, "0.52").unwrap();
        assert_eq!(s.seen, vec![learn::TIP_SCOPE.to_string()]);
    }
}
