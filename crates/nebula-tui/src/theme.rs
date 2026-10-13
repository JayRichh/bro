//! Color theme: every color the UI draws, keyed by role rather than
//! hardcoded at the call site. The active theme is picked by name in the
//! settings overlay (`theme` in config.json) and lives on `App`, refreshed
//! by the event loop when the setting changes.
//!
//! The ANSI presets stick to ANSI-16 and 256-color indexed values so they
//! render everywhere. One exception: `focus_tint` needs a near-black shade
//! of the accent that the 256 palette simply doesn't have (its darkest
//! chromatic steps start around 40%), so it's truecolor RGB — supported by
//! modern terminals including Terminal.app since macOS Tahoe.
//!
//! The TRUECOLOR PRESETS (`one-dark`, `one-light`) go further: every role
//! is an exact RGB, and they bring their own canvas ([`Theme::bg`]) so the
//! window reads as the editor beside it rather than as whatever the
//! terminal's palette makes of the ANSI names. Two more names are not
//! presets but sources, resolved by [`resolve`] against the machine:
//! `auto` follows the OS app theme between the two One presets, and
//! `vscode` derives the roles from the theme VS Code itself is set to
//! (`crate::vscode_theme`).

use ratatui::style::Color;

/// What the BLACK BACKGROUND setting paints under every cell nothing else
/// colored. Truecolor rather than ANSI `Black`, which a terminal palette is
/// free to map to a dark gray (a stock Ghostty's is #1d1f21) — the very
/// gray the setting exists to get away from.
pub const BLACK_BACKGROUND: Color = Color::Rgb(0, 0, 0);

/// The presets [`Theme::by_name`] knows by itself, with no look at the
/// machine: what the invariant tests below walk.
pub const PRESETS: &[&str] = &[
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
];

/// The theme that follows the OS app theme: `one-light` while the OS says
/// light, `one-dark` otherwise ([`resolve`]).
pub const AUTO: &str = "auto";
/// The theme read out of VS Code's own settings (`crate::vscode_theme`),
/// falling back to `one-dark` when there is none to read.
pub const VSCODE: &str = "vscode";

/// Names the settings overlay cycles through: every preset, then the two
/// that are resolved against the machine. `by_name` accepts the presets
/// case-insensitively and falls back to the first entry; [`resolve`]
/// accepts all of them.
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
    AUTO,
    VSCODE,
];

/// The theme a fresh config starts on: One Dark on Windows, where Bro sits
/// beside a VS Code that most likely wears it; the classic look elsewhere,
/// as upstream nebula ships it.
pub const DEFAULT_THEME: &str = if cfg!(windows) { "one-dark" } else { "default" };

/// Semantic color roles for the whole TUI.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Theme {
    /// Focus borders, titles, cursors, match highlights.
    pub accent: Color,
    /// Text drawn on top of an `accent` background (focused title chip).
    pub on_accent: Color,
    /// Primary input text.
    pub text: Color,
    /// Secondary text: unfocused titles, inactive breadcrumb segments.
    /// Also what `dim` spans brighten to on an unfocused selection bar.
    pub muted: Color,
    /// Hints, dividers, badges, archived rows; also the unfocused
    /// selection-bar background.
    pub dim: Color,
    /// Added / connected / a review file ticked off — the plain "this went
    /// well" green. Also a finished turn you have already read: still a
    /// good outcome, just no longer a job.
    pub ok: Color,
    /// A turn that finished and nobody has read yet: the dot, and the
    /// `n done` counts pointing at it. Deliberately NOT `ok` — the whole
    /// point is that this one wants a human, and green is the color a
    /// terminal teaches you to skip over. Reading the session turns the
    /// dot green. Sky blue unless the preset already owns blue, and never
    /// a violet: `merged` is purple in every preset, a merged checkout's
    /// row sits right above its unread session's, and the two dots say
    /// opposite things (come read this / this landed, delete it).
    pub done: Color,
    /// Running — the STATUS DOT and, on the grid, the running card's
    /// frame, one yellow for both — modified / flash messages / remote
    /// host.
    pub warn: Color,
    /// Needs feedback / deleted / destructive actions.
    pub err: Color,
    /// Terminated sessions and the session kind badge.
    pub special: Color,
    /// A merged pull request: its row's arrow, rail and badge, and the
    /// state word in the PR PREVIEW. Purple in every preset — the color
    /// GitHub paints a merge in, so the row says "landed" before the badge
    /// is read — and its own role rather than `special`, so a merged PR
    /// never wears the same color as a terminated session, whatever the
    /// preset makes of that one.
    pub merged: Color,
    /// The checkout a LAUNCHER CARD's session runs in, when that checkout
    /// is the project's ROOT WORKTREE: the branch its edits land on with
    /// nothing fenced around them. Amber in every preset, the way `merged`
    /// is purple in every preset — which of the two scopes a card is in is
    /// not a matter of taste, and two presets (`amber`, `sand`) carry a
    /// warm `accent` that would leave them unable to tell one from the
    /// other.
    pub root: Color,
    /// The counterpart, for a session in a worktree of its own — work
    /// fenced off from the root branch. Far enough from `root` across the
    /// 256-color cube to read as a different hue rather than a shade of
    /// it: telling the pair apart at a glance is the whole job, and
    /// `the_two_scope_colors_are_a_hue_apart_in_every_preset` holds them
    /// to it.
    pub worktree: Color,
    /// Selected-row fill in the focused panel (a subtle raised surface,
    /// not a reverse-video slab).
    pub sel_bg: Color,
    /// Selected-row fill in unfocused panels (barely raised).
    pub sel_bg_dim: Color,
    /// Structural chrome: column rules, header underlines, dividers.
    /// Darker than `dim` so the frame recedes behind the content.
    pub edge: Color,
    /// Shades for the running-row text sweep, `[tail, mid, head]`: the
    /// whole name sits on the tail shade while a brighter two-cell band
    /// sweeps across it. Yellow family, paired with `warn`.
    pub warn_sweep: [Color; 3],
    /// Needs-feedback counterpart of `warn_sweep`. Red family, paired
    /// with `err`.
    pub err_sweep: [Color; 3],
    /// Merged counterpart of `warn_sweep`, for the checkout row whose pull
    /// request has landed. Purple family, paired with `merged`. A ONE-SHOT
    /// SWEEP: the row rides it for a few seconds after the merge is seen,
    /// then rests on `merged`.
    pub merged_sweep: [Color; 3],
    /// Unread-done counterpart, the other ONE-SHOT SWEEP: a row rides it
    /// for a few seconds after a turn finishes unread, then holds still.
    /// Rests on `done`, so a preset that moves `done` moves this with it.
    pub done_sweep: [Color; 3],
    /// Focused-surface background, behind the session pane or the card
    /// keys land in: the accent's own hue taken down to a near-black —
    /// OKLCH lightness 0.20 and chroma 0.04 in every preset (mono's is
    /// the one gray). On a black window it reads as the accent glowing
    /// faintly rather than as a gray slab, and it is darker than the gray
    /// it replaced, so dim text on it keeps more of its contrast.
    /// Truecolor by necessity (see module docs).
    pub focus_tint: Color,
    /// The canvas: what every cell nothing else colored is painted
    /// (`ui::draw_canvas`), the session pane's default background
    /// included. None in the ANSI presets, which leave it to the
    /// terminal — or to BLACK BACKGROUND; a TRUECOLOR PRESET always
    /// brings its own, since One Light on a dark terminal's own
    /// background would be dark text on dark.
    pub bg: Option<Color>,
    /// The theme is a light one: dark text on a light canvas. What the
    /// fills that are a status color taken toward the canvas
    /// ([`Theme::toward_bg`]) and the invariant tests key on.
    pub light: bool,
    /// Syntax token colors, for the previews and the DIFF VIEWER
    /// (`ui::token_style`): keywords, string literals, comments, numbers.
    /// The ANSI presets keep the roles the highlighter always borrowed
    /// (`special`, `ok`, `dim`, `warn`); a truecolor preset brings its
    /// editor's own.
    pub syn_keyword: Color,
    pub syn_string: Color,
    pub syn_comment: Color,
    pub syn_number: Color,
    /// The NESTED layout's grays — a live root's title, an idle one's
    /// (and every row's age, count and connector) — and the blue a pull
    /// request's underlined `#42` wears there. The [`nested`] constants in
    /// the ANSI presets; a light canvas needs them dark instead.
    pub nested_bright: Color,
    pub nested_dim: Color,
    pub link: Color,
    /// The DIFF VIEWER's backgrounds, VS Code's way: an added or removed
    /// line wears a faint wash of the add green or the remove red, and the
    /// words that changed within it a stronger one — the line says what
    /// kind of change, the words say where. Truecolor in every preset: the
    /// 256 palette has no faint shade of either.
    pub diff_add_bg: Color,
    pub diff_del_bg: Color,
    pub diff_add_word: Color,
    pub diff_del_word: Color,
}

/// `c` taken `level` of the way from the canvas toward itself: at 0 the
/// canvas, at 1 the color — truecolor, as `focus_tint` already is, since
/// the 256 palette has no faint shade of most hues. A theme with no canvas
/// of its own blends from black, the dark terminal it assumes. A color
/// with no fixed value (`Reset`) is returned as is.
pub fn blend(c: Color, toward: Option<Color>, level: f32) -> Color {
    let Some((r, g, b)) = color_rgb(c) else {
        return c;
    };
    let (br, bg, bb) = toward.and_then(color_rgb).unwrap_or((0, 0, 0));
    let f = |v: u8, base: u8| {
        (f32::from(base) + (f32::from(v) - f32::from(base)) * level)
            .round()
            .clamp(0.0, 255.0) as u8
    };
    Color::Rgb(f(r, br), f(g, bg), f(b, bb))
}

/// The RGB a terminal most likely shows for `c`: xterm's defaults for the
/// sixteen named colors, the 6×6×6 cube and the gray ramp for the rest of
/// the 256.
pub fn color_rgb(c: Color) -> Option<(u8, u8, u8)> {
    const ANSI: [(u8, u8, u8); 16] = [
        (0, 0, 0),
        (205, 0, 0),
        (0, 205, 0),
        (205, 205, 0),
        (0, 0, 238),
        (205, 0, 205),
        (0, 205, 205),
        (229, 229, 229),
        (127, 127, 127),
        (255, 0, 0),
        (0, 255, 0),
        (255, 255, 0),
        (92, 92, 255),
        (255, 0, 255),
        (0, 255, 255),
        (255, 255, 255),
    ];
    let index = match c {
        Color::Rgb(r, g, b) => return Some((r, g, b)),
        Color::Indexed(i) => i,
        Color::Black => 0,
        Color::Red => 1,
        Color::Green => 2,
        Color::Yellow => 3,
        Color::Blue => 4,
        Color::Magenta => 5,
        Color::Cyan => 6,
        Color::Gray => 7,
        Color::DarkGray => 8,
        Color::LightRed => 9,
        Color::LightGreen => 10,
        Color::LightYellow => 11,
        Color::LightBlue => 12,
        Color::LightMagenta => 13,
        Color::LightCyan => 14,
        Color::White => 15,
        Color::Reset => return None,
    };
    Some(match index {
        0..=15 => ANSI[usize::from(index)],
        16..=231 => {
            let i = index - 16;
            let level = |v: u8| if v == 0 { 0 } else { 55 + v * 40 };
            (level(i / 36), level(i / 6 % 6), level(i % 6))
        }
        _ => {
            let v = 8 + (index - 232) * 10;
            (v, v, v)
        }
    })
}

/// `#rrggbb` as a truecolor, at compile time: the presets below are
/// spelled the way the editor themes they copy spell them. Only ever
/// handed literals; a digit that is not hex reads as 0.
pub const fn hex(s: &str) -> Color {
    const fn nib(c: u8) -> u8 {
        match c {
            b'0'..=b'9' => c - b'0',
            b'a'..=b'f' => c - b'a' + 10,
            b'A'..=b'F' => c - b'A' + 10,
            _ => 0,
        }
    }
    const fn byte(b: &[u8], i: usize) -> u8 {
        nib(b[i]) * 16 + nib(b[i + 1])
    }
    let b = s.as_bytes();
    Color::Rgb(byte(b, 1), byte(b, 3), byte(b, 5))
}

/// `done` and its sweep for a preset that already owns blue.
const DONE_PINK: [Color; 3] = [
    Color::Indexed(212),
    Color::Indexed(218),
    Color::Indexed(225),
];
/// `done` and its sweep for a preset whose pinks and violets crowd the blue.
const DONE_TURQUOISE: [Color; 3] = [Color::Indexed(45), Color::Indexed(81), Color::Indexed(159)];

impl Default for Theme {
    fn default() -> Self {
        // The classic nebula look: cyan accent on ANSI colors.
        Self {
            accent: Color::Cyan,
            on_accent: Color::Black,
            text: Color::White,
            muted: Color::Gray,
            dim: Color::DarkGray,
            ok: Color::Green,
            done: Color::Indexed(75), // sky blue — a hue away from merged's purple, not a shade
            warn: Color::Yellow,
            err: Color::Red,
            special: Color::Magenta,
            merged: Color::Indexed(135),  // purple — GitHub's merged
            root: Color::Indexed(214),    // amber — the branch itself
            worktree: Color::Indexed(79), // aquamarine — fenced off from it
            sel_bg: Color::Indexed(237),
            sel_bg_dim: Color::Indexed(235),
            edge: Color::Indexed(238),
            warn_sweep: [Color::Yellow, Color::Indexed(220), Color::Indexed(230)],
            err_sweep: [Color::Red, Color::Indexed(203), Color::Indexed(217)],
            merged_sweep: [
                Color::Indexed(135),
                Color::Indexed(141),
                Color::Indexed(183),
            ],
            done_sweep: [Color::Indexed(75), Color::Indexed(111), Color::Indexed(153)],
            focus_tint: Color::Rgb(0, 27, 28),
            bg: None,
            light: false,
            syn_keyword: Color::Magenta,
            syn_string: Color::Green,
            syn_comment: Color::DarkGray,
            syn_number: Color::Yellow,
            nested_bright: nested::BRIGHT,
            nested_dim: nested::DIM,
            link: nested::LINK,
            diff_add_bg: Color::Rgb(18, 42, 22),
            diff_del_bg: Color::Rgb(52, 18, 22),
            diff_add_word: Color::Rgb(36, 86, 44),
            diff_del_word: Color::Rgb(104, 34, 42),
        }
    }
}

/// Atom One Dark, as VS Code's `akamud.vscode-theme-onedark` paints it:
/// `#282C34` under `#ABB2BF`, the blue `#61AFEF` as the accent. The roles
/// keep the rules every preset keeps — `merged` purple, `root` the warm
/// scope against a cool `worktree`, `done` a hue no status takes — on the
/// One palette's own seven colors: the blue being the accent, `done`
/// goes pink, as it does in every preset that owns blue.
pub const ONE_DARK: Theme = Theme {
    accent: hex("#61AFEF"),
    on_accent: hex("#282C34"),
    text: hex("#ABB2BF"),
    muted: hex("#828997"),
    dim: hex("#5C6370"),
    ok: hex("#98C379"),
    done: hex("#E9A0C6"),
    warn: hex("#E5C07B"),
    err: hex("#E06C75"),
    special: hex("#BE5046"),
    merged: hex("#C678DD"),
    root: hex("#D19A66"),
    worktree: hex("#56B6C2"),
    sel_bg: hex("#3E4451"),
    sel_bg_dim: hex("#2C313A"),
    edge: hex("#3E4451"),
    warn_sweep: [hex("#E5C07B"), hex("#EED49B"), hex("#F7E8C4")],
    err_sweep: [hex("#E06C75"), hex("#E9939A"), hex("#F2BEC2")],
    merged_sweep: [hex("#C678DD"), hex("#D69CE7"), hex("#E7C3F1")],
    done_sweep: [hex("#E9A0C6"), hex("#F2BEDA"), hex("#F9DDEB")],
    focus_tint: hex("#2B3445"),
    bg: Some(hex("#282C34")),
    light: false,
    syn_keyword: hex("#C678DD"),
    syn_string: hex("#98C379"),
    syn_comment: hex("#7F848E"),
    syn_number: hex("#D19A66"),
    nested_bright: hex("#D7DAE0"),
    nested_dim: hex("#7F848E"),
    link: hex("#61AFEF"),
    // #98C379 and #E06C75 over #282C34: a sixth for the line, a third
    // for the words.
    diff_add_bg: hex("#3A443F"),
    diff_del_bg: hex("#45363E"),
    diff_add_word: hex("#4F614C"),
    diff_del_word: hex("#68424B"),
};

/// Atom One Light, One Dark's standard counterpart: `#383A42` on
/// `#FAFAFA`, the blue `#4078F2` as the accent. A light canvas turns the
/// sweeps around — their heads run darker, the brighter shade being the
/// one that would vanish — and the tints toward the canvas come out as
/// pale washes rather than near-blacks.
pub const ONE_LIGHT: Theme = Theme {
    accent: hex("#4078F2"),
    on_accent: hex("#FAFAFA"),
    text: hex("#383A42"),
    muted: hex("#696C77"),
    dim: hex("#A0A1A7"),
    ok: hex("#50A14F"),
    done: hex("#D1478F"),
    warn: hex("#C18401"),
    err: hex("#E45649"),
    special: hex("#A05A48"),
    merged: hex("#A626A4"),
    root: hex("#986801"),
    worktree: hex("#0184BC"),
    sel_bg: hex("#E5E5E6"),
    sel_bg_dim: hex("#F0F0F1"),
    edge: hex("#DBDBDC"),
    warn_sweep: [hex("#C18401"), hex("#A87300"), hex("#8A5E00")],
    err_sweep: [hex("#E45649"), hex("#C93D30"), hex("#A82E22")],
    merged_sweep: [hex("#A626A4"), hex("#8C1D8A"), hex("#6E156C")],
    done_sweep: [hex("#D1478F"), hex("#B33276"), hex("#91225D")],
    focus_tint: hex("#EAF0FE"),
    bg: Some(hex("#FAFAFA")),
    light: true,
    syn_keyword: hex("#A626A4"),
    syn_string: hex("#50A14F"),
    syn_comment: hex("#A0A1A7"),
    syn_number: hex("#986801"),
    nested_bright: hex("#202227"),
    nested_dim: hex("#8A8C93"),
    link: hex("#4078F2"),
    // #50A14F and #E45649 over #FAFAFA, the same way.
    diff_add_bg: hex("#E1EDE0"),
    diff_del_bg: hex("#F7E1DF"),
    diff_add_word: hex("#C4DEC3"),
    diff_del_word: hex("#F3C6C1"),
};

impl Theme {
    /// `c` taken `level` of the way from this theme's canvas toward
    /// itself ([`blend`]): the washes HIGHLIGHT CURRENT CARD and the
    /// NESTED layout's cursor row fill with. Near-black on a dark
    /// canvas, a pale tint on a light one.
    pub fn toward_bg(&self, c: Color, level: f32) -> Color {
        blend(c, self.bg, level)
    }

    /// Is this one of the presets that paint every cell themselves?
    pub fn truecolor(&self) -> bool {
        self.bg.is_some()
    }

    pub fn by_name(name: &str) -> Self {
        let base = Self::default();
        let mut th = match name.trim().to_ascii_lowercase().as_str() {
            "one-dark" | "onedark" | "one dark" => ONE_DARK,
            "one-light" | "onelight" | "one light" => ONE_LIGHT,
            "ocean" => Self {
                accent: Color::Indexed(39),  // deep sky blue
                special: Color::Indexed(75), // steel blue
                // Blue is spoken for twice over here (the done sky blue IS
                // this preset's terminated), so done goes pink — the one
                // hue a blue preset leaves free.
                done: DONE_PINK[0],
                done_sweep: DONE_PINK,
                focus_tint: Color::Rgb(3, 24, 38),
                ..base
            },
            "forest" => Self {
                accent: Color::Indexed(114),  // pale green
                special: Color::Indexed(108), // sage
                focus_tint: Color::Rgb(10, 27, 10),
                ..base
            },
            "rose" => Self {
                accent: Color::Indexed(211),  // pink
                special: Color::Indexed(141), // violet
                // Pink and violet are both spoken for here, and sky blue
                // sits too near the violet, so done goes turquoise — the one
                // hue this preset leaves free.
                done: DONE_TURQUOISE[0],
                done_sweep: DONE_TURQUOISE,
                focus_tint: Color::Rgb(36, 14, 21),
                ..base
            },
            "amber" => Self {
                accent: Color::Indexed(214),  // orange
                special: Color::Indexed(173), // copper
                focus_tint: Color::Rgb(32, 19, 1),
                ..base
            },
            "lavender" => Self {
                accent: Color::Indexed(147),  // periwinkle
                special: Color::Indexed(176), // orchid
                // The done sky blue would vanish into a periwinkle accent,
                // so done goes turquoise here, as in rose.
                done: DONE_TURQUOISE[0],
                done_sweep: DONE_TURQUOISE,
                focus_tint: Color::Rgb(20, 19, 39),
                ..base
            },
            "coral" => Self {
                accent: Color::Indexed(209), // coral
                // The cooled-off complement, the way default pairs cyan with
                // magenta: a terminated session in a warm preset reads as
                // gone cold rather than as a faded needs-feedback red.
                special: Color::Indexed(73), // cadet teal
                focus_tint: Color::Rgb(37, 15, 8),
                ..base
            },
            "slate" => Self {
                accent: Color::Indexed(110), // dusty sky blue
                special: Color::Indexed(67), // steel blue
                // The done sky blue would land between this preset's two
                // blues, so done goes pink here, as in ocean.
                done: DONE_PINK[0],
                done_sweep: DONE_PINK,
                focus_tint: Color::Rgb(6, 23, 39),
                ..base
            },
            "sand" => Self {
                accent: Color::Indexed(180),  // tan
                special: Color::Indexed(137), // bronze
                focus_tint: Color::Rgb(34, 18, 2),
                ..base
            },
            "mono" => Self {
                // Grayscale chrome for a terminal whose own palette is
                // colorful enough: the frame, titles and cursors go white
                // and gray, and only the statuses keep their color.
                accent: Color::White,
                // A step under the accent, so a white-on-white match
                // highlight still stands out from the text around it.
                text: Color::Indexed(252),
                // And a step under that, so unfocused titles still read as
                // secondary next to the text.
                muted: Color::Indexed(247),
                special: Color::Indexed(245),
                focus_tint: Color::Rgb(22, 22, 22),
                ..base
            },
            _ => base,
        };
        // The ANSI presets color code with the roles the highlighter
        // always borrowed, so a preset that moves `special` moves its
        // keywords with it, as it always has.
        if !th.truecolor() {
            th.syn_keyword = th.special;
            th.syn_string = th.ok;
            th.syn_comment = th.dim;
            th.syn_number = th.warn;
        }
        th
    }
}

/// What a theme NAME needs from the machine to become a palette: whether
/// the OS app theme is light (`auto`, and `vscode` with VS Code's own
/// auto-detect on), and the theme read out of VS Code (`vscode`). Read by
/// the caller — the registry, `defaults`, VS Code's settings — so
/// [`resolve`] stays pure and testable.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Machine {
    /// The OS says apps are light. None where it cannot be asked.
    pub os_light: Option<bool>,
    /// The palette `crate::vscode_theme` derived, if VS Code is installed
    /// and its theme could be read.
    pub vscode: Option<Theme>,
}

/// A theme NAME as the UI draws it: a preset by [`Theme::by_name`];
/// [`AUTO`] the One preset the OS's light or dark asks for; [`VSCODE`]
/// what VS Code's settings resolved to, or One Dark with nothing to read.
pub fn resolve(name: &str, machine: &Machine) -> Theme {
    match name.trim().to_ascii_lowercase().as_str() {
        AUTO => {
            if machine.os_light == Some(true) {
                ONE_LIGHT
            } else {
                ONE_DARK
            }
        }
        VSCODE => machine.vscode.unwrap_or(ONE_DARK),
        _ => Theme::by_name(name),
    }
}

/// Does `name` read anything off the machine? The event loop re-checks
/// the OS and VS Code on a beat only while one of these is picked.
pub fn is_live(name: &str) -> bool {
    matches!(name.trim().to_ascii_lowercase().as_str(), AUTO | VSCODE)
}

/// The NESTED layout's grays (Settings → Appearance → **Worktree layout**
/// → `nested`): a live root's title against an idle one's, the ages and
/// counts, and the cursor row's fill. Truecolor, like `focus_tint`: an
/// ANSI gray is whatever the terminal's palette makes of it, and the two
/// title steps could land on one shade there. Every hue the layout draws
/// — the status dots, the cursor's accent — is still the preset's own.
pub mod nested {
    use ratatui::style::Color;

    /// A live root's title: working, or waiting on you.
    pub const BRIGHT: Color = Color::Rgb(0xf2, 0xf2, 0xf2);
    /// An idle root's title, and every row's age, count and connector.
    pub const DIM: Color = Color::Rgb(0x8c, 0x8c, 0x8c);
    /// A pull request's `#42`, underlined: a link, in the blue links wear.
    pub const LINK: Color = Color::Rgb(0x58, 0xa6, 0xff);
    /// How much of the accent the cursor's row keeps as its fill: the
    /// accent taken nearly to black, so the row is washed in it.
    pub const SELECTED_FILL: f32 = 0.18;
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Red, green, blue coordinates (0..=5) of a 6x6x6 cube color.
    fn cube(c: Color) -> Option<[u8; 3]> {
        match c {
            Color::Indexed(n @ 16..=231) => {
                let n = n - 16;
                Some([n / 36, (n / 6) % 6, n % 6])
            }
            _ => None,
        }
    }

    /// How far two cube colors sit apart, in steps; None when either is
    /// not a cube color and the distance is not ours to measure.
    fn cube_steps(a: Color, b: Color) -> Option<u8> {
        let (a, b) = (cube(a)?, cube(b)?);
        Some((0..3).map(|i| a[i].abs_diff(b[i])).sum())
    }

    /// A truecolor's hue in degrees, None for a gray or a non-RGB color.
    fn rgb_hue(c: Color) -> Option<f64> {
        let Color::Rgb(r, g, b) = c else {
            return None;
        };
        let (r, g, b) = (f64::from(r), f64::from(g), f64::from(b));
        let (hi, lo) = (r.max(g).max(b), r.min(g).min(b));
        if hi - lo < 1.0 {
            return None;
        }
        let d = hi - lo;
        let h = if hi == r {
            ((g - b) / d).rem_euclid(6.0)
        } else if hi == g {
            (b - r) / d + 2.0
        } else {
            (r - g) / d + 4.0
        };
        Some(h * 60.0)
    }

    /// How far round the color wheel two truecolors sit, in degrees; None
    /// when either is not one and the distance is not ours to measure —
    /// the truecolor counterpart of [`cube_steps`], for the presets the
    /// cube says nothing about.
    fn hue_apart(a: Color, b: Color) -> Option<f64> {
        let (a, b) = (rgb_hue(a)?, rgb_hue(b)?);
        let d = (a - b).abs();
        Some(d.min(360.0 - d))
    }

    /// WCAG relative luminance.
    fn luminance(rgb: [u8; 3]) -> f64 {
        let lin = |c: u8| {
            let c = f64::from(c) / 255.0;
            if c <= 0.04045 {
                c / 12.92
            } else {
                ((c + 0.055) / 1.055).powf(2.4)
            }
        };
        0.2126 * lin(rgb[0]) + 0.7152 * lin(rgb[1]) + 0.0722 * lin(rgb[2])
    }

    /// WCAG contrast ratio between two colors that have an RGB.
    fn contrast(a: Color, b: Color) -> f64 {
        let l = |c: Color| {
            let (r, g, b) = color_rgb(c).expect("a fixed color");
            luminance([r, g, b])
        };
        let (x, y) = (l(a), l(b));
        (x.max(y) + 0.05) / (x.min(y) + 0.05)
    }

    #[test]
    fn by_name_covers_all_presets_and_falls_back() {
        for name in PRESETS {
            // Every listed preset must parse (and not accidentally be a
            // misspelling that falls back to default without noticing).
            let theme = Theme::by_name(name);
            if *name != "default" {
                assert_ne!(theme, Theme::default(), "{name} fell back to default");
            }
        }
        assert_eq!(Theme::by_name("no-such-theme"), Theme::default());
        assert_eq!(Theme::by_name(" Ocean "), Theme::by_name("ocean"));

        // A done dot has to stay readable AS done: never green (that's
        // `ok`, which the eye files as "nothing to do here"), and never
        // the same color as another status in the same preset.
        for name in PRESETS {
            let th = Theme::by_name(name);
            assert_ne!(th.done, th.ok, "{name}: done reads as plain success");
            assert_ne!(th.done, th.warn, "{name}: done reads as running");
            assert_ne!(th.done, th.err, "{name}: done reads as needs-feedback");
            assert_ne!(th.done, th.special, "{name}: done reads as terminated");
            assert_ne!(th.done, th.dim, "{name}: done reads as fresh");
            assert_ne!(th.done, th.accent, "{name}: done reads as the cursor");
        }
    }

    /// A running card's frame is the running yellow itself — the color its
    /// STATUS DOT wears — so no preset may hand `warn` to the accent or to
    /// the quiet edge: a running card would then pass for the cursor's, or
    /// for a card with nothing going on.
    #[test]
    fn a_running_frame_never_reads_as_the_focus() {
        for name in PRESETS {
            let th = Theme::by_name(name);
            assert_ne!(th.warn, th.accent, "{name}: running reads as the cursor");
            assert_ne!(th.warn, th.edge, "{name}: running reads as a quiet card");
        }
    }

    /// An unread-done dot sits right under a merged checkout's purple one,
    /// and the two mean opposite things — so done keeps a whole hue away
    /// from the merge, not a shade: it is no color the merged sweep passes
    /// through (the branch name would flash "done" on every pass), and —
    /// when both sit in the 256-color cube — it is at least three steps
    /// from the merge there. The violet this replaced was one.
    #[test]
    fn done_is_never_a_shade_of_merged() {
        for name in PRESETS {
            let th = Theme::by_name(name);
            assert!(
                !th.merged_sweep.contains(&th.done),
                "{name}: the merged sweep flashes the done color"
            );
            if let (Some(done), Some(merged)) = (cube(th.done), cube(th.merged)) {
                let steps: u8 = (0..3).map(|i| done[i].abs_diff(merged[i])).sum();
                assert!(
                    steps >= 3,
                    "{name}: done is {steps} cube steps from merged: a shade, not a hue"
                );
            }
            // …and, in a truecolor preset, a quarter-turn's worth of the
            // wheel's way off it.
            if let Some(deg) = hue_apart(th.done, th.merged) {
                assert!(
                    deg >= 25.0,
                    "{name}: done is {deg:.0}° from merged: a shade, not a hue"
                );
            }
        }
    }

    /// The ONE-SHOT SWEEP an unread finish rides rests on that preset's
    /// `done` — a preset that moves the one moves the other — brightens
    /// toward its head, and is nobody else's sweep.
    #[test]
    fn done_sweep_rests_on_done_in_every_preset() {
        for name in PRESETS {
            let th = Theme::by_name(name);
            assert_eq!(th.done_sweep[0], th.done, "{name}: the sweep rests on done");
            let [tail, mid, head] = th.done_sweep;
            assert!(
                tail != mid && mid != head && tail != head,
                "{name}: a flat band"
            );
            for other in [th.warn_sweep, th.err_sweep, th.merged_sweep] {
                assert!(
                    th.done_sweep.iter().all(|c| !other.contains(c)),
                    "{name}: the done sweep borrows a shade from another"
                );
            }
        }
    }

    /// A LAUNCHER CARD names the checkout its session runs in in a SCOPE
    /// COLOR — `⌂` amber on the project's root branch, `↳` aquamarine in a
    /// worktree of its own — so a screenful of cards sorts into the two
    /// without a word being read. That only holds while the pair keeps a
    /// whole hue between them (the three cube steps `done` keeps from
    /// `merged`), and while neither lands on a color the same card already
    /// wears somewhere else: the status its dot and name take, the `done`
    /// its ago label takes, the `merged` its pull-request row takes, or
    /// the dim its harness takes.
    #[test]
    fn the_two_scope_colors_are_a_hue_apart_in_every_preset() {
        for name in PRESETS {
            let th = Theme::by_name(name);
            assert_ne!(th.root, th.worktree, "{name}: one scope, painted twice");
            let steps = cube_steps(th.root, th.worktree);
            assert!(
                steps.is_none_or(|s| s >= 3),
                "{name}: the scopes are {steps:?} cube steps apart: a shade, not a hue"
            );
            let deg = hue_apart(th.root, th.worktree);
            assert!(
                deg.is_none_or(|d| d >= 60.0),
                "{name}: the scopes are {deg:?}° apart: a shade, not a hue"
            );
            for (role, color) in [
                ("ok", th.ok),
                ("warn", th.warn),
                ("err", th.err),
                ("done", th.done),
                ("merged", th.merged),
                ("dim", th.dim),
                ("muted", th.muted),
            ] {
                assert_ne!(th.root, color, "{name}: a root checkout reads as {role}");
                assert_ne!(th.worktree, color, "{name}: a worktree reads as {role}");
            }
        }
    }

    /// The accent has to stand out from the text it highlights, and the
    /// text from the muted tier under it, in every preset — including the
    /// grayscale one, which is the preset that moves those roles.
    #[test]
    fn accent_text_and_muted_stay_three_tiers_in_every_preset() {
        for name in PRESETS {
            let th = Theme::by_name(name);
            assert_ne!(th.accent, th.text, "{name}: a match highlight vanishes");
            assert_ne!(th.text, th.muted, "{name}: secondary text reads as primary");
            assert_ne!(th.muted, th.dim, "{name}: secondary text reads as a hint");
            assert_ne!(
                th.accent, th.muted,
                "{name}: an unfocused title reads as focused"
            );
        }
    }

    /// A merged pull request is purple in every preset, and that purple is
    /// nobody else's: not the terminated `special`, not the unread `done`
    /// a preset may move around, and not a status that would make a
    /// landed pull request look like it needs someone.
    #[test]
    fn merged_is_purple_and_its_own_color_in_every_preset() {
        for name in PRESETS {
            let th = Theme::by_name(name);
            // The ANSI presets share the one cube purple; a truecolor
            // preset brings its editor's own, which only has to be purple.
            match rgb_hue(th.merged) {
                Some(hue) => assert!(
                    (255.0..=320.0).contains(&hue),
                    "{name}: merged is purple, not hue {hue:.0}"
                ),
                None => assert_eq!(th.merged, Color::Indexed(135), "{name}: merged is purple"),
            }
            assert_ne!(th.merged, th.special, "{name}: merged reads as terminated");
            assert_ne!(th.merged, th.done, "{name}: merged reads as unread done");
            assert_ne!(th.merged, th.ok, "{name}: merged reads as plain success");
            assert_ne!(th.merged, th.warn, "{name}: merged reads as running");
            assert_ne!(th.merged, th.err, "{name}: merged reads as needs-feedback");
            assert_ne!(th.merged, th.accent, "{name}: merged reads as open");
            assert_ne!(th.merged, th.dim, "{name}: merged reads as a draft");
            // The sweep a merged checkout's branch name rides rests on that
            // same purple, and is nobody else's sweep.
            assert_eq!(
                th.merged_sweep[0], th.merged,
                "{name}: the sweep rests on merged"
            );
            assert_ne!(
                th.merged_sweep, th.warn_sweep,
                "{name}: the sweep reads as running"
            );
            assert_ne!(
                th.merged_sweep, th.err_sweep,
                "{name}: the sweep reads as needs-feedback"
            );
        }
    }

    /// `focus_tint` fills the surface keys land in, so it has to be seen —
    /// much darker and it reads as plain black (issue #6) — read as the
    /// accent rather than as gray, which every channel near one value is
    /// on a black window, and stay dark enough that dim text keeps its
    /// contrast: `#666` holds over 3.1:1 of the 3.7:1 it has on black.
    /// Mono is the one gray preset, and takes the one gray tint. A
    /// TRUECOLOR PRESET measures all of that against its own canvas
    /// instead of black: the tint is a visible step off `bg`, hued, and
    /// dim text keeps most of the contrast it has on the canvas — a light
    /// preset's tint being a pale wash, not a near-black.
    #[test]
    fn focus_tint_is_visible_hued_and_dark() {
        for name in PRESETS {
            let th = Theme::by_name(name);
            let Color::Rgb(r, g, b) = th.focus_tint else {
                panic!("{name}: focus_tint must be truecolor RGB");
            };
            let (hi, lo) = (r.max(g).max(b), r.min(g).min(b));
            if let Some(canvas) = th.bg {
                let (cr, cg, cb) = color_rgb(canvas).unwrap();
                let step = u16::from(r.abs_diff(cr))
                    + u16::from(g.abs_diff(cg))
                    + u16::from(b.abs_diff(cb));
                assert!(step >= 15, "{name}: focus_tint vanishes into the canvas");
                assert!(
                    contrast(th.dim, th.focus_tint) >= 0.85 * contrast(th.dim, canvas),
                    "{name}: focus_tint costs dim text its contrast"
                );
                assert!(hi - lo >= 10, "{name}: focus_tint reads as gray");
                assert_eq!(
                    th.light,
                    luminance([cr, cg, cb]) > 0.5,
                    "{name}: `light` says what the canvas is"
                );
                continue;
            }
            assert!(
                hi >= 22,
                "{name}: focus_tint {:?} reads as black",
                (r, g, b)
            );
            assert!(
                luminance([r, g, b]) <= 0.009,
                "{name}: focus_tint {:?} costs dim text its contrast",
                (r, g, b)
            );
            if *name == "mono" {
                assert_eq!(hi, lo, "mono: a gray preset takes a gray tint");
            } else {
                assert!(
                    hi - lo >= 15,
                    "{name}: focus_tint {:?} reads as gray",
                    (r, g, b)
                );
            }
        }
    }

    /// A TRUECOLOR PRESET paints its own canvas, so every tier of text has
    /// to read on it: the text at WCAG AA (4.5:1), the muted tier at 3:1
    /// and the dim hints at 2:1 — their job is to recede, not to vanish —
    /// and each status color at 2.5:1, so a dot is told from the air
    /// around it.
    #[test]
    fn a_truecolor_preset_is_readable_on_its_own_canvas() {
        for name in PRESETS {
            let th = Theme::by_name(name);
            let Some(canvas) = th.bg else { continue };
            assert!(contrast(th.text, canvas) >= 4.5, "{name}: text");
            assert!(contrast(th.muted, canvas) >= 3.0, "{name}: muted");
            assert!(contrast(th.dim, canvas) >= 2.0, "{name}: dim");
            for (role, c) in [
                ("accent", th.accent),
                ("ok", th.ok),
                ("done", th.done),
                ("warn", th.warn),
                ("err", th.err),
                ("merged", th.merged),
                ("root", th.root),
                ("worktree", th.worktree),
            ] {
                assert!(contrast(c, canvas) >= 2.5, "{name}: {role} on the canvas");
            }
            assert!(
                contrast(th.on_accent, th.accent) >= 2.5,
                "{name}: the focused title chip"
            );
        }
    }

    /// The two names that are not presets resolve against the machine:
    /// `auto` by the OS's light or dark, `vscode` to what VS Code gave or
    /// One Dark without it — and a preset name resolves to the preset,
    /// whatever the machine says.
    #[test]
    fn resolve_follows_the_machine_for_auto_and_vscode_only() {
        let dark = Machine {
            os_light: Some(false),
            vscode: None,
        };
        let light = Machine {
            os_light: Some(true),
            vscode: None,
        };
        assert_eq!(resolve("auto", &dark), ONE_DARK);
        assert_eq!(resolve("Auto", &light), ONE_LIGHT);
        assert_eq!(resolve("auto", &Machine::default()), ONE_DARK);
        assert_eq!(
            resolve("vscode", &light),
            ONE_DARK,
            "nothing read: One Dark"
        );
        let read = Machine {
            vscode: Some(Theme::by_name("ocean")),
            ..light.clone()
        };
        assert_eq!(resolve("vscode", &read), Theme::by_name("ocean"));
        assert_eq!(resolve("forest", &light), Theme::by_name("forest"));
        assert!(is_live("auto") && is_live(" VSCode ") && !is_live("one-dark"));
        for name in THEMES {
            assert!(
                PRESETS.contains(name) || is_live(name),
                "{name}: listed but neither a preset nor a source"
            );
        }
        assert!(THEMES.contains(&DEFAULT_THEME));
    }

    /// The syntax roles of an ANSI preset are the roles the highlighter
    /// always borrowed, so moving `special` still moves the keywords.
    #[test]
    fn ansi_presets_color_code_with_their_status_roles() {
        for name in PRESETS {
            let th = Theme::by_name(name);
            if th.truecolor() {
                continue;
            }
            assert_eq!(th.syn_keyword, th.special, "{name}");
            assert_eq!(th.syn_string, th.ok, "{name}");
            assert_eq!(th.syn_comment, th.dim, "{name}");
            assert_eq!(th.syn_number, th.warn, "{name}");
        }
    }

    /// The DIFF VIEWER's washes: an added line reads green and a removed
    /// one red (their strongest channel says so), the changed words stand
    /// further off the canvas than their line does, and neither wash is
    /// the canvas itself or the selection fill a row may also wear.
    #[test]
    fn diff_washes_are_green_and_red_and_the_words_stand_out() {
        for name in PRESETS {
            let th = Theme::by_name(name);
            let canvas = th.bg.unwrap_or(Color::Rgb(0, 0, 0));
            let rgb = |c: Color| color_rgb(c).unwrap();
            for (role, c) in [("add", th.diff_add_bg), ("add word", th.diff_add_word)] {
                let (r, g, b) = rgb(c);
                assert!(g > r && g >= b, "{name}: the {role} wash is green");
            }
            for (role, c) in [("del", th.diff_del_bg), ("del word", th.diff_del_word)] {
                let (r, g, b) = rgb(c);
                assert!(r > g && r >= b, "{name}: the {role} wash is red");
            }
            for (line, word) in [
                (th.diff_add_bg, th.diff_add_word),
                (th.diff_del_bg, th.diff_del_word),
            ] {
                assert!(
                    contrast(word, canvas) > contrast(line, canvas),
                    "{name}: the words stand further off the canvas than the line"
                );
                assert_ne!(line, canvas, "{name}");
                assert_ne!(line, th.sel_bg, "{name}");
            }
        }
    }

    /// `blend` and `hex` agree with the colors they are handed.
    #[test]
    fn blend_runs_from_the_canvas_to_the_color() {
        let c = hex("#E06C75");
        assert_eq!(c, Color::Rgb(0xE0, 0x6C, 0x75));
        assert_eq!(blend(c, None, 1.0), c);
        assert_eq!(blend(c, None, 0.0), Color::Rgb(0, 0, 0));
        assert_eq!(blend(c, Some(hex("#FAFAFA")), 0.0), hex("#FAFAFA"));
        assert_eq!(blend(Color::Reset, None, 0.5), Color::Reset);
        assert_eq!(
            ONE_LIGHT.toward_bg(ONE_LIGHT.accent, 0.1),
            blend(ONE_LIGHT.accent, Some(hex("#FAFAFA")), 0.1)
        );
    }
}
