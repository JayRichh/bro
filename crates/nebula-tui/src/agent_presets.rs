//! AGENT PRESETS in the TUI: the data model and the store moved to
//! `nebula_config::presets` (shared with the desktop window) and are
//! re-exported here under the old paths; what stays is the PRESET EDITOR's
//! [`PresetTextExt`].

pub use nebula_config::presets::*;

/// Which side of the task a preset's text goes: a PREFIX before it, a
/// POSTFIX after it, or both. The PRESET EDITOR shows one box per side —
/// its Text row picks — and **Preset text** (Settings → Sessions,
/// [`crate::config::Config::preset_text`]) is what a new preset starts on:
/// prefix alone by default, the framing most people reach for and one box
/// to fill. Never stored on the preset — a side is what it holds, so the
/// form derives it ([`PresetText::for_preset`]) and an older nebula reads
/// the file unchanged.
pub use nebula_config::choices::PresetText;

/// [`PresetText`] against a stored preset, which only the TUI knows.
pub trait PresetTextExt {
    /// What the editor shows for a stored preset under the `default`
    /// setting: every side the preset already holds text on, plus the
    /// side(s) the setting names — so nothing saved is ever hidden, and a
    /// blank preset opens the way a new one would.
    fn for_preset(preset: &AgentPreset, default: PresetText) -> PresetText {
        PresetText::of_sides(
            !preset.prefix.trim().is_empty() || default.has_prefix(),
            !preset.postfix.trim().is_empty() || default.has_postfix(),
        )
        .unwrap_or(default)
    }
}

impl PresetTextExt for PresetText {}

#[cfg(test)]
mod tests {
    use super::*;
    use nebula_core::AgentKind;

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
    fn preset_text_labels_round_trip_and_both_is_an_alias() {
        for side in PresetText::ALL {
            assert_eq!(PresetText::parse(side.as_str()), Some(side));
            assert_eq!(
                PresetText::parse(&format!(" {} ", side.as_str().to_uppercase())),
                Some(side)
            );
        }
        assert_eq!(PresetText::parse(" both "), Some(PresetText::Both));
        assert_eq!(PresetText::parse("suffix"), None);
        assert_eq!(PresetText::parse(""), None);
        assert_eq!(PresetText::DEFAULT, PresetText::Prefix);
        assert!(PresetText::Prefix.has_prefix() && !PresetText::Prefix.has_postfix());
        assert!(!PresetText::Postfix.has_prefix() && PresetText::Postfix.has_postfix());
        assert!(PresetText::Both.has_prefix() && PresetText::Both.has_postfix());
        assert_eq!(PresetText::of_sides(false, false), None);
    }

    /// A stored side is never hidden: the form shows what the preset holds
    /// on top of what the setting names, and a blank preset follows the
    /// setting alone.
    #[test]
    fn for_preset_shows_every_side_with_text_and_the_setting_for_the_rest() {
        use PresetText::*;
        let blank = preset("p", AgentKind::Claude);
        let pre = AgentPreset {
            prefix: "PRE".into(),
            ..blank.clone()
        };
        let post = AgentPreset {
            postfix: "POST".into(),
            ..blank.clone()
        };
        let both = AgentPreset {
            prefix: "PRE".into(),
            postfix: "POST".into(),
            ..blank.clone()
        };
        assert_eq!(PresetText::for_preset(&blank, Prefix), Prefix);
        assert_eq!(PresetText::for_preset(&blank, Postfix), Postfix);
        assert_eq!(PresetText::for_preset(&blank, Both), Both);
        assert_eq!(PresetText::for_preset(&pre, Prefix), Prefix);
        assert_eq!(
            PresetText::for_preset(&pre, Postfix),
            Both,
            "the prefix it holds stays on screen"
        );
        assert_eq!(
            PresetText::for_preset(&post, Prefix),
            Both,
            "the postfix it holds stays on screen"
        );
        assert_eq!(PresetText::for_preset(&post, Postfix), Postfix);
        assert_eq!(PresetText::for_preset(&both, Prefix), Both);
        let spaces = AgentPreset {
            postfix: "  \n".into(),
            ..blank
        };
        assert_eq!(
            PresetText::for_preset(&spaces, Prefix),
            Prefix,
            "whitespace is no text"
        );
    }
}

#[cfg(test)]
mod golden {
    use super::*;
    use nebula_core::AgentKind;

    const GOLDEN: &str = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/src/testdata/config_golden/agent_presets.json"
    );

    #[test]
    fn a_preset_save_writes_these_bytes() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("agent_presets.json");
        std::fs::write(
            &path,
            r#"[
                {"name": "reviewer", "kind": "codex", "pinned": true, "model": "gpt-x"},
                {"name": "future", "kind": "antigravity", "model": "x"},
                {"name": "gone", "kind": "claude"}
            ]"#,
        )
        .unwrap();
        let mut presets = load_from(&path);
        presets.retain(|p| p.name != "gone");
        presets[0].prefix = "Be strict.".into();
        presets[0].effort = Some("high".into());
        presets.push(AgentPreset {
            name: "scratch".into(),
            kind: AgentKind::Custom,
            custom_harness: Some("aider".into()),
            model: None,
            effort: None,
            prefix: String::new(),
            postfix: "Run the tests.".into(),
            skip_task: true,
        });
        save_to(&path, &presets).unwrap();
        let actual = std::fs::read_to_string(&path).unwrap();
        if std::env::var_os("KETE_BLESS").is_some() {
            std::fs::write(GOLDEN, &actual).unwrap();
            return;
        }
        let want = std::fs::read_to_string(GOLDEN)
            .unwrap_or_else(|e| panic!("{GOLDEN}: {e} (run with KETE_BLESS=1)"));
        assert_eq!(actual, want.replace("\r\n", "\n"), "preset save drifted");
    }
}
