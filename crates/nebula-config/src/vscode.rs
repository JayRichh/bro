//! VS CODE'S THEME, READ OFF THE MACHINE: which theme VS Code is set to,
//! the file it lives in and the colours in it — the reading both front
//! ends do before each maps the colours onto its own roles (the TUI's
//! `vscode_theme`, the desktop window's [`crate::look`]). Nothing here
//! draws, and a colour leaves as text or [`Rgba`].
//!
//! Four reads, each of which may come up empty:
//!
//! 1. Where VS Code is ([`install`]): its user `settings.json`, the user's
//!    extensions and the built-in ones under the install, stable first,
//!    then Insiders.
//! 2. Its `settings.json` (JSONC, [`crate::jsonc`]): which theme is set —
//!    `workbench.colorTheme`, or with `window.autoDetectColorScheme` on,
//!    `workbench.preferredDarkColorTheme` / `preferredLightColorTheme` by
//!    what the OS says ([`chosen_theme`]) — the colours it overrides
//!    (`workbench.colorCustomizations`, [`customize`]) and the fonts
//!    ([`fonts`]).
//! 3. The theme's file: every installed extension's `package.json` lists
//!    the themes it contributes by `id` and `label` (what settings name;
//!    a `%key%` label is looked up in `package.nls.json`), with a
//!    `uiTheme` that says light or dark ([`themes_in`], [`find_theme`]).
//! 4. Its `colors` (the workbench) and `tokenColors` (the editor's syntax),
//!    with every file it `include`s read first, as [`ThemeData`].

use serde::Serialize;
use serde_json::Value;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// The default theme VS Code shows before anyone picks one, for each side
/// of `window.autoDetectColorScheme`. Newer releases name them without
/// the `Default ` ([`find_theme`] takes either).
pub const DEFAULT_DARK: &str = "Default Dark Modern";
pub const DEFAULT_LIGHT: &str = "Default Light Modern";

/// How deep a chain of `include`s is followed before it is taken for a
/// loop.
const MAX_INCLUDES: usize = 6;

/// The biggest theme or package file read. Real ones are tens of KiB; a
/// file picked by hand could be anything.
pub const MAX_FILE_BYTES: u64 = 4 * 1024 * 1024;

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

/// One VS Code on this machine: where its settings and its extensions are.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Install {
    /// `VS Code` or `VS Code Insiders`, for what the window says.
    pub name: &'static str,
    /// The user's `settings.json` (there or not).
    pub settings: PathBuf,
    /// The folder the user's extensions are installed in.
    pub extensions: PathBuf,
    /// The built-in extensions under the install, which hold the default
    /// themes; None where the install was not found.
    pub builtin: Option<PathBuf>,
}

impl Install {
    /// The folders [`find_theme`] and [`themes_in`] look through: the
    /// user's extensions first, so one of theirs wins over a built-in of
    /// the same name.
    pub fn extension_dirs(&self) -> Vec<PathBuf> {
        let mut dirs = vec![self.extensions.clone()];
        dirs.extend(self.builtin.clone());
        dirs
    }
}

/// Every VS Code this reader knows how to look for, stable first, then
/// Insiders, whether or not it is installed.
///
/// This is the one place that knows VS Code's paths: when the editors
/// detection in `nebula_core` (product.json's `nameShort` and
/// `dataFolderName`) can say where each member of the family keeps its
/// settings and extensions, this is the function to point at it.
pub fn installs() -> Vec<Install> {
    #[cfg(any(test, feature = "test-support"))]
    if let Some(held) = INSTALLS.with(|i| i.borrow().clone()) {
        return held;
    }
    let home = std::env::home_dir();
    let config = if cfg!(windows) {
        std::env::var_os("APPDATA").map(PathBuf::from)
    } else if cfg!(target_os = "macos") {
        home.as_ref().map(|h| h.join("Library/Application Support"))
    } else {
        match std::env::var_os("XDG_CONFIG_HOME") {
            Some(dir) if !dir.is_empty() => Some(PathBuf::from(dir)),
            _ => home.as_ref().map(|h| h.join(".config")),
        }
    };
    let flavours: [(&'static str, &str, &str, &[&str]); 2] = [
        (
            "VS Code",
            "Code",
            ".vscode",
            if cfg!(windows) {
                &["Microsoft VS Code"]
            } else if cfg!(target_os = "macos") {
                &["/Applications/Visual Studio Code.app/Contents/Resources/app"]
            } else {
                &["/usr/share/code", "/snap/code/current/usr/share/code"]
            },
        ),
        (
            "VS Code Insiders",
            "Code - Insiders",
            ".vscode-insiders",
            if cfg!(windows) {
                &["Microsoft VS Code Insiders"]
            } else if cfg!(target_os = "macos") {
                &["/Applications/Visual Studio Code - Insiders.app/Contents/Resources/app"]
            } else {
                &["/usr/share/code-insiders"]
            },
        ),
    ];
    let mut out = Vec::new();
    for (name, settings_dir, data_dir, roots) in flavours {
        let roots: Vec<PathBuf> = if cfg!(windows) {
            let mut list = Vec::new();
            for (var, under) in [("LOCALAPPDATA", Some("Programs")), ("ProgramFiles", None)] {
                if let Some(base) = std::env::var_os(var) {
                    let base = PathBuf::from(base);
                    let base = under.map_or(base.clone(), |u| base.join(u));
                    list.extend(roots.iter().map(|r| base.join(r)));
                }
            }
            list
        } else {
            roots.iter().map(PathBuf::from).collect()
        };
        let root = roots.into_iter().find(|r| r.exists());
        // A portable install keeps everything beside itself.
        let portable = root.as_ref().map(|r| r.join("data")).filter(|d| d.is_dir());
        let (settings, extensions) = match (&portable, &config, &home) {
            (Some(data), _, _) => (
                data.join("user-data").join("User").join("settings.json"),
                data.join("extensions"),
            ),
            (None, Some(config), Some(home)) => (
                config.join(settings_dir).join("User").join("settings.json"),
                home.join(data_dir).join("extensions"),
            ),
            _ => continue,
        };
        out.push(Install {
            name,
            settings,
            extensions,
            builtin: root.as_deref().and_then(builtin_extensions),
        });
    }
    out
}

#[cfg(any(test, feature = "test-support"))]
thread_local! {
    static INSTALLS: std::cell::RefCell<Option<Vec<Install>>> =
        const { std::cell::RefCell::new(None) };
}

/// Run `f` with [`installs`] answering `held` instead of looking at the
/// machine: a test's VS Code, made of fixtures, so it never reads the
/// developer's own.
#[cfg(any(test, feature = "test-support"))]
pub fn with_installs<T>(held: Vec<Install>, f: impl FnOnce() -> T) -> T {
    let before = INSTALLS.with(|i| i.replace(Some(held)));
    let out = f();
    INSTALLS.with(|i| *i.borrow_mut() = before);
    out
}

/// The VS Code whose settings Kete reads: the first of [`installs`] with
/// a `settings.json`. None when there is none.
pub fn install() -> Option<Install> {
    installs().into_iter().find(|i| i.settings.is_file())
}

/// The built-in extensions of the install at `root`: `resources/app/
/// extensions` there, or (the versioned layout updates leave behind)
/// under the newest `<commit>/resources/app`.
pub fn builtin_extensions(root: &Path) -> Option<PathBuf> {
    let flat = [
        root.join("resources").join("app").join("extensions"),
        // macOS and the roots above that are `resources/app` already.
        root.join("extensions"),
    ];
    if let Some(dir) = flat.into_iter().find(|d| d.is_dir()) {
        return Some(dir);
    }
    let newest = std::fs::read_dir(root)
        .ok()?
        .flatten()
        .map(|e| e.path().join("resources").join("app"))
        .filter(|app| app.join("extensions").is_dir())
        .max_by_key(|app| {
            std::fs::metadata(app.join("product.json"))
                .and_then(|m| m.modified())
                .ok()
        })?;
    Some(newest.join("extensions"))
}

/// VS Code's user `settings.json`: [`install`]'s, else where the stable
/// release would keep one.
pub fn settings_path() -> Option<PathBuf> {
    install()
        .or_else(|| installs().into_iter().next())
        .map(|i| i.settings)
}

/// The folders the extensions are in, as [`Install::extension_dirs`]
/// says for [`install`] (or the stable release's places without one).
pub fn extension_dirs() -> Vec<PathBuf> {
    install()
        .or_else(|| installs().into_iter().next())
        .map(|i| i.extension_dirs())
        .unwrap_or_default()
}

/// VS Code's `settings.json`, read; None when it is missing or broken.
pub fn read_settings(install: &Install) -> Option<Value> {
    read_capped(&install.settings)
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

/// A theme some extension contributes.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ThemeFile {
    /// What the theme pickers call it (a `%key%` looked up).
    pub label: String,
    /// What settings may name it by instead of the label.
    pub id: Option<String>,
    /// The theme file.
    #[serde(skip)]
    pub path: PathBuf,
    /// A light theme: `uiTheme` `vs` or `hc-light`.
    pub light: bool,
    /// The extension's display name, for telling two of a name apart.
    pub extension: String,
    /// One of VS Code's own.
    pub builtin: bool,
}

impl ThemeFile {
    /// The name `workbench.colorTheme` stores for it, and the one
    /// `colorCustomizations`' `[...]` scopes match: its id, else its label.
    pub fn settings_id(&self) -> &str {
        self.id.as_deref().unwrap_or(&self.label)
    }
}

/// The extension folders installed in `dir`: the ones its
/// `extensions.json` lists where it has one (VS Code's own record), else
/// every folder there, newest version of each; never one `.obsolete`
/// marks for removal. Sorted by name.
pub fn extension_folders(dir: &Path) -> Vec<PathBuf> {
    let obsolete: Vec<String> = read_capped(&dir.join(".obsolete"))
        .and_then(|v| v.as_object().map(|o| o.keys().cloned().collect()))
        .unwrap_or_default();
    let gone = |p: &Path| {
        p.file_name()
            .and_then(|n| n.to_str())
            .is_some_and(|n| obsolete.iter().any(|o| o == n))
    };
    let listed: Option<Vec<PathBuf>> = read_capped(&dir.join("extensions.json")).and_then(|v| {
        let list = v.as_array()?;
        Some(
            list.iter()
                .filter_map(|e| {
                    let rel = e.get("relativeLocation").and_then(Value::as_str);
                    rel.map(|r| dir.join(r)).or_else(|| {
                        let loc = e.get("location")?;
                        let p = loc
                            .get("fsPath")
                            .or_else(|| loc.get("path"))
                            .and_then(Value::as_str)?;
                        // A URI path: `/c:/Users/...` on Windows.
                        let p = if cfg!(windows) {
                            p.strip_prefix('/').unwrap_or(p)
                        } else {
                            p
                        };
                        Some(PathBuf::from(p))
                    })
                })
                .collect(),
        )
    });
    let mut folders: Vec<PathBuf> = match listed {
        Some(list) => list.into_iter().filter(|p| p.is_dir()).collect(),
        None => {
            let Ok(entries) = std::fs::read_dir(dir) else {
                return Vec::new();
            };
            let mut newest: HashMap<String, (Vec<u64>, PathBuf)> = HashMap::new();
            for path in entries.flatten().map(|e| e.path()).filter(|p| p.is_dir()) {
                let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
                    continue;
                };
                if name.starts_with('.') {
                    continue;
                }
                let (id, version) = split_version(name);
                let keep = newest
                    .get(&id)
                    .is_none_or(|(held, _)| version.as_slice() > held.as_slice());
                if keep {
                    newest.insert(id, (version, path));
                }
            }
            newest.into_values().map(|(_, p)| p).collect()
        }
    };
    folders.retain(|p| !gone(p));
    folders.sort();
    folders
}

/// `publisher.name-1.2.3-win32-x64` as its id and its version's numbers:
/// (`publisher.name`, `[1, 2, 3]`). A name with no version is all id.
fn split_version(folder: &str) -> (String, Vec<u64>) {
    let mut at = None;
    for (i, _) in folder.match_indices('-') {
        if folder[i + 1..].starts_with(|c: char| c.is_ascii_digit()) {
            at = Some(i);
            break;
        }
    }
    let Some(i) = at else {
        return (folder.to_lowercase(), Vec::new());
    };
    let version = folder[i + 1..]
        .split(['.', '-'])
        .map_while(|part| part.parse::<u64>().ok())
        .collect();
    (folder[..i].to_lowercase(), version)
}

/// Every theme the extensions under `dirs` contribute, in the order
/// `dirs` give (the user's before the built-ins), each folder's themes
/// in the order its `package.json` lists them.
pub fn themes_in(dirs: &[PathBuf]) -> Vec<ThemeFile> {
    let mut out = Vec::new();
    for dir in dirs {
        let builtin = is_builtin_dir(dir);
        for ext in extension_folders(dir) {
            out.extend(extension_themes(&ext, builtin));
        }
    }
    out
}

/// Is `dir` the extensions folder under a VS Code install (`resources/
/// app/extensions`), whose extensions are VS Code's own, rather than the
/// user's?
fn is_builtin_dir(dir: &Path) -> bool {
    let parts: Vec<String> = dir
        .components()
        .rev()
        .take(3)
        .map(|c| c.as_os_str().to_string_lossy().to_lowercase())
        .collect();
    parts.len() == 3 && parts[0] == "extensions" && parts[1] == "app" && parts[2] == "resources"
}

/// The themes the extension in `ext` contributes.
fn extension_themes(ext: &Path, builtin: bool) -> Vec<ThemeFile> {
    let Some(pkg) = read_capped(&ext.join("package.json")) else {
        return Vec::new();
    };
    let Some(themes) = pkg.pointer("/contributes/themes").and_then(Value::as_array) else {
        return Vec::new();
    };
    // `%key%` strings are looked up here, once a theme needs it.
    let mut nls: Option<Option<Value>> = None;
    let mut text = |s: &str| -> String {
        let Some(key) = s.strip_prefix('%').and_then(|k| k.strip_suffix('%')) else {
            return s.to_string();
        };
        let table = nls.get_or_insert_with(|| read_capped(&ext.join("package.nls.json")));
        match table.as_ref().and_then(|t| t.get(key)) {
            Some(Value::String(v)) => v.clone(),
            // The `{ "message": ..., "comment": [...] }` spelling.
            Some(v) => v
                .get("message")
                .and_then(Value::as_str)
                .unwrap_or(key)
                .to_string(),
            None => key.to_string(),
        }
    };
    let extension = pkg
        .get("displayName")
        .and_then(Value::as_str)
        .map(&mut text)
        .or_else(|| pkg.get("name").and_then(Value::as_str).map(str::to_string))
        .unwrap_or_default();
    let mut out = Vec::new();
    for t in themes {
        let Some(rel) = t.get("path").and_then(Value::as_str) else {
            continue;
        };
        let id = t.get("id").and_then(Value::as_str).map(str::to_string);
        let label = t
            .get("label")
            .and_then(Value::as_str)
            .map(&mut text)
            .or_else(|| id.clone());
        let Some(label) = label.filter(|l| !l.trim().is_empty()) else {
            continue;
        };
        let ui = t
            .get("uiTheme")
            .and_then(Value::as_str)
            .unwrap_or("vs-dark");
        out.push(ThemeFile {
            label,
            id,
            path: ext.join(rel),
            light: matches!(ui, "vs" | "hc-light"),
            extension: extension.clone(),
            builtin,
        });
    }
    out
}

/// Find the theme settings call `name` among the extensions under `dirs`,
/// case-insensitively: by the name VS Code stores for it (its id, else
/// its label) first, then by its label, the user's extensions before the
/// built-ins. `Default Dark Modern` also finds the `Dark Modern` newer
/// releases ship.
pub fn find_theme(name: &str, dirs: &[PathBuf]) -> Option<ThemeFile> {
    let all = themes_in(dirs);
    let pick = |want: &str| {
        let want = want.trim().to_lowercase();
        all.iter()
            .find(|t| t.settings_id().to_lowercase() == want)
            .or_else(|| all.iter().find(|t| t.label.to_lowercase() == want))
            .cloned()
    };
    pick(name).or_else(|| {
        let bare = name.trim().strip_prefix("Default ")?;
        pick(bare)
    })
}

/// A theme file's `colors` and `tokenColors`, with every file it
/// `include`s read first and overridden by what includes it.
#[derive(Debug, Default, Clone)]
pub struct ThemeData {
    pub colors: HashMap<String, String>,
    /// `(scope, foreground)` in file order, a later entry overriding.
    pub tokens: Vec<(String, String)>,
    /// What the theme's own `type` says (`light`, `dark`, `hc`…): the
    /// lightness of a file picked by hand, which no `uiTheme` vouches for.
    pub kind: Option<String>,
}

impl ThemeData {
    /// The colour the theme gives `key`, alpha and all.
    pub fn color(&self, key: &str) -> Option<Rgba> {
        Rgba::parse(self.colors.get(key)?)
    }

    /// Is it a light theme, by its `type`; None where it has none.
    pub fn kind_light(&self) -> Option<bool> {
        let kind = self.kind.as_deref()?.to_ascii_lowercase();
        Some(kind == "light" || kind == "hc-light" || kind == "vs")
    }

    /// The foreground the theme gives the first of `scopes` it styles, as
    /// VS Code picks it: the most specific entry that covers the scope —
    /// the scope itself, else the longest one it starts with (`keyword`
    /// covers `keyword.control`) — the later entry winning a tie. As
    /// written (alpha and all); [`ThemeData::token`] parses it.
    pub fn token_str(&self, scopes: &[&str]) -> Option<&str> {
        for want in scopes {
            let covers = |s: &str| s == *want || want.starts_with(&format!("{s}."));
            let hit = self
                .tokens
                .iter()
                .enumerate()
                .filter(|(_, (s, fg))| covers(s) && Rgba::parse(fg).is_some())
                .max_by_key(|(i, (s, _))| (s.len(), *i));
            if let Some((_, (_, fg))) = hit {
                return Some(fg);
            }
        }
        None
    }

    /// [`ThemeData::token_str`], parsed.
    pub fn token(&self, scopes: &[&str]) -> Option<Rgba> {
        self.token_str(scopes).and_then(Rgba::parse)
    }
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
    let json = read_capped(path)?;
    if let Some(include) = json.get("include").and_then(Value::as_str) {
        let parent = path.parent().unwrap_or(Path::new("."));
        // A missing base still leaves this file's own colors worth having.
        let _ = load_into(&parent.join(include), data, depth + 1);
    }
    if let Some(kind) = json.get("type").and_then(Value::as_str) {
        data.kind = Some(kind.to_string());
    }
    if let Some(colors) = json.get("colors").and_then(Value::as_object) {
        for (k, v) in colors {
            if let Some(v) = v.as_str() {
                data.colors.insert(k.clone(), v.to_string());
            }
        }
    }
    // A `.tmTheme` path instead of a list is TextMate's plist: not read.
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

/// A JSONC file, unless it is bigger than [`MAX_FILE_BYTES`].
fn read_capped(path: &Path) -> Option<Value> {
    let size = std::fs::metadata(path).ok()?.len();
    if size > MAX_FILE_BYTES {
        return None;
    }
    crate::jsonc::read(path)
}

/// Lay `workbench.colorCustomizations` from `settings` over `data`, as
/// VS Code does for the theme whose settings id or label is `names`: the
/// unscoped keys first, then every `[Theme Name]` block that names it
/// (several `[A][B]` in one key, `*` matching any run of characters).
pub fn customize(data: &mut ThemeData, settings: &Value, names: &[&str]) {
    let Some(custom) = settings
        .get("workbench.colorCustomizations")
        .and_then(Value::as_object)
    else {
        return;
    };
    let mut scoped = Vec::new();
    for (key, value) in custom {
        if key.starts_with('[') {
            scoped.push((key, value));
        } else if let Some(v) = value.as_str() {
            data.colors.insert(key.clone(), v.to_string());
        }
    }
    for (key, value) in scoped {
        let scopes = key
            .split(']')
            .filter_map(|part| part.trim().strip_prefix('['));
        let hit = scopes
            .into_iter()
            .any(|scope| names.iter().any(|name| glob(scope, name)));
        let Some(colors) = value.as_object().filter(|_| hit) else {
            continue;
        };
        for (k, v) in colors {
            if let Some(v) = v.as_str() {
                data.colors.insert(k.clone(), v.to_string());
            }
        }
    }
}

/// `pattern` (with `*` for any run of characters) matches all of `text`,
/// ignoring case.
fn glob(pattern: &str, text: &str) -> bool {
    let (p, t) = (pattern.to_lowercase(), text.to_lowercase());
    let parts: Vec<&str> = p.split('*').collect();
    if parts.len() == 1 {
        return p == t;
    }
    let mut rest = t.as_str();
    for (i, part) in parts.iter().enumerate() {
        if i == 0 {
            let Some(after) = rest.strip_prefix(part) else {
                return false;
            };
            rest = after;
        } else if i == parts.len() - 1 {
            return rest.ends_with(part);
        } else if let Some(at) = rest.find(part) {
            rest = &rest[at + part.len()..];
        } else {
            return false;
        }
    }
    true
}

/// A theme as VS Code shows it: the file's colours with the user's
/// customizations laid over.
#[derive(Debug, Clone)]
pub struct Loaded {
    pub file: ThemeFile,
    pub data: ThemeData,
}

/// The theme VS Code (at `install`, with its `settings`) shows now.
pub fn current_in(install: &Install, settings: &Value, os_light: Option<bool>) -> Option<Loaded> {
    named_in(install, settings, &chosen_theme(settings, os_light))
}

/// The installed theme `name` (an id or a label), with VS Code's
/// customizations for it.
pub fn named_in(install: &Install, settings: &Value, name: &str) -> Option<Loaded> {
    let file = find_theme(name, &install.extension_dirs())?;
    let mut data = load_theme(&file.path)?;
    customize(&mut data, settings, &[file.settings_id(), &file.label]);
    Some(Loaded { file, data })
}

/// What the `vscode` source shows on this machine right now; None when
/// VS Code, its settings or the theme can't be found.
pub fn current(os_light: Option<bool>) -> Option<Loaded> {
    let install = install()?;
    let settings = read_settings(&install)?;
    current_in(&install, &settings, os_light)
}

/// The installed theme `name` on this machine, as VS Code would show it.
pub fn named(name: &str) -> Option<Loaded> {
    let install = install()?;
    let settings = read_settings(&install).unwrap_or(Value::Null);
    named_in(&install, &settings, name)
}

/// Every theme installed for the VS Code Kete reads, one per label (the
/// user's extension over a built-in of the same name), built-ins last.
pub fn installed_themes() -> Vec<ThemeFile> {
    let Some(install) = install() else {
        return Vec::new();
    };
    dedupe(themes_in(&install.extension_dirs()))
}

fn dedupe(all: Vec<ThemeFile>) -> Vec<ThemeFile> {
    let mut seen = std::collections::HashSet::new();
    let mut out: Vec<ThemeFile> = all
        .into_iter()
        .filter(|t| seen.insert(t.label.to_lowercase()))
        .collect();
    out.sort_by(|a, b| {
        (a.builtin, a.label.to_lowercase()).cmp(&(b.builtin, b.label.to_lowercase()))
    });
    out
}

/// When VS Code's settings last changed, so a beat can tell a theme
/// switched in the editor from one that has not, by a `stat`.
pub fn settings_stamp() -> Option<std::time::SystemTime> {
    std::fs::metadata(settings_path()?).ok()?.modified().ok()
}

/// VS Code's fonts, from its settings: the editor's family and size, and
/// the terminal's (which follow the editor's where unset).
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct Fonts {
    /// The first family of `editor.fontFamily` that is a real one.
    pub editor_family: Option<String>,
    pub editor_size: Option<f64>,
    /// `terminal.integrated.fontFamily`'s first, else the editor's.
    pub terminal_family: Option<String>,
    /// `terminal.integrated.fontSize`, else the editor's.
    pub terminal_size: Option<f64>,
}

/// What `settings` says about the fonts.
pub fn fonts(settings: &Value) -> Fonts {
    let family = |key: &str| {
        settings
            .get(key)
            .and_then(Value::as_str)
            .and_then(first_family)
    };
    let size = |key: &str| {
        settings
            .get(key)
            .and_then(Value::as_f64)
            .filter(|s| s.is_finite() && *s > 0.0)
    };
    let editor_family = family("editor.fontFamily");
    let editor_size = size("editor.fontSize");
    Fonts {
        terminal_family: family("terminal.integrated.fontFamily").or(editor_family.clone()),
        terminal_size: size("terminal.integrated.fontSize").or(editor_size),
        editor_family,
        editor_size,
    }
}

/// The first family a CSS-style font list names, quotes off, skipping the
/// generic ones (`monospace`) that name no font.
pub fn first_family(stack: &str) -> Option<String> {
    const GENERIC: &[&str] = &[
        "monospace",
        "ui-monospace",
        "sans-serif",
        "serif",
        "system-ui",
        "cursive",
        "fantasy",
    ];
    stack
        .split(',')
        .map(|f| f.trim().trim_matches(['\'', '"']).trim())
        .find(|f| !f.is_empty() && !GENERIC.contains(&f.to_ascii_lowercase().as_str()))
        .map(str::to_string)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

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

    #[test]
    fn a_folder_name_splits_into_its_id_and_version() {
        assert_eq!(
            split_version("akamud.vscode-theme-onedark-2.3.0"),
            ("akamud.vscode-theme-onedark".into(), vec![2, 3, 0])
        );
        assert_eq!(
            split_version("ms-azuretools.vscode-azure-github-copilot-1.0.241-win32-x64"),
            (
                "ms-azuretools.vscode-azure-github-copilot".into(),
                vec![1, 0, 241]
            )
        );
        assert_eq!(
            split_version("theme-defaults"),
            ("theme-defaults".into(), vec![])
        );
        assert!(
            split_version("a.b-1.10.0").1 > split_version("a.b-1.9.0").1,
            "by number"
        );
    }

    #[test]
    fn the_first_real_family_of_a_stack_is_taken() {
        assert_eq!(
            first_family("'JetBrains Mono', 'monospace', monospace").as_deref(),
            Some("JetBrains Mono")
        );
        assert_eq!(
            first_family(r#""Cascadia Code", Consolas"#).as_deref(),
            Some("Cascadia Code")
        );
        assert_eq!(first_family("monospace"), None);
        assert_eq!(first_family("  "), None);
    }

    #[test]
    fn fonts_follow_the_editor_where_the_terminal_is_unset() {
        let s = json!({
            "editor.fontFamily": "'JetBrains Mono', 'monospace', monospace",
            "editor.fontSize": 15,
        });
        assert_eq!(
            fonts(&s),
            Fonts {
                editor_family: Some("JetBrains Mono".into()),
                editor_size: Some(15.0),
                terminal_family: Some("JetBrains Mono".into()),
                terminal_size: Some(15.0),
            }
        );
        let s = json!({
            "editor.fontFamily": "Fira Code",
            "terminal.integrated.fontFamily": "MesloLGS NF",
            "terminal.integrated.fontSize": 13,
        });
        let f = fonts(&s);
        assert_eq!(f.terminal_family.as_deref(), Some("MesloLGS NF"));
        assert_eq!((f.editor_size, f.terminal_size), (None, Some(13.0)));
        assert_eq!(fonts(&json!({})), Fonts::default());
    }

    #[test]
    fn a_glob_matches_whole_names_ignoring_case() {
        assert!(glob("Atom One Dark", "atom one dark"));
        assert!(glob("*Dark*", "Atom One Dark"));
        assert!(glob("Atom*", "Atom One Light"));
        assert!(!glob("Atom", "Atom One Dark"));
        assert!(!glob("*Light", "Atom One Dark"));
    }
}
