//! The Files view's tree: a checkout's files as git lists them (tracked,
//! and untracked but not ignored), one folder at a time as the tree opens,
//! each with git's marks (staged, changed, untracked, added, deleted,
//! conflicted; a folder carries the marks of everything under it).
//!
//! Every folder the web view names is a path inside the checkout, checked
//! by its words alone (no `..`, nothing absolute) so a folder whose files
//! are all deleted on disk still opens; where the folder is on disk it must
//! also resolve inside the checkout, so a symlink can't lead out. git never
//! walks into a symlinked folder, so the listing itself stays inside. Files
//! are read through `editors::worktree_file_read`, behind the same check.

use crate::daemon::Client;
use crate::editors::{blocking, checkout, inside};
use crate::git::{git, status_entries, StatusEntry};
use crate::Res;
use serde::Serialize;
use std::collections::{BTreeMap, HashSet};
use std::path::{Component, Path};
use std::sync::Arc;
use tauri::State;

/// The most rows one folder's listing sends; a "more" row asks for the
/// next page.
pub const LEVEL_CAP: usize = 2_000;
/// The most rows a whole-subtree listing sends (Expand all, the filter).
pub const DEEP_CAP: usize = 20_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Dir,
    File,
    /// A symbolic link (git lists the link, never what it points at).
    Link,
    /// A submodule or a repo inside the checkout: a folder git doesn't
    /// list into.
    Module,
}

impl Kind {
    fn folder_like(self) -> bool {
        matches!(self, Kind::Dir | Kind::Module)
    }
}

fn is_false(b: &bool) -> bool {
    !*b
}

/// git's marks on a file, or on everything under a folder.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct Marks {
    /// Changes in the index.
    #[serde(skip_serializing_if = "is_false")]
    pub staged: bool,
    /// Changes on disk not yet staged.
    #[serde(skip_serializing_if = "is_false")]
    pub changed: bool,
    #[serde(skip_serializing_if = "is_false")]
    pub untracked: bool,
    /// New in the index.
    #[serde(skip_serializing_if = "is_false")]
    pub added: bool,
    /// Tracked, gone from disk.
    #[serde(skip_serializing_if = "is_false")]
    pub deleted: bool,
    #[serde(skip_serializing_if = "is_false")]
    pub conflicted: bool,
}

impl Marks {
    fn of(e: &StatusEntry) -> Marks {
        let (x, y) = (e.x, e.y);
        if x == '?' {
            return Marks {
                untracked: true,
                ..Marks::default()
            };
        }
        if x == 'U' || y == 'U' || (x, y) == ('A', 'A') || (x, y) == ('D', 'D') {
            return Marks {
                conflicted: true,
                ..Marks::default()
            };
        }
        Marks {
            staged: x != ' ',
            changed: y != ' ',
            added: x == 'A',
            deleted: y == 'D',
            ..Marks::default()
        }
    }

    fn join(&mut self, o: Marks) {
        self.staged |= o.staged;
        self.changed |= o.changed;
        self.untracked |= o.untracked;
        self.added |= o.added;
        self.deleted |= o.deleted;
        self.conflicted |= o.conflicted;
    }
}

/// One row of the tree.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Entry {
    /// From the checkout's root, `/`-separated.
    pub path: String,
    pub name: String,
    pub kind: Kind,
    /// A file's size on disk, in a one-folder listing (Open in Kete is
    /// greyed over its cap); left out where it wasn't read.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size: Option<u64>,
    #[serde(flatten)]
    pub marks: Marks,
}

/// What `worktree_tree` sends.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Listing {
    /// The folder listed, `''` for the checkout's root.
    pub dir: String,
    /// One folder: its rows from `offset`, folders first, each by name.
    /// Deep: every folder and file under it, in that order depth first.
    pub entries: Vec<Entry>,
    /// Rows past the cap.
    pub more: usize,
}

/// `dir` as a folder inside the checkout, `/`-separated (`''` the root),
/// by its words alone: nothing absolute, no drive, no `..`.
pub fn tree_dir(dir: &str) -> Res<String> {
    let given = dir.replace('\\', "/");
    let mut parts = Vec::new();
    for c in Path::new(&given).components() {
        match c {
            Component::Normal(s) => parts.push(s.to_string_lossy().into_owned()),
            Component::CurDir => {}
            _ => return Err(format!("{dir} is not a folder inside the checkout")),
        }
    }
    Ok(parts.join("/"))
}

/// One file git lists, with what it is.
#[derive(Debug, Clone, PartialEq)]
struct Listed {
    path: String,
    kind: Kind,
}

/// The parts of `ls-files -s -z`: `mode sha stage\tpath`, a conflicted
/// path once per stage (kept once).
fn parse_ls_files(raw: &[u8]) -> Vec<Listed> {
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for entry in raw.split(|b| *b == 0).filter(|p| !p.is_empty()) {
        let Some(tab) = entry.iter().position(|b| *b == b'\t') else {
            continue;
        };
        let mode = entry.split(|b| *b == b' ').next().unwrap_or_default();
        let path = String::from_utf8_lossy(&entry[tab + 1..]).into_owned();
        if !seen.insert(path.clone()) {
            continue;
        }
        let kind = match mode {
            b"120000" => Kind::Link,
            b"160000" => Kind::Module,
            _ => Kind::File,
        };
        out.push(Listed { path, kind });
    }
    out
}

/// Under `dir` (`''` the root): `path` past `dir/`, or None.
fn under<'a>(dir: &str, path: &'a str) -> Option<&'a str> {
    if dir.is_empty() {
        return Some(path);
    }
    path.strip_prefix(dir)?.strip_prefix('/')
}

/// A node of the folder being listed.
#[derive(Debug, Default)]
struct Node {
    kind: Option<Kind>,
    marks: Marks,
    /// Child name to its node.
    kids: BTreeMap<String, Node>,
}

/// Folders first, then by name, ignoring case (ties by the exact name).
fn order(a: (&String, &Node), b: (&String, &Node)) -> std::cmp::Ordering {
    let folder = |n: &Node| n.kind.is_some_and(Kind::folder_like);
    folder(b.1)
        .cmp(&folder(a.1))
        .then_with(|| a.0.to_lowercase().cmp(&b.0.to_lowercase()))
        .then_with(|| a.0.cmp(b.0))
}

fn sorted(kids: &BTreeMap<String, Node>) -> Vec<(&String, &Node)> {
    let mut v: Vec<_> = kids.iter().collect();
    v.sort_by(|a, b| order(*a, *b));
    v
}

/// The folder `dir` as a tree of what git lists under it, marks rolled up
/// into each folder.
fn grow(dir: &str, files: &[Listed], status: &[StatusEntry]) -> Node {
    let mut top = Node {
        kind: Some(Kind::Dir),
        ..Node::default()
    };
    for f in files {
        let Some(rest) = under(dir, &f.path) else {
            continue;
        };
        let mut at = &mut top;
        let mut parts = rest.split('/').filter(|p| !p.is_empty()).peekable();
        while let Some(part) = parts.next() {
            let last = parts.peek().is_none();
            at = at.kids.entry(part.to_string()).or_default();
            if last {
                at.kind = Some(f.kind);
            } else if at.kind.is_none() {
                at.kind = Some(Kind::Dir);
            }
        }
    }
    for s in status {
        let Some(rest) = under(dir, &s.path) else {
            continue;
        };
        let m = Marks::of(s);
        top.marks.join(m);
        let mut at = &mut top;
        for part in rest.split('/').filter(|p| !p.is_empty()) {
            // A staged deletion is in no listing: its folders carry it.
            let Some(next) = at.kids.get_mut(part) else {
                break;
            };
            at = next;
            at.marks.join(m);
        }
    }
    top
}

fn entry(dir: &str, name: &str, n: &Node) -> Entry {
    Entry {
        path: if dir.is_empty() {
            name.to_string()
        } else {
            format!("{dir}/{name}")
        },
        name: name.to_string(),
        kind: n.kind.unwrap_or(Kind::File),
        size: None,
        marks: n.marks,
    }
}

/// The listing of `dir` from git's files and status: one folder's rows
/// from `offset` (at most `cap`), or with `deep` the whole subtree (at
/// most `cap`).
fn listing(
    dir: &str,
    files: &[Listed],
    status: &[StatusEntry],
    deep: bool,
    offset: usize,
    cap: usize,
) -> Listing {
    let top = grow(dir, files, status);
    let mut entries = Vec::new();
    let mut total = 0;
    if deep {
        // Depth first, each folder's rows in their order.
        fn walk(path: &str, n: &Node, out: &mut Vec<Entry>, total: &mut usize, cap: usize) {
            for (name, kid) in sorted(&n.kids) {
                *total += 1;
                let e = entry(path, name, kid);
                let next = e.path.clone();
                if out.len() < cap {
                    out.push(e);
                }
                if kid.kind == Some(Kind::Dir) {
                    walk(&next, kid, out, total, cap);
                }
            }
        }
        walk(dir, &top, &mut entries, &mut total, cap);
    } else {
        let all = sorted(&top.kids);
        total = all.len();
        entries = all
            .into_iter()
            .skip(offset)
            .take(cap)
            .map(|(name, kid)| entry(dir, name, kid))
            .collect();
    }
    let shown = if deep {
        entries.len()
    } else {
        offset.min(total) + entries.len()
    };
    Listing {
        dir: dir.to_string(),
        entries,
        more: total.saturating_sub(shown),
    }
}

/// What git lists under `dir` in the checkout at `root`, with its marks:
/// one folder from `offset`, or with `deep` everything under it.
pub fn list(root: &Path, dir: &str, deep: bool, offset: usize) -> Res<Listing> {
    let dir = tree_dir(dir)?;
    if !root.is_dir() {
        return Err(format!("{} is not there any more", root.display()));
    }
    // On disk, it must resolve inside the checkout (no symlink out); a
    // folder only the index still has is listed by its words.
    if !dir.is_empty() && root.join(&dir).exists() {
        inside(root, &dir)?;
    }
    let spec: Option<&str> = (!dir.is_empty()).then_some(dir.as_str());
    let mut args = vec!["ls-files", "-s", "-z"];
    if let Some(s) = spec {
        args.extend(["--", s]);
    }
    let mut files = parse_ls_files(&git(root, &args)?);
    // `status` names paths from the repo's root, `ls-files` from here: a
    // checkout that is a folder of its repo cuts its prefix off.
    let prefix = String::from_utf8_lossy(&git(root, &["rev-parse", "--show-prefix"])?)
        .trim()
        .to_string();
    let status: Vec<StatusEntry> = status_entries(root, spec)?
        .into_iter()
        .filter_map(|mut s| {
            s.path = s.path.strip_prefix(&prefix)?.to_string();
            Some(s)
        })
        .collect();
    let tracked: HashSet<&str> = files.iter().map(|f| f.path.as_str()).collect();
    let mut untracked: Vec<Listed> = status
        .iter()
        .filter(|s| s.x == '?' && !tracked.contains(s.path.as_str()))
        .map(|s| match s.path.strip_suffix('/') {
            // A repo inside the checkout: git lists it, not into it.
            Some(p) => Listed {
                path: p.to_string(),
                kind: Kind::Module,
            },
            None => Listed {
                path: s.path.clone(),
                kind: Kind::File,
            },
        })
        .collect();
    files.append(&mut untracked);
    let mut out = listing(
        &dir,
        &files,
        &status,
        deep,
        offset,
        if deep { DEEP_CAP } else { LEVEL_CAP },
    );
    if !deep {
        // One folder's files: their size, and an untracked link seen as one.
        for e in out.entries.iter_mut().filter(|e| e.kind != Kind::Dir) {
            let Ok(meta) = std::fs::symlink_metadata(root.join(&e.path)) else {
                continue;
            };
            if meta.file_type().is_symlink() {
                e.kind = Kind::Link;
            } else if e.kind == Kind::File && meta.is_file() {
                e.size = Some(meta.len());
            }
        }
    }
    Ok(out)
}

/// The Files tree's rows under `dir` in checkout `worktree`: one folder
/// from `offset` (`LEVEL_CAP` at a time), or with `deep` the whole subtree.
#[tauri::command]
pub async fn worktree_tree(
    client: State<'_, Arc<Client>>,
    worktree: String,
    dir: Option<String>,
    deep: Option<bool>,
    offset: Option<usize>,
) -> Res<Listing> {
    let (root, _) = checkout(&client, &worktree)?;
    let dir = dir.unwrap_or_default();
    blocking(move || list(&root, &dir, deep.unwrap_or(false), offset.unwrap_or(0))).await?
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git::tests::Repo;

    fn names(l: &Listing) -> Vec<&str> {
        l.entries.iter().map(|e| e.path.as_str()).collect()
    }

    fn get<'a>(l: &'a Listing, path: &str) -> &'a Entry {
        l.entries
            .iter()
            .find(|e| e.path == path)
            .unwrap_or_else(|| panic!("{path} in {:?}", names(l)))
    }

    /// Tracked, changed, staged, untracked, ignored, deleted and nested.
    fn fixture() -> Repo {
        let r = Repo::new("tree");
        r.write(".gitignore", b"target/\n*.log\n");
        r.write("README.md", b"# demo\n");
        r.write("gone.md", b"bye\n");
        r.write("src/main.rs", b"fn main() {}\n");
        r.write("src/lib/mod.rs", b"pub fn a() {}\n");
        r.write("docs/a b.md", b"spaced\n");
        r.commit("init");
        r.write("src/main.rs", b"fn main() { changed(); }\n");
        r.write("docs/a b.md", b"spaced and staged\n");
        r.git(&["add", "docs/a b.md"]);
        r.write("src/new_mod.rs", b"// new\n");
        r.write("new.txt", b"new\n");
        r.write("target/out.bin", b"\0\0");
        r.write("debug.log", b"noise\n");
        std::fs::remove_file(r.0.join("gone.md")).unwrap();
        r
    }

    #[test]
    fn a_folder_is_named_by_its_words_inside_the_checkout() {
        assert_eq!(tree_dir("").unwrap(), "");
        assert_eq!(tree_dir(".").unwrap(), "");
        assert_eq!(tree_dir("src/").unwrap(), "src");
        assert_eq!(tree_dir("./src/lib").unwrap(), "src/lib");
        assert_eq!(tree_dir("src\\lib").unwrap(), "src/lib");
        for bad in [
            "..",
            "../x",
            "src/../..",
            "a/../b",
            "/etc",
            "\\\\server\\share",
        ] {
            assert!(tree_dir(bad).is_err(), "{bad}");
        }
        #[cfg(windows)]
        for bad in ["C:\\Windows", "C:x", "C:/"] {
            assert!(tree_dir(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn the_root_lists_folders_first_with_marks_and_no_ignored_files() {
        let r = fixture();
        let l = list(&r.0, "", false, 0).unwrap();
        assert_eq!(
            names(&l),
            [
                "docs",
                "src",
                ".gitignore",
                "gone.md",
                "new.txt",
                "README.md"
            ]
        );
        assert_eq!(l.more, 0);
        assert_eq!(get(&l, "docs").kind, Kind::Dir);
        assert!(get(&l, "docs").marks.staged && !get(&l, "docs").marks.changed);
        let src = get(&l, "src").marks;
        assert!(src.changed && src.untracked && !src.staged);
        let gone = get(&l, "gone.md");
        assert!(gone.marks.deleted && gone.marks.changed);
        assert_eq!(gone.size, None);
        assert!(get(&l, "new.txt").marks.untracked);
        assert_eq!(get(&l, "README.md").marks, Marks::default());
        assert_eq!(get(&l, "README.md").size, Some(7));
    }

    #[test]
    fn a_folder_lists_its_own_level() {
        let r = fixture();
        let l = list(&r.0, "src", false, 0).unwrap();
        assert_eq!(l.dir, "src");
        assert_eq!(names(&l), ["src/lib", "src/main.rs", "src/new_mod.rs"]);
        assert!(get(&l, "src/main.rs").marks.changed);
        assert!(get(&l, "src/new_mod.rs").marks.untracked);
        assert_eq!(get(&l, "src/lib").marks, Marks::default());
        let docs = list(&r.0, "docs", false, 0).unwrap();
        let spaced = get(&docs, "docs/a b.md");
        assert!(spaced.marks.staged && !spaced.marks.added);
        // An ignored folder lists nothing.
        assert!(list(&r.0, "target", false, 0).unwrap().entries.is_empty());
    }

    #[test]
    fn deep_lists_the_subtree_depth_first() {
        let r = fixture();
        let l = list(&r.0, "", true, 0).unwrap();
        assert_eq!(
            names(&l),
            [
                "docs",
                "docs/a b.md",
                "src",
                "src/lib",
                "src/lib/mod.rs",
                "src/main.rs",
                "src/new_mod.rs",
                ".gitignore",
                "gone.md",
                "new.txt",
                "README.md"
            ]
        );
        let sub = list(&r.0, "src", true, 0).unwrap();
        assert_eq!(
            names(&sub),
            ["src/lib", "src/lib/mod.rs", "src/main.rs", "src/new_mod.rs"]
        );
    }

    #[test]
    fn a_folder_only_the_index_has_still_opens() {
        let r = Repo::new("tree-gone");
        r.write("old/a.txt", b"a\n");
        r.write("keep.txt", b"k\n");
        r.commit("init");
        std::fs::remove_dir_all(r.0.join("old")).unwrap();
        let top = list(&r.0, "", false, 0).unwrap();
        assert!(get(&top, "old").marks.deleted);
        let l = list(&r.0, "old", false, 0).unwrap();
        assert_eq!(names(&l), ["old/a.txt"]);
        assert!(l.entries[0].marks.deleted);
    }

    #[test]
    fn nothing_outside_the_checkout_is_listed() {
        let r = fixture();
        for bad in ["..", "../..", "src/../..", "/", "/etc"] {
            assert!(list(&r.0, bad, false, 0).is_err(), "{bad}");
        }
        let outside = r.0.parent().unwrap().display().to_string();
        assert!(list(&r.0, &outside, false, 0).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn a_symlinked_folder_out_of_the_checkout_is_refused() {
        let r = fixture();
        let away = tempfile::tempdir().unwrap();
        std::fs::write(away.path().join("secret.txt"), "no").unwrap();
        std::os::unix::fs::symlink(away.path(), r.0.join("away")).unwrap();
        assert!(list(&r.0, "away", false, 0).is_err());
        // The link itself is a row, never a folder to open.
        let top = list(&r.0, "", false, 0).unwrap();
        assert_eq!(get(&top, "away").kind, Kind::Link);
    }

    #[test]
    fn a_checkout_inside_its_repo_lists_from_its_own_root() {
        let r = fixture();
        let l = list(&r.0.join("src"), "", false, 0).unwrap();
        assert_eq!(names(&l), ["lib", "main.rs", "new_mod.rs"]);
        assert!(get(&l, "main.rs").marks.changed);
        assert!(get(&l, "new_mod.rs").marks.untracked);
    }

    fn files(paths: &[&str]) -> Vec<Listed> {
        paths
            .iter()
            .map(|p| Listed {
                path: p.to_string(),
                kind: Kind::File,
            })
            .collect()
    }

    #[test]
    fn a_long_folder_pages_by_the_cap() {
        let f = files(&["a", "b", "c", "d", "e"]);
        let first = listing("", &f, &[], false, 0, 2);
        assert_eq!(names(&first), ["a", "b"]);
        assert_eq!(first.more, 3);
        let next = listing("", &f, &[], false, 2, 2);
        assert_eq!(names(&next), ["c", "d"]);
        assert_eq!(next.more, 1);
        let last = listing("", &f, &[], false, 4, 2);
        assert_eq!((names(&last), last.more), (vec!["e"], 0));
        let deep = listing("", &files(&["x/1", "x/2", "y"]), &[], true, 0, 2);
        assert_eq!(names(&deep), ["x", "x/1"]);
        assert_eq!(deep.more, 2);
    }

    #[test]
    fn ls_files_kinds_and_stages_are_read() {
        let raw = b"100644 aaaa 0\tsrc/a.rs\0120000 bbbb 0\tlink\0160000 cccc 0\tvendor/lib\0100644 dddd 1\tboth.rs\0100644 eeee 2\tboth.rs\0";
        let got = parse_ls_files(raw);
        assert_eq!(
            got,
            vec![
                Listed {
                    path: "src/a.rs".into(),
                    kind: Kind::File
                },
                Listed {
                    path: "link".into(),
                    kind: Kind::Link
                },
                Listed {
                    path: "vendor/lib".into(),
                    kind: Kind::Module
                },
                Listed {
                    path: "both.rs".into(),
                    kind: Kind::File
                },
            ]
        );
    }

    #[test]
    fn conflicts_and_staged_deletions_mark_their_folders() {
        let st = |x: char, y: char, p: &str| StatusEntry {
            x,
            y,
            path: p.into(),
            from: None,
        };
        let f = files(&["a/clash.rs", "a/keep.rs"]);
        let status = [st('U', 'U', "a/clash.rs"), st('D', ' ', "a/removed.rs")];
        let l = listing("", &f, &status, false, 0, 10);
        let a = get(&l, "a").marks;
        assert!(a.conflicted && a.staged);
        let inner = listing("a", &f, &status, false, 0, 10);
        assert_eq!(names(&inner), ["a/clash.rs", "a/keep.rs"]);
        assert!(get(&inner, "a/clash.rs").marks.conflicted);
    }
}
