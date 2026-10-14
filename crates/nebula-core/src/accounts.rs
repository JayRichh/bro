//! ACCOUNT SPREAD: which profile a launch that names none runs under.
//!
//! `account_spread` in `config.json` (see `nebula-config`'s Sessions tab):
//! - `manual` (default): the harness's default profile, else none — what
//!   kete did before spreads existed.
//! - `failover`: the default until its session (5h) or weekly use reaches
//!   `account_spread_threshold` percent, then the next enabled profile of
//!   that harness under it.
//! - `least_used`: the enabled profile with the least use.
//!
//! A disabled profile is never picked ([`crate::profiles::Profile::disabled`]).
//! An account with no reading yet counts as unused: picking it is how it
//! gets one. One pure function ([`pick`]) serves the daemon (a create
//! with no profile) and both apps (the toast that says what was picked
//! and why).

use serde::Deserialize;

use crate::profiles::{self, Profile};
use crate::usage::AccountUsage;

/// The `config.json` keys.
pub const SPREAD_KEY: &str = "account_spread";
pub const THRESHOLD_KEY: &str = "account_spread_threshold";

/// The spreads, as config.json spells them.
pub const SPREADS: &[&str] = &["manual", "failover", "least_used"];
/// The thresholds offered, percent.
pub const THRESHOLDS: &[&str] = &["50", "70", "80", "90", "95"];
pub const DEFAULT_THRESHOLD: u8 = 90;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Spread {
    #[default]
    Manual,
    Failover,
    LeastUsed,
}

impl Spread {
    pub fn parse(word: &str) -> Spread {
        match word.trim() {
            "failover" => Spread::Failover,
            "least_used" | "least-used" => Spread::LeastUsed,
            _ => Spread::Manual,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Policy {
    pub spread: Spread,
    /// Percent at which `failover` moves on.
    pub threshold: u8,
}

impl Default for Policy {
    fn default() -> Self {
        Self {
            spread: Spread::Manual,
            threshold: DEFAULT_THRESHOLD,
        }
    }
}

impl Policy {
    /// Read from config.json values as written: an odd value is the default.
    pub fn from_words(spread: Option<&str>, threshold: Option<&str>) -> Policy {
        Policy {
            spread: spread.map(Spread::parse).unwrap_or_default(),
            threshold: threshold
                .and_then(|t| t.trim().trim_end_matches('%').parse::<u8>().ok())
                .filter(|t| (1..=100).contains(t))
                .unwrap_or(DEFAULT_THRESHOLD),
        }
    }
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct PolicyOnly {
    account_spread: Option<String>,
    account_spread_threshold: Option<serde_json::Value>,
}

/// The policy `config.json` (with `config.local.json` over it) sets.
pub fn load_policy() -> Policy {
    let loaded = crate::settings::load::<PolicyOnly>(
        &crate::paths::config_path(),
        &crate::paths::config_local_path(),
    );
    let threshold = loaded.value.account_spread_threshold.map(|v| match v {
        serde_json::Value::String(s) => s,
        other => other.to_string(),
    });
    Policy::from_words(loaded.value.account_spread.as_deref(), threshold.as_deref())
}

/// Why [`pick`] chose what it chose.
#[derive(Debug, Clone, PartialEq)]
pub enum Reason {
    /// The launch named the profile.
    Named,
    /// `manual`: the harness's default profile.
    Default,
    /// No usable profile: the CLI's own login.
    NoProfile,
    /// `failover`: the default (or first) is under the threshold.
    UnderThreshold { percent: Option<f32> },
    /// `failover`: moved past accounts at or over the threshold.
    FailedOver { from: String, percent: f32 },
    /// `failover`: every account is over; the least used one.
    AllOver { percent: f32 },
    /// `least_used`.
    LeastUsed { percent: Option<f32> },
}

/// A spread's choice.
#[derive(Debug, Clone, PartialEq)]
pub struct Pick<'a> {
    pub profile: Option<&'a Profile>,
    pub reason: Reason,
}

impl Pick<'_> {
    /// One line for a toast or the status bar: `claude: work (default)`,
    /// `claude: home (failover: work at 92%)`.
    pub fn describe(&self, harness: &str) -> String {
        let who = self
            .profile
            .map_or("its own login".to_string(), |p| p.name.trim().to_string());
        let why = match &self.reason {
            Reason::Named => "picked".to_string(),
            Reason::Default => "default".to_string(),
            Reason::NoProfile => "no profile".to_string(),
            Reason::UnderThreshold { percent } => {
                format!("failover, {} used", crate::usage::pct_text(*percent))
            }
            Reason::FailedOver { from, percent } => {
                format!(
                    "failover: {from} at {}",
                    crate::usage::pct_text(Some(*percent))
                )
            }
            Reason::AllOver { percent } => format!(
                "every account over its threshold; least used, {}",
                crate::usage::pct_text(Some(*percent))
            ),
            Reason::LeastUsed { percent } => {
                format!("least used, {}", crate::usage::pct_text(*percent))
            }
        };
        format!("{harness}: {who} ({why})")
    }
}

/// The profile a launch on `harness` naming none runs under. `usage`
/// looks an account's latest reading up by profile name; `now_secs`
/// empties windows past their reset.
pub fn pick<'a>(
    all: &'a [Profile],
    harness: &str,
    policy: Policy,
    now_secs: i64,
    usage: impl Fn(&str) -> Option<AccountUsage>,
) -> Pick<'a> {
    if policy.spread == Spread::Manual {
        return match profiles::default_for(all, harness) {
            Some(p) => Pick {
                profile: Some(p),
                reason: Reason::Default,
            },
            None => Pick {
                profile: None,
                reason: Reason::NoProfile,
            },
        };
    }
    // The default first, then config order.
    let mut candidates = profiles::enabled_for(all, harness);
    if let Some(at) = candidates.iter().position(|p| p.default) {
        let d = candidates.remove(at);
        candidates.insert(0, d);
    }
    if candidates.is_empty() {
        return Pick {
            profile: None,
            reason: Reason::NoProfile,
        };
    }
    let used: Vec<Option<f32>> = candidates
        .iter()
        .map(|p| usage(p.name.trim()).and_then(|u| u.worst_at(now_secs)))
        .collect();
    let least = || {
        let mut best = 0;
        for i in 1..candidates.len() {
            if used[i].unwrap_or(0.0) < used[best].unwrap_or(0.0) {
                best = i;
            }
        }
        best
    };
    match policy.spread {
        Spread::Manual => unreachable!("handled above"),
        Spread::LeastUsed => {
            let i = least();
            Pick {
                profile: Some(candidates[i]),
                reason: Reason::LeastUsed { percent: used[i] },
            }
        }
        Spread::Failover => {
            let limit = policy.threshold as f32;
            match used.iter().position(|u| u.unwrap_or(0.0) < limit) {
                Some(0) => Pick {
                    profile: Some(candidates[0]),
                    reason: Reason::UnderThreshold { percent: used[0] },
                },
                Some(i) => Pick {
                    profile: Some(candidates[i]),
                    reason: Reason::FailedOver {
                        from: candidates[0].name.trim().to_string(),
                        percent: used[0].unwrap_or(0.0),
                    },
                },
                None => {
                    let i = least();
                    Pick {
                        profile: Some(candidates[i]),
                        reason: Reason::AllOver {
                            percent: used[i].unwrap_or(0.0),
                        },
                    }
                }
            }
        }
    }
}

/// Whether an account has a login, as far as its files say. Only the
/// existence of a credentials file (and, for Claude on macOS, where the
/// keychain holds it, the account entry in `.claude.json`) is looked at:
/// nothing in them is read out.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LoginState {
    LoggedIn,
    NeedsLogin,
    /// A harness kete knows no sign-in files for.
    Unknown,
}

/// The config dir `harness` keeps its login in under `profile` (its home
/// dir, or the CLI's default for an adopted login or no profile).
fn login_dir(
    profile: Option<&Profile>,
    harness: &str,
    root: &std::path::Path,
) -> Option<std::path::PathBuf> {
    use crate::env;
    let var = match harness.trim() {
        "claude" => env::CLAUDE_CONFIG_DIR,
        "codex" => env::CODEX_HOME,
        "copilot" => profiles::GH_CONFIG_DIR,
        _ => return None,
    };
    if let Some(dir) = profile.and_then(|p| p.dir_for(root, var)) {
        return Some(dir);
    }
    if let Some(dir) = env::non_empty(var) {
        return Some(std::path::PathBuf::from(dir));
    }
    let home = env::home_dir()?;
    Some(match harness.trim() {
        "claude" => home.join(".claude"),
        "codex" => home.join(".codex"),
        _ => {
            // gh: %APPDATA%\GitHub CLI on Windows, ~/.config/gh elsewhere.
            #[cfg(windows)]
            {
                std::path::PathBuf::from(std::env::var_os("APPDATA")?).join("GitHub CLI")
            }
            #[cfg(not(windows))]
            {
                home.join(".config").join("gh")
            }
        }
    })
}

/// [`LoginState`] of `profile` (None = the harness's own default login).
pub fn login_state_in(
    profile: Option<&Profile>,
    harness: &str,
    root: &std::path::Path,
) -> LoginState {
    let Some(dir) = login_dir(profile, harness, root) else {
        return LoginState::Unknown;
    };
    let found = match harness.trim() {
        "claude" => {
            dir.join(".credentials.json").is_file() || {
                // `.claude.json` sits in the config dir for a moved one,
                // beside `~/.claude` for the default.
                let json = if profile.is_some_and(|p| !p.use_default)
                    || crate::env::non_empty(crate::env::CLAUDE_CONFIG_DIR).is_some()
                {
                    dir.join(".claude.json")
                } else {
                    dir.with_file_name(".claude.json")
                };
                std::fs::read_to_string(json)
                    .ok()
                    .and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok())
                    .is_some_and(|v| v.get("oauthAccount").is_some_and(|a| a.is_object()))
            }
        }
        "codex" => dir.join("auth.json").is_file(),
        _ => dir.join("hosts.yml").is_file(),
    };
    if found {
        LoginState::LoggedIn
    } else {
        LoginState::NeedsLogin
    }
}

/// [`login_state_in`] under the profiles root.
pub fn login_state(profile: Option<&Profile>, harness: &str) -> LoginState {
    login_state_in(profile, harness, &profiles::profiles_root())
}

/// [`pick`] against the kept readings, now.
pub fn pick_now<'a>(all: &'a [Profile], harness: &str, policy: Policy) -> Pick<'a> {
    pick(
        all,
        harness,
        policy,
        crate::clock::now_secs() as i64,
        crate::usage::read_account,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::usage::{UsageSource, UsageWindow, WindowKind};
    use std::collections::BTreeMap;

    fn profile(name: &str, default: bool) -> Profile {
        Profile {
            name: name.into(),
            harness: "claude".into(),
            env: BTreeMap::new(),
            default,
            use_default: false,
            disabled: false,
        }
    }

    fn usage(map: &[(&str, f32, f32)]) -> impl Fn(&str) -> Option<AccountUsage> {
        let map: Vec<(String, f32, f32)> = map
            .iter()
            .map(|(n, s, w)| (n.to_string(), *s, *w))
            .collect();
        move |name: &str| {
            map.iter()
                .find(|(n, _, _)| n == name)
                .map(|(n, s, w)| AccountUsage {
                    account: n.clone(),
                    harness: "claude".into(),
                    windows: vec![
                        UsageWindow {
                            kind: WindowKind::Session,
                            label: None,
                            percent: *s,
                            resets_at: None,
                            read_at: 1,
                        },
                        UsageWindow {
                            kind: WindowKind::Weekly,
                            label: None,
                            percent: *w,
                            resets_at: Some(1000),
                            read_at: 1,
                        },
                    ],
                    source: UsageSource::Statusline,
                    at: 0,
                    plan: None,
                })
        }
    }

    fn policy(spread: Spread) -> Policy {
        Policy {
            spread,
            threshold: 90,
        }
    }

    #[test]
    fn manual_is_todays_behaviour() {
        let all = vec![profile("a", false), profile("b", true)];
        let p = pick(
            &all,
            "claude",
            policy(Spread::Manual),
            0,
            usage(&[("b", 99.0, 99.0)]),
        );
        assert_eq!(
            p.profile.unwrap().name,
            "b",
            "the default, whatever its use"
        );
        assert_eq!(p.reason, Reason::Default);
        let none = vec![profile("a", false)];
        let p = pick(&none, "claude", policy(Spread::Manual), 0, usage(&[]));
        assert_eq!(p.profile, None, "no default flagged: no profile");
        assert_eq!(p.reason, Reason::NoProfile);
        assert_eq!(p.describe("claude"), "claude: its own login (no profile)");
    }

    #[test]
    fn failover_moves_on_at_the_threshold_and_skips_disabled() {
        let mut off = profile("off", false);
        off.disabled = true;
        let all = vec![
            profile("a", false),
            off,
            profile("main", true),
            profile("c", false),
        ];
        let u = usage(&[
            ("main", 20.0, 95.0),
            ("off", 0.0, 0.0),
            ("a", 91.0, 10.0),
            ("c", 5.0, 5.0),
        ]);
        let p = pick(&all, "claude", policy(Spread::Failover), 0, &u);
        assert_eq!(
            p.profile.unwrap().name,
            "c",
            "main over on weekly, a over on 5h"
        );
        assert_eq!(
            p.reason,
            Reason::FailedOver {
                from: "main".into(),
                percent: 95.0
            }
        );
        assert_eq!(p.describe("claude"), "claude: c (failover: main at 95%)");
        // Under the threshold: the default stays.
        let u = usage(&[("main", 89.0, 10.0)]);
        let p = pick(&all, "claude", policy(Spread::Failover), 0, &u);
        assert_eq!(p.profile.unwrap().name, "main");
        // A window past its reset no longer counts.
        let u = usage(&[("main", 0.0, 99.0)]);
        let p = pick(&all, "claude", policy(Spread::Failover), 2000, &u);
        assert_eq!(p.profile.unwrap().name, "main");
    }

    /// A reading with no reset time that has aged past its window no longer
    /// holds an account back: failover comes back to the default.
    #[test]
    fn failover_forgets_an_expired_reading() {
        let all = vec![profile("main", true), profile("b", false)];
        let u = usage(&[("main", 99.0, 99.0)]);
        let p = pick(&all, "claude", policy(Spread::Failover), 1 + 5 * 3600, &u);
        assert_eq!(p.profile.unwrap().name, "main");
        let p = pick(&all, "claude", policy(Spread::Failover), 2, &u);
        assert_eq!(p.profile.unwrap().name, "b", "fresh: still over");
    }

    #[test]
    fn failover_with_every_account_over_takes_the_least_used() {
        let all = vec![profile("a", true), profile("b", false)];
        let u = usage(&[("a", 99.0, 0.0), ("b", 92.0, 0.0)]);
        let p = pick(&all, "claude", policy(Spread::Failover), 0, &u);
        assert_eq!(p.profile.unwrap().name, "b");
        assert_eq!(p.reason, Reason::AllOver { percent: 92.0 });
    }

    #[test]
    fn least_used_prefers_the_unmeasured_and_then_the_default_on_ties() {
        let all = vec![profile("a", false), profile("b", true), profile("c", false)];
        let u = usage(&[("a", 10.0, 30.0), ("b", 40.0, 0.0)]);
        let p = pick(&all, "claude", policy(Spread::LeastUsed), 0, &u);
        assert_eq!(
            p.profile.unwrap().name,
            "c",
            "no reading yet counts as unused"
        );
        let u = usage(&[("a", 0.0, 0.0), ("b", 0.0, 0.0), ("c", 0.0, 0.0)]);
        let p = pick(&all, "claude", policy(Spread::LeastUsed), 0, &u);
        assert_eq!(p.profile.unwrap().name, "b");
        let p = pick(&[], "claude", policy(Spread::LeastUsed), 0, &u);
        assert_eq!(p.profile, None);
    }

    #[test]
    fn login_state_looks_only_for_the_credentials_files() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let work = profile("work", false);
        assert_eq!(
            login_state_in(Some(&work), "claude", root),
            LoginState::NeedsLogin
        );
        let claude = root.join("work").join("claude");
        std::fs::create_dir_all(&claude).unwrap();
        std::fs::write(
            claude.join(".claude.json"),
            r#"{"hasCompletedOnboarding":true}"#,
        )
        .unwrap();
        assert_eq!(
            login_state_in(Some(&work), "claude", root),
            LoginState::NeedsLogin
        );
        std::fs::write(claude.join(".claude.json"), r#"{"oauthAccount":{}}"#).unwrap();
        assert_eq!(
            login_state_in(Some(&work), "claude", root),
            LoginState::LoggedIn
        );
        let mut codex = profile("cx", false);
        codex.harness = "codex".into();
        assert_eq!(
            login_state_in(Some(&codex), "codex", root),
            LoginState::NeedsLogin
        );
        std::fs::create_dir_all(root.join("cx").join("codex")).unwrap();
        std::fs::write(root.join("cx").join("codex").join("auth.json"), "{}").unwrap();
        assert_eq!(
            login_state_in(Some(&codex), "codex", root),
            LoginState::LoggedIn
        );
        assert_eq!(login_state_in(None, "aider", root), LoginState::Unknown);
    }

    #[test]
    fn policy_words_parse_and_odd_ones_read_as_the_default() {
        assert_eq!(
            Policy::from_words(Some("failover"), Some("80")),
            Policy {
                spread: Spread::Failover,
                threshold: 80
            }
        );
        assert_eq!(
            Policy::from_words(Some("nope"), Some("200")),
            Policy::default()
        );
        assert_eq!(Policy::from_words(None, Some("95%")).threshold, 95);
        assert_eq!(Spread::parse("least_used"), Spread::LeastUsed);
    }
}
