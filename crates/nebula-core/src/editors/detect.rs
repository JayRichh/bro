//! Which editors this machine has. Everything the scan reads goes through a
//! [`Probe`] (the registry, files, folders, the environment, vswhere's
//! output), so the tests run the same scan over fixtures, on any OS.
//!
//! A VS Code-family install is whatever folder holds a `product.json` with
//! an `applicationName`: VS Code, Insiders, VSCodium, Cursor, Windsurf,
//! Kiro, Trae and any other Code-OSS fork, with no list of names to keep
//! up. The folder comes from, in order: the `HKCU`/`HKLM` uninstall keys
//! (`InstallLocation`, `DisplayIcon`), `App Paths`, the folders the
//! installers use, and `PATH`. The `product.json` sits either at
//! `<install>\resources\app\` (forks, older builds) or, since VS Code 1.10x,
//! under a folder named for the build's commit
//! (`<install>\2a59476c9b\resources\app\`). Display names are never read:
//! "Zed Attack Proxy" is not an editor.
//!
//! Visual Studio comes from vswhere, every product and prerelease. Only an
//! instance whose `productPath` is `devenv.exe` is an IDE: Build Tools'
//! is `LaunchDevCmd.bat`.
//!
//! Off Windows the scan is PATH alone (`code`, `code-insiders`, `codium`,
//! `cursor`, `windsurf`), and there is no Visual Studio.

use super::{Editor, Family, Source};
use serde::Deserialize;
use std::collections::HashSet;
use std::path::{Path, PathBuf};

/// One uninstall key's values, as the registry has them.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct UninstallEntry {
    /// The key's name (`{771FD6B0-…}_is1`).
    pub key: String,
    /// Under `HKLM` (a system install) rather than `HKCU`.
    pub machine: bool,
    pub display_version: String,
    pub install_location: String,
    pub display_icon: String,
}

/// What the scan reads. [`RealProbe`] reads this machine; the tests build
/// fixtures.
pub trait Probe {
    /// Lay out installs as on Windows (registry, `.exe`, `product.json`)
    /// rather than as PATH commands.
    fn windows(&self) -> bool;
    /// Every uninstall key, user and machine (both registry views).
    fn uninstall_entries(&self) -> Vec<UninstallEntry>;
    /// `App Paths\<exe>`'s default value, user then machine.
    fn app_path(&self, exe: &str) -> Option<String>;
    fn env(&self, var: &str) -> Option<String>;
    fn is_file(&self, path: &Path) -> bool;
    fn read(&self, path: &Path) -> Option<String>;
    /// The names of the folders in `path`.
    fn subdirs(&self, path: &Path) -> Vec<String>;
    /// vswhere's JSON for every instance, or None without vswhere.
    fn vswhere(&self) -> Option<String>;
}

/// The command names a Code-family install puts on PATH, in preference
/// order (on Windows each is a `bin\<name>.cmd`).
pub const PATH_COMMANDS: &[&str] = &["code", "code-insiders", "codium", "cursor", "windsurf"];

/// `App Paths` entries the Code-family installers register.
const APP_PATH_EXES: &[&str] = &[
    "code.exe",
    "code - insiders.exe",
    "vscodium.exe",
    "cursor.exe",
    "windsurf.exe",
];

/// The folders the installers use, under an environment variable.
const KNOWN_FOLDERS: &[(&str, &str)] = &[
    ("LOCALAPPDATA", "Programs/Microsoft VS Code"),
    ("LOCALAPPDATA", "Programs/Microsoft VS Code Insiders"),
    ("LOCALAPPDATA", "Programs/VSCodium"),
    ("LOCALAPPDATA", "Programs/cursor"),
    ("LOCALAPPDATA", "Programs/Windsurf"),
    ("LOCALAPPDATA", "Programs/Kiro"),
    ("LOCALAPPDATA", "Programs/Trae"),
    ("LOCALAPPDATA", "Programs/Antigravity"),
    ("ProgramFiles", "Microsoft VS Code"),
    ("ProgramFiles", "Microsoft VS Code Insiders"),
    ("ProgramFiles", "VSCodium"),
];

/// Every editor `probe` finds: the Code family first, VS Code before its
/// forks, then Visual Studio, the newest release before older ones and
/// previews. Each family's first is its default.
pub fn detect(probe: &dyn Probe) -> Vec<Editor> {
    let mut out = if probe.windows() {
        code_installs(probe)
    } else {
        code_on_path(probe)
    };
    // By app, then a user install before a system one of the same app,
    // then the newest.
    out.sort_by(|a, b| {
        rank(a.application_name.as_deref().unwrap_or(""))
            .cmp(&rank(b.application_name.as_deref().unwrap_or("")))
            .then_with(|| a.source.cmp(&b.source))
            .then_with(|| version_cmp(&b.version, &a.version))
    });
    assign_ids(&mut out);
    if probe.windows() {
        out.extend(visual_studio(probe.vswhere().as_deref().unwrap_or("")));
    }
    out
}

// ---- the Code family ----

/// The fields of a `product.json` the scan keeps.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Product {
    #[serde(default)]
    pub application_name: String,
    #[serde(default)]
    pub name_short: String,
    #[serde(default)]
    pub name_long: String,
    #[serde(default)]
    pub data_folder_name: String,
    #[serde(default)]
    pub version: String,
    #[serde(default)]
    pub quality: String,
}

/// A `product.json`'s text, read; None when it isn't a Code-family
/// product (no `applicationName` or `nameShort`).
pub fn parse_product(text: &str) -> Option<Product> {
    let p: Product = serde_json::from_str(text.trim_start_matches('\u{feff}')).ok()?;
    (!p.application_name.trim().is_empty() && !p.name_short.trim().is_empty()).then_some(p)
}

/// A folder that may hold an install, and how it was found.
struct Candidate {
    dir: PathBuf,
    /// The exe the source names (an uninstall key's `DisplayIcon`, App
    /// Paths), when it does.
    exe: Option<PathBuf>,
    source: Source,
    /// The version the uninstall key states: which commit folder is
    /// current while an update leaves two.
    version: Option<String>,
}

/// A registry path value without its quotes and `,<icon index>`; None
/// unless it names an `.exe`.
pub fn exe_of(value: &str) -> Option<PathBuf> {
    let v = value.trim();
    let v = match v.strip_prefix('"') {
        Some(rest) => rest.split('"').next().unwrap_or(""),
        None => match v.rfind(',') {
            Some(i) if v[i + 1..].trim().parse::<i32>().is_ok() => &v[..i],
            _ => v,
        },
    };
    let v = v.trim();
    v.to_ascii_lowercase()
        .ends_with(".exe")
        .then(|| PathBuf::from(v))
}

fn non_empty(s: &str) -> Option<&str> {
    let s = s.trim();
    (!s.is_empty()).then_some(s)
}

fn code_installs(probe: &dyn Probe) -> Vec<Editor> {
    let mut cands = Vec::new();
    for e in probe.uninstall_entries() {
        let exe = exe_of(&e.display_icon);
        let dir = non_empty(&e.install_location)
            .map(PathBuf::from)
            .or_else(|| exe.as_deref().and_then(Path::parent).map(Path::to_path_buf));
        if let Some(dir) = dir {
            cands.push(Candidate {
                dir,
                exe,
                source: if e.machine {
                    Source::SystemInstall
                } else {
                    Source::UserInstall
                },
                version: non_empty(&e.display_version).map(String::from),
            });
        }
    }
    for name in APP_PATH_EXES {
        if let Some(exe) = probe.app_path(name).as_deref().and_then(exe_of) {
            if let Some(dir) = exe.parent().map(Path::to_path_buf) {
                cands.push(Candidate {
                    dir,
                    exe: Some(exe),
                    source: Source::AppPaths,
                    version: None,
                });
            }
        }
    }
    for (var, sub) in KNOWN_FOLDERS {
        if let Some(base) = probe.env(var) {
            cands.push(Candidate {
                dir: Path::new(&base).join(sub),
                exe: None,
                source: Source::KnownFolder,
                version: None,
            });
        }
    }
    // PATH last: `<install>\bin\code.cmd`, or a fork's deeper
    // `<install>\resources\app\bin\cursor.cmd`: the install is the
    // nearest folder above it that resolves.
    for dir in path_dirs(probe) {
        for name in PATH_COMMANDS {
            if !probe.is_file(&dir.join(format!("{name}.cmd"))) {
                continue;
            }
            for up in dir.ancestors().skip(1).take(3) {
                cands.push(Candidate {
                    dir: up.to_path_buf(),
                    exe: None,
                    source: Source::Path,
                    version: None,
                });
            }
        }
    }
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for c in cands {
        let Some(editor) = resolve(probe, &c) else {
            continue;
        };
        if seen.insert(path_key(&editor.exe)) {
            out.push(editor);
        }
    }
    out
}

/// A path as a dedupe key: Windows paths are case-blind, either slash.
fn path_key(p: &Path) -> String {
    p.to_string_lossy()
        .replace('\\', "/")
        .trim_end_matches('/')
        .to_ascii_lowercase()
}

/// The `product.json`s under install folder `dir`: at
/// `resources\app\`, and under each build-commit folder.
fn products(probe: &dyn Probe, dir: &Path) -> Vec<Product> {
    let mut out = Vec::new();
    let flat = dir.join("resources").join("app").join("product.json");
    if let Some(p) = probe.read(&flat).as_deref().and_then(parse_product) {
        out.push(p);
    }
    for sub in probe.subdirs(dir) {
        if !is_commit_folder(&sub) {
            continue;
        }
        let file = dir
            .join(&sub)
            .join("resources")
            .join("app")
            .join("product.json");
        if let Some(p) = probe.read(&file).as_deref().and_then(parse_product) {
            out.push(p);
        }
    }
    out
}

/// A build-commit folder's name: ten hex digits (`2a59476c9b`).
pub fn is_commit_folder(name: &str) -> bool {
    name.len() == 10 && name.bytes().all(|b| b.is_ascii_hexdigit())
}

fn resolve(probe: &dyn Probe, c: &Candidate) -> Option<Editor> {
    let all = products(probe, &c.dir);
    // Mid-update two commit folders can sit side by side: the one the
    // uninstall key names, else the newest.
    let product = c
        .version
        .as_deref()
        .and_then(|v| all.iter().find(|p| p.version == v))
        .or_else(|| {
            all.iter()
                .max_by(|a, b| version_cmp(&a.version, &b.version))
        })?
        .clone();
    let named = c.exe.clone().filter(|exe| {
        probe.is_file(exe)
            && !exe.file_name().is_some_and(|f| {
                f.to_string_lossy()
                    .to_ascii_lowercase()
                    .starts_with("unins")
            })
    });
    let exe = named.or_else(|| {
        let guess = c.dir.join(format!("{}.exe", product.name_short.trim()));
        probe.is_file(&guess).then_some(guess)
    })?;
    let app = product.application_name.trim().to_string();
    let quality = non_empty(&product.quality).map(String::from);
    Some(Editor {
        id: app.clone(),
        family: Family::Code,
        short: code_short(&app, &product),
        name: non_empty(&product.name_long)
            .unwrap_or(product.name_short.trim())
            .to_string(),
        version: product.version.trim().to_string(),
        prerelease: quality.as_deref().is_some_and(|q| q != "stable"),
        edition: quality,
        year: None,
        exe,
        dir: c.dir.clone(),
        source: c.source,
        instance_id: None,
        application_name: Some(app),
        name_short: Some(product.name_short.trim().to_string()),
        data_folder_name: non_empty(&product.data_folder_name).map(String::from),
    })
}

/// What a menu calls a Code-family app, by its `applicationName`; its own
/// `nameShort` for one Kete has no words for.
fn code_short(app: &str, product: &Product) -> String {
    let known = match app {
        "code" => "VS Code",
        "code-insiders" => "VS Code Insiders",
        "code-exploration" => "VS Code Exploration",
        "codium" => "VSCodium",
        "codium-insiders" => "VSCodium Insiders",
        "cursor" => "Cursor",
        "windsurf" => "Windsurf",
        "windsurf-next" => "Windsurf Next",
        "kiro" => "Kiro",
        "trae" => "Trae",
        "antigravity" => "Antigravity",
        _ => "",
    };
    if known.is_empty() {
        non_empty(&product.name_short).unwrap_or(app).to_string()
    } else {
        known.to_string()
    }
}

/// Which Code-family app is the default when several are installed.
fn rank(app: &str) -> usize {
    const ORDER: &[&str] = &[
        "code",
        "code-insiders",
        "code-exploration",
        "codium",
        "codium-insiders",
        "cursor",
        "windsurf",
        "windsurf-next",
    ];
    ORDER.iter().position(|a| *a == app).unwrap_or(ORDER.len())
}

/// Ids unique across the list: the app's name, `-system` for a system
/// install of an app already listed, `-2`, `-3` past that.
fn assign_ids(editors: &mut [Editor]) {
    let mut used = HashSet::new();
    for e in editors.iter_mut() {
        let base = e.id.clone();
        let mut id = base.clone();
        if used.contains(&id) && e.source == Source::SystemInstall {
            id = format!("{base}-system");
        }
        let mut n = 2;
        while used.contains(&id) {
            id = format!("{base}-{n}");
            n += 1;
        }
        used.insert(id.clone());
        e.id = id;
    }
}

/// PATH's folders, split the way the layout's OS splits them (`;` on
/// Windows, whatever OS the scan runs on, so a fixture reads the same
/// everywhere).
fn path_dirs(probe: &dyn Probe) -> Vec<PathBuf> {
    let Some(path) = probe.env("PATH") else {
        return Vec::new();
    };
    if probe.windows() {
        path.split(';')
            .map(|d| d.trim().trim_matches('"'))
            .filter(|d| !d.is_empty())
            .map(PathBuf::from)
            .collect()
    } else {
        std::env::split_paths(&path).collect()
    }
}

/// Off Windows: each of [`PATH_COMMANDS`] found on PATH, as itself.
fn code_on_path(probe: &dyn Probe) -> Vec<Editor> {
    let dirs = path_dirs(probe);
    PATH_COMMANDS
        .iter()
        .filter_map(|name| {
            let exe = dirs
                .iter()
                .map(|d| d.join(name))
                .find(|p| probe.is_file(p))?;
            let short = code_short(name, &Product::default());
            Some(Editor {
                id: (*name).to_string(),
                family: Family::Code,
                name: short.clone(),
                short,
                version: String::new(),
                edition: None,
                year: None,
                prerelease: false,
                dir: exe.parent().map(Path::to_path_buf).unwrap_or_default(),
                exe,
                source: Source::Path,
                instance_id: None,
                application_name: Some((*name).to_string()),
                name_short: None,
                data_folder_name: None,
            })
        })
        .collect()
}

// ---- Visual Studio ----

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct VsInstance {
    #[serde(default)]
    instance_id: String,
    #[serde(default)]
    installation_path: String,
    #[serde(default)]
    installation_version: String,
    #[serde(default)]
    product_id: String,
    #[serde(default)]
    product_path: String,
    #[serde(default)]
    display_name: String,
    #[serde(default)]
    is_prerelease: bool,
    #[serde(default = "yes")]
    is_launchable: bool,
    #[serde(default = "yes")]
    is_complete: bool,
    #[serde(default)]
    catalog: VsCatalog,
}

fn yes() -> bool {
    true
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct VsCatalog {
    #[serde(default)]
    product_line_version: String,
    #[serde(default)]
    product_display_version: String,
}

/// The product line year of a Visual Studio major version.
fn year_of(major: &str) -> Option<&'static str> {
    match major {
        "15" => Some("2017"),
        "16" => Some("2019"),
        "17" => Some("2022"),
        "18" => Some("2026"),
        _ => None,
    }
}

/// The IDE instances in vswhere's JSON (`-all -prerelease -products *
/// -format json`): each one whose product is `devenv.exe`, launchable and
/// complete; the newest release first, previews after.
pub fn visual_studio(json: &str) -> Vec<Editor> {
    let all: Vec<VsInstance> =
        serde_json::from_str(json.trim_start_matches('\u{feff}')).unwrap_or_default();
    let mut out: Vec<Editor> = all
        .into_iter()
        .filter(|i| {
            let path = i.product_path.replace('/', "\\").to_ascii_lowercase();
            (path.ends_with("\\devenv.exe") || path == "devenv.exe")
                && i.is_launchable
                && i.is_complete
        })
        .map(|i| {
            let major = i.installation_version.split('.').next().unwrap_or("");
            let year = non_empty(&i.catalog.product_line_version)
                .map(String::from)
                .or_else(|| year_of(major).map(String::from));
            let edition = i
                .product_id
                .rsplit('.')
                .next()
                .and_then(non_empty)
                .map(String::from);
            let version = i
                .catalog
                .product_display_version
                .split_whitespace()
                .next()
                .and_then(non_empty)
                .unwrap_or(&i.installation_version)
                .to_string();
            // A preview says which kind in its display name
            // ("… 2022 Preview", "… 2026 Insiders").
            let channel = i.is_prerelease.then(|| {
                let last = i.display_name.split_whitespace().last().unwrap_or("");
                if last.chars().all(|c| c.is_ascii_alphabetic()) && !last.is_empty() {
                    last.to_string()
                } else {
                    "Preview".to_string()
                }
            });
            let short = match (&year, &channel) {
                (Some(y), Some(c)) => format!("Visual Studio {y} {c}"),
                (Some(y), None) => format!("Visual Studio {y}"),
                (None, Some(c)) => format!("Visual Studio {c}"),
                (None, None) => "Visual Studio".to_string(),
            };
            Editor {
                id: format!("vs-{}", i.instance_id),
                family: Family::VisualStudio,
                short,
                name: non_empty(&i.display_name)
                    .unwrap_or("Visual Studio")
                    .to_string(),
                version,
                edition,
                year,
                prerelease: i.is_prerelease,
                exe: PathBuf::from(&i.product_path),
                dir: PathBuf::from(&i.installation_path),
                source: Source::Vswhere,
                instance_id: Some(i.instance_id),
                application_name: None,
                name_short: None,
                data_folder_name: None,
            }
        })
        .collect();
    out.sort_by(|a, b| {
        a.prerelease
            .cmp(&b.prerelease)
            .then_with(|| version_cmp(&b.version, &a.version))
    });
    // Two of one year and channel (Community and Enterprise 2022) say
    // their edition.
    let shorts: Vec<String> = out.iter().map(|e| e.short.clone()).collect();
    for e in out.iter_mut() {
        if shorts.iter().filter(|s| **s == e.short).count() > 1 {
            if let Some(ed) = &e.edition {
                e.short = format!("{} {ed}", e.short);
            }
        }
    }
    out
}

/// Compare dotted versions number by number ("1.141.0" > "1.99.3").
pub fn version_cmp(a: &str, b: &str) -> std::cmp::Ordering {
    let parts = |v: &str| -> Vec<u64> {
        v.split(|c: char| !c.is_ascii_digit())
            .filter(|p| !p.is_empty())
            .map(|p| p.parse().unwrap_or(0))
            .collect()
    };
    parts(a).cmp(&parts(b))
}

// ---- this machine ----

/// The scan's reads on this machine.
pub struct RealProbe;

impl Probe for RealProbe {
    fn windows(&self) -> bool {
        cfg!(windows)
    }

    fn uninstall_entries(&self) -> Vec<UninstallEntry> {
        #[cfg(windows)]
        {
            super::win::uninstall_entries()
        }
        #[cfg(not(windows))]
        {
            Vec::new()
        }
    }

    fn app_path(&self, exe: &str) -> Option<String> {
        #[cfg(windows)]
        {
            super::win::app_path(exe)
        }
        #[cfg(not(windows))]
        {
            let _ = exe;
            None
        }
    }

    fn env(&self, var: &str) -> Option<String> {
        std::env::var(var).ok().filter(|v| !v.is_empty())
    }

    fn is_file(&self, path: &Path) -> bool {
        path.is_file()
    }

    fn read(&self, path: &Path) -> Option<String> {
        std::fs::read_to_string(path).ok()
    }

    fn subdirs(&self, path: &Path) -> Vec<String> {
        std::fs::read_dir(path)
            .map(|rd| {
                rd.flatten()
                    .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
                    .map(|e| e.file_name().to_string_lossy().into_owned())
                    .collect()
            })
            .unwrap_or_default()
    }

    fn vswhere(&self) -> Option<String> {
        #[cfg(windows)]
        {
            super::win::vswhere()
        }
        #[cfg(not(windows))]
        {
            None
        }
    }
}
