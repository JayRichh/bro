//! `kete open <file>…` run inside a session: the daemon's `FilesOpened`,
//! which the terminal app raises as its FILE TABS (crates/nebula-tui/src/
//! file_tabs.rs). The window shows the same files in a read-only sheet,
//! one tab per file. Only files a session opened can be read or handed to
//! the system's default app from here: the sheet is not a general file
//! reader for the web view.

use serde::Serialize;
use std::collections::HashSet;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use tauri::State;
use tauri_plugin_opener::OpenerExt;

use crate::Res;

/// What a preview reads at most, as the terminal app's tree browser.
const MAX_BYTES: usize = 2 * 1024 * 1024;
const MAX_LINES: usize = 10_000;
/// A tab label's widest, in characters, as the terminal app's.
const MAX_LABEL_CHARS: usize = 28;

/// Every path a session has opened this run: the only ones served.
#[derive(Default)]
pub struct OpenedFiles(Mutex<HashSet<PathBuf>>);

impl OpenedFiles {
    pub fn allow(&self, paths: &[PathBuf]) {
        self.0
            .lock()
            .expect("opened lock")
            .extend(paths.iter().cloned());
    }

    /// `path`, when a session opened it: the editors' verbs take these too.
    pub(crate) fn check_path(&self, path: &str) -> Res<PathBuf> {
        self.check(path)
    }

    fn check(&self, path: &str) -> Res<PathBuf> {
        let p = PathBuf::from(path);
        if self.0.lock().expect("opened lock").contains(&p) {
            Ok(p)
        } else {
            Err("only files a session opened can be read here".into())
        }
    }
}

/// One tab of the sheet.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct OpenedFile {
    pub path: String,
    pub label: String,
}

/// The `kete://files-opened` payload.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct FilesOpened {
    pub root: String,
    pub files: Vec<OpenedFile>,
}

pub fn opened(root: &Path, paths: &[PathBuf]) -> FilesOpened {
    FilesOpened {
        root: root.display().to_string(),
        files: paths
            .iter()
            .map(|p| OpenedFile {
                path: p.display().to_string(),
                label: label_for(root, p),
            })
            .collect(),
    }
}

/// A tab's label: the path relative to the checkout (just the file name
/// outside it), cut from the left past [`MAX_LABEL_CHARS`] so the tail,
/// which tells `a/README.md` from `b/README.md`, stays.
fn label_for(root: &Path, path: &Path) -> String {
    let shown = match path.strip_prefix(root) {
        Ok(rel) if !rel.as_os_str().is_empty() => rel.to_string_lossy().into_owned(),
        _ => path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| path.to_string_lossy().into_owned()),
    };
    let count = shown.chars().count();
    if count <= MAX_LABEL_CHARS {
        return shown;
    }
    let tail: String = shown.chars().skip(count - (MAX_LABEL_CHARS - 1)).collect();
    format!("\u{2026}{tail}")
}

/// A file's text for the sheet, or why there is none to show.
#[derive(Debug, Clone, Serialize, PartialEq, Default)]
pub struct FileText {
    pub text: Option<String>,
    pub note: Option<String>,
}

fn note(s: impl Into<String>) -> FileText {
    FileText {
        text: None,
        note: Some(s.into()),
    }
}

/// Read `path` capped and binary-guarded, as the terminal app's preview.
pub(crate) fn read_text(path: &Path) -> FileText {
    let file = match std::fs::File::open(path) {
        Ok(f) => f,
        Err(e) => return note(format!("Couldn't read the file: {e}")),
    };
    let mut bytes = Vec::new();
    if let Err(e) = file.take((MAX_BYTES + 1) as u64).read_to_end(&mut bytes) {
        return note(format!("Couldn't read the file: {e}"));
    }
    if bytes[..bytes.len().min(8192)].contains(&0) {
        return note("A binary file: open it with its own app.");
    }
    let capped = bytes.len() > MAX_BYTES;
    let text = String::from_utf8_lossy(&bytes[..bytes.len().min(MAX_BYTES)]);
    if text.trim().is_empty() {
        return note("The file is empty.");
    }
    let lines = text.lines().count();
    let mut out: String = if lines > MAX_LINES {
        text.lines().take(MAX_LINES).collect::<Vec<_>>().join("\n")
    } else {
        text.into_owned()
    };
    let cut = capped || lines > MAX_LINES;
    if cut {
        out.push_str("\n\u{2026} (cut short: open it with its own app for the rest)");
    }
    FileText {
        text: Some(out),
        note: cut.then(|| "Showing the start of a large file.".into()),
    }
}

#[tauri::command]
pub async fn opened_file_read(opened: State<'_, OpenedFiles>, path: String) -> Res<FileText> {
    let p = opened.check(&path)?;
    tauri::async_runtime::spawn_blocking(move || read_text(&p))
        .await
        .map_err(|e| e.to_string())
}

/// Hand an opened file to the system's default app for its kind.
#[tauri::command]
pub fn opened_file_open(
    app: tauri::AppHandle,
    opened: State<'_, OpenedFiles>,
    path: String,
) -> Res<()> {
    let p = opened.check(&path)?;
    // An agent can `kete open` anything: a script's default app runs it.
    if nebula_core::editors::runs_program(&p) {
        return Err(nebula_core::editors::RUNS_PROGRAM.into());
    }
    app.opener()
        .open_path(p.display().to_string(), None::<&str>)
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn labels_are_checkout_relative_and_keep_the_tail() {
        let root = Path::new("/w/demo");
        assert_eq!(
            label_for(root, Path::new("/w/demo/src/main.rs")),
            "src/main.rs"
        );
        assert_eq!(
            label_for(root, Path::new("/elsewhere/notes.md")),
            "notes.md"
        );
        let long = label_for(
            root,
            Path::new("/w/demo/a_very_long_directory_name/deeper/README.md"),
        );
        assert_eq!(long.chars().count(), MAX_LABEL_CHARS);
        assert!(long.starts_with('\u{2026}') && long.ends_with("README.md"));
    }

    #[test]
    fn only_opened_paths_are_served() {
        let opened = OpenedFiles::default();
        let dir = tempfile::tempdir().unwrap();
        let f = dir.path().join("plan.md");
        std::fs::write(&f, "# Plan\n").unwrap();
        let path = f.display().to_string();
        assert!(opened.check(&path).is_err());
        opened.allow(std::slice::from_ref(&f));
        assert_eq!(opened.check(&path).unwrap(), f);
        assert!(opened
            .check(&dir.path().join("other.md").display().to_string())
            .is_err());
    }

    #[test]
    fn reads_text_and_names_what_it_cannot_show() {
        let dir = tempfile::tempdir().unwrap();
        let text = dir.path().join("a.txt");
        std::fs::write(&text, "one\ntwo\n").unwrap();
        assert_eq!(read_text(&text).text.as_deref(), Some("one\ntwo\n"));
        assert_eq!(read_text(&text).note, None);

        let bin = dir.path().join("a.bin");
        std::fs::write(&bin, [0x89, b'P', 0, 1]).unwrap();
        assert!(read_text(&bin).text.is_none());
        assert!(read_text(&bin).note.unwrap().contains("binary"));

        let empty = dir.path().join("e.txt");
        std::fs::write(&empty, "  \n").unwrap();
        assert_eq!(
            read_text(&empty).note.as_deref(),
            Some("The file is empty.")
        );

        let long = dir.path().join("long.txt");
        std::fs::write(&long, "x\n".repeat(MAX_LINES + 5)).unwrap();
        let got = read_text(&long);
        assert_eq!(got.text.unwrap().lines().count(), MAX_LINES + 1);
        assert!(got.note.is_some());

        assert!(read_text(&dir.path().join("missing"))
            .note
            .unwrap()
            .starts_with("Couldn't read"));
    }
}
