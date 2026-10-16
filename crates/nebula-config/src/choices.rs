//! The words the settings store for their choice rows, and the small
//! enums some of them parse into. They live here, not with the TUI code
//! that draws them, so the desktop window offers the same choices; the
//! TUI's modules re-export them under their old paths.

/// The theme names the settings overlay cycles through: every preset,
/// then the two that are resolved against the machine (`auto`, `vscode`).
/// The palettes themselves are the TUI's (`nebula_tui::theme`).
pub const THEMES: &[&str] = &[
    "default",
    "ocean",
    "forest",
    "rose",
    "amber",
    "lavender",
    "coral",
    "slate",
    "sand",
    "mono",
    "one-dark",
    "one-light",
    "kete-dark",
    "kete-light",
    AUTO,
    VSCODE,
];

/// The theme name that follows the OS's light or dark app theme.
pub const AUTO: &str = "auto";
/// The theme name that reads the theme VS Code is set to.
pub const VSCODE: &str = "vscode";
/// `vscode:<name>`: one installed VS Code theme by its label or id,
/// whichever VS Code itself is set to. Picked from the theme row's list
/// rather than cycled to, so it is not in [`THEMES`].
pub const VSCODE_PREFIX: &str = "vscode:";

/// The installed VS Code theme a `vscode:<name>` theme names; None for
/// any other theme.
pub fn vscode_theme_name(theme: &str) -> Option<&str> {
    let theme = theme.trim();
    let head = theme.get(..VSCODE_PREFIX.len())?;
    head.eq_ignore_ascii_case(VSCODE_PREFIX)
        .then(|| theme[VSCODE_PREFIX.len()..].trim())
        .filter(|name| !name.is_empty())
}

/// The theme a fresh config starts on, on every OS: Kete Workshop, dark.
pub const DEFAULT_THEME: &str = "kete-dark";

/// The FILE ICONS styles; `auto` first, the default.
pub const ICON_STYLES: &[&str] = &["auto", "nerd", "ascii", "off"];

/// The INLINE GRAPHICS modes; `auto` the default.
pub const INLINE_GRAPHICS_MODES: &[&str] =
    &["auto", "kitty", "iterm", "sixel", "halfblocks", "off"];
pub const DEFAULT_INLINE_GRAPHICS: &str = "auto";

/// A stored `inline_graphics` word as one of [`INLINE_GRAPHICS_MODES`];
/// anything else is the default.
pub fn mode_label(value: &str) -> &'static str {
    INLINE_GRAPHICS_MODES
        .iter()
        .copied()
        .find(|mode| mode.eq_ignore_ascii_case(value.trim()))
        .unwrap_or(DEFAULT_INLINE_GRAPHICS)
}

/// The words `session_pane` stores, the default first: the TUI's
/// `launcher::PaneSide` by name.
pub const PANE_SIDES: &[&str] = &["right", "bottom"];

/// A stored `session_pane` word read back: `bottom`, or the right for
/// anything else (a typo, the `left` older builds offered).
pub fn pane_side_word(word: &str) -> &'static str {
    match word.trim() {
        "bottom" => "bottom",
        _ => PANE_SIDES[0],
    }
}

/// How a DIFF VIEWER lays a diff out.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DiffMode {
    #[default]
    SideBySide,
    Inline,
    Replacement,
}

impl DiffMode {
    /// The order `Ctrl+s` and the settings row cycle through.
    pub const ALL: [DiffMode; 3] = [
        DiffMode::SideBySide,
        DiffMode::Inline,
        DiffMode::Replacement,
    ];

    /// The word config.json stores.
    pub fn as_str(self) -> &'static str {
        match self {
            DiffMode::SideBySide => "side-by-side",
            DiffMode::Inline => "inline",
            DiffMode::Replacement => "replacement",
        }
    }

    /// A stored word, read forgivingly: `split` and `unified` are what
    /// the older `Ctrl+s` toggle called the first two.
    pub fn parse(word: &str) -> Option<Self> {
        match word.trim().to_ascii_lowercase().as_str() {
            "side-by-side" | "side by side" | "split" => Some(DiffMode::SideBySide),
            "inline" | "unified" => Some(DiffMode::Inline),
            "replacement" | "replace" => Some(DiffMode::Replacement),
            _ => None,
        }
    }

    /// The layout after this one, round to the first.
    pub fn next(self) -> Self {
        let at = Self::ALL.iter().position(|m| *m == self).unwrap_or(0);
        Self::ALL[(at + 1) % Self::ALL.len()]
    }

    /// What the pane's frame and the footer call it.
    pub fn label(self) -> &'static str {
        match self {
            DiffMode::SideBySide => "side by side",
            DiffMode::Inline => "inline",
            DiffMode::Replacement => "replacement",
        }
    }
}

/// Where an AGENT PRESET's text goes around the task: a prefix, a postfix
/// or both. `preset_text` in config.json stores [`PresetText::as_str`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PresetText {
    Prefix,
    Postfix,
    Both,
}

impl PresetText {
    /// In cycle order, as the Text row and the setting step them.
    pub const ALL: [PresetText; 3] = [PresetText::Prefix, PresetText::Postfix, PresetText::Both];
    /// What a config that never set `preset_text` means.
    pub const DEFAULT: PresetText = PresetText::Prefix;

    /// The label the setting stores and both rows show.
    pub const fn as_str(self) -> &'static str {
        match self {
            PresetText::Prefix => "prefix",
            PresetText::Postfix => "postfix",
            PresetText::Both => "prefix & postfix",
        }
    }

    /// A label back to its side, case-insensitively and trimmed; `both` is
    /// accepted for a hand edit. None for anything else.
    pub fn parse(value: &str) -> Option<PresetText> {
        let value = value.trim();
        if value.eq_ignore_ascii_case("both") {
            return Some(PresetText::Both);
        }
        Self::ALL
            .into_iter()
            .find(|side| side.as_str().eq_ignore_ascii_case(value))
    }

    pub fn has_prefix(self) -> bool {
        matches!(self, PresetText::Prefix | PresetText::Both)
    }

    pub fn has_postfix(self) -> bool {
        matches!(self, PresetText::Postfix | PresetText::Both)
    }

    /// The side (or sides) two flags name; None when neither is set.
    pub fn of_sides(prefix: bool, postfix: bool) -> Option<PresetText> {
        match (prefix, postfix) {
            (true, true) => Some(PresetText::Both),
            (true, false) => Some(PresetText::Prefix),
            (false, true) => Some(PresetText::Postfix),
            (false, false) => None,
        }
    }
}
