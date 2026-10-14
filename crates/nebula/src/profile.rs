//! `kete profile`: the PROFILES list from the command line — list, add,
//! remove, pick the default, and sign one in.
//!
//! The list is `config.json`'s `profiles` key (`nebula_core::profiles`),
//! edited as raw JSON so every other key, and every field a newer kete
//! wrote, survives. Nothing here talks to the daemon: it reads the list
//! fresh at each spawn. `login` runs the harness's own sign-in in this
//! terminal with the profile's environment applied, so the credentials it
//! writes land in the PROFILE HOME (`<data dir>/profiles/<name>/`).

use anyhow::{bail, Context, Result};
use nebula_core::profiles::{self, Profile};
use std::collections::BTreeMap;

/// What `kete profile` was asked to do.
pub enum ProfileOp {
    List,
    Add {
        name: String,
        harness: String,
        use_default: bool,
        default: bool,
        env: Vec<String>,
    },
    Remove {
        name: String,
        purge: bool,
    },
    Default {
        name: String,
    },
    Enable {
        name: String,
        on: bool,
    },
    Login {
        name: String,
        args: Vec<String>,
    },
}

pub fn run_profile(op: ProfileOp) -> Result<()> {
    let config = nebula_core::paths::config_path();
    match op {
        ProfileOp::List => list(),
        ProfileOp::Add {
            name,
            harness,
            use_default,
            default,
            env,
        } => {
            let registry = nebula_tui::config::Config::load().harness_registry();
            if !registry.iter().any(|h| h.id == harness.trim()) {
                bail!(
                    "unknown harness `{harness}`: expected one of {}",
                    registry
                        .iter()
                        .map(|h| h.id.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                );
            }
            let mut vars = BTreeMap::new();
            for pair in env {
                let Some((key, value)) = pair.split_once('=') else {
                    bail!("--env takes KEY=VALUE, not `{pair}`");
                };
                vars.insert(key.trim().to_string(), value.to_string());
            }
            let profile = Profile {
                name: name.trim().to_string(),
                harness: harness.trim().to_string(),
                env: vars,
                default,
                use_default,
                disabled: false,
            };
            profiles::add_to_file(&config, &profile).map_err(anyhow::Error::msg)?;
            match profile.home() {
                Some(home) => {
                    profile
                        .prepare(None)
                        .with_context(|| format!("create {}", home.display()))?;
                    println!(
                        "added profile `{}` ({}), home {}",
                        profile.name,
                        profile.harness,
                        home.display()
                    );
                    if profiles::login_args(&profile.harness).is_some() {
                        println!("sign it in with: kete profile login {}", profile.name);
                    }
                }
                None => println!(
                    "added profile `{}` ({}) on the CLI's default login",
                    profile.name, profile.harness
                ),
            }
            Ok(())
        }
        ProfileOp::Remove { name, purge } => {
            let profile = profiles::load()
                .into_iter()
                .find(|p| p.name.trim() == name.trim());
            if !profiles::remove_from_file(&config, &name).map_err(anyhow::Error::msg)? {
                bail!("no profile named `{name}`");
            }
            println!("removed profile `{}`", name.trim());
            // The home holds the login: kept unless asked, so adding the
            // name back signs nothing in again. Never an adopted login's —
            // that is the CLI's own default config, and not ours.
            if let Some(home) = profile.as_ref().and_then(Profile::home) {
                if purge {
                    purge_home(&home)?;
                    println!("deleted {}", home.display());
                } else if home.exists() {
                    println!(
                        "its home stays at {} (kete profile remove --purge deletes it)",
                        home.display()
                    );
                }
            }
            Ok(())
        }
        ProfileOp::Default { name } => {
            profiles::set_default_in_file(&config, &name).map_err(anyhow::Error::msg)?;
            println!("`{}` is now its harness's default profile", name.trim());
            Ok(())
        }
        ProfileOp::Enable { name, on } => {
            profiles::set_enabled_in_file(&config, &name, on).map_err(anyhow::Error::msg)?;
            println!(
                "`{}` is {}",
                name.trim(),
                if on {
                    "enabled"
                } else {
                    "disabled: launches that pick no profile skip it"
                }
            );
            Ok(())
        }
        ProfileOp::Login { name, args } => login(&name, &args),
    }
}

fn list() -> Result<()> {
    let all = profiles::load();
    let usage = nebula_core::usage::read_all_in(&profiles::profiles_root());
    let policy = nebula_core::accounts::load_policy();
    let now = nebula_core::clock::now_secs() as i64;
    let lines = render_list(&all, &usage, now, |p, harness| {
        nebula_core::accounts::login_state(p, harness)
    });
    if all.is_empty() {
        println!(
            "no profiles: add one with: kete profile add <name> --harness <claude|codex|copilot>"
        );
    }
    for line in lines {
        println!("{line}");
    }
    if !all.is_empty() {
        let spread = match policy.spread {
            nebula_core::accounts::Spread::Manual => "manual (the default profile)".to_string(),
            nebula_core::accounts::Spread::Failover => {
                format!("failover at {}%", policy.threshold)
            }
            nebula_core::accounts::Spread::LeastUsed => "least used".to_string(),
        };
        println!("\naccount spread: {spread}  (Settings \u{2192} Sessions \u{2192} Accounts)");
    }
    Ok(())
}

/// Width of a usage bar in `kete profile list`.
const BAR: usize = 10;

/// `kete profile list`'s lines: every profile, then each harness's own
/// login that has a reading (`@claude`), with a bar for its session (5h)
/// and weekly use, and whether it is signed in.
fn render_list(
    all: &[Profile],
    usage: &[nebula_core::usage::AccountUsage],
    now: i64,
    login: impl Fn(Option<&Profile>, &str) -> nebula_core::accounts::LoginState,
) -> Vec<String> {
    use nebula_core::accounts::LoginState;
    use nebula_core::usage::{block_bar, pct_text};
    let cell = |w: Option<&nebula_core::usage::UsageWindow>| {
        let p = w.map(|w| w.percent_at(now));
        format!("{} {:>4}", block_bar(p, BAR), pct_text(p))
    };
    let mut rows: Vec<(String, String, String, String, String)> = Vec::new();
    let mut push = |name: String, harness: &str, profile: Option<&Profile>| {
        let key = nebula_core::usage::account_key(profile.map(|p| p.name.as_str()), harness);
        let reading = usage.iter().find(|u| u.account == key);
        let [top, bottom] = reading.map_or([None, None], |u| u.bars());
        let state = match profile.and_then(Profile::problem) {
            Some(problem) => format!("! {problem}"),
            None => match login(profile, harness) {
                LoginState::LoggedIn => "signed in".to_string(),
                LoginState::NeedsLogin => match profile.and_then(profiles::login_hint) {
                    Some(hint) => format!("needs login: {hint}"),
                    None => "needs login".to_string(),
                },
                LoginState::Unknown => String::new(),
            },
        };
        rows.push((name, harness.to_string(), cell(top), cell(bottom), state));
    };
    for profile in all {
        let mut name = profile.name.clone();
        if profiles::default_for(all, &profile.harness).is_some_and(|d| d.name == profile.name) {
            name.push_str(" *");
        }
        if profile.disabled {
            name.push_str(" (off)");
        }
        push(name, profile.harness.trim(), Some(profile));
    }
    for u in usage {
        if let Some(harness) = nebula_core::usage::default_login_harness(&u.account) {
            push(format!("{} (own login)", harness), harness, None);
        }
    }
    if rows.is_empty() {
        return Vec::new();
    }
    let mut out = vec![format!(
        "{:<22} {:<8} {:<15} {:<15} {}",
        "ACCOUNT", "HARNESS", "5H / FIRST", "WEEKLY / NEXT", "LOGIN"
    )];
    for (name, harness, top, bottom, state) in rows {
        out.push(
            format!("{name:<22} {harness:<8} {top:<15} {bottom:<15} {state}")
                .trim_end()
                .to_string(),
        );
    }
    out.push("* default".to_string());
    out
}

/// Run the harness's sign-in in this terminal under the profile's
/// environment, and wait for it. `extra` rides after the sign-in's own
/// arguments (`--email you@example.com`, `--device-code`).
fn login(name: &str, extra: &[String]) -> Result<()> {
    let all = profiles::load();
    let Some(profile) = all.iter().find(|p| p.name.trim() == name.trim()) else {
        bail!("no profile named `{name}`: kete profile list shows them");
    };
    if let Some(problem) = profile.problem() {
        bail!("{problem}");
    }
    let Some(args) = profiles::login_args(&profile.harness) else {
        bail!(
            "kete knows no sign-in command for `{}`: sign in from a session under the profile instead",
            profile.harness
        );
    };
    let registry = nebula_tui::config::Config::load().harness_registry();
    let program = registry
        .iter()
        .find(|h| h.id == profile.harness.trim())
        .map(|h| h.program.trim().to_string())
        .filter(|p| !p.is_empty())
        .unwrap_or_else(|| profile.harness.trim().to_string());
    profile.prepare(None).context("prepare the profile home")?;
    let env = profile.launch_env();
    let args: Vec<String> = args
        .iter()
        .map(|a| a.to_string())
        .chain(extra.iter().cloned())
        .collect();
    match profile.home() {
        Some(home) => eprintln!(
            "kete: signing `{}` in with `{program} {}` (home {})",
            profile.name,
            args.join(" "),
            home.display()
        ),
        None => eprintln!(
            "kete: signing `{}` in with `{program} {}` (the CLI's default login)",
            profile.name,
            args.join(" ")
        ),
    }
    let mut command = login_command(&program, &args)?;
    for name in env.removals() {
        command.env_remove(name);
    }
    command.envs(env.set.iter().map(|(k, v)| (k, v)));
    let status = command
        .status()
        .with_context(|| format!("run {program}: is it installed?"))?;
    if !status.success() {
        bail!("{program} exited with {status}");
    }
    Ok(())
}

/// `program args…` as a process: on Windows resolved through PATH and
/// PATHEXT, a `.cmd` or `.ps1` under its host, as a session spawn does.
#[cfg(windows)]
fn login_command(program: &str, args: &[String]) -> Result<std::process::Command> {
    let launch = nebula_core::shell::launch(program, args).map_err(anyhow::Error::msg)?;
    let mut command = std::process::Command::new(&launch.program);
    command
        .args(&launch.args)
        .envs(launch.env.iter().map(|(k, v)| (k, v)));
    Ok(command)
}

#[cfg(unix)]
fn login_command(program: &str, args: &[String]) -> Result<std::process::Command> {
    let mut command = std::process::Command::new(program);
    command.args(args);
    Ok(command)
}

/// Delete a PROFILE HOME — only ever a directory directly inside
/// `<data dir>/profiles`, checked on the resolved paths.
fn purge_home(home: &std::path::Path) -> Result<()> {
    if !home.exists() {
        return Ok(());
    }
    let root = nebula_core::paths::canonical(&profiles::profiles_root())
        .context("resolve the profiles dir")?;
    let target = nebula_core::paths::canonical(home).context("resolve the profile home")?;
    if target.parent() != Some(root.as_path()) {
        bail!(
            "refusing to delete {}: not a profile home under {}",
            target.display(),
            root.display()
        );
    }
    std::fs::remove_dir_all(&target).with_context(|| format!("delete {}", target.display()))
}

#[cfg(test)]
mod list_tests {
    use super::*;
    use nebula_core::accounts::LoginState;
    use nebula_core::usage::{AccountUsage, UsageSource, UsageWindow, WindowKind};

    #[test]
    fn the_list_draws_bars_flags_and_the_login_command() {
        let p = |name: &str, default: bool, disabled: bool| Profile {
            name: name.into(),
            harness: "claude".into(),
            env: BTreeMap::new(),
            default,
            use_default: false,
            disabled,
        };
        let all = vec![p("work", true, false), p("home", false, true)];
        let usage = vec![
            AccountUsage {
                account: "work".into(),
                harness: "claude".into(),
                windows: vec![
                    UsageWindow {
                        kind: WindowKind::Session,
                        label: None,
                        percent: 50.0,
                        resets_at: None,
                    },
                    UsageWindow {
                        kind: WindowKind::Weekly,
                        label: None,
                        percent: 10.0,
                        resets_at: None,
                    },
                ],
                source: UsageSource::Statusline,
                at: 0,
                plan: None,
            },
            AccountUsage {
                account: "@codex".into(),
                harness: "codex".into(),
                windows: Vec::new(),
                source: UsageSource::Rollout,
                at: 0,
                plan: None,
            },
        ];
        let lines = render_list(&all, &usage, 0, |p, _| match p {
            Some(p) if p.name == "home" => LoginState::NeedsLogin,
            _ => LoginState::LoggedIn,
        });
        assert!(lines[0].starts_with("ACCOUNT"));
        assert!(lines[1].starts_with("work *"), "{}", lines[1]);
        assert!(
            lines[1].contains("\u{25b0}\u{25b0}\u{25b0}\u{25b0}\u{25b0}\u{25b1}"),
            "{}",
            lines[1]
        );
        assert!(lines[1].contains(" 50%") && lines[1].contains(" 10%"));
        assert!(lines[1].ends_with("signed in"));
        assert!(lines[2].starts_with("home (off)"));
        assert!(lines[2].contains("\u{2014}"), "unknown reads as a dash");
        assert!(lines[2].ends_with("needs login: kete profile login home"));
        assert!(lines[3].starts_with("codex (own login)"));
    }
}
