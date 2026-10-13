//! The settings as the desktop window renders them: every tab, group and
//! row the TUI's overlay declares ([`crate::SETTINGS_TABS`]), each with a
//! control, its current and default value, and the [`Target`] a write
//! sends back — plus the window's own Desktop tab. The window draws what
//! this exports rather than a copy of the rows, so a row added to the TUI
//! shows up there too; [`TERMINAL_ONLY`] says which rows only mean
//! something to the terminal app, and they are shown under a "Terminal
//! app" group rather than mixed in with the rest.

use crate::desktop::DesktopPrefs;
use crate::{
    agent_kind_names, choices, effort_choices_in, model_choices_in, Config, HarnessField,
    SettingKind, SettingSpec, TabBody, AGENTS_HEAD, DEFAULT_CHOICE, DIFF_CONTEXTS, EDITORS,
    INLINE_GRAPHICS, KANBAN_SCOPES, PRESET_TEXTS, PROJECT_FILE_CHOICE, SESSION_IDLE_TIMEOUTS,
    SETTINGS_TABS, SOUNDS, WORKTREE_LAYOUTS,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// Rows that only mean something to the terminal app — its search, file
/// editor, sounds, grid layout, card chrome, colours and quick-prompt
/// focus. The window shows them under a "Terminal app" group in their tab,
/// still editable, since they are the same file.
pub const TERMINAL_ONLY: &[SettingKind] = &[
    SettingKind::PaletteEnterAttaches,
    SettingKind::Editor,
    SettingKind::CloseFinderOnOpen,
    SettingKind::DoneSound,
    SettingKind::FeedbackSound,
    SettingKind::PresetText,
    SettingKind::DeleteEmptyWorktree,
    SettingKind::AutoCleanupMerged,
    SettingKind::ShowAllWorktrees,
    SettingKind::AskBeforeArchive,
    SettingKind::ConfirmDragMove,
    SettingKind::Theme,
    SettingKind::Icons,
    SettingKind::Animations,
    SettingKind::BlackBackground,
    SettingKind::HideCardMarks,
    SettingKind::HighlightCurrentCard,
    SettingKind::SourceControlGraph,
    SettingKind::DiffMode,
    SettingKind::DiffContext,
    SettingKind::SessionPane,
    SettingKind::WorktreeLayout,
    SettingKind::Kanban,
    SettingKind::KanbanScope,
    SettingKind::InlineGraphics,
    SettingKind::ExpandAllWorktrees,
    SettingKind::CardIssueNumber,
    SettingKind::HideDraftPrs,
    SettingKind::QuickPromptFocus,
    SettingKind::FollowNewSession,
    SettingKind::QuickPromptNewWorktree,
    SettingKind::RememberHarness,
];

/// Rows the window leaves out altogether. Empty: every row the overlay
/// declares is rendered, shared or terminal-only. The parity test holds
/// the two lists to the tabs.
pub const DESKTOP_EXCLUDED: &[SettingKind] = &[];

/// The browsers a project's localhost preview can remember (`w` in the
/// terminal app); empty is none remembered.
pub const BROWSERS: &[&str] = &["chrome", "edge", "firefox", "default"];

impl SettingKind {
    /// The config.json key the row reads and writes. A Project tab row's
    /// key is inside the project's `projects` entry.
    pub fn key(self) -> &'static str {
        match self {
            SettingKind::PaletteEnterAttaches => "palette_enter_attaches",
            SettingKind::WorktreeBaseBranch => "worktree_base_branch",
            SettingKind::Editor => "editor",
            SettingKind::CloseFinderOnOpen => "close_finder_on_open",
            SettingKind::SshSyncConfig => "ssh_sync_config",
            SettingKind::SessionIdleTimeout => "session_idle_timeout",
            SettingKind::PrewarmAgents => "prewarm_agents",
            SettingKind::PrewarmSessions => "prewarm_sessions",
            SettingKind::DoneSound => "done_sound",
            SettingKind::FeedbackSound => "feedback_sound",
            SettingKind::PresetText => "preset_text",
            SettingKind::DeleteEmptyWorktree => "delete_empty_worktree",
            SettingKind::AutoCleanupMerged => "auto_cleanup_merged",
            SettingKind::ShowAllWorktrees => "show_all_worktrees",
            SettingKind::Theme => "theme",
            SettingKind::Icons => "icons",
            SettingKind::Animations => "animations",
            SettingKind::BlackBackground => "black_background",
            SettingKind::HideCardMarks => "hide_card_marks",
            SettingKind::HighlightCurrentCard => "highlight_current_card",
            SettingKind::SourceControlGraph => "source_control_graph",
            SettingKind::DiffMode => "diff_mode",
            SettingKind::DiffContext => "diff_context",
            SettingKind::SessionPane => "session_pane",
            SettingKind::WorktreeLayout => "worktree_layout",
            SettingKind::Kanban => "kanban",
            SettingKind::KanbanScope => "kanban_scope",
            SettingKind::InlineGraphics => "inline_graphics",
            SettingKind::ExpandAllWorktrees => "expand_all_worktrees",
            SettingKind::CardIssueNumber => "card_issue_number",
            SettingKind::HideDraftPrs => "hide_draft_prs",
            SettingKind::QuickPromptKind => "quick_prompt_kind",
            SettingKind::QuickPromptFocus => "quick_prompt_focus",
            SettingKind::FollowNewSession => "follow_new_session",
            SettingKind::QuickPromptNewWorktree => "quick_prompt_new_worktree",
            SettingKind::RunCommand => "run_command",
            SettingKind::OpenCommand => "open_command",
            SettingKind::RememberHarness => "remember_harness",
            SettingKind::HideUninstalledHarnesses => "hide_uninstalled_harnesses",
            SettingKind::AskBeforeArchive => "ask_before_archive",
            SettingKind::ConfirmDragMove => "confirm_drag_move",
        }
    }
}

/// Where a write lands, as the window sends it back.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "scope", rename_all = "snake_case")]
pub enum Target {
    /// A top-level config.json key ([`SettingKind::key`]).
    Global { key: String },
    /// An Agents tab harness row: `field` is enabled, model or effort.
    Agent { harness: String, field: String },
    /// A Project tab row of the project at `path` (its repo path as the
    /// daemon stores it): run_command, open_command or browser.
    Project { path: String, field: String },
    /// One of the window's own preferences ([`crate::desktop`]).
    Desktop { key: String },
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Control {
    Toggle,
    /// `labels` says each option for display, index for index.
    Choice {
        options: Vec<String>,
        labels: Vec<String>,
    },
    Text {
        placeholder: String,
    },
}

#[derive(Debug, Clone, Serialize)]
pub struct Row {
    pub target: Target,
    pub label: String,
    pub hint: String,
    pub control: Control,
    /// A bool for a toggle, a string otherwise.
    pub value: Value,
    pub default: Value,
    /// Set when a running terminal app won't see a change until it
    /// restarts. None on every row: a running TUI reloads config.json
    /// live (its config beat), mirrored values and hotkeys included.
    pub restart: Option<String>,
    /// Shipped in the last week, as the overlay's `(new)` says.
    pub is_new: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct Group {
    /// Empty for a tab's untitled first group.
    pub title: String,
    pub subtitle: Option<String>,
    /// The "Terminal app" group: rows only that app acts on.
    pub terminal: bool,
    pub rows: Vec<Row>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Hotkey {
    pub id: String,
    pub label: String,
    pub hint: String,
    pub group: String,
    pub defaults: Vec<String>,
    /// What the key is bound to now: the defaults, or the override.
    pub bound: Vec<String>,
    pub overridden: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct Tab {
    pub id: String,
    pub title: String,
    pub note: Option<String>,
    pub groups: Vec<Group>,
    /// The Hotkeys tab's rows; empty on every other tab.
    pub hotkeys: Vec<Hotkey>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Schema {
    pub tabs: Vec<Tab>,
}

/// A project the daemon knows, for the Project tab.
#[derive(Debug, Clone)]
pub struct ProjectRef {
    pub name: String,
    pub path: PathBuf,
}

const TERMINAL_GROUP: &str = "Terminal app";

/// Toggle rows: the bool field they flip.
fn flag(cfg: &mut Config, kind: SettingKind) -> Option<&mut bool> {
    Some(match kind {
        SettingKind::PaletteEnterAttaches => &mut cfg.palette_enter_attaches,
        SettingKind::CloseFinderOnOpen => &mut cfg.close_finder_on_open,
        SettingKind::SshSyncConfig => &mut cfg.ssh_sync_config,
        SettingKind::PrewarmAgents => &mut cfg.prewarm_agents,
        SettingKind::PrewarmSessions => &mut cfg.prewarm_sessions,
        SettingKind::DeleteEmptyWorktree => &mut cfg.delete_empty_worktree,
        SettingKind::AutoCleanupMerged => &mut cfg.auto_cleanup_merged,
        SettingKind::ShowAllWorktrees => &mut cfg.show_all_worktrees,
        SettingKind::AskBeforeArchive => &mut cfg.ask_before_archive,
        SettingKind::ConfirmDragMove => &mut cfg.confirm_drag_move,
        SettingKind::Animations => &mut cfg.animations,
        SettingKind::BlackBackground => &mut cfg.black_background,
        SettingKind::HighlightCurrentCard => &mut cfg.highlight_current_card,
        SettingKind::SourceControlGraph => &mut cfg.source_control_graph,
        SettingKind::Kanban => &mut cfg.kanban,
        SettingKind::ExpandAllWorktrees => &mut cfg.expand_all_worktrees,
        SettingKind::CardIssueNumber => &mut cfg.card_issue_number,
        SettingKind::QuickPromptFocus => &mut cfg.quick_prompt_focus,
        SettingKind::FollowNewSession => &mut cfg.follow_new_session,
        SettingKind::QuickPromptNewWorktree => &mut cfg.quick_prompt_new_worktree,
        SettingKind::RememberHarness => &mut cfg.remember_harness,
        SettingKind::HideUninstalledHarnesses => &mut cfg.hide_uninstalled_harnesses,
        _ => return None,
    })
}

fn strs(list: &[&str]) -> Vec<String> {
    list.iter().map(|s| s.to_string()).collect()
}

/// Choice rows: what they offer. A hand-edited value off the list joins
/// the end, so the row still shows what the file holds.
fn choice_options(cfg: &Config, kind: SettingKind) -> Option<Vec<String>> {
    let (mut list, current) = match kind {
        SettingKind::Editor => (strs(EDITORS), cfg.editor.clone()),
        SettingKind::SessionIdleTimeout => (
            strs(SESSION_IDLE_TIMEOUTS),
            cfg.session_idle_timeout.clone(),
        ),
        SettingKind::DoneSound => (strs(SOUNDS), cfg.done_sound.clone()),
        SettingKind::FeedbackSound => (strs(SOUNDS), cfg.feedback_sound.clone()),
        SettingKind::PresetText => (strs(PRESET_TEXTS), String::new()),
        SettingKind::Theme => (strs(choices::THEMES), cfg.theme.clone()),
        SettingKind::Icons => (strs(choices::ICON_STYLES), String::new()),
        SettingKind::SessionPane => (strs(choices::PANE_SIDES), String::new()),
        SettingKind::WorktreeLayout => (strs(WORKTREE_LAYOUTS), String::new()),
        SettingKind::KanbanScope => (strs(KANBAN_SCOPES), String::new()),
        SettingKind::InlineGraphics => (strs(INLINE_GRAPHICS), String::new()),
        SettingKind::DiffMode => (
            choices::DiffMode::ALL
                .iter()
                .map(|m| m.as_str().to_string())
                .collect(),
            String::new(),
        ),
        SettingKind::DiffContext => (strs(DIFF_CONTEXTS), String::new()),
        SettingKind::HideCardMarks | SettingKind::HideDraftPrs => {
            (strs(&["shown", "hidden"]), String::new())
        }
        SettingKind::QuickPromptKind => (agent_kind_names(), cfg.quick_prompt_kind.clone()),
        _ => return None,
    };
    let current = current.trim();
    if !current.is_empty() && !crate::fits(current, &list) {
        list.push(current.to_string());
    }
    Some(list)
}

fn control(cfg: &Config, kind: SettingKind) -> Control {
    if let Some(options) = choice_options(cfg, kind) {
        // The quick prompt's harness reads by its label (Claude Code),
        // and stores its id.
        let labels = if kind == SettingKind::QuickPromptKind {
            options
                .iter()
                .map(|id| cfg.effective_harness_by_id(id).display_label().to_string())
                .collect()
        } else {
            options.clone()
        };
        return Control::Choice { labels, options };
    }
    if kind.is_text() {
        let placeholder = if kind.is_project() {
            PROJECT_FILE_CHOICE
        } else {
            crate::AUTO_CHOICE
        };
        return Control::Text {
            placeholder: placeholder.into(),
        };
    }
    Control::Toggle
}

/// A top-level row's value: a bool for a toggle, the stored text for a
/// text row, the word the overlay shows for a choice.
pub fn get_value(cfg: &Config, kind: SettingKind) -> Value {
    let mut copy = cfg.clone();
    if let Some(on) = flag(&mut copy, kind) {
        return Value::Bool(*on);
    }
    if kind.is_text() {
        return Value::String(cfg.text_value(kind));
    }
    Value::String(cfg.value_label(kind))
}

/// Write a top-level row: a bool into a toggle, any text into a text row
/// (trimmed; empty is the default), one of the offered words into a choice.
pub fn set_value(cfg: &mut Config, kind: SettingKind, value: &Value) -> Result<(), String> {
    if let Some(slot) = flag(cfg, kind) {
        *slot = value
            .as_bool()
            .ok_or_else(|| format!("{} is on or off", kind.key()))?;
        return Ok(());
    }
    let text = value
        .as_str()
        .ok_or_else(|| format!("{} takes text", kind.key()))?;
    if kind.is_text() {
        if kind.is_project() {
            return Err(format!("{} belongs to a project", kind.key()));
        }
        if cfg.text_value(kind) != text.trim() {
            cfg.set_text(kind, text);
        }
        return Ok(());
    }
    let options =
        choice_options(cfg, kind).ok_or_else(|| format!("{} is not a row", kind.key()))?;
    let word = options
        .iter()
        .find(|o| o.eq_ignore_ascii_case(text.trim()))
        .ok_or_else(|| format!("{} is one of {}", kind.key(), options.join(", ")))?
        .clone();
    // The word the row already shows changes nothing: an unset key the
    // overlay reads as its default (an empty diff_mode) stays unset.
    if cfg.value_label(kind).eq_ignore_ascii_case(&word) {
        return Ok(());
    }
    match kind {
        SettingKind::Editor => cfg.editor = word,
        SettingKind::SessionIdleTimeout => cfg.session_idle_timeout = word,
        SettingKind::DoneSound => cfg.done_sound = word,
        SettingKind::FeedbackSound => cfg.feedback_sound = word,
        SettingKind::PresetText => cfg.preset_text = word,
        SettingKind::Theme => cfg.theme = word,
        SettingKind::Icons => cfg.icons = word,
        SettingKind::SessionPane => cfg.session_pane = word,
        SettingKind::WorktreeLayout => cfg.worktree_layout = word,
        SettingKind::KanbanScope => cfg.kanban_scope = word,
        SettingKind::InlineGraphics => cfg.inline_graphics = word,
        SettingKind::DiffMode => cfg.diff_mode = word,
        SettingKind::DiffContext => cfg.diff_context = word,
        SettingKind::HideCardMarks => cfg.hide_card_marks = word == "hidden",
        SettingKind::HideDraftPrs => cfg.hide_draft_prs = word == "hidden",
        SettingKind::QuickPromptKind => cfg.quick_prompt_kind = word,
        _ => return Err(format!("{} is not a choice", kind.key())),
    }
    Ok(())
}

/// Every top-level row the overlay declares, tab by tab: the Values tabs'
/// rows and the Agents tab's head. The Project tab's rows are per project.
fn global_kinds() -> impl Iterator<Item = &'static SettingSpec> {
    SETTINGS_TABS.iter().flat_map(|tab| match tab.body {
        TabBody::Values(rows) => rows,
        TabBody::Agents => AGENTS_HEAD,
        TabBody::Project(_) | TabBody::Hotkeys => &[],
    })
}

/// The row by its config key.
pub fn kind_for_key(key: &str) -> Option<SettingKind> {
    global_kinds()
        .map(|spec| spec.kind)
        .find(|kind| kind.key() == key)
}

fn global_row(cfg: &Config, spec: &SettingSpec, today: i64) -> Row {
    let defaults = Config::default();
    Row {
        target: Target::Global {
            key: spec.kind.key().into(),
        },
        label: spec.label.into(),
        hint: spec.hint.into(),
        control: control(cfg, spec.kind),
        value: get_value(cfg, spec.kind),
        default: get_value(&defaults, spec.kind),
        restart: None,
        is_new: spec.kind.is_new(today),
    }
}

/// A Values tab (or the Agents head) laid out: its own groups in order,
/// then the terminal-only rows under one "Terminal app" group.
fn value_groups(cfg: &Config, specs: &[SettingSpec], today: i64) -> Vec<Group> {
    let mut groups: Vec<Group> = Vec::new();
    let mut terminal = Vec::new();
    for spec in specs {
        if DESKTOP_EXCLUDED.contains(&spec.kind) {
            continue;
        }
        let row = global_row(cfg, spec, today);
        if TERMINAL_ONLY.contains(&spec.kind) {
            terminal.push(row);
            continue;
        }
        match groups.last_mut() {
            Some(g) if g.title == spec.group => g.rows.push(row),
            _ => groups.push(Group {
                title: spec.group.into(),
                subtitle: None,
                terminal: false,
                rows: vec![row],
            }),
        }
    }
    if !terminal.is_empty() {
        groups.push(Group {
            title: TERMINAL_GROUP.into(),
            subtitle: Some(
                "Only the terminal app acts on these. They share its config file.".into(),
            ),
            terminal: true,
            rows: terminal,
        });
    }
    groups
}

fn field_name(field: HarnessField) -> &'static str {
    match field {
        HarnessField::Enabled => "enabled",
        HarnessField::Model => "model",
        HarnessField::Effort => "effort",
    }
}

fn parse_field(field: &str) -> Result<HarnessField, String> {
    match field {
        "enabled" => Ok(HarnessField::Enabled),
        "model" => Ok(HarnessField::Model),
        "effort" => Ok(HarnessField::Effort),
        other => Err(format!("no harness field {other}")),
    }
}

/// What a launch picker offers for one harness, from the same lists the
/// Agents tab rows use: the models (`default` first), the configured model,
/// the efforts each model takes (Cursor's follow the model; others are one
/// list), and the configured effort (`n/a` when the harness has none).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct LaunchChoices {
    pub models: Vec<String>,
    pub model: String,
    pub efforts: BTreeMap<String, Vec<String>>,
    pub effort: String,
}

pub fn launch_choices(cfg: &Config, id: &str) -> LaunchChoices {
    let descriptor = cfg.effective_harness_by_id(id);
    let mut models = model_choices_in(&descriptor);
    let model = descriptor.model.default.trim().to_string();
    if !model.is_empty() && !crate::fits(&model, &models) {
        models.push(model.clone());
    }
    let efforts = models
        .iter()
        .map(|m| {
            let pick = (!m.eq_ignore_ascii_case(DEFAULT_CHOICE)).then_some(m.as_str());
            let pick = pick.or(descriptor.default_model());
            (m.clone(), effort_choices_in(&descriptor, pick))
        })
        .collect();
    LaunchChoices {
        models,
        model,
        efforts,
        effort: cfg.agent_value(id, HarnessField::Effort),
    }
}

fn agent_groups(cfg: &Config) -> Vec<Group> {
    let mut groups = Vec::new();
    let mut current: Option<Group> = None;
    for (id, field) in cfg.agent_rows() {
        let descriptor = cfg.effective_harness_by_id(&id);
        let title = descriptor.display_label().to_string();
        if current.as_ref().is_none_or(|g| g.title != title) {
            groups.extend(current.take());
            current = Some(Group {
                title: title.clone(),
                subtitle: descriptor.problem().map(|p| format!("broken: {p}")),
                terminal: false,
                rows: Vec::new(),
            });
        }
        let (control, value, default) = match field {
            HarnessField::Enabled => (
                Control::Toggle,
                Value::Bool(descriptor.enabled),
                Value::Bool(true),
            ),
            HarnessField::Model => {
                let mut options = model_choices_in(&descriptor);
                let now = descriptor.model.default.trim().to_string();
                if !now.is_empty() && !crate::fits(&now, &options) {
                    options.push(now.clone());
                }
                (
                    Control::Choice {
                        labels: options.clone(),
                        options,
                    },
                    Value::String(now),
                    Value::String(DEFAULT_CHOICE.into()),
                )
            }
            HarnessField::Effort => {
                let options = effort_choices_in(
                    &descriptor,
                    descriptor.default_model().map(str::to_string).as_deref(),
                );
                (
                    Control::Choice {
                        labels: options.clone(),
                        options,
                    },
                    Value::String(cfg.agent_value(&id, field)),
                    Value::String(DEFAULT_CHOICE.into()),
                )
            }
        };
        if let Some(group) = current.as_mut() {
            group.rows.push(Row {
                target: Target::Agent {
                    harness: id.clone(),
                    field: field_name(field).into(),
                },
                label: field.label().into(),
                hint: cfg.agent_hint(&id, field),
                control,
                value,
                default,
                restart: None,
                is_new: false,
            });
        }
    }
    groups.extend(current);
    groups
}

fn project_groups(cfg: &Config, projects: &[ProjectRef]) -> Vec<Group> {
    let mut specs: Vec<&SettingSpec> = Vec::new();
    for tab in SETTINGS_TABS {
        if let TabBody::Project(rows) = tab.body {
            specs.extend(rows.iter().filter(|s| !DESKTOP_EXCLUDED.contains(&s.kind)));
        }
    }
    projects
        .iter()
        .map(|project| {
            let settings = cfg.project(&project.path);
            let path = project.path.to_string_lossy().to_string();
            let mut rows: Vec<Row> = specs
                .iter()
                .map(|spec| Row {
                    target: Target::Project {
                        path: path.clone(),
                        field: spec.kind.key().into(),
                    },
                    label: spec.label.into(),
                    hint: spec.hint.into(),
                    control: control(cfg, spec.kind),
                    value: Value::String(settings.text_value(spec.kind)),
                    default: Value::String(String::new()),
                    restart: None,
                    is_new: spec.kind.is_new(crate::today_days()),
                })
                .collect();
            let mut options = vec![String::new()];
            options.extend(strs(BROWSERS));
            if !settings.browser.is_empty() && !options.contains(&settings.browser) {
                options.push(settings.browser.clone());
            }
            let labels = options
                .iter()
                .map(|o| if o.is_empty() { "not set".into() } else { o.clone() })
                .collect();
            rows.push(Row {
                target: Target::Project {
                    path: path.clone(),
                    field: "browser".into(),
                },
                label: "Preview browser".into(),
                hint: "The browser the terminal app's w opens this project's dev server in first (it remembers the last pick)".into(),
                control: Control::Choice { options, labels },
                value: Value::String(settings.browser.clone()),
                default: Value::String(String::new()),
                restart: None,
                is_new: false,
            });
            Group {
                title: project.name.clone(),
                subtitle: Some(path),
                terminal: false,
                rows,
            }
        })
        .collect()
}

fn desktop_tab(prefs: &DesktopPrefs) -> Tab {
    let row = |key: &str, label: &str, hint: &str, labels: &[&str]| {
        let options = strs(crate::desktop::choices(key).unwrap_or(&[]));
        Row {
            target: Target::Desktop { key: key.into() },
            label: label.into(),
            hint: hint.into(),
            control: Control::Choice {
                options,
                labels: strs(labels),
            },
            value: Value::String(prefs.get(key).into()),
            default: Value::String(DesktopPrefs::default().get(key).into()),
            restart: None,
            is_new: false,
        }
    };
    Tab {
        id: "desktop".into(),
        title: "Desktop".into(),
        note: Some("This window's own preferences. The terminal app has its own colour theme, under Appearance.".into()),
        groups: vec![Group {
            title: String::new(),
            subtitle: None,
            terminal: false,
            rows: vec![
                row("theme", "Theme", "Follow the system's light or dark mode, or pin one (Ctrl+Shift+L cycles; --theme overrides for one run)", &["System", "Light", "Dark"]),
                row("home", "Home view", "What the main column shows with nothing open: the overview of repos, or the board of lanes (Shift+K switches)", &["Overview", "Board"]),
                row("diff_mode", "Diff layout", "How the Diff view lays out a repo that has no layout saved yet (--diff overrides for one run)", &["Side by side", "Inline", "Replacement"]),
            ],
        }],
        hotkeys: Vec::new(),
    }
}

fn hotkeys(cfg: &Config) -> Vec<Hotkey> {
    crate::actions::ACTIONS
        .iter()
        .map(|spec| {
            let defaults = strs(spec.defaults);
            let over = cfg.keybindings.get(spec.id);
            let bound = match over {
                Some(list) => list
                    .split(',')
                    .map(str::trim)
                    .filter(|c| !c.is_empty())
                    .map(String::from)
                    .collect(),
                None => defaults.clone(),
            };
            Hotkey {
                id: spec.id.into(),
                label: spec.label.into(),
                hint: spec.hint.into(),
                group: spec.group.into(),
                defaults,
                bound,
                overridden: over.is_some(),
            }
        })
        .collect()
}

fn tab_id(title: &str) -> String {
    title.to_ascii_lowercase()
}

/// Every tab the window renders, the Desktop tab first.
pub fn export(cfg: &Config, prefs: &DesktopPrefs, projects: &[ProjectRef]) -> Schema {
    let today = crate::today_days();
    let mut tabs = vec![desktop_tab(prefs)];
    for tab in SETTINGS_TABS {
        let (groups, note, keys) = match tab.body {
            TabBody::Values(specs) => (value_groups(cfg, specs, today), None, Vec::new()),
            TabBody::Agents => {
                let head = value_groups(cfg, AGENTS_HEAD, today);
                let (terminal, shared): (Vec<Group>, Vec<Group>) =
                    head.into_iter().partition(|g| g.terminal);
                let mut groups = shared;
                groups.extend(agent_groups(cfg));
                groups.extend(terminal);
                (groups, None, Vec::new())
            }
            TabBody::Project(_) => (
                project_groups(cfg, projects),
                Some(if projects.is_empty() {
                    "No repos yet. Add one from the rail and its settings appear here.".into()
                } else {
                    "Per repo. Empty commands fall back to the checkout's .kete.json.".into()
                }),
                Vec::new(),
            ),
            TabBody::Hotkeys => (
                Vec::new(),
                Some("The terminal app's keys, read-only here: rebind them in its Settings, Hotkeys tab. A running terminal app picks up changes when it next starts.".into()),
                hotkeys(cfg),
            ),
        };
        tabs.push(Tab {
            id: tab_id(tab.title),
            title: tab.title.into(),
            note,
            groups,
            hotkeys: keys,
        });
    }
    Schema { tabs }
}

/// Apply a write to `cfg` (every target but the window's own).
pub fn apply(cfg: &mut Config, target: &Target, value: &Value) -> Result<(), String> {
    match target {
        Target::Global { key } => {
            let kind = kind_for_key(key).ok_or_else(|| format!("no setting {key}"))?;
            set_value(cfg, kind, value)
        }
        Target::Agent { harness, field } => {
            let field = parse_field(field)?;
            let text = match value {
                Value::Bool(true) => "on".to_string(),
                Value::Bool(false) => "off".to_string(),
                Value::String(s) => s.clone(),
                _ => return Err("a harness row takes text or on/off".into()),
            };
            cfg.set_agent_value(harness, field, &text)
        }
        Target::Project { path, field } => {
            let text = value.as_str().ok_or("a project row takes text")?;
            let path = Path::new(path);
            match field.as_str() {
                "run_command" => cfg.set_project_text(path, SettingKind::RunCommand, text),
                "open_command" => cfg.set_project_text(path, SettingKind::OpenCommand, text),
                "browser" => {
                    let mut settings = cfg.project(path);
                    settings.browser = text.trim().to_string();
                    cfg.set_project(path, settings);
                    true
                }
                other => return Err(format!("no project field {other}")),
            };
            Ok(())
        }
        Target::Desktop { .. } => Err("a desktop preference is not in Config".into()),
    }
}

/// Write one row to the layers at `path` / `local`: a desktop preference
/// into the `desktop` object, anything else through [`Config::update_in`],
/// which writes only the keys the change touched.
pub fn set_in(path: &Path, local: &Path, target: &Target, value: &Value) -> Result<(), String> {
    if let Target::Desktop { key } = target {
        let text = value.as_str().ok_or("a desktop preference takes text")?;
        return DesktopPrefs::update_in(path, local, |p| p.set(key, text)).map(|_| ());
    }
    // Check the write on what is stored first, so a refused one says why
    // and writes nothing.
    let mut trial = Config::load_at(path, local);
    apply(&mut trial, target, value)?;
    let mut outcome = Ok(());
    Config::update_in(path, local, |cfg| outcome = apply(cfg, target, value))
        .map_err(|e| e.to_string())?;
    outcome
}

/// What the window's Reset puts back: every row these tabs show, each
/// harness's enabled / model / effort, and the window's own preferences.
/// Projects' commands, profiles, harness definitions, hotkeys and keys
/// added by hand stay — unlike the terminal app's reset, which rewrites
/// the whole file.
pub fn reset_config(cfg: &mut Config) {
    let defaults = Config::default();
    for spec in global_kinds() {
        let _ = set_value(cfg, spec.kind, &get_value(&defaults, spec.kind));
    }
    cfg.claude_enabled = true;
    cfg.codex_enabled = true;
    cfg.cursor_enabled = true;
    cfg.pi_enabled = true;
    cfg.muse_enabled = true;
    cfg.opencode_enabled = true;
    for (model, effort) in [
        (&mut cfg.claude_model, Some(&mut cfg.claude_effort)),
        (&mut cfg.codex_model, Some(&mut cfg.codex_effort)),
        (&mut cfg.cursor_model, Some(&mut cfg.cursor_effort)),
        (&mut cfg.pi_model, Some(&mut cfg.pi_effort)),
        (&mut cfg.muse_model, Some(&mut cfg.muse_effort)),
        (&mut cfg.opencode_model, None),
    ] {
        *model = DEFAULT_CHOICE.into();
        if let Some(effort) = effort {
            *effort = DEFAULT_CHOICE.into();
        }
    }
    cfg.harnesses.retain(|_, over| {
        over.enabled = None;
        over.model_default = None;
        over.effort_default = None;
        *over != nebula_core::harness::HarnessOverride::default()
    });
    for entry in &mut cfg.custom_harnesses {
        entry.enabled = true;
        entry.model = DEFAULT_CHOICE.into();
    }
}

/// [`reset_config`] and the window's preferences, written to the layers.
pub fn reset_in(path: &Path, local: &Path) -> Result<(), String> {
    Config::update_in(path, local, reset_config).map_err(|e| e.to_string())?;
    DesktopPrefs::update_in(path, local, |p| {
        let other = std::mem::take(&mut p.other);
        *p = DesktopPrefs {
            other,
            ..DesktopPrefs::default()
        };
        Ok(())
    })
    .map(|_| ())
}

/// Every overlay row the window renders, for the parity check.
pub fn rendered_kinds(schema: &Schema) -> Vec<SettingKind> {
    let mut kinds = Vec::new();
    for tab in &schema.tabs {
        for group in &tab.groups {
            for row in &group.rows {
                match &row.target {
                    Target::Global { key } => kinds.extend(kind_for_key(key)),
                    Target::Project { field, .. } => {
                        if field == SettingKind::RunCommand.key() {
                            kinds.push(SettingKind::RunCommand);
                        } else if field == SettingKind::OpenCommand.key() {
                            kinds.push(SettingKind::OpenCommand);
                        }
                    }
                    _ => {}
                }
            }
        }
    }
    kinds
}
