//! The user's own editors: which VS Code-family installs and which Visual
//! Studio instances this machine has, how each opens a checkout, a folder,
//! a file or a solution, which windows they have open, and docking one
//! beside Kete. No UI: the desktop window and the TUI both call this, so a
//! verb opens the same editor the same way in either app.
//!
//! Kete never embeds an editor (see the 0.54 spec's "Embedding"). It
//! starts the editor's own exe with an argv, detached and with Kete's own
//! environment stripped, and lets the editor decide which window takes it:
//! VS Code focuses the window that already has the folder, `devenv /Edit`
//! the most recent Visual Studio. Window titles are read only for the
//! "a window is open" dot and for picking which window Dock moves, never
//! to decide focus.
//!
//! - Detection ([`detect`]): VS Code and its forks from the Inno
//!   uninstall keys, App Paths, the known install folders and PATH, each
//!   one confirmed by its own `product.json`; Visual Studio from vswhere,
//!   IDE instances only (Build Tools has no devenv). Cached, with a
//!   re-scan ([`detected`], [`rescan`]).
//! - Solutions ([`solutions`]): the `.slnx`/`.sln` (else `.csproj` /
//!   `.vcxproj`) a checkout has at its root or one level down: Visual
//!   Studio is offered only where one exists.
//! - Launch ([`launch`]): the argv per editor and what each opens, the
//!   environment a launched editor must not inherit.
//! - Windows ([`dock`], and on Windows the Win32 calls): the open-window
//!   check, Dock beside Kete, the exe's own icon as a PNG.

pub mod detect;
pub mod dock;
pub mod launch;
pub mod solutions;
pub mod titles;

#[cfg(windows)]
mod win;

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

pub use detect::{detect, Probe};
pub use launch::{argv, launch, launch_env_removals, Open, Options};
pub use solutions::{solutions, Solution, SolutionKind};

/// Which kind of editor an install is: the two take different argv and
/// open different things (VS Code a folder, Visual Studio a solution).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Family {
    /// VS Code and the Code-OSS forks: Insiders, VSCodium, Cursor,
    /// Windsurf, Kiro, Trae and the rest.
    Code,
    VisualStudio,
}

impl Family {
    /// The word the desktop's `editor_folder` / `editor_file` choices use
    /// for the family.
    pub fn word(self) -> &'static str {
        match self {
            Family::Code => "vscode",
            Family::VisualStudio => "visual-studio",
        }
    }

    /// The family a choice word names (`vscode`, `visual-studio`); None
    /// for `auto`, `command`, `kete` and anything else.
    pub fn from_word(word: &str) -> Option<Self> {
        match word.trim() {
            "vscode" | "code" => Some(Family::Code),
            "visual-studio" | "vs" => Some(Family::VisualStudio),
            _ => None,
        }
    }
}

/// Where an install was found, most authoritative first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Source {
    /// An `HKCU` uninstall key: a user install.
    UserInstall,
    /// An `HKLM` uninstall key: a system install.
    SystemInstall,
    /// `App Paths`, without an uninstall key (a portable or zip install
    /// the user registered).
    AppPaths,
    /// One of the folders the installers use, without any key.
    KnownFolder,
    /// The `PATH`: the last resort, and the only source off Windows.
    Path,
    /// vswhere, for Visual Studio.
    Vswhere,
}

/// One detected editor install.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Editor {
    /// Stable for the install: a Code-family install's `applicationName`
    /// (`code`, `code-insiders`, `codium`, `cursor`), `-system` added for
    /// a system install of an app also installed for the user; Visual
    /// Studio's `vs-<instanceId>`.
    pub id: String,
    pub family: Family,
    /// What a menu row calls it: "VS Code", "VS Code Insiders", "Cursor",
    /// "Visual Studio 2022", "Visual Studio 2026 Preview".
    pub short: String,
    /// The product's own full name: "Visual Studio Code", "Visual Studio
    /// Community 2022".
    pub name: String,
    /// "1.141.0", "17.14.16". Empty when the install doesn't say.
    pub version: String,
    /// Visual Studio's edition (Community, Professional, Enterprise);
    /// a Code-family install's quality (stable, insider).
    pub edition: Option<String>,
    /// Visual Studio's product line year ("2022").
    pub year: Option<String>,
    pub prerelease: bool,
    /// The exe Kete starts: `Code.exe`, `Cursor.exe`, `devenv.exe`; off
    /// Windows, the command found on PATH.
    pub exe: PathBuf,
    /// Where it is installed.
    pub dir: PathBuf,
    pub source: Source,
    /// Visual Studio's vswhere `instanceId`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub instance_id: Option<String>,
    /// A Code-family install's `product.json` `applicationName`: its CLI
    /// name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub application_name: Option<String>,
    /// `product.json` `nameShort`: its settings live in
    /// `%APPDATA%\<nameShort>\User`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name_short: Option<String>,
    /// `product.json` `dataFolderName`: its extensions live in
    /// `~\<dataFolderName>\extensions`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data_folder_name: Option<String>,
}

impl Editor {
    /// Whether this Visual Studio opens `.slnx` solutions (17.14 on).
    pub fn opens_slnx(&self) -> bool {
        if self.family != Family::VisualStudio {
            return false;
        }
        let mut parts = self
            .version
            .split('.')
            .map(|p| p.parse::<u32>().unwrap_or(0));
        let major = parts.next().unwrap_or(0);
        let minor = parts.next().unwrap_or(0);
        major > 17 || (major == 17 && minor >= 14)
    }
}

/// The install a family's verb opens when none is named: the first of
/// `editors` in that family. [`detect`] returns each family in preference
/// order (VS Code before its forks, the newest Visual Studio release before
/// older ones and previews).
pub fn default_of(editors: &[Editor], family: Family) -> Option<&Editor> {
    editors.iter().find(|e| e.family == family)
}

/// The install `id` names, or the family's default when `id` is a family
/// word (`vscode`, `visual-studio`) or None.
pub fn pick<'a>(editors: &'a [Editor], id: Option<&str>, family: Family) -> Option<&'a Editor> {
    match id.map(str::trim).filter(|s| !s.is_empty()) {
        Some(id) if Family::from_word(id).is_some() => {
            default_of(editors, Family::from_word(id).unwrap_or(family))
        }
        Some(id) => editors.iter().find(|e| e.id == id),
        None => default_of(editors, family),
    }
}

// ---- the cache ----

/// What [`detected`] holds: None until the first scan lands.
fn cache() -> &'static Mutex<Option<Arc<Vec<Editor>>>> {
    static CACHE: OnceLock<Mutex<Option<Arc<Vec<Editor>>>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(None))
}

/// The installs found, scanning this machine the first time it is asked
/// (registry reads, a few folder listings and one vswhere run: a few
/// hundred milliseconds, so call it off any UI loop, or [`cached`]).
pub fn detected() -> Arc<Vec<Editor>> {
    if let Some(found) = cached() {
        return found;
    }
    rescan()
}

/// The installs found so far, without scanning: None before the first scan
/// has finished. What a menu reads, so building one never waits on vswhere.
pub fn cached() -> Option<Arc<Vec<Editor>>> {
    cache().lock().ok().and_then(|c| c.clone())
}

/// Scan again (Settings' Re-scan, after an install), replacing the cache.
pub fn rescan() -> Arc<Vec<Editor>> {
    let found = Arc::new(detect(&detect::RealProbe));
    if let Ok(mut c) = cache().lock() {
        *c = Some(found.clone());
    }
    found
}

/// Start the first scan on a thread of its own, unless one has landed: what
/// an app calls at startup so its first menu already knows its editors.
pub fn detect_in_background() {
    if cached().is_some() {
        return;
    }
    let _ = std::thread::Builder::new()
        .name("kete-editors".into())
        .spawn(|| {
            detected();
        });
}

// ---- windows, docking, icons (Windows; elsewhere they say so) ----

/// A top-level window of an editor, as the open-window check sees it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct EditorWindow {
    /// The window's handle, as an integer (an `HWND` on Windows).
    pub hwnd: isize,
    pub pid: u32,
    /// The [`Editor::id`] whose exe owns it.
    pub editor: String,
    pub title: String,
}

/// The visible, unowned, uncloaked top-level windows `editors`' exes own,
/// topmost first. Empty off Windows.
pub fn windows(editors: &[Editor]) -> Vec<EditorWindow> {
    #[cfg(windows)]
    {
        win::editor_windows(editors)
    }
    #[cfg(not(windows))]
    {
        let _ = editors;
        Vec::new()
    }
}

/// The windows of `editors` that look open on `folder` (a checkout) or on
/// one of `solutions`, by title: a heuristic, for the "open" dot only.
pub fn open_on(editors: &[Editor], folder: &Path, solutions: &[Solution]) -> Vec<EditorWindow> {
    let stems: Vec<String> = solutions.iter().map(|s| s.stem()).collect();
    windows(editors)
        .into_iter()
        .filter(|w| {
            let family = editors
                .iter()
                .find(|e| e.id == w.editor)
                .map(|e| e.family)
                .unwrap_or(Family::Code);
            titles::matches(&w.title, family, folder, &stems)
        })
        .collect()
}

/// Let the process `pid` (an editor just started) take the foreground,
/// which Kete may hand on while it has it. A no-op off Windows.
pub fn allow_foreground(pid: u32) {
    #[cfg(windows)]
    win::allow_foreground(pid);
    #[cfg(not(windows))]
    let _ = pid;
}

/// Why Dock can't run, in the words the apps show.
pub const DOCK_NO_WINDOW: &str = "No VS Code or Visual Studio window is open on this checkout";

/// Snap an editor window for `folder` (or one of `solutions`) to the half
/// of Kete's monitor opposite `kete` (Kete's own top-level window), and fit
/// Kete to the other half. The editor window is the one whose title names
/// the checkout, else the topmost window of the default editor; neither
/// window is activated. Err is the line to show: no window, an elevated
/// editor (UIPI won't let Kete move it), or not on Windows.
pub fn dock(
    editors: &[Editor],
    folder: &Path,
    solutions: &[Solution],
    kete: isize,
) -> Result<(), String> {
    #[cfg(windows)]
    {
        win::dock(editors, folder, solutions, kete)
    }
    #[cfg(not(windows))]
    {
        let _ = (editors, folder, solutions, kete);
        Err("Docking an editor beside Kete is Windows-only for now".into())
    }
}

/// The top-level window of the terminal the TUI runs in, for Dock: the
/// console's own window, or the terminal that owns its pseudo-console
/// (Windows Terminal). None when there is none to find.
pub fn console_window() -> Option<isize> {
    #[cfg(windows)]
    {
        win::console_window()
    }
    #[cfg(not(windows))]
    {
        None
    }
}

/// The editor's own icon at `size` px, as a PNG file under `cache_dir`,
/// extracted from its exe the first time and kept (keyed by the exe's
/// path, size and modification time). None when the icon can't be read,
/// and always off Windows: the apps fall back to a drawn icon.
pub fn icon(editor: &Editor, size: u32, cache_dir: &Path) -> Option<PathBuf> {
    #[cfg(windows)]
    {
        win::cached_icon(&editor.exe, size, cache_dir)
    }
    #[cfg(not(windows))]
    {
        let _ = (editor, size, cache_dir);
        None
    }
}

/// Kinds of file Open with default app refuses: what the system would run
/// rather than show (GitHub Desktop's rule). Show in Explorer instead.
const RUNS: &[&str] = &[
    "exe",
    "com",
    "bat",
    "cmd",
    "ps1",
    "psm1",
    "msi",
    "msix",
    "appx",
    "lnk",
    "pif",
    "scr",
    "vbs",
    "vbe",
    "js",
    "jse",
    "wsf",
    "wsh",
    "hta",
    "cpl",
    "msc",
    "jar",
    "reg",
    "sh",
    "application",
    "appref-ms",
    "url",
];

/// Why Open with default app refuses a file.
pub const RUNS_PROGRAM: &str = "Kete doesn't run programs: use Show in Explorer";

/// Whether Open with default app refuses `path`: a program or a script.
pub fn runs_program(path: &Path) -> bool {
    path.extension()
        .map(|e| e.to_string_lossy().to_ascii_lowercase())
        .is_some_and(|e| RUNS.contains(&e.as_str()))
}

/// Where the icons are cached under Kete's data dir.
pub fn icon_cache_dir(data_dir: &Path) -> PathBuf {
    data_dir.join("cache").join("editor-icons")
}

#[cfg(test)]
mod tests;
