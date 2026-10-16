// The Whakaoho view's arithmetic: where things sit on the 24-hour strip,
// the anchor's steps, the days, the stagger. Pure, so the view stays
// markup and these stay tested (timeline.test.ts).
//
// The strip runs 24 hours from a little before now: daily things (the
// usual hours, the windows an anchor makes, the heat) are drawn at each
// of their occurrences inside it, absolute ones (a live window, the next
// run) where they fall.

import type { WakeAccount, WakeModeChoice, WakePreview, WakeSchedule, WakeSessionWindow } from './types';

export const DAY = 1440;
export const STEP = 15;
export const WINDOW = 300;
/** How much of the past the strip shows before the now line, in minutes. */
export const LEAD = 120;

export const DAY_LABELS = ['Mon', 'Tue', 'Wed', 'Thu', 'Fri', 'Sat', 'Sun'] as const;
export const ALL_DAYS = 0x7f;
export const WEEKDAYS = 0x1f;

/** `HH:MM` for minutes after midnight (wrapping). */
export function hhmm(min: number): string {
  const m = ((Math.round(min) % DAY) + DAY) % DAY;
  return `${String(Math.floor(m / 60)).padStart(2, '0')}:${String(m % 60).padStart(2, '0')}`;
}

/** Minutes after local midnight of an epoch second, `offset` seconds east of UTC. */
export function localMinute(epoch: number, offset: number): number {
  const local = epoch + offset;
  return Math.floor((((local % 86400) + 86400) % 86400) / 60);
}

/** 0 = Monday ... 6 = Sunday. */
export function weekday(epoch: number, offset: number): number {
  const day = Math.floor((epoch + offset) / 86400);
  // 1970-01-01 was a Thursday.
  return (((day + 3) % 7) + 7) % 7;
}

/** `Sat 05:30`. */
export function when(epoch: number, offset: number): string {
  return `${DAY_LABELS[weekday(epoch, offset)]} ${hhmm(localMinute(epoch, offset))}`;
}

/** The strip: 24 hours of epoch seconds from `start`. */
export interface Strip {
  start: number;
  end: number;
  offset: number;
  /** The local minute of the day the strip starts at. */
  startMinute: number;
  /** Where now sits, in percent. */
  nowPct: number;
}

export function strip(now: number, offset: number, lead = LEAD): Strip {
  const start = now - lead * 60;
  return {
    start,
    end: start + 86400,
    offset,
    startMinute: localMinute(start, offset),
    nowPct: (lead / DAY) * 100,
  };
}

/** A stretch of the strip, in percent. */
export interface Seg {
  left: number;
  width: number;
}

/** Percent across the strip of an epoch second, clamped to it. */
export function pctAt(s: Strip, epoch: number): number {
  return Math.min(100, Math.max(0, ((epoch - s.start) / 86400) * 100));
}

/** An absolute stretch (epoch seconds) clipped to the strip; null when outside it. */
export function span(s: Strip, from: number, to: number): Seg | null {
  const a = Math.max(from, s.start);
  const b = Math.min(to, s.end);
  if (b <= a) return null;
  return { left: ((a - s.start) / 86400) * 100, width: ((b - a) / 86400) * 100 };
}

/** A daily stretch (minutes of the local day; `end` past 1440 runs into
 *  the next) at each of its occurrences inside the strip. */
export function daily(s: Strip, start: number, end: number): Seg[] {
  const len = end - start;
  if (len <= 0) return [];
  // Minutes from the strip's start to the stretch's first start, then a
  // day before and after, so a stretch over the strip's edge shows both
  // halves.
  const first = (((start - s.startMinute) % DAY) + DAY) % DAY;
  const out: Seg[] = [];
  for (const at of [first - DAY, first, first + DAY]) {
    const a = Math.max(at, 0);
    const b = Math.min(at + len, DAY);
    if (b > a) out.push({ left: (a / DAY) * 100, width: ((b - a) / DAY) * 100 });
  }
  return out;
}

/** The live five-hour window: from its start (reset - 5 h) to its reset,
 *  while the reset is still ahead; null otherwise. */
export function liveWindow(s: Strip, w: WakeSessionWindow | null, now: number): Seg | null {
  if (!w || w.resets_at == null || w.resets_at <= now) return null;
  return span(s, w.resets_at - WINDOW * 60, w.resets_at);
}

/** Each quarter hour with turns in it, and how busy it was (0-1 against the busiest). */
export function heat(s: Strip, bins: number[] | null): { seg: Seg; level: number }[] {
  if (!bins || bins.length !== DAY / STEP) return [];
  const most = Math.max(...bins);
  if (most <= 0) return [];
  return bins.flatMap((n, i) =>
    n > 0 ? daily(s, i * STEP, (i + 1) * STEP).map((seg) => ({ seg, level: n / most })) : [],
  );
}

/** The anchor moved by `steps` quarter hours, wrapping round the day. */
export function nudge(anchor: number, steps: number): number {
  return (((anchor + steps * STEP) % DAY) + DAY) % DAY;
}

/** Put a minute on the anchor's grid (15-minute steps inside the day). */
export function snap(min: number): number {
  const m = ((Math.round(min / STEP) * STEP) % DAY + DAY) % DAY;
  return m;
}

/** What an anchor does to the account's day: the precomputed preview for
 *  it when the usual hours are known, else its own window alone. */
export function previewAt(account: WakeAccount, anchor: number): WakePreview {
  const at = account.previews?.[Math.floor(snap(anchor) / STEP)];
  if (at) return at;
  return { windows: [{ start: anchor, end: anchor + WINDOW }], fresh_in_active: null, baseline: null };
}

/** `3 fresh windows in your usual hours (2 without wake-up)`; null without usual hours. */
export function freshText(p: WakePreview): string | null {
  if (p.fresh_in_active == null) return null;
  const n = p.fresh_in_active;
  const base = p.baseline == null ? '' : ` (${p.baseline} without wake-up)`;
  return `${n} fresh window${n === 1 ? '' : 's'} in your usual hours${base}`;
}

/** The days mask with day `i` (0 = Monday) flipped; never down to no day. */
export function toggleDay(mask: number, i: number): number {
  const next = (mask ^ (1 << i)) & ALL_DAYS;
  return next === 0 ? mask : next;
}

export function onDay(mask: number, i: number): boolean {
  return (mask & (1 << i)) !== 0;
}

/** Stagger: the first account keeps its anchor and each next one sits
 *  `step` minutes after the one before, so their resets interleave and a
 *  spread always has a fresh window to move to. */
export function stagger(anchors: { account: string; anchor: number }[], step = 150): Record<string, number> {
  const out: Record<string, number> = {};
  const first = anchors[0]?.anchor ?? 0;
  anchors.forEach((a, i) => {
    out[a.account] = snap(first + i * step);
  });
  return out;
}

/** The usual hours as `09:00-17:00`. */
export function hoursText(u: { start: number; len: number } | null): string | null {
  if (!u) return null;
  return `${hhmm(u.start)}\u2013${hhmm(u.start + u.len)}`;
}

/** The slider's top step: the last quarter hour of the day. */
export const MAX_ANCHOR = DAY - STEP;

/** How far along the slider (0-1) an anchor sits. */
export function sliderFrac(anchor: number): number {
  return Math.min(1, Math.max(0, anchor / MAX_ANCHOR));
}

export interface Tick {
  /** Percent across the strip. */
  pct: number;
  /** `HH:MM` on every third hour, else null. */
  label: string | null;
  /** Every sixth hour: the labels that stay when the strip is narrow. */
  major: boolean;
}

/** The strip's hour ticks, each at its local hour boundary. */
export function ticks(s: Strip): Tick[] {
  const out: Tick[] = [];
  for (let m = (60 - (s.startMinute % 60)) % 60; m < DAY; m += 60) {
    const local = (s.startMinute + m) % DAY;
    const hour = local / 60;
    out.push({ pct: (m / DAY) * 100, label: hour % 3 === 0 ? hhmm(local) : null, major: hour % 6 === 0 });
  }
  return out;
}

/** Percent across the strip of an epoch second; null when it falls outside. */
export function tickAt(s: Strip, epoch: number): number | null {
  return epoch < s.start || epoch > s.end ? null : pctAt(s, epoch);
}

/** The five-hour window to draw: the live reading while its reset is
 *  still ahead, else the one the overview kept. */
export function pickSession(
  live: WakeSessionWindow | null | undefined,
  kept: WakeSessionWindow | null,
  now: number,
): WakeSessionWindow | null {
  if (live && live.resets_at != null && live.resets_at > now) return live;
  return kept;
}

/** The minutes of the day to mark as wake-ups: the daemon's slots while
 *  the draft still matches what it computed them for, else just the
 *  anchor being dragged. */
export function slotsOf(account: WakeAccount, sched: WakeSchedule): number[] {
  const saved = account.schedule;
  const same = sched.anchor_min === saved.anchor_min && sched.keep_rolling === saved.keep_rolling;
  return same && account.slots.length ? account.slots : [sched.anchor_min];
}

/** A preview's windows as `05:00-10:00, 10:00-15:00` (en dashes). */
export function windowsText(p: WakePreview): string {
  return p.windows.map((w) => hoursText({ start: w.start, len: w.end - w.start })).join(', ');
}

/** The mode the options show: no mode saved is auto. */
export function modeChoice(sched: WakeSchedule): WakeModeChoice {
  return sched.mode ?? 'auto';
}

/** An account's name as the view says it (`@claude` is the CLI's own login). */
export function accountLabel(account: string): string {
  return account === '@claude' ? 'claude (own login)' : account;
}

/** An account key as an element id part: a profile name may hold spaces,
 *  which an id (and a label's `for`) can't. Never use it in a selector. */
export function domId(account: string): string {
  return account.replace(/[^A-Za-z0-9_-]/g, '_');
}
