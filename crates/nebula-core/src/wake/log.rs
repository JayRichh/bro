//! The WAKE LOG: `<data>/wake-log.jsonl`, one JSON line per decision —
//! a heartbeat fired, skipped and why, or failed — appended under a lock
//! and read back by `kete wake status` and both apps. It is how a
//! heartbeat that didn't move the window shows: `resets_before` and
//! `resets_after` are the account's five-hour reset times either side of
//! it. Kept at about [`MAX_BYTES`]: past that the file moves to `.1`
//! (replacing the older one) and a new one starts.

use std::io::Write;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::WakeMode;

/// Rotate past this size.
pub const MAX_BYTES: u64 = 1_000_000;

/// What became of a heartbeat.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Outcome {
    Fired,
    Skipped,
    Failed,
}

impl Outcome {
    pub fn as_str(self) -> &'static str {
        match self {
            Outcome::Fired => "fired",
            Outcome::Skipped => "skipped",
            Outcome::Failed => "failed",
        }
    }
}

/// One line of the log.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Entry {
    /// Epoch seconds.
    pub at: i64,
    pub account: String,
    pub decision: Outcome,
    /// A short code: `on_time`, `catch_up`, `forced` for a fire; the
    /// [`super::Skip`] code for a skip; why for a failure (`timeout`,
    /// `no_prompt`, `old_daemon`, …).
    pub reason: String,
    #[serde(default)]
    pub mode: WakeMode,
    /// The five-hour window's reset (epoch seconds) before the heartbeat.
    #[serde(default)]
    pub resets_before: Option<i64>,
    /// …and after it: later than before means it started a window.
    #[serde(default)]
    pub resets_after: Option<i64>,
    /// A line for a person.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    /// The slot (epoch seconds) the decision was for: what keeps a slot
    /// to one heartbeat.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub slot: Option<i64>,
    /// Asked for by hand (`kete wake <account> --force`, Wake now).
    #[serde(default, skip_serializing_if = "is_false")]
    pub forced: bool,
}

fn is_false(value: &bool) -> bool {
    !value
}

impl Entry {
    /// Did a fired heartbeat start a window? None while it can't be told
    /// (no reading after it, or not a fire).
    pub fn started_window(&self) -> Option<bool> {
        if self.decision != Outcome::Fired {
            return None;
        }
        let after = self.resets_after?;
        Some(match self.resets_before {
            Some(before) => after > before,
            None => true,
        })
    }

    /// `05:02 fired · window to 10:02`, `05:00 skipped: a window is
    /// already live` — one line, local time by `offset_secs`.
    pub fn summary(&self, offset_secs: i32) -> String {
        let at = super::hhmm(super::LocalTime::at(self.at, offset_secs).minute);
        let mut line = format!("{at} {}", self.decision.as_str());
        match self.decision {
            Outcome::Fired => match (self.started_window(), self.resets_after) {
                (Some(true), Some(after)) => {
                    let until = super::LocalTime::at(after, offset_secs).minute;
                    line.push_str(&format!(" \u{b7} window to {}", super::hhmm(until)));
                }
                (Some(false), _) => line.push_str(" \u{b7} didn't start a window"),
                _ => line.push_str(" \u{b7} no reading yet"),
            },
            Outcome::Skipped | Outcome::Failed => {
                let why = self.detail.as_deref().unwrap_or(&self.reason);
                line.push_str(&format!(": {why}"));
            }
        }
        line
    }
}

/// The rotated file beside `path`.
fn older(path: &Path) -> PathBuf {
    let mut name = path
        .file_name()
        .map(|n| n.to_os_string())
        .unwrap_or_default();
    name.push(".1");
    path.with_file_name(name)
}

/// Append `entry` to the log at `path`, under its lock, rotating first
/// when the line would take the file past [`MAX_BYTES`].
pub fn append_in(path: &Path, entry: &Entry) -> std::io::Result<()> {
    let mut line = serde_json::to_vec(entry)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    line.push(b'\n');
    let _lock = crate::settings::lock(path)?;
    let size = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
    if size > 0 && size + line.len() as u64 > MAX_BYTES {
        let old = older(path);
        let _ = std::fs::remove_file(&old);
        std::fs::rename(path, &old)?;
    }
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?;
    file.write_all(&line)
}

/// [`append_in`] on this machine's log.
pub fn append(entry: &Entry) -> std::io::Result<()> {
    append_in(&super::log_path(), entry)
}

/// Every entry under `path`, oldest first: the rotated file, then the
/// current one. A line this build can't read is passed over.
pub fn read_in(path: &Path) -> Vec<Entry> {
    [older(path), path.to_path_buf()]
        .iter()
        .filter_map(|p| std::fs::read_to_string(p).ok())
        .flat_map(|text| {
            text.lines()
                .filter(|l| !l.trim().is_empty())
                .filter_map(|l| serde_json::from_str::<Entry>(l).ok())
                .collect::<Vec<_>>()
        })
        .collect()
}

/// The last `n` entries under `path` — `account`'s alone when one is
/// named — newest first.
pub fn recent_in(path: &Path, account: Option<&str>, n: usize) -> Vec<Entry> {
    read_in(path)
        .into_iter()
        .rev()
        .filter(|e| account.is_none_or(|a| e.account == a.trim()))
        .take(n)
        .collect()
}

/// [`recent_in`] on this machine's log.
pub fn recent(account: Option<&str>, n: usize) -> Vec<Entry> {
    recent_in(&super::log_path(), account, n)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(at: i64, account: &str, decision: Outcome) -> Entry {
        Entry {
            at,
            account: account.into(),
            decision,
            reason: "on_time".into(),
            mode: WakeMode::Interactive,
            resets_before: None,
            resets_after: None,
            detail: None,
            slot: Some(at),
            forced: false,
        }
    }

    #[test]
    fn entries_append_and_read_back_newest_first_per_account() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("wake-log.jsonl");
        append_in(&path, &entry(1, "work", Outcome::Fired)).unwrap();
        append_in(&path, &entry(2, "home", Outcome::Skipped)).unwrap();
        append_in(&path, &entry(3, "work", Outcome::Failed)).unwrap();
        let work = recent_in(&path, Some("work"), 10);
        assert_eq!(work.iter().map(|e| e.at).collect::<Vec<_>>(), vec![3, 1]);
        assert_eq!(recent_in(&path, None, 2).len(), 2);
        assert_eq!(recent_in(&path, None, 2)[0].at, 3);
        assert!(
            !dir.path().join("wake-log.jsonl.lock").exists(),
            "lock let go"
        );
    }

    #[test]
    fn a_line_it_cant_read_is_passed_over() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("wake-log.jsonl");
        append_in(&path, &entry(1, "work", Outcome::Fired)).unwrap();
        let mut text = std::fs::read_to_string(&path).unwrap();
        text.push_str("not json\n{\"at\": 2}\n");
        std::fs::write(&path, text).unwrap();
        append_in(&path, &entry(3, "work", Outcome::Fired)).unwrap();
        assert_eq!(read_in(&path).len(), 2);
    }

    #[test]
    fn the_log_rotates_past_its_cap_and_reads_both_files() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("wake-log.jsonl");
        let mut big = entry(0, "work", Outcome::Skipped);
        big.detail = Some("x".repeat(400_000));
        for at in 0..3 {
            big.at = at;
            append_in(&path, &big).unwrap();
        }
        assert!(older(&path).exists(), "rotated once");
        assert!(std::fs::metadata(&path).unwrap().len() <= MAX_BYTES);
        let all = read_in(&path);
        assert_eq!(all.iter().map(|e| e.at).collect::<Vec<_>>(), vec![0, 1, 2]);
        // A second rotation drops the oldest file.
        for at in 3..6 {
            big.at = at;
            append_in(&path, &big).unwrap();
        }
        assert!(read_in(&path).iter().all(|e| e.at >= 2));
    }

    #[test]
    fn a_fire_reads_as_starting_a_window_only_when_the_reset_moved() {
        let mut e = entry(0, "work", Outcome::Fired);
        assert_eq!(e.started_window(), None, "no reading after");
        e.resets_after = Some(100);
        assert_eq!(e.started_window(), Some(true), "nothing before");
        e.resets_before = Some(100);
        assert_eq!(e.started_window(), Some(false));
        assert!(e.summary(0).contains("didn't start a window"));
        e.resets_before = Some(50);
        e.at = 5 * 3600;
        e.resets_after = Some(10 * 3600);
        assert_eq!(e.summary(0), "05:00 fired \u{b7} window to 10:00");
        let mut s = entry(5 * 3600, "work", Outcome::Skipped);
        s.reason = "live_window".into();
        s.detail = Some("a window is live until 07:00".into());
        assert_eq!(s.summary(0), "05:00 skipped: a window is live until 07:00");
        assert_eq!(s.started_window(), None);
    }

    #[test]
    fn older_entries_without_newer_fields_still_read() {
        let e: Entry = serde_json::from_str(
            r#"{"at": 1, "account": "a", "decision": "fired", "reason": "on_time"}"#,
        )
        .unwrap();
        assert_eq!(e.mode, WakeMode::Interactive);
        assert_eq!(e.slot, None);
        let text = serde_json::to_string(&e).unwrap();
        assert!(!text.contains("forced"), "{text}");
    }
}
