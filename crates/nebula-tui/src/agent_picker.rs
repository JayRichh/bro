//! The one AGENT KIND picker behind every launch surface — the NEW SESSION
//! PICKER (`n` on the grid, which opens the QUICK PROMPT on the pick; from
//! a menu's "New agent" row it launches outright), the PR SESSION picker
//! (`Tab` in the PULL REQUESTS MODAL) and the QUICK PROMPT's `Tab`. Each is one
//! `ContextMenu` with a row per harness still enabled on the AGENTS TAB (a
//! disabled one is absent, not greyed), every row a
//! `MenuAction::NewAgentOfKind` carrying the surface's launch context, so
//! `→` drills into the same MODEL / EFFORT submenus everywhere.

use crate::app::{App, ContextMenu, MenuAction, MenuItem, Overlay};
use crate::config::Config;
use crate::pull_request::{OpenPr, PrLaunch};
use crate::quick_prompt::QuickReturn;
use nebula_core::{Agent, AgentKind, WorktreeId};

/// Shown instead of a picker when a hand-edited config has switched every
/// harness off (the AGENTS TAB refuses to turn off the last one).
pub(crate) const NO_HARNESS_FLASH: &str =
    "every harness is disabled: enable one in Settings › Agents";

/// What one kind picker opens with: its title, where its rows launch, and
/// the context every row carries.
#[derive(Debug, Clone)]
pub(crate) struct KindPicker {
    pub title: String,
    pub worktree: WorktreeId,
    /// OPEN PRS launch context (a PR SESSION picker).
    pub pr: Option<PrLaunch>,
    /// The QUICK PROMPT box owed back (its `Tab` picker).
    pub quick: Option<Box<QuickReturn>>,
    /// The row to start on: the surface's own ask (a QUICK PROMPT picker
    /// opens on its box's harness); None leaves it to REMEMBER HARNESS
    /// (Settings → Experimental — the last launch's), else the first
    /// row, which one not offered any more falls to as well.
    pub hover: Option<HarnessRow>,
}

impl KindPicker {
    /// The NEW SESSION PICKER: plain rows into `worktree`.
    pub fn new_session(worktree: WorktreeId) -> Self {
        Self {
            title: "New session".into(),
            worktree,
            pr: None,
            quick: None,
            hover: None,
        }
    }

    /// The PR SESSION picker: every row carries the PR's URL and head
    /// branch. `worktree` is the PROJECT's ROOT WORKTREE — it names the
    /// PROJECT the create is addressed to, not where the session ends up;
    /// the DAEMON puts a PR SESSION in the head branch's own checkout.
    pub fn pr_session(worktree: WorktreeId, pr: &OpenPr) -> Self {
        Self {
            title: format!("New PR session · #{}", pr.number),
            worktree,
            pr: Some(PrLaunch::of(pr)),
            quick: None,
            hover: None,
        }
    }

    /// The QUICK PROMPT's `Tab` picker: every row hands the box back,
    /// and the cursor starts on the harness the box is already set to.
    /// `worktree` is the checkout the menu is built against, not where
    /// the launch lands — that stays the box's own `QuickLaunch::target`.
    pub fn quick_prompt(worktree: WorktreeId, back: QuickReturn) -> Self {
        Self {
            title: "Quick prompt agent".into(),
            ..Self::new_session_box(worktree, back)
        }
    }

    /// `n` on the grid: the NEW SESSION PICKER with no box up yet — the
    /// harness is asked first, and Enter on a row OPENS the QUICK PROMPT
    /// set to it (`back.from_box` is false: Esc closes the picker and
    /// opens nothing). The cursor starts on the harness the box would
    /// have opened on — the Settings → Agents one, which REMEMBER
    /// HARNESS writes the last launch into — so `Enter` at once is `p`.
    pub fn new_session_box(worktree: WorktreeId, back: QuickReturn) -> Self {
        Self {
            title: "New session".into(),
            worktree,
            pr: None,
            hover: Some(HarnessRow {
                kind: back.launch.kind,
                custom: back.launch.custom.clone(),
            }),
            quick: Some(Box::new(back)),
        }
    }
}

/// One launch row: a built-in kind, or a custom registry entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct HarnessRow {
    pub kind: AgentKind,
    /// Registry id when `kind` is [`AgentKind::Custom`].
    pub custom: Option<String>,
}

/// The harnesses the pickers offer — enabled built-ins plus usable custom
/// entries — or None, with the FLASH set, when a hand-edited config left
/// nothing to offer. An empty `ContextMenu` panics on Enter and `j`, so no
/// caller opens one.
pub(crate) fn enabled_harnesses_or_flash(app: &mut App, cfg: &Config) -> Option<Vec<HarnessRow>> {
    let rows: Vec<HarnessRow> = cfg
        .offered_harnesses()
        .into_iter()
        .map(|(kind, custom)| HarnessRow { kind, custom })
        .collect();
    if rows.is_empty() {
        app.chrome.flash = Some(NO_HARNESS_FLASH.into());
        return None;
    }
    Some(rows)
}

/// One `NewAgentOfKind` row per harness, labelled by `label`, every row
/// carrying the same launch context (`→` on any of them drills into that
/// kind's MODEL submenu with the context intact; customs offer no EFFORT
/// submenu — see `effort_choices`).
pub(crate) fn kind_rows(
    rows: &[HarnessRow],
    worktree: &WorktreeId,
    pr: Option<&PrLaunch>,
    quick: Option<&QuickReturn>,
    label: impl Fn(AgentKind, Option<&str>) -> String,
) -> Vec<MenuItem> {
    rows.iter()
        .map(|row| {
            // A QUICK PROMPT picker opens on the box as it stands: a box
            // already set to CLAUDE CLOUD shows its Claude row toggled.
            let cloud = row.kind == AgentKind::Claude
                && row.custom.is_none()
                && quick.is_some_and(|back| back.launch.cloud);
            let label = label(row.kind, row.custom.as_deref());
            MenuItem::new(
                if cloud {
                    format!("{label}{}", crate::app::CLOUD_LABEL)
                } else {
                    label
                },
                MenuAction::NewAgentOfKind {
                    worktree: worktree.clone(),
                    kind: row.kind,
                    custom: row.custom.clone(),
                    model: None,
                    effort: None,
                    cloud,
                    pr: pr.cloned(),
                    quick: quick.map(|back| Box::new(back.clone())),
                    profile: None,
                },
            )
        })
        .collect()
}

/// The harness rows with the ACCOUNTS each can launch on: a harness with
/// PROFILES says on its own row which account it takes —
/// `Claude · main ▰▰▱▱ 21% ▰▱▱▱ 3%`, or `Claude · auto → max2 …` under an
/// ACCOUNT SPREAD, which names none and leaves the pick to the DAEMON —
/// and gets a row under it for each other account (switched-off ones
/// last, marked off), so the account is chosen with the harness. The
/// usage bars are the ones the QUICK PROMPT's field shows. A harness
/// without profiles keeps its one row as it was.
pub(crate) fn with_account_rows(
    items: Vec<MenuItem>,
    profiles: &[nebula_core::profiles::Profile],
    policy: nebula_core::accounts::Policy,
    readings: &[nebula_core::usage::AccountUsage],
    project: &std::collections::BTreeMap<String, String>,
    now_secs: i64,
) -> Vec<MenuItem> {
    let mut out = Vec::with_capacity(items.len());
    for item in items {
        let MenuAction::NewAgentOfKind { kind, custom, .. } = &item.action else {
            out.push(item);
            continue;
        };
        let harness = custom.clone().unwrap_or_else(|| kind.as_str().to_string());
        // The project's default account for the harness ranks ahead of the
        // spread, as at the DAEMON.
        let project_default = project.get(&harness).map(String::as_str);
        let choices =
            crate::quick_prompt::profile_choices_in(profiles, &harness, policy, project_default);
        if choices.is_empty() {
            out.push(item);
            continue;
        }
        for choice in choices {
            let name = crate::quick_prompt::profile_choice_label(
                choice.as_deref(),
                profiles,
                &harness,
                policy,
                readings,
                project_default,
            );
            let on = crate::quick_prompt::resolved_profile(
                choice.as_deref(),
                profiles,
                &harness,
                policy,
                readings,
                project_default,
            );
            let key = nebula_core::usage::account_key(on.as_deref(), &harness);
            let reading = readings.iter().find(|u| u.account == key);
            let mut row = item.clone();
            row.label = format!(
                "{} · {}",
                item.label,
                crate::accounts_overlay::field_text(&name, reading, now_secs)
            );
            if let MenuAction::NewAgentOfKind { profile, .. } = &mut row.action {
                *profile = choice;
            }
            out.push(row);
        }
    }
    out
}

/// The label a picker row shows: the registry entry's label for a known
/// id (falling back to the id when it carries none), the kind name
/// otherwise — including a custom id the registry no longer names.
pub(crate) fn harness_label(kind: AgentKind, custom: Option<&str>) -> String {
    let cfg = Config::load();
    match (kind, custom) {
        (AgentKind::Custom, Some(id))
            if cfg.harness_registry().iter().any(|entry| entry.id == id) =>
        {
            cfg.effective_harness_by_id(id).display_label().to_string()
        }
        _ => kind_label(kind).to_string(),
    }
}

/// Open `picker` as the OVERLAY, or FLASH when no harness is enabled.
pub(crate) fn open_kind_picker(app: &mut App, picker: KindPicker) {
    let cfg = Config::load();
    let Some(rows) = enabled_harnesses_or_flash(app, &cfg) else {
        return;
    };
    let KindPicker {
        title,
        worktree,
        pr,
        quick,
        hover,
    } = picker;
    let mut items = kind_rows(
        &rows,
        &worktree,
        pr.as_ref(),
        quick.as_deref(),
        harness_label,
    );
    // A picker no box is up behind — `n`'s, whose pick opens the box, a
    // PR SESSION's, a row menu's — offers each account too; the QUICK
    // PROMPT's own `Tab` picker keeps the box's account (`^A`).
    if quick.as_ref().is_none_or(|back| !back.from_box) {
        crate::quick_prompt::refresh_profiles(app);
        let project = crate::quick_prompt::project_accounts(
            app,
            &crate::quick_prompt::QuickTarget::Worktree(worktree.clone()),
        );
        items = with_account_rows(
            items,
            &app.launcher.profiles,
            app.launcher.spread,
            &app.launcher.account_usage,
            &project,
            nebula_core::clock::now_secs() as i64,
        );
    }
    let hover = hover
        .or_else(|| {
            cfg.remembered_kind()
                .map(|kind| HarnessRow { kind, custom: None })
        })
        .and_then(|wanted| {
            items.iter().position(|item| {
                matches!(&item.action, MenuAction::NewAgentOfKind { kind, custom, .. }
                    if *kind == wanted.kind && custom.as_deref() == wanted.custom.as_deref())
            })
        })
        .unwrap_or(0);
    app.modals.overlay = Some(Overlay::Menu(ContextMenu {
        title: Some(title),
        items,
        at: None,
        hover,
        area: ratatui::layout::Rect::default(),
        scroll: 0,
        parent: None,
        filter: None,
    }));
}

/// The PR SESSION rows a CONTEXT MENU on a PROJECT OPEN PRS GROUP row
/// offers — `New Claude session`, `New Codex session`, … — one per enabled
/// harness, and none (no FLASH: the menu's other verbs still apply) when
/// every harness is off.
pub(crate) fn pr_session_menu_rows(worktree: WorktreeId, pr: &OpenPr) -> Vec<MenuItem> {
    let rows: Vec<HarnessRow> = Config::load()
        .offered_harnesses()
        .into_iter()
        .map(|(kind, custom)| HarnessRow { kind, custom })
        .collect();
    kind_rows(
        &rows,
        &worktree,
        Some(&PrLaunch::of(pr)),
        None,
        |kind, custom| format!("New {} session", harness_label(kind, custom)),
    )
}

/// The same badge against a config already in hand.
pub(crate) fn session_harness_badge_in(agent: &Agent, cfg: &Config) -> String {
    match (agent.kind, agent.custom_harness.as_deref()) {
        (AgentKind::Custom, Some(id)) => {
            cfg.effective_harness_by_id(id).display_label().to_string()
        }
        _ => agent.kind.as_str().to_string(),
    }
}

pub(crate) fn kind_label(kind: AgentKind) -> &'static str {
    match kind {
        AgentKind::Claude => "Claude",
        AgentKind::Codex => "Codex",
        AgentKind::Cursor => "Cursor",
        AgentKind::Pi => "Pi",
        AgentKind::Muse => "Muse",
        AgentKind::Grok => "Grok Build",
        AgentKind::OpenCode => "OpenCode",
        AgentKind::Copilot => "Copilot",
        // Custom rows label through `harness_label` (the entry's own
        // label); this is only the fallback when its entry is gone.
        AgentKind::Custom => "Custom",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::SubmenuKind;
    use crate::quick_prompt::QuickLaunch;

    const PR_URL: &str = "https://github.com/o/r/pull/7";
    const PR_HEAD: &str = "attach-links";

    /// Pin the config to a temp file holding `json`: every picker reads
    /// `Config::load`, and the dev's real file must stay out of it.
    fn pinned<T>(json: &str, f: impl FnOnce() -> T) -> T {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.json");
        std::fs::write(&path, json).unwrap();
        crate::config::with_config_path(path, f)
    }

    fn open_pr() -> OpenPr {
        OpenPr {
            number: 7,
            title: "Attach links".into(),
            url: PR_URL.into(),
            is_draft: false,
            health: Default::default(),
            head: PR_HEAD.into(),
            base: "main".into(),
        }
    }

    fn labels(menu: &ContextMenu) -> Vec<&str> {
        menu.items.iter().map(|item| item.label.as_str()).collect()
    }

    fn row_profile(item: &MenuItem) -> Option<String> {
        match &item.action {
            MenuAction::NewAgentOfKind { profile, .. } => profile.clone(),
            other => panic!("{other:?}"),
        }
    }

    /// `n` picks the account with the harness: the harness's own row
    /// takes the default (named, so the label never moves), a row under
    /// it each other account, the switched-off one last; a model picked
    /// from an account's row keeps that account. A harness with no
    /// profiles keeps its one row.
    #[test]
    fn the_new_session_picker_offers_each_account() {
        let json = r#"{"profiles": [
            {"name": "main", "harness": "claude", "default": true},
            {"name": "old", "harness": "claude", "disabled": true},
            {"name": "max2", "harness": "claude"}
        ]}"#;
        pinned(json, || {
            let mut app = App::new();
            open_kind_picker(&mut app, KindPicker::new_session(WorktreeId("w1".into())));
            let Some(Overlay::Menu(menu)) = &app.modals.overlay else {
                panic!("{:?}", app.modals.overlay);
            };
            assert_eq!(
                labels(menu)[..4],
                [
                    "Claude · main",
                    "Claude · max2",
                    "Claude · old (off)",
                    "Codex"
                ]
            );
            let profiles: Vec<_> = menu.items[..4].iter().map(row_profile).collect();
            assert_eq!(
                profiles,
                [
                    Some("main".to_string()),
                    Some("max2".to_string()),
                    Some("old".to_string()),
                    None
                ]
            );
            let models = crate::event_loop::build_submenu(&menu.items[1]).expect("models");
            assert!(models
                .items
                .iter()
                .all(|item| row_profile(item).as_deref() == Some("max2")));
        });

        // Under an ACCOUNT SPREAD the harness's row is "auto", naming none.
        let json = r#"{"account_spread": "least_used",
            "profiles": [{"name": "main", "harness": "claude", "default": true}]}"#;
        pinned(json, || {
            let mut app = App::new();
            open_kind_picker(&mut app, KindPicker::new_session(WorktreeId("w1".into())));
            let Some(Overlay::Menu(menu)) = &app.modals.overlay else {
                panic!("{:?}", app.modals.overlay);
            };
            assert_eq!(labels(menu)[..2], ["Claude · auto → main", "Claude · main"]);
            assert_eq!(row_profile(&menu.items[0]), None);
        });
    }

    /// Every row is the same launch context under a different harness, so
    /// a submenu drilled from any row keeps that context — including the
    /// custom registry id.
    #[test]
    fn kind_rows_carry_the_launch_context_into_every_row() {
        let worktree = WorktreeId("w1".into());
        let pr = PrLaunch::of(&open_pr());
        let harness_rows: Vec<HarnessRow> = AgentKind::ALL
            .into_iter()
            .filter(|kind| *kind != AgentKind::Custom)
            .map(|kind| HarnessRow { kind, custom: None })
            .chain([HarnessRow {
                kind: AgentKind::Custom,
                custom: Some("agy".into()),
            }])
            .collect();
        let rows = kind_rows(&harness_rows, &worktree, Some(&pr), None, |kind, custom| {
            format!("New {} session", harness_label(kind, custom))
        });
        assert!(rows.iter().any(|row| row.label == "New Codex session"));
        assert!(rows.iter().any(|row| row.label == "New Custom session"));
        for (row, expected) in rows.iter().zip(harness_rows.iter()) {
            assert!(
                matches!(
                    &row.action,
                    MenuAction::NewAgentOfKind {
                        worktree,
                        kind,
                        custom,
                        model: None,
                        effort: None,
                        cloud: false,
                        pr: Some(pr),
                        quick: None,
                        profile: None,
                    } if worktree.as_str() == "w1"
                        && *kind == expected.kind
                        && custom.as_deref() == expected.custom.as_deref()
                        && pr.url == PR_URL
                        && pr.head == PR_HEAD
                ),
                "{row:?}"
            );
            assert_eq!(row.action.submenu(), Some(SubmenuKind::Models));
        }
    }

    /// Custom entries ride after the built-ins under their own labels;
    /// disabled, invalid and (under the hide switch) missing ones stay
    /// out, and their rows carry the registry id into the launch.
    #[test]
    fn picker_lists_usable_custom_entries_after_the_builtins() {
        let json = r#"{"custom_harnesses": [
            {"id": "agy", "label": "Agy", "program": "agy"},
            {"id": "off", "label": "Off", "program": "off", "enabled": false},
            {"id": "broken", "label": "Broken", "program": ""}
        ]}"#;
        pinned(json, || {
            let worktree = WorktreeId("w1".into());
            let mut app = App::new();
            open_kind_picker(&mut app, KindPicker::new_session(worktree));
            let Some(Overlay::Menu(menu)) = &app.modals.overlay else {
                panic!("{:?}", app.modals.overlay);
            };
            let names = labels(menu);
            assert_eq!(names.last(), Some(&"Agy"));
            assert!(!names.contains(&"Off"), "{names:?}");
            assert!(!names.contains(&"Broken"), "{names:?}");
            let agy = menu.items.iter().find(|item| item.label == "Agy").unwrap();
            assert!(matches!(
                &agy.action,
                MenuAction::NewAgentOfKind {
                    kind: AgentKind::Custom,
                    custom: Some(id),
                    ..
                } if id == "agy"
            ));
            assert_eq!(agy.action.submenu(), Some(SubmenuKind::Models));
        });
    }

    /// The Agents tab's per-entry Enabled row is the toggle now: flip
    /// it, save, and the picker drops the entry until it flips back.
    #[test]
    fn agents_tab_toggles_custom_entries_off_the_picker() {
        use crate::config::{locate_agent, HarnessField};
        let json = r#"{"custom_harnesses": [
            {"id": "agy", "label": "Agy", "program": "agy"}
        ]}"#;
        pinned(json, || {
            let worktree = WorktreeId("w1".into());
            let mut app = App::new();
            open_kind_picker(&mut app, KindPicker::new_session(worktree.clone()));
            let Some(Overlay::Menu(menu)) = &app.modals.overlay else {
                panic!("{:?}", app.modals.overlay);
            };
            assert!(labels(menu).contains(&"Agy"));

            let mut cfg = Config::load();
            let (tab, row) = locate_agent("agy", HarnessField::Enabled).unwrap();
            cfg.cycle(tab, row, 0);
            cfg.save().unwrap();

            open_kind_picker(&mut app, KindPicker::new_session(worktree));
            let Some(Overlay::Menu(menu)) = &app.modals.overlay else {
                panic!("{:?}", app.modals.overlay);
            };
            assert!(!labels(menu).contains(&"Agy"), "{:?}", labels(menu));

            let mut cfg = Config::load();
            let (tab, row) = locate_agent("agy", HarnessField::Enabled).unwrap();
            cfg.cycle(tab, row, 0);
            cfg.save().unwrap();
            assert!(
                Config::load()
                    .offered_harnesses()
                    .contains(&(AgentKind::Custom, Some("agy".into()))),
                "flipping back re-offers the entry"
            );
        });
    }

    /// The three surfaces differ only in title, context and starting row;
    /// a harness switched off on the AGENTS TAB is missing from all of them.
    #[test]
    fn every_surface_opens_the_same_rows_minus_disabled_harnesses() {
        pinned(r#"{"codex_enabled": false}"#, || {
            let worktree = WorktreeId("w1".into());
            let mut app = App::new();
            // Every built-in harness but the one switched off, in the ALL
            // order. A bare Custom kind is never offered (entries come
            // from the registry), and the pinned config defines none.
            let offered: Vec<&str> = AgentKind::ALL
                .iter()
                .filter(|kind| **kind != AgentKind::Codex && **kind != AgentKind::Custom)
                .map(|kind| kind_label(*kind))
                .collect();
            assert!(offered.starts_with(&["Claude", "Cursor"]), "{offered:?}");

            open_kind_picker(&mut app, KindPicker::new_session(worktree.clone()));
            let Some(Overlay::Menu(menu)) = &app.modals.overlay else {
                panic!("{:?}", app.modals.overlay);
            };
            assert_eq!(menu.title.as_deref(), Some("New session"));
            assert_eq!(labels(menu), offered);
            assert_eq!(menu.hover, 0);

            open_kind_picker(
                &mut app,
                KindPicker::pr_session(worktree.clone(), &open_pr()),
            );
            let Some(Overlay::Menu(menu)) = &app.modals.overlay else {
                panic!("{:?}", app.modals.overlay);
            };
            assert_eq!(menu.title.as_deref(), Some("New PR session · #7"));
            assert_eq!(labels(menu), offered);
            assert!(menu.items.iter().all(|item| matches!(
                &item.action,
                MenuAction::NewAgentOfKind { pr: Some(pr), quick: None, .. }
                    if pr.url == PR_URL && pr.head == PR_HEAD
            )));

            let back = QuickReturn {
                launch: QuickLaunch {
                    profile: None,
                    target: crate::quick_prompt::QuickTarget::Worktree(worktree.clone()),
                    kind: AgentKind::Cursor,
                    custom: None,
                    model: None,
                    effort: None,
                    preset: None,
                    issue: None,
                    pr: None,
                    under: None,
                    cloud: false,
                },
                text: "typed so far".into(),
                from_box: true,
            };
            open_kind_picker(
                &mut app,
                KindPicker::quick_prompt(worktree.clone(), back.clone()),
            );
            let Some(Overlay::Menu(menu)) = &app.modals.overlay else {
                panic!("{:?}", app.modals.overlay);
            };
            assert_eq!(menu.title.as_deref(), Some("Quick prompt agent"));
            assert_eq!(labels(menu), offered);
            assert_eq!(menu.hover, 1, "starts on the box's own harness");
            assert!(menu.items.iter().all(|item| matches!(
                &item.action,
                MenuAction::NewAgentOfKind { pr: None, quick: Some(back), .. }
                    if back.text == "typed so far"
            )));

            // `n` on the grid: the same rows under the NEW SESSION title,
            // owing a box that is not up yet.
            open_kind_picker(
                &mut app,
                KindPicker::new_session_box(
                    worktree.clone(),
                    QuickReturn {
                        text: String::new(),
                        from_box: false,
                        ..back
                    },
                ),
            );
            let Some(Overlay::Menu(menu)) = &app.modals.overlay else {
                panic!("{:?}", app.modals.overlay);
            };
            assert_eq!(menu.title.as_deref(), Some("New session"));
            assert_eq!(labels(menu), offered);
            assert_eq!(menu.hover, 1, "starts on the harness the box would open on");
            assert!(menu.items.iter().all(|item| matches!(
                &item.action,
                MenuAction::NewAgentOfKind { pr: None, quick: Some(back), .. }
                    if !back.from_box && back.text.is_empty()
            )));

            let rows = pr_session_menu_rows(worktree, &open_pr());
            let names: Vec<String> = rows.iter().map(|row| row.label.clone()).collect();
            let expected: Vec<String> = offered
                .iter()
                .map(|label| format!("New {label} session"))
                .collect();
            assert_eq!(names, expected);
        });
    }

    /// The QUICK PROMPT's picker offers the NEW SESSION PICKER's cloud
    /// toggle on its Claude row, and opens with it on for a box already set
    /// to cloud — but not for a box the DAEMON would refuse a cloud task
    /// for, one carrying a pull request or an issue.
    #[test]
    fn the_quick_prompt_picker_toggles_cloud_where_the_box_can_go() {
        pinned("{}", || {
            let worktree = WorktreeId("w1".into());
            let claude = QuickLaunch::of_kind(
                crate::quick_prompt::QuickTarget::Worktree(worktree.clone()),
                AgentKind::Claude,
                None,
                None,
                None,
                &Config::load(),
            );
            let open = |app: &mut App, launch: QuickLaunch| -> ContextMenu {
                let back = QuickReturn {
                    launch,
                    text: "typed so far".into(),
                    from_box: true,
                };
                open_kind_picker(app, KindPicker::quick_prompt(worktree.clone(), back));
                match &app.modals.overlay {
                    Some(Overlay::Menu(menu)) => menu.clone(),
                    other => panic!("{other:?}"),
                }
            };
            let mut app = App::new();

            let mut menu = open(&mut app, claude.clone());
            assert_eq!(menu.items[menu.hover].label, "Claude");
            assert_eq!(menu.hovered_claude_cloud(), Some(false));
            assert!(menu.toggle_hovered_claude_cloud());
            assert_eq!(menu.items[menu.hover].label, "Claude · cloud");

            let menu = open(&mut app, claude.clone().with_cloud(true));
            assert_eq!(menu.items[menu.hover].label, "Claude · cloud");
            assert_eq!(menu.hovered_claude_cloud(), Some(true));

            let mut menu = open(&mut app, claude.with_pr(Some(PrLaunch::of(&open_pr()))));
            assert_eq!(menu.hovered_claude_cloud(), None, "no cloud for a PR");
            assert!(!menu.toggle_hovered_claude_cloud());
            assert_eq!(menu.items[menu.hover].label, "Claude");
        });
    }

    /// REMEMBER HARNESS on: the NEW SESSION and PR SESSION pickers open on
    /// the last launch's harness — the `quick_prompt_kind` it wrote; off,
    /// on the first row whatever that setting says. A remembered harness
    /// switched off since steps to the first enabled one, and the QUICK
    /// PROMPT's picker still opens on its own box's harness.
    #[test]
    fn pickers_open_on_the_remembered_harness_only_while_the_switch_is_on() {
        let worktree = WorktreeId("w1".into());
        let pr = open_pr();
        fn hover_of(app: &App) -> usize {
            match &app.modals.overlay {
                Some(Overlay::Menu(menu)) => menu.hover,
                other => panic!("expected a picker, got {other:?}"),
            }
        }

        pinned(r#"{"quick_prompt_kind": "cursor"}"#, || {
            let mut app = App::new();
            open_kind_picker(&mut app, KindPicker::new_session(worktree.clone()));
            assert_eq!(
                hover_of(&app),
                0,
                "off: the quick prompt's harness is its own"
            );
        });

        pinned(
            r#"{"remember_harness": true, "quick_prompt_kind": "cursor"}"#,
            || {
                let mut app = App::new();
                open_kind_picker(&mut app, KindPicker::new_session(worktree.clone()));
                assert_eq!(hover_of(&app), 2, "on: Cursor, the last launch's");
                open_kind_picker(&mut app, KindPicker::pr_session(worktree.clone(), &pr));
                assert_eq!(hover_of(&app), 2, "the PR SESSION picker too");

                let back = QuickReturn {
                    launch: QuickLaunch {
                        profile: None,
                        target: crate::quick_prompt::QuickTarget::Worktree(worktree.clone()),
                        kind: AgentKind::Codex,
                        custom: None,
                        model: None,
                        effort: None,
                        preset: None,
                        issue: None,
                        pr: None,
                        under: None,
                        cloud: false,
                    },
                    text: String::new(),
                    from_box: true,
                };
                open_kind_picker(&mut app, KindPicker::quick_prompt(worktree.clone(), back));
                assert_eq!(
                    hover_of(&app),
                    1,
                    "the box's own harness outranks the remembered one"
                );
            },
        );

        pinned(
            r#"{"remember_harness": true, "quick_prompt_kind": "cursor", "cursor_enabled": false}"#,
            || {
                let mut app = App::new();
                open_kind_picker(&mut app, KindPicker::new_session(worktree.clone()));
                let Some(Overlay::Menu(menu)) = &app.modals.overlay else {
                    panic!("{:?}", app.modals.overlay);
                };
                assert_eq!(
                    labels(menu),
                    [
                        "Claude",
                        "Codex",
                        "Pi",
                        "Muse",
                        "Grok Build",
                        "OpenCode",
                        "Copilot"
                    ]
                );
                assert_eq!(
                    menu.hover, 0,
                    "a remembered harness switched off steps to the first enabled"
                );
            },
        );
    }

    /// Only a hand-edited config reaches an empty list: the picker flashes
    /// instead of opening, and a CONTEXT MENU just has no PR SESSION rows.
    #[test]
    fn no_harness_flashes_the_picker_and_empties_the_menu_rows() {
        pinned(
            r#"{"claude_enabled": false, "codex_enabled": false, "cursor_enabled": false, "pi_enabled": false, "muse_enabled": false, "opencode_enabled": false, "harnesses":{"grok":{"enabled":false},"copilot":{"enabled":false}}}"#,
            || {
                let worktree = WorktreeId("w1".into());
                let mut app = App::new();
                open_kind_picker(&mut app, KindPicker::new_session(worktree.clone()));
                assert!(app.modals.overlay.is_none(), "{:?}", app.modals.overlay);
                assert_eq!(app.chrome.flash.as_deref(), Some(NO_HARNESS_FLASH));
                assert!(pr_session_menu_rows(worktree, &open_pr()).is_empty());
            },
        );
    }
}
