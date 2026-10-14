//! Agent presets from the window: the same `agent_presets.json` list the
//! TUI's `e` edits (`nebula_config::presets`), listed, created, edited and
//! deleted here. A save keeps entries and fields this build can't read.

use crate::Res;
use nebula_config::presets::{self, AgentPreset};
use nebula_config::Config;
use serde::{Deserialize, Serialize};
use std::path::Path;

/// One preset as the window shows and edits it: the harness by id (a
/// custom harness's id or a built-in kind's), and whether that harness is
/// offered now.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PresetRow {
    pub name: String,
    pub harness: String,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub effort: Option<String>,
    #[serde(default)]
    pub prefix: String,
    #[serde(default)]
    pub postfix: String,
    #[serde(default)]
    pub skip_task: bool,
    /// `claude · opus · high`, as the TUI's list reads.
    #[serde(default)]
    pub spec: String,
    /// The harness is enabled and valid; a preset for one switched off
    /// lists, but launches nothing.
    #[serde(default)]
    pub usable: bool,
}

fn row_of(cfg: &Config, p: &AgentPreset) -> PresetRow {
    let harness = p.harness_id().to_string();
    let usable = crate::settings::offered(cfg)
        .iter()
        .any(|h| h.id == harness);
    PresetRow {
        name: p.name.clone(),
        harness,
        model: p.model.clone(),
        effort: p.effort.clone(),
        prefix: p.prefix.clone(),
        postfix: p.postfix.clone(),
        skip_task: p.skip_task,
        spec: p.spec_label(),
        usable,
    }
}

/// "default" or blank means follow the Agents tab: stored as no value.
fn pinned(v: Option<String>) -> Option<String> {
    v.map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty() && !s.eq_ignore_ascii_case("default"))
}

fn preset_of(cfg: &Config, row: PresetRow) -> Res<AgentPreset> {
    let (kind, custom) = crate::settings::resolve_kind(cfg, row.harness.trim())?;
    Ok(AgentPreset {
        name: row.name.trim().to_string(),
        kind,
        custom_harness: custom,
        model: pinned(row.model),
        effort: pinned(row.effort),
        prefix: row.prefix,
        postfix: row.postfix,
        skip_task: row.skip_task,
    })
}

pub fn list_at(cfg: &Config, store: &Path) -> Vec<PresetRow> {
    presets::load_from(store)
        .iter()
        .map(|p| row_of(cfg, p))
        .collect()
}

/// Save `row`: a new preset at the end when `original` is None, else in
/// place of the one named `original` (which may be renamed).
pub fn save_at(
    cfg: &Config,
    store: &Path,
    original: Option<&str>,
    row: PresetRow,
) -> Res<Vec<PresetRow>> {
    let preset = preset_of(cfg, row)?;
    presets::edit_at(store, |list| {
        let at = match original {
            Some(name) => Some(
                list.iter()
                    .position(|p| p.name.eq_ignore_ascii_case(name.trim()))
                    .ok_or_else(|| format!("No preset named \"{}\" any more.", name.trim()))?,
            ),
            None => None,
        };
        let others: Vec<AgentPreset> = list
            .iter()
            .enumerate()
            .filter(|(i, _)| Some(*i) != at)
            .map(|(_, p)| p.clone())
            .collect();
        if let Some(problem) = presets::name_problem(&preset, &others) {
            return Err(problem);
        }
        let mut renamed = None;
        match at {
            Some(i) => {
                if list[i].name != preset.name {
                    renamed = Some((list[i].name.clone(), preset.name.clone()));
                }
                list[i] = preset;
            }
            None => list.push(preset),
        }
        Ok(renamed)
    })?;
    Ok(list_at(cfg, store))
}

pub fn delete_at(cfg: &Config, store: &Path, name: &str) -> Res<Vec<PresetRow>> {
    presets::edit_at(store, |list| {
        let before = list.len();
        list.retain(|p| !p.name.eq_ignore_ascii_case(name.trim()));
        if list.len() == before {
            return Err(format!("No preset named \"{}\".", name.trim()));
        }
        Ok(None)
    })?;
    Ok(list_at(cfg, store))
}

/// The starting prompt a launch with preset `name` sends for `task`.
pub fn compose_at(store: &Path, name: &str, task: &str) -> Res<String> {
    presets::load_from(store)
        .into_iter()
        .find(|p| p.name.eq_ignore_ascii_case(name.trim()))
        .map(|p| p.compose(task))
        .ok_or_else(|| format!("No preset named \"{}\" any more.", name.trim()))
}

#[tauri::command]
pub fn presets_list() -> Vec<PresetRow> {
    list_at(&Config::load(), &presets::store_path())
}

#[tauri::command]
pub fn preset_save(original: Option<String>, preset: PresetRow) -> Res<Vec<PresetRow>> {
    save_at(
        &Config::load(),
        &presets::store_path(),
        original.as_deref(),
        preset,
    )
}

#[tauri::command]
pub fn preset_delete(name: String) -> Res<Vec<PresetRow>> {
    delete_at(&Config::load(), &presets::store_path(), &name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{json, Value};

    fn row(name: &str, harness: &str) -> PresetRow {
        PresetRow {
            name: name.into(),
            harness: harness.into(),
            model: None,
            effort: None,
            prefix: String::new(),
            postfix: String::new(),
            skip_task: false,
            spec: String::new(),
            usable: false,
        }
    }

    fn setup() -> (tempfile::TempDir, Config, std::path::PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let cfg_path = dir.path().join("config.json");
        std::fs::write(
            &cfg_path,
            json!({"codex_enabled": false, "custom_harnesses": [
                {"id": "aider", "program": "aider", "model": "default", "model_flag": "--model"}
            ]})
            .to_string(),
        )
        .unwrap();
        let cfg = Config::load_at(&cfg_path, &dir.path().join("config.local.json"));
        let store = dir.path().join("agent_presets.json");
        (dir, cfg, store)
    }

    #[test]
    fn create_edit_and_delete_land_in_the_tuis_file() {
        let (_d, cfg, store) = setup();
        let mut reviewer = row("reviewer", "claude");
        reviewer.model = Some("opus".into());
        reviewer.effort = Some("default".into());
        reviewer.prefix = "Be strict.".into();
        let list = save_at(&cfg, &store, None, reviewer).unwrap();
        assert_eq!(list[0].spec, "claude · opus");
        assert!(list[0].usable);
        let list = save_at(&cfg, &store, None, row("pair", "aider")).unwrap();
        assert_eq!(list[1].harness, "aider");

        // What the TUI reads back.
        let stored = presets::load_from(&store);
        assert_eq!(stored[0].effort, None, "\"default\" is stored as no value");
        assert_eq!(stored[1].kind, nebula_core::AgentKind::Custom);
        assert_eq!(stored[1].custom_harness.as_deref(), Some("aider"));

        // An edit renames in place.
        let mut edited = row("strict reviewer", "codex");
        edited.skip_task = true;
        let list = save_at(&cfg, &store, Some("Reviewer"), edited).unwrap();
        assert_eq!(list[0].name, "strict reviewer");
        assert!(!list[0].usable, "codex is switched off");

        let list = delete_at(&cfg, &store, "pair").unwrap();
        assert_eq!(list.len(), 1);
        assert!(delete_at(&cfg, &store, "pair").is_err());
    }

    #[test]
    fn duplicate_names_unknown_harnesses_and_gone_originals_are_refused() {
        let (_d, cfg, store) = setup();
        save_at(&cfg, &store, None, row("a", "claude")).unwrap();
        save_at(&cfg, &store, None, row("b", "claude")).unwrap();
        assert!(save_at(&cfg, &store, None, row("A", "claude")).is_err());
        assert!(save_at(&cfg, &store, Some("b"), row("a", "claude")).is_err());
        assert!(
            save_at(&cfg, &store, Some("b"), row("B", "claude")).is_ok(),
            "own name is fine"
        );
        assert!(save_at(&cfg, &store, None, row("c", "nope")).is_err());
        assert!(save_at(&cfg, &store, Some("zz"), row("c", "claude")).is_err());
        assert!(save_at(&cfg, &store, None, row("  ", "claude")).is_err());
    }

    #[test]
    fn a_save_keeps_what_the_window_cannot_read() {
        let (_d, cfg, store) = setup();
        std::fs::write(
            &store,
            r#"[{"name": "a", "kind": "claude", "pinned": true}, {"name": "future", "kind": "antigravity"}]"#,
        )
        .unwrap();
        save_at(&cfg, &store, Some("a"), row("a", "claude")).unwrap();
        let saved: Vec<Value> =
            serde_json::from_str(&std::fs::read_to_string(&store).unwrap()).unwrap();
        assert_eq!(saved[0]["pinned"], true);
        assert_eq!(saved[1]["name"], "future");

        // A rename keeps them too.
        save_at(&cfg, &store, Some("a"), row("b", "claude")).unwrap();
        let saved: Vec<Value> =
            serde_json::from_str(&std::fs::read_to_string(&store).unwrap()).unwrap();
        assert_eq!(saved[0]["name"], "b");
        assert_eq!(saved[0]["pinned"], true);
        assert_eq!(saved.len(), 2);
    }

    #[test]
    fn a_preset_composes_prefix_task_and_postfix() {
        let (_d, cfg, store) = setup();
        let mut p = row("wrap", "claude");
        p.prefix = "PRE".into();
        p.postfix = "POST".into();
        save_at(&cfg, &store, None, p).unwrap();
        assert_eq!(
            compose_at(&store, "wrap", "do it").unwrap(),
            "PRE\n\ndo it\n\nPOST"
        );
        assert!(compose_at(&store, "gone", "x").is_err());
    }

    /// A presets file that no longer parses is left alone: the save and
    /// the delete both fail with a message naming it.
    #[test]
    fn an_unreadable_presets_file_is_refused_not_replaced() {
        let (_d, cfg, store) = setup();
        std::fs::write(&store, "{ broken").unwrap();
        let err = save_at(&cfg, &store, None, row("a", "claude")).unwrap_err();
        assert!(err.contains("agent_presets.json"), "{err}");
        let err = delete_at(&cfg, &store, "a").unwrap_err();
        assert!(err.contains("agent_presets.json"), "{err}");
        assert_eq!(std::fs::read_to_string(&store).unwrap(), "{ broken");
    }
}
