//! What Visual Studio would open in a checkout: its `.slnx` / `.sln`
//! solutions, else its `.csproj` / `.vcxproj` projects, at the root or one
//! folder down. Visual Studio is offered only where one exists (Jay's "icon
//! for the API project"); `devenv <folder>` is no documented switch.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

/// What kind of file a [`Solution`] is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SolutionKind {
    /// The XML solution format, opened by Visual Studio 17.14 and later.
    Slnx,
    Sln,
    Csproj,
    Vcxproj,
}

impl SolutionKind {
    fn of(name: &str) -> Option<Self> {
        let lower = name.to_ascii_lowercase();
        let ext = lower.rsplit_once('.')?.1;
        Some(match ext {
            "slnx" => SolutionKind::Slnx,
            "sln" => SolutionKind::Sln,
            "csproj" => SolutionKind::Csproj,
            "vcxproj" => SolutionKind::Vcxproj,
            _ => return None,
        })
    }

    fn is_solution(self) -> bool {
        matches!(self, SolutionKind::Slnx | SolutionKind::Sln)
    }
}

/// A solution or project in a checkout.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Solution {
    pub path: PathBuf,
    /// Its path from the checkout's root, `/`-separated (`Api.sln`,
    /// `src/Api.sln`).
    pub rel: String,
    pub kind: SolutionKind,
}

impl Solution {
    /// The file name without its extension: what Visual Studio's title
    /// shows.
    pub fn stem(&self) -> String {
        self.path
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default()
    }
}

/// Folders never searched: dependencies and build output (and every
/// hidden folder, `.git` and `.vs` among them).
const SKIP: &[&str] = &["node_modules", "bin", "obj", "target"];

/// At most this many folders at the root are looked into.
const MAX_DIRS: usize = 400;

/// The solutions at `root` and one folder down, best first: one named for
/// the checkout's folder, then the root's before a subfolder's, `.slnx`
/// before `.sln`, then by name. With none, the projects (`.csproj`,
/// `.vcxproj`) the same way. Hidden folders and [`SKIP`] are not
/// searched. Empty when there is nothing for Visual Studio.
pub fn solutions(root: &Path) -> Vec<Solution> {
    let folder = root
        .file_name()
        .map(|s| s.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();
    let mut found = Vec::new();
    let mut dirs = Vec::new();
    for entry in read(root) {
        match entry {
            Entry::File(name) => push(&mut found, root, "", &name),
            Entry::Dir(name) => {
                let lower = name.to_ascii_lowercase();
                if !name.starts_with('.') && !SKIP.contains(&lower.as_str()) {
                    dirs.push(name);
                }
            }
        }
    }
    dirs.sort();
    for dir in dirs.into_iter().take(MAX_DIRS) {
        for entry in read(&root.join(&dir)) {
            if let Entry::File(name) = entry {
                push(&mut found, &root.join(&dir), &format!("{dir}/"), &name);
            }
        }
    }
    let want_solutions = found.iter().any(|s: &Solution| s.kind.is_solution());
    found.retain(|s| s.kind.is_solution() == want_solutions);
    found.sort_by_key(|s| {
        (
            s.stem().to_ascii_lowercase() != folder,
            s.rel.contains('/'),
            s.kind,
            s.rel.to_ascii_lowercase(),
        )
    });
    found
}

enum Entry {
    File(String),
    Dir(String),
}

fn read(dir: &Path) -> Vec<Entry> {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    rd.flatten()
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().into_owned();
            let kind = e.file_type().ok()?;
            Some(if kind.is_dir() {
                Entry::Dir(name)
            } else {
                Entry::File(name)
            })
        })
        .collect()
}

fn push(found: &mut Vec<Solution>, dir: &Path, prefix: &str, name: &str) {
    if let Some(kind) = SolutionKind::of(name) {
        found.push(Solution {
            path: dir.join(name),
            rel: format!("{prefix}{name}"),
            kind,
        });
    }
}

/// How long [`solutions_cached`] trusts a scan.
const FRESH: Duration = Duration::from_secs(15);

/// [`solutions`], scanned at most once every few seconds per checkout: for
/// a menu built on every keypress (the terminal app's), so building one
/// never lists folders twice in a row.
pub fn solutions_cached(root: &Path) -> Vec<Solution> {
    type Cache = Mutex<HashMap<PathBuf, (Instant, Vec<Solution>)>>;
    static CACHE: OnceLock<Cache> = OnceLock::new();
    let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    if let Ok(c) = cache.lock() {
        if let Some((at, hit)) = c.get(root) {
            if at.elapsed() < FRESH {
                return hit.clone();
            }
        }
    }
    let found = solutions(root);
    if let Ok(mut c) = cache.lock() {
        if c.len() > 256 {
            c.clear();
        }
        c.insert(root.to_path_buf(), (Instant::now(), found.clone()));
    }
    found
}
