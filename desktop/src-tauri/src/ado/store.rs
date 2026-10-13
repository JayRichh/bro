//! `ado.json`: per-repo ADO state (branch to task links, recent reviewers,
//! team, a small cache, UI bits) and each org's tenant. Never holds a
//! token.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fmt;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

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

    /// Load `path`. A missing file is empty. An unreadable one is moved to
    /// `ado.json.bad` and the store starts empty; if that move fails, the
    /// store keeps its data in memory only and records the error.
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
            Err(_) => StoreData::default(),
        };
        Self {
            path,
            data: Mutex::new(data),
            last_error: Mutex::new(hold.clone()),
            hold,
        }
    }

    #[cfg(test)]
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
        let data = self.data.lock().unwrap_or_else(|p| p.into_inner());
        match data.repos.get(&key.to_string()) {
            Some(state) => f(state),
            None => f(&RepoState::default()),
        }
    }

    /// Every org's remembered tenant, by org URL.
    pub fn tenants(&self) -> std::collections::HashMap<String, String> {
        let data = self.data.lock().unwrap_or_else(|p| p.into_inner());
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

    fn mutate_data<R>(&self, f: impl FnOnce(&mut StoreData) -> R) -> (R, std::io::Result<()>) {
        let mut data = self.data.lock().unwrap_or_else(|p| p.into_inner());
        let out = f(&mut data);
        let res = match &self.hold {
            Some(why) => Err(std::io::Error::other(why.clone())),
            None => write_atomic(&self.path, &data),
        };
        if let Err(e) = &res {
            *self.last_error.lock().unwrap_or_else(|p| p.into_inner()) = Some(match &self.hold {
                Some(why) => why.clone(),
                None => format!(
                    "Could not save {}: {e}. The change is kept until Kete closes.",
                    self.path.display()
                ),
            });
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

/// `ado.json` plus `suffix`, e.g. `ado.json.tmp`.
fn sibling(path: &Path, suffix: &str) -> PathBuf {
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(suffix);
    path.with_file_name(name)
}

/// Write to `ado.json.tmp`, then rename over `ado.json`.
fn write_atomic(path: &Path, data: &StoreData) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = sibling(path, ".tmp");
    let bytes = serde_json::to_vec_pretty(data).map_err(std::io::Error::other)?;
    let written = (|| {
        let mut file = std::fs::File::create(&tmp)?;
        file.write_all(&bytes)?;
        file.sync_all()
    })();
    written
        .and_then(|()| std::fs::rename(&tmp, path))
        .inspect_err(|_| {
            let _ = std::fs::remove_file(&tmp);
        })
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
        assert!(!dir.join("ado.json.tmp").exists());
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
