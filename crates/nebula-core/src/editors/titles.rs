//! Whether an editor window's title names a checkout. A heuristic: a
//! custom `window.title`, or two checkouts whose folders share a name,
//! defeat it, so it drives the "a window is open" dot and which window
//! Dock moves, never which window takes focus.
//!
//! VS Code's default title is `${dirty}${activeEditorShort}${separator}
//! ${rootName}${separator}${profileName}${separator}${appName}`, with
//! `" - "` between the parts and the folder's base name as `rootName`
//! ("actions.ts - lane-editors - Visual Studio Code"). Visual Studio's is
//! the solution's name first ("Api - Microsoft Visual Studio").

use super::Family;
use std::path::Path;

/// The parts of a title between its `" - "` separators, each without a
/// dirty marker or an elevation suffix.
fn parts(title: &str) -> Vec<String> {
    title
        .split(" - ")
        .map(|p| {
            p.trim()
                .trim_start_matches(['\u{25cf}', '*'])
                .trim()
                .trim_end_matches("(Administrator)")
                .trim()
                .to_lowercase()
        })
        .filter(|p| !p.is_empty())
        .collect()
}

/// Whether `title` (a window of a `family` editor) looks open on the
/// checkout at `folder` or one of its solutions (`stems`, the file names
/// without extensions).
pub fn matches(title: &str, family: Family, folder: &Path, stems: &[String]) -> bool {
    let parts = parts(title);
    if parts.is_empty() {
        return false;
    }
    match family {
        Family::Code => {
            let Some(name) = folder
                .file_name()
                .map(|n| n.to_string_lossy().to_lowercase())
            else {
                return false;
            };
            // The last part is the app's name; the root folder is
            // before it, a workspace reads "name (Workspace)".
            let ws = format!("{name} (workspace)");
            parts[..parts.len().saturating_sub(1).max(1)]
                .iter()
                .any(|p| *p == name || *p == ws)
        }
        Family::VisualStudio => {
            let first = &parts[0];
            stems.iter().any(|s| s.to_lowercase() == *first)
        }
    }
}
