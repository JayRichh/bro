//! The environment every git this crate starts runs under, so a fetch or
//! a push from either app never asks for anything: no terminal prompt
//! (`GIT_TERMINAL_PROMPT=0`, which in the TUI would write over the
//! screen), no Git Credential Manager window (`GCM_INTERACTIVE=never`),
//! and ssh in batch mode (`-o BatchMode=yes`), so a key with a passphrase
//! or an unknown host fails rather than asks. A failure for want of a
//! sign-in is then read from git's stderr ([`needs_sign_in`]).
//!
//! ssh is only put in batch mode where that cannot break the user's own
//! setup: an exported `GIT_SSH_COMMAND` or `GIT_SSH` (plink on many
//! Windows machines) is theirs and left alone, since a `GIT_SSH_COMMAND`
//! of ours would replace it; a `core.sshCommand` that runs OpenSSH gets the
//! option appended, any other is left alone; with none, plain
//! `ssh -o BatchMode=yes`.

use std::path::Path;

/// The option that keeps ssh from asking.
const BATCH: &str = "-o BatchMode=yes";

/// What the user's machine says about ssh, read once per command.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct SshSetup {
    /// `GIT_SSH_COMMAND` in our environment.
    pub ssh_command: Option<String>,
    /// `GIT_SSH` in our environment.
    pub git_ssh: Option<String>,
    /// The repo's `core.sshCommand`, when it was read (network commands).
    pub core_ssh_command: Option<String>,
}

impl SshSetup {
    /// Our environment's half; `core.sshCommand` is the caller's to add.
    pub fn from_env() -> Self {
        let var = |k: &str| std::env::var(k).ok().filter(|v| !v.trim().is_empty());
        Self {
            ssh_command: var("GIT_SSH_COMMAND"),
            git_ssh: var("GIT_SSH"),
            core_ssh_command: None,
        }
    }
}

/// The variables to set on a git command: the two no-prompt switches,
/// and `GIT_SSH_COMMAND` where ssh can be put in batch mode safely.
pub fn quiet_env(ssh: &SshSetup) -> Vec<(&'static str, String)> {
    let mut out = vec![
        ("GIT_TERMINAL_PROMPT", "0".to_string()),
        ("GCM_INTERACTIVE", "never".to_string()),
    ];
    if ssh.ssh_command.is_some() || ssh.git_ssh.is_some() {
        return out;
    }
    let command = match ssh.core_ssh_command.as_deref().map(str::trim) {
        Some(c) if !c.is_empty() => {
            if !runs_openssh(c) || has_batch_mode(c) {
                return out;
            }
            format!("{c} {BATCH}")
        }
        _ => format!("ssh {BATCH}"),
    };
    out.push(("GIT_SSH_COMMAND", command));
    out
}

/// Does `command` start OpenSSH (`ssh`, `ssh.exe`, a path to one, quoted
/// or not)? Not plink, not a wrapper script: those take other options.
fn runs_openssh(command: &str) -> bool {
    let c = command.trim_start();
    let program = match c.strip_prefix('"') {
        Some(rest) => rest.split('"').next().unwrap_or_default(),
        None => c.split_whitespace().next().unwrap_or_default(),
    };
    let name = program
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or_default()
        .to_ascii_lowercase();
    name == "ssh" || name == "ssh.exe"
}

fn has_batch_mode(command: &str) -> bool {
    command.to_ascii_lowercase().contains("batchmode")
}

/// Does `args` reach a remote, so ssh may run?
pub fn is_network(args: &[&str]) -> bool {
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match *a {
            // `-c key=value` and `-C dir` take the next word.
            "-c" | "-C" => {
                it.next();
            }
            a if a.starts_with('-') => {}
            a => return matches!(a, "push" | "fetch" | "pull" | "ls-remote" | "clone"),
        }
    }
    false
}

/// `git config --get core.sshCommand` in `cwd`, with no prompts.
pub fn read_core_ssh_command(cwd: &Path) -> Option<String> {
    let mut cmd = std::process::Command::new("git");
    cmd.args(["config", "--get", "core.sshCommand"])
        .current_dir(cwd)
        .stdin(std::process::Stdio::null())
        .env("GIT_TERMINAL_PROMPT", "0");
    #[cfg(windows)]
    nebula_core::platform::no_window(&mut cmd);
    let out = cmd.output().ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).trim().to_string())
        .filter(|s| !s.is_empty())
}

/// The same, off the async runtime's threads.
pub async fn read_core_ssh_command_async(cwd: &Path) -> Option<String> {
    let cwd = cwd.to_path_buf();
    tokio::task::spawn_blocking(move || read_core_ssh_command(&cwd))
        .await
        .ok()
        .flatten()
}

/// The setup for `git args` in `cwd`: the environment's, and for a
/// command that reaches a remote the repo's `core.sshCommand`.
pub fn setup_for(cwd: &Path, args: &[&str]) -> SshSetup {
    let mut s = SshSetup::from_env();
    if is_network(args) && s.ssh_command.is_none() && s.git_ssh.is_none() {
        s.core_ssh_command = read_core_ssh_command(cwd);
    }
    s
}

/// [`setup_for`] off the async runtime's threads.
pub async fn setup_for_async(cwd: &Path, args: &[&str]) -> SshSetup {
    let mut s = SshSetup::from_env();
    if is_network(args) && s.ssh_command.is_none() && s.git_ssh.is_none() {
        s.core_ssh_command = read_core_ssh_command_async(cwd).await;
    }
    s
}

/// Set [`quiet_env`] on a std command.
pub fn quiet(cmd: &mut std::process::Command, ssh: &SshSetup) {
    for (k, v) in quiet_env(ssh) {
        cmd.env(k, v);
    }
}

/// Set [`quiet_env`] on a tokio command.
pub fn quiet_tokio(cmd: &mut tokio::process::Command, ssh: &SshSetup) {
    for (k, v) in quiet_env(ssh) {
        cmd.env(k, v);
    }
}

/// The line of git's stderr that says it was refused for want of a
/// sign-in (a key ssh would not offer, a credential the server refused,
/// one git was not allowed to ask for), if one does. A host key git could
/// not check, or a remote it could not read for another reason, is not.
pub fn needs_sign_in(stderr: &str) -> Option<&str> {
    const SAYS: &[&str] = &[
        "permission denied (publickey",
        "authentication failed",
        "could not read username",
        "could not read password",
        "terminal prompts disabled",
        "invalid username or password",
        "password authentication was removed",
        "the requested url returned error: 401",
        "the requested url returned error: 403",
    ];
    stderr.lines().map(str::trim).find(|l| {
        let l = l.to_ascii_lowercase();
        SAYS.iter().any(|s| l.contains(s))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env(ssh: &SshSetup) -> Vec<(&'static str, String)> {
        quiet_env(ssh)
    }

    fn ssh_of(ssh: &SshSetup) -> Option<String> {
        env(ssh)
            .into_iter()
            .find(|(k, _)| *k == "GIT_SSH_COMMAND")
            .map(|(_, v)| v)
    }

    #[test]
    fn never_prompts_and_puts_plain_ssh_in_batch_mode() {
        let e = env(&SshSetup::default());
        assert!(e.contains(&("GIT_TERMINAL_PROMPT", "0".into())));
        assert!(e.contains(&("GCM_INTERACTIVE", "never".into())));
        assert_eq!(
            ssh_of(&SshSetup::default()).as_deref(),
            Some("ssh -o BatchMode=yes")
        );
    }

    #[test]
    fn leaves_the_users_own_ssh_alone() {
        // An exported command or GIT_SSH (plink) is theirs: ours would
        // replace it.
        for s in [
            SshSetup {
                ssh_command: Some("ssh -i ~/.ssh/work".into()),
                ..Default::default()
            },
            SshSetup {
                git_ssh: Some("C:\\Program Files\\PuTTY\\plink.exe".into()),
                ..Default::default()
            },
        ] {
            let e = env(&s);
            assert_eq!(ssh_of(&s), None, "{s:?}");
            assert!(e.contains(&("GCM_INTERACTIVE", "never".into())));
            assert!(e.contains(&("GIT_TERMINAL_PROMPT", "0".into())));
        }
    }

    #[test]
    fn appends_batch_mode_to_an_openssh_core_ssh_command_only() {
        let core = |c: &str| SshSetup {
            core_ssh_command: Some(c.into()),
            ..Default::default()
        };
        assert_eq!(
            ssh_of(&core("C:/Windows/System32/OpenSSH/ssh.exe")).as_deref(),
            Some("C:/Windows/System32/OpenSSH/ssh.exe -o BatchMode=yes")
        );
        assert_eq!(
            ssh_of(&core("\"C:\\Program Files\\Git\\usr\\bin\\ssh.exe\" -i k")).as_deref(),
            Some("\"C:\\Program Files\\Git\\usr\\bin\\ssh.exe\" -i k -o BatchMode=yes")
        );
        assert_eq!(ssh_of(&core("plink -batch")), None);
        assert_eq!(ssh_of(&core("/usr/local/bin/my-ssh-wrapper")), None);
        assert_eq!(ssh_of(&core("ssh -o BatchMode=no")), None);
        assert_eq!(ssh_of(&core("  ")).as_deref(), Some("ssh -o BatchMode=yes"));
    }

    #[test]
    fn knows_the_commands_that_reach_a_remote() {
        assert!(is_network(&["push", "--set-upstream", "origin", "b"]));
        assert!(is_network(&[
            "-c",
            "credential.interactive=never",
            "fetch",
            "origin"
        ]));
        assert!(!is_network(&["remote", "get-url", "origin"]));
        assert!(!is_network(&["-c", "push.default=simple", "status"]));
        assert!(!is_network(&[]));
    }

    #[test]
    fn reads_a_refused_sign_in_and_nothing_else() {
        let ssh = "git@github.com: Permission denied (publickey).\nfatal: Could not read from remote repository.\n";
        assert_eq!(
            needs_sign_in(ssh),
            Some("git@github.com: Permission denied (publickey).")
        );
        for s in [
            "fatal: Authentication failed for 'https://dev.azure.com/o/p/_git/r/'",
            "fatal: could not read Username for 'https://github.com': terminal prompts disabled",
            "remote: Invalid username or password.",
            "fatal: unable to access 'https://x/': The requested URL returned error: 403",
        ] {
            assert!(needs_sign_in(s).is_some(), "{s}");
        }
        for s in [
            "Host key verification failed.\nfatal: Could not read from remote repository.",
            "fatal: Could not read from remote repository.",
            "! [rejected]        b -> b (non-fast-forward)",
            "fatal: unable to access 'https://x/': Could not resolve host: x",
        ] {
            assert_eq!(needs_sign_in(s), None, "{s}");
        }
    }
}
