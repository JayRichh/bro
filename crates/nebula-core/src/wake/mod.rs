//! WHAKAOHO (wake-up): a heartbeat that starts an account's five-hour
//! window at an hour the user picks, so it resets earlier in their day.
//!
//! When an account is switched on and the conditions hold ([`decide`]),
//! kete sends one tiny prompt to the cheapest model under that account at
//! the ANCHOR (05:00, say): the window runs 05:00–10:00, and the user, who
//! starts at 09:00, gets a fresh window at 10:00 instead of 14:00.
//!
//! Everything here is machine-local and readable without a daemon, so
//! `kete wake --all` works from the Task Scheduler with Kete closed:
//! - the schedules: `config.local.json`'s `wake` map, keyed by ACCOUNT
//!   (a profile name, or `@claude` for the CLI's own login). Never
//!   `config.json`, which `kete ssh` carries to other machines, and never a
//!   `Config` field, so no settings save can write a stale copy back.
//! - the decisions: `<data>/wake-log.jsonl` ([`log`]).
//! - the usual hours: a histogram of the account's turns ([`activity`]),
//!   which the daemon serves (protocol 61, `AccountActivity`).
//!
//! The Windows scheduled task that runs `kete wake --all` is built here
//! too ([`task`]), as data, so tests never touch the real Task Scheduler.

pub mod activity;
pub mod decide;
pub mod log;
pub mod task;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::Value;

pub use activity::{preview, suggest_anchor, usual_hours, Preview, UsualHours, Window};
pub use decide::{decide, AccountState, Decision, Skip};

/// The key in `config.local.json` holding the schedules.
pub const LOCAL_KEY: &str = "wake";
/// Minutes in a day.
pub const DAY_MIN: u16 = 1440;
/// The anchor moves in steps of this many minutes.
pub const STEP_MIN: u16 = 15;
/// A Claude session window, in minutes.
pub const WINDOW_MIN: u16 = 300;
/// The cheapest model alias a heartbeat runs on.
pub const MODEL: &str = "haiku";
/// What a heartbeat says.
pub const PROMPT: &str = "hi";
/// Every day, Monday first: bit 0 is Monday, bit 6 Sunday.
pub const ALL_DAYS: u8 = 0x7f;
/// Monday to Friday.
pub const WEEKDAYS: u8 = 0x1f;

const DAY_NAMES: [&str; 7] = ["mon", "tue", "wed", "thu", "fri", "sat", "sun"];

/// How a heartbeat runs.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WakeMode {
    /// A hidden interactive Claude session the daemon runs: plainly
    /// subscription use, and Kete's status line reads the real reset time.
    #[default]
    Interactive,
    /// `claude -p`: no daemon needed, but it may bill the plan's API
    /// credits rather than start the window, and reads no reset time.
    Print,
}

impl WakeMode {
    pub fn as_str(self) -> &'static str {
        match self {
            WakeMode::Interactive => "interactive",
            WakeMode::Print => "print",
        }
    }

    pub fn parse(word: &str) -> Result<WakeMode, String> {
        match word.trim().to_ascii_lowercase().as_str() {
            "interactive" | "session" => Ok(WakeMode::Interactive),
            "print" | "p" => Ok(WakeMode::Print),
            other => Err(format!("unknown mode `{other}`: use interactive or print")),
        }
    }
}

/// One account's wake-up schedule.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct WakeSchedule {
    /// The switch. Off by default: nothing fires until the user says so.
    pub enabled: bool,
    /// Minutes after local midnight the heartbeat goes out, in
    /// [`STEP_MIN`] steps (`300` = 05:00).
    pub anchor_min: u16,
    /// The days it fires on, a bit per day, Monday first ([`ALL_DAYS`]).
    pub days: u8,
    /// Keep the chain rolling: at the anchor + 5h, + 10h… (inside the
    /// usual hours, when known), start the next window if none is live, so
    /// resets stay on the anchor's grid.
    pub keep_rolling: bool,
    /// Don't wake an account that saw no use in this many days.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub skip_idle_days: Option<u32>,
    pub mode: WakeMode,
    /// Let the scheduled task wake the PC from sleep to run.
    pub wake_pc: bool,
}

impl Default for WakeSchedule {
    fn default() -> Self {
        Self {
            enabled: false,
            anchor_min: 5 * 60,
            days: ALL_DAYS,
            keep_rolling: false,
            skip_idle_days: None,
            mode: WakeMode::Interactive,
            wake_pc: false,
        }
    }
}

impl WakeSchedule {
    /// The anchor as stored, put on the grid: inside the day, on a
    /// [`STEP_MIN`] step (a hand edit of `07:07` reads as 07:00).
    pub fn anchor(&self) -> u16 {
        let min = self.anchor_min.min(DAY_MIN - 1);
        min - min % STEP_MIN
    }

    /// The days as stored, only the seven real bits.
    pub fn day_mask(&self) -> u8 {
        self.days & ALL_DAYS
    }

    /// Does it fire on `weekday` (0 = Monday)?
    pub fn on_day(&self, weekday: u8) -> bool {
        weekday < 7 && self.day_mask() & (1 << weekday) != 0
    }

    /// The minutes of the day it fires at: the anchor, then with
    /// `keep_rolling` each 5 h after it until midnight. Ascending.
    pub fn slots(&self) -> Vec<u16> {
        let anchor = self.anchor();
        let mut out = vec![anchor];
        if self.keep_rolling {
            let mut next = anchor + WINDOW_MIN;
            while next < DAY_MIN {
                out.push(next);
                next += WINDOW_MIN;
            }
        }
        out
    }
}

/// `HH:MM` for minutes after midnight.
pub fn hhmm(min: u16) -> String {
    let min = min % DAY_MIN;
    format!("{:02}:{:02}", min / 60, min % 60)
}

/// `05:00`, `5:00`, `5`, `17:30` as minutes after midnight, on a
/// [`STEP_MIN`] step.
pub fn parse_hhmm(text: &str) -> Result<u16, String> {
    let text = text.trim();
    let bad = || format!("`{text}` is not a time: use HH:MM, like 05:00");
    let (h, m) = match text.split_once(':') {
        Some((h, m)) => (h, m),
        None => (text, "0"),
    };
    let h: u16 = h.trim().parse().map_err(|_| bad())?;
    let m: u16 = m.trim().parse().map_err(|_| bad())?;
    if h > 23 || m > 59 {
        return Err(bad());
    }
    let min = h * 60 + m;
    if !min.is_multiple_of(STEP_MIN) {
        return Err(format!(
            "{text}: the hour moves in {STEP_MIN}-minute steps (try {})",
            hhmm(min - min % STEP_MIN)
        ));
    }
    Ok(min)
}

/// `all`, `weekdays` / `mon-fri`, `weekends`, `mon,wed,fri`, `fri-mon` as
/// a day mask.
pub fn parse_days(text: &str) -> Result<u8, String> {
    let text = text.trim().to_ascii_lowercase();
    match text.as_str() {
        "all" | "every" | "daily" | "every-day" => return Ok(ALL_DAYS),
        "weekdays" | "workdays" => return Ok(WEEKDAYS),
        "weekends" | "weekend" => return Ok(ALL_DAYS & !WEEKDAYS),
        _ => {}
    }
    let day = |word: &str| -> Result<u8, String> {
        let word = word.trim();
        DAY_NAMES
            .iter()
            .position(|d| word.len() >= 2 && d.starts_with(&word[..word.len().min(3)]))
            .map(|i| i as u8)
            .ok_or_else(|| {
                format!("unknown day `{word}`: use mon..sun, mon-fri, weekdays, weekends or all")
            })
    };
    let mut mask = 0u8;
    for part in text.split(',').filter(|p| !p.trim().is_empty()) {
        match part.split_once('-') {
            Some((from, to)) => {
                let (from, to) = (day(from)?, day(to)?);
                let mut d = from;
                loop {
                    mask |= 1 << d;
                    if d == to {
                        break;
                    }
                    d = (d + 1) % 7;
                }
            }
            None => mask |= 1 << day(part)?,
        }
    }
    if mask == 0 {
        return Err("name at least one day".into());
    }
    Ok(mask)
}

/// `every day`, `Mon–Fri`, `Sat, Sun`, `Mon, Wed, Fri`.
pub fn days_text(mask: u8) -> String {
    let mask = mask & ALL_DAYS;
    match mask {
        ALL_DAYS => return "every day".into(),
        0 => return "no days".into(),
        WEEKDAYS => return "Mon\u{2013}Fri".into(),
        _ => {}
    }
    (0..7)
        .filter(|d| mask & (1 << d) != 0)
        .map(|d| {
            let name = DAY_NAMES[d as usize];
            format!("{}{}", name[..1].to_ascii_uppercase(), &name[1..])
        })
        .collect::<Vec<_>>()
        .join(", ")
}

/// A moment as the local clock reads it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LocalTime {
    /// Local days since 1970-01-01.
    pub day: i64,
    /// 0 = Monday … 6 = Sunday.
    pub weekday: u8,
    /// Minutes after local midnight.
    pub minute: u16,
}

impl LocalTime {
    /// `epoch_secs` on a clock `offset_secs` east of UTC.
    pub fn at(epoch_secs: i64, offset_secs: i32) -> LocalTime {
        let local = epoch_secs + offset_secs as i64;
        let day = local.div_euclid(86_400);
        LocalTime {
            day,
            // 1970-01-01 was a Thursday.
            weekday: (day + 3).rem_euclid(7) as u8,
            minute: (local.rem_euclid(86_400) / 60) as u16,
        }
    }

    /// The epoch seconds of `minute` on this local day.
    pub fn epoch_of(&self, minute: u16, offset_secs: i32) -> i64 {
        self.day * 86_400 + minute as i64 * 60 - offset_secs as i64
    }
}

/// Where a heartbeat's session runs: an empty folder Kete owns, so no
/// project's hooks, CLAUDE.md or worktree are touched.
pub fn wake_dir() -> PathBuf {
    crate::paths::data_dir().join("wake")
}

/// The decisions log.
pub fn log_path() -> PathBuf {
    crate::paths::data_dir().join("wake-log.jsonl")
}

/// The schedules in a raw settings object, entry by entry: an entry this
/// build can't read is left out, not fatal.
pub fn schedules_from(obj: &serde_json::Map<String, Value>) -> BTreeMap<String, WakeSchedule> {
    obj.get(LOCAL_KEY)
        .and_then(Value::as_object)
        .map(|map| {
            map.iter()
                .filter(|(key, _)| !key.trim().is_empty())
                .filter_map(|(key, value)| {
                    serde_json::from_value(value.clone())
                        .ok()
                        .map(|s| (key.trim().to_string(), s))
                })
                .collect()
        })
        .unwrap_or_default()
}

/// The schedules `local` (a `config.local.json`) holds. A missing or
/// unreadable file has none.
pub fn load_in(local: &Path) -> BTreeMap<String, WakeSchedule> {
    crate::settings::read_object(local)
        .ok()
        .flatten()
        .map(|obj| schedules_from(&obj))
        .unwrap_or_default()
}

/// [`load_in`] on this machine's `config.local.json`.
pub fn load() -> BTreeMap<String, WakeSchedule> {
    load_in(&crate::paths::config_local_path())
}

/// Set (or with None, remove) `account`'s schedule in `local`, keeping
/// every other key of the file and any field of the entry this build has
/// no name for. Taken under the lock on `config` (`config.json`): the
/// settings writers patch both layers under that one lock, so a lock on
/// the local file alone could lose this write to a save that read it
/// before. A local file that is there but can't be parsed is refused,
/// never replaced.
pub fn set_in(
    config: &Path,
    local: &Path,
    account: &str,
    schedule: Option<&WakeSchedule>,
) -> Result<(), String> {
    let account = account.trim();
    if account.is_empty() {
        return Err("no account named".into());
    }
    let _lock = crate::settings::lock(config).map_err(|e| e.to_string())?;
    let mut obj = crate::settings::read_object(local)
        .map_err(|e| format!("{}: {e}: fix it, then try again", local.display()))?
        .unwrap_or_default();
    let mut map = match obj.remove(LOCAL_KEY) {
        None => serde_json::Map::new(),
        Some(Value::Object(map)) => map,
        Some(_) => {
            return Err(format!(
                "{}: `{LOCAL_KEY}` is not an object: fix it, then try again",
                local.display()
            ))
        }
    };
    match schedule {
        Some(schedule) => {
            let mut entry = match map.remove(account) {
                Some(Value::Object(entry)) => entry,
                _ => serde_json::Map::new(),
            };
            let Value::Object(known) = serde_json::to_value(schedule).map_err(|e| e.to_string())?
            else {
                unreachable!("a schedule serializes to an object");
            };
            // `skip_idle_days` off is its absence.
            if schedule.skip_idle_days.is_none() {
                entry.remove("skip_idle_days");
            }
            entry.extend(known);
            map.insert(account.to_string(), Value::Object(entry));
        }
        None => {
            map.remove(account);
        }
    }
    if !map.is_empty() {
        obj.insert(LOCAL_KEY.into(), Value::Object(map));
    }
    crate::settings::write_json(local, &Value::Object(obj))
        .map_err(|e| format!("write {}: {e}", local.display()))
}

/// [`set_in`] on this machine's settings files.
pub fn set(account: &str, schedule: Option<&WakeSchedule>) -> Result<(), String> {
    set_in(
        &crate::paths::config_path(),
        &crate::paths::config_local_path(),
        account,
        schedule,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn times_parse_on_the_quarter_hour_and_print_back() {
        assert_eq!(parse_hhmm("05:00"), Ok(300));
        assert_eq!(parse_hhmm("5:00"), Ok(300));
        assert_eq!(parse_hhmm("5"), Ok(300));
        assert_eq!(parse_hhmm("23:45"), Ok(1425));
        assert_eq!(parse_hhmm("0:00"), Ok(0));
        assert!(parse_hhmm("05:07").unwrap_err().contains("05:00"));
        assert!(parse_hhmm("24:00").is_err());
        assert!(parse_hhmm("five").is_err());
        assert_eq!(hhmm(300), "05:00");
        assert_eq!(hhmm(1425), "23:45");
        assert_eq!(hhmm(1440 + 60), "01:00", "wraps");
    }

    #[test]
    fn days_parse_from_words_lists_and_ranges() {
        assert_eq!(parse_days("all"), Ok(ALL_DAYS));
        assert_eq!(parse_days("mon-fri"), Ok(WEEKDAYS));
        assert_eq!(parse_days("weekdays"), Ok(WEEKDAYS));
        assert_eq!(parse_days("weekends"), Ok(0b110_0000));
        assert_eq!(parse_days("mon,wed"), Ok(0b101));
        assert_eq!(parse_days("Monday, Friday"), Ok(0b1_0001));
        assert_eq!(parse_days("fri-mon"), Ok(0b111_0001), "a range wraps");
        assert!(parse_days("someday").is_err());
        assert!(parse_days(",").is_err());
        assert_eq!(days_text(ALL_DAYS), "every day");
        assert_eq!(days_text(WEEKDAYS), "Mon\u{2013}Fri");
        assert_eq!(days_text(0b101), "Mon, Wed");
    }

    #[test]
    fn the_slots_are_the_anchor_then_its_five_hour_grid_until_midnight() {
        let mut s = WakeSchedule {
            anchor_min: 300,
            ..WakeSchedule::default()
        };
        assert_eq!(s.slots(), vec![300]);
        s.keep_rolling = true;
        assert_eq!(s.slots(), vec![300, 600, 900, 1200]);
        s.anchor_min = 1380;
        assert_eq!(s.slots(), vec![1380], "23:00 + 5h is tomorrow");
        s.anchor_min = 307;
        assert_eq!(s.anchor(), 300, "off-grid hand edits snap down");
        s.anchor_min = 5000;
        assert_eq!(s.anchor(), 1425, "past midnight clamps");
    }

    #[test]
    fn local_time_counts_days_and_weekdays_from_the_offset() {
        // 2026-10-16 16:00 UTC = 2026-10-17 05:00 NZDT (+13h), a Saturday.
        let utc = 1_792_166_400;
        let nz = LocalTime::at(utc, 13 * 3600);
        assert_eq!(nz.minute, 300);
        assert_eq!(nz.weekday, 5, "Saturday");
        let gmt = LocalTime::at(utc, 0);
        assert_eq!(gmt.minute, 960);
        assert_eq!(gmt.weekday, 4, "still Friday in London");
        assert_eq!(nz.epoch_of(300, 13 * 3600), utc);
        // West of UTC.
        let la = LocalTime::at(utc, -7 * 3600);
        assert_eq!(la.minute, 540);
        assert_eq!(la.weekday, 4);
    }

    #[test]
    fn the_local_offset_is_a_whole_quarter_hour_and_sane() {
        let offset = crate::clock::local_offset_secs();
        assert!(offset.abs() <= 14 * 3600, "{offset}");
        assert_eq!(offset % 900, 0, "{offset}");
    }

    #[test]
    fn a_schedule_goes_to_the_local_layer_only_and_keeps_everything_else() {
        let dir = tempfile::tempdir().unwrap();
        let config = dir.path().join("config.json");
        let local = dir.path().join("config.local.json");
        std::fs::write(&config, r#"{"theme": "ocean"}"#).unwrap();
        std::fs::write(
            &local,
            r#"{"session_idle_timeout": "1h", "wake": {"work": {"enabled": true, "colour": "red"}}}"#,
        )
        .unwrap();
        let s = WakeSchedule {
            enabled: true,
            anchor_min: 330,
            days: WEEKDAYS,
            keep_rolling: true,
            skip_idle_days: Some(3),
            mode: WakeMode::Print,
            wake_pc: true,
        };
        set_in(&config, &local, "work", Some(&s)).unwrap();
        set_in(&config, &local, "@claude", Some(&WakeSchedule::default())).unwrap();
        assert_eq!(
            std::fs::read_to_string(&config).unwrap(),
            r#"{"theme": "ocean"}"#,
            "config.json is never touched"
        );
        let saved: Value = serde_json::from_str(&std::fs::read_to_string(&local).unwrap()).unwrap();
        assert_eq!(saved["session_idle_timeout"], "1h");
        assert_eq!(saved["wake"]["work"]["colour"], "red", "unknown field kept");
        assert_eq!(saved["wake"]["work"]["mode"], "print");
        let loaded = load_in(&local);
        assert_eq!(loaded["work"], s);
        assert_eq!(loaded["@claude"], WakeSchedule::default());

        // Switching idle-skipping off drops the key; removing the last
        // entry drops the map.
        let off = WakeSchedule {
            skip_idle_days: None,
            ..s.clone()
        };
        set_in(&config, &local, "work", Some(&off)).unwrap();
        let saved: Value = serde_json::from_str(&std::fs::read_to_string(&local).unwrap()).unwrap();
        assert!(saved["wake"]["work"].get("skip_idle_days").is_none());
        set_in(&config, &local, "work", None).unwrap();
        set_in(&config, &local, "@claude", None).unwrap();
        let saved: Value = serde_json::from_str(&std::fs::read_to_string(&local).unwrap()).unwrap();
        assert!(saved.get("wake").is_none());
        assert_eq!(saved["session_idle_timeout"], "1h");
    }

    #[test]
    fn an_unreadable_local_file_is_refused_and_left_alone() {
        let dir = tempfile::tempdir().unwrap();
        let config = dir.path().join("config.json");
        let local = dir.path().join("config.local.json");
        std::fs::write(&local, "{ broken").unwrap();
        let err = set_in(&config, &local, "work", Some(&WakeSchedule::default())).unwrap_err();
        assert!(err.contains("fix it"), "{err}");
        assert_eq!(std::fs::read_to_string(&local).unwrap(), "{ broken");
        assert!(load_in(&local).is_empty());
        std::fs::write(&local, r#"{"wake": [1]}"#).unwrap();
        assert!(set_in(&config, &local, "work", None).is_err());
    }

    #[test]
    fn one_bad_entry_costs_only_that_entry() {
        let obj = json!({"wake": {
            "good": {"enabled": true, "anchor_min": 360},
            "bad": {"anchor_min": "six"},
            "": {"enabled": true}
        }});
        let map = schedules_from(obj.as_object().unwrap());
        assert_eq!(map.len(), 1);
        assert!(map["good"].enabled);
        assert_eq!(map["good"].days, ALL_DAYS, "missing fields take defaults");
    }
}
