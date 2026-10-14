//! `ado.json`: per-repo ADO state (branch to task links, recent reviewers,
//! team, a small cache, UI bits) and each org's tenant. Never holds a
//! token.
//!
//! The desktop window and `kete ado` each keep an `AdoStore` on the same
//! file, so every write is a read-modify-write under the settings lock
//! (`ado.json.lock`, as `nebula_core::settings::lock` takes for the other
//! settings files): the file is read again, the change applied to that
//! copy, and the result written through a temp file of this writer's own.
//! Reads pick up another writer's change when the file's modified time or
//! length moved since this store last saw it.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::SystemTime;

/// Most cache entries kept per repo.
pub const CACHE_MAX: usize = 500;
/// Most recent reviewers kept per repo.
pub const REVIEWERS_MAX: usize = 12;

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct RepoKey {
    pub org: String,
    pub project: String,
    pub repo: String,
}

impl fmt::Display for RepoKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}/{}/{}", self.org, self.project, self.repo)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Reviewer {
    pub id: String,
    pub name: String,
    pub email: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CacheEntry {
    /// Unix milliseconds when stored.
    pub at: i64,
    pub value: serde_json::Value,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct RepoState {
    /// Branch name to work item id.
    pub branch_tasks: BTreeMap<String, u32>,
    /// Newest first, deduped by id.
    pub recent_reviewers: Vec<Reviewer>,
    pub team: Option<String>,
    pub cache: BTreeMap<String, CacheEntry>,
    pub ui: serde_json::Value,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct StoreData {
    /// Keyed by `org/project/repo`.
    pub repos: BTreeMap<String, RepoState>,
    /// The Entra tenant per org URL, learned from the org's OAuth tokens
    /// and passed to `az`.
    pub tenants: BTreeMap<String, String>,
}

pub struct AdoStore {
    path: PathBuf,
    data: Mutex<StoreData>,
    /// The file's modified time and length when this store last read or
    /// wrote it, so a read knows when another writer has changed it.
    seen: Mutex<Option<Stamp>>,
    /// The last failed load or write, for a command to surface.
    last_error: Mutex<Option<String>>,
    /// Set when a corrupt file could not be backed up: the store then
    /// never writes, so the only copy of that file is not overwritten.
    hold: Option<String>,
}

impl AdoStore {
    /// `<kete data dir>/ado.json`.
    pub fn default_path() -> PathBuf {
        nebula_core::paths::data_dir().join("ado.json")
    }

    /// Load `path`. A missing file is empty. A corrupt one is moved to
    /// `ado.json.bad` and the store starts empty; if that move fails, the
    /// store keeps its data in memory only and records the error. Any
    /// other read failure (locked, denied, a passing I/O error) holds the
    /// same way: the file may be fine, so it is never overwritten.
    pub fn open(path: impl AsRef<Path>) -> Self {
        let path = path.as_ref().to_path_buf();
        let mut hold = None;
        let data = match std::fs::read(&path) {
            Ok(bytes) => serde_json::from_slice(&bytes).unwrap_or_else(|_| {
                let bad = sibling(&path, ".bad");
                if let Err(e) = std::fs::rename(&path, &bad) {
                    hold = Some(format!(
                        "{} is unreadable and could not be backed up to {}: {e}. Changes are kept in memory only.",
                        path.display(),
                        bad.display()
                    ));
                }
                StoreData::default()
            }),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => StoreData::default(),
            Err(e) => {
                hold = Some(format!(
                    "{} could not be read: {e}. Changes are kept in memory only.",
                    path.display()
                ));
                StoreData::default()
            }
        };
        let seen = stamp(&path);
        Self {
            path,
            data: Mutex::new(data),
            seen: Mutex::new(seen),
            last_error: Mutex::new(hold.clone()),
            hold,
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// The last load or write error, cleared by reading it. The window
    /// reads it through `ado_store_error`.
    pub fn take_last_error(&self) -> Option<String> {
        self.last_error
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .take()
    }

    /// Run `f` on the repo's state, then write the file atomically. A
    /// failed write keeps the change in memory and is recorded for
    /// [`take_last_error`](Self::take_last_error).
    pub fn with_repo<R>(&self, key: &RepoKey, f: impl FnOnce(&mut RepoState) -> R) -> R {
        self.mutate(key, f).0
    }

    /// As [`with_repo`](Self::with_repo), but a failed write is returned.
    /// The change stays in memory either way.
    pub fn try_with_repo<R>(
        &self,
        key: &RepoKey,
        f: impl FnOnce(&mut RepoState) -> R,
    ) -> std::io::Result<R> {
        let (out, res) = self.mutate(key, f);
        res.map(|()| out)
    }

    /// Read the repo's state without writing, and without adding an entry
    /// for a repo the store has never seen.
    pub fn read_repo<R>(&self, key: &RepoKey, f: impl FnOnce(&RepoState) -> R) -> R {
        let mut data = self.data.lock().unwrap_or_else(|p| p.into_inner());
        self.refresh(&mut data);
        match data.repos.get(&key.to_string()) {
            Some(state) => f(state),
            None => f(&RepoState::default()),
        }
    }

    /// Every org's remembered tenant, by org URL.
    pub fn tenants(&self) -> std::collections::HashMap<String, String> {
        let mut data = self.data.lock().unwrap_or_else(|p| p.into_inner());
        self.refresh(&mut data);
        data.tenants
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect()
    }

    /// Remember the org's tenant and write the file. A failed write is
    /// recorded for [`take_last_error`](Self::take_last_error).
    pub fn set_tenant(&self, org_url: &str, tenant: &str) {
        let _ = self.mutate_data(|d| {
            d.tenants.insert(org_url.to_string(), tenant.to_string());
        });
    }

    fn mutate<R>(
        &self,
        key: &RepoKey,
        f: impl FnOnce(&mut RepoState) -> R,
    ) -> (R, std::io::Result<()>) {
        self.mutate_data(|d| f(d.repos.entry(key.to_string()).or_default()))
    }

    /// Take another writer's change into `data` when the file moved since
    /// this store last saw it. A file that is missing, unreadable or does
    /// not parse leaves memory as it is.
    fn refresh(&self, data: &mut StoreData) {
        if self.hold.is_some() {
            return;
        }
        let mut seen = self.seen.lock().unwrap_or_else(|p| p.into_inner());
        let now = stamp(&self.path);
        if now.is_none() || now == *seen {
            return;
        }
        if let Disk::Data(fresh, at) = read_disk(&self.path) {
            *data = fresh;
            *seen = at;
        }
    }

    /// Under the file lock: read the file again, apply `f` to that copy,
    /// write it. Anything that stops the write keeps the change in memory
    /// and is recorded for [`take_last_error`](Self::take_last_error).
    fn mutate_data<R>(&self, f: impl FnOnce(&mut StoreData) -> R) -> (R, std::io::Result<()>) {
        let mut data = self.data.lock().unwrap_or_else(|p| p.into_inner());
        let (out, res) = match &self.hold {
            Some(why) => (f(&mut data), Err(why.clone())),
            None => self.locked_write(&mut data, f),
        };
        let res = res.map_err(|why| {
            *self.last_error.lock().unwrap_or_else(|p| p.into_inner()) = Some(why.clone());
            std::io::Error::other(why)
        });
        (out, res)
    }

    fn locked_write<R>(
        &self,
        data: &mut StoreData,
        f: impl FnOnce(&mut StoreData) -> R,
    ) -> (R, Result<(), String>) {
        let unsaved = |e: &dyn fmt::Display| {
            format!(
                "Could not save {}: {e}. The change is kept until Kete closes.",
                self.path.display()
            )
        };
        let _lock = match nebula_core::settings::lock(&self.path) {
            Ok(lock) => lock,
            Err(e) => return (f(data), Err(unsaved(&e))),
        };
        match read_disk(&self.path) {
            Disk::Data(fresh, _) => *data = fresh,
            Disk::Missing => {}
            Disk::Unreadable(why) => return (f(data), Err(unsaved(&why))),
        }
        let out = f(data);
        let res = serde_json::to_vec_pretty(&*data)
            .map_err(std::io::Error::other)
            .and_then(|bytes| nebula_core::settings::write_atomic(&self.path, &bytes))
            .map_err(|e| unsaved(&e));
        if res.is_ok() {
            *self.seen.lock().unwrap_or_else(|p| p.into_inner()) = stamp(&self.path);
        }
        (out, res)
    }

    /// Put `r` first, dropping any older entry with the same id, keeping
    /// at most [`REVIEWERS_MAX`].
    pub fn remember_reviewer(&self, key: &RepoKey, r: Reviewer) {
        self.with_repo(key, |s| {
            s.recent_reviewers.retain(|x| x.id != r.id);
            s.recent_reviewers.insert(0, r);
            s.recent_reviewers.truncate(REVIEWERS_MAX);
        });
    }

    /// Store `value` under `name`, evicting the oldest entries past
    /// [`CACHE_MAX`].
    pub fn cache_put(&self, key: &RepoKey, name: &str, value: serde_json::Value) {
        self.with_repo(key, |s| {
            // Strictly increasing, so "oldest" is well defined even when
            // many puts land in the same millisecond.
            let newest = s.cache.values().map(|e| e.at).max().unwrap_or(i64::MIN);
            let at = now_ms().max(newest.saturating_add(1));
            s.cache.insert(name.to_string(), CacheEntry { at, value });
            while s.cache.len() > CACHE_MAX {
                let oldest = s
                    .cache
                    .iter()
                    .min_by_key(|(_, e)| e.at)
                    .map(|(k, _)| k.clone());
                match oldest {
                    Some(k) => s.cache.remove(&k),
                    None => break,
                };
            }
        });
    }
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// `ado.json` plus `suffix`, e.g. `ado.json.bad`.
fn sibling(path: &Path, suffix: &str) -> PathBuf {
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(suffix);
    path.with_file_name(name)
}

/// A file's modified time and length: what tells a reader it changed.
type Stamp = (SystemTime, u64);

fn stamp(path: &Path) -> Option<Stamp> {
    let meta = std::fs::metadata(path).ok()?;
    Some((meta.modified().ok()?, meta.len()))
}

/// The file as it is on disk now.
enum Disk {
    Missing,
    Data(StoreData, Option<Stamp>),
    /// There but not readable or not JSON: never written over.
    Unreadable(String),
}

fn read_disk(path: &Path) -> Disk {
    let at = stamp(path);
    match std::fs::read(path) {
        Ok(bytes) => match serde_json::from_slice(&bytes) {
            Ok(data) => Disk::Data(data, at),
            Err(e) => Disk::Unreadable(format!("{} does not parse ({e})", path.display())),
        },
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Disk::Missing,
        Err(e) => Disk::Unreadable(format!("{} could not be read ({e})", path.display())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    fn temp_dir() -> PathBuf {
        static N: AtomicUsize = AtomicUsize::new(0);
        let dir = std::env::temp_dir().join(format!(
            "kete-ado-store-{}-{}",
            std::process::id(),
            N.fetch_add(1, Ordering::SeqCst)
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn key() -> RepoKey {
        RepoKey {
            org: "tnt".into(),
            project: "Platform".into(),
            repo: "api".into(),
        }
    }

    fn reviewer(i: usize) -> Reviewer {
        Reviewer {
            id: format!("id-{i}"),
            name: format!("Person {i}"),
            email: format!("p{i}@example.com"),
        }
    }

    fn other_key() -> RepoKey {
        RepoKey {
            org: "tnt".into(),
            project: "Platform".into(),
            repo: "web".into(),
        }
    }

    /// Windows ignores the read-only flag on directories, so the write is
    /// made to fail portably by putting a plain file where the parent
    /// directory should be.
    #[test]
    fn write_failure_is_recorded_and_returned() {
        let dir = temp_dir();
        std::fs::write(dir.join("blocker"), "x").unwrap();
        let s = AdoStore::open(dir.join("blocker").join("ado.json"));
        s.with_repo(&key(), |r| r.team = Some("a".into()));
        assert!(s.take_last_error().is_some());
        assert!(s.take_last_error().is_none());
        assert!(s
            .try_with_repo(&key(), |r| r.team = Some("b".into()))
            .is_err());
        assert!(s.take_last_error().is_some());
        // The change is kept in memory.
        assert_eq!(
            s.read_repo(&key(), |r| r.team.clone()).as_deref(),
            Some("b")
        );
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn failed_backup_never_overwrites_the_corrupt_file() {
        let dir = temp_dir();
        let path = dir.join("ado.json");
        std::fs::write(&path, "{ not json").unwrap();
        // A directory in the way makes the backup rename fail.
        std::fs::create_dir(dir.join("ado.json.bad")).unwrap();
        let s = AdoStore::open(&path);
        assert!(s.take_last_error().is_some());
        assert!(s
            .try_with_repo(&key(), |r| r.team = Some("a".into()))
            .is_err());
        s.with_repo(&key(), |r| r.team = Some("b".into()));
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "{ not json");
        assert_eq!(
            s.read_repo(&key(), |r| r.team.clone()).as_deref(),
            Some("b")
        );
        let _ = std::fs::remove_dir_all(dir);
    }

    /// A directory where the file should be: reading fails with something
    /// other than NotFound, on every platform.
    #[test]
    fn a_failed_read_holds_instead_of_wiping_the_file() {
        let dir = temp_dir();
        let path = dir.join("ado.json");
        std::fs::create_dir(&path).unwrap();
        std::fs::write(path.join("keep"), "x").unwrap();
        let s = AdoStore::open(&path);
        let why = s.take_last_error().expect("the read failure is surfaced");
        assert!(why.contains("could not be read"), "{why}");
        assert!(s
            .try_with_repo(&key(), |r| r.team = Some("a".into()))
            .is_err());
        // Memory only: the change is there, the path is untouched.
        assert_eq!(
            s.read_repo(&key(), |r| r.team.clone()).as_deref(),
            Some("a")
        );
        assert!(path.is_dir());
        assert!(path.join("keep").exists());
        assert!(!dir.join("ado.json.bad").exists());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn read_repo_neither_persists_nor_inserts() {
        let dir = temp_dir();
        let path = dir.join("ado.json");
        let s = AdoStore::open(&path);
        assert!(s.read_repo(&key(), |r| r.team.is_none()));
        assert!(!path.exists());
        s.with_repo(&key(), |r| r.team = Some("a".into()));
        std::fs::remove_file(&path).unwrap();
        assert_eq!(
            s.read_repo(&key(), |r| r.team.clone()).as_deref(),
            Some("a")
        );
        assert!(s.read_repo(&other_key(), |r| r.team.is_none()));
        assert!(!path.exists());
        assert_eq!(s.data.lock().unwrap().repos.len(), 1);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn key_display() {
        assert_eq!(key().to_string(), "tnt/Platform/api");
    }

    #[test]
    fn round_trips_through_the_file() {
        let dir = temp_dir();
        let path = dir.join("ado.json");
        let s = AdoStore::open(&path);
        s.with_repo(&key(), |r| {
            r.branch_tasks.insert("jari/gst".into(), 4211);
            r.team = Some("Platform Team".into());
        });
        s.set_tenant("https://dev.azure.com/tnt", "t-1");
        let again = AdoStore::open(&path);
        assert_eq!(
            again
                .tenants()
                .get("https://dev.azure.com/tnt")
                .map(String::as_str),
            Some("t-1")
        );
        let (task, team) = again.with_repo(&key(), |r| {
            (r.branch_tasks.get("jari/gst").copied(), r.team.clone())
        });
        assert_eq!(task, Some(4211));
        assert_eq!(team.as_deref(), Some("Platform Team"));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn atomic_write_leaves_no_tmp() {
        let dir = temp_dir();
        let path = dir.join("ado.json");
        let s = AdoStore::open(&path);
        s.with_repo(&key(), |r| r.team = Some("a".into()));
        s.with_repo(&key(), |r| r.team = Some("b".into()));
        assert!(path.exists());
        let leftovers: Vec<_> = std::fs::read_dir(&dir)
            .unwrap()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|n| n != "ado.json")
            .collect();
        assert!(leftovers.is_empty(), "{leftovers:?}");
        let _ = std::fs::remove_dir_all(dir);
    }

    /// The desktop window and `kete ado` each hold an `AdoStore` on the
    /// same file: neither's stale copy may overwrite the other's write.
    #[test]
    fn two_writers_on_one_file_keep_each_others_changes() {
        let dir = temp_dir();
        let path = dir.join("ado.json");
        let desktop = AdoStore::open(&path);
        let cli = AdoStore::open(&path);
        desktop.with_repo(&key(), |r| {
            r.branch_tasks.insert("jari/gst".into(), 4211);
        });
        cli.set_tenant("https://dev.azure.com/tnt", "t-1");
        cli.with_repo(&other_key(), |r| r.team = Some("Web".into()));
        desktop.with_repo(&key(), |r| r.team = Some("Platform".into()));
        let again = AdoStore::open(&path);
        assert_eq!(
            again.read_repo(&key(), |r| r.branch_tasks.get("jari/gst").copied()),
            Some(4211)
        );
        assert_eq!(
            again.read_repo(&key(), |r| r.team.clone()).as_deref(),
            Some("Platform")
        );
        assert_eq!(
            again.read_repo(&other_key(), |r| r.team.clone()).as_deref(),
            Some("Web")
        );
        assert_eq!(
            again
                .tenants()
                .get("https://dev.azure.com/tnt")
                .map(String::as_str),
            Some("t-1")
        );
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn two_writers_racing_lose_nothing() {
        let dir = temp_dir();
        let path = dir.join("ado.json");
        let a = std::sync::Arc::new(AdoStore::open(&path));
        let b = std::sync::Arc::new(AdoStore::open(&path));
        let run = |s: std::sync::Arc<AdoStore>, prefix: &'static str| {
            std::thread::spawn(move || {
                for i in 0..20 {
                    s.try_with_repo(&key(), |r| {
                        r.branch_tasks.insert(format!("{prefix}/{i}"), i);
                    })
                    .unwrap();
                }
            })
        };
        let (ta, tb) = (run(a, "a"), run(b, "b"));
        ta.join().unwrap();
        tb.join().unwrap();
        let again = AdoStore::open(&path);
        assert_eq!(again.read_repo(&key(), |r| r.branch_tasks.len()), 40);
        let _ = std::fs::remove_dir_all(dir);
    }

    /// A reader sees what another process wrote since it opened the file.
    #[test]
    fn a_read_picks_up_another_writers_change() {
        let dir = temp_dir();
        let path = dir.join("ado.json");
        let desktop = AdoStore::open(&path);
        assert!(desktop.read_repo(&key(), |r| r.team.is_none()));
        let cli = AdoStore::open(&path);
        cli.with_repo(&key(), |r| r.team = Some("Platform".into()));
        cli.set_tenant("https://dev.azure.com/tnt", "t-1");
        assert_eq!(
            desktop.read_repo(&key(), |r| r.team.clone()).as_deref(),
            Some("Platform")
        );
        assert!(desktop.tenants().contains_key("https://dev.azure.com/tnt"));
        let _ = std::fs::remove_dir_all(dir);
    }

    /// A file another writer left unparseable is not replaced by a write.
    #[test]
    fn a_write_never_replaces_a_file_gone_bad_since_open() {
        let dir = temp_dir();
        let path = dir.join("ado.json");
        let s = AdoStore::open(&path);
        s.with_repo(&key(), |r| r.team = Some("a".into()));
        std::fs::write(&path, "{ not json").unwrap();
        assert!(s
            .try_with_repo(&key(), |r| r.team = Some("b".into()))
            .is_err());
        assert!(s.take_last_error().is_some());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "{ not json");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn reviewers_dedupe_newest_first_capped() {
        let dir = temp_dir();
        let s = AdoStore::open(dir.join("ado.json"));
        for i in 0..15 {
            s.remember_reviewer(&key(), reviewer(i));
        }
        s.remember_reviewer(&key(), reviewer(3));
        let ids: Vec<String> = s.with_repo(&key(), |r| {
            r.recent_reviewers.iter().map(|r| r.id.clone()).collect()
        });
        assert_eq!(ids.len(), 12);
        assert_eq!(ids[0], "id-3");
        assert_eq!(ids.iter().filter(|i| *i == "id-3").count(), 1);
        assert_eq!(ids[1], "id-14");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn cache_caps_at_500_evicting_the_oldest() {
        let dir = temp_dir();
        let s = AdoStore::open(dir.join("ado.json"));
        for i in 0..501 {
            s.cache_put(&key(), &format!("k{i}"), serde_json::json!(i));
        }
        s.with_repo(&key(), |r| {
            assert_eq!(r.cache.len(), 500);
            assert!(!r.cache.contains_key("k0"));
            assert!(r.cache.contains_key("k1"));
            assert!(r.cache.contains_key("k500"));
        });
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn corrupt_file_opens_empty_and_is_backed_up() {
        let dir = temp_dir();
        let path = dir.join("ado.json");
        std::fs::write(&path, "{ not json").unwrap();
        let s = AdoStore::open(&path);
        assert!(s.with_repo(&key(), |r| r.branch_tasks.is_empty()));
        assert_eq!(
            std::fs::read_to_string(dir.join("ado.json.bad")).unwrap(),
            "{ not json"
        );
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn missing_file_opens_empty_without_a_backup() {
        let dir = temp_dir();
        let s = AdoStore::open(dir.join("ado.json"));
        assert!(s.with_repo(&key(), |r| r.team.is_none()));
        assert!(!dir.join("ado.json.bad").exists());
        let _ = std::fs::remove_dir_all(dir);
    }
}
