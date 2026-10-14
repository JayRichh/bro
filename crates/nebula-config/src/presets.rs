//! AGENT PRESETS: saved launch definitions — an AGENT KIND, a MODEL / EFFORT
//! choice, optional prefix / postfix text, and whether to ask for a task at
//! all. The TUI's `e` lists them for the checkout under the cursor and the
//! desktop window's quick prompt and presets manager read the same list.
//! Launching one asks for an optional task (or, with `skip_task`, nothing)
//! and hands the CLI `prefix + task + postfix` as its positional starting
//! prompt.
//!
//! A plain JSON list in the DATA DIR beside `config.json`, in list order.
//! A missing or malformed file reads as empty — like the SSH HOSTS FILE it
//! is a convenience store, never load-bearing — but a malformed one is
//! never written over: a save refuses it, so a hand edit gone wrong can be
//! fixed instead of lost. Saves take the settings lock (`<file>.lock`) the
//! other kete writers take, and go through a temp file + rename so a crash
//! mid-write cannot truncate the list. The list is
//! read entry by entry, and a save keeps what this build can't read — a
//! preset for a harness a newer nebula added, a field it has no name for —
//! so an older nebula sharing the file never deletes a newer one's presets.

use nebula_core::AgentKind;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentPreset {
    /// The row's label; unique (case-insensitively) within the list.
    pub name: String,
    /// The CLI the preset launches.
    #[serde(default)]
    pub kind: AgentKind,
    /// Registry id when `kind` is [`AgentKind::Custom`].
    #[serde(default)]
    pub custom_harness: Option<String>,
    /// Launch model; None = follow the Settings → Agents default.
    #[serde(default)]
    pub model: Option<String>,
    /// Launch effort; None = follow the Settings → Agents default.
    #[serde(default)]
    pub effort: Option<String>,
    /// Text sent before the task (may be empty).
    #[serde(default)]
    pub prefix: String,
    /// Text sent after the task (may be empty).
    #[serde(default)]
    pub postfix: String,
    /// Launch without asking for a task: Enter on the preset starts the CLI
    /// on prefix + postfix alone — a "commit and push" preset has nothing
    /// left to say. Off, the task is still asked for, but optional.
    #[serde(default)]
    pub skip_task: bool,
}

impl AgentPreset {
    /// `claude · opus · high` / `codex · gpt-5.5` / `cursor` — the kind plus
    /// whichever of model and effort the preset pins.
    pub fn spec_label(&self) -> String {
        let harness = self
            .custom_harness
            .as_deref()
            .unwrap_or_else(|| self.kind.as_str());
        let mut parts = vec![harness.to_string()];
        parts.extend(self.model.iter().cloned());
        parts.extend(self.effort.iter().cloned());
        parts.join(" · ")
    }

    /// The harness id the preset launches: the custom entry's id, else the
    /// built-in kind's.
    pub fn harness_id(&self) -> &str {
        match (&self.kind, self.custom_harness.as_deref()) {
            (AgentKind::Custom, Some(id)) => id,
            _ => self.kind.as_str(),
        }
    }

    /// True when the preset wraps the task in any text at all.
    pub fn has_wrapping(&self) -> bool {
        !self.prefix.trim().is_empty() || !self.postfix.trim().is_empty()
    }

    /// The starting prompt: prefix, task and postfix — each trimmed, empty
    /// parts skipped — joined by a blank line. Empty when all three are:
    /// the launch then has no starting prompt at all.
    pub fn compose(&self, task: &str) -> String {
        [self.prefix.as_str(), task, self.postfix.as_str()]
            .iter()
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join("\n\n")
    }
}

pub fn load() -> Vec<AgentPreset> {
    load_from(&store_path())
}

/// Persist the whole list, in order.
pub fn save(presets: &[AgentPreset]) -> std::io::Result<()> {
    save_to(&store_path(), presets)
}

/// [`save`] after an edit renamed one preset `(old, new)`: the renamed
/// entry keeps the fields its stored entry had under the old name.
pub fn save_renamed(presets: &[AgentPreset], renamed: (&str, &str)) -> std::io::Result<()> {
    save_to_renamed(&store_path(), presets, Some(renamed))
}

/// `<data dir>/agent_presets.json`, or this thread's test override.
pub fn store_path() -> PathBuf {
    #[cfg(any(test, feature = "test-support"))]
    {
        if let Some(path) = PRESETS_PATH_OVERRIDE.with(|p| p.borrow().clone()) {
            return path;
        }
    }
    nebula_core::paths::data_dir().join(nebula_core::paths::PRESETS_FILE_NAME)
}

/// The presets the file at `path` holds, entry by entry (unreadable
/// entries skipped).
pub fn load_from(path: &Path) -> Vec<AgentPreset> {
    nebula_core::settings::read_list(path).0
}

/// The `name` a raw preset entry carries, as the file spells it.
pub fn entry_name(entry: &Value) -> Option<&str> {
    entry.get("name")?.as_str()
}

/// Write `presets` in order. Each keeps the fields its stored entry (same
/// name) had that this build doesn't know, and every stored entry this build
/// can't read goes after them unless a preset now holds its name.
pub fn save_to(store: &Path, presets: &[AgentPreset]) -> std::io::Result<()> {
    save_to_renamed(store, presets, None)
}

/// [`save_to`], with one preset renamed `(old, new)` by the edit: its
/// stored entry is found under the old name, so the fields this build
/// doesn't know ride across the rename.
///
/// Takes the settings lock on the store for the read and the write. A file
/// that exists but no longer reads as a list is refused, never replaced.
pub fn save_to_renamed(
    store: &Path,
    presets: &[AgentPreset],
    renamed: Option<(&str, &str)>,
) -> std::io::Result<()> {
    let _lock = nebula_core::settings::lock(store)?;
    write_locked(store, presets, renamed)
}

/// Edit the list as the file holds it now: lock the store, read it, let
/// `edit` change it (returning the `(old, new)` names of a rename, if it
/// made one), then save, all under the one lock. So a preset another app
/// saved since this one last read the list survives. An unreadable file is
/// refused before `edit` runs.
pub fn edit_at(
    store: &Path,
    edit: impl FnOnce(&mut Vec<AgentPreset>) -> Result<Option<(String, String)>, String>,
) -> Result<(), String> {
    let saving = |err: std::io::Error| format!("Could not save the presets: {err}");
    let _lock = nebula_core::settings::lock(store).map_err(saving)?;
    stored_entries(store).map_err(saving)?;
    let mut list = load_from(store);
    let renamed = edit(&mut list)?;
    let renamed = renamed.as_ref().map(|(o, n)| (o.as_str(), n.as_str()));
    write_locked(store, &list, renamed).map_err(saving)
}

/// The raw entries the store holds; a missing file is an empty list, and
/// one that is there but unreadable is an error naming the file.
fn stored_entries(store: &Path) -> std::io::Result<Vec<Value>> {
    nebula_core::settings::read_array(store)
        .map(Option::unwrap_or_default)
        .map_err(|err| {
            let name = store.file_name().map_or_else(
                || store.display().to_string(),
                |n| n.to_string_lossy().into(),
            );
            std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("{name} is {err}: fix or remove it before saving presets"),
            )
        })
}

/// The write behind [`save_to_renamed`] and [`edit_at`]; the caller holds
/// the lock.
fn write_locked(
    store: &Path,
    presets: &[AgentPreset],
    renamed: Option<(&str, &str)>,
) -> std::io::Result<()> {
    let stored = stored_entries(store)?;
    let named = |name: &str| {
        stored
            .iter()
            .find(|entry| entry_name(entry).is_some_and(|n| n.eq_ignore_ascii_case(name)))
    };
    let mut entries = Vec::with_capacity(presets.len());
    for preset in presets {
        let Value::Object(known) = serde_json::to_value(preset)
            .map_err(|err| std::io::Error::new(std::io::ErrorKind::InvalidData, err))?
        else {
            unreachable!("a preset serializes to a JSON object");
        };
        let lookup = match renamed {
            Some((old, new)) if new.eq_ignore_ascii_case(&preset.name) => old,
            _ => preset.name.as_str(),
        };
        let mut entry = named(lookup)
            .and_then(Value::as_object)
            .cloned()
            .unwrap_or_default();
        entry.extend(known);
        entries.push(Value::Object(entry));
    }
    for entry in &stored {
        let unreadable = serde_json::from_value::<AgentPreset>(entry.clone()).is_err();
        let taken = entry_name(entry)
            .is_some_and(|n| presets.iter().any(|p| p.name.eq_ignore_ascii_case(n)));
        if unreadable && !taken {
            entries.push(entry.clone());
        }
    }
    nebula_core::settings::write_json(store, &Value::Array(entries))
}

/// Why `preset` can't join `others` (the list without the entry being
/// edited): an empty name, or one another preset already holds.
pub fn name_problem(preset: &AgentPreset, others: &[AgentPreset]) -> Option<String> {
    let name = preset.name.trim();
    if name.is_empty() {
        return Some("A preset needs a name.".into());
    }
    others
        .iter()
        .any(|p| p.name.trim().eq_ignore_ascii_case(name))
        .then(|| format!("A preset named \"{name}\" already exists."))
}

#[cfg(any(test, feature = "test-support"))]
thread_local! {
    static PRESETS_PATH_OVERRIDE: std::cell::RefCell<Option<PathBuf>> =
        const { std::cell::RefCell::new(None) };
}

/// Test hook (the `with_config_path` pattern): route this thread's preset
/// store at `path` for the duration of `f`.
#[cfg(any(test, feature = "test-support"))]
pub fn with_presets_path<T>(path: PathBuf, f: impl FnOnce() -> T) -> T {
    PRESETS_PATH_OVERRIDE.with(|slot| {
        let prev = slot.replace(Some(path));
        let out = f();
        slot.replace(prev);
        out
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store() -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("agent_presets.json");
        (dir, path)
    }

    fn preset(name: &str, kind: AgentKind) -> AgentPreset {
        AgentPreset {
            name: name.into(),
            kind,
            custom_harness: None,
            model: None,
            effort: None,
            prefix: String::new(),
            postfix: String::new(),
            skip_task: false,
        }
    }

    #[test]
    fn missing_or_malformed_file_reads_empty() {
        let (_dir, path) = store();
        assert!(load_from(&path).is_empty());
        std::fs::write(&path, "not json").unwrap();
        assert!(load_from(&path).is_empty());
    }

    #[test]
    fn save_round_trips_in_order_and_leaves_no_temp_file() {
        let (dir, path) = store();
        let presets = vec![
            AgentPreset {
                model: Some("opus".into()),
                effort: Some("high".into()),
                prefix: "Be strict.".into(),
                postfix: "Run the tests.".into(),
                ..preset("reviewer", AgentKind::Claude)
            },
            AgentPreset {
                skip_task: true,
                ..preset("scratch", AgentKind::Codex)
            },
        ];
        save_to(&path, &presets).unwrap();
        assert_eq!(load_from(&path), presets);
        let leftovers: Vec<_> = std::fs::read_dir(dir.path())
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name().to_string_lossy().ends_with(".tmp"))
            .collect();
        assert!(leftovers.is_empty());
        // A rewrite replaces, never appends.
        save_to(&path, &presets[1..]).unwrap();
        assert_eq!(load_from(&path), presets[1..].to_vec());
    }

    /// A preset for a harness this build has never heard of — a newer
    /// nebula's — survives a save, and so does a field this build doesn't know.
    #[test]
    fn a_save_keeps_what_a_newer_nebula_wrote() {
        let (_dir, path) = store();
        std::fs::write(
            &path,
            r#"[
                {"name": "reviewer", "kind": "codex", "pinned": true},
                {"name": "future", "kind": "antigravity", "model": "x"}
            ]"#,
        )
        .unwrap();
        let mut presets = load_from(&path);
        assert_eq!(presets.len(), 1, "only the readable preset lists");
        presets[0].prefix = "Be strict.".into();
        presets.push(preset("scratch", AgentKind::Claude));
        save_to(&path, &presets).unwrap();

        let saved: Vec<Value> =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        let names: Vec<&str> = saved.iter().filter_map(entry_name).collect();
        assert_eq!(names, ["reviewer", "scratch", "future"]);
        assert_eq!(saved[0]["pinned"], true);
        assert_eq!(saved[0]["prefix"], "Be strict.");
        assert_eq!(saved[2]["kind"], "antigravity");
        assert_eq!(load_from(&path).len(), 2);
    }

    #[test]
    fn a_rename_carries_the_fields_this_build_does_not_know() {
        let (_dir, path) = store();
        std::fs::write(
            &path,
            r#"[{"name": "reviewer", "kind": "codex", "pinned": true}]"#,
        )
        .unwrap();
        let mut presets = load_from(&path);
        presets[0].name = "strict reviewer".into();
        save_to_renamed(&path, &presets, Some(("reviewer", "strict reviewer"))).unwrap();
        let saved: Vec<Value> =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(saved.len(), 1);
        assert_eq!(saved[0]["name"], "strict reviewer");
        assert_eq!(saved[0]["pinned"], true);
    }

    #[test]
    fn a_name_only_record_deserializes_with_defaults() {
        let (_dir, path) = store();
        std::fs::write(&path, r#"[{"name": "old"}]"#).unwrap();
        let presets = load_from(&path);
        assert_eq!(presets.len(), 1);
        assert_eq!(presets[0].name, "old");
        assert_eq!(presets[0].kind, AgentKind::Claude);
        assert_eq!(presets[0].model, None);
        assert!(presets[0].prefix.is_empty() && presets[0].postfix.is_empty());
        assert!(!presets[0].skip_task, "an old record still asks for a task");
    }

    #[test]
    fn compose_skips_empty_parts_and_trims() {
        let mut p = preset("p", AgentKind::Claude);
        assert_eq!(p.compose(""), "", "nothing at all composes to nothing");
        assert_eq!(p.compose("  do it \n"), "do it");
        p.prefix = "PRE\n".into();
        assert_eq!(p.compose("do it"), "PRE\n\ndo it");
        p.postfix = "  POST".into();
        assert_eq!(p.compose("do it"), "PRE\n\ndo it\n\nPOST");
        assert_eq!(p.compose(" \n"), "PRE\n\nPOST", "the task is optional");
        p.prefix = "   ".into();
        assert_eq!(p.compose("line1\nline2"), "line1\nline2\n\nPOST");
        assert!(p.has_wrapping());
        p.postfix.clear();
        assert!(!p.has_wrapping());
    }

    #[test]
    fn spec_label_names_only_what_is_pinned() {
        assert_eq!(preset("p", AgentKind::Cursor).spec_label(), "cursor");
        let full = AgentPreset {
            model: Some("opus".into()),
            effort: Some("high".into()),
            ..preset("p", AgentKind::Claude)
        };
        assert_eq!(full.spec_label(), "claude · opus · high");
        let model_only = AgentPreset {
            model: Some("gpt-5.5".into()),
            ..preset("p", AgentKind::Codex)
        };
        assert_eq!(model_only.spec_label(), "codex · gpt-5.5");
    }

    #[test]
    fn harness_id_is_the_custom_entry_or_the_kind() {
        assert_eq!(preset("p", AgentKind::Codex).harness_id(), "codex");
        let custom = AgentPreset {
            custom_harness: Some("aider".into()),
            ..preset("p", AgentKind::Custom)
        };
        assert_eq!(custom.harness_id(), "aider");
    }

    #[test]
    fn a_name_must_be_present_and_unique_ignoring_case() {
        let others = [preset("Reviewer", AgentKind::Claude)];
        assert!(name_problem(&preset("  ", AgentKind::Claude), &others).is_some());
        assert!(name_problem(&preset("reviewer ", AgentKind::Claude), &others).is_some());
        assert!(name_problem(&preset("scratch", AgentKind::Claude), &others).is_none());
    }

    #[test]
    fn the_override_routes_the_store() {
        let (_dir, path) = store();
        with_presets_path(path.clone(), || {
            save(&[preset("a", AgentKind::Claude)]).unwrap();
            assert_eq!(store_path(), path);
            assert_eq!(load().len(), 1);
        });
    }
    /// A file that is there but no longer reads as a list (a hand edit gone
    /// wrong) is never replaced by a save: the save fails and the bytes stay.
    #[test]
    fn a_save_refuses_to_overwrite_an_unreadable_file() {
        let (_dir, path) = store();
        std::fs::write(&path, "[{\"name\": \"a\",,]").unwrap();
        let err = save_to(&path, &[preset("b", AgentKind::Claude)]).unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::InvalidData);
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            "[{\"name\": \"a\",,]"
        );
        let err = edit_at(&path, |list| {
            list.push(preset("b", AgentKind::Claude));
            Ok(None)
        })
        .unwrap_err();
        assert!(err.contains("agent_presets.json"), "{err}");
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            "[{\"name\": \"a\",,]"
        );
    }

    /// Notepad's byte order mark is not damage: the list reads and saves.
    #[test]
    fn a_bom_file_reads_and_saves() {
        let (_dir, path) = store();
        std::fs::write(&path, "\u{feff}[{\"name\": \"a\", \"kind\": \"claude\"}]").unwrap();
        assert_eq!(load_from(&path).len(), 1);
        edit_at(&path, |list| {
            list.push(preset("b", AgentKind::Codex));
            Ok(None)
        })
        .unwrap();
        let names: Vec<_> = load_from(&path).into_iter().map(|p| p.name).collect();
        assert_eq!(names, ["a", "b"]);
    }

    /// A save waits for a writer holding the lock, and an edit works on the
    /// list as the file holds it under the lock, so a preset another app
    /// added meanwhile survives.
    #[test]
    fn saves_take_the_lock_and_edits_reread_under_it() {
        let (_dir, path) = store();
        save_to(&path, &[preset("a", AgentKind::Claude)]).unwrap();
        let held = nebula_core::settings::lock(&path).unwrap();
        let writer = {
            let path = path.clone();
            std::thread::spawn(move || {
                edit_at(&path, |list| {
                    list.push(preset("mine", AgentKind::Codex));
                    Ok(None)
                })
            })
        };
        std::thread::sleep(std::time::Duration::from_millis(200));
        assert!(!writer.is_finished(), "the edit waits for the holder");
        // The holder adds a preset while the edit waits.
        let mut list = load_from(&path);
        list.push(preset("theirs", AgentKind::Claude));
        nebula_core::settings::write_json(&path, &serde_json::to_value(&list).unwrap()).unwrap();
        drop(held);
        writer.join().unwrap().unwrap();
        let names: Vec<_> = load_from(&path).into_iter().map(|p| p.name).collect();
        assert_eq!(names, ["a", "theirs", "mine"]);
    }
}
