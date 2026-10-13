//! The `vscode` THEME SOURCE: the palette VS Code itself is wearing, read
//! off the machine, so Bro beside it looks like the rest of the editor.
//!
//! Three reads, each of which may come up empty — and then the theme is
//! One Dark, as with nothing to read at all:
//!
//! 1. VS Code's user `settings.json` (JSONC, `crate::jsonc`): which theme
//!    is set — `workbench.colorTheme`, or with `window.autoDetectColorScheme`
//!    on, `workbench.preferredDarkColorTheme` / `preferredLightColorTheme`
//!    by what the OS says ([`chosen_theme`]).
//! 2. The theme's file: every installed extension's `package.json` lists
//!    the themes it contributes by `label` (what settings name) and `id`,
//!    with a `uiTheme` that says light or dark; VS Code's own built-in
//!    themes are extensions too, under its install ([`find_theme`]).
//!    A theme file may `include` another it builds on, followed here.
//! 3. Its `colors` (the workbench) and `tokenColors` (the editor's
//!    syntax): mapped onto Bro's roles by [`derive`], over One Dark or One
//!    Light as the base, and then held to the rules every preset keeps —
//!    a derived role that would break one (a `merged` that is not purple,
//!    a `done` that reads as a status) gives way to the base's.

use crate::theme::{blend, color_rgb, Theme, ONE_DARK, ONE_LIGHT};
use ratatui::style::Color;
use serde_json::Value;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// The default theme VS Code shows before anyone picks one, for each side
/// of `window.autoDetectColorScheme`.
const DEFAULT_DARK: &str = "Default Dark Modern";
const DEFAULT_LIGHT: &str = "Default Light Modern";

/// How deep a chain of `include`s is followed before it is taken for a
/// loop.
const MAX_INCLUDES: usize = 6;

/// VS Code's user `settings.json` on this OS: `%APPDATA%\Code\User` on
/// Windows, `~/Library/Application Support/Code/User` on macOS,
/// `$XDG_CONFIG_HOME/Code/User` (else `~/.config`) elsewhere.
pub fn settings_path() -> Option<PathBuf> {
    let dir = if cfg!(windows) {
        PathBuf::from(std::env::var_os("APPDATA")?)
    } else if cfg!(target_os = "macos") {
        std::env::home_dir()?.join("Library/Application Support")
    } else {
        match std::env::var_os("XDG_CONFIG_HOME") {
            Some(dir) if !dir.is_empty() => PathBuf::from(dir),
            _ => std::env::home_dir()?.join(".config"),
        }
    };
    Some(dir.join("Code").join("User").join("settings.json"))
}

/// Where VS Code's extensions live: the user's (`~/.vscode/extensions`),
/// then the built-in ones under the install, which hold the default
/// themes.
pub fn extension_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Some(home) = std::env::home_dir() {
        dirs.push(home.join(".vscode").join("extensions"));
    }
    if cfg!(windows) {
        for var in ["LOCALAPPDATA", "ProgramFiles"] {
            if let Some(base) = std::env::var_os(var) {
                let base = PathBuf::from(base);
                let install = if var == "LOCALAPPDATA" {
                    base.join("Programs").join("Microsoft VS Code")
                } else {
                    base.join("Microsoft VS Code")
                };
                dirs.push(install.join("resources").join("app").join("extensions"));
            }
        }
    } else if cfg!(target_os = "macos") {
        dirs.push(PathBuf::from(
            "/Applications/Visual Studio Code.app/Contents/Resources/app/extensions",
        ));
    } else {
        dirs.push(PathBuf::from("/usr/share/code/resources/app/extensions"));
    }
    dirs
}

/// The theme the user's VS Code shows: with `window.autoDetectColorScheme`
/// on and the OS's light or dark known, the preferred theme for that side
/// (VS Code's own defaults where unset); otherwise `workbench.colorTheme`.
pub fn chosen_theme(settings: &Value, os_light: Option<bool>) -> String {
    let get = |key: &str| {
        settings
            .get(key)
            .and_then(Value::as_str)
            .map(str::to_string)
    };
    let auto = settings
        .get("window.autoDetectColorScheme")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    match (auto, os_light) {
        (true, Some(true)) => {
            get("workbench.preferredLightColorTheme").unwrap_or_else(|| DEFAULT_LIGHT.into())
        }
        (true, Some(false)) => {
            get("workbench.preferredDarkColorTheme").unwrap_or_else(|| DEFAULT_DARK.into())
        }
        _ => get("workbench.colorTheme").unwrap_or_else(|| DEFAULT_DARK.into()),
    }
}

/// A theme some extension contributes: the file, and whether it is a
/// light one (`uiTheme` `vs` or `hc-light`).
#[derive(Debug, Clone, PartialEq)]
pub struct ThemeFile {
    pub path: PathBuf,
    pub light: bool,
}

/// Find the theme named `label` (or with that `id`) among the extensions
/// under `dirs`, case-insensitively. The first directory that has it wins,
/// so the user's own extensions come before the built-in ones.
pub fn find_theme(label: &str, dirs: &[PathBuf]) -> Option<ThemeFile> {
    let want = label.trim().to_lowercase();
    for dir in dirs {
        let Ok(entries) = std::fs::read_dir(dir) else {
            continue;
        };
        let mut exts: Vec<PathBuf> = entries.flatten().map(|e| e.path()).collect();
        // Newest version last, so it is the one kept when two are
        // installed side by side.
        exts.sort();
        let mut hit = None;
        for ext in exts {
            let Some(pkg) = crate::jsonc::read(&ext.join("package.json")) else {
                continue;
            };
            let Some(themes) = pkg.pointer("/contributes/themes").and_then(Value::as_array) else {
                continue;
            };
            for t in themes {
                let names = ["label", "id"].map(|k| t.get(k).and_then(Value::as_str));
                if !names.iter().flatten().any(|n| n.to_lowercase() == want) {
                    continue;
                }
                let Some(rel) = t.get("path").and_then(Value::as_str) else {
                    continue;
                };
                let ui = t
                    .get("uiTheme")
                    .and_then(Value::as_str)
                    .unwrap_or("vs-dark");
                hit = Some(ThemeFile {
                    path: ext.join(rel),
                    light: matches!(ui, "vs" | "hc-light"),
                });
            }
        }
        if hit.is_some() {
            return hit;
        }
    }
    None
}

/// A theme file's `colors` and `tokenColors`, with every file it
/// `include`s read first and overridden by what includes it.
#[derive(Debug, Default, Clone)]
pub struct ThemeData {
    pub colors: HashMap<String, String>,
    /// `(scope, foreground)` in file order, a later entry overriding.
    pub tokens: Vec<(String, String)>,
}

/// Read `path`'s theme, following its `include` chain.
pub fn load_theme(path: &Path) -> Option<ThemeData> {
    let mut data = ThemeData::default();
    load_into(path, &mut data, 0)?;
    Some(data)
}

fn load_into(path: &Path, data: &mut ThemeData, depth: usize) -> Option<()> {
    if depth > MAX_INCLUDES {
        return None;
    }
    let json = crate::jsonc::read(path)?;
    if let Some(include) = json.get("include").and_then(Value::as_str) {
        let parent = path.parent().unwrap_or(Path::new("."));
        // A missing base still leaves this file's own colors worth having.
        let _ = load_into(&parent.join(include), data, depth + 1);
    }
    if let Some(colors) = json.get("colors").and_then(Value::as_object) {
        for (k, v) in colors {
            if let Some(v) = v.as_str() {
                data.colors.insert(k.clone(), v.to_string());
            }
        }
    }
    if let Some(tokens) = json.get("tokenColors").and_then(Value::as_array) {
        for t in tokens {
            let Some(fg) = t.pointer("/settings/foreground").and_then(Value::as_str) else {
                continue;
            };
            let scopes: Vec<String> = match t.get("scope") {
                Some(Value::String(s)) => s.split(',').map(|s| s.trim().to_string()).collect(),
                Some(Value::Array(list)) => list
                    .iter()
                    .filter_map(Value::as_str)
                    .map(|s| s.trim().to_string())
                    .collect(),
                _ => continue,
            };
            for scope in scopes {
                data.tokens.push((scope, fg.to_string()));
            }
        }
    }
    Some(())
}

/// A VS Code color string — `#RGB`, `#RGBA`, `#RRGGBB` or `#RRGGBBAA` —
/// with any alpha blended over `under`, the way VS Code composites it.
pub fn parse_color(s: &str, under: Option<Color>) -> Option<Color> {
    let hex = s.trim().strip_prefix('#')?;
    let digits: Vec<u8> = hex
        .chars()
        .map(|c| c.to_digit(16).map(|d| d as u8))
        .collect::<Option<_>>()?;
    let (rgb, alpha) = match digits.len() {
        3 => ([0, 1, 2].map(|i| digits[i] * 17), 255),
        4 => ([0, 1, 2].map(|i| digits[i] * 17), digits[3] * 17),
        6 => ([0, 2, 4].map(|i| digits[i] * 16 + digits[i + 1]), 255),
        8 => (
            [0, 2, 4].map(|i| digits[i] * 16 + digits[i + 1]),
            digits[6] * 16 + digits[7],
        ),
        _ => return None,
    };
    let color = Color::Rgb(rgb[0], rgb[1], rgb[2]);
    if alpha == 255 {
        return Some(color);
    }
    Some(blend(color, under, f32::from(alpha) / 255.0))
}

/// The foreground the theme gives the first of `scopes` it styles: an
/// exact scope first, then a broader one it starts with (`keyword` for
/// `keyword.control`), the later entry winning either way, as in VS Code.
fn token_color(data: &ThemeData, scopes: &[&str], bg: Option<Color>) -> Option<Color> {
    for want in scopes {
        let exact = data.tokens.iter().rev().find(|(s, _)| s == want);
        let broader = || {
            data.tokens
                .iter()
                .rev()
                .find(|(s, _)| want.starts_with(&format!("{s}.")))
        };
        if let Some((_, fg)) = exact.or_else(broader) {
            if let Some(c) = parse_color(fg, bg) {
                return Some(c);
            }
        }
    }
    None
}

/// Bro's palette, from a VS Code theme's colors over the One preset of the
/// same lightness. Every role has a VS Code key it is read from (the
/// terminal's ANSI colors for the statuses, the list and editor keys for
/// the surfaces, the token colors for the syntax) and keeps the base's
/// where the theme does not set it, or sets something that would break a
/// rule every preset keeps ([`repair`]).
pub fn derive(data: &ThemeData, light: bool) -> Theme {
    let base = if light { ONE_LIGHT } else { ONE_DARK };
    let bg = data
        .colors
        .get("editor.background")
        .and_then(|c| parse_color(c, None))
        .or(base.bg);
    let get = |keys: &[&str]| {
        keys.iter()
            .find_map(|k| data.colors.get(*k).and_then(|c| parse_color(c, bg)))
    };
    let text = get(&["editor.foreground", "foreground"]).unwrap_or(base.text);
    let accent =
        get(&["terminal.ansiBlue", "textLink.foreground", "focusBorder"]).unwrap_or(base.accent);
    let comment = token_color(data, &["comment"], bg);
    let dim = get(&["editorLineNumber.foreground"])
        .or(comment)
        .unwrap_or(base.dim);
    let muted = get(&["descriptionForeground"]).unwrap_or_else(|| blend(text, bg, 0.75));
    let ok = get(&[
        "terminal.ansiGreen",
        "gitDecoration.addedResourceForeground",
    ])
    .unwrap_or(base.ok);
    let warn = get(&["terminal.ansiYellow", "editorWarning.foreground"]).unwrap_or(base.warn);
    let err = get(&[
        "terminal.ansiRed",
        "errorForeground",
        "editorError.foreground",
    ])
    .unwrap_or(base.err);
    let merged = get(&["terminal.ansiMagenta"]).unwrap_or(base.merged);
    let worktree = get(&["terminal.ansiCyan"]).unwrap_or(base.worktree);
    let number = token_color(data, &["constant.numeric", "constant"], bg);
    let sweep = |c: Color| {
        // A sweep brightens toward its head on a dark canvas, and darkens
        // toward it on a light one — the direction that stands out.
        let to = if light {
            Color::Rgb(0, 0, 0)
        } else {
            Color::Rgb(255, 255, 255)
        };
        [c, blend(to, Some(c), 0.3), blend(to, Some(c), 0.6)]
    };
    let mut th = Theme {
        accent,
        on_accent: bg.unwrap_or(base.on_accent),
        text,
        muted,
        dim,
        ok,
        done: base.done,
        warn,
        err,
        special: base.special,
        merged,
        root: number.unwrap_or(base.root),
        worktree,
        sel_bg: get(&[
            "list.activeSelectionBackground",
            "editor.selectionBackground",
        ])
        .unwrap_or(base.sel_bg),
        sel_bg_dim: get(&["list.hoverBackground", "list.inactiveSelectionBackground"])
            .unwrap_or(base.sel_bg_dim),
        edge: get(&[
            "editorGroup.border",
            "panel.border",
            "editorIndentGuide.background",
        ])
        .unwrap_or(base.edge),
        warn_sweep: sweep(warn),
        err_sweep: sweep(err),
        merged_sweep: sweep(merged),
        done_sweep: base.done_sweep,
        focus_tint: blend(accent, bg, 0.12),
        bg,
        light,
        syn_keyword: token_color(data, &["keyword", "storage.type", "storage"], bg)
            .unwrap_or(base.syn_keyword),
        syn_string: token_color(data, &["string"], bg).unwrap_or(base.syn_string),
        syn_comment: comment.unwrap_or(base.syn_comment),
        syn_number: number.unwrap_or(base.syn_number),
        nested_bright: blend(text, bg, 1.15),
        nested_dim: dim,
        link: get(&["textLink.foreground"]).unwrap_or(accent),
    };
    repair(&mut th, &base);
    th
}

/// Hold a derived palette to the rules every preset keeps
/// (`theme::tests`), putting the base's role back wherever the theme's
/// own breaks one: `merged` purple and nobody else's, `done` a color no
/// status wears, the two SCOPE COLORS apart and off every status, three
/// tiers of text, a running frame that is not the cursor's.
fn repair(th: &mut Theme, base: &Theme) {
    let hue = |c: Color| {
        let (r, g, b) = color_rgb(c)?;
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
    };
    let apart = |a: Color, b: Color| match (hue(a), hue(b)) {
        (Some(a), Some(b)) => {
            let d = (a - b).abs();
            d.min(360.0 - d)
        }
        _ => 0.0,
    };
    if !hue(th.merged).is_some_and(|h| (255.0..=320.0).contains(&h))
        || [th.ok, th.warn, th.err, th.accent, th.dim, th.special].contains(&th.merged)
    {
        th.merged = base.merged;
        th.merged_sweep = base.merged_sweep;
    }
    if [th.ok, th.warn, th.err, th.special, th.dim, th.accent].contains(&th.done)
        || apart(th.done, th.merged) < 25.0
    {
        th.done = base.done;
        th.done_sweep = base.done_sweep;
    }
    let statuses = [th.ok, th.warn, th.err, th.done, th.merged, th.dim, th.muted];
    if statuses.contains(&th.root) {
        th.root = base.root;
    }
    if statuses.contains(&th.worktree) || apart(th.root, th.worktree) < 60.0 {
        th.worktree = base.worktree;
        if apart(th.root, th.worktree) < 60.0 {
            th.root = base.root;
        }
    }
    if th.warn == th.accent || th.warn == th.edge {
        th.warn = base.warn;
        th.warn_sweep = base.warn_sweep;
    }
    if th.text == th.muted || th.muted == th.dim || th.accent == th.text || th.accent == th.muted {
        th.text = base.text;
        th.muted = base.muted;
        th.dim = base.dim;
    }
    for sweep in [th.warn_sweep, th.err_sweep, th.merged_sweep] {
        if th.done_sweep.iter().any(|c| sweep.contains(c)) {
            th.done_sweep = base.done_sweep;
            th.done = base.done;
        }
    }
}

/// What the `vscode` source resolves to on this machine right now: the
/// theme its settings name, found and derived. None when VS Code, its
/// settings or the theme can't be found — the caller falls back to One
/// Dark.
pub fn load(os_light: Option<bool>) -> Option<Theme> {
    let settings = crate::jsonc::read(&settings_path()?)?;
    let label = chosen_theme(&settings, os_light);
    let file = find_theme(&label, &extension_dirs())?;
    let data = load_theme(&file.path)?;
    Some(derive(&data, file.light))
}

/// When VS Code's settings last changed, so a beat can tell a theme
/// switched in the editor from one that has not, by a `stat`.
pub fn settings_stamp() -> Option<std::time::SystemTime> {
    std::fs::metadata(settings_path()?).ok()?.modified().ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// An extension directory with one theme in it, `include`ing a base,
    /// the way VS Code's own Dark Modern builds on Dark+.
    fn fixture(dir: &Path) -> PathBuf {
        let ext = dir.join("acme.theme-1.0.0");
        std::fs::create_dir_all(ext.join("themes")).unwrap();
        std::fs::write(
            ext.join("package.json"),
            r#"{ "contributes": { "themes": [
                { "label": "Acme Night", "uiTheme": "vs-dark", "path": "./themes/night.json" },
                { "label": "Acme Day", "id": "acme-day", "uiTheme": "vs", "path": "./themes/day.json" },
            ] } }"#,
        )
        .unwrap();
        std::fs::write(
            ext.join("themes/base.json"),
            r##"{ "colors": { "editor.background": "#101010", "terminal.ansiGreen": "#00ff00" },
                 "tokenColors": [ { "scope": "comment", "settings": { "foreground": "#777777" } } ] }"##,
        )
        .unwrap();
        std::fs::write(
            ext.join("themes/night.json"),
            r##"{
                // the dark one
                "include": "./base.json",
                "colors": {
                    "editor.background": "#1E1E2E",
                    "editor.foreground": "#CDD6F4",
                    "terminal.ansiBlue": "#89B4FA",
                    "terminal.ansiRed": "#F38BA8",
                    "terminal.ansiYellow": "#F9E2AF",
                    "terminal.ansiMagenta": "#CBA6F7",
                    "terminal.ansiCyan": "#94E2D5",
                    "list.activeSelectionBackground": "#FFFFFF20",
                },
                "tokenColors": [
                    { "scope": ["keyword", "storage.type"], "settings": { "foreground": "#CBA6F7" } },
                    { "scope": "string, string.quoted", "settings": { "foreground": "#A6E3A1" } },
                    { "scope": "constant.numeric", "settings": { "foreground": "#FAB387" } },
                ],
            }"##,
        )
        .unwrap();
        std::fs::write(
            ext.join("themes/day.json"),
            r##"{ "colors": { "editor.background": "#FFFFFF", "editor.foreground": "#222222" } }"##,
        )
        .unwrap();
        dir.to_path_buf()
    }

    /// Settings pick the theme: `workbench.colorTheme`, or with auto-detect
    /// on, the preferred theme for the OS's side — VS Code's own defaults
    /// where those are unset.
    #[test]
    fn the_chosen_theme_follows_settings_and_auto_detect() {
        let s = json!({ "workbench.colorTheme": "Atom One Dark" });
        assert_eq!(chosen_theme(&s, Some(true)), "Atom One Dark");
        let s = json!({
            "workbench.colorTheme": "Atom One Dark",
            "window.autoDetectColorScheme": true,
            "workbench.preferredLightColorTheme": "Atom One Light",
        });
        assert_eq!(chosen_theme(&s, Some(true)), "Atom One Light");
        assert_eq!(chosen_theme(&s, Some(false)), DEFAULT_DARK);
        assert_eq!(chosen_theme(&s, None), "Atom One Dark", "OS unknown");
        assert_eq!(chosen_theme(&json!({}), None), DEFAULT_DARK);
    }

    /// The theme is found by label or id, its lightness read off
    /// `uiTheme`, and its `include` chain merged base-first.
    #[test]
    fn a_theme_is_found_and_its_includes_merged() {
        let tmp = tempfile::tempdir().unwrap();
        let dirs = vec![fixture(tmp.path())];
        let night = find_theme("acme night", &dirs).expect("found by label");
        assert!(!night.light);
        let day = find_theme("acme-day", &dirs).expect("found by id");
        assert!(day.light);
        assert_eq!(find_theme("Nope", &dirs), None);

        let data = load_theme(&night.path).unwrap();
        assert_eq!(
            data.colors["editor.background"], "#1E1E2E",
            "the child wins"
        );
        assert_eq!(
            data.colors["terminal.ansiGreen"], "#00ff00",
            "the base fills in"
        );
        assert!(data.tokens.contains(&("comment".into(), "#777777".into())));
        assert!(data
            .tokens
            .contains(&("string.quoted".into(), "#A6E3A1".into())));
    }

    /// The derived palette takes the theme's own colors where it has them
    /// and keeps the base's where it does not.
    #[test]
    fn derive_maps_colors_and_tokens_onto_roles() {
        let tmp = tempfile::tempdir().unwrap();
        let dirs = vec![fixture(tmp.path())];
        let file = find_theme("Acme Night", &dirs).unwrap();
        let th = derive(&load_theme(&file.path).unwrap(), file.light);
        assert_eq!(th.bg, Some(Color::Rgb(0x1e, 0x1e, 0x2e)));
        assert_eq!(th.text, Color::Rgb(0xcd, 0xd6, 0xf4));
        assert_eq!(th.accent, Color::Rgb(0x89, 0xb4, 0xfa));
        assert_eq!(th.ok, Color::Rgb(0, 0xff, 0));
        assert_eq!(th.merged, Color::Rgb(0xcb, 0xa6, 0xf7));
        assert_eq!(th.syn_keyword, Color::Rgb(0xcb, 0xa6, 0xf7));
        assert_eq!(th.syn_string, Color::Rgb(0xa6, 0xe3, 0xa1));
        assert_eq!(th.syn_comment, Color::Rgb(0x77, 0x77, 0x77));
        assert_eq!(th.syn_number, Color::Rgb(0xfa, 0xb3, 0x87));
        assert_eq!(
            th.root, th.syn_number,
            "the warm scope is the number orange"
        );
        assert!(!th.light);
        // `#FFFFFF20` is white at an eighth, over the canvas.
        assert_eq!(
            th.sel_bg,
            blend(Color::Rgb(255, 255, 255), th.bg, 32.0 / 255.0)
        );

        let file = find_theme("Acme Day", &dirs).unwrap();
        let th = derive(&load_theme(&file.path).unwrap(), file.light);
        assert!(th.light);
        assert_eq!(th.bg, Some(Color::Rgb(255, 255, 255)));
        assert_eq!(th.ok, ONE_LIGHT.ok, "unset: the One Light base");
    }

    /// A theme whose magenta is not purple, or whose cyan sits on its
    /// orange, gives those roles back to the base.
    #[test]
    fn repair_restores_roles_a_theme_would_break() {
        let mut data = ThemeData::default();
        for (k, v) in [
            ("editor.background", "#000000"),
            ("terminal.ansiMagenta", "#FF0000"),
            ("terminal.ansiRed", "#FF0000"),
            ("terminal.ansiCyan", "#D19A66"),
        ] {
            data.colors.insert(k.into(), v.into());
        }
        let th = derive(&data, false);
        assert_eq!(th.merged, ONE_DARK.merged, "red is not a merge");
        assert_ne!(th.root, th.worktree);
    }

    #[test]
    fn colors_parse_in_every_vscode_spelling() {
        assert_eq!(
            parse_color("#abc", None),
            Some(Color::Rgb(0xaa, 0xbb, 0xcc))
        );
        assert_eq!(
            parse_color("#A0B0C0", None),
            Some(Color::Rgb(0xa0, 0xb0, 0xc0))
        );
        assert_eq!(
            parse_color("#ffffff80", Some(Color::Rgb(0, 0, 0))),
            Some(Color::Rgb(128, 128, 128))
        );
        assert_eq!(parse_color("#fff8", None), Some(Color::Rgb(136, 136, 136)));
        assert_eq!(parse_color("red", None), None);
        assert_eq!(parse_color("#12345", None), None);
    }
}
