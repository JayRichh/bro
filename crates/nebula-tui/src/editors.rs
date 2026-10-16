//! The editor verbs in the terminal app: Open in VS Code (or the fork that
//! is installed), Open in Visual Studio where a checkout has a solution or
//! project, Show in Explorer, Dock editor beside Kete, Open with default app
//! and Open in Kete — what a menu row, `?` and a bound key run, on the same
//! `nebula_core::editors` the desktop window uses, so a verb opens the same
//! editor the same way in either app.
//!
//! Rows follow the menu matrix's state rules, the terminal app's way (no
//! disabled rows: a row that can't run now is left out): none over ssh
//! (the editor would open on the far machine's screen, not this one); an
//! editor only once it is known to be installed (VS Code is assumed until
//! the scan answers, Visual Studio is not); Visual Studio only where the
//! checkout has a solution; Dock only on Windows and not once the window
//! check says no editor is open on the checkout. Building a menu never
//! scans: the installs come from the scan started at launch, the solutions
//! and the window check from short caches.

use crate::app::{App, ContextMenu, MenuAction, MenuItem, Overlay};
use crate::keymap::Action;
use nebula_core::editors::{self, Editor, Family};
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// What an editor row does, carried by its menu row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EditorAct {
    /// Open `path` (a checkout, or a folder or file in one) in `editor` (an
    /// install's id; None: the family's default). `root`: the checkout it
    /// is in, so VS Code's window for that checkout takes a file. `line`:
    /// where a file opens.
    Open {
        family: Family,
        editor: Option<String>,
        path: PathBuf,
        root: Option<PathBuf>,
        line: Option<u32>,
    },
    /// Visual Studio on the checkout at `root`: its solution, or a pick of
    /// them when there are several.
    Solution {
        editor: Option<String>,
        root: PathBuf,
    },
    /// One solution picked.
    OpenSolution {
        editor: Option<String>,
        root: PathBuf,
        solution: PathBuf,
    },
    /// Show in Explorer: a checkout's folder opened (`folder`), or a file
    /// or folder selected in its parent.
    Reveal { path: PathBuf, folder: bool },
    /// Dock the editor open on the checkout at this path beside the
    /// terminal Kete runs in.
    Dock(PathBuf),
    /// Open a file with the app the system gives its kind.
    OpenDefault(PathBuf),
    /// Open a file in Kete's own FILE TABS.
    View { root: PathBuf, path: PathBuf },
}

impl EditorAct {
    /// The editor family the row opens, for its glyph.
    pub fn family(&self) -> Option<Family> {
        match self {
            EditorAct::Open { family, .. } => Some(*family),
            EditorAct::Solution { .. } | EditorAct::OpenSolution { .. } => {
                Some(Family::VisualStudio)
            }
            _ => None,
        }
    }
}

/// Dock moves windows on Windows only.
const DOCKABLE: bool = cfg!(windows);

/// The installs found: the scan's cache, None until it has answered.
pub(crate) fn found() -> Option<Arc<Vec<Editor>>> {
    #[cfg(test)]
    if let Some(over) = test::EDITORS.with(|e| e.borrow().clone()) {
        return over;
    }
    editors::cached()
}

/// What Visual Studio would open in the checkout at `root`, cached a few
/// seconds.
fn solutions(root: &Path) -> Vec<editors::Solution> {
    editors::solutions::solutions_cached(root)
}

/// Whether an editor window looks open on the checkout at `root`: None
/// while that isn't known (no installs yet, or not on Windows).
fn window_open(found: &[Editor], root: &Path) -> Option<bool> {
    #[cfg(test)]
    {
        let _ = (found, root);
        test::WINDOW.with(|w| *w.borrow())
    }
    #[cfg(not(test))]
    {
        use std::collections::HashMap;
        use std::sync::{Mutex, OnceLock};
        use std::time::{Duration, Instant};
        type Seen = Mutex<HashMap<PathBuf, (Instant, bool)>>;
        static SEEN: OnceLock<Seen> = OnceLock::new();
        if !DOCKABLE || found.is_empty() {
            return None;
        }
        let seen = SEEN.get_or_init(|| Mutex::new(HashMap::new()));
        if let Some((at, open)) = seen.lock().ok()?.get(root) {
            if at.elapsed() < Duration::from_secs(2) {
                return Some(*open);
            }
        }
        let open = !editors::open_on(found, root, &solutions(root)).is_empty();
        seen.lock()
            .ok()?
            .insert(root.to_path_buf(), (Instant::now(), open));
        Some(open)
    }
}

/// Whether `family` has an install: VS Code is assumed until the scan has
/// answered (unknown is not "no"), Visual Studio is not.
fn has(found: &Option<Arc<Vec<Editor>>>, family: Family) -> bool {
    match found {
        Some(list) => list.iter().any(|e| e.family == family),
        None => family == Family::Code,
    }
}

/// What a checkout's editor verb does on the checkout at `root`, as its
/// menu row runs it; None where it means nothing now (see the module's
/// rules).
pub(crate) fn checkout_action(app: &App, action: Action, root: &Path) -> Option<MenuAction> {
    if app.chrome.is_remote {
        return None;
    }
    let found = found();
    let act = match action {
        Action::OpenInVscode if has(&found, Family::Code) => EditorAct::Open {
            family: Family::Code,
            editor: None,
            path: root.to_path_buf(),
            root: Some(root.to_path_buf()),
            line: None,
        },
        Action::OpenInVisualStudio
            if has(&found, Family::VisualStudio) && !solutions(root).is_empty() =>
        {
            EditorAct::Solution {
                editor: None,
                root: root.to_path_buf(),
            }
        }
        Action::RevealInExplorer => EditorAct::Reveal {
            path: root.to_path_buf(),
            folder: true,
        },
        Action::DockEditor if DOCKABLE || cfg!(test) => {
            let list = found.as_deref().map(Vec::as_slice);
            if list.is_some_and(<[Editor]>::is_empty)
                || window_open(list.unwrap_or_default(), root) == Some(false)
            {
                return None;
            }
            EditorAct::Dock(root.to_path_buf())
        }
        _ => return None,
    };
    Some(MenuAction::Editor(act))
}

/// What a file's editor verb does on the file at `path` in the checkout at
/// `root` (`line`: where it opens), for the file rows' menus (a diff's
/// file list, the tree, a finder): lane/tui-menus routes its File targets
/// here. None where it means nothing now.
pub fn file_action(
    app: &App,
    action: Action,
    root: &Path,
    path: &Path,
    line: Option<u32>,
) -> Option<MenuAction> {
    let found = found();
    let local = !app.chrome.is_remote;
    let act = match action {
        Action::OpenFileInKete => EditorAct::View {
            root: root.to_path_buf(),
            path: path.to_path_buf(),
        },
        Action::OpenInVscode if local && has(&found, Family::Code) => EditorAct::Open {
            family: Family::Code,
            editor: None,
            path: path.to_path_buf(),
            root: Some(root.to_path_buf()),
            line,
        },
        Action::OpenInVisualStudio
            if local && has(&found, Family::VisualStudio) && !solutions(root).is_empty() =>
        {
            EditorAct::Open {
                family: Family::VisualStudio,
                editor: None,
                path: path.to_path_buf(),
                root: Some(root.to_path_buf()),
                line: None,
            }
        }
        // A program or a script never goes to its default app.
        Action::OpenWithDefault if local && !editors::runs_program(path) => {
            EditorAct::OpenDefault(path.to_path_buf())
        }
        Action::RevealInExplorer if local => EditorAct::Reveal {
            path: path.to_path_buf(),
            folder: false,
        },
        _ => return None,
    };
    Some(MenuAction::Editor(act))
}

/// A Code-family install's row words: the registry's "Open in VS Code"
/// for VS Code, else its own name ("Open in Cursor").
pub(crate) fn code_label(registry: &str) -> String {
    match found()
        .as_deref()
        .and_then(|l| editors::default_of(l, Family::Code))
    {
        Some(e) if e.short != "VS Code" => format!("Open in {}", e.short),
        _ => registry.to_string(),
    }
}

/// Visual Studio's row words: the year when several are installed, `…`
/// when the checkout has several solutions to pick from.
pub(crate) fn vs_label(registry: &str, root: &Path) -> String {
    let list = found();
    let all: Vec<&Editor> = list
        .as_deref()
        .map(|l| {
            l.iter()
                .filter(|e| e.family == Family::VisualStudio)
                .collect()
        })
        .unwrap_or_default();
    let base = match all.first() {
        Some(e) if all.len() > 1 => format!("Open in {}", e.short),
        _ => registry.to_string(),
    };
    if solutions(root).len() > 1 {
        format!("{base}\u{2026}")
    } else {
        base
    }
}

/// The rows after an editor verb's own for the other installs of its
/// family (Insiders and Cursor after VS Code; Visual Studio 2019 after
/// 2022), each opening that install.
pub(crate) fn more_installs(action: &MenuAction) -> Vec<MenuItem> {
    let Some(list) = found() else {
        return Vec::new();
    };
    let MenuAction::Editor(act) = action else {
        return Vec::new();
    };
    let Some(family) = act.family() else {
        return Vec::new();
    };
    let several = match act {
        EditorAct::Solution { root, .. } if solutions(root).len() > 1 => "\u{2026}",
        _ => "",
    };
    list.iter()
        .filter(|e| e.family == family)
        .skip(1)
        .map(|e| {
            let mut act = act.clone();
            match &mut act {
                EditorAct::Open { editor, .. } | EditorAct::Solution { editor, .. } => {
                    *editor = Some(e.id.clone())
                }
                _ => {}
            }
            MenuItem::new(
                format!("Open in {}{several}", e.short),
                MenuAction::Editor(act),
            )
        })
        .collect()
}

/// A menu row's editor glyph and its colour: Nerd Font's VS Code and
/// Visual Studio marks, `VSC` / `VS` in the `ascii` style.
pub(crate) fn glyph(
    item: &MenuItem,
    style: crate::icons::IconStyle,
) -> Option<(String, ratatui::style::Color)> {
    use crate::icons::IconStyle;
    use ratatui::style::Color;
    let MenuAction::Editor(act) = &item.action else {
        return None;
    };
    let (nerd, ascii, color) = match act.family()? {
        // nf-md-microsoft_visual_studio_code, nf-md-microsoft_visual_studio.
        Family::Code => ('\u{f0a1e}', "VSC", Color::Rgb(35, 150, 243)),
        Family::VisualStudio => ('\u{f0610}', "VS", Color::Rgb(177, 128, 215)),
    };
    match style {
        IconStyle::Off => None,
        IconStyle::Nerd => Some((format!("{nerd} "), color)),
        IconStyle::Ascii => Some((format!("{ascii:<3} "), color)),
    }
}

// ---- running a row ----

fn flash(app: &mut App, line: impl Into<String>) {
    app.chrome.flash = Some(line.into());
    app.chrome.dirty = true;
}

/// The installs, scanning now if the scan started at launch hasn't
/// answered yet (a user's own press, so a moment's wait is theirs).
fn installs() -> Arc<Vec<Editor>> {
    found().unwrap_or_else(editors::detected)
}

fn name_of(p: &Path) -> String {
    p.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| p.display().to_string())
}

/// Run an editor row.
pub(crate) fn run(app: &mut App, act: EditorAct) {
    match act {
        EditorAct::Open {
            family,
            editor,
            path,
            root,
            line,
        } => {
            let list = installs();
            let Some(e) = editors::pick(&list, editor.as_deref(), family) else {
                return flash(app, missing(family));
            };
            let what = if path.is_file() {
                editors::Open::File {
                    folder: root.as_deref(),
                    file: &path,
                    line,
                    column: None,
                }
            } else {
                editors::Open::Folder(&path)
            };
            let line = match launch(e, &what) {
                Ok(()) => format!("opening {} in {}", name_of(&path), e.short),
                Err(why) => why,
            };
            flash(app, line);
        }
        EditorAct::Solution { editor, root } => {
            let all = ordered(app, &root);
            match all.as_slice() {
                [] => flash(
                    app,
                    "Visual Studio opens a solution or a project, and this checkout has none",
                ),
                [one] => run(
                    app,
                    EditorAct::OpenSolution {
                        editor,
                        root: root.clone(),
                        solution: one.path.clone(),
                    },
                ),
                several => {
                    let items = several
                        .iter()
                        .map(|s| {
                            MenuItem::new(
                                s.rel.clone(),
                                MenuAction::Editor(EditorAct::OpenSolution {
                                    editor: editor.clone(),
                                    root: root.clone(),
                                    solution: s.path.clone(),
                                }),
                            )
                        })
                        .collect();
                    app.modals.overlay = Some(Overlay::Menu(ContextMenu {
                        title: Some("Open in Visual Studio".into()),
                        items,
                        at: None,
                        hover: 0,
                        area: ratatui::layout::Rect::default(),
                        filter: None,
                        parent: None,
                    }));
                    app.chrome.dirty = true;
                }
            }
        }
        EditorAct::OpenSolution {
            editor,
            root,
            solution,
        } => {
            let list = installs();
            let Some(e) = editors::pick(&list, editor.as_deref(), Family::VisualStudio) else {
                return flash(app, missing(Family::VisualStudio));
            };
            let slnx = solution
                .extension()
                .is_some_and(|x| x.eq_ignore_ascii_case("slnx"));
            if slnx && !e.opens_slnx() {
                return flash(
                    app,
                    format!(
                        "{} opens .slnx solutions from 17.14 on; this one is {}",
                        e.short, e.version
                    ),
                );
            }
            let line = match launch(e, &editors::Open::Solution(&solution)) {
                Ok(()) => {
                    remember(app, &root, &solution);
                    format!("opening {} in {}", name_of(&solution), e.short)
                }
                Err(why) => why,
            };
            flash(app, line);
        }
        EditorAct::Reveal { path, folder } => {
            let line = if reveal(&path, folder) {
                format!("showing {}", path.display())
            } else {
                format!("couldn't show {}", path.display())
            };
            flash(app, line);
        }
        EditorAct::Dock(root) => {
            let Some(kete) = console_window() else {
                return flash(
                    app,
                    "Kete couldn't find its terminal's window to dock beside",
                );
            };
            let list = installs();
            let line = match dock(&list, &root, kete) {
                Ok(()) => "docked the editor beside Kete".to_string(),
                Err(why) => why,
            };
            flash(app, line);
        }
        EditorAct::OpenDefault(path) => {
            if editors::runs_program(&path) {
                return flash(app, editors::RUNS_PROGRAM);
            }
            let line = if open_default(&path) {
                format!("opening {}", name_of(&path))
            } else {
                format!("couldn't open {}", path.display())
            };
            flash(app, line);
        }
        EditorAct::View { root, path } => crate::file_tabs::open(app, root, vec![path]),
    }
}

fn missing(family: Family) -> &'static str {
    match family {
        Family::Code => "VS Code isn't installed (or Kete hasn't found it: `kete editors list` looks again)",
        Family::VisualStudio => "Visual Studio isn't installed (or Kete hasn't found it: `kete editors list` looks again)",
    }
}

/// The checkout's solutions, the one last opened in its repo first.
fn ordered(app: &App, root: &Path) -> Vec<editors::Solution> {
    let mut all = editors::solutions(root);
    let prefs = nebula_config::desktop::DesktopPrefs::load();
    let repo = repo_of(app, root);
    if let Some(at) = repo
        .as_deref()
        .and_then(|r| prefs.solution_for(r))
        .and_then(|rel| all.iter().position(|s| s.rel == rel))
    {
        all[..=at].rotate_right(1);
    }
    all
}

/// The repo a checkout at `root` belongs to, keyed as the window keys it:
/// its project's path.
fn repo_of(app: &App, root: &Path) -> Option<String> {
    let w = app.tree.worktrees.iter().find(|w| w.path == root)?;
    let p = app.tree.projects.iter().find(|p| p.id == w.project_id)?;
    Some(p.repo_path.to_string_lossy().into_owned())
}

/// Remember the solution picked from several, as the window does.
fn remember(app: &App, root: &Path, solution: &Path) {
    if editors::solutions(root).len() < 2 || cfg!(test) {
        return;
    }
    let (Some(repo), Ok(rel)) = (repo_of(app, root), solution.strip_prefix(root)) else {
        return;
    };
    let rel = rel.to_string_lossy().replace('\\', "/");
    if let Err(e) = nebula_config::desktop::DesktopPrefs::update(|p| {
        p.solution.insert(repo, rel);
        Ok(())
    }) {
        tracing::warn!(%e, "solution not remembered");
    }
}

// ---- the OS (never from a test) ----

fn launch(e: &Editor, what: &editors::Open) -> Result<(), String> {
    #[cfg(test)]
    {
        test::LAUNCHED.with(|l| l.borrow_mut().push(format!("{} {what:?}", e.id)));
        Ok(())
    }
    #[cfg(not(test))]
    {
        let opts = editors::Options {
            reuse_window: nebula_config::desktop::DesktopPrefs::load().editor_reuse == "on",
        };
        editors::launch(e, what, &opts).map(|_| ())
    }
}

fn console_window() -> Option<isize> {
    if cfg!(test) {
        return Some(1);
    }
    editors::console_window()
}

fn dock(list: &[Editor], root: &Path, kete: isize) -> Result<(), String> {
    #[cfg(test)]
    {
        let _ = (list, kete);
        test::LAUNCHED.with(|l| l.borrow_mut().push(format!("dock {}", root.display())));
        Ok(())
    }
    #[cfg(not(test))]
    {
        editors::dock(list, root, &solutions(root), kete)
    }
}

/// Start an opener with nothing attached and reap it; whether it started.
#[cfg(not(test))]
fn start(mut cmd: std::process::Command) -> bool {
    use std::process::Stdio;
    let Ok(mut child) = cmd
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
    else {
        return false;
    };
    // Explorer answers 1 even when it did what it was asked.
    std::thread::spawn(move || {
        let _ = child.wait();
    });
    true
}

/// Show `path` in the system's file manager.
fn reveal(path: &Path, folder: bool) -> bool {
    #[cfg(test)]
    {
        test::LAUNCHED.with(|l| {
            l.borrow_mut()
                .push(format!("reveal {} {folder}", path.display()))
        });
        true
    }
    #[cfg(all(not(test), windows))]
    {
        use std::os::windows::process::CommandExt;
        let mut cmd = std::process::Command::new("explorer.exe");
        if folder {
            cmd.arg(path);
        } else {
            // `/select,"<path>"` as one argument, quoted the way Explorer
            // reads it (a path can't hold a `"` on Windows).
            cmd.raw_arg(format!("/select,\"{}\"", path.display()));
        }
        start(cmd)
    }
    #[cfg(all(not(test), target_os = "macos"))]
    {
        let mut cmd = std::process::Command::new("open");
        if !folder {
            cmd.arg("-R");
        }
        cmd.arg(path);
        start(cmd)
    }
    #[cfg(all(not(test), unix, not(target_os = "macos")))]
    {
        let dir = if folder { Some(path) } else { path.parent() };
        let mut cmd = std::process::Command::new("xdg-open");
        cmd.arg(dir.unwrap_or(path));
        start(cmd)
    }
}

/// Hand `path` to the app the system gives its kind.
fn open_default(path: &Path) -> bool {
    #[cfg(test)]
    {
        test::LAUNCHED.with(|l| l.borrow_mut().push(format!("default {}", path.display())));
        true
    }
    #[cfg(all(not(test), windows))]
    {
        let mut cmd = std::process::Command::new("explorer.exe");
        cmd.arg(path);
        start(cmd)
    }
    #[cfg(all(not(test), target_os = "macos"))]
    {
        let mut cmd = std::process::Command::new("open");
        cmd.arg(path);
        start(cmd)
    }
    #[cfg(all(not(test), unix, not(target_os = "macos")))]
    {
        let mut cmd = std::process::Command::new("xdg-open");
        cmd.arg(path);
        start(cmd)
    }
}

#[cfg(test)]
pub(crate) mod test {
    use super::*;
    use std::cell::RefCell;

    thread_local! {
        /// The installs a test's menus see (`Some(None)`: the scan hasn't
        /// answered); unset, the real cache.
        pub(crate) static EDITORS: RefCell<Option<Option<Arc<Vec<Editor>>>>> = const { RefCell::new(None) };
        /// The window check's answer for every checkout.
        pub(crate) static WINDOW: RefCell<Option<bool>> = const { RefCell::new(None) };
        /// What would have been launched, revealed, docked or opened.
        pub(crate) static LAUNCHED: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
    }

    /// Make the menus on this thread see `list` (None: not scanned yet).
    pub(crate) fn installs(list: Option<Vec<Editor>>) {
        EDITORS.with(|e| *e.borrow_mut() = Some(list.map(Arc::new)));
    }

    pub(crate) fn window(open: Option<bool>) {
        WINDOW.with(|w| *w.borrow_mut() = open);
    }

    pub(crate) fn launched() -> Vec<String> {
        LAUNCHED.with(|l| std::mem::take(&mut *l.borrow_mut()))
    }

    /// An install for a test.
    pub(crate) fn install(id: &str, family: Family, short: &str) -> Editor {
        Editor {
            id: id.into(),
            family,
            short: short.into(),
            name: short.into(),
            version: if family == Family::VisualStudio {
                "17.14.16".into()
            } else {
                "1.141.0".into()
            },
            edition: None,
            year: None,
            prerelease: false,
            exe: PathBuf::from(format!("C:/x/{id}.exe")),
            dir: PathBuf::from("C:/x"),
            source: nebula_core::editors::Source::UserInstall,
            instance_id: None,
            application_name: None,
            name_short: None,
            data_folder_name: None,
        }
    }
}
