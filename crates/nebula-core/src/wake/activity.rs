//! An account's USUAL HOURS, read off when its turns started, and what a
//! wake-up anchor does to the windows inside them.
//!
//! The daemon counts the account's turns over the last [`DAYS`] days into
//! [`BINS`] quarter-hour bins of the local day ([`histogram`]); the usual
//! hours are the shortest stretch of the day holding most of them
//! ([`usual_hours`]). [`preview`] walks a day of five-hour windows — the
//! heartbeat at the anchor, then the user's own prompts all through their
//! usual hours — and counts how many fresh windows land inside them;
//! [`suggest_anchor`] picks the anchor that counts most.

use serde::{Deserialize, Serialize};

use super::{DAY_MIN, STEP_MIN, WINDOW_MIN};

/// Quarter hours in a day.
pub const BINS: usize = 96;
/// How far back the histogram looks.
pub const DAYS: u32 = 14;
/// Fewer turns than this say nothing about anyone's day.
pub const MIN_TURNS: u32 = 10;
/// The share of turns the usual hours hold.
const COVER: f64 = 0.9;

/// Count turn starts (epoch seconds) from the last `days` days before
/// `now` into the local day's quarter hours, by the clock `offset_secs`
/// east of UTC.
pub fn histogram(
    starts: impl IntoIterator<Item = i64>,
    now: i64,
    offset_secs: i32,
    days: u32,
) -> Vec<u32> {
    let mut bins = vec![0u32; BINS];
    let from = now - days as i64 * 86_400;
    for at in starts.into_iter().filter(|at| (from..=now).contains(at)) {
        let minute = super::LocalTime::at(at, offset_secs).minute as usize;
        bins[(minute / STEP_MIN as usize).min(BINS - 1)] += 1;
    }
    bins
}

/// A stretch of the local day, which may run past midnight.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct UsualHours {
    /// Minutes after midnight it starts.
    pub start: u16,
    /// How long it runs, in minutes (1440 at most).
    pub len: u16,
}

impl UsualHours {
    /// Minutes after midnight it ends.
    pub fn end(&self) -> u16 {
        (self.start + self.len) % DAY_MIN
    }

    /// Does it hold `minute` (after midnight)?
    pub fn contains(&self, minute: u16) -> bool {
        (minute % DAY_MIN + DAY_MIN - self.start) % DAY_MIN < self.len
    }
}

/// The shortest stretch of the day holding nine in ten of the turns
/// `bins` counts ([`BINS`] quarter hours); the busier on a tie, then the
/// earlier. None with fewer than [`MIN_TURNS`] turns.
pub fn usual_hours(bins: &[u32]) -> Option<UsualHours> {
    if bins.len() != BINS {
        return None;
    }
    let total: u32 = bins.iter().sum();
    if total < MIN_TURNS {
        return None;
    }
    let need = (total as f64 * COVER).ceil() as u32;
    let mut best: Option<(usize, usize, u32)> = None;
    for start in 0..BINS {
        let mut sum = 0;
        for len in 1..=BINS {
            sum += bins[(start + len - 1) % BINS];
            if sum >= need {
                let better = match best {
                    None => true,
                    Some((_, best_len, best_sum)) => {
                        len < best_len || (len == best_len && sum > best_sum)
                    }
                };
                if better {
                    best = Some((start, len, sum));
                }
                break;
            }
        }
    }
    let (start, len, _) = best?;
    Some(UsualHours {
        start: start as u16 * STEP_MIN,
        len: len as u16 * STEP_MIN,
    })
}

/// One five-hour window, as minutes after midnight; `end` past 1440 runs
/// into the next day.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Window {
    pub start: u16,
    pub end: u16,
}

/// What an anchor does to a day.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Preview {
    /// The windows of a day, in order: the heartbeat's, then the ones the
    /// user's own prompts start.
    pub windows: Vec<Window>,
    /// How many of them reach into the usual hours — each one a fresh
    /// allowance. None without usual hours.
    pub fresh_in_active: Option<u32>,
    /// The same count with no heartbeat at all, to compare against.
    pub baseline: Option<u32>,
}

/// A simulated day: window starts, minutes from the end of the usual
/// hours (relative 0), for the steady state's second day.
fn simulate(usual: UsualHours, anchor: Option<u16>) -> Vec<u16> {
    let rel_start = DAY_MIN - usual.len.min(DAY_MIN);
    let rel_anchor = anchor.map(|a| (a % DAY_MIN + DAY_MIN - usual.end()) % DAY_MIN);
    let mut live_until: i32 = -1;
    let mut starts = Vec::new();
    // Two days, so the first day's last window (which may still be live
    // at the anchor) carries into the one reported.
    for t in 0..(2 * DAY_MIN as i32) {
        let rel = (t % DAY_MIN as i32) as u16;
        let active = rel >= rel_start;
        let heartbeat = rel_anchor == Some(rel);
        if live_until <= t && (active || heartbeat) {
            live_until = t + WINDOW_MIN as i32;
            if t >= DAY_MIN as i32 {
                starts.push(rel);
            }
        }
    }
    starts
}

fn windows_of(usual: UsualHours, starts: &[u16]) -> Vec<Window> {
    starts
        .iter()
        .map(|rel| {
            let start = (rel + usual.end()) % DAY_MIN;
            Window {
                start,
                end: start + WINDOW_MIN,
            }
        })
        .collect()
}

/// How much of each window falls inside the usual hours, in minutes;
/// windows that miss them are left out.
fn segments(usual: UsualHours, starts: &[u16]) -> Vec<u16> {
    let rel_start = DAY_MIN - usual.len.min(DAY_MIN);
    starts
        .iter()
        .filter_map(|&s| {
            let from = s.max(rel_start);
            let to = (s + WINDOW_MIN).min(DAY_MIN);
            (to > from).then(|| to - from)
        })
        .collect()
}

/// The windows `anchor` produces in a day of `usual` hours. Without usual
/// hours it is the heartbeat's own window and nothing to count.
pub fn preview(anchor: u16, usual: Option<UsualHours>) -> Preview {
    let anchor = anchor % DAY_MIN;
    let Some(usual) = usual else {
        return Preview {
            windows: vec![Window {
                start: anchor,
                end: anchor + WINDOW_MIN,
            }],
            fresh_in_active: None,
            baseline: None,
        };
    };
    let with = simulate(usual, Some(anchor));
    let without = simulate(usual, None);
    Preview {
        windows: windows_of(usual, &with),
        fresh_in_active: Some(segments(usual, &with).len() as u32),
        baseline: Some(segments(usual, &without).len() as u32),
    }
}

/// The anchor that lands the most fresh windows inside the usual hours
/// `bins` shows, the first reset shortly after the usual start: among the
/// best, the one whose smallest piece of a window inside the hours is
/// largest (9–5 gives 05:30: resets at 10:30 and 15:30, an hour and a half
/// either end). None without usual hours, or when no anchor beats not
/// waking at all.
pub fn suggest_anchor(bins: &[u32]) -> Option<u16> {
    suggest_for(usual_hours(bins)?)
}

/// [`suggest_anchor`] for known usual hours.
pub fn suggest_for(usual: UsualHours) -> Option<u16> {
    let baseline = segments(usual, &simulate(usual, None)).len();
    let mut best: Option<(u16, usize, u16)> = None;
    // From the end of the usual hours round the clock, so a tie goes to
    // the anchor furthest ahead of the usual start.
    for step in 0..(DAY_MIN / STEP_MIN) {
        let anchor = (usual.end() + step * STEP_MIN) % DAY_MIN;
        let seg = segments(usual, &simulate(usual, Some(anchor)));
        let count = seg.len();
        let least = seg.iter().copied().min().unwrap_or(0);
        let better = match best {
            None => true,
            Some((_, c, l)) => count > c || (count == c && least > l),
        };
        if better {
            best = Some((anchor, count, least));
        }
    }
    let (anchor, count, _) = best?;
    (count > baseline).then_some(anchor)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `per_bin` turns in each quarter hour from `from` to `to` (HH, end
    /// exclusive, may wrap).
    fn day(from: u16, to: u16, per_bin: u32) -> Vec<u32> {
        let mut bins = vec![0; BINS];
        let (a, b) = ((from * 4) as usize, (to * 4) as usize);
        let mut i = a;
        while i != b {
            bins[i] += per_bin;
            i = (i + 1) % BINS;
        }
        bins
    }

    fn at(h: u16, m: u16) -> u16 {
        h * 60 + m
    }

    #[test]
    fn turns_fall_into_local_quarter_hours_inside_the_lookback() {
        let now = 1_792_166_400; // 2026-10-16 16:00 UTC
        let nz = 13 * 3600;
        // 05:07 and 05:14 NZDT today, 23:59 NZDT three days ago, and one
        // older than the lookback.
        let turns = [
            now + 7 * 60,
            now + 14 * 60,
            now - 3 * 86_400 + 18 * 3600 + 59 * 60 + 1,
            now - 20 * 86_400,
        ];
        let bins = histogram(turns, now + 3600, nz, DAYS);
        assert_eq!(bins.len(), BINS);
        assert_eq!(bins[20], 2, "05:00–05:15");
        assert_eq!(bins[95], 1, "23:45–24:00");
        assert_eq!(bins.iter().sum::<u32>(), 3, "the old one is out");
    }

    #[test]
    fn a_nine_to_five_worker_starts_at_nine() {
        let mut bins = day(9, 17, 3);
        bins[20 * 4] += 1; // one evening turn
        let usual = usual_hours(&bins).unwrap();
        assert_eq!(usual.start, at(9, 0));
        assert!((at(15, 0)..=at(17, 0)).contains(&usual.end()), "{usual:?}");
        assert!(usual.contains(at(12, 0)));
        assert!(!usual.contains(at(20, 0)));
    }

    #[test]
    fn a_night_owl_runs_past_midnight() {
        let usual = usual_hours(&day(20, 3, 2)).unwrap();
        assert_eq!(usual.start, at(20, 0));
        assert!(usual.contains(at(1, 0)));
        assert!(usual.contains(at(23, 0)));
        assert!(!usual.contains(at(12, 0)));
    }

    #[test]
    fn too_little_history_says_nothing() {
        let mut bins = vec![0; BINS];
        assert_eq!(usual_hours(&bins), None);
        bins[40] = MIN_TURNS - 1;
        assert_eq!(usual_hours(&bins), None);
        assert_eq!(suggest_anchor(&bins), None);
        assert_eq!(usual_hours(&[1; 10]), None, "wrong bin count");
    }

    #[test]
    fn nine_to_five_suggests_half_past_five_for_three_fresh_windows() {
        let usual = UsualHours {
            start: at(9, 0),
            len: 8 * 60,
        };
        assert_eq!(suggest_for(usual), Some(at(5, 30)));
        let p = preview(at(5, 30), Some(usual));
        assert_eq!(p.fresh_in_active, Some(3));
        assert_eq!(p.baseline, Some(2), "9–14, 14–19 without a heartbeat");
        assert_eq!(
            p.windows,
            vec![
                Window {
                    start: at(5, 30),
                    end: at(10, 30)
                },
                Window {
                    start: at(10, 30),
                    end: at(15, 30)
                },
                Window {
                    start: at(15, 30),
                    end: at(20, 30)
                },
            ]
        );
        // From a histogram: nine in ten turns fall 09:00–16:15, so the
        // first reset comes an hour in.
        let anchor = suggest_anchor(&day(9, 17, 2)).unwrap();
        assert!((at(5, 0)..=at(5, 30)).contains(&anchor), "{anchor}");
    }

    #[test]
    fn an_anchor_inside_the_usual_hours_changes_nothing() {
        let usual = UsualHours {
            start: at(9, 0),
            len: 8 * 60,
        };
        let p = preview(at(12, 0), Some(usual));
        assert_eq!(p.fresh_in_active, p.baseline);
        assert_eq!(p.windows[0].start, at(9, 0), "the user's own start");
        // Too early: its window is over before the day starts.
        let p = preview(at(3, 0), Some(usual));
        assert_eq!(p.fresh_in_active, Some(2));
        assert_eq!(p.windows.len(), 3, "the heartbeat's plus two");
    }

    #[test]
    fn a_night_owl_gets_an_anchor_before_the_evening() {
        let usual = UsualHours {
            start: at(20, 0),
            len: 7 * 60,
        };
        let anchor = suggest_for(usual).unwrap();
        let p = preview(anchor, Some(usual));
        assert_eq!(p.fresh_in_active, Some(3), "{p:?}");
        assert_eq!(p.baseline, Some(2));
        assert!(
            (at(15, 15)..at(20, 0)).contains(&anchor),
            "before 20:00, after 15:00: {}",
            super::super::hhmm(anchor)
        );
    }

    #[test]
    fn a_short_day_gets_one_reset_in_the_middle() {
        let usual = UsualHours {
            start: at(9, 0),
            len: 2 * 60,
        };
        assert_eq!(suggest_for(usual), Some(at(5, 0)), "reset at 10:00");
    }

    #[test]
    fn a_day_with_no_gap_gains_nothing() {
        let usual = UsualHours {
            start: 0,
            len: DAY_MIN,
        };
        assert_eq!(suggest_for(usual), None);
    }

    #[test]
    fn without_usual_hours_the_preview_is_the_heartbeat_alone() {
        let p = preview(at(5, 0), None);
        assert_eq!(
            p.windows,
            vec![Window {
                start: at(5, 0),
                end: at(10, 0)
            }]
        );
        assert_eq!(p.fresh_in_active, None);
    }
}
