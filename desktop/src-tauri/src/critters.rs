//! Critters: the window's half of the collection log, `<data>/critters.json`
//! (`kete_critters::log`), which the terminal app writes too, and the
//! settings the overlay reads at startup. The file is never held in memory
//! as the truth: each change reads it fresh, applies the notes and writes
//! it back, so a count another app wrote a moment ago is not lost.

use crate::Res;
use kete_critters::log::{CritterLog, LOG_FILE};
use kete_critters::{species, Note};
use nebula_config::Config;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Where the log lives, beside the database.
fn log_path() -> PathBuf {
    nebula_core::paths::data_dir().join(LOG_FILE)
}

/// The log at `path`. A missing or unparsable file is an empty log: it is
/// an easter egg's tally, never worth an error.
pub fn read_at(path: &Path) -> CritterLog {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|raw| serde_json::from_str(nebula_core::settings::strip_bom(&raw)).ok())
        .unwrap_or_default()
}

/// One thing the window saw happen, as the web side sends it:
/// `{kind: 'seen', species: 'kiwi'}`, `{kind: 'named', species, name}`,
/// `{kind: 'lamp'}`. The crate's [`Note`] borrows static species ids, so it
/// can't be deserialized into directly.
#[derive(Clone, Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum NoteIn {
    Seen { species: String },
    Petted { species: String },
    Fed { species: String },
    Named { species: String, name: String },
    Lamp,
}

impl NoteIn {
    /// The crate's note for this one, or None for a species id the cast
    /// doesn't have (a stale window, a typo): it would only add a ghost row.
    fn note(&self) -> Option<Note> {
        let id = |s: &str| species::by_id(s).map(|i| kete_critters::SPECIES[i].id);
        Some(match self {
            NoteIn::Seen { species } => Note::Seen {
                species: id(species)?,
            },
            NoteIn::Petted { species } => Note::Petted {
                species: id(species)?,
            },
            NoteIn::Fed { species } => Note::Fed {
                species: id(species)?,
            },
            NoteIn::Named { species, name } => Note::Named {
                species: id(species)?,
                name: name.clone(),
            },
            NoteIn::Lamp => Note::Lamp,
        })
    }
}

/// Fold `notes` into the log at `path` (read fresh), set or clear the
/// "hide until tomorrow" day (`hide`: Some(true) is today, Some(false)
/// clears it, None leaves it), write it back and return it. `today` is the
/// local date, `YYYY-MM-DD`; the web side sends it because only it knows
/// the user's clock.
pub fn update_at(
    path: &Path,
    notes: &[NoteIn],
    hide: Option<bool>,
    today: &str,
) -> Res<CritterLog> {
    // The terminal app folds its notes in the same way; the lock keeps two
    // writers from reading the same old counts. A lock that can't be had
    // (a stale one, a read-only dir) doesn't stop a tally.
    let _lock = nebula_core::settings::lock(path).ok();
    let mut log = read_at(path);
    for note in notes.iter().filter_map(NoteIn::note) {
        log.record(&note, today);
    }
    match hide {
        Some(true) => log.hidden_on = Some(today.to_string()),
        Some(false) => log.hidden_on = None,
        None => {}
    }
    let mut bytes = serde_json::to_vec_pretty(&log).map_err(|e| e.to_string())?;
    bytes.push(b'\n');
    // The shared helper writes a temp file beside it and renames, so the
    // terminal app never reads half a file.
    nebula_core::settings::write_atomic(path, &bytes).map_err(|e| e.to_string())?;
    Ok(log)
}

/// The collection log as the file has it now.
#[tauri::command]
pub fn critters_log() -> CritterLog {
    read_at(&log_path())
}

/// Record `notes` in the log and optionally hide visitors for `today`;
/// returns the log as written.
#[tauri::command]
pub fn critters_log_update(
    notes: Vec<NoteIn>,
    hide: Option<bool>,
    today: String,
) -> Res<CritterLog> {
    update_at(&log_path(), &notes, hide, &today)
}

/// The settings the overlay reads: the Critters rows, and Animations (off
/// means no critters at all).
#[derive(Debug, Serialize, PartialEq)]
pub struct CrittersPrefs {
    pub on: bool,
    /// `small`, `medium` or `large`.
    pub size: String,
    /// `rare`, `normal` or `busy`.
    pub often: String,
    pub animations: bool,
    /// A debug build: the overlay's test hook (`window.__kete_critters`)
    /// is only there for the e2e suite's debug builds.
    pub debug: bool,
}

/// The prefs in `cfg`, the size and often words resolved to ones on the list.
pub fn prefs_of(cfg: &Config) -> CrittersPrefs {
    CrittersPrefs {
        on: cfg.critters,
        size: cfg.critters_size_word().to_string(),
        often: cfg.critters_often_word().to_string(),
        animations: cfg.animations,
        debug: cfg!(debug_assertions),
    }
}

/// The Critters settings, read at startup.
#[tauri::command]
pub fn critters_prefs() -> CrittersPrefs {
    prefs_of(&Config::load())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn seen(id: &str) -> NoteIn {
        NoteIn::Seen { species: id.into() }
    }

    #[test]
    fn a_missing_file_is_an_empty_log() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(
            read_at(&dir.path().join("critters.json")),
            CritterLog::default()
        );
    }

    #[test]
    fn a_file_that_cannot_be_parsed_is_an_empty_log() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("critters.json");
        std::fs::write(&path, "{ not json").unwrap();
        assert_eq!(read_at(&path), CritterLog::default());
        // A later update starts over from it rather than failing.
        let log = update_at(&path, &[seen("kiwi")], None, "2026-10-16").unwrap();
        assert_eq!(log.species["kiwi"].seen, 1);
    }

    #[test]
    fn an_update_applies_every_kind_of_note_and_writes_it() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sub").join("critters.json");
        let notes = [
            seen("kiwi"),
            seen("kiwi"),
            NoteIn::Petted {
                species: "kiwi".into(),
            },
            NoteIn::Fed {
                species: "cat".into(),
            },
            NoteIn::Named {
                species: "kiwi".into(),
                name: "Bob".into(),
            },
            NoteIn::Lamp,
        ];
        let log = update_at(&path, &notes, None, "2026-10-16").unwrap();
        let kiwi = &log.species["kiwi"];
        assert_eq!((kiwi.seen, kiwi.pets, kiwi.fed), (2, 1, 0));
        assert_eq!(kiwi.name.as_deref(), Some("Bob"));
        assert_eq!(kiwi.first_seen.as_deref(), Some("2026-10-16"));
        assert_eq!(log.species["cat"].fed, 1);
        assert_eq!(log.lamps, 1);
        // What came back is what is on disk (the parent was made for it).
        assert_eq!(read_at(&path), log);
        assert_eq!(log.names()["kiwi"], "Bob");
    }

    #[test]
    fn a_second_update_reads_what_the_first_wrote() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("critters.json");
        update_at(&path, &[seen("kiwi")], None, "2026-10-16").unwrap();
        // The terminal app writes the file between the two.
        let mut theirs = read_at(&path);
        theirs.species.entry("cat".into()).or_default().pets = 4;
        std::fs::write(&path, serde_json::to_string(&theirs).unwrap()).unwrap();
        let log = update_at(&path, &[seen("kiwi")], None, "2026-10-17").unwrap();
        assert_eq!(log.species["kiwi"].seen, 2);
        // The first visit's date stays.
        assert_eq!(
            log.species["kiwi"].first_seen.as_deref(),
            Some("2026-10-16")
        );
        assert_eq!(log.species["cat"].pets, 4);
    }

    #[test]
    fn hiding_sets_clears_or_leaves_the_day() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("critters.json");
        let log = update_at(&path, &[], Some(true), "2026-10-16").unwrap();
        assert_eq!(log.hidden_on.as_deref(), Some("2026-10-16"));
        assert!(read_at(&path).hidden("2026-10-16"));
        // None leaves it, even with notes in the same call.
        let log = update_at(&path, &[seen("kiwi")], None, "2026-10-17").unwrap();
        assert_eq!(log.hidden_on.as_deref(), Some("2026-10-16"));
        assert!(!log.hidden("2026-10-17"));
        let log = update_at(&path, &[], Some(false), "2026-10-17").unwrap();
        assert_eq!(log.hidden_on, None);
        assert_eq!(read_at(&path).hidden_on, None);
    }

    #[test]
    fn an_unknown_species_is_skipped() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("critters.json");
        let notes = [
            seen("dragon"),
            NoteIn::Named {
                species: "dragon".into(),
                name: "Smaug".into(),
            },
            seen("kiwi"),
        ];
        let log = update_at(&path, &notes, None, "2026-10-16").unwrap();
        assert_eq!(log.species.keys().collect::<Vec<_>>(), ["kiwi"]);
    }

    #[test]
    fn the_web_sides_notes_deserialize() {
        let notes: Vec<NoteIn> = serde_json::from_value(json!([
            {"kind": "seen", "species": "kiwi"},
            {"kind": "petted", "species": "kiwi"},
            {"kind": "fed", "species": "kiwi"},
            {"kind": "named", "species": "kiwi", "name": "Bob"},
            {"kind": "lamp"},
        ]))
        .unwrap();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("critters.json");
        let log = update_at(&path, &notes, None, "2026-10-16").unwrap();
        let kiwi = &log.species["kiwi"];
        assert_eq!((kiwi.seen, kiwi.pets, kiwi.fed), (1, 1, 1));
        assert_eq!(log.lamps, 1);
    }

    #[test]
    fn the_prefs_follow_the_config_with_words_off_the_list_resolved() {
        let off = prefs_of(&Config::default());
        assert_eq!(
            off,
            CrittersPrefs {
                on: false,
                size: "medium".into(),
                often: "normal".into(),
                animations: true,
                debug: cfg!(debug_assertions),
            }
        );
        let on = prefs_of(&Config {
            critters: true,
            critters_size: "LARGE".into(),
            critters_often: "whenever".into(),
            animations: false,
            ..Config::default()
        });
        assert_eq!(
            on,
            CrittersPrefs {
                on: true,
                size: "large".into(),
                often: "normal".into(),
                animations: false,
                debug: cfg!(debug_assertions),
            }
        );
    }
}
