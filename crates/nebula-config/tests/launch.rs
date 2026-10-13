//! The launch pickers' choices come from the Agents tab's own lists and
//! values.

use nebula_config::schema::{export, launch_choices, Control, Target};
use nebula_config::{desktop::DesktopPrefs, Config};
use serde_json::json;

fn cfg_from(value: serde_json::Value) -> (tempfile::TempDir, Config) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.json");
    std::fs::write(&path, value.to_string()).unwrap();
    let cfg = Config::load_at(&path, &dir.path().join("config.local.json"));
    (dir, cfg)
}

#[test]
fn launch_choices_match_the_agents_tab_rows() {
    let (_d, cfg) = cfg_from(json!({"codex_model": "gpt-5.5", "codex_effort": "high"}));
    let schema = export(&cfg, &DesktopPrefs::default(), &[]);
    let agents = schema.tabs.iter().find(|t| t.id == "agents").unwrap();
    let row = |field: &str| {
        agents
            .groups
            .iter()
            .flat_map(|g| &g.rows)
            .find(|r| matches!(&r.target, Target::Agent { harness, field: f } if harness == "codex" && f == field))
            .unwrap()
            .clone()
    };
    let choices = launch_choices(&cfg, "codex");
    let Control::Choice { options, .. } = row("model").control else {
        panic!("model is a choice")
    };
    assert_eq!(choices.models, options);
    assert_eq!(choices.models[0], "default");
    assert_eq!(choices.model, "gpt-5.5");
    assert_eq!(choices.effort, "high");
    let Control::Choice { options, .. } = row("effort").control else {
        panic!("effort is a choice")
    };
    assert_eq!(choices.efforts["gpt-5.5"], options);
}

#[test]
fn a_custom_harness_with_no_effort_offers_none() {
    let (_d, cfg) = cfg_from(json!({"custom_harnesses": [
        {"id": "aider", "program": "aider", "model": "default", "model_flag": "--model"}
    ]}));
    let choices = launch_choices(&cfg, "aider");
    assert_eq!(choices.models[0], "default");
    assert!(choices.efforts.values().all(Vec::is_empty));
    assert_eq!(choices.effort, "n/a");
}
