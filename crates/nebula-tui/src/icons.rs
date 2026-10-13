//! FILE ICONS: a per-file-type icon and color wherever files are listed —
//! the TREE BROWSER, the FILE FINDER, the DIFF VIEWER's file list and tree,
//! the FILE TABS — the way Material Icon Theme marks them in VS Code's
//! explorer beside it.
//!
//! The associations are Material Icon Theme's own (file names, file
//! extensions, folder names), cut down to the icons a Nerd Font has a
//! glyph for and vendored as `icons_table` by `scripts/icons/gen-icons.mjs`
//! (MIT; credited there), so they work without VS Code installed. Each
//! icon is drawn in the fill color of its material SVG.
//!
//! The ICON STYLE (`icons` in config.json) picks how they are drawn:
//! `nerd` — the glyph, which needs a Nerd Font in the terminal; `ascii` —
//! a two-letter tag in the icon's color (`rs`, `ts`, `{}`), which any font
//! has; `off`. Nothing can see which font the terminal draws with, so
//! `auto`, the default, guesses: `nerd` on Windows when the Windows
//! Terminal profile Kete runs in names a Nerd Font (`Nerd` or ` NF` in its
//! `font.face`), else `ascii`. `kete setup terminal` installs one and points
//! a profile at it.

use crate::icons_table::{
    IconDef, FILE, FILE_EXTENSIONS, FILE_NAMES, FOLDER, FOLDER_NAMES, FOLDER_OPEN, ICONS,
};
use ratatui::style::{Color, Style};
use ratatui::text::Span;

/// How FILE ICONS are drawn (the module docs).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum IconStyle {
    Nerd,
    #[default]
    Ascii,
    Off,
}

/// What the `icons` setting cycles through.
pub use nebula_config::choices::ICON_STYLES;

impl IconStyle {
    /// The setting's word, `auto` resolved against the terminal.
    pub fn from_setting(word: &str) -> Self {
        match word.trim().to_ascii_lowercase().as_str() {
            "nerd" => IconStyle::Nerd,
            "ascii" => IconStyle::Ascii,
            "off" => IconStyle::Off,
            _ => detect(),
        }
    }
}

/// `auto` on this machine ([`detect_from`] over the environment and
/// Windows Terminal's settings). Off Windows, or in this crate's tests —
/// whose rendered rows must not hang on the font of the machine they run
/// on — `ascii`.
pub fn detect() -> IconStyle {
    if !cfg!(windows) || cfg!(test) {
        return IconStyle::Ascii;
    }
    let in_wt = std::env::var_os("WT_SESSION").is_some();
    let settings = if in_wt {
        crate::wt_settings::path().and_then(|p| crate::jsonc::read(&p))
    } else {
        None
    };
    let profile = std::env::var("WT_PROFILE_ID").ok();
    detect_from(in_wt, profile.as_deref(), settings.as_ref())
}

/// `auto`: a Nerd Font in the Windows Terminal profile Kete runs in, by
/// its `font.face` — the profile `profile` (`WT_PROFILE_ID`) names, else
/// the default profile's, else the profiles' defaults. Only inside
/// Windows Terminal (`in_wt`, its `WT_SESSION` set): in the VS Code
/// terminal or a console window the font is another one, which nothing
/// can read, so `ascii`. With no settings to read, `ascii`.
pub fn detect_from(
    in_wt: bool,
    profile: Option<&str>,
    settings: Option<&serde_json::Value>,
) -> IconStyle {
    let face = settings
        .filter(|_| in_wt)
        .and_then(|s| crate::wt_settings::font_face(s, profile));
    match face {
        Some(face) if is_nerd_font(&face) => IconStyle::Nerd,
        _ => IconStyle::Ascii,
    }
}

/// Does a font face name say it is a Nerd Font patch? `JetBrainsMono
/// Nerd Font`, `CaskaydiaCove NF`, `FiraCode Nerd Font Mono`.
pub fn is_nerd_font(face: &str) -> bool {
    face.contains("Nerd") || face.contains(" NF")
}

fn find(table: &[(&str, u16)], key: &str) -> Option<&'static IconDef> {
    table
        .binary_search_by(|(k, _)| (*k).cmp(key))
        .ok()
        .map(|i| &ICONS[table[i].1 as usize])
}

/// The icon for a file at `path` (its last component counts): its whole
/// name first (`package.json`, `Dockerfile`), then its extensions from the
/// longest (`d.ts` before `ts`), then the generic file — Material Icon
/// Theme's own order.
pub fn file_icon(path: &str) -> &'static IconDef {
    let name = path
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or(path)
        .to_lowercase();
    if let Some(icon) = find(FILE_NAMES, &name) {
        return icon;
    }
    let mut rest = name.as_str();
    while let Some((_, ext)) = rest.split_once('.') {
        if let Some(icon) = find(FILE_EXTENSIONS, ext) {
            return icon;
        }
        rest = ext;
    }
    &ICONS[FILE as usize]
}

/// The icon for a folder called `name` (`src`, `.github`): its own, or
/// the plain folder.
pub fn folder_icon(name: &str) -> &'static IconDef {
    let name = name
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or(name)
        .to_lowercase();
    find(FOLDER_NAMES, &name).unwrap_or(&ICONS[FOLDER as usize])
}

/// A folder row's icon span: the folder's own icon, and — open, with the
/// plain folder glyph — the open folder's glyph in the folder's color.
pub fn folder_span(name: &str, open: bool, style: IconStyle, light: bool) -> Option<Span<'static>> {
    let icon = folder_icon(name);
    let plain = ICONS[FOLDER as usize].glyph;
    if style == IconStyle::Nerd && open && icon.glyph == plain {
        let glyph = ICONS[FOLDER_OPEN as usize].glyph;
        return Some(Span::styled(
            format!("{glyph} "),
            Style::default().fg(color(icon, light)),
        ));
    }
    span(icon, style, light)
}

/// The icon's color. On a light theme the paler material fills are taken
/// a third of the way to black, so a yellow JSON or a light-blue Markdown
/// icon still reads on `#FAFAFA`.
pub fn color(icon: &IconDef, light: bool) -> Color {
    let (r, g, b) = icon.rgb;
    if light && luminance(r, g, b) > 0.45 {
        let f = |v: u8| (f32::from(v) * 0.66).round() as u8;
        return Color::Rgb(f(r), f(g), f(b));
    }
    Color::Rgb(r, g, b)
}

fn luminance(r: u8, g: u8, b: u8) -> f32 {
    (0.2126 * f32::from(r) + 0.7152 * f32::from(g) + 0.0722 * f32::from(b)) / 255.0
}

/// The span a list row starts with for `icon` in `style`: the glyph and a
/// space (a Nerd Font glyph may draw wider than its cell, so it gets air),
/// or the tag and a space, colored; nothing at all with the icons off, or
/// for a folder in `ascii`, whose fold marker already says what it is.
pub fn span(icon: &IconDef, style: IconStyle, light: bool) -> Option<Span<'static>> {
    let text = match style {
        IconStyle::Off => return None,
        IconStyle::Nerd => format!("{} ", icon.glyph),
        IconStyle::Ascii if icon.tag.is_empty() => return None,
        IconStyle::Ascii => format!("{:<2} ", icon.tag),
    };
    Some(Span::styled(text, Style::default().fg(color(icon, light))))
}

/// How many columns [`span`] takes, for laying out what follows it.
pub fn width(icon: &IconDef, style: IconStyle) -> usize {
    match style {
        IconStyle::Off => 0,
        IconStyle::Nerd => 2,
        IconStyle::Ascii if icon.tag.is_empty() => 0,
        IconStyle::Ascii => 3,
    }
}

/// A file row's icon span, by path: [`file_icon`] then [`span`].
pub fn file_span(path: &str, style: IconStyle, light: bool) -> Option<Span<'static>> {
    span(file_icon(path), style, light)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The tables are sorted, as the binary search needs, and every index
    /// is an icon.
    #[test]
    fn the_vendored_tables_are_sorted_and_sound() {
        for table in [FILE_NAMES, FILE_EXTENSIONS, FOLDER_NAMES] {
            assert!(table.windows(2).all(|w| w[0].0 < w[1].0), "sorted");
            assert!(table.iter().all(|(_, i)| (*i as usize) < ICONS.len()));
        }
        assert!(ICONS.windows(2).all(|w| w[0].name < w[1].name));
    }

    /// Names before extensions, the longest extension first, the generic
    /// file last; case does not matter.
    #[test]
    fn a_file_takes_its_names_icon_then_its_extensions() {
        assert_eq!(file_icon("crates/x/src/main.rs").name, "rust");
        assert_eq!(file_icon("web/package.json").name, "nodejs");
        assert_eq!(file_icon("Cargo.toml").name, "rust");
        assert_eq!(file_icon("app/Dockerfile").name, "docker");
        assert_eq!(file_icon("README.md").name, "readme");
        assert_eq!(file_icon("notes.md").name, "markdown");
        assert_eq!(file_icon("index.ts").name, "typescript");
        assert_eq!(file_icon("App.TSX").name, "react_ts");
        assert_eq!(file_icon(".gitignore").name, "git");
        assert_eq!(file_icon("data.weird-ext").name, "file");
        assert_eq!(file_icon("Makefile").name, "makefile");
    }

    #[test]
    fn folders_take_their_names_icon_and_open() {
        let src = folder_icon("crates/x/src");
        assert_eq!(src.name, "folder-src");
        assert_eq!(folder_icon(".github").name, "folder-github");
        assert_eq!(folder_icon("whatever").name, "folder");
        let open = folder_span("src", true, IconStyle::Nerd, false).unwrap();
        let shut = folder_span("src", false, IconStyle::Nerd, false).unwrap();
        assert_ne!(open.content, shut.content, "the open glyph");
        assert_eq!(open.style.fg, shut.style.fg, "in the folder's own color");
        let gh = folder_span(".github", true, IconStyle::Nerd, false).unwrap();
        assert_eq!(gh.content, format!("{} ", folder_icon(".github").glyph));
    }

    /// Each style draws what it says: the glyph, the tag, or nothing; a
    /// folder has no tag in `ascii`.
    #[test]
    fn the_styles_draw_glyph_tag_or_nothing() {
        let rs = file_icon("a.rs");
        let nerd = span(rs, IconStyle::Nerd, false).unwrap();
        assert_eq!(nerd.content, format!("{} ", rs.glyph));
        assert_eq!(
            nerd.style.fg,
            Some(Color::Rgb(rs.rgb.0, rs.rgb.1, rs.rgb.2))
        );
        assert_eq!(span(rs, IconStyle::Ascii, false).unwrap().content, "rs ");
        assert!(span(rs, IconStyle::Off, false).is_none());
        assert!(span(folder_icon("src"), IconStyle::Ascii, false).is_none());
        assert_eq!(width(rs, IconStyle::Nerd), 2);
        assert_eq!(IconStyle::from_setting("OFF"), IconStyle::Off);
        assert!(is_nerd_font("JetBrainsMono Nerd Font"));
        assert!(is_nerd_font("CaskaydiaCove NF"));
        assert!(!is_nerd_font("Cascadia Mono"));
    }

    /// `auto` is `nerd` only inside Windows Terminal, in a profile whose
    /// font is a Nerd Font — its own, or the default profile's with no
    /// profile id to go on.
    #[test]
    fn auto_wants_windows_terminal_and_a_nerd_font() {
        let settings = serde_json::json!({
            "defaultProfile": "{a}",
            "profiles": { "list": [
                { "guid": "{a}", "font": { "face": "JetBrainsMono Nerd Font" } },
                { "guid": "{b}", "font": { "face": "Cascadia Mono" } },
            ] }
        });
        assert_eq!(
            detect_from(true, Some("{a}"), Some(&settings)),
            IconStyle::Nerd
        );
        assert_eq!(detect_from(true, None, Some(&settings)), IconStyle::Nerd);
        assert_eq!(
            detect_from(true, Some("{b}"), Some(&settings)),
            IconStyle::Ascii
        );
        assert_eq!(
            detect_from(false, Some("{a}"), Some(&settings)),
            IconStyle::Ascii,
            "the VS Code terminal, say"
        );
        assert_eq!(detect_from(true, Some("{a}"), None), IconStyle::Ascii);
    }

    /// A pale icon is darkened on a light theme, a dark one left alone.
    #[test]
    fn pale_icons_darken_on_a_light_theme() {
        let json = file_icon("a.json");
        assert_ne!(color(json, true), color(json, false));
        let Color::Rgb(r, ..) = color(json, true) else {
            panic!()
        };
        assert!(r < json.rgb.0);
    }
}
