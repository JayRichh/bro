//! `ado.json`: per-repo ADO state (branch to task links, recent reviewers,
//! team, tenant, a small cache, UI bits). Never holds a token.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fmt;
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
    pub tenant: Option<String>,
    pub cache: BTreeMap<String, CacheEntry>,
    pub ui: serde_json::Value,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct StoreData {
    /// Keyed by `org/project/repo`.
    pub repos: BTreeMap<String, RepoState>,
}

pub struct AdoStore {
    path: PathBuf,
    data: Mutex<StoreData>,
}

impl AdoStore {
    /// `<kete data dir>/ado.json`.
    pub fn default_path() -> PathBuf {
        nebula_core::paths::data_dir().join("ado.json")
    }

    /// Load `path`. A missing file is empty; an unreadable one is moved to
    /// `ado.json.bad` and the store starts empty.
    pub fn open(path: impl AsRef<Path>) -> Self {
        let path = path.as_ref().to_path_buf();
        let data = match std::fs::read(&path) {
            Ok(bytes) => serde_json::from_slice(&bytes).unwrap_or_else(|_| {
                let _ = std::fs::rename(&path, sibling(&path, ".bad"));
                StoreData::default()
            }),
            Err(_) => StoreData::default(),
        };
        Self {
            path,
            data: Mutex::new(data),
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Run `f` on the repo's state, then write the file atomically.
    pub fn with_repo<R>(&self, key: &RepoKey, f: impl FnOnce(&mut RepoState) -> R) -> R {
        let mut data = self.data.lock().unwrap_or_else(|p| p.into_inner());
        let out = f(data.repos.entry(key.to_string()).or_default());
        // Best effort: a failed write keeps the in-memory state, and the
        // next successful write carries it.
        let _ = write_atomic(&self.path, &data);
        out
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
    std::fs::write(&tmp, bytes)?;
    std::fs::rename(&tmp, path).inspect_err(|_| {
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
            r.tenant = Some("t-1".into());
        });
        let again = AdoStore::open(&path);
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
