//! VS CODE'S THEME, READ OFF THE MACHINE: which theme VS Code is set to,
//! the file it lives in and the colours in it — the reading both front
//! ends do before each maps the colours onto its own roles (the TUI's
//! `vscode_theme`, the desktop window's `look`). Nothing here draws, and a
//! colour leaves as text or [`Rgba`].
//!
//! Three reads, each of which may come up empty:
//!
//! 1. VS Code's user `settings.json` (JSONC, [`crate::jsonc`]): which theme
//!    is set — `workbench.colorTheme`, or with `window.autoDetectColorScheme`
//!    on, `workbench.preferredDarkColorTheme` / `preferredLightColorTheme`
//!    by what the OS says ([`chosen_theme`]).
//! 2. The theme's file: every installed extension's `package.json` lists
//!    the themes it contributes by `label` (what settings name) and `id`,
//!    with a `uiTheme` that says light or dark; VS Code's own built-in
//!    themes are extensions too, under its install ([`find_theme`]).
//!    A theme file may `include` another it builds on, followed here.
//! 3. Its `colors` (the workbench) and `tokenColors` (the editor's syntax),
//!    as [`ThemeData`].

use serde_json::Value;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// The default theme VS Code shows before anyone picks one, for each side
/// of `window.autoDetectColorScheme`.
pub const DEFAULT_DARK: &str = "Default Dark Modern";
pub const DEFAULT_LIGHT: &str = "Default Light Modern";

/// How deep a chain of `include`s is followed before it is taken for a
/// loop.
const MAX_INCLUDES: usize = 6;

/// A colour as VS Code writes one, alpha and all.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rgba {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Rgba {
    pub const fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b, a: 255 }
    }

    /// `#RGB`, `#RGBA`, `#RRGGBB` or `#RRGGBBAA`; None for anything else
    /// (a colour name, a `var()`).
    pub fn parse(s: &str) -> Option<Rgba> {
        let hex = s.trim().strip_prefix('#')?;
        let digits: Vec<u8> = hex
            .chars()
            .map(|c| c.to_digit(16).map(|d| d as u8))
            .collect::<Option<_>>()?;
        let ([r, g, b], a) = match digits.len() {
            3 => ([0, 1, 2].map(|i| digits[i] * 17), 255),
            4 => ([0, 1, 2].map(|i| digits[i] * 17), digits[3] * 17),
            6 => ([0, 2, 4].map(|i| digits[i] * 16 + digits[i + 1]), 255),
            8 => (
                [0, 2, 4].map(|i| digits[i] * 16 + digits[i + 1]),
                digits[6] * 16 + digits[7],
            ),
            _ => return None,
        };
        Some(Rgba { r, g, b, a })
    }

    pub fn opaque(self) -> bool {
        self.a == 255
    }

    /// This colour composited over `under`, the way VS Code paints a
    /// translucent one: opaque wherever `under` is.
    pub fn over(self, under: Rgba) -> Rgba {
        if self.a == 255 {
            return self;
        }
        let a = f32::from(self.a) / 255.0;
        let ua = f32::from(under.a) / 255.0;
        let out_a = a + ua * (1.0 - a);
        let ch = |top: u8, bottom: u8| {
            if out_a <= 0.0 {
                return 0;
            }
            ((f32::from(top) * a + f32::from(bottom) * ua * (1.0 - a)) / out_a)
                .round()
                .clamp(0.0, 255.0) as u8
        };
        Rgba {
            r: ch(self.r, under.r),
            g: ch(self.g, under.g),
            b: ch(self.b, under.b),
            a: (out_a * 255.0).round().clamp(0.0, 255.0) as u8,
        }
    }

    /// `#rrggbb`, or `#rrggbbaa` when it is translucent.
    pub fn hex(self) -> String {
        if self.a == 255 {
            format!("#{:02x}{:02x}{:02x}", self.r, self.g, self.b)
        } else {
            format!("#{:02x}{:02x}{:02x}{:02x}", self.r, self.g, self.b, self.a)
        }
    }
}

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

/// When VS Code's settings last changed, so a beat can tell a theme
/// switched in the editor from one that has not, by a `stat`.
pub fn settings_stamp() -> Option<std::time::SystemTime> {
    std::fs::metadata(settings_path()?).ok()?.modified().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colours_parse_in_every_vscode_spelling_and_composite() {
        assert_eq!(Rgba::parse("#abc"), Some(Rgba::rgb(0xaa, 0xbb, 0xcc)));
        assert_eq!(Rgba::parse("#A0B0C0"), Some(Rgba::rgb(0xa0, 0xb0, 0xc0)));
        let half = Rgba::parse("#ffffff80").unwrap();
        assert_eq!(half.a, 0x80);
        assert_eq!(half.over(Rgba::rgb(0, 0, 0)), Rgba::rgb(128, 128, 128));
        assert_eq!(Rgba::parse("#fff8").unwrap().a, 0x88);
        assert_eq!(Rgba::parse("red"), None);
        assert_eq!(Rgba::parse("#12345"), None);
        assert_eq!(Rgba::rgb(1, 2, 255).hex(), "#0102ff");
        assert_eq!(half.hex(), "#ffffff80");
    }
}
