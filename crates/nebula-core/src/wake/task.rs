//! The trigger: one Windows scheduled task, `Kete\Whakaoho`, that runs
//! `kete wake --all` at every switched-on account's slots, so heartbeats
//! go out with Kete closed. It is built from an XML definition
//! ([`task_xml`]) because only that form sets `StartWhenAvailable` (a
//! slot the PC slept through runs on waking, and [`super::decide`] judges
//! the catch-up) and `WakeToRun`. Rewritten whenever a schedule changes,
//! deleted when no account is switched on ([`sync`]).
//!
//! Everything that touches the real Task Scheduler goes through a
//! [`TaskRunner`], so tests hand in a fake one; and [`system_runner`]
//! does nothing while `NEBULA_WAKE_NO_TASK` is set, since no data dir
//! isolates the machine's Task Scheduler. macOS and Linux get the
//! equivalent crontab lines to install by hand ([`crontab_lines`]).

use std::collections::BTreeMap;
use std::path::Path;

use super::{WakeSchedule, ALL_DAYS, DAY_MIN};

/// The task's path in the Task Scheduler library.
pub const TASK_NAME: &str = r"Kete\Whakaoho";

/// What the task runs, after the program.
pub const TASK_ARGS: &str = "wake --all";

/// Every minute of the day some switched-on schedule fires at, with the
/// days (Monday-first bits) any of them fires on it. A minute shared by
/// two accounts is one trigger: `wake --all` decides per account.
pub fn triggers(schedules: &BTreeMap<String, WakeSchedule>) -> BTreeMap<u16, u8> {
    let mut out: BTreeMap<u16, u8> = BTreeMap::new();
    for s in schedules
        .values()
        .filter(|s| s.enabled && s.day_mask() != 0)
    {
        for slot in s.slots() {
            *out.entry(slot).or_insert(0) |= s.day_mask();
        }
    }
    out
}

fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

const DAY_TAGS: [&str; 7] = [
    "Monday",
    "Tuesday",
    "Wednesday",
    "Thursday",
    "Friday",
    "Saturday",
    "Sunday",
];

/// The task definition for `schedules`, running `exe` as `user` (the
/// interactive user, `DOMAIN\name`; None lets the Task Scheduler take the
/// one registering it). None when no account is switched on: there is
/// nothing to schedule and the task should go.
pub fn task_xml(
    schedules: &BTreeMap<String, WakeSchedule>,
    exe: &str,
    user: Option<&str>,
) -> Option<String> {
    let triggers = triggers(schedules);
    if triggers.is_empty() {
        return None;
    }
    let wake_pc = schedules.values().any(|s| s.enabled && s.wake_pc);
    let mut xml = String::new();
    xml.push_str("<?xml version=\"1.0\" encoding=\"UTF-16\"?>\n");
    xml.push_str(
        "<Task version=\"1.2\" xmlns=\"http://schemas.microsoft.com/windows/2004/02/mit/task\">\n",
    );
    xml.push_str("  <RegistrationInfo>\n");
    xml.push_str("    <Author>Kete</Author>\n");
    xml.push_str(
        "    <Description>Whakaoho (wake-up): runs kete wake --all at the hours set in Kete, \
         which sends one tiny prompt to each switched-on account whose window isn't live. \
         Kete rewrites this task when a schedule changes and removes it when none is on.\
         </Description>\n",
    );
    xml.push_str(&format!("    <URI>\\{}</URI>\n", escape(TASK_NAME)));
    xml.push_str("  </RegistrationInfo>\n");
    xml.push_str("  <Triggers>\n");
    for (minute, days) in &triggers {
        // A fixed past date: only the time and the days matter, and with
        // no zone suffix the time is local, daylight saving included.
        xml.push_str("    <CalendarTrigger>\n");
        xml.push_str(&format!(
            "      <StartBoundary>2026-01-05T{:02}:{:02}:00</StartBoundary>\n",
            minute / 60,
            minute % 60
        ));
        xml.push_str("      <Enabled>true</Enabled>\n");
        if *days == ALL_DAYS {
            xml.push_str("      <ScheduleByDay>\n");
            xml.push_str("        <DaysInterval>1</DaysInterval>\n");
            xml.push_str("      </ScheduleByDay>\n");
        } else {
            xml.push_str("      <ScheduleByWeek>\n");
            xml.push_str("        <DaysOfWeek>\n");
            for (bit, tag) in DAY_TAGS.iter().enumerate() {
                if days & (1 << bit) != 0 {
                    xml.push_str(&format!("          <{tag} />\n"));
                }
            }
            xml.push_str("        </DaysOfWeek>\n");
            xml.push_str("        <WeeksInterval>1</WeeksInterval>\n");
            xml.push_str("      </ScheduleByWeek>\n");
        }
        xml.push_str("    </CalendarTrigger>\n");
    }
    xml.push_str("  </Triggers>\n");
    xml.push_str("  <Principals>\n");
    xml.push_str("    <Principal id=\"Author\">\n");
    if let Some(user) = user.map(str::trim).filter(|u| !u.is_empty()) {
        xml.push_str(&format!("      <UserId>{}</UserId>\n", escape(user)));
    }
    // Runs only while the user is logged on, as them, with no password
    // stored and no elevation.
    xml.push_str("      <LogonType>InteractiveToken</LogonType>\n");
    xml.push_str("      <RunLevel>LeastPrivilege</RunLevel>\n");
    xml.push_str("    </Principal>\n");
    xml.push_str("  </Principals>\n");
    xml.push_str("  <Settings>\n");
    xml.push_str("    <MultipleInstancesPolicy>IgnoreNew</MultipleInstancesPolicy>\n");
    xml.push_str("    <DisallowStartIfOnBatteries>false</DisallowStartIfOnBatteries>\n");
    xml.push_str("    <StopIfGoingOnBatteries>false</StopIfGoingOnBatteries>\n");
    xml.push_str("    <AllowHardTerminate>true</AllowHardTerminate>\n");
    xml.push_str("    <StartWhenAvailable>true</StartWhenAvailable>\n");
    xml.push_str("    <RunOnlyIfNetworkAvailable>false</RunOnlyIfNetworkAvailable>\n");
    xml.push_str("    <IdleSettings>\n");
    xml.push_str("      <StopOnIdleEnd>false</StopOnIdleEnd>\n");
    xml.push_str("      <RestartOnIdle>false</RestartOnIdle>\n");
    xml.push_str("    </IdleSettings>\n");
    xml.push_str("    <AllowStartOnDemand>true</AllowStartOnDemand>\n");
    xml.push_str("    <Enabled>true</Enabled>\n");
    xml.push_str("    <Hidden>false</Hidden>\n");
    xml.push_str("    <RunOnlyIfIdle>false</RunOnlyIfIdle>\n");
    xml.push_str(&format!("    <WakeToRun>{wake_pc}</WakeToRun>\n"));
    xml.push_str("    <ExecutionTimeLimit>PT15M</ExecutionTimeLimit>\n");
    xml.push_str("    <Priority>7</Priority>\n");
    xml.push_str("  </Settings>\n");
    xml.push_str("  <Actions Context=\"Author\">\n");
    xml.push_str("    <Exec>\n");
    xml.push_str(&format!("      <Command>{}</Command>\n", escape(exe)));
    xml.push_str(&format!("      <Arguments>{TASK_ARGS}</Arguments>\n"));
    xml.push_str("    </Exec>\n");
    xml.push_str("  </Actions>\n");
    xml.push_str("</Task>\n");
    Some(xml)
}

/// `xml` as the Task Scheduler writes its own exports: UTF-16LE behind a
/// byte order mark, matching the declaration's `encoding`.
pub fn utf16_bytes(xml: &str) -> Vec<u8> {
    let mut out = vec![0xFF, 0xFE];
    for unit in xml.encode_utf16() {
        out.extend_from_slice(&unit.to_le_bytes());
    }
    out
}

/// One `crontab` line per trigger, for macOS and Linux, where kete
/// installs nothing: `0 5 * * 1-5 '/usr/local/bin/kete' wake --all`.
pub fn crontab_lines(schedules: &BTreeMap<String, WakeSchedule>, exe: &str) -> Vec<String> {
    triggers(schedules)
        .into_iter()
        .map(|(minute, days)| {
            let dow = if days == ALL_DAYS {
                "*".to_string()
            } else {
                // cron counts Sunday as 0.
                (0..7u8)
                    .filter(|d| days & (1 << d) != 0)
                    .map(|d| ((d + 1) % 7).to_string())
                    .collect::<Vec<_>>()
                    .join(",")
            };
            format!(
                "{} {} * * {dow} '{}' {TASK_ARGS}",
                minute % 60,
                (minute % DAY_MIN) / 60,
                exe.replace('\'', r"'\''")
            )
        })
        .collect()
}

/// Runs `schtasks` (or stands in for it).
pub trait TaskRunner {
    /// Run with `args`; Err carries what it said when it failed.
    fn run(&self, args: &[String]) -> Result<(), String>;
}

/// The real `schtasks.exe`, with no console window.
#[cfg(windows)]
pub struct Schtasks;

#[cfg(windows)]
impl TaskRunner for Schtasks {
    fn run(&self, args: &[String]) -> Result<(), String> {
        let mut command = std::process::Command::new("schtasks.exe");
        command
            .args(args)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());
        crate::platform::no_window(&mut command);
        let out = command
            .output()
            .map_err(|e| format!("schtasks didn't run: {e}"))?;
        if out.status.success() {
            Ok(())
        } else {
            let said = String::from_utf8_lossy(&out.stderr);
            let said = if said.trim().is_empty() {
                String::from_utf8_lossy(&out.stdout)
            } else {
                said
            };
            Err(said.trim().to_string())
        }
    }
}

/// A runner that runs nothing: `NEBULA_WAKE_NO_TASK`, and macOS / Linux.
pub struct NoTask;

impl TaskRunner for NoTask {
    fn run(&self, _args: &[String]) -> Result<(), String> {
        Ok(())
    }
}

/// The runner a binary uses: the real Task Scheduler on Windows, unless
/// `NEBULA_WAKE_NO_TASK` is set.
pub fn system_runner() -> Box<dyn TaskRunner> {
    if crate::env::non_empty(crate::env::WAKE_NO_TASK).is_some() {
        return Box::new(NoTask);
    }
    #[cfg(windows)]
    {
        Box::new(Schtasks)
    }
    #[cfg(not(windows))]
    {
        Box::new(NoTask)
    }
}

/// What [`sync`] did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Synced {
    /// The task was written, firing at this many times of day.
    Written { triggers: usize },
    /// No account is on: the task was removed (or never there).
    Removed,
}

/// Bring the scheduled task in line with `schedules`: write `xml_path`
/// and register it over any older one, or delete the task when nothing is
/// switched on. `exe` is kete's own binary.
pub fn sync(
    runner: &dyn TaskRunner,
    schedules: &BTreeMap<String, WakeSchedule>,
    exe: &str,
    user: Option<&str>,
    xml_path: &Path,
) -> Result<Synced, String> {
    match task_xml(schedules, exe, user) {
        Some(xml) => {
            if let Some(dir) = xml_path.parent() {
                std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
            }
            std::fs::write(xml_path, utf16_bytes(&xml))
                .map_err(|e| format!("write {}: {e}", xml_path.display()))?;
            runner.run(&[
                "/Create".into(),
                "/XML".into(),
                xml_path.to_string_lossy().into_owned(),
                "/TN".into(),
                TASK_NAME.into(),
                "/F".into(),
            ])?;
            Ok(Synced::Written {
                triggers: triggers(schedules).len(),
            })
        }
        None => {
            let _ = std::fs::remove_file(xml_path);
            // Deleting a task that isn't there fails; that is the state
            // wanted, so the failure is no news.
            let _ = runner.run(&[
                "/Delete".into(),
                "/TN".into(),
                TASK_NAME.into(),
                "/F".into(),
            ]);
            Ok(Synced::Removed)
        }
    }
}

/// The interactive user as the Task Scheduler names them, `DOMAIN\name`.
pub fn current_user() -> Option<String> {
    let name = crate::env::non_empty("USERNAME")?;
    Some(match crate::env::non_empty("USERDOMAIN") {
        Some(domain) => format!(r"{domain}\{name}"),
        None => name,
    })
}

/// Where the definition is written before it is registered.
pub fn xml_path() -> std::path::PathBuf {
    crate::paths::data_dir().join("wake-task.xml")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wake::WEEKDAYS;
    use std::cell::RefCell;

    #[derive(Default)]
    struct Fake {
        calls: RefCell<Vec<Vec<String>>>,
        fail: bool,
    }

    impl TaskRunner for Fake {
        fn run(&self, args: &[String]) -> Result<(), String> {
            self.calls.borrow_mut().push(args.to_vec());
            if self.fail {
                Err("ERROR: Access is denied.".into())
            } else {
                Ok(())
            }
        }
    }

    fn on(anchor_min: u16, days: u8) -> WakeSchedule {
        WakeSchedule {
            enabled: true,
            anchor_min,
            days,
            ..WakeSchedule::default()
        }
    }

    fn map(entries: &[(&str, WakeSchedule)]) -> BTreeMap<String, WakeSchedule> {
        entries
            .iter()
            .map(|(k, v)| (k.to_string(), v.clone()))
            .collect()
    }

    #[test]
    fn triggers_merge_shared_minutes_and_follow_the_grid() {
        let mut rolling = on(300, WEEKDAYS);
        rolling.keep_rolling = true;
        let schedules = map(&[
            ("work", rolling),
            ("home", on(300, 0b110_0000)),
            ("spare", on(450, crate::wake::ALL_DAYS)),
            ("off", WakeSchedule::default()),
        ]);
        let t = triggers(&schedules);
        assert_eq!(
            t.keys().copied().collect::<Vec<_>>(),
            vec![300, 450, 600, 900, 1200]
        );
        assert_eq!(t[&300], crate::wake::ALL_DAYS, "weekdays + weekends");
        assert_eq!(t[&600], WEEKDAYS);
        assert!(triggers(&map(&[("off", WakeSchedule::default())])).is_empty());
    }

    #[test]
    fn the_xml_names_the_task_the_command_and_each_trigger() {
        let mut work = on(300, WEEKDAYS);
        work.wake_pc = true;
        let schedules = map(&[("work", work), ("home", on(450, crate::wake::ALL_DAYS))]);
        let xml = task_xml(
            &schedules,
            r"C:\Program Files\Kete & Co\kete.exe",
            Some(r"PC\Jay"),
        )
        .unwrap();
        assert!(xml.starts_with("<?xml version=\"1.0\" encoding=\"UTF-16\"?>"));
        assert!(xml.contains(r"<URI>\Kete\Whakaoho</URI>"));
        assert!(xml.contains(r"<Command>C:\Program Files\Kete &amp; Co\kete.exe</Command>"));
        assert!(xml.contains("<Arguments>wake --all</Arguments>"));
        assert!(xml.contains(r"<UserId>PC\Jay</UserId>"));
        assert!(xml.contains("<LogonType>InteractiveToken</LogonType>"));
        assert!(xml.contains("<StartWhenAvailable>true</StartWhenAvailable>"));
        assert!(xml.contains("<WakeToRun>true</WakeToRun>"));
        assert!(xml.contains("<StartBoundary>2026-01-05T05:00:00</StartBoundary>"));
        assert!(xml.contains("<StartBoundary>2026-01-05T07:30:00</StartBoundary>"));
        assert_eq!(xml.matches("<CalendarTrigger>").count(), 2);
        assert_eq!(xml.matches("<ScheduleByDay>").count(), 1, "07:30 every day");
        assert!(xml.contains("<Friday />") && !xml.contains("<Saturday />"));
        assert!(!xml.contains("<Password>"));
        // Without wake-the-PC on any switched-on account.
        let xml = task_xml(&map(&[("home", on(450, 1))]), "kete.exe", None).unwrap();
        assert!(xml.contains("<WakeToRun>false</WakeToRun>"));
        assert!(!xml.contains("<UserId>"));
        assert_eq!(task_xml(&BTreeMap::new(), "kete.exe", None), None);
    }

    #[test]
    fn the_file_is_utf16_with_a_bom() {
        let bytes = utf16_bytes("<a>é</a>");
        assert_eq!(&bytes[..2], &[0xFF, 0xFE]);
        assert_eq!(&bytes[2..4], &[b'<', 0]);
        assert_eq!(bytes.len(), 2 + 8 * 2);
    }

    #[test]
    fn crontab_lines_count_sunday_as_zero() {
        let schedules = map(&[
            ("work", on(300, WEEKDAYS)),
            ("home", on(450, 0b100_0000)),
            ("all", on(1425, crate::wake::ALL_DAYS)),
        ]);
        assert_eq!(
            crontab_lines(&schedules, "/usr/bin/kete"),
            vec![
                "0 5 * * 1,2,3,4,5 '/usr/bin/kete' wake --all",
                "30 7 * * 0 '/usr/bin/kete' wake --all",
                "45 23 * * * '/usr/bin/kete' wake --all",
            ]
        );
    }

    #[test]
    fn sync_registers_the_xml_or_deletes_the_task() {
        let dir = tempfile::tempdir().unwrap();
        let xml = dir.path().join("wake-task.xml");
        let fake = Fake::default();
        let synced = sync(
            &fake,
            &map(&[("work", on(300, WEEKDAYS))]),
            "kete.exe",
            None,
            &xml,
        )
        .unwrap();
        assert_eq!(synced, Synced::Written { triggers: 1 });
        assert!(xml.exists());
        assert_eq!(
            fake.calls.borrow()[0],
            vec![
                "/Create".to_string(),
                "/XML".into(),
                xml.to_string_lossy().into_owned(),
                "/TN".into(),
                r"Kete\Whakaoho".into(),
                "/F".into()
            ]
        );
        let synced = sync(&fake, &BTreeMap::new(), "kete.exe", None, &xml).unwrap();
        assert_eq!(synced, Synced::Removed);
        assert!(!xml.exists());
        assert_eq!(fake.calls.borrow()[1][0], "/Delete");
        // A failed delete is the wanted state anyway; a failed create isn't.
        let failing = Fake {
            fail: true,
            ..Fake::default()
        };
        assert_eq!(
            sync(&failing, &BTreeMap::new(), "kete.exe", None, &xml),
            Ok(Synced::Removed)
        );
        let err = sync(
            &failing,
            &map(&[("work", on(300, WEEKDAYS))]),
            "kete.exe",
            None,
            &xml,
        )
        .unwrap_err();
        assert!(err.contains("Access is denied"));
    }
}
