//! The rail's version plate and its update cue: is a newer Kete published
//! on GitHub than this window? The same check as the terminal app's footer
//! (crates/nebula-tui/src/update_check.rs), kept here rather than shared
//! because the window's copy must not flash a console: one `HEAD` of
//! GitHub's `releases/latest`, whose redirect names the newest release tag
//! (drafts and pre-releases excluded) without spending any API quota.
//!
//! A nudge, not a clock: a check at start and then on a slow beat,
//! `NEBULA_UPDATE_CHECK_SECS` overriding the beat (`0` turns it off), and
//! every failure is "no news". A recheck that can't ask never clears a cue
//! an earlier one lit. Nothing here installs anything; `kete upgrade` does.

use serde::Serialize;
use std::sync::Mutex;
use std::time::Duration;
use tauri::{Emitter, Manager};

/// GitHub's "latest release" page for this repo, the TUI's too.
pub const LATEST_URL: &str = "https://github.com/JayRichh/kete/releases/latest";

/// How often the check re-runs once the window is up.
const DEFAULT_INTERVAL: Duration = Duration::from_secs(60 * 60);

/// How long the probe may take before it is given up on.
const TIMEOUT: Duration = Duration::from_secs(20);

/// The newest release seen this run, when it is newer than this build.
#[derive(Default)]
pub struct UpdateState(Mutex<Option<String>>);

/// What the plate shows.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct UpdateStatus {
    /// This build, `0.46.0`.
    pub version: String,
    /// A newer published version, when there is one.
    pub latest: Option<String>,
    /// The release page to read about it.
    pub url: String,
    /// What updates it.
    pub command: String,
}

fn status(latest: Option<String>) -> UpdateStatus {
    let url = match &latest {
        Some(v) => format!("https://github.com/JayRichh/kete/releases/tag/v{v}"),
        None => LATEST_URL.to_string(),
    };
    UpdateStatus {
        version: env!("CARGO_PKG_VERSION").to_string(),
        latest,
        url,
        command: "kete upgrade".into(),
    }
}

#[tauri::command]
pub fn update_status(state: tauri::State<'_, UpdateState>) -> UpdateStatus {
    status(state.0.lock().expect("update lock").clone())
}

/// The check's cadence: the env override when set, `None` when it is `0`.
fn interval_from(var: Option<String>) -> Option<Duration> {
    match var {
        None => Some(DEFAULT_INTERVAL),
        Some(v) => match v.trim().parse::<u64>() {
            Ok(0) => None,
            Ok(secs) => Some(Duration::from_secs(secs)),
            Err(_) => Some(DEFAULT_INTERVAL),
        },
    }
}

/// Check now and on every beat after, for the life of the app; tell the
/// window (`kete://update`) when a newer release turns up.
pub async fn run(app: tauri::AppHandle) {
    let Some(every) = interval_from(nebula_core::env::non_empty(
        nebula_core::env::UPDATE_CHECK_SECS,
    )) else {
        return;
    };
    loop {
        if let Some(v) = newer_release().await {
            let state = app.state::<UpdateState>();
            let mut seen = state.0.lock().expect("update lock");
            if seen.as_deref() != Some(v.as_str()) {
                *seen = Some(v.clone());
                drop(seen);
                let _ = app.emit("kete://update", status(Some(v)));
            }
        }
        tokio::time::sleep(every).await;
    }
}

/// The latest published version when it is strictly newer than this build.
async fn newer_release() -> Option<String> {
    let redirect = tauri::async_runtime::spawn_blocking(|| probe(LATEST_URL))
        .await
        .ok()??;
    newer_than(tag_from_redirect(&redirect)?, env!("CARGO_PKG_VERSION"))
}

/// Where `url` redirects to, without following it. `curl`, which
/// `install.ps1` and `kete upgrade` already need, with no console window.
fn probe(url: &str) -> Option<String> {
    let null = if cfg!(windows) { "NUL" } else { "/dev/null" };
    let mut cmd = std::process::Command::new("curl");
    cmd.args(["-sS", "-I", "-o", null, "-w", "%{redirect_url}"])
        .args(["--max-time", &TIMEOUT.as_secs().to_string(), url])
        .stdin(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    #[cfg(windows)]
    nebula_core::platform::no_window(&mut cmd);
    let out = cmd.output().ok()?;
    if !out.status.success() {
        return None;
    }
    let redirect = String::from_utf8_lossy(&out.stdout).trim().to_string();
    (!redirect.is_empty()).then_some(redirect)
}

/// The tag a `…/releases/tag/<tag>` URL names.
fn tag_from_redirect(url: &str) -> Option<&str> {
    let (_, tag) = url.split_once("/releases/tag/")?;
    let tag = tag.trim_end_matches('/');
    (!tag.is_empty()).then_some(tag)
}

/// `vX.Y.Z` / `X.Y.Z` as three numbers; a pre-release never compares.
fn parse_version(s: &str) -> Option<(u64, u64, u64)> {
    let s = s.trim();
    let s = s.strip_prefix('v').unwrap_or(s);
    let mut parts = s.split('.').map(|p| p.parse::<u64>().ok());
    let v = (parts.next()??, parts.next()??, parts.next()??);
    parts.next().is_none().then_some(v)
}

/// `published` as `X.Y.Z` when it is strictly newer than `running`.
fn newer_than(published: &str, running: &str) -> Option<String> {
    let p = parse_version(published)?;
    let r = parse_version(running)?;
    (p > r).then(|| format!("{}.{}.{}", p.0, p.1, p.2))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_redirect_names_the_tag() {
        let base = "https://github.com/JayRichh/kete";
        assert_eq!(
            tag_from_redirect(&format!("{base}/releases/tag/v0.47.0/")),
            Some("v0.47.0")
        );
        assert_eq!(tag_from_redirect(&format!("{base}/releases")), None);
        assert_eq!(tag_from_redirect(""), None);
    }

    #[test]
    fn only_a_strictly_newer_release_counts() {
        assert_eq!(newer_than("v0.47.0", "0.46.0").as_deref(), Some("0.47.0"));
        assert_eq!(newer_than("v0.46.10", "0.46.9").as_deref(), Some("0.46.10"));
        assert_eq!(newer_than("v0.46.0", "0.46.0"), None, "up to date");
        assert_eq!(newer_than("v0.45.0", "0.46.0"), None, "a dev build ahead");
        assert_eq!(newer_than("v0.47.0-rc1", "0.46.0"), None, "pre-release");
        assert_eq!(newer_than("latest", "0.46.0"), None);
    }

    #[test]
    fn the_beat_reads_the_override() {
        assert_eq!(interval_from(None), Some(DEFAULT_INTERVAL));
        assert_eq!(interval_from(Some("0".into())), None, "0 turns it off");
        assert_eq!(
            interval_from(Some("90".into())),
            Some(Duration::from_secs(90))
        );
        assert_eq!(interval_from(Some("soon".into())), Some(DEFAULT_INTERVAL));
    }

    #[test]
    fn the_plate_links_the_release_it_names() {
        let s = status(Some("0.47.0".into()));
        assert_eq!(s.version, env!("CARGO_PKG_VERSION"));
        assert_eq!(
            s.url,
            "https://github.com/JayRichh/kete/releases/tag/v0.47.0"
        );
        assert_eq!(s.command, "kete upgrade");
        assert_eq!(status(None).url, LATEST_URL);
    }
}
