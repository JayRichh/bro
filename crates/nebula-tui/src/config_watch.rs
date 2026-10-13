//! CONFIG WATCH: notices when the settings files change under a running
//! TUI — the desktop window's Settings view writes `config.json` through
//! `Config::update` — so the event loop can adopt the values it mirrors
//! into app state (`event_loop::reload_foreign_config`). Most settings are
//! read fresh at each use and need nothing; this is for the rest.
//!
//! The check is a `stat` of each layer, `(mtime, len)`, compared with the
//! last one seen: cheap enough for the loop's three-second beat. The
//! TUI's own saves move the stamp along ([`ConfigWatch::own_write`]) so
//! they don't read as foreign.

use std::path::Path;
use std::time::SystemTime;

/// One file's `(mtime, len)`; None for a file that isn't there (a missing
/// `config.local.json` is the usual case, not an error).
type FileStamp = Option<(Option<SystemTime>, u64)>;

/// Both layers, `config.json` then `config.local.json`.
type Stamp = [FileStamp; 2];

/// Unarmed until [`ConfigWatch::arm`] takes the first stamp — the app
/// arms it at startup — so building an `App` (as every test does) never
/// looks at a settings file.
#[derive(Debug, Default)]
pub struct ConfigWatch {
    seen: Option<Stamp>,
}

impl ConfigWatch {
    /// Take the files as they are now as the baseline.
    pub fn arm(&mut self) {
        self.seen = Some(current());
    }

    /// The files moved since the last stamp, without taking the change.
    /// False while unarmed.
    pub fn changed(&self) -> bool {
        self.seen.is_some_and(|seen| seen != current())
    }

    /// True once per change: the files moved since the last stamp, which
    /// now becomes the baseline. False while unarmed.
    pub fn poll(&mut self) -> bool {
        let Some(seen) = self.seen else {
            return false;
        };
        let now = current();
        if now == seen {
            return false;
        }
        self.seen = Some(now);
        true
    }

    /// Run the TUI's own write `save` and keep it from reading as foreign:
    /// the stamp moves past it only when nothing else had changed the files
    /// first. A change already pending stays pending, so the next poll
    /// still fires and the full reload picks up what this write didn't
    /// (a save adopts its own values, not every mirrored one).
    pub fn own_write<T>(&mut self, save: impl FnOnce() -> T) -> T {
        let clean = self.seen.is_some() && !self.changed();
        let out = save();
        if clean {
            self.arm();
        }
        out
    }
}

fn current() -> Stamp {
    let (path, local) = crate::config::Config::layer_paths();
    [stamp(&path), stamp(&local)]
}

fn stamp(path: &Path) -> FileStamp {
    let meta = std::fs::metadata(path).ok()?;
    Some((meta.modified().ok(), meta.len()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{with_config_path, Config};
    use std::time::Duration;

    /// Pin a file's mtime, so a test never waits on the clock's resolution.
    fn touch(path: &Path, secs: u64) {
        std::fs::OpenOptions::new()
            .write(true)
            .open(path)
            .unwrap()
            .set_modified(SystemTime::UNIX_EPOCH + Duration::from_secs(secs))
            .unwrap();
    }

    fn in_temp_config(f: impl FnOnce(&Path)) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.json");
        std::fs::write(
            &path, "{}
",
        )
        .unwrap();
        with_config_path(path.clone(), || f(&path));
    }

    #[test]
    fn an_unarmed_watch_never_fires() {
        in_temp_config(|path| {
            let mut watch = ConfigWatch::default();
            Config::update(|c| c.kanban = true).unwrap();
            touch(path, 1_000_000);
            assert!(!watch.changed());
            assert!(!watch.poll());
        });
    }

    #[test]
    fn unchanged_files_do_not_fire() {
        in_temp_config(|path| {
            Config::update(|c| c.kanban = true).unwrap();
            touch(path, 1_000_000);
            let mut watch = ConfigWatch::default();
            watch.arm();
            assert!(!watch.changed());
            assert!(!watch.poll());
            assert!(!watch.poll());
        });
    }

    #[test]
    fn a_foreign_write_fires_once() {
        in_temp_config(|path| {
            Config::update(|c| c.kanban = false).unwrap();
            touch(path, 1_000_000);
            let mut watch = ConfigWatch::default();
            watch.arm();
            Config::update(|c| c.kanban = true).unwrap();
            touch(path, 2_000_000);
            assert!(watch.changed());
            assert!(watch.poll(), "the write is seen");
            assert!(!watch.poll(), "and only once");
        });
    }

    /// The same length and a new mtime still count: `true` → `false` in a
    /// key can change a file's length, but a theme swap may not.
    #[test]
    fn an_mtime_alone_fires() {
        in_temp_config(|path| {
            Config::update(|c| c.kanban = true).unwrap();
            touch(path, 1_000_000);
            let mut watch = ConfigWatch::default();
            watch.arm();
            touch(path, 1_000_001);
            assert!(watch.poll());
        });
    }

    #[test]
    fn the_local_layer_appearing_fires() {
        in_temp_config(|path| {
            Config::update(|c| c.kanban = true).unwrap();
            let mut watch = ConfigWatch::default();
            watch.arm();
            std::fs::write(
                crate::config::sibling_local_path(path),
                r#"{"kanban": false}"#,
            )
            .unwrap();
            assert!(watch.poll());
        });
    }

    #[test]
    fn an_own_write_does_not_fire() {
        in_temp_config(|path| {
            Config::update(|c| c.kanban = false).unwrap();
            touch(path, 1_000_000);
            let mut watch = ConfigWatch::default();
            watch.arm();
            watch.own_write(|| {
                Config::update(|c| c.kanban = true).unwrap();
                touch(path, 2_000_000);
            });
            assert!(!watch.poll());
        });
    }

    /// A foreign change, then the TUI's own save before the beat: the save
    /// must not swallow the change, or the values only a full reload
    /// adopts (the keymap, say) never arrive.
    #[test]
    fn an_own_write_keeps_a_pending_foreign_change() {
        in_temp_config(|path| {
            Config::update(|c| c.kanban = false).unwrap();
            touch(path, 1_000_000);
            let mut watch = ConfigWatch::default();
            watch.arm();
            Config::update(|c| c.kanban = true).unwrap();
            touch(path, 2_000_000);
            watch.own_write(|| {
                Config::update(|c| c.animations = false).unwrap();
                touch(path, 3_000_000);
            });
            assert!(watch.poll(), "the foreign change is still pending");
            assert!(!watch.poll());
        });
    }
}
