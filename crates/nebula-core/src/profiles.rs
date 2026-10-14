//! PROFILES: several accounts per harness.
//!
//! A PROFILE is a named account for one harness — a second Claude Max
//! subscription, a work ChatGPT login for Codex, another GitHub account for
//! Copilot. Each lives in `config.json`'s `profiles` list ([`Profile`]) and,
//! unless it adopts the CLI's own default login ([`Profile::use_default`]),
//! owns a PROFILE HOME under `<data dir>/profiles/<name>/` holding the
//! harness's config dirs: `claude/` (`CLAUDE_CONFIG_DIR`), `codex/`
//! (`CODEX_HOME`), `copilot/` (`COPILOT_HOME`) with `gh/` beside it
//! (`GH_CONFIG_DIR`). A session spawned under a profile gets those dirs in
//! its environment and every inherited credential variable stripped
//! ([`Profile::launch_env`]), so two subscriptions of one harness run side
//! by side without either seeing the other's login.
//!
//! The session keeps the profile's name (`Agent::profile`), so a RESUME or
//! a restart boots under the same account. A launch that names no profile
//! takes its harness's default one ([`resolve`]) — the profile flagged
//! `default`, else none at all: the CLI runs on whatever the daemon's own
//! environment says, as before profiles existed.
//!
//! Design after agent-office's per-account homes (`src/server/signins.ts`,
//! MIT): the same isolating variables, the same stripped credentials, and
//! the same seeded `.claude.json` that marks onboarding done and carries
//! the default config's folder trust over, so a fresh profile's first
//! session asks for a login and nothing else.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

/// The `config.json` key holding the list.
pub const CONFIG_KEY: &str = "profiles";

/// One PROFILE: a named account for one harness.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Profile {
    /// Unique across every harness: it names the PROFILE HOME directory
    /// and is what a session stores. Letters, digits, `-` and `_`.
    pub name: String,
    /// The harness id it is an account for (`claude`, `codex`, `copilot`,
    /// or a `harnesses` entry's id). Which config dirs it isolates follows
    /// from the id: a harness with none of its own gets only `env`.
    pub harness: String,
    /// Extra variables set on the profile's sessions after the isolating
    /// ones — an API key for an API-billed account, a proxy.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub env: BTreeMap<String, String>,
    /// The profile a launch on this harness takes when it names none. At
    /// most one per harness counts: the first flagged wins.
    #[serde(default, skip_serializing_if = "is_false")]
    pub default: bool,
    /// Adopt the CLI's own default login (`~/.claude`, `~/.codex`, …)
    /// instead of a PROFILE HOME: no dir variable is set, nothing is
    /// copied, and only inherited credential variables are stripped — so
    /// the existing login is one of the profiles a picker cycles through.
    #[serde(default, skip_serializing_if = "is_false")]
    pub use_default: bool,
    /// Switched off in the accounts list: never picked for a launch that
    /// names no profile (no default, no spread), and left out of the
    /// pickers. A session already running under it keeps it.
    #[serde(default, skip_serializing_if = "is_false")]
    pub disabled: bool,
}

fn is_false(value: &bool) -> bool {
    !value
}

/// A config dir a harness keeps its login in, and the variable that
/// moves it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HomeDir {
    /// Sub-directory of the PROFILE HOME.
    pub dir: &'static str,
    /// The variable pointing the CLI at it.
    pub var: &'static str,
}

/// The dirs a harness's PROFILE HOME holds. Copilot keeps `gh/` beside its
/// own home: its sessions push and open PRs as the account they run as.
pub fn home_dirs(harness: &str) -> &'static [HomeDir] {
    match harness.trim() {
        "claude" => &[HomeDir {
            dir: "claude",
            var: crate::env::CLAUDE_CONFIG_DIR,
        }],
        "codex" => &[HomeDir {
            dir: "codex",
            var: crate::env::CODEX_HOME,
        }],
        "copilot" => &[
            HomeDir {
                dir: "copilot",
                var: crate::env::COPILOT_HOME,
            },
            HomeDir {
                dir: "gh",
                var: GH_CONFIG_DIR,
            },
        ],
        _ => &[],
    }
}

/// The GitHub CLI's config-dir override.
pub const GH_CONFIG_DIR: &str = "GH_CONFIG_DIR";

/// Inherited variables that would override a profile's login, per
/// harness: an API key or token in the daemon's environment wins over the
/// credentials file in any config dir, so a profile's sessions never see
/// them (its `env` can set one back on purpose). The dir variables are
/// listed too: an isolated profile replaces them, an adopted one keeps
/// what the environment says (see [`Profile::launch_env`]).
pub fn credential_vars(harness: &str) -> &'static [&'static str] {
    match harness.trim() {
        "claude" => &[
            "ANTHROPIC_API_KEY",
            "ANTHROPIC_AUTH_TOKEN",
            "CLAUDE_CODE_OAUTH_TOKEN",
            "CLAUDE_CODE_OAUTH_REFRESH_TOKEN",
            "CLAUDE_CODE_OAUTH_TOKEN_FILE_DESCRIPTOR",
            "CLAUDE_SECURESTORAGE_CONFIG_DIR",
        ],
        "codex" => &["OPENAI_API_KEY", "CODEX_API_KEY"],
        "copilot" => &["COPILOT_GITHUB_TOKEN", "GH_TOKEN", "GITHUB_TOKEN"],
        _ => &[],
    }
}

/// What a profile does to a session's environment: variables to remove,
/// then variables to set (in order — `env` overrides come last).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LaunchEnv {
    pub unset: Vec<String>,
    pub set: Vec<(String, String)>,
}

impl LaunchEnv {
    /// `unset` with the names `set` assigns taken out: removing a variable
    /// and then setting it is the set alone.
    pub fn removals(&self) -> Vec<String> {
        self.unset
            .iter()
            .filter(|name| !self.set.iter().any(|(set, _)| set == *name))
            .cloned()
            .collect()
    }
}

/// `<data dir>/profiles` — every PROFILE HOME is a directory in here.
pub fn profiles_root() -> PathBuf {
    crate::paths::data_dir().join("profiles")
}

impl Profile {
    /// Why this entry is unusable, or None.
    pub fn problem(&self) -> Option<String> {
        let name = self.name.trim();
        if name.is_empty() {
            return Some("profile with an empty name".into());
        }
        if name.len() > 40
            || !name
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        {
            return Some(format!(
                "profile `{name}`: use letters, digits, `-` and `_` (40 at most)"
            ));
        }
        if self.harness.trim().is_empty() {
            return Some(format!("profile `{name}` names no harness"));
        }
        None
    }

    /// The PROFILE HOME under `root` (normally [`profiles_root`]), or None
    /// for an adopted default login, which has none.
    pub fn home_in(&self, root: &Path) -> Option<PathBuf> {
        (!self.use_default).then(|| root.join(self.name.trim()))
    }

    /// The PROFILE HOME, or None for an adopted default login.
    pub fn home(&self) -> Option<PathBuf> {
        self.home_in(&profiles_root())
    }

    /// Where this profile's harness keeps its config, by variable — the
    /// PROFILE HOME's sub-directories. Empty for an adopted login.
    pub fn dirs_in(&self, root: &Path) -> Vec<(&'static str, PathBuf)> {
        let Some(home) = self.home_in(root) else {
            return Vec::new();
        };
        home_dirs(&self.harness)
            .iter()
            .map(|d| (d.var, home.join(d.dir)))
            .collect()
    }

    /// The dir a variable points at under this profile, when the profile
    /// moves it.
    pub fn dir_for(&self, root: &Path, var: &str) -> Option<PathBuf> {
        self.dirs_in(root)
            .into_iter()
            .find(|(v, _)| *v == var)
            .map(|(_, dir)| dir)
    }

    /// The environment edit a session under this profile gets: every
    /// inherited credential variable of its harness removed; an isolated
    /// profile's dir variables pointed into its PROFILE HOME (an adopted
    /// one leaves them as the environment has them); then `env`.
    pub fn launch_env_in(&self, root: &Path) -> LaunchEnv {
        let mut out = LaunchEnv::default();
        out.unset
            .extend(credential_vars(&self.harness).iter().map(|v| v.to_string()));
        for (var, dir) in self.dirs_in(root) {
            out.set
                .push((var.to_string(), dir.to_string_lossy().into_owned()));
        }
        for (key, value) in &self.env {
            if !key.trim().is_empty() {
                out.set.push((key.trim().to_string(), value.clone()));
            }
        }
        out
    }

    /// [`Profile::launch_env_in`] under [`profiles_root`].
    pub fn launch_env(&self) -> LaunchEnv {
        self.launch_env_in(&profiles_root())
    }

    /// Make the PROFILE HOME's dirs exist, and seed a Claude profile's
    /// `.claude.json` so its first session goes straight to the login:
    /// onboarding marked done, and folder trust for `worktree` carried
    /// over from the default config when the default trusts it (or a
    /// folder above it). A no-op for an adopted login. Writes only when
    /// something changed.
    pub fn prepare_in(&self, root: &Path, worktree: Option<&Path>) -> std::io::Result<()> {
        for (_, dir) in self.dirs_in(root) {
            std::fs::create_dir_all(&dir)?;
        }
        if let Some(config_dir) = self.dir_for(root, crate::env::CLAUDE_CONFIG_DIR) {
            seed_claude_config(&config_dir, default_claude_json().as_deref(), worktree)?;
        }
        Ok(())
    }

    /// [`Profile::prepare_in`] under [`profiles_root`].
    pub fn prepare(&self, worktree: Option<&Path>) -> std::io::Result<()> {
        self.prepare_in(&profiles_root(), worktree)
    }
}

/// The profile a launch on `harness` runs under: `name` when one was
/// picked (it must exist and be an account for that harness), else the
/// harness's default profile, else none. `Err` says why a named profile
/// cannot be used.
pub fn resolve<'a>(
    all: &'a [Profile],
    harness: &str,
    name: Option<&str>,
) -> Result<Option<&'a Profile>, String> {
    let harness = harness.trim();
    match name.map(str::trim).filter(|n| !n.is_empty()) {
        Some(name) => {
            let Some(profile) = all.iter().find(|p| p.name.trim() == name) else {
                return Err(format!("profile `{name}` is no longer defined"));
            };
            if let Some(problem) = profile.problem() {
                return Err(problem);
            }
            if profile.harness.trim() != harness {
                return Err(format!(
                    "profile `{name}` is a {} account, not {harness}",
                    profile.harness.trim()
                ));
            }
            Ok(Some(profile))
        }
        None => Ok(default_for(all, harness)),
    }
}

/// The harness's own sign-in, as its argv after the program — what
/// `kete profile login` runs. None for a harness kete knows no sign-in for.
pub fn login_args(harness: &str) -> Option<&'static [&'static str]> {
    match harness.trim() {
        // The subscription flow, as agent-office signs accounts in: Claude
        // opens the browser and takes the callback, or prints the URL and
        // asks for the code when it cannot.
        "claude" => Some(&["auth", "login", "--claudeai"]),
        "codex" => Some(&["login"]),
        "copilot" => Some(&["login"]),
        _ => None,
    }
}

/// The command that signs `profile` in, for a front end to show (never to
/// run with credentials of its own): None for an adopted default login,
/// which signs in as the CLI always does, or a harness with no known
/// sign-in.
pub fn login_hint(profile: &Profile) -> Option<String> {
    if profile.use_default || login_args(&profile.harness).is_none() {
        return None;
    }
    Some(format!("kete profile login {}", profile.name.trim()))
}

/// The harness's default profile: the first usable one flagged `default`.
/// A disabled default is no default: a launch naming no profile runs on
/// the CLI's own login, as when none is flagged.
pub fn default_for<'a>(all: &'a [Profile], harness: &str) -> Option<&'a Profile> {
    all.iter()
        .find(|p| p.default && p.harness.trim() == harness.trim() && p.problem().is_none())
        .filter(|p| !p.disabled)
}

/// Every usable, enabled profile for `harness`, in config order: what a
/// picker offers and a spread chooses from.
pub fn enabled_for<'a>(all: &'a [Profile], harness: &str) -> Vec<&'a Profile> {
    for_harness(all, harness)
        .into_iter()
        .filter(|p| !p.disabled)
        .collect()
}

/// Switch `name` on or off in the settings file, in place.
pub fn set_enabled_in_file(path: &Path, name: &str, enabled: bool) -> Result<(), String> {
    edit_file(path, |entries| {
        let Some(entry) = entries
            .iter_mut()
            .find(|e| e.get("name").and_then(Value::as_str) == Some(name.trim()))
            .and_then(Value::as_object_mut)
        else {
            return Err(format!("no profile named `{}`", name.trim()));
        };
        if enabled {
            entry.remove("disabled");
        } else {
            entry.insert("disabled".into(), json!(true));
        }
        Ok(())
    })
}

/// Every usable profile for `harness`, in config order.
pub fn for_harness<'a>(all: &'a [Profile], harness: &str) -> Vec<&'a Profile> {
    all.iter()
        .filter(|p| p.harness.trim() == harness.trim() && p.problem().is_none())
        .collect()
}

/// The `profiles` list of a raw settings object, entry by entry: an entry
/// this build cannot read is skipped, not fatal.
pub fn from_settings(obj: &serde_json::Map<String, Value>) -> Vec<Profile> {
    obj.get(CONFIG_KEY)
        .and_then(Value::as_array)
        .map(|entries| {
            entries
                .iter()
                .filter_map(|e| serde_json::from_value(e.clone()).ok())
                .collect()
        })
        .unwrap_or_default()
}

/// The profiles `config.json` (with `config.local.json` over it) defines.
pub fn load() -> Vec<Profile> {
    let loaded = crate::settings::load::<ProfilesOnly>(
        &crate::paths::config_path(),
        &crate::paths::config_local_path(),
    );
    loaded.value.profiles
}

/// The one key [`load`] reads.
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct ProfilesOnly {
    profiles: Vec<Profile>,
}

/// Add `profile` to the settings file at `path`, keeping every other key
/// and every entry as written. Refuses a duplicate name. A `default` entry
/// takes the flag off the harness's other entries.
pub fn add_to_file(path: &Path, profile: &Profile) -> Result<(), String> {
    if let Some(problem) = profile.problem() {
        return Err(problem);
    }
    edit_file(path, |entries| {
        if entries
            .iter()
            .any(|e| e.get("name").and_then(Value::as_str) == Some(profile.name.trim()))
        {
            return Err(format!("profile `{}` already exists", profile.name.trim()));
        }
        if profile.default {
            clear_default(entries, &profile.harness);
        }
        entries.push(serde_json::to_value(profile).map_err(|e| e.to_string())?);
        Ok(())
    })
}

/// Rewrite the entry named `profile.name` in the settings file at `path`,
/// in place: its position and any field this build has no name for stay,
/// the known fields take `profile`'s values. The name itself is what is
/// looked up (it names the PROFILE HOME), so it never changes here. A
/// `default` entry takes the flag off the harness's other entries.
pub fn update_in_file(path: &Path, profile: &Profile) -> Result<(), String> {
    if let Some(problem) = profile.problem() {
        return Err(problem);
    }
    let name = profile.name.trim();
    edit_file(path, |entries| {
        let Some(at) = entries
            .iter()
            .position(|e| e.get("name").and_then(Value::as_str) == Some(name))
        else {
            return Err(format!("no profile named `{name}`"));
        };
        if profile.default {
            clear_default(entries, &profile.harness);
        }
        let Value::Object(known) = serde_json::to_value(profile).map_err(|e| e.to_string())? else {
            return Err("a profile serializes to an object".into());
        };
        let mut entry = entries[at].as_object().cloned().unwrap_or_default();
        // Fields left out when empty or off must go, not linger.
        for key in [
            "name",
            "harness",
            "env",
            "default",
            "use_default",
            "disabled",
        ] {
            entry.remove(key);
        }
        entry.extend(known);
        entries[at] = Value::Object(entry);
        Ok(())
    })
}

/// Remove the profile named `name` from the settings file at `path`;
/// `Ok(false)` when there was none.
pub fn remove_from_file(path: &Path, name: &str) -> Result<bool, String> {
    let mut removed = false;
    edit_file(path, |entries| {
        let before = entries.len();
        entries.retain(|e| e.get("name").and_then(Value::as_str) != Some(name.trim()));
        removed = entries.len() != before;
        Ok(())
    })?;
    Ok(removed)
}

/// Make `name` its harness's default profile in the settings file.
pub fn set_default_in_file(path: &Path, name: &str) -> Result<(), String> {
    edit_file(path, |entries| {
        let Some(harness) = entries
            .iter()
            .find(|e| e.get("name").and_then(Value::as_str) == Some(name.trim()))
            .and_then(|e| e.get("harness").and_then(Value::as_str))
            .map(str::to_string)
        else {
            return Err(format!("no profile named `{}`", name.trim()));
        };
        clear_default(entries, &harness);
        for entry in entries.iter_mut() {
            if entry.get("name").and_then(Value::as_str) == Some(name.trim()) {
                if let Some(obj) = entry.as_object_mut() {
                    obj.insert("default".into(), json!(true));
                }
            }
        }
        Ok(())
    })
}

fn clear_default(entries: &mut [Value], harness: &str) {
    for entry in entries.iter_mut() {
        if entry.get("harness").and_then(Value::as_str).map(str::trim) == Some(harness.trim()) {
            if let Some(obj) = entry.as_object_mut() {
                obj.remove("default");
            }
        }
    }
}

/// Read the settings object at `path` (missing = empty), let `edit` change
/// its `profiles` list, and write it back atomically, under the settings
/// lock the TUI's and the window's saves take. A file that is there but
/// can't be parsed is an error, never replaced.
fn edit_file(
    path: &Path,
    edit: impl FnOnce(&mut Vec<Value>) -> Result<(), String>,
) -> Result<(), String> {
    let _lock = crate::settings::lock(path).map_err(|e| e.to_string())?;
    let mut obj = crate::settings::read_object(path)
        .map_err(|e| format!("{}: {e}", path.display()))?
        .unwrap_or_default();
    let mut entries = match obj.remove(CONFIG_KEY) {
        None => Vec::new(),
        Some(Value::Array(entries)) => entries,
        Some(_) => return Err(format!("{}: `profiles` is not a list", path.display())),
    };
    edit(&mut entries)?;
    obj.insert(CONFIG_KEY.into(), Value::Array(entries));
    crate::settings::write_json(path, &Value::Object(obj))
        .map_err(|e| format!("write {}: {e}", path.display()))
}

/// The latest PLAN USAGE a profile's sessions printed — Claude's
/// `You've used 91% of your weekly limit · resets 2pm` — read off a
/// session's screen by the daemon (`pty::usage`) and kept per profile, so
/// the QUICK PROMPT's profile field and `kete profile list` can say how
/// much of each account is left before you pick one.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Usage {
    /// Percent of the limit used.
    pub percent: u8,
    /// Which limit: `weekly`, `5-hour`, `Opus weekly` — as printed.
    pub window: String,
    /// When it resets, as printed (`2pm`, `Oct 14 at 9am`); None when the
    /// line said nothing.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resets: Option<String>,
    /// Epoch ms it was read.
    #[serde(default)]
    pub at: i64,
}

impl Usage {
    /// `91% weekly · resets 2pm`.
    pub fn summary(&self) -> String {
        let mut out = format!("{}% {}", self.percent, self.window);
        if let Some(resets) = &self.resets {
            out.push_str(" · resets ");
            out.push_str(resets);
        }
        out
    }

    /// The usage a line of a session's screen reports, when it is Claude's
    /// usage notice: `…used <n>% of your <window> limit[ · resets <when>]`.
    /// The screen text arrives with its escape sequences already taken
    /// out; anything else reads as None.
    pub fn parse(line: &str) -> Option<Usage> {
        let at = line.find("used ")?;
        let rest = &line[at + "used ".len()..];
        let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
        if digits.is_empty() || digits.len() > 3 {
            return None;
        }
        let percent: u16 = digits.parse().ok()?;
        let rest = rest[digits.len()..].strip_prefix("% of your ")?;
        let end = rest.find(" limit")?;
        let window = rest[..end].trim();
        if window.is_empty() || window.len() > 32 {
            return None;
        }
        let tail = &rest[end + " limit".len()..];
        let resets = tail
            .find("resets ")
            .map(|i| &tail[i + "resets ".len()..])
            .map(|r| {
                r.split(['\u{b7}', '(', ')', '|', '\r', '\n'])
                    .next()
                    .unwrap_or("")
                    .trim()
                    .trim_end_matches('.')
                    .to_string()
            })
            .filter(|r| !r.is_empty() && r.len() <= 40);
        Some(Usage {
            percent: percent.min(100) as u8,
            window: window.to_string(),
            resets,
            at: 0,
        })
    }
}

/// Where a profile's latest [`Usage`] is kept: `<profiles>/.usage/<name>.json`
/// — beside the homes rather than in one, so an adopted login has a place
/// too.
pub fn usage_path_in(root: &Path, name: &str) -> PathBuf {
    root.join(".usage").join(format!("{}.json", name.trim()))
}

/// The latest usage `profile`'s sessions printed, if any has.
pub fn read_usage(profile: &Profile) -> Option<Usage> {
    read_usage_in(&profiles_root(), &profile.name)
}

/// [`read_usage`] under `root`.
pub fn read_usage_in(root: &Path, name: &str) -> Option<Usage> {
    let text = std::fs::read_to_string(usage_path_in(root, name)).ok()?;
    serde_json::from_str(&text).ok()
}

/// Record `usage` as profile `name`'s latest.
pub fn write_usage_in(root: &Path, name: &str, usage: &Usage) -> std::io::Result<()> {
    let value = serde_json::to_value(usage)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    crate::settings::write_json(&usage_path_in(root, name), &value)
}

/// The default Claude config's `.claude.json`: in `$CLAUDE_CONFIG_DIR`
/// when the environment moves it, else in the home directory (Claude keeps
/// it beside `~/.claude`, not in it).
fn default_claude_json() -> Option<PathBuf> {
    match crate::env::non_empty(crate::env::CLAUDE_CONFIG_DIR) {
        Some(dir) => Some(PathBuf::from(dir).join(".claude.json")),
        None => crate::env::home_dir().map(|home| home.join(".claude.json")),
    }
}

/// The key Claude files a folder under in `.claude.json`'s `projects`:
/// the path with `/` separators (`C:/Code/app` on Windows).
fn claude_project_key(path: &Path) -> String {
    let text = path.to_string_lossy();
    let text = text.strip_prefix(r"\\?\").unwrap_or(&text);
    text.replace('\\', "/")
}

/// Do two `.claude.json` project keys name the same folder? Windows keys
/// differ in drive-letter (and other) case for one folder.
fn same_project_key(a: &str, b: &str) -> bool {
    if cfg!(windows) {
        a.eq_ignore_ascii_case(b)
    } else {
        a == b
    }
}

/// Seed `<config_dir>/.claude.json`: `hasCompletedOnboarding`, and folder
/// trust for `worktree` and each folder above it that `default_json`
/// trusts. Leaves the file untouched when nothing would change, and
/// refuses to touch one that is not a JSON object.
pub fn seed_claude_config(
    config_dir: &Path,
    default_json: Option<&Path>,
    worktree: Option<&Path>,
) -> std::io::Result<()> {
    let file = config_dir.join(".claude.json");
    let mut root = match std::fs::read_to_string(&file) {
        Ok(text) if text.trim().is_empty() => json!({}),
        Ok(text) => match serde_json::from_str::<Value>(&text) {
            Ok(v) if v.is_object() => v,
            // Claude's own file, unreadable to us: never clobber it.
            _ => return Ok(()),
        },
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => json!({}),
        Err(e) => return Err(e),
    };
    let before = root.clone();
    let obj = root.as_object_mut().expect("checked an object above");
    obj.insert("hasCompletedOnboarding".into(), json!(true));

    let trusted: Vec<String> = match (worktree, default_json) {
        (Some(worktree), Some(default_json)) => {
            let default = std::fs::read_to_string(default_json)
                .ok()
                .and_then(|t| serde_json::from_str::<Value>(&t).ok());
            let projects = default
                .as_ref()
                .and_then(|d| d.get("projects"))
                .and_then(Value::as_object);
            worktree
                .ancestors()
                .map(claude_project_key)
                .filter(|key| {
                    projects.is_some_and(|projects| {
                        projects.iter().any(|(k, v)| {
                            same_project_key(k, key)
                                && v.get("hasTrustDialogAccepted").and_then(Value::as_bool)
                                    == Some(true)
                        })
                    })
                })
                .collect()
        }
        _ => Vec::new(),
    };
    if !trusted.is_empty() {
        let projects = obj
            .entry("projects")
            .or_insert_with(|| json!({}))
            .as_object_mut();
        if let Some(projects) = projects {
            for key in trusted {
                let entry = projects.entry(key).or_insert_with(|| json!({}));
                if let Some(entry) = entry.as_object_mut() {
                    entry.insert("hasTrustDialogAccepted".into(), json!(true));
                }
            }
        }
    }
    if root == before {
        return Ok(());
    }
    std::fs::create_dir_all(config_dir)?;
    crate::settings::write_json(&file, &root)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn profile(name: &str, harness: &str) -> Profile {
        Profile {
            name: name.into(),
            harness: harness.into(),
            env: BTreeMap::new(),
            default: false,
            use_default: false,
            disabled: false,
        }
    }

    #[test]
    fn an_update_rewrites_the_entry_in_place_and_keeps_unknown_fields() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.json");
        std::fs::write(
            &path,
            r#"{"theme": "ocean", "profiles": [
                {"name": "work", "harness": "claude", "default": true, "env": {"A": "1"}, "colour": "red"},
                {"name": "home", "harness": "claude"}
            ]}"#,
        )
        .unwrap();
        let mut home = profile("home", "claude");
        home.default = true;
        update_in_file(&path, &home).unwrap();
        let mut work = profile("work", "claude");
        work.use_default = true;
        update_in_file(&path, &work).unwrap();
        let saved: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(saved["theme"], "ocean");
        let list = saved["profiles"].as_array().unwrap();
        assert_eq!(list[0]["name"], "work", "position kept");
        assert_eq!(list[0]["colour"], "red", "unknown field kept");
        assert_eq!(list[0]["use_default"], true);
        assert!(list[0].get("env").is_none(), "an emptied env goes");
        assert!(
            list[0].get("default").is_none(),
            "the default moved to home"
        );
        assert_eq!(list[1]["default"], true);
        assert!(update_in_file(&path, &profile("nope", "claude")).is_err());
        assert!(update_in_file(&path, &profile("bad name", "claude")).is_err());
    }

    #[test]
    fn a_login_hint_names_the_cli_command_only_where_one_exists() {
        assert_eq!(
            login_hint(&profile("work", "claude")).as_deref(),
            Some("kete profile login work")
        );
        let mut adopted = profile("main", "codex");
        adopted.use_default = true;
        assert_eq!(login_hint(&adopted), None);
        assert_eq!(login_hint(&profile("x", "cursor")), None);
        assert_eq!(login_args("copilot"), Some(&["login"][..]));
    }

    #[test]
    fn an_isolated_profile_strips_credentials_and_points_its_dirs_home() {
        let root = Path::new("/data/profiles");
        let mut work = profile("work", "claude");
        work.env.insert("HTTPS_PROXY".into(), "http://proxy".into());
        let env = work.launch_env_in(root);
        assert!(env.unset.contains(&"ANTHROPIC_API_KEY".to_string()));
        assert!(env.unset.contains(&"CLAUDE_CODE_OAUTH_TOKEN".to_string()));
        assert_eq!(
            env.set,
            vec![
                (
                    "CLAUDE_CONFIG_DIR".to_string(),
                    root.join("work")
                        .join("claude")
                        .to_string_lossy()
                        .into_owned()
                ),
                ("HTTPS_PROXY".to_string(), "http://proxy".to_string()),
            ]
        );

        let copilot = profile("gh2", "copilot").launch_env_in(root);
        let vars: Vec<&str> = copilot.set.iter().map(|(k, _)| k.as_str()).collect();
        assert_eq!(vars, vec!["COPILOT_HOME", "GH_CONFIG_DIR"]);
        assert!(copilot.unset.contains(&"GH_TOKEN".to_string()));
    }

    #[test]
    fn an_adopted_login_moves_no_dir_and_an_env_key_survives_its_strip() {
        let root = Path::new("/data/profiles");
        let mut main = profile("main", "codex");
        main.use_default = true;
        main.env.insert("OPENAI_API_KEY".into(), "sk-mine".into());
        assert_eq!(main.home_in(root), None);
        let env = main.launch_env_in(root);
        assert_eq!(
            env.set,
            vec![("OPENAI_API_KEY".to_string(), "sk-mine".to_string())]
        );
        assert!(
            !env.removals().contains(&"OPENAI_API_KEY".to_string()),
            "a variable the profile sets is not also removed"
        );
        assert!(env.removals().contains(&"CODEX_API_KEY".to_string()));
    }

    #[test]
    fn resolve_takes_the_named_profile_or_the_harness_default() {
        let mut a = profile("a", "claude");
        let mut b = profile("b", "claude");
        b.default = true;
        let c = profile("c", "codex");
        a.env.clear();
        let all = vec![a, b, c];
        assert_eq!(resolve(&all, "claude", None).unwrap().unwrap().name, "b");
        assert_eq!(
            resolve(&all, "claude", Some("a")).unwrap().unwrap().name,
            "a"
        );
        assert_eq!(resolve(&all, "codex", None).unwrap(), None, "no default");
        assert!(resolve(&all, "claude", Some("gone")).is_err());
        assert!(
            resolve(&all, "claude", Some("c")).is_err(),
            "a codex account on a claude launch"
        );
        assert_eq!(for_harness(&all, "claude").len(), 2);
    }

    #[test]
    fn claudes_usage_notice_parses_and_anything_else_does_not() {
        let usage =
            Usage::parse("  You've used 91% of your weekly limit \u{b7} resets 2pm").unwrap();
        assert_eq!(usage.percent, 91);
        assert_eq!(usage.window, "weekly");
        assert_eq!(usage.resets.as_deref(), Some("2pm"));
        assert_eq!(usage.summary(), "91% weekly \u{b7} resets 2pm");
        let usage = Usage::parse("You've used 100% of your Opus weekly limit").unwrap();
        assert_eq!((usage.percent, usage.window.as_str()), (100, "Opus weekly"));
        assert_eq!(usage.resets, None);
        assert_eq!(Usage::parse("I used 3 of your tools"), None);
        assert_eq!(Usage::parse("used % of your weekly limit"), None);

        let dir = tempfile::tempdir().unwrap();
        assert_eq!(read_usage_in(dir.path(), "work"), None);
        write_usage_in(dir.path(), "work", &usage).unwrap();
        assert_eq!(read_usage_in(dir.path(), "work"), Some(usage));
    }

    #[test]
    fn a_disabled_profile_is_never_the_default_and_its_flag_round_trips() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.json");
        let mut a = profile("a", "claude");
        a.default = true;
        add_to_file(&path, &a).unwrap();
        add_to_file(&path, &profile("b", "claude")).unwrap();
        let raw = std::fs::read_to_string(&path).unwrap();
        assert!(
            !raw.contains("disabled"),
            "an enabled profile writes no flag"
        );
        set_enabled_in_file(&path, "a", false).unwrap();
        let obj = crate::settings::read_object(&path).unwrap().unwrap();
        let all = from_settings(&obj);
        assert!(all[0].disabled);
        assert_eq!(
            default_for(&all, "claude"),
            None,
            "a disabled default is none"
        );
        assert_eq!(
            resolve(&all, "claude", Some("a")).unwrap().unwrap().name,
            "a",
            "a session naming it still resolves (restart, resume)"
        );
        let names: Vec<&str> = enabled_for(&all, "claude")
            .iter()
            .map(|p| p.name.as_str())
            .collect();
        assert_eq!(names, vec!["b"]);
        // An edit that leaves it enabled takes the flag off.
        let mut back = all[0].clone();
        back.disabled = false;
        update_in_file(&path, &back).unwrap();
        let obj = crate::settings::read_object(&path).unwrap().unwrap();
        assert!(!from_settings(&obj)[0].disabled);
        assert!(set_enabled_in_file(&path, "nope", true).is_err());
    }

    #[test]
    fn names_are_checked() {
        assert!(profile("", "claude").problem().is_some());
        assert!(profile("a b", "claude").problem().is_some());
        assert!(profile("../x", "claude").problem().is_some());
        assert!(profile("work_2", "").problem().is_some());
        assert_eq!(profile("work-2", "claude").problem(), None);
    }

    #[test]
    fn file_edits_keep_other_keys_and_one_default_per_harness() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.json");
        std::fs::write(
            &path,
            r#"{"theme":"dark","profiles":[{"name":"keep","harness":"claude","future":1}]}"#,
        )
        .unwrap();
        let mut a = profile("a", "claude");
        a.default = true;
        add_to_file(&path, &a).unwrap();
        assert!(add_to_file(&path, &a).is_err(), "duplicate name");
        add_to_file(&path, &profile("b", "claude")).unwrap();
        set_default_in_file(&path, "b").unwrap();
        let saved: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(saved["theme"], "dark");
        assert_eq!(saved["profiles"][0]["future"], 1, "unknown fields kept");
        assert_eq!(saved["profiles"][1].get("default"), None);
        assert_eq!(saved["profiles"][2]["default"], true);
        assert!(remove_from_file(&path, "a").unwrap());
        assert!(!remove_from_file(&path, "a").unwrap());
        let obj = crate::settings::read_object(&path).unwrap().unwrap();
        let names: Vec<String> = from_settings(&obj).into_iter().map(|p| p.name).collect();
        assert_eq!(names, vec!["keep", "b"]);
    }

    #[test]
    fn a_file_edit_reads_past_a_bom_and_never_replaces_a_broken_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.json");
        std::fs::write(&path, "\u{feff}{\"theme\": \"dark\"}").unwrap();
        add_to_file(&path, &profile("a", "claude")).unwrap();
        let saved: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(saved["theme"], "dark");

        let broken = "\u{feff}{\"theme\": \"dark\",}";
        std::fs::write(&path, broken).unwrap();
        assert!(add_to_file(&path, &profile("b", "claude")).is_err());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), broken);
        assert!(!dir.path().join("config.json.lock").exists());
    }

    #[test]
    fn seeding_marks_onboarding_and_carries_trust_over() {
        let dir = tempfile::tempdir().unwrap();
        let worktree = dir.path().join("code").join("app");
        let default_json = dir.path().join("default.json");
        let trusted_key = claude_project_key(&dir.path().join("code"));
        std::fs::write(
            &default_json,
            json!({"projects": {
                trusted_key.clone(): {"hasTrustDialogAccepted": true},
                "/elsewhere": {"hasTrustDialogAccepted": true},
            }})
            .to_string(),
        )
        .unwrap();
        let config = dir.path().join("profile-claude");
        seed_claude_config(&config, Some(&default_json), Some(&worktree)).unwrap();
        let seeded: Value =
            serde_json::from_str(&std::fs::read_to_string(config.join(".claude.json")).unwrap())
                .unwrap();
        assert_eq!(seeded["hasCompletedOnboarding"], true);
        assert_eq!(
            seeded["projects"][&trusted_key]["hasTrustDialogAccepted"], true,
            "the trusted folder above the worktree carries over"
        );
        assert!(seeded["projects"].get("/elsewhere").is_none());

        // Unchanged on a second seed; a file Claude wrote keeps its keys.
        std::fs::write(
            config.join(".claude.json"),
            json!({"hasCompletedOnboarding": true, "oauthAccount": {"x": 1}}).to_string(),
        )
        .unwrap();
        seed_claude_config(&config, None, Some(&worktree)).unwrap();
        let kept: Value =
            serde_json::from_str(&std::fs::read_to_string(config.join(".claude.json")).unwrap())
                .unwrap();
        assert_eq!(kept["oauthAccount"]["x"], 1);
    }
}
