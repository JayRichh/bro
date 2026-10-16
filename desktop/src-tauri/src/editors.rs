//! The user's own editors from the window: which are installed (with their
//! icons), what Visual Studio would open in each checkout, which checkouts
//! have an editor window open, and the verbs: open a checkout, folder or
//! file in VS Code or Visual Studio, dock the editor beside the window,
//! show a path in Explorer, open a file with its default app, read a
//! checkout's file for Kete's own viewer. nebula-core's `editors` does the
//! work; this resolves the web view's worktree-relative paths first.
//!
//! The web view never names a path to launch on its own: every path is a
//! worktree id plus a path inside that checkout (or a file a session
//! opened with `kete open`), canonicalised and checked to stay inside, so a
//! `..` or a symlink can't reach anywhere else.

use crate::daemon::Client;
use crate::files::OpenedFiles;
use crate::Res;
use base64::Engine;
use nebula_config::desktop::DesktopPrefs;
use nebula_core::editors::{self, Editor, Family, Open, Solution};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;
use tauri::{Manager, State};

/// One detected editor, as the window lists it.
#[derive(Debug, Clone, Serialize)]
pub struct EditorRow {
    #[serde(flatten)]
    pub editor: Editor,
    /// Its icon at 32 px as a data URL; None when it couldn't be read.
    pub icon: Option<String>,
}

/// The installs found and the user's choices about them.
#[derive(Debug, Clone, Serialize)]
pub struct EditorsState {
    pub editors: Vec<EditorRow>,
    /// `editor_folder`, `editor_file`, `editor_reuse` as stored.
    pub folder: String,
    pub file: String,
    pub reuse: bool,
}

fn data_url(png: &[u8]) -> String {
    format!(
        "data:image/png;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(png)
    )
}

fn icon_url(e: &Editor, size: u32) -> Option<String> {
    let dir = editors::icon_cache_dir(&nebula_core::paths::data_dir());
    let file = editors::icon(e, size, &dir)?;
    std::fs::read(file).ok().map(|png| data_url(&png))
}

fn state_of(found: &[Editor]) -> EditorsState {
    let prefs = DesktopPrefs::load();
    EditorsState {
        editors: found
            .iter()
            .map(|e| EditorRow {
                editor: e.clone(),
                icon: icon_url(e, 32),
            })
            .collect(),
        folder: prefs.editor_folder,
        file: prefs.editor_file,
        reuse: prefs.editor_reuse == "on",
    }
}

pub(crate) async fn blocking<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> Res<T> {
    tauri::async_runtime::spawn_blocking(f)
        .await
        .map_err(|e| e.to_string())
}

/// The editors this machine has (scanned once, then cached).
#[tauri::command]
pub async fn editors_list() -> Res<EditorsState> {
    blocking(|| state_of(&editors::detected())).await
}

/// Scan for editors again (after an install or an update).
#[tauri::command]
pub async fn editors_rescan() -> Res<EditorsState> {
    blocking(|| state_of(&editors::rescan())).await
}

/// An editor's icon at `size` px (16 or 32) as a data URL.
#[tauri::command]
pub async fn editor_icon(id: String, size: Option<u32>) -> Res<Option<String>> {
    blocking(move || {
        let found = editors::detected();
        let size = size.unwrap_or(32).clamp(16, 64);
        found
            .iter()
            .find(|e| e.id == id)
            .and_then(|e| icon_url(e, size))
    })
    .await
}

/// A solution or project in a checkout, as the window offers it.
#[derive(Debug, Clone, Serialize)]
pub struct SolutionRow {
    pub rel: String,
    pub kind: nebula_core::editors::SolutionKind,
}

/// What a checkout can be opened with, besides VS Code.
#[derive(Debug, Clone, Serialize)]
pub struct Targets {
    /// What Visual Studio would open, best first, the one last opened in
    /// the repo first of all.
    pub solutions: Vec<SolutionRow>,
    /// It has an Open command (the project's setting or a `.kete.json`).
    pub open_command: bool,
}

/// What each of `worktrees` can be opened with.
#[tauri::command]
pub async fn editor_targets(
    client: State<'_, Arc<Client>>,
    worktrees: Vec<String>,
) -> Res<HashMap<String, Targets>> {
    let model = client.snapshot();
    let roots: Vec<(String, PathBuf, String)> = worktrees
        .iter()
        .filter_map(|id| {
            let w = model.worktrees.iter().find(|w| &w.id.0 == id)?;
            let repo = model
                .projects
                .iter()
                .find(|p| p.id == w.project_id)
                .map_or_else(|| w.path.clone(), |p| p.repo_path.clone());
            Some((
                id.clone(),
                w.path.clone(),
                repo.to_string_lossy().into_owned(),
            ))
        })
        .collect();
    blocking(move || {
        let prefs = DesktopPrefs::load();
        let cfg = nebula_config::Config::load();
        roots
            .into_iter()
            .map(|(id, root, repo)| {
                let mut found = editors::solutions(&root);
                remembered_first(&mut found, prefs.solution_for(&repo));
                let solutions = found
                    .into_iter()
                    .map(|s| SolutionRow {
                        rel: s.rel,
                        kind: s.kind,
                    })
                    .collect();
                let open_command =
                    crate::projects::open_command_for(&cfg, &root, Path::new(&repo)).is_ok();
                (
                    id,
                    Targets {
                        solutions,
                        open_command,
                    },
                )
            })
            .collect()
    })
    .await
}

/// Put the solution remembered for the repo first.
fn remembered_first(found: &mut [Solution], remembered: Option<&str>) {
    if let Some(at) = remembered.and_then(|r| found.iter().position(|s| s.rel == r)) {
        found[..=at].rotate_right(1);
    }
}

/// Which editors have a window open on each of `worktrees` (by title: a
/// heuristic, for the "open" dot only), by worktree id.
#[tauri::command]
pub async fn editor_windows(
    client: State<'_, Arc<Client>>,
    worktrees: Vec<String>,
) -> Res<HashMap<String, Vec<String>>> {
    let model = client.snapshot();
    let roots: Vec<(String, PathBuf)> = worktrees
        .iter()
        .filter_map(|id| {
            let w = model.worktrees.iter().find(|w| &w.id.0 == id)?;
            Some((id.clone(), w.path.clone()))
        })
        .collect();
    blocking(move || {
        let found = editors::detected();
        let open = editors::windows(&found);
        let family = |id: &str| {
            found
                .iter()
                .find(|e| e.id == id)
                .map_or(Family::Code, |e| e.family)
        };
        roots
            .into_iter()
            .map(|(id, root)| {
                let stems: Vec<String> = editors::solutions::solutions_cached(&root)
                    .iter()
                    .map(Solution::stem)
                    .collect();
                let mut ids: Vec<String> = open
                    .iter()
                    .filter(|w| {
                        editors::titles::matches(&w.title, family(&w.editor), &root, &stems)
                    })
                    .map(|w| w.editor.clone())
                    .collect();
                ids.dedup();
                (id, ids)
            })
            .collect()
    })
    .await
}

// ---- paths ----

/// A checkout's root and its repo's main checkout.
pub(crate) fn checkout(client: &Client, worktree: &str) -> Res<(PathBuf, PathBuf)> {
    let model = client.snapshot();
    let w = model
        .worktrees
        .iter()
        .find(|w| w.id.0 == worktree)
        .ok_or_else(|| "That worktree is gone.".to_string())?;
    let main = model
        .projects
        .iter()
        .find(|p| p.id == w.project_id)
        .map_or_else(|| w.path.clone(), |p| p.repo_path.clone());
    Ok((w.path.clone(), main))
}

/// `rel` (a path inside the checkout at `root`, or an absolute one under
/// it) as a path on disk, refused when it leaves the checkout: no `..`,
/// and once symlinks are resolved it must still be inside. The path given
/// back is the plain joined one (editors want `C:\…`, not `\\?\C:\…`).
pub fn inside(root: &Path, rel: &str) -> Res<PathBuf> {
    let given = Path::new(rel);
    let joined = if given.is_absolute() {
        given.to_path_buf()
    } else {
        if given
            .components()
            .any(|c| !matches!(c, Component::Normal(_) | Component::CurDir))
        {
            return Err(format!("{rel} is not a path inside the checkout"));
        }
        root.join(given)
    };
    let real_root = root
        .canonicalize()
        .map_err(|_| format!("{} is not there any more", root.display()))?;
    let real = joined
        .canonicalize()
        .map_err(|_| format!("{} is not there any more", joined.display()))?;
    if !real.starts_with(&real_root) {
        return Err(format!("{rel} is outside the checkout"));
    }
    Ok(joined)
}

/// What a verb acts on: a checkout's root, a folder or file in it, or a
/// file a session opened (`kete open`).
struct Place {
    /// The checkout (its root), when the path is in one.
    root: Option<PathBuf>,
    /// The repo's main checkout, for what is remembered per repo.
    repo: Option<PathBuf>,
    path: PathBuf,
}

fn place(
    client: &Client,
    opened: &OpenedFiles,
    worktree: Option<&str>,
    path: Option<&str>,
) -> Res<Place> {
    match (worktree, path) {
        (Some(w), path) => {
            let (root, repo) = checkout(client, w)?;
            let path = match path.filter(|p| !p.is_empty()) {
                Some(p) => inside(&root, p)?,
                None => {
                    if !root.is_dir() {
                        return Err(format!("{} is not there any more", root.display()));
                    }
                    root.clone()
                }
            };
            Ok(Place {
                root: Some(root),
                repo: Some(repo),
                path,
            })
        }
        (None, Some(p)) => {
            // A file a session opened, or a path in a checkout Kete knows
            // (a viewer's file, which knows its path, not its worktree):
            // the innermost checkout it is in.
            let model = client.snapshot();
            let w = model
                .worktrees
                .iter()
                .filter(|w| Path::new(p).starts_with(&w.path))
                .max_by_key(|w| w.path.as_os_str().len());
            let path = match (opened.check_path(p), w) {
                (Ok(path), _) => path,
                (Err(_), Some(w)) => inside(&w.path, p)?,
                (Err(e), None) => return Err(e),
            };
            let repo = w.and_then(|w| model.projects.iter().find(|x| x.id == w.project_id));
            Ok(Place {
                root: w.map(|w| w.path.clone()),
                repo: repo.map(|x| x.repo_path.clone()),
                path,
            })
        }
        (None, None) => Err("Nothing to open.".into()),
    }
}

// ---- verbs ----

/// What `editor_open` asks for.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct OpenAsk {
    /// The checkout; None for a file a session opened.
    pub worktree: Option<String>,
    /// A folder or file inside the checkout (relative to its root, or
    /// absolute under it); None for the checkout itself.
    pub path: Option<String>,
    /// An install's id, or `vscode` / `visual-studio` for that family's
    /// default; None for VS Code's.
    pub editor: Option<String>,
    pub line: Option<u32>,
    pub column: Option<u32>,
    /// The solution Visual Studio opens, relative to the checkout's root;
    /// None for the remembered one, else the best.
    pub solution: Option<String>,
}

/// What opened, for the window to say.
#[derive(Debug, Clone, Serialize)]
pub struct Opened {
    pub editor: String,
    /// What it opened: the solution's or the path's file name.
    pub what: String,
}

fn name_of(p: &Path) -> String {
    p.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| p.display().to_string())
}

/// Open a checkout, a folder or a file in VS Code or Visual Studio.
#[tauri::command]
pub async fn editor_open(
    client: State<'_, Arc<Client>>,
    opened: State<'_, OpenedFiles>,
    ask: OpenAsk,
) -> Res<Opened> {
    let at = place(
        &client,
        &opened,
        ask.worktree.as_deref(),
        ask.path.as_deref(),
    )?;
    blocking(move || open_in(&editors::detected(), &at, &ask)).await?
}

fn open_in(found: &[Editor], at: &Place, ask: &OpenAsk) -> Res<Opened> {
    let family = if ask.solution.is_some() {
        Family::VisualStudio
    } else {
        ask.editor
            .as_deref()
            .and_then(|id| {
                Family::from_word(id)
                    .or_else(|| found.iter().find(|e| e.id == id).map(|e| e.family))
            })
            .unwrap_or(Family::Code)
    };
    let editor = editors::pick(found, ask.editor.as_deref(), family).ok_or_else(|| {
        match family {
            Family::Code => "VS Code isn't installed (or Kete hasn't found it: re-scan in Settings, Desktop)",
            Family::VisualStudio => "Visual Studio isn't installed (or Kete hasn't found it: re-scan in Settings, Desktop)",
        }
        .to_string()
    })?;
    let prefs = DesktopPrefs::load();
    let opts = editors::launch::Options {
        reuse_window: prefs.editor_reuse == "on",
    };
    let is_file = at.path.is_file();
    let mut solution: Option<PathBuf> = None;
    let what = match editor.family {
        Family::Code if is_file => Open::File {
            folder: at.root.as_deref(),
            file: &at.path,
            line: ask.line,
            column: ask.column,
        },
        Family::Code => Open::Folder(&at.path),
        Family::VisualStudio if is_file && ask.solution.is_none() => Open::File {
            folder: at.root.as_deref(),
            file: &at.path,
            line: None,
            column: None,
        },
        Family::VisualStudio => {
            let root = at.root.as_deref().unwrap_or(&at.path);
            let picked = match ask.solution.as_deref() {
                Some(rel) => inside(root, rel)?,
                None => {
                    let mut all = editors::solutions(root);
                    let repo = at.repo.as_deref().map(|r| r.to_string_lossy().into_owned());
                    remembered_first(
                        &mut all,
                        repo.as_deref().and_then(|r| prefs.solution_for(r)),
                    );
                    all.into_iter()
                        .find(|s| s.kind != editors::SolutionKind::Slnx || editor.opens_slnx())
                        .map(|s| s.path)
                        .ok_or("Visual Studio opens a solution or a project, and this checkout has none")?
                }
            };
            if picked
                .extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("slnx"))
                && !editor.opens_slnx()
            {
                return Err(format!(
                    "{} opens .slnx solutions from 17.14 on; this one is {}",
                    editor.short, editor.version
                ));
            }
            solution = Some(picked);
            Open::Solution(solution.as_deref().unwrap_or(root))
        }
    };
    editors::launch::launch(editor, &what, &opts)?;
    // Several to pick from: the next open in this repo starts on this one.
    if let (Some(sln), Some(root), Some(repo)) = (&solution, &at.root, &at.repo) {
        if let Ok(rel) = sln.strip_prefix(root) {
            let rel = rel.to_string_lossy().replace('\\', "/");
            let repo = repo.to_string_lossy().into_owned();
            let _ = DesktopPrefs::update(|p| {
                p.solution.insert(repo, rel);
                Ok(())
            });
        }
    }
    let shown = solution.as_deref().unwrap_or(&at.path);
    Ok(Opened {
        editor: editor.short.clone(),
        what: name_of(shown),
    })
}

/// Snap the checkout's editor window beside this window, each on half the
/// monitor.
#[tauri::command]
pub async fn editor_dock(
    app: tauri::AppHandle,
    client: State<'_, Arc<Client>>,
    worktree: String,
) -> Res<()> {
    let (root, _) = checkout(&client, &worktree)?;
    let kete = kete_window(&app)?;
    blocking(move || {
        let found = editors::detected();
        let sols = editors::solutions::solutions_cached(&root);
        editors::dock(&found, &root, &sols, kete)
    })
    .await?
}

#[cfg(windows)]
fn kete_window(app: &tauri::AppHandle) -> Res<isize> {
    let w = app
        .get_webview_window("main")
        .ok_or("Kete's window isn't there")?;
    let h = w.hwnd().map_err(|e| e.to_string())?;
    Ok(h.0 as isize)
}

#[cfg(not(windows))]
fn kete_window(app: &tauri::AppHandle) -> Res<isize> {
    let _ = app.get_webview_window("main");
    Err("Docking an editor beside Kete is Windows-only for now".into())
}

/// Show a checkout in Explorer (its folder opened), or a folder or file in
/// it (selected in its parent).
#[tauri::command]
pub fn path_reveal(
    app: tauri::AppHandle,
    client: State<'_, Arc<Client>>,
    opened: State<'_, OpenedFiles>,
    worktree: Option<String>,
    path: Option<String>,
) -> Res<()> {
    use tauri_plugin_opener::OpenerExt;
    let at = place(&client, &opened, worktree.as_deref(), path.as_deref())?;
    if at.root.as_deref() == Some(at.path.as_path()) {
        return app
            .opener()
            .open_path(at.path.display().to_string(), None::<&str>)
            .map_err(|e| e.to_string());
    }
    app.opener()
        .reveal_item_in_dir(&at.path)
        .map_err(|e| e.to_string())
}

/// Open a file in a checkout with the app the system gives its kind:
/// never a program or a script.
#[tauri::command]
pub fn path_open_default(
    app: tauri::AppHandle,
    client: State<'_, Arc<Client>>,
    opened: State<'_, OpenedFiles>,
    worktree: Option<String>,
    path: String,
) -> Res<()> {
    use tauri_plugin_opener::OpenerExt;
    let at = place(&client, &opened, worktree.as_deref(), Some(&path))?;
    if editors::runs_program(&at.path) {
        return Err(editors::RUNS_PROGRAM.into());
    }
    if !at.path.is_file() {
        return Err(format!("{} is not a file", at.path.display()));
    }
    app.opener()
        .open_path(at.path.display().to_string(), None::<&str>)
        .map_err(|e| e.to_string())
}

/// A file in a checkout, for Kete's own viewer (the same caps as a file a
/// session opened).
#[tauri::command]
pub async fn worktree_file_read(
    client: State<'_, Arc<Client>>,
    worktree: String,
    path: String,
) -> Res<crate::files::FileText> {
    let (root, _) = checkout(&client, &worktree)?;
    let file = inside(&root, &path)?;
    blocking(move || crate::files::read_text(&file)).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_path_must_stay_inside_its_checkout() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("api");
        std::fs::create_dir_all(root.join("src")).unwrap();
        std::fs::write(root.join("src/main.rs"), "fn main() {}").unwrap();
        std::fs::write(dir.path().join("secret.txt"), "no").unwrap();
        assert_eq!(
            inside(&root, "src/main.rs").unwrap(),
            root.join("src/main.rs")
        );
        assert_eq!(inside(&root, "./src").unwrap(), root.join("./src"));
        assert!(inside(&root, "../secret.txt").is_err());
        assert!(inside(&root, "src/../../secret.txt").is_err());
        let abs = dir.path().join("secret.txt");
        assert!(inside(&root, &abs.display().to_string()).is_err());
        let ok = root.join("src").join("main.rs");
        assert_eq!(inside(&root, &ok.display().to_string()).unwrap(), ok);
        assert!(inside(&root, "src/missing.rs").is_err());
    }

    #[cfg(unix)]
    #[test]
    fn a_symlink_out_of_the_checkout_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("api");
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(dir.path().join("secret.txt"), "no").unwrap();
        std::os::unix::fs::symlink(dir.path().join("secret.txt"), root.join("link")).unwrap();
        assert!(inside(&root, "link").is_err());
    }

    #[test]
    fn the_remembered_solution_goes_first() {
        let s = |rel: &str| Solution {
            path: PathBuf::from(rel),
            rel: rel.into(),
            kind: editors::SolutionKind::Sln,
        };
        let mut all = vec![s("Api.sln"), s("tools/Tools.sln"), s("web/Web.sln")];
        remembered_first(&mut all, Some("web/Web.sln"));
        let rels: Vec<&str> = all.iter().map(|s| s.rel.as_str()).collect();
        assert_eq!(rels, ["web/Web.sln", "Api.sln", "tools/Tools.sln"]);
        remembered_first(&mut all, Some("gone.sln"));
        assert_eq!(all[0].rel, "web/Web.sln");
    }
}
