//! A worktree's changes against HEAD, for the diff view, and its pull
//! request, for the kanban's PR open lane. Both shell out (`git`, `gh`)
//! with no console window, as the daemon's own helpers do.

use serde::Serialize;
use std::path::Path;
use std::process::{Command, Stdio};

/// Files above this are shown as too large rather than diffed.
const MAX_FILE_BYTES: u64 = 2 * 1024 * 1024;

#[derive(Debug, Clone, Serialize)]
pub struct DiffFile {
    pub path: String,
    /// Where a rename came from.
    pub from: Option<String>,
    /// `M`odified, `A`dded, `D`eleted, `R`enamed, `U`ntracked, `C`onflicted.
    pub status: char,
    pub added: Option<u64>,
    pub removed: Option<u64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DiffPair {
    pub original: String,
    pub modified: String,
    /// Why there is no text to compare (binary, too large), if so.
    pub note: Option<String>,
}

fn command(program: &str, cwd: &Path) -> Command {
    let mut cmd = Command::new(program);
    cmd.current_dir(cwd)
        .stdin(Stdio::null())
        .stderr(Stdio::null());
    #[cfg(windows)]
    nebula_core::platform::no_window(&mut cmd);
    cmd
}

fn git(cwd: &Path, args: &[&str]) -> Result<Vec<u8>, String> {
    let out = command("git", cwd)
        .args(["-c", "core.quotepath=off"])
        .args(args)
        .output()
        .map_err(|e| format!("could not run git: {e}"))?;
    if !out.status.success() {
        return Err(format!("git {} failed", args.first().unwrap_or(&"")));
    }
    Ok(out.stdout)
}

/// Every changed path in the checkout: staged, unstaged and untracked, each
/// once, with line counts against HEAD where git has them.
pub fn changed_files(root: &Path) -> Result<Vec<DiffFile>, String> {
    let status = git(
        root,
        &["status", "--porcelain=v1", "-z", "--untracked-files=all"],
    )?;
    let mut files = Vec::new();
    let mut parts = status.split(|b| *b == 0).filter(|p| !p.is_empty());
    while let Some(entry) = parts.next() {
        if entry.len() < 4 {
            continue;
        }
        let (x, y) = (entry[0] as char, entry[1] as char);
        let path = String::from_utf8_lossy(&entry[3..]).into_owned();
        let renamed = x == 'R' || y == 'R' || x == 'C';
        let from = if renamed {
            parts
                .next()
                .map(|p| String::from_utf8_lossy(p).into_owned())
        } else {
            None
        };
        let status = match (x, y) {
            ('?', '?') => 'U',
            ('U', _) | (_, 'U') | ('A', 'A') | ('D', 'D') => 'C',
            _ if renamed => 'R',
            ('A', _) => 'A',
            ('D', _) | (_, 'D') => 'D',
            _ => 'M',
        };
        files.push(DiffFile {
            path,
            from,
            status,
            added: None,
            removed: None,
        });
    }
    // Line counts for tracked changes; a repo with no commit yet has none.
    if let Ok(numstat) = git(root, &["diff", "HEAD", "--numstat", "-z", "-M"]) {
        apply_numstat(&mut files, &numstat);
    }
    for f in files.iter_mut().filter(|f| f.status == 'U') {
        if let Ok(text) = std::fs::read(root.join(&f.path)) {
            if !is_binary(&text) {
                f.added = Some(text.split(|b| *b == b'\n').count() as u64);
                f.removed = Some(0);
            }
        }
    }
    files.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(files)
}

/// `git diff --numstat -z`: `add\tdel\tpath\0`, or for a rename
/// `add\tdel\t\0from\0to\0`. Binary files count `-`.
fn apply_numstat(files: &mut [DiffFile], raw: &[u8]) {
    let mut parts = raw.split(|b| *b == 0);
    while let Some(head) = parts.next() {
        if head.is_empty() {
            continue;
        }
        let text = String::from_utf8_lossy(head);
        let mut cols = text.splitn(3, '\t');
        let (Some(a), Some(d), Some(path)) = (cols.next(), cols.next(), cols.next()) else {
            continue;
        };
        let path = if path.is_empty() {
            let _from = parts.next();
            parts
                .next()
                .map(|p| String::from_utf8_lossy(p).into_owned())
                .unwrap_or_default()
        } else {
            path.to_string()
        };
        if let Some(f) = files.iter_mut().find(|f| f.path == path) {
            f.added = a.parse().ok();
            f.removed = d.parse().ok();
        }
    }
}

fn is_binary(bytes: &[u8]) -> bool {
    bytes.iter().take(8000).any(|b| *b == 0)
}

/// HEAD's side and the disk's side of one changed file.
pub fn file_pair(root: &Path, file: &DiffFile) -> Result<DiffPair, String> {
    let head_path = file.from.as_deref().unwrap_or(&file.path);
    let original = if matches!(file.status, 'A' | 'U') {
        Vec::new()
    } else {
        git(root, &["show", &format!("HEAD:{head_path}")]).unwrap_or_default()
    };
    let disk = root.join(&file.path);
    let modified = if file.status == 'D' {
        Vec::new()
    } else {
        match std::fs::metadata(&disk) {
            Ok(m) if m.len() > MAX_FILE_BYTES => {
                return Ok(DiffPair {
                    original: String::new(),
                    modified: String::new(),
                    note: Some(format!("{} KB, too large to diff here", m.len() / 1024)),
                })
            }
            _ => std::fs::read(&disk).unwrap_or_default(),
        }
    };
    if is_binary(&original) || is_binary(&modified) {
        return Ok(DiffPair {
            original: String::new(),
            modified: String::new(),
            note: Some("Binary file".into()),
        });
    }
    Ok(DiffPair {
        original: String::from_utf8_lossy(&original).into_owned(),
        modified: String::from_utf8_lossy(&modified).into_owned(),
        note: None,
    })
}

/// The open pull request for `branch`, asked of `gh` in the checkout.
/// None when there is none, or no `gh`, or no network.
pub fn open_pr(root: &Path, branch: &str) -> Option<crate::daemon::PrInfo> {
    let out = command("gh", root)
        .args([
            "pr",
            "view",
            branch,
            "--json",
            "number,url,title,state,isDraft",
        ])
        .stdout(Stdio::piped())
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).ok()?;
    if v.get("state")?.as_str()? != "OPEN" {
        return None;
    }
    Some(crate::daemon::PrInfo {
        number: v.get("number")?.as_u64()?,
        url: v.get("url")?.as_str()?.to_string(),
        title: v.get("title")?.as_str().unwrap_or_default().to_string(),
        draft: v.get("isDraft").and_then(|d| d.as_bool()).unwrap_or(false),
    })
}

/// Whether `gh` is on PATH at all, so the lookups are skipped cleanly.
pub fn have_gh() -> bool {
    let mut cmd = Command::new("gh");
    cmd.arg("--version")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    #[cfg(windows)]
    nebula_core::platform::no_window(&mut cmd);
    cmd.status().map(|s| s.success()).unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numstat_fills_plain_and_renamed_entries() {
        let mut files = vec![
            DiffFile {
                path: "a.rs".into(),
                from: None,
                status: 'M',
                added: None,
                removed: None,
            },
            DiffFile {
                path: "new.rs".into(),
                from: Some("old.rs".into()),
                status: 'R',
                added: None,
                removed: None,
            },
        ];
        apply_numstat(&mut files, b"3\t1\ta.rs\x002\t0\t\x00old.rs\x00new.rs\x00");
        assert_eq!((files[0].added, files[0].removed), (Some(3), Some(1)));
        assert_eq!((files[1].added, files[1].removed), (Some(2), Some(0)));
    }
}
