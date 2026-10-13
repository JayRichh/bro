use crate::env;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

/// The settings files' names in the data dir. Their loaders, the SETTINGS
/// BUNDLE and `nebula config path` all spell them from here.
pub const CONFIG_FILE_NAME: &str = "config.json";
pub const CONFIG_LOCAL_FILE_NAME: &str = "config.local.json";
pub const PRESETS_FILE_NAME: &str = "agent_presets.json";
pub const SSH_HOSTS_FILE_NAME: &str = "ssh_hosts.json";

/// Where the runtime dir lands when neither override nor `XDG_RUNTIME_DIR`
/// is set. World-writable, so the per-uid subdir is what carries mode 0700.
#[cfg(unix)]
const FALLBACK_RUNTIME_ROOT: &str = "/tmp";

/// Runtime dir holding the socket + pidfile. Mode 0700 — this is the auth
/// boundary, same model as tmux. `NEBULA_RUNTIME_DIR` overrides (tests,
/// parallel instances).
#[cfg(unix)]
pub fn runtime_dir() -> PathBuf {
    if let Some(dir) = env::non_empty(env::RUNTIME_DIR) {
        return PathBuf::from(dir);
    }
    if legacy_in_use() {
        return legacy_runtime_dir();
    }
    if let Some(dir) = xdg_runtime_dir() {
        return dir.join("kete");
    }
    fallback_runtime_dir()
}

/// Runtime dir holding the pidfile and buildstamp: `%LOCALAPPDATA%\kete\run`.
/// The daemon's endpoint is a named pipe, not a file in here, but its name
/// is derived from this dir ([`pipe_name`]), so `NEBULA_RUNTIME_DIR` still
/// isolates tests and parallel instances. The per-user profile is the
/// access boundary; there is no mode to set.
#[cfg(windows)]
pub fn runtime_dir() -> PathBuf {
    if let Some(dir) = env::non_empty(env::RUNTIME_DIR) {
        return PathBuf::from(dir);
    }
    windows_app_dir().join("run")
}

/// `%LOCALAPPDATA%\kete`, the root of everything the Windows build keeps.
/// Local rather than roaming: the database, logs and pidfile describe this
/// machine's sessions and must not follow a roaming profile elsewhere.
#[cfg(windows)]
fn windows_app_dir() -> PathBuf {
    if legacy_in_use() {
        return windows_local_dir().join(LEGACY_NAME);
    }
    windows_local_dir().join("kete")
}

/// `%LOCALAPPDATA%`.
#[cfg(windows)]
fn windows_local_dir() -> PathBuf {
    env::non_empty("LOCALAPPDATA")
        .map(PathBuf::from)
        .or_else(|| env::home_dir().map(|home| home.join("AppData").join("Local")))
        .unwrap_or_else(std::env::temp_dir)
}

/// Kete's old name. Its dirs are carried over by [`migrate_legacy_dirs`].
pub const LEGACY_NAME: &str = "bro";

/// Set when Bro's daemon is still up over Bro's dirs and they have not
/// moved yet: this process then uses those dirs (and that daemon's pipe)
/// rather than start a second daemon beside it.
static LEGACY_IN_USE: AtomicBool = AtomicBool::new(false);

fn legacy_in_use() -> bool {
    LEGACY_IN_USE.load(Ordering::Relaxed)
}

/// One of Bro's dirs and where Kete keeps it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LegacyDir {
    pub old: PathBuf,
    pub new: PathBuf,
    /// A file only a used dir has (the database, the pidfile). A `new`
    /// without it is fresh, made by something that ran before the move (a
    /// `kete setup terminal`, a test), and Bro's dir still moves into it.
    pub marker: &'static str,
}

impl LegacyDir {
    fn pending(&self) -> bool {
        self.old.is_dir() && !self.new.join(self.marker).exists()
    }

    /// Rename `old` to `new`; into a fresh `new` that exists already, each
    /// entry of `old` it lacks, and `old` goes once empty.
    fn carry_over(&self) -> std::io::Result<()> {
        if !self.new.exists() {
            if let Some(parent) = self.new.parent() {
                std::fs::create_dir_all(parent)?;
            }
            return std::fs::rename(&self.old, &self.new);
        }
        for entry in std::fs::read_dir(&self.old)? {
            let entry = entry?;
            let to = self.new.join(entry.file_name());
            if !to.exists() {
                std::fs::rename(entry.path(), to)?;
            }
        }
        let _ = std::fs::remove_dir(&self.old);
        Ok(())
    }
}

/// What [`migrate_dirs`] did.
#[derive(Debug, PartialEq, Eq)]
pub enum Migration {
    /// Nothing to move: no old dir, or the new one is in use already.
    Nothing,
    /// The old daemon holds its lock, so nothing moved.
    InUse,
    /// These dirs moved, and these failed to (with why).
    Moved {
        moved: Vec<LegacyDir>,
        failed: Vec<(LegacyDir, String)>,
    },
}

/// Carry over each of `dirs` still to move, unless the daemon lock at
/// `legacy_lock` is held (that daemon still has files open in them). The
/// first is the data dir: when it fails to move, nothing after it is
/// tried, so the data and runtime dirs never end up under different names.
pub fn migrate_dirs(dirs: &[LegacyDir], legacy_lock: &Path) -> Migration {
    let pending: Vec<&LegacyDir> = dirs.iter().filter(|d| d.pending()).collect();
    if pending.is_empty() {
        return Migration::Nothing;
    }
    if lock_held(legacy_lock) {
        return Migration::InUse;
    }
    let mut moved = Vec::new();
    let mut failed = Vec::new();
    for dir in pending {
        match dir.carry_over() {
            Ok(()) => moved.push(dir.clone()),
            Err(e) => {
                failed.push((dir.clone(), e.to_string()));
                if moved.is_empty() {
                    break;
                }
            }
        }
    }
    Migration::Moved { moved, failed }
}

/// Whether some process holds the daemon lock on `lockfile`. Opening and
/// trying it is the whole test; a file that isn't there is held by no one.
fn lock_held(lockfile: &Path) -> bool {
    let Ok(file) = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(lockfile)
    else {
        return false;
    };
    matches!(file.try_lock(), Err(std::fs::TryLockError::WouldBlock))
}

/// Kete was called Bro. On first start, move Bro's data and runtime dirs
/// to Kete's names, unless Kete's are in use already. While Bro's daemon
/// still runs over the old dirs, nothing moves: this process uses the old
/// dirs and reaches that daemon instead, and the move happens on a start
/// after it has stopped (`kete kill`, or a reboot). Skipped when either dir
/// is overridden (tests, parallel instances). Returns a line per dir moved
/// or left behind, for the caller to show once.
pub fn migrate_legacy_dirs() -> Vec<String> {
    if env::non_empty(env::DATA_DIR).is_some() || env::non_empty(env::RUNTIME_DIR).is_some() {
        return Vec::new();
    }
    let Some((dirs, legacy_run)) = legacy_dirs() else {
        return Vec::new();
    };
    // The daemon's lock: on Windows a file beside the pidfile, on unix the
    // pidfile itself.
    #[cfg(windows)]
    let lock = lock_path_for(&legacy_run.join("daemon.pid"));
    #[cfg(unix)]
    let lock = legacy_run.join("daemon.pid");
    match migrate_dirs(&dirs, &lock) {
        Migration::Nothing => Vec::new(),
        Migration::InUse => {
            // Only while the data has not moved: Kete's data dir, once in
            // use, wins.
            if dirs.first().is_some_and(LegacyDir::pending) {
                LEGACY_IN_USE.store(true, Ordering::Relaxed);
            }
            Vec::new()
        }
        Migration::Moved { moved, failed } => {
            if moved.is_empty() {
                // The data stayed put, so keep reading it where it is.
                LEGACY_IN_USE.store(true, Ordering::Relaxed);
            }
            let mut notes: Vec<String> = moved
                .iter()
                .map(|d| format!("moved {} to {}", d.old.display(), d.new.display()))
                .collect();
            notes.extend(failed.iter().map(|(d, err)| {
                format!(
                    "couldn't move {} ({err}); using it where it is",
                    d.old.display()
                )
            }));
            notes
        }
    }
}

/// Bro's dirs beside Kete's, data dir first, and Bro's runtime dir (where
/// its daemon's lock lives).
#[cfg(windows)]
fn legacy_dirs() -> Option<(Vec<LegacyDir>, PathBuf)> {
    let old = windows_local_dir().join(LEGACY_NAME);
    let new = windows_local_dir().join("kete");
    let dirs = vec![
        LegacyDir {
            old: old.join("data"),
            new: new.join("data"),
            marker: "nebula.db",
        },
        LegacyDir {
            old: old.join("run"),
            new: new.join("run"),
            marker: "daemon.pid",
        },
    ];
    Some((dirs, old.join("run")))
}

#[cfg(unix)]
fn legacy_dirs() -> Option<(Vec<LegacyDir>, PathBuf)> {
    let old = legacy_project_dirs()?;
    let new = project_dirs()?;
    let mut dirs = vec![LegacyDir {
        old: old.data_dir().to_path_buf(),
        new: new.data_dir().to_path_buf(),
        marker: "nebula.db",
    }];
    if let (Some(old_state), Some(new_state)) = (old.state_dir(), new.state_dir()) {
        dirs.push(LegacyDir {
            old: old_state.to_path_buf(),
            new: new_state.to_path_buf(),
            marker: "daemon.log",
        });
    }
    let new_run = match xdg_runtime_dir() {
        Some(dir) => dir.join("kete"),
        None => fallback_runtime_dir(),
    };
    dirs.push(LegacyDir {
        old: legacy_runtime_dir(),
        new: new_run,
        marker: "daemon.pid",
    });
    Some((dirs, legacy_runtime_dir()))
}

/// Bro's runtime dir: `$XDG_RUNTIME_DIR/bro`, else `/tmp/bro-<uid>`.
#[cfg(unix)]
fn legacy_runtime_dir() -> PathBuf {
    match xdg_runtime_dir() {
        Some(dir) => dir.join(LEGACY_NAME),
        None => Path::new(FALLBACK_RUNTIME_ROOT).join(format!("{LEGACY_NAME}-{}", libc_geteuid())),
    }
}

#[cfg(unix)]
fn legacy_project_dirs() -> Option<directories::ProjectDirs> {
    directories::ProjectDirs::from("dev", LEGACY_NAME, LEGACY_NAME)
}

/// The named pipe the daemon listens on for `runtime_dir`, as `user`:
/// `\\.\pipe\kete-<user>-<hash>`. Pipe names are machine-global, so the user
/// keeps two accounts apart and the hash of the runtime dir keeps apart two
/// instances one user runs (`NEBULA_RUNTIME_DIR`). The hash is FNV-1a over
/// the path's bytes: fixed across builds and toolchains, so a client and a
/// daemon from different builds still agree on the name.
pub fn pipe_name_for(user: &str, runtime_dir: &Path) -> String {
    const PRIME: u64 = 0x100_0000_01b3;
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in runtime_dir.to_string_lossy().bytes() {
        hash = (hash ^ u64::from(byte)).wrapping_mul(PRIME);
    }
    // Pipe names take any character but `\`; keep the user part tame anyway
    // so the name reads cleanly in tools that list pipes.
    let user: String = user
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    // Bro's daemon, still up over Bro's dirs, listens under Bro's prefix.
    let app = if legacy_in_use() { LEGACY_NAME } else { "kete" };
    format!(r"\\.\pipe\{app}-{user}-{:08x}", hash as u32)
}

/// [`pipe_name_for`] the current user and a socket path: the pipe stands in
/// for the socket that would sit at `socket`, so its dir picks the pipe.
pub fn pipe_name_for_socket(socket: &Path) -> String {
    let user = env::non_empty("USERNAME")
        .or_else(|| env::non_empty("USER"))
        .unwrap_or_else(|| "user".into());
    pipe_name_for(&user, socket.parent().unwrap_or(Path::new("")))
}

/// The daemon's named pipe for this user and [`runtime_dir`].
pub fn pipe_name() -> String {
    pipe_name_for_socket(&socket_path())
}

#[cfg(unix)]
fn xdg_runtime_dir() -> Option<PathBuf> {
    let dir = PathBuf::from(env::non_empty("XDG_RUNTIME_DIR")?);
    // XDG_RUNTIME_DIR is supposed to be created by the login manager before
    // applications start. WSL shells can inherit `/run/user/$UID` without
    // systemd/PAM having made it, and an unprivileged daemon cannot create
    // that parent under `/run`; use the same private /tmp fallback tmux-style
    // tools use when the XDG dir is absent.
    dir.is_dir().then_some(dir)
}

#[cfg(unix)]
fn fallback_runtime_dir() -> PathBuf {
    let uid = libc_geteuid();
    Path::new(FALLBACK_RUNTIME_ROOT).join(format!("kete-{uid}"))
}

// Avoid a libc dependency in this dep-light crate for one call.
#[cfg(unix)]
fn libc_geteuid() -> u32 {
    extern "C" {
        fn geteuid() -> u32;
    }
    unsafe { geteuid() }
}

pub fn socket_path() -> PathBuf {
    runtime_dir().join("daemon.sock")
}

pub fn pidfile_path() -> PathBuf {
    runtime_dir().join("daemon.pid")
}

/// `std::fs::canonicalize`, minus the verbatim prefix Windows puts on the
/// result (`\\?\C:\repo`, `\\?\UNC\host\share`). Paths nebula stores and
/// compares come from git, hook payloads and the user as well, all in the
/// plain form, and a verbatim path equals none of them.
pub fn canonical(path: &Path) -> std::io::Result<PathBuf> {
    let canon = std::fs::canonicalize(path)?;
    #[cfg(windows)]
    {
        let text = canon.to_string_lossy();
        if let Some(rest) = text.strip_prefix(r"\\?\UNC\") {
            return Ok(PathBuf::from(format!(r"\\{rest}")));
        }
        if let Some(rest) = text.strip_prefix(r"\\?\") {
            let bytes = rest.as_bytes();
            if bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':' {
                return Ok(PathBuf::from(rest));
            }
        }
    }
    Ok(canon)
}

/// The file a Windows daemon holds its lock on, beside `pidfile`. Windows
/// locks are mandatory — a locked file cannot be read through another
/// handle — so the lock lives apart from the pid clients read.
pub fn lock_path_for(pidfile: &Path) -> PathBuf {
    pidfile.with_extension("lock")
}

/// Fingerprint of the binary the running daemon was launched from, written
/// by the daemon at startup. Installers compare it against the binary they
/// just installed to tell an up-to-date daemon from a stale one.
pub fn buildstamp_path() -> PathBuf {
    runtime_dir().join("daemon.build")
}

/// `<pid> <version>` of a daemon that can restart in place onto a new
/// binary, keeping its sessions. A daemon from before IN-PLACE RESTARTS
/// never writes it, which is how a client knows not to ask.
pub fn restart_capability_path() -> PathBuf {
    runtime_dir().join("daemon.restart")
}

/// A client's request for an IN-PLACE RESTART: which binary to restart
/// onto. The daemon reads and removes it when signalled.
pub fn restart_request_path() -> PathBuf {
    runtime_dir().join("restart.request")
}

/// How the last IN-PLACE RESTART went, written by whichever image of the
/// daemon finished it: the old one on a refusal, the new one on success.
pub fn restart_result_path() -> PathBuf {
    runtime_dir().join("restart.result")
}

/// The live sessions an IN-PLACE RESTART carries across the exec, read
/// and removed by the new image as it boots.
pub fn restart_state_path() -> PathBuf {
    runtime_dir().join("restart.state")
}

/// The platform's per-user dirs for this app (`~/Library/Application
/// Support/dev.nebula.nebula` on macOS, `~/.local/share/nebula` on Linux).
#[cfg(unix)]
fn project_dirs() -> Option<directories::ProjectDirs> {
    if legacy_in_use() {
        return legacy_project_dirs();
    }
    directories::ProjectDirs::from("dev", "kete", "kete")
}

#[cfg(unix)]
pub fn data_dir() -> PathBuf {
    if let Some(dir) = env::non_empty(env::DATA_DIR) {
        return PathBuf::from(dir);
    }
    project_dirs()
        .map(|d| d.data_dir().to_path_buf())
        .unwrap_or_else(|| env::home_dir().unwrap_or_default().join(".kete"))
}

/// `%LOCALAPPDATA%\kete\data`, beside the runtime dir, rather than the
/// roaming `%APPDATA%` dir `ProjectDirs` would pick: the database holds
/// this machine's sessions and checkouts.
#[cfg(windows)]
pub fn data_dir() -> PathBuf {
    if let Some(dir) = env::non_empty(env::DATA_DIR) {
        return PathBuf::from(dir);
    }
    windows_app_dir().join("data")
}

pub fn db_path() -> PathBuf {
    data_dir().join("nebula.db")
}

/// User settings file (JSON) — the portable layer, which backups and
/// `nebula ssh` carry. Lives beside the DB so `NEBULA_DATA_DIR` isolates it
/// for tests and parallel instances too; `NEBULA_CONFIG_FILE` moves this one
/// file alone (a leading `~/` is expanded, since a quoted value never is).
pub fn config_path() -> PathBuf {
    match env::non_empty(env::CONFIG_FILE) {
        Some(file) => match file.strip_prefix("~/") {
            Some(rest) => env::home_dir().unwrap_or_default().join(rest),
            None => PathBuf::from(file),
        },
        None => data_dir().join(CONFIG_FILE_NAME),
    }
}

/// This machine's settings, layered over [`config_path`] key by key: never
/// exported, forwarded or overwritten by an import. Stays in the data dir
/// when `NEBULA_CONFIG_FILE` moves the portable file.
pub fn config_local_path() -> PathBuf {
    data_dir().join(CONFIG_LOCAL_FILE_NAME)
}

/// Claude Code's config dir: `$CLAUDE_CONFIG_DIR`, else `~/.claude`.
pub fn claude_config_dir() -> Option<PathBuf> {
    env::non_empty(env::CLAUDE_CONFIG_DIR)
        .map(PathBuf::from)
        .or_else(|| env::home_dir().map(|home| home.join(".claude")))
}

/// Logs live under the data dir's `state` on Windows, which has no separate
/// state dir.
#[cfg(windows)]
pub fn log_dir() -> PathBuf {
    data_dir().join("state")
}

#[cfg(unix)]
pub fn log_dir() -> PathBuf {
    // Tests and parallel instances override the data dir; keep their logs
    // beside their data instead of the real user's state dir.
    if env::non_empty(env::DATA_DIR).is_some() {
        return data_dir().join("state");
    }
    project_dirs()
        .map(|d| {
            d.state_dir()
                .map(|s| s.to_path_buf())
                .unwrap_or_else(|| d.data_dir().join("state"))
        })
        .unwrap_or_else(|| data_dir().join("state"))
}

pub fn daemon_log_path() -> PathBuf {
    log_dir().join("daemon.log")
}

pub fn tui_log_path() -> PathBuf {
    log_dir().join("tui.log")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Restores an env var to what it was when the guard was made, so a
    /// failed assertion can't leak an override into the next test.
    struct EnvRestore(&'static str, Option<std::ffi::OsString>);
    impl EnvRestore {
        fn new(var: &'static str) -> Self {
            Self(var, std::env::var_os(var))
        }
    }
    impl Drop for EnvRestore {
        fn drop(&mut self) {
            match &self.1 {
                Some(v) => std::env::set_var(self.0, v),
                None => std::env::remove_var(self.0),
            }
        }
    }

    // One test owns both override vars: env is process-global, and splitting
    // this into several `#[test]`s would race them across threads.
    #[test]
    fn overrides_win_only_when_non_empty() {
        let _restore = (
            EnvRestore::new(crate::env::RUNTIME_DIR),
            EnvRestore::new(crate::env::DATA_DIR),
            EnvRestore::new(crate::env::CONFIG_FILE),
            EnvRestore::new("XDG_RUNTIME_DIR"),
        );
        std::env::remove_var("XDG_RUNTIME_DIR");
        std::env::remove_var(crate::env::CONFIG_FILE);
        let runtime = std::env::temp_dir().join(format!("nebula-paths-rt-{}", std::process::id()));
        let data = std::env::temp_dir().join(format!("nebula-paths-data-{}", std::process::id()));

        std::env::set_var(crate::env::RUNTIME_DIR, &runtime);
        std::env::set_var(crate::env::DATA_DIR, &data);
        assert_eq!(runtime_dir(), runtime);
        assert_eq!(socket_path(), runtime.join("daemon.sock"));
        assert_eq!(pidfile_path(), runtime.join("daemon.pid"));
        assert_eq!(buildstamp_path(), runtime.join("daemon.build"));
        assert_eq!(data_dir(), data);
        assert_eq!(db_path(), data.join("nebula.db"));
        assert_eq!(config_path(), data.join("config.json"));
        assert_eq!(config_local_path(), data.join("config.local.json"));
        // Logs follow the data override so isolated instances keep their
        // logs beside their state.
        assert_eq!(log_dir(), data.join("state"));
        assert_eq!(daemon_log_path(), data.join("state").join("daemon.log"));
        assert_eq!(tui_log_path(), data.join("state").join("tui.log"));

        // The config file override moves that one file; the local layer and
        // everything else stay in the data dir.
        let dotfile = std::env::temp_dir().join("dotfiles").join("nebula.json");
        std::env::set_var(crate::env::CONFIG_FILE, &dotfile);
        assert_eq!(config_path(), dotfile);
        assert_eq!(config_local_path(), data.join("config.local.json"));
        assert_eq!(db_path(), data.join("nebula.db"));
        std::env::set_var(crate::env::CONFIG_FILE, "~/dotfiles/nebula.json");
        assert_eq!(
            config_path(),
            crate::env::home_dir()
                .unwrap()
                .join("dotfiles")
                .join("nebula.json")
        );
        std::env::set_var(crate::env::CONFIG_FILE, "");
        assert_eq!(config_path(), data.join("config.json"));

        std::env::set_var(crate::env::RUNTIME_DIR, "");
        std::env::set_var(crate::env::DATA_DIR, "");
        assert_ne!(runtime_dir(), runtime, "empty override must fall through");
        assert_ne!(data_dir(), data, "empty override must fall through");
        assert_ne!(log_dir(), data.join("state"));
        assert!(
            runtime_dir().is_absolute() && data_dir().is_absolute(),
            "defaults are absolute: {} / {}",
            runtime_dir().display(),
            data_dir().display()
        );

        #[cfg(unix)]
        {
            let xdg = std::env::temp_dir().join(format!("nebula-paths-xdg-{}", std::process::id()));
            std::fs::create_dir_all(&xdg).unwrap();
            std::env::set_var("XDG_RUNTIME_DIR", &xdg);
            assert_eq!(runtime_dir(), xdg.join("kete"));

            std::fs::remove_dir_all(&xdg).unwrap();
            assert_eq!(runtime_dir(), fallback_runtime_dir());
        }
        #[cfg(windows)]
        {
            // Both defaults sit side by side under one `kete` dir.
            assert_eq!(runtime_dir().parent(), data_dir().parent());
            assert!(
                runtime_dir().ends_with(r"kete\run"),
                "{}",
                runtime_dir().display()
            );
            assert_eq!(log_dir(), data_dir().join("state"));
        }
    }

    #[cfg(windows)]
    #[test]
    fn canonical_paths_drop_the_verbatim_prefix() {
        let dir = tempfile::tempdir().unwrap();
        let canon = canonical(dir.path()).unwrap();
        assert!(
            !canon.to_string_lossy().starts_with(r"\\?\"),
            "{}",
            canon.display()
        );
        assert!(canon.is_absolute() && canon.is_dir());
        // The same directory spelled with git's `/` compares equal to it.
        let slashed = PathBuf::from(canon.to_string_lossy().replace('\\', "/"));
        assert_eq!(canonical(&slashed).unwrap(), canon);
    }

    #[test]
    fn pipe_name_is_per_user_and_per_runtime_dir() {
        let a = Path::new(r"C:\Users\jay\AppData\Local\kete\run");
        let b = Path::new(r"C:\Temp\test-run");
        let name = pipe_name_for("jay", a);
        assert!(name.starts_with(r"\\.\pipe\kete-jay-"), "{name}");
        assert_eq!(name.len(), r"\\.\pipe\kete-jay-".len() + 8, "{name}");
        // Stable for the same inputs, so client and daemon agree.
        assert_eq!(name, pipe_name_for("jay", a));
        assert_ne!(name, pipe_name_for("jay", b), "runtime dirs isolate");
        assert_ne!(name, pipe_name_for("ann", a), "users isolate");
        // A user name with characters a pipe listing would mangle.
        let odd = pipe_name_for(r"DOMAIN\Jay Smith", a);
        assert!(odd.starts_with(r"\\.\pipe\kete-DOMAIN_Jay_Smith-"), "{odd}");
        assert_eq!(odd.matches('\\').count(), 4, "only the prefix's: {odd}");
        // The socket path's dir picks the pipe: tests that put a socket in
        // a temp dir get a pipe of their own.
        assert_eq!(
            pipe_name_for_socket(&b.join("daemon.sock")),
            pipe_name_for_socket(&b.join("other.sock"))
        );
    }

    /// A Bro install's dirs laid out under `root`: `bro/data` holding the
    /// database and a profile, `bro/run` holding a pidfile and the lock.
    fn bro_install(root: &Path) -> Vec<LegacyDir> {
        let old = root.join(LEGACY_NAME);
        let new = root.join("kete");
        let profile = old.join("data").join("profiles").join("jayrich");
        std::fs::create_dir_all(&profile).unwrap();
        std::fs::write(profile.join(".claude.json"), b"{}").unwrap();
        std::fs::write(old.join("data").join("nebula.db"), b"db").unwrap();
        std::fs::create_dir_all(old.join("run")).unwrap();
        std::fs::write(old.join("run").join("daemon.pid"), b"42\n").unwrap();
        std::fs::write(old.join("run").join("daemon.lock"), b"").unwrap();
        vec![
            LegacyDir {
                old: old.join("data"),
                new: new.join("data"),
                marker: "nebula.db",
            },
            LegacyDir {
                old: old.join("run"),
                new: new.join("run"),
                marker: "daemon.pid",
            },
        ]
    }

    fn lock_of(root: &Path) -> PathBuf {
        root.join(LEGACY_NAME).join("run").join("daemon.lock")
    }

    #[test]
    fn bro_dirs_move_to_kete_once() {
        let root = tempfile::tempdir().unwrap();
        let dirs = bro_install(root.path());

        let Migration::Moved { moved, failed } = migrate_dirs(&dirs, &lock_of(root.path())) else {
            panic!("nothing moved");
        };
        assert_eq!(moved, dirs);
        assert!(failed.is_empty(), "{failed:?}");
        let kete = root.path().join("kete");
        let profile = kete.join("data").join("profiles").join("jayrich");
        assert!(profile.join(".claude.json").is_file());
        assert_eq!(
            std::fs::read(kete.join("data").join("nebula.db")).unwrap(),
            b"db"
        );
        assert!(kete.join("run").join("daemon.pid").is_file());
        assert!(!root.path().join(LEGACY_NAME).join("data").exists());

        // Moved is moved: the next start finds nothing to do.
        assert_eq!(
            migrate_dirs(&dirs, &lock_of(root.path())),
            Migration::Nothing
        );
    }

    /// Something that ran before the move made a Kete data dir with no
    /// database in it (a setup step's icon, a stray file): Bro's data still
    /// moves in, around what is there.
    #[test]
    fn bro_data_moves_into_a_fresh_kete_dir() {
        let root = tempfile::tempdir().unwrap();
        let dirs = bro_install(root.path());
        let data = root.path().join("kete").join("data");
        std::fs::create_dir_all(&data).unwrap();
        std::fs::write(data.join("kete-256.png"), b"png").unwrap();

        assert!(matches!(
            migrate_dirs(&dirs, &lock_of(root.path())),
            Migration::Moved { ref moved, .. } if moved.len() == 2
        ));
        assert!(data.join("nebula.db").is_file());
        assert!(data.join("profiles").join("jayrich").is_dir());
        assert_eq!(std::fs::read(data.join("kete-256.png")).unwrap(), b"png");
        assert!(!root.path().join(LEGACY_NAME).join("data").exists());
    }

    /// Kete's data dir in use (it has its database) is Kete's: Bro's is left
    /// alone, never merged into it.
    #[test]
    fn a_kete_dir_in_use_wins_and_bro_is_left_alone() {
        let root = tempfile::tempdir().unwrap();
        let dirs = bro_install(root.path());
        let data = root.path().join("kete").join("data");
        std::fs::create_dir_all(&data).unwrap();
        std::fs::write(data.join("nebula.db"), b"kete's").unwrap();

        // Only the runtime dir is still to move.
        let Migration::Moved { moved, .. } = migrate_dirs(&dirs, &lock_of(root.path())) else {
            panic!("the runtime dir should move");
        };
        assert_eq!(moved, dirs[1..]);
        assert_eq!(std::fs::read(data.join("nebula.db")).unwrap(), b"kete's");
        assert!(root
            .path()
            .join(LEGACY_NAME)
            .join("data")
            .join("profiles")
            .is_dir());
    }

    #[test]
    fn nothing_moves_while_the_bro_daemon_holds_its_lock() {
        let root = tempfile::tempdir().unwrap();
        let dirs = bro_install(root.path());
        let held = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(lock_of(root.path()))
            .unwrap();
        held.try_lock().unwrap();

        assert_eq!(migrate_dirs(&dirs, &lock_of(root.path())), Migration::InUse);
        assert!(root.path().join(LEGACY_NAME).join("data").is_dir());
        assert!(!root.path().join("kete").exists());

        // Once the daemon is gone, the next start moves them.
        drop(held);
        assert!(matches!(
            migrate_dirs(&dirs, &lock_of(root.path())),
            Migration::Moved { ref moved, .. } if moved.len() == 2
        ));
    }
}
