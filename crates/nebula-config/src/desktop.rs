//! The desktop window's own preferences, kept under one `desktop` object
//! in `config.json` that [`crate::Config`] has no field for: the TUI never
//! writes it (an unknown key rides through its saves untouched) and the
//! window never writes a TUI key for them — its light/dark choice is not
//! the TUI's `theme`.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;

/// The top-level key the preferences live under.
pub const KEY: &str = "desktop";

/// The window theme: follow the OS, or pin light or dark.
pub const THEMES: &[&str] = &["system", "light", "dark"];
/// The Diff view's layout for a repo with none saved yet.
pub const DIFF_MODES: &[&str] = &["side", "inline", "replacement"];
/// What the main column shows with no view open.
pub const HOMES: &[&str] = &["overview", "board"];

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct DesktopPrefs {
    pub theme: String,
    pub diff_mode: String,
    pub home: String,
    /// Keys a newer window wrote, kept through a save.
    #[serde(flatten)]
    pub other: BTreeMap<String, serde_json::Value>,
}

impl Default for DesktopPrefs {
    fn default() -> Self {
        Self {
            theme: THEMES[0].into(),
            diff_mode: DIFF_MODES[0].into(),
            home: HOMES[0].into(),
            other: BTreeMap::new(),
        }
    }
}

/// The choices a preference offers, by key; None for a key there is none.
pub fn choices(key: &str) -> Option<&'static [&'static str]> {
    match key {
        "theme" => Some(THEMES),
        "diff_mode" => Some(DIFF_MODES),
        "home" => Some(HOMES),
        _ => None,
    }
}

impl DesktopPrefs {
    /// The preferences in the layers, `local` over `path`. A missing or
    /// unreadable object is the defaults, and a word off its list is that
    /// preference's default — never an error the window has to show.
    pub fn load_at(path: &Path, local: &Path) -> Self {
        Self::from_raw(crate::read_raw_key(path, local, KEY))
    }

    /// The preferences a stored `desktop` value holds, on [`Self::load_at`]'s
    /// terms.
    pub fn from_raw(raw: Option<serde_json::Value>) -> Self {
        let mut prefs: DesktopPrefs = raw
            .and_then(|value| serde_json::from_value(value).ok())
            .unwrap_or_default();
        let fallback = DesktopPrefs::default();
        for key in ["theme", "diff_mode", "home"] {
            let ok = choices(key).is_some_and(|list| list.contains(&prefs.get(key)));
            if !ok {
                prefs.set_unchecked(key, fallback.get(key));
            }
        }
        prefs
    }

    pub fn load() -> Self {
        let (path, local) = crate::layer_paths();
        Self::load_at(&path, &local)
    }

    pub fn get(&self, key: &str) -> &str {
        match key {
            "theme" => &self.theme,
            "diff_mode" => &self.diff_mode,
            "home" => &self.home,
            _ => "",
        }
    }

    fn set_unchecked(&mut self, key: &str, value: &str) {
        match key {
            "theme" => self.theme = value.into(),
            "diff_mode" => self.diff_mode = value.into(),
            "home" => self.home = value.into(),
            _ => {}
        }
    }

    /// Set one preference to one of its choices.
    pub fn set(&mut self, key: &str, value: &str) -> Result<(), String> {
        let list = choices(key).ok_or_else(|| format!("no desktop preference {key}"))?;
        if !list.contains(&value) {
            return Err(format!(
                "{key} is one of {}, not {value:?}",
                list.join(", ")
            ));
        }
        self.set_unchecked(key, value);
        Ok(())
    }

    /// Read the stored object fresh, apply `change`, and write the object
    /// back, leaving every other key in both layers alone — one read and
    /// one write under the settings lock. A refused change, or a settings
    /// file that can't be parsed, writes nothing.
    pub fn update_in(
        path: &Path,
        local: &Path,
        change: impl FnOnce(&mut DesktopPrefs) -> Result<(), String>,
    ) -> Result<DesktopPrefs, String> {
        let value = crate::update_raw_key(path, local, KEY, |stored| {
            let mut prefs = Self::from_raw(stored.cloned());
            change(&mut prefs).map_err(std::io::Error::other)?;
            serde_json::to_value(&prefs).map_err(std::io::Error::other)
        })
        .map_err(|e| e.to_string())?;
        Ok(Self::from_raw(Some(value)))
    }

    pub fn update(
        change: impl FnOnce(&mut DesktopPrefs) -> Result<(), String>,
    ) -> Result<DesktopPrefs, String> {
        let (path, local) = crate::layer_paths();
        Self::update_in(&path, &local, change)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_or_bad_words_read_as_the_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.json");
        let local = dir.path().join("config.local.json");
        assert_eq!(
            DesktopPrefs::load_at(&path, &local),
            DesktopPrefs::default()
        );
        std::fs::write(
            &path,
            r#"{"desktop": {"theme": "purple", "home": "board"}}"#,
        )
        .unwrap();
        let prefs = DesktopPrefs::load_at(&path, &local);
        assert_eq!(prefs.theme, "system");
        assert_eq!(prefs.home, "board");
    }

    #[test]
    fn a_save_touches_only_the_desktop_object_and_keeps_its_unknown_keys() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.json");
        let local = dir.path().join("config.local.json");
        std::fs::write(
            &path,
            r#"{"theme": "kete-light", "desktop": {"future": 3}, "x": [1]}"#,
        )
        .unwrap();
        DesktopPrefs::update_in(&path, &local, |p| p.set("theme", "dark")).unwrap();
        let saved: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(
            saved["theme"], "kete-light",
            "the TUI theme is not the window's"
        );
        assert_eq!(saved["x"], serde_json::json!([1]));
        assert_eq!(saved["desktop"]["theme"], "dark");
        assert_eq!(saved["desktop"]["future"], 3);
        assert!(DesktopPrefs::update_in(&path, &local, |p| p.set("theme", "neon")).is_err());
    }
}
