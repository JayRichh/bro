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
                    if login_args(&profile.harness).is_some() {
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
        ProfileOp::Login { name, args } => login(&name, &args),
    }
}

fn list() -> Result<()> {
    let all = profiles::load();
    if all.is_empty() {
        println!(
            "no profiles: add one with: kete profile add <name> --harness <claude|codex|copilot>"
        );
        return Ok(());
    }
    for profile in &all {
        let flag = if profiles::default_for(&all, &profile.harness)
            .is_some_and(|d| d.name == profile.name)
        {
            " (default)"
        } else {
            ""
        };
        let home = match profile.home() {
            Some(home) => home.display().to_string(),
            None => "the CLI's default login".to_string(),
        };
        let usage = profiles::read_usage(profile)
            .map(|u| format!("  · {}", u.summary()))
            .unwrap_or_default();
        let problem = profile
            .problem()
            .map(|p| format!("  ! {p}"))
            .unwrap_or_default();
        println!(
            "{:<16} {:<8}{flag}  {home}{usage}{problem}",
            profile.name, profile.harness
        );
    }
    Ok(())
}

/// The harness's own sign-in, as its argv after the program.
fn login_args(harness: &str) -> Option<&'static [&'static str]> {
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
    let Some(args) = login_args(&profile.harness) else {
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
