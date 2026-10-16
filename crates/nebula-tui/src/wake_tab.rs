//! The WAKE TAB of the accounts overlay (`⇧U`, then `Tab`): WHAKAOHO, the
//! wake-up heartbeat, for each Claude account. One row an account: its
//! switch, the hour its one tiny prompt goes out at, a one-line strip of
//! the day showing what that hour does to the five-hour windows, and the
//! last decision. The rows are `kete wake status --json`'s; the core
//! (`nebula_core::wake`) decides and writes, this module only reads that
//! JSON, edits the schedule it describes, and draws it.
//!
//! Nothing here runs on the UI thread that could wait: the status is read
//! by a child `kete` on a blocking thread, an edit is saved by a thread
//! of its own (it takes a file lock and may run `schtasks`), and Wake now
//! is a child `kete wake <account> --force`. Each hands what it found
//! back through [`Inbox`], which the overlay's draw reads
//! ([`WakeTab::absorb`]), and a footer word through `view_jobs`' own
//! `Answer::Flash`. A child never writes to the TUI's screen: its output
//! is piped and parsed, never inherited.
//!
//! An edit shows at once (the row is changed in place) and is saved
//! behind a short debounce, so holding an arrow key saves the hour it
//! settles on, not every step. A status read that began before the last
//! edit is dropped when it lands: it would put the old hour back until
//! the read after the save corrected it.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use nebula_core::wake::{self, decide, UsualHours, WakeSchedule, DAY_MIN, STEP_MIN};
use serde::Deserialize;

use crate::view_jobs::{Answer, Jobs};

/// How long the anchor rests before it is saved: a held arrow key moves
/// it many steps and saves once.
pub const DEBOUNCE: Duration = Duration::from_millis(400);

/// What the footer says on the Wake view (`ui/footer.rs` shows it while
/// the accounts overlay is on that view).
pub const FOOTER_HINT: &str = "\u{2191}/\u{2193}: account  Space: on/off  \u{2190}/\u{2192}: hour (\u{21e7}: an hour)  s: suggested  w: wake now  Tab: accounts  Esc: close";

const DAY_NAMES: [&str; 7] = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];

// ---- the status JSON ----

/// A window of the day, minutes after midnight; `end` past 1440 runs into
/// the next day.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(default)]
pub struct WakeWindow {
    pub start: u16,
    pub end: u16,
}

/// What an anchor does to a day (`nebula_core::wake::Preview`'s shape).
#[derive(Debug, Clone, PartialEq, Eq, Default, Deserialize)]
#[serde(default)]
pub struct WakePreview {
    pub windows: Vec<WakeWindow>,
    pub fresh_in_active: Option<u32>,
    pub baseline: Option<u32>,
}

/// The hours an account is usually used: a start and a length in
/// minutes, which may run past midnight.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(default)]
pub struct Usual {
    pub start: u16,
    pub len: u16,
}

impl Usual {
    /// Does it hold `minute` of the day?
    pub fn contains(&self, minute: u16) -> bool {
        self.len >= DAY_MIN
            || (minute % DAY_MIN + DAY_MIN - self.start % DAY_MIN) % DAY_MIN < self.len
    }

    fn core(self) -> UsualHours {
        UsualHours {
            start: self.start,
            len: self.len,
        }
    }

    /// `09:00–17:00`.
    pub fn text(&self) -> String {
        format!(
            "{}\u{2013}{}",
            wake::hhmm(self.start),
            wake::hhmm(self.start.saturating_add(self.len))
        )
    }
}

/// One account of the status: every field tolerated missing, so a status
/// from a kete that knows less (or more) still draws.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(default)]
pub struct WakeAccount {
    /// The profile name, or `@claude` for the CLI's own login.
    pub account: String,
    /// There is such an account (a schedule can outlive its profile).
    pub known: bool,
    pub disabled: bool,
    pub logged_in: bool,
    pub login_hint: String,
    pub schedule: WakeSchedule,
    pub days_text: String,
    /// When it next goes out, epoch seconds, as the read worked it out.
    pub next: Option<i64>,
    pub usual: Option<Usual>,
    pub suggested: Option<u16>,
    pub preview: Option<WakePreview>,
    /// Every anchor's preview, by anchor / 15 minutes.
    pub previews: Option<Vec<WakePreview>>,
    pub last_turn_at: Option<i64>,
    /// `05:02 fired · window to 10:02`, the log's last line for it.
    pub last_summary: Option<String>,
}

impl Default for WakeAccount {
    fn default() -> Self {
        Self {
            account: String::new(),
            known: true,
            disabled: false,
            logged_in: true,
            login_hint: String::new(),
            schedule: WakeSchedule::default(),
            days_text: String::new(),
            next: None,
            usual: None,
            suggested: None,
            preview: None,
            previews: None,
            last_turn_at: None,
            last_summary: None,
        }
    }
}

/// `kete wake status --json`.
#[derive(Debug, Clone, PartialEq, Default, Deserialize)]
#[serde(default)]
pub struct WakeStatus {
    /// Epoch seconds the status was read at.
    pub now: i64,
    /// Seconds the local clock is east of UTC.
    pub offset: i32,
    /// None: no daemon running. Some(false): one too old for interactive
    /// wake-ups.
    pub daemon: Option<bool>,
    /// The machine runs Windows (a scheduled task), else crontab lines.
    #[serde(rename = "windows")]
    pub on_windows: bool,
    pub task_line: String,
    pub crontab: Vec<String>,
    pub model: String,
    pub accounts: Vec<WakeAccount>,
}

impl WakeStatus {
    pub fn account(&self, key: &str) -> Option<&WakeAccount> {
        self.accounts.iter().find(|a| a.account == key)
    }
}

/// The JSON `kete wake status --json` printed. Anything ahead of the
/// first `{` is not it.
pub fn parse_status(bytes: &[u8]) -> Result<WakeStatus, String> {
    let text = String::from_utf8_lossy(bytes);
    let json = match text.find('{') {
        Some(at) => &text[at..],
        None => &text[..],
    };
    serde_json::from_str(json).map_err(|e| format!("couldn't read the wake status: {e}"))
}

/// What the tab says it does, in words.
pub fn explain() -> &'static str {
    "Sends one tiny prompt with your own subscription at the hour you pick, so the account's 5-hour window starts early. Off until switched on."
}

/// How an account is named: the CLI's own login reads as it does on the
/// Accounts view.
pub fn label_of(account: &str) -> String {
    match account.strip_prefix('@') {
        Some(harness) => format!("{harness} (own login)"),
        None => account.to_string(),
    }
}

/// `Sat 05:30`: an epoch stamp as the local clock reads it.
pub fn day_clock(at: i64, offset: i32) -> String {
    let t = wake::LocalTime::at(at, offset);
    format!(
        "{} {}",
        DAY_NAMES[t.weekday as usize % 7],
        wake::hhmm(t.minute)
    )
}

/// The anchor `steps` quarter hours on (or back, negative), round the
/// clock: 00:00 back a step is 23:45.
pub fn stepped(anchor: u16, steps: i32) -> u16 {
    (anchor as i32 + steps * STEP_MIN as i32).rem_euclid(DAY_MIN as i32) as u16
}

impl WakeAccount {
    pub fn label(&self) -> String {
        label_of(&self.account)
    }

    /// Why the heartbeat would not go out, when something about the
    /// account stops it: the part that needs the user.
    pub fn problem(&self) -> Option<String> {
        if !self.known {
            Some("no such account".into())
        } else if !self.logged_in {
            Some(if self.login_hint.trim().is_empty() {
                "not logged in".to_string()
            } else {
                format!("not logged in: {}", self.login_hint.trim())
            })
        } else if self.disabled {
            Some("switched off".into())
        } else {
            None
        }
    }

    /// The windows `anchor` produces: the read's own answer for that
    /// anchor when it sent every anchor's, else worked out here (the same
    /// function the read used) from the usual hours.
    pub fn windows_at(&self, anchor: u16) -> Vec<(u16, u16)> {
        let at = (anchor / STEP_MIN) as usize;
        if let Some(p) = self.previews.as_ref().and_then(|p| p.get(at)) {
            return p.windows.iter().map(|w| (w.start, w.end)).collect();
        }
        wake::preview(anchor, self.usual.map(Usual::core))
            .windows
            .iter()
            .map(|w| (w.start, w.end))
            .collect()
    }

    /// When it next goes out (epoch seconds), by the schedule as it is
    /// now: the read's own `next` is out of date once the hour has moved.
    pub fn next_run(&self, now: i64, offset: i32) -> Option<i64> {
        decide::next_slot(now, offset, &self.schedule, self.usual.map(Usual::core))
    }
}

// ---- the strip ----

/// What one cell of the day strip shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cell {
    /// Outside the windows and the usual hours.
    Idle,
    /// Only the usual hours.
    Usual,
    /// Inside the wake-up window `index` (counted by start time), and
    /// inside the usual hours or not.
    Window { index: usize, usual: bool },
    /// The time now.
    Now,
    /// The hour the heartbeat goes out at.
    Anchor,
}

/// The day as `width` cells, midnight at the left. The anchor and now
/// are marks on top of whatever the cell holds; the anchor wins a cell
/// they share, as it is what the arrows move.
pub fn strip(
    width: usize,
    windows: &[(u16, u16)],
    usual: Option<Usual>,
    anchor: u16,
    now_min: u16,
) -> Vec<Cell> {
    let day = DAY_MIN as usize;
    let mut order = windows.to_vec();
    order.sort_by_key(|w| w.0);
    let anchor_at = anchor as usize % day * width / day;
    let now_at = now_min as usize % day * width / day;
    (0..width)
        .map(|c| {
            if c == anchor_at {
                return Cell::Anchor;
            }
            if c == now_at {
                return Cell::Now;
            }
            let from = c * day / width;
            let to = ((c + 1) * day / width).max(from + 1);
            let mid = ((from + to) / 2) as u16;
            // A window may run past midnight: it holds the small hours of
            // the day too.
            let index = order
                .iter()
                .position(|&(s, e)| (s..e).contains(&mid) || (s..e).contains(&(mid + DAY_MIN)));
            let in_usual = usual.is_some_and(|u| u.contains(mid));
            match (index, in_usual) {
                (Some(index), usual) => Cell::Window { index, usual },
                (None, true) => Cell::Usual,
                (None, false) => Cell::Idle,
            }
        })
        .collect()
}

/// The hours above the strip: `00` `06` `12` `18` at their cells.
pub fn scale(width: usize) -> String {
    let mut cells = vec![' '; width];
    for hour in [0usize, 6, 12, 18] {
        let at = hour * width / 24;
        for (k, ch) in format!("{hour:02}").chars().enumerate() {
            if let Some(cell) = cells.get_mut(at + k) {
                *cell = ch;
            }
        }
    }
    cells.into_iter().collect()
}

// ---- what the threads share with the tab ----

/// An edit not yet saved: the two things the tab changes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Edit {
    pub enabled: bool,
    pub anchor_min: u16,
}

/// The edits a save thread took after the debounce.
#[derive(Debug, Default)]
struct Batch {
    edits: BTreeMap<String, Edit>,
    /// How many edits had been made when it was taken.
    count: u64,
}

#[derive(Debug, Default)]
struct Shared {
    /// The newest status read to land, and the edit count it began under.
    landed: Option<(u64, Result<WakeStatus, String>)>,
    /// Edits waiting out the debounce, one entry an account: the latest
    /// hour and switch it was left on.
    edits: BTreeMap<String, Edit>,
    /// When the save thread may take them.
    due: Option<Instant>,
    /// A save thread is waiting on `due`.
    worker: bool,
    /// Edits made, ever: a read that began under fewer is stale.
    count: u64,
    /// Accounts whose Wake now is running, so a second press waits.
    waking: Vec<String>,
}

/// The hand-off between the tab and its threads. Cloning shares it, and
/// a thread keeps its own clone: closing the overlay never loses the edit
/// a held arrow key was still settling on.
#[derive(Debug, Clone, Default)]
pub struct Inbox(Arc<Mutex<Shared>>);

impl PartialEq for Inbox {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}

impl Inbox {
    fn lock(&self) -> MutexGuard<'_, Shared> {
        self.0.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Note `edit` for `account`, to be saved `delay` from now. True when
    /// no thread is waiting to save, so the caller starts one.
    pub fn queue(&self, account: &str, edit: Edit, delay: Duration) -> bool {
        let mut s = self.lock();
        s.edits.insert(account.to_string(), edit);
        s.count += 1;
        s.due = Some(Instant::now() + delay);
        !std::mem::replace(&mut s.worker, true)
    }

    /// The edits waiting to be saved.
    pub fn pending(&self) -> Vec<(String, Edit)> {
        self.lock()
            .edits
            .iter()
            .map(|(k, v)| (k.clone(), *v))
            .collect()
    }

    /// How many edits have been made.
    pub fn count(&self) -> u64 {
        self.lock().count
    }

    /// The waiting edits, once the debounce is up; else how long is left.
    /// Taking them is the save thread's leaving: the next edit starts
    /// another.
    fn take_batch(&self) -> Result<Batch, Duration> {
        let mut s = self.lock();
        let now = Instant::now();
        let due = s.due.unwrap_or(now);
        if due > now {
            return Err(due - now);
        }
        s.worker = false;
        s.due = None;
        let count = s.count;
        Ok(Batch {
            edits: std::mem::take(&mut s.edits),
            count,
        })
    }

    /// A status read finished, having begun when `tag` edits were made.
    pub fn land(&self, tag: u64, result: Result<WakeStatus, String>) {
        self.lock().landed = Some((tag, result));
    }

    /// Mark `account` as being woken; false when it already is.
    pub fn begin_wake(&self, account: &str) -> bool {
        let mut s = self.lock();
        if s.waking.iter().any(|a| a == account) {
            return false;
        }
        s.waking.push(account.to_string());
        true
    }

    pub fn end_wake(&self, account: &str) {
        self.lock().waking.retain(|a| a != account);
    }
}

/// What an edit asks of the caller.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Saving {
    /// No account is on the tab: nothing changed.
    Nothing,
    /// The edit is queued behind a save thread already waiting.
    Queued,
    /// The caller starts the thread that waits out the debounce and saves.
    Start,
}

// ---- the tab ----

/// The Wake tab's state: the rows of the last status, the cursor, and
/// the inbox its threads answer into.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct WakeTab {
    pub status: Option<WakeStatus>,
    pub cursor: usize,
    /// The last read failed, and why.
    pub error: Option<String>,
    /// A read is on its way and nothing has landed yet.
    pub loading: bool,
    /// The account the cursor goes to once the rows are known.
    pub focus: Option<String>,
    landed_at: Option<Instant>,
    pub inbox: Inbox,
}

impl WakeTab {
    pub fn rows(&self) -> &[WakeAccount] {
        match &self.status {
            Some(status) => &status.accounts,
            None => &[],
        }
    }

    pub fn selected(&self) -> Option<&WakeAccount> {
        self.rows().get(self.cursor)
    }

    pub fn selected_key(&self) -> Option<String> {
        self.selected().map(|a| a.account.clone())
    }

    /// Seconds the local clock is east of UTC, as the read found it.
    pub fn offset(&self) -> i32 {
        self.status.as_ref().map_or(0, |s| s.offset)
    }

    /// The time now, epoch seconds: the read's stamp and the time since
    /// it landed, so the strip's mark moves without asking again.
    pub fn now(&self) -> i64 {
        let Some(status) = &self.status else {
            return 0;
        };
        status.now + self.landed_at.map_or(0, |at| at.elapsed().as_secs() as i64)
    }

    pub fn step(&mut self, delta: i32) {
        let last = self.rows().len().saturating_sub(1) as i32;
        self.cursor = (self.cursor as i32 + delta).clamp(0, last) as usize;
    }

    pub fn select(&mut self, i: usize) {
        self.cursor = i.min(self.rows().len().saturating_sub(1));
    }

    /// Put the cursor on the account `focus` names, when the rows hold it.
    pub fn select_focus(&mut self) {
        if let Some(key) = self.focus.take() {
            let at = self.rows().iter().position(|a| a.account == key);
            match at {
                Some(at) => self.cursor = at,
                None => self.focus = Some(key),
            }
        }
    }

    /// Take what a thread landed, if it is not stale. Run by the draw: the
    /// loop redraws when a thread's answer wakes it.
    pub fn absorb(&mut self) {
        let (landed, current) = {
            let mut s = self.inbox.lock();
            (s.landed.take(), s.count)
        };
        let Some((tag, result)) = landed else {
            return;
        };
        // An edit made after this read began is already on the row, and
        // the read after its save will confirm it.
        if tag < current {
            return;
        }
        self.loading = false;
        match result {
            Ok(status) => self.apply(status),
            Err(why) => self.error = Some(why),
        }
    }

    fn apply(&mut self, status: WakeStatus) {
        let keep = self.selected_key();
        self.status = Some(status);
        self.error = None;
        self.landed_at = Some(Instant::now());
        if let Some(key) = keep {
            let at = self.rows().iter().position(|a| a.account == key);
            if let Some(at) = at {
                self.cursor = at;
            }
        }
        self.cursor = self.cursor.min(self.rows().len().saturating_sub(1));
        // An account the rows do not hold is not waited for.
        self.select_focus();
        self.focus = None;
    }

    fn row_mut(&mut self) -> Option<&mut WakeAccount> {
        let at = self.cursor;
        self.status.as_mut().and_then(|s| s.accounts.get_mut(at))
    }

    /// Queue the selected account's switch and hour to be saved `delay`
    /// from now.
    fn queue(&mut self, delay: Duration) -> Saving {
        let Some(row) = self.selected() else {
            return Saving::Nothing;
        };
        let edit = Edit {
            enabled: row.schedule.enabled,
            anchor_min: row.schedule.anchor(),
        };
        let account = row.account.clone();
        if self.inbox.queue(&account, edit, delay) {
            Saving::Start
        } else {
            Saving::Queued
        }
    }

    /// Flip the selected account's switch. Saved at once.
    pub fn toggle(&mut self) -> Saving {
        let Some(row) = self.row_mut() else {
            return Saving::Nothing;
        };
        row.schedule.enabled = !row.schedule.enabled;
        self.queue(Duration::ZERO)
    }

    /// Move the selected account's hour `steps` quarter hours. Saved once
    /// it rests.
    pub fn nudge(&mut self, steps: i32) -> Saving {
        let Some(row) = self.row_mut() else {
            return Saving::Nothing;
        };
        row.schedule.anchor_min = stepped(row.schedule.anchor(), steps);
        self.queue(DEBOUNCE)
    }

    /// Move the selected account's hour to the one the usual hours
    /// suggest.
    pub fn snap(&mut self) -> Result<Saving, String> {
        let Some(row) = self.row_mut() else {
            return Ok(Saving::Nothing);
        };
        let Some(best) = row.suggested else {
            return Err(format!(
                "{}: no suggested hour yet: it needs a few days of sessions on the account",
                row.label()
            ));
        };
        row.schedule.anchor_min = best;
        Ok(self.queue(DEBOUNCE))
    }
}

// ---- the threads ----

/// What a thread that has nothing to say in the footer answers with, so
/// the loop wakes and redraws: a "slow" notice on a ticket nobody waits
/// on is the one answer that touches nothing.
fn redraw() -> Answer {
    Answer::Slow {
        ticket: crate::view_jobs::ticket(),
    }
}

/// Read the status on a thread and land it in `inbox`.
pub fn spawn_status(jobs: &Jobs, inbox: &Inbox) {
    let inbox = inbox.clone();
    let tag = inbox.count();
    jobs.run(move || {
        inbox.land(tag, kete::read_status());
        Some(redraw())
    });
}

/// Wait out the debounce, save what was edited, read the status again
/// and land it; the footer says what was saved, or why not.
pub fn spawn_flush(jobs: &Jobs, inbox: &Inbox) {
    let inbox = inbox.clone();
    jobs.run(move || {
        let batch = loop {
            match inbox.take_batch() {
                Ok(batch) => break batch,
                Err(wait) => std::thread::sleep(wait),
            }
        };
        let said = kete::save_all(&batch.edits);
        inbox.land(batch.count, kete::read_status());
        Some(Answer::Flash(said))
    });
}

/// Run `kete wake <account> --force`, read the status again, and say in
/// the footer how it went.
pub fn spawn_wake(jobs: &Jobs, inbox: &Inbox, account: String) {
    let inbox = inbox.clone();
    let tag = inbox.count();
    jobs.run(move || {
        let outcome = kete::wake_now(&account);
        let status = kete::read_status();
        let line = match outcome {
            Err(why) => format!("wake {account} failed: {why}"),
            Ok(()) => {
                let summary = status
                    .as_ref()
                    .ok()
                    .and_then(|s| s.account(&account))
                    .and_then(|a| a.last_summary.clone());
                match summary {
                    Some(summary) => format!("woke {account}: {summary}"),
                    None => format!("woke {account}"),
                }
            }
        };
        inbox.land(tag, status);
        inbox.end_wake(&account);
        Some(Answer::Flash(line))
    });
}

/// `work: wake-up on, 05:30` / `work: wake-up off`.
pub fn saved_line(account: &str, edit: &Edit) -> String {
    if edit.enabled {
        format!("{account}: wake-up on, {}", wake::hhmm(edit.anchor_min))
    } else {
        format!("{account}: wake-up off")
    }
}

/// The first line a failed `kete` said on stderr, without anyhow's
/// `Error: `.
fn first_line(stderr: &[u8]) -> Option<String> {
    String::from_utf8_lossy(stderr)
        .lines()
        .map(str::trim)
        .find(|l| !l.is_empty())
        .map(|l| l.trim_start_matches("Error: ").to_string())
}

/// How `kete wake <account> --force --json` went: Ok, or what went wrong
/// in its own words. The JSON first, whatever the exit code, since a
/// wake that failed prints its report and then exits non-zero; with no
/// JSON (an account that isn't logged in stops before it), stderr's.
pub fn wake_outcome(stdout: &[u8], stderr: &[u8], success: bool) -> Result<(), String> {
    let text = String::from_utf8_lossy(stdout);
    let list = text
        .find('[')
        .and_then(|at| serde_json::from_str::<Vec<serde_json::Value>>(&text[at..]).ok());
    if let Some(first) = list.as_ref().and_then(|l| l.first()) {
        let error = first
            .pointer("/report/error")
            .and_then(serde_json::Value::as_str)
            .map(str::trim)
            .filter(|e| !e.is_empty());
        if let Some(error) = error {
            return Err(error.to_string());
        }
        if success {
            return Ok(());
        }
    }
    Err(first_line(stderr).unwrap_or_else(|| "kete wake didn't say why".into()))
}

/// The child `kete` the tab runs, and the saves it makes. Every function
/// here is a no-op error under unit tests: a test must never start a
/// process, run `schtasks`, or write the machine's real wake-up files.
mod kete {
    use super::*;
    use std::process::{Command, Output, Stdio};

    /// `kete <args>` with nothing on the terminal: no stdin, and stdout
    /// and stderr piped for the caller to read. On Windows no console
    /// window opens either.
    fn run(args: &[&str]) -> Result<Output, String> {
        if cfg!(test) {
            return Err("kete is not started under unit tests".into());
        }
        let exe =
            std::env::current_exe().map_err(|e| format!("couldn't find kete's own path: {e}"))?;
        let mut command = Command::new(exe);
        command
            .args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        #[cfg(windows)]
        nebula_core::platform::no_window(&mut command);
        command
            .output()
            .map_err(|e| format!("couldn't run kete: {e}"))
    }

    /// kete's own binary, as the scheduled task names it: this one, with
    /// Windows' `\\?\` prefix off.
    fn own_exe() -> Result<String, String> {
        let exe =
            std::env::current_exe().map_err(|e| format!("couldn't find kete's own path: {e}"))?;
        let text = exe.to_string_lossy().into_owned();
        Ok(text.strip_prefix(r"\\?\").unwrap_or(&text).to_string())
    }

    pub fn read_status() -> Result<WakeStatus, String> {
        let out = run(&["wake", "status", "--json"])?;
        parse_status(&out.stdout).map_err(|parse| {
            if out.status.success() {
                parse
            } else {
                first_line(&out.stderr).unwrap_or(parse)
            }
        })
    }

    pub fn wake_now(account: &str) -> Result<(), String> {
        let out = run(&["wake", account, "--force", "--json"])?;
        wake_outcome(&out.stdout, &out.stderr, out.status.success())
    }

    /// Save each edit through the core (`task::save`: the schedule in
    /// `config.local.json`, then the scheduled task), on top of the
    /// schedule as it is on disk now so the days, the options and a mode
    /// pinned by hand stay as they are. The footer's line.
    pub fn save_all(edits: &BTreeMap<String, Edit>) -> String {
        if cfg!(test) {
            return "saving is off under unit tests".into();
        }
        let exe = match own_exe() {
            Ok(exe) => exe,
            Err(why) => return why,
        };
        let mut said = Vec::new();
        for (account, edit) in edits {
            let mut schedule = wake::load().remove(account).unwrap_or_default();
            schedule.enabled = edit.enabled;
            schedule.anchor_min = edit.anchor_min;
            said.push(match wake::task::save(account, Some(&schedule), &exe) {
                Ok(_) => saved_line(account, edit),
                Err(why) => format!("{account}: {why}"),
            });
        }
        said.join("; ")
    }
}

/// A status as `kete wake status --json` prints it, for the tests here
/// and the overlay's.
#[cfg(test)]
pub(crate) mod fixture {
    use nebula_core::wake::{self, UsualHours, WakeSchedule};

    /// Saturday 2026-10-17 03:10 NZDT. `work` is on at 05:30 with usual
    /// hours 09:00–17:00 and every anchor's preview; `home` is off, not
    /// logged in, with no usual hours.
    pub fn status_json() -> String {
        let usual = UsualHours {
            start: 540,
            len: 480,
        };
        let previews: Vec<_> = (0..96u16)
            .map(|i| wake::preview(i * 15, Some(usual)))
            .collect();
        let on = WakeSchedule {
            enabled: true,
            anchor_min: 330,
            ..WakeSchedule::default()
        };
        serde_json::json!({
            "now": 1_792_159_800i64,
            "offset": 46_800,
            "daemon": true,
            "windows": true,
            "task_name": "Kete\\Whakaoho",
            "task_line": "Task Scheduler: Kete\\Whakaoho runs `kete wake --all` at 05:30",
            "task": ["05:30"],
            "crontab": [],
            "log": "wake-log.jsonl",
            "wake_dir": "wake",
            "model": "haiku",
            "accounts": [
                {
                    "account": "work",
                    "known": true,
                    "disabled": false,
                    "logged_in": true,
                    "login_hint": "kete profile login work",
                    "schedule": on,
                    "mode": "auto",
                    "days_text": "every day",
                    "slots": [330],
                    "next": 1_792_168_200i64,
                    "usual": usual,
                    "suggested": 330,
                    "preview": previews[22],
                    "previews": previews,
                    "bins": null,
                    "last_turn_at": 1_792_140_000i64,
                    "last": null,
                    "last_summary": "05:32 fired \u{b7} window to 10:32",
                    "recent": [],
                    "session": null
                },
                {
                    "account": "home",
                    "known": true,
                    "disabled": false,
                    "logged_in": false,
                    "login_hint": "kete profile login home",
                    "schedule": WakeSchedule::default(),
                    "mode": "auto",
                    "days_text": "every day",
                    "slots": [300],
                    "next": null,
                    "usual": null,
                    "suggested": null,
                    "preview": wake::preview(300, None),
                    "previews": null,
                    "bins": null,
                    "last_turn_at": null,
                    "last": null,
                    "last_summary": null,
                    "recent": [],
                    "session": null
                }
            ]
        })
        .to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tab() -> WakeTab {
        WakeTab {
            status: Some(parse_status(fixture::status_json().as_bytes()).unwrap()),
            ..WakeTab::default()
        }
    }

    #[test]
    fn the_status_json_parses_into_rows() {
        let status = parse_status(fixture::status_json().as_bytes()).unwrap();
        assert_eq!(status.accounts.len(), 2);
        assert_eq!(status.offset, 46_800);
        assert_eq!(status.daemon, Some(true));
        assert!(status.on_windows);
        assert!(status.task_line.contains("Kete\\Whakaoho"));
        let work = &status.accounts[0];
        assert_eq!(work.account, "work");
        assert!(work.schedule.enabled && work.logged_in && work.known);
        assert_eq!(work.schedule.anchor(), 330);
        assert_eq!(
            work.usual,
            Some(Usual {
                start: 540,
                len: 480
            })
        );
        assert_eq!(work.usual.unwrap().text(), "09:00\u{2013}17:00");
        assert_eq!(work.suggested, Some(330));
        assert_eq!(work.previews.as_ref().map(Vec::len), Some(96));
        assert_eq!(work.next, Some(1_792_168_200));
        assert_eq!(
            work.last_summary.as_deref(),
            Some("05:32 fired \u{b7} window to 10:32")
        );
        let home = &status.accounts[1];
        assert!(!home.logged_in && !home.schedule.enabled);
        assert!(home.usual.is_none() && home.previews.is_none() && home.next.is_none());
        assert_eq!(
            home.problem().unwrap(),
            "not logged in: kete profile login home"
        );
        assert_eq!(
            status.account("home").map(|a| a.label()),
            Some("home".into())
        );
        assert_eq!(label_of("@claude"), "claude (own login)");
    }

    #[test]
    fn a_status_missing_fields_still_parses() {
        let status = parse_status(br#"{"accounts":[{"account":"x"},{}]}"#).unwrap();
        assert_eq!(status.accounts.len(), 2);
        let x = &status.accounts[0];
        assert!(
            x.known && x.logged_in && !x.disabled,
            "an absent flag assumes fine"
        );
        assert!(
            !x.schedule.enabled,
            "a schedule not sent is the default: off"
        );
        assert_eq!(x.schedule.anchor(), 300);
        assert!(x.problem().is_none());
        assert!(x.next.is_none() && x.last_summary.is_none() && x.usual.is_none());
        assert_eq!(parse_status(b"{}").unwrap(), WakeStatus::default());
        // Fields this build has no name for are ignored; text ahead of the
        // JSON is skipped.
        let status = parse_status(b"note\n{\"now\": 5, \"surprise\": {\"a\": [1]}}").unwrap();
        assert_eq!(status.now, 5);
        // A preview short of fields, an account with a partial schedule.
        let status = parse_status(
            br#"{"accounts":[{"account":"y","schedule":{"enabled":true},"preview":{},"usual":{"start":60}}]}"#,
        )
        .unwrap();
        let y = &status.accounts[0];
        assert!(y.schedule.enabled);
        assert_eq!(y.schedule.anchor(), 300, "the hour not sent is the default");
        assert_eq!(y.preview, Some(WakePreview::default()));
        assert_eq!(y.usual, Some(Usual { start: 60, len: 0 }));
        assert!(parse_status(b"nope").is_err());
    }

    #[test]
    fn the_hour_moves_by_quarter_hours_and_wraps_the_day() {
        assert_eq!(stepped(0, -1), 1425, "00:00 back is 23:45");
        assert_eq!(stepped(1425, 1), 0, "23:45 on is 00:00");
        assert_eq!(stepped(330, 4), 390);
        assert_eq!(stepped(330, -4), 270);
        assert_eq!(stepped(60, -8), 1380);
        assert_eq!(wake::hhmm(stepped(330, 1)), "05:45");
    }

    #[test]
    fn the_strip_marks_windows_usual_hours_now_and_the_anchor() {
        let usual = Some(Usual {
            start: 540,
            len: 480,
        });
        let windows = [(330, 630), (630, 930), (930, 1230)];
        // 48 cells of half an hour.
        let cells = strip(48, &windows, usual, 330, 190);
        assert_eq!(cells.len(), 48);
        assert_eq!(cells[11], Cell::Anchor, "05:30");
        assert_eq!(cells[6], Cell::Now, "03:10");
        assert_eq!(
            cells[12],
            Cell::Window {
                index: 0,
                usual: false
            }
        );
        assert_eq!(
            cells[19],
            Cell::Window {
                index: 0,
                usual: true
            }
        );
        assert_eq!(
            cells[22],
            Cell::Window {
                index: 1,
                usual: true
            }
        );
        assert_eq!(
            cells[40],
            Cell::Window {
                index: 2,
                usual: false
            }
        );
        assert_eq!(cells[45], Cell::Idle);
        assert_eq!(cells[0], Cell::Idle);
        // Usual hours with no window over them.
        let cells = strip(48, &[(330, 540)], usual, 330, 0);
        assert_eq!(cells[20], Cell::Usual);
        // A window that runs past midnight holds the small hours.
        let cells = strip(48, &[(1380, 1680)], None, 1380, 700);
        assert_eq!(
            cells[1],
            Cell::Window {
                index: 0,
                usual: false
            }
        );
        // The anchor wins a cell it shares with now.
        let cells = strip(48, &[], None, 330, 335);
        assert_eq!(cells[11], Cell::Anchor);
        assert!(!cells.contains(&Cell::Now));
        assert_eq!(scale(48).chars().count(), 48);
        assert!(scale(48).starts_with("00"));
        assert_eq!(&scale(48)[12..14], "06");
        assert_eq!(&scale(48)[24..26], "12");
        assert_eq!(&scale(48)[36..38], "18");
    }

    #[test]
    fn a_row_takes_its_windows_from_the_previews_else_works_them_out() {
        let tab = tab();
        let work = &tab.rows()[0];
        let home = &tab.rows()[1];
        assert_eq!(
            work.windows_at(330),
            work.previews.as_ref().unwrap()[22]
                .windows
                .iter()
                .map(|w| (w.start, w.end))
                .collect::<Vec<_>>()
        );
        assert_eq!(
            work.windows_at(330)[0],
            (330, 630),
            "the heartbeat's window"
        );
        assert_ne!(
            work.windows_at(330),
            work.windows_at(360),
            "it follows the hour"
        );
        assert_eq!(
            home.windows_at(300),
            vec![(300, 600)],
            "no usual hours: the one window"
        );
        assert_eq!(home.windows_at(360), vec![(360, 660)]);
    }

    #[test]
    fn the_next_run_follows_the_hour_as_it_moves() {
        let mut tab = tab();
        let (now, offset) = (tab.now(), tab.offset());
        let work = &tab.rows()[0];
        assert_eq!(work.next_run(now, offset), Some(1_792_168_200));
        assert_eq!(day_clock(1_792_168_200, offset), "Sat 05:30");
        tab.nudge(4);
        let moved = tab.rows()[0].next_run(now, offset).unwrap();
        assert_eq!(day_clock(moved, offset), "Sat 06:30");
        assert_eq!(
            tab.rows()[1].next_run(now, offset),
            None,
            "off: nothing next"
        );
    }

    #[test]
    fn an_edit_shows_at_once_and_queues_one_save_an_account() {
        let mut tab = tab();
        assert_eq!(
            tab.nudge(1),
            Saving::Start,
            "the first edit starts the saver"
        );
        assert_eq!(tab.nudge(1), Saving::Queued, "the next waits behind it");
        assert_eq!(tab.selected().unwrap().schedule.anchor(), 360);
        assert_eq!(
            tab.inbox.pending(),
            vec![(
                "work".to_string(),
                Edit {
                    enabled: true,
                    anchor_min: 360
                }
            )]
        );
        assert_eq!(tab.toggle(), Saving::Queued);
        assert!(!tab.selected().unwrap().schedule.enabled);
        assert_eq!(
            tab.inbox.pending(),
            vec![(
                "work".to_string(),
                Edit {
                    enabled: false,
                    anchor_min: 360
                }
            )],
            "one entry an account: the latest"
        );
        tab.step(1);
        assert_eq!(tab.toggle(), Saving::Queued);
        assert_eq!(tab.inbox.pending().len(), 2);
        assert_eq!(tab.inbox.count(), 4);
        // Nothing to edit before a status lands.
        let mut empty = WakeTab::default();
        assert_eq!(empty.toggle(), Saving::Nothing);
        assert_eq!(empty.nudge(1), Saving::Nothing);
        assert_eq!(empty.snap(), Ok(Saving::Nothing));
    }

    #[test]
    fn snap_goes_to_the_suggested_hour_or_says_there_is_none() {
        let mut tab = tab();
        tab.nudge(3);
        assert_eq!(tab.selected().unwrap().schedule.anchor(), 375);
        assert!(tab.snap().is_ok());
        assert_eq!(tab.selected().unwrap().schedule.anchor(), 330);
        tab.step(1);
        let why = tab.snap().unwrap_err();
        assert!(why.contains("no suggested hour"), "{why}");
        assert_eq!(tab.selected().unwrap().schedule.anchor(), 300, "left alone");
    }

    #[test]
    fn the_saver_takes_its_edits_only_after_the_debounce() {
        let inbox = Inbox::default();
        let edit = Edit {
            enabled: true,
            anchor_min: 330,
        };
        assert!(inbox.queue("work", edit, Duration::from_secs(60)));
        let wait = inbox.take_batch().unwrap_err();
        assert!(wait > Duration::from_secs(30), "{wait:?}");
        assert!(
            !inbox.queue("work", edit, Duration::ZERO),
            "a saver is already waiting"
        );
        let batch = inbox.take_batch().unwrap();
        assert_eq!(batch.edits.len(), 1);
        assert_eq!(batch.count, 2);
        assert!(inbox.pending().is_empty());
        assert!(
            inbox.queue("home", edit, Duration::ZERO),
            "it left: the next edit starts another"
        );
    }

    #[test]
    fn a_read_begun_before_an_edit_does_not_undo_it() {
        let mut tab = tab();
        let before = tab.inbox.count();
        tab.nudge(1);
        let status = || parse_status(fixture::status_json().as_bytes());
        tab.inbox.land(before, status());
        tab.absorb();
        assert_eq!(
            tab.selected().unwrap().schedule.anchor(),
            345,
            "the old hour didn't come back"
        );
        // The read begun after the edit is the truth, whatever it says.
        tab.inbox.land(tab.inbox.count(), status());
        tab.absorb();
        assert_eq!(tab.selected().unwrap().schedule.anchor(), 330);
        assert!(tab.error.is_none() && !tab.loading);
        // A failed read keeps the rows and says why.
        tab.step(1);
        tab.inbox
            .land(tab.inbox.count(), Err("kete didn't answer".into()));
        tab.absorb();
        assert_eq!(tab.error.as_deref(), Some("kete didn't answer"));
        assert_eq!(tab.rows().len(), 2);
        assert_eq!(tab.cursor, 1, "the cursor stays");
        // A landing puts the cursor on the account asked for.
        let mut fresh = WakeTab {
            focus: Some("home".into()),
            ..WakeTab::default()
        };
        fresh.inbox.land(0, status());
        fresh.absorb();
        assert_eq!(fresh.selected_key().as_deref(), Some("home"));
        assert!(fresh.focus.is_none());
    }

    #[test]
    fn a_wake_runs_once_at_a_time_an_account() {
        let inbox = Inbox::default();
        assert!(inbox.begin_wake("work"));
        assert!(!inbox.begin_wake("work"), "the second press waits");
        assert!(inbox.begin_wake("home"));
        inbox.end_wake("work");
        assert!(inbox.begin_wake("work"));
    }

    #[test]
    fn wake_now_reads_its_report_before_its_exit_code() {
        assert_eq!(
            wake_outcome(
                br#"[{"account":"work","decision":"fire","report":{"error":null}}]"#,
                b"",
                true
            ),
            Ok(())
        );
        assert_eq!(
            wake_outcome(
                br#"[{"account":"work","report":{"error":"the turn timed out"}}]"#,
                b"Error: a wake-up failed: see above, or kete wake status\n",
                false
            ),
            Err("the turn timed out".to_string())
        );
        assert_eq!(
            wake_outcome(
                b"",
                b"Error: work isn't logged in, so a wake-up can't run: kete profile login work\n",
                false
            ),
            Err(
                "work isn't logged in, so a wake-up can't run: kete profile login work".to_string()
            )
        );
        assert_eq!(
            wake_outcome(b"", b"", false),
            Err("kete wake didn't say why".to_string())
        );
        assert!(wake_outcome(br#"[{"report":null}]"#, b"boom", false).is_err());
    }

    #[test]
    fn saved_lines_name_the_account_the_state_and_the_hour() {
        let on = Edit {
            enabled: true,
            anchor_min: 330,
        };
        assert_eq!(saved_line("work", &on), "work: wake-up on, 05:30");
        assert_eq!(
            saved_line(
                "work",
                &Edit {
                    enabled: false,
                    ..on
                }
            ),
            "work: wake-up off"
        );
    }

    #[test]
    fn nothing_is_run_or_saved_under_unit_tests() {
        assert!(kete::read_status().is_err());
        assert!(kete::wake_now("work").is_err());
        let mut edits = BTreeMap::new();
        edits.insert(
            "work".to_string(),
            Edit {
                enabled: true,
                anchor_min: 330,
            },
        );
        assert!(kete::save_all(&edits).contains("off under unit tests"));
    }
}
