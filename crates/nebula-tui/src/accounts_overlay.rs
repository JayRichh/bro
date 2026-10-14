//! The ACCOUNTS OVERLAY (`⇧U`): every account kete can launch under —
//! each PROFILE, and each harness's own login in use (`@claude`: a usage
//! reading, or a session running with no profile) — with its plan usage
//! as two block bars (session 5h and weekly; a Copilot account's monthly
//! allowances), when the windows reset, where the numbers came from,
//! whether it is signed in, and the live sessions running on it. A
//! profile the settings got wrong is listed with what is wrong, never
//! left out. `Space` (or `e`) switches a profile on or off, `d` (or `*`)
//! makes it its harness's default; both write the settings layer holding
//! the profiles in place (`nebula_core::profiles`), which the config beat
//! then reloads everywhere. A login is never run from here: an account
//! that needs one shows the command to type.

use std::path::PathBuf;

use nebula_core::accounts::{self, LoginCheck, LoginState};
use nebula_core::profiles::{self, Profile};
use nebula_core::usage::{self, AccountUsage, ContextUsage};
use ratatui::layout::Rect;

/// One row of the overlay.
#[derive(Debug, Clone, PartialEq)]
pub struct AccountRow {
    /// The account key: the profile's name, or `@<harness>`.
    pub key: String,
    pub harness: String,
    /// None for a harness's own login.
    pub profile: Option<Profile>,
    pub usage: Option<AccountUsage>,
    pub login: LoginState,
    /// The login was there and has run out.
    pub expired: bool,
    /// The harness's default profile.
    pub default: bool,
    /// Flagged default but switched off, so no launch takes it: the star
    /// is drawn dim.
    pub default_off: bool,
    /// What the settings got wrong about the profile; it cannot launch.
    pub problem: Option<String>,
    /// The live sessions running on the account, by name.
    pub sessions: Vec<String>,
}

impl AccountRow {
    /// What the row is called: the profile's name, or `claude (own login)`.
    pub fn label(&self) -> String {
        match &self.profile {
            Some(p) => p.name.trim().to_string(),
            None => format!("{} (own login)", self.harness),
        }
    }

    pub fn disabled(&self) -> bool {
        self.profile.as_ref().is_some_and(|p| p.disabled)
    }

    /// The login hint for an account that needs one.
    pub fn login_text(&self) -> String {
        if self.problem.is_some() {
            return String::new();
        }
        match self.login {
            LoginState::LoggedIn => "signed in".into(),
            LoginState::NeedsLogin => {
                let lead = if self.expired {
                    "login expired"
                } else {
                    "needs login"
                };
                match accounts::login_command(self.profile.as_ref(), &self.harness) {
                    Some(cmd) => format!("{lead}: {cmd}"),
                    None => lead.into(),
                }
            }
            LoginState::Unknown => String::new(),
        }
    }

    /// How the account is used: `2 sessions: fix-auth, docs`, the names
    /// cut after a few; empty with none running.
    pub fn sessions_text(&self) -> String {
        const NAMED: usize = 3;
        let n = self.sessions.len();
        if n == 0 {
            return String::new();
        }
        let mut names = self
            .sessions
            .iter()
            .take(NAMED)
            .cloned()
            .collect::<Vec<_>>()
            .join(", ");
        if n > NAMED {
            names.push_str(&format!(" +{} more", n - NAMED));
        }
        format!("{n} session{}: {names}", if n == 1 { "" } else { "s" })
    }
}

/// The kept readings on disk (`<profiles>/.usage`), for a daemon too old
/// to push them. Unit tests never read the machine's real data dir: a
/// reading Jay's own sessions left there would change what they draw.
pub fn kept_readings() -> Vec<nebula_core::usage::AccountUsage> {
    if cfg!(test) {
        return Vec::new();
    }
    nebula_core::usage::read_all_in(&profiles::profiles_root())
}

/// [`nebula_core::accounts::login_check`], except under unit tests, which
/// never look at the machine's real logins.
pub fn login_lookup(profile: Option<&Profile>, harness: &str) -> LoginCheck {
    if cfg!(test) {
        return LoginCheck {
            state: LoginState::Unknown,
            expired: false,
        };
    }
    accounts::login_check(profile, harness)
}

/// The rows for `all` profiles, the `usage` readings and the `agents`
/// running: every profile in config order — one the settings got wrong
/// too, with its problem — then each own login in use
/// (`accounts::own_login_harnesses`). Each row counts the live sessions
/// on it (`accounts::sessions_by_account`).
pub fn build_rows(
    all: &[Profile],
    usage: &[AccountUsage],
    agents: &[nebula_core::Agent],
    login: impl Fn(Option<&Profile>, &str) -> LoginCheck,
) -> Vec<AccountRow> {
    let reading = |key: &str| usage.iter().find(|u| u.account == key).cloned();
    let by_account = accounts::sessions_by_account(agents);
    let sessions = |key: &str| -> Vec<String> {
        by_account
            .get(key)
            .map(|list| list.iter().map(|a| a.name.clone()).collect())
            .unwrap_or_default()
    };
    let mut rows: Vec<AccountRow> = all
        .iter()
        .map(|p| {
            let key = p.name.trim().to_string();
            let harness = p.harness.trim().to_string();
            let problem = p.problem();
            let check = if problem.is_some() {
                LoginCheck {
                    state: LoginState::Unknown,
                    expired: false,
                }
            } else {
                login(Some(p), &harness)
            };
            let flagged =
                profiles::flagged_default(all, &harness).is_some_and(|d| d.name == p.name);
            AccountRow {
                usage: reading(&key),
                sessions: sessions(&key),
                key,
                profile: Some(p.clone()),
                login: check.state,
                expired: check.expired,
                default: flagged && !p.disabled,
                default_off: flagged && p.disabled,
                problem,
                harness,
            }
        })
        .collect();
    let own = accounts::own_login_harnesses(usage.iter().map(|u| u.account.as_str()), agents);
    for harness in own {
        let key = usage::account_key(None, &harness);
        let check = login(None, &harness);
        rows.push(AccountRow {
            usage: reading(&key),
            sessions: sessions(&key),
            key,
            profile: None,
            login: check.state,
            expired: check.expired,
            default: false,
            default_off: false,
            problem: None,
            harness,
        });
    }
    rows
}

/// The overlay's state.
#[derive(Debug, Clone, PartialEq)]
pub struct AccountsView {
    pub rows: Vec<AccountRow>,
    pub cursor: usize,
    /// config.json, and config.local.json beside it: edits go to the
    /// layer whose `profiles` list is in force (`Self::on_profiles_layer`).
    pub config: PathBuf,
    pub local: PathBuf,
    /// Drawn rect, for click-outside.
    pub area: Rect,
}

impl AccountsView {
    pub fn new(rows: Vec<AccountRow>, config: PathBuf, local: PathBuf) -> Self {
        Self {
            rows,
            cursor: 0,
            config,
            local,
            area: Rect::default(),
        }
    }

    pub fn selected(&self) -> Option<&AccountRow> {
        self.rows.get(self.cursor)
    }

    pub fn step(&mut self, delta: i32) {
        if self.rows.is_empty() {
            self.cursor = 0;
            return;
        }
        let n = self.rows.len() as i32;
        self.cursor = (self.cursor as i32 + delta).clamp(0, n - 1) as usize;
    }

    /// Run a profile edit on the layer the profiles come from: a
    /// config.local.json holding a `profiles` key replaces config.json's
    /// list whole, so the edit goes there, under config.json's lock too (the
    /// settings writers take that one for both layers). A local layer that
    /// is there but unreadable is refused: there is no telling which list
    /// is in force.
    fn on_profiles_layer(
        &self,
        edit: impl FnOnce(&std::path::Path) -> Result<(), String>,
    ) -> Result<(), String> {
        let held = nebula_core::settings::read_object(&self.local)
            .map_err(|e| format!("{}: {e}: fix it, then try again", self.local.display()))?;
        if held.is_some_and(|obj| obj.contains_key(profiles::CONFIG_KEY)) {
            let _lock = nebula_core::settings::lock(&self.config).map_err(|e| e.to_string())?;
            edit(&self.local)
        } else {
            edit(&self.config)
        }
    }

    /// Switch the selected profile on or off in the settings file. The
    /// message for the footer, or why not.
    pub fn toggle_enabled(&mut self) -> Result<String, String> {
        let row = self.selected().ok_or("no account selected")?;
        if let Some(problem) = &row.problem {
            return Err(format!("{}: {problem}", row.label()));
        }
        let Some(profile) = &row.profile else {
            return Err(format!(
                "{} is the CLI's own login: add it as a profile to switch it off (kete profile add <name> --harness {} --use-default)",
                row.label(),
                row.harness
            ));
        };
        let name = profile.name.trim().to_string();
        let on = profile.disabled;
        self.on_profiles_layer(|layer| profiles::set_enabled_in_file(layer, &name, on))?;
        if let Some(row) = self.rows.get_mut(self.cursor) {
            if let Some(p) = row.profile.as_mut() {
                p.disabled = !on;
            }
            // The star stays where it is, lit only while the profile is on.
            let flagged = row.default || row.default_off;
            row.default = flagged && on;
            row.default_off = flagged && !on;
        }
        Ok(if on {
            format!("{name} is on")
        } else {
            format!("{name} is off: launches that pick no profile skip it")
        })
    }

    /// Make the selected profile its harness's default.
    pub fn set_default(&mut self) -> Result<String, String> {
        let row = self.selected().ok_or("no account selected")?;
        if let Some(problem) = &row.problem {
            return Err(format!("{}: {problem}", row.label()));
        }
        let Some(profile) = &row.profile else {
            return Err(format!("{} is not a profile", row.label()));
        };
        let (name, harness) = (profile.name.trim().to_string(), row.harness.clone());
        self.on_profiles_layer(|layer| profiles::set_default_in_file(layer, &name))?;
        for r in &mut self.rows {
            if r.harness == harness && r.profile.is_some() {
                let flagged = r.key == name;
                r.default = flagged && !r.disabled();
                r.default_off = flagged && r.disabled();
            }
        }
        Ok(format!("{name} is now the {harness} default"))
    }
}

/// `ctx 42%` (with `⟳2` after two compactions) for a session card; None
/// when nothing is known.
pub fn context_label(ctx: &ContextUsage) -> Option<String> {
    let p = ctx.percent?;
    let mut out = format!("ctx {}", usage::pct_text(Some(p)));
    if ctx.compactions > 0 {
        out.push_str(&format!(" \u{27f3}{}", ctx.compactions));
    }
    Some(out)
}

/// The QUICK PROMPT's profile field: `work ▰▰▱▱ 21% ▰▱▱▱ 3%`, or the
/// name alone when no reading is known.
pub fn field_text(name: &str, reading: Option<&AccountUsage>, now: i64) -> String {
    let Some(u) = reading else {
        return name.to_string();
    };
    let mut out = name.to_string();
    for w in u.bars() {
        let p = w.and_then(|w| w.shown_at(now));
        out.push(' ');
        out.push_str(&usage::block_bar(p, 4));
        out.push(' ');
        out.push_str(&usage::pct_text(p));
    }
    out
}

/// The worst tone of a reading's two bars.
pub fn reading_tone(reading: Option<&AccountUsage>, now: i64) -> usage::Tone {
    reading
        .and_then(|u| u.worst_at(now))
        .map_or(usage::Tone::Ok, usage::tone)
}

/// `5m ago`, `2h ago` — how old a reading is.
pub fn age(at_ms: i64, now_ms: i64) -> String {
    if at_ms <= 0 {
        return "time unknown".into();
    }
    let secs = ((now_ms - at_ms) / 1000).max(0);
    match secs {
        0..=59 => "just now".into(),
        60..=3599 => format!("{}m ago", secs / 60),
        3600..=86_399 => format!("{}h ago", secs / 3600),
        _ => format!("{}d ago", secs / 86_400),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nebula_core::usage::{UsageSource, UsageWindow, WindowKind};
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

    fn reading(key: &str, s: f32, w: f32) -> AccountUsage {
        AccountUsage {
            account: key.into(),
            harness: "claude".into(),
            windows: vec![
                UsageWindow {
                    kind: WindowKind::Session,
                    label: None,
                    percent: s,
                    resets_at: None,
                    read_at: 1,
                },
                UsageWindow {
                    kind: WindowKind::Weekly,
                    label: None,
                    percent: w,
                    resets_at: None,
                    read_at: 1,
                },
            ],
            source: UsageSource::Statusline,
            at: 0,
            plan: None,
        }
    }

    fn check(state: LoginState) -> LoginCheck {
        LoginCheck {
            state,
            expired: false,
        }
    }

    fn signed_in(_: Option<&Profile>, _: &str) -> LoginCheck {
        check(LoginState::LoggedIn)
    }

    fn session(name: &str, profile: Option<&str>, archived: bool) -> nebula_core::Agent {
        nebula_core::Agent {
            parent: None,
            profile: profile.map(str::to_string),
            id: nebula_core::AgentId(name.into()),
            worktree_id: nebula_core::WorktreeId("w".into()),
            name: name.into(),
            status: nebula_core::AgentStatus::Running,
            archived,
            archived_at: 0,
            unseen: false,
            kind: nebula_core::AgentKind::Claude,
            custom_harness: None,
            model: None,
            effort: None,
            session_id: None,
            cloud_session_id: None,
            sort_order: 0,
            status_changed_at: 0,
            alive: true,
            issue_url: None,
            role: nebula_core::AgentRole::Worker,
            recent_prompts: Vec::new(),
        }
    }

    #[test]
    fn rows_are_profiles_then_own_logins_with_readings() {
        let all = vec![profile("work", true), profile("home", false)];
        let usage = vec![reading("work", 10.0, 20.0), reading("@claude", 5.0, 1.0)];
        let rows = build_rows(&all, &usage, &[], |p, _| match p {
            Some(p) if p.name == "home" => check(LoginState::NeedsLogin),
            Some(p) if p.name == "work" => LoginCheck {
                state: LoginState::NeedsLogin,
                expired: true,
            },
            _ => check(LoginState::LoggedIn),
        });
        let keys: Vec<&str> = rows.iter().map(|r| r.key.as_str()).collect();
        assert_eq!(keys, ["work", "home", "@claude"]);
        assert!(rows[0].default && !rows[1].default);
        assert!(rows[0].usage.is_some() && rows[1].usage.is_none());
        assert_eq!(rows[2].label(), "claude (own login)");
        assert_eq!(rows[1].login_text(), "needs login: kete profile login home");
        assert_eq!(
            rows[0].login_text(),
            "login expired: kete profile login work"
        );
        assert_eq!(rows[2].login_text(), "signed in");
        // Jay's case: no profiles, one own-login reading.
        let rows = build_rows(&[], &[reading("@claude", 1.0, 1.0)], &[], signed_in);
        assert_eq!(rows.len(), 1);
    }

    /// How each account is used: its live sessions, counted and named; an
    /// own login with a session on it and no reading still gets a row.
    #[test]
    fn rows_count_the_live_sessions_on_each_account() {
        let all = vec![profile("work", true), profile("home", false)];
        let agents = vec![
            session("fix-auth", Some("work"), false),
            session("docs", Some("work"), false),
            session("old", Some("work"), true),
            session("bare", None, false),
        ];
        let rows = build_rows(&all, &[], &agents, signed_in);
        let keys: Vec<&str> = rows.iter().map(|r| r.key.as_str()).collect();
        assert_eq!(keys, ["work", "home", "@claude"]);
        assert_eq!(rows[0].sessions, ["fix-auth", "docs"]);
        assert_eq!(rows[0].sessions_text(), "2 sessions: fix-auth, docs");
        assert_eq!(rows[1].sessions_text(), "");
        assert_eq!(rows[2].sessions_text(), "1 session: bare");
    }

    /// A profile the settings got wrong is listed with its problem, and
    /// neither switch nor default touches it; a default switched off keeps
    /// a dim star.
    #[test]
    fn a_broken_profile_is_listed_with_its_problem() {
        let mut broken = profile("bad", false);
        broken.harness = String::new();
        let mut off = profile("off", true);
        off.disabled = true;
        let all = vec![broken, off];
        let rows = build_rows(&all, &[], &[], signed_in);
        assert_eq!(rows.len(), 2, "nothing filtered out");
        assert!(rows[0].problem.is_some(), "{:?}", rows[0]);
        assert_eq!(rows[0].login_text(), "");
        assert!(!rows[1].default && rows[1].default_off);
        let dir = tempfile::tempdir().unwrap();
        let mut view = AccountsView::new(
            rows,
            dir.path().join("config.json"),
            dir.path().join("config.local.json"),
        );
        assert!(view.toggle_enabled().is_err());
        assert!(view.set_default().is_err());
    }

    /// A config.local.json holding `profiles` is the list in force: the
    /// switch and the default land there, config.json is left alone.
    #[test]
    fn edits_go_to_the_local_layer_when_it_holds_the_profiles() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.json");
        let local = dir.path().join("config.local.json");
        std::fs::write(
            &path,
            r#"{"profiles":[{"name":"base","harness":"claude"}]}"#,
        )
        .unwrap();
        std::fs::write(
            &local,
            r#"{"profiles":[{"name":"work","harness":"claude"},{"name":"home","harness":"claude"}]}"#,
        )
        .unwrap();
        let all = vec![profile("work", false), profile("home", false)];
        let mut view = AccountsView::new(
            build_rows(&all, &[], &[], signed_in),
            path.clone(),
            local.clone(),
        );
        view.toggle_enabled().unwrap();
        view.step(1);
        view.set_default().unwrap();
        let saved: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&local).unwrap()).unwrap();
        assert_eq!(saved["profiles"][0]["disabled"], true);
        assert_eq!(saved["profiles"][1]["default"], true);
        let base = std::fs::read_to_string(&path).unwrap();
        assert!(
            !base.contains("disabled") && !base.contains("default"),
            "{base}"
        );
        assert!(
            !dir.path().join("config.json.lock").exists(),
            "lock released"
        );
        std::fs::write(&local, "{ not json").unwrap();
        assert!(view.toggle_enabled().is_err());
    }

    #[test]
    fn toggle_and_default_write_the_settings_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.json");
        std::fs::write(
            &path,
            r#"{"theme":"x","profiles":[{"name":"work","harness":"claude","default":true},{"name":"home","harness":"claude"}]}"#,
        )
        .unwrap();
        let all = vec![profile("work", true), profile("home", false)];
        let mut view = AccountsView::new(
            build_rows(&all, &[reading("@claude", 0.0, 0.0)], &[], signed_in),
            path.clone(),
            dir.path().join("config.local.json"),
        );
        view.step(1);
        assert!(view.toggle_enabled().unwrap().contains("off"));
        assert!(view.rows[1].disabled());
        let saved: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(saved["profiles"][1]["disabled"], true);
        assert_eq!(saved["theme"], "x", "other keys kept");
        assert!(view.toggle_enabled().unwrap().contains("on"));
        view.set_default().unwrap();
        assert!(view.rows[1].default && !view.rows[0].default);
        let saved: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(saved["profiles"][1]["default"], true);
        assert!(saved["profiles"][0].get("default").is_none());
        assert!(saved["profiles"][1].get("disabled").is_none());
        // The own-login row is not a profile.
        view.step(5);
        assert_eq!(view.cursor, 2, "clamped");
        assert!(view.toggle_enabled().is_err());
        assert!(view.set_default().is_err());
    }

    #[test]
    fn field_and_card_labels() {
        assert_eq!(field_text("work", None, 0), "work");
        assert_eq!(
            field_text("work", Some(&reading("work", 50.0, 10.0)), 0),
            "work \u{25b0}\u{25b0}\u{25b1}\u{25b1} 50% \u{25b0}\u{25b1}\u{25b1}\u{25b1} 10%"
        );
        assert_eq!(
            reading_tone(Some(&reading("w", 81.0, 0.0)), 0),
            usage::Tone::Warn
        );
        assert_eq!(reading_tone(None, 0), usage::Tone::Ok);
        let ctx = ContextUsage {
            percent: Some(42.4),
            tokens: None,
            window: None,
            compactions: 2,
            last_compact_at: None,
            source: UsageSource::Statusline,
            at: 0,
        };
        assert_eq!(context_label(&ctx).as_deref(), Some("ctx 42% \u{27f3}2"));
        assert_eq!(
            context_label(&ContextUsage {
                percent: None,
                ..ctx
            }),
            None
        );
        assert_eq!(age(1, 30_000), "just now");
        assert_eq!(age(1, 3 * 3_600_000 + 1), "3h ago");
        assert_eq!(age(0, 5), "time unknown");
    }
}
