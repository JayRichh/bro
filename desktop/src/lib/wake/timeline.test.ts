import { describe, expect, it } from 'vitest';
import {
  MAX_ANCHOR,
  accountLabel,
  daily,
  domId,
  freshText,
  heat,
  hhmm,
  hoursText,
  liveWindow,
  localMinute,
  modeChoice,
  nudge,
  pickSession,
  pctAt,
  previewAt,
  snap,
  sliderFrac,
  slotsOf,
  span,
  stagger,
  strip,
  tickAt,
  ticks,
  toggleDay,
  weekday,
  windowsText,
  when,
} from './timeline';
import type { WakeAccount, WakePreview, WakeSchedule, WakeSessionWindow } from './types';

/** 2026-10-17 05:00 NZDT (+13h), a Saturday. */
const FIVE_NZ = 1_792_166_400;
const NZ = 13 * 3600;

describe('clock', () => {
  it('reads local minutes and weekdays off the offset', () => {
    expect(localMinute(FIVE_NZ, NZ)).toBe(300);
    expect(weekday(FIVE_NZ, NZ)).toBe(5);
    expect(localMinute(FIVE_NZ, 0)).toBe(960);
    expect(weekday(FIVE_NZ, 0)).toBe(4);
    expect(when(FIVE_NZ, NZ)).toBe('Sat 05:00');
    expect(hhmm(1425)).toBe('23:45');
    expect(hhmm(1440 + 60)).toBe('01:00');
    expect(hhmm(-15)).toBe('23:45');
  });
});

describe('the strip', () => {
  // Now is 05:00; the strip starts at 03:00 and runs to 03:00 tomorrow.
  const s = strip(FIVE_NZ, NZ);

  it('puts now a lead in from the left', () => {
    expect(s.startMinute).toBe(180);
    expect(s.nowPct).toBeCloseTo((120 / 1440) * 100);
    expect(pctAt(s, FIVE_NZ)).toBeCloseTo(s.nowPct);
    expect(pctAt(s, s.start - 999)).toBe(0);
    expect(pctAt(s, s.end + 999)).toBe(100);
  });

  it('places a daily stretch at each occurrence, both halves over the edge', () => {
    // 09:00-14:00 is six hours in, five long.
    const [seg, ...rest] = daily(s, 540, 840);
    expect(rest).toEqual([]);
    expect(seg.left).toBeCloseTo((360 / 1440) * 100);
    expect(seg.width).toBeCloseTo((300 / 1440) * 100);
    // 01:00-04:00 crosses the strip's start: 03:00-04:00 now, 01:00-03:00 tomorrow.
    const parts = daily(s, 60, 240);
    expect(parts).toHaveLength(2);
    const widths = parts.map((p) => Math.round((p.width / 100) * 1440)).sort((a, b) => a - b);
    expect(widths).toEqual([60, 120]);
    // A window running past midnight (22:00-03:00).
    const late = daily(s, 1320, 1620);
    expect(late.reduce((n, p) => n + (p.width / 100) * 1440, 0)).toBeCloseTo(300);
    expect(daily(s, 600, 600)).toEqual([]);
  });

  it('clips absolute stretches and shows a live window until it resets', () => {
    expect(span(s, s.start - 3600, s.start + 3600)).toEqual({ left: 0, width: (3600 / 86400) * 100 });
    expect(span(s, s.end, s.end + 10)).toBeNull();
    const live = liveWindow(
      s,
      { kind: 'session', label: null, percent: 12, resets_at: FIVE_NZ + 2 * 3600, read_at: FIVE_NZ },
      FIVE_NZ,
    );
    // Started 02:00, before the strip: drawn from 03:00 to 07:00.
    expect(live?.left).toBe(0);
    expect(Math.round(((live?.width ?? 0) / 100) * 1440)).toBe(240);
    expect(
      liveWindow(s, { kind: 'session', label: null, percent: 1, resets_at: FIVE_NZ - 1, read_at: 0 }, FIVE_NZ),
    ).toBeNull();
    expect(liveWindow(s, null, FIVE_NZ)).toBeNull();
  });

  it('turns the turn histogram into a heat row', () => {
    const bins = new Array(96).fill(0);
    bins[36] = 4; // 09:00
    bins[37] = 2;
    const cells = heat(s, bins);
    expect(cells).toHaveLength(2);
    expect(cells[0].level).toBe(1);
    expect(cells[1].level).toBe(0.5);
    expect(heat(s, null)).toEqual([]);
    expect(heat(s, new Array(96).fill(0))).toEqual([]);
    expect(heat(s, [1, 2])).toEqual([]);
  });
});

describe('the anchor', () => {
  it('moves in quarter hours and wraps', () => {
    expect(nudge(300, 1)).toBe(315);
    expect(nudge(0, -1)).toBe(1425);
    expect(nudge(1425, 1)).toBe(0);
    expect(nudge(300, -4)).toBe(240);
    expect(snap(307)).toBe(300);
    expect(snap(308)).toBe(315);
    expect(snap(1439)).toBe(0);
  });

  it('reads its preview from the precomputed list, or alone without usual hours', () => {
    const p = (fresh: number): WakePreview => ({ windows: [], fresh_in_active: fresh, baseline: 2 });
    const account = { previews: Array.from({ length: 96 }, (_, i) => p(i)) } as unknown as WakeAccount;
    expect(previewAt(account, 330).fresh_in_active).toBe(22);
    const none = { previews: null } as unknown as WakeAccount;
    expect(previewAt(none, 300)).toEqual({
      windows: [{ start: 300, end: 600 }],
      fresh_in_active: null,
      baseline: null,
    });
    expect(freshText(p(3))).toBe('3 fresh windows in your usual hours (2 without wake-up)');
    expect(freshText({ windows: [], fresh_in_active: 1, baseline: null })).toBe(
      '1 fresh window in your usual hours',
    );
    expect(freshText(previewAt(none, 300))).toBeNull();
    expect(hoursText({ start: 540, len: 480 })).toBe('09:00\u201317:00');
    expect(hoursText({ start: 1200, len: 420 })).toBe('20:00\u201303:00');
    expect(hoursText(null)).toBeNull();
  });
});

describe('days and stagger', () => {
  it('flips a day but never the last one', () => {
    expect(toggleDay(0x7f, 6)).toBe(0x3f);
    expect(toggleDay(0x1f, 5)).toBe(0x3f);
    expect(toggleDay(0b1, 0)).toBe(0b1);
  });

  it('spaces accounts two and a half hours apart from the first', () => {
    expect(
      stagger([
        { account: 'work', anchor: 300 },
        { account: 'max2', anchor: 300 },
        { account: '@claude', anchor: 600 },
      ]),
    ).toEqual({ work: 300, max2: 450, '@claude': 600 });
    expect(stagger([{ account: 'a', anchor: 1380 }, { account: 'b', anchor: 0 }])).toEqual({ a: 1380, b: 90 });
    expect(stagger([])).toEqual({});
  });
});

describe('the axis', () => {
  // Now is 05:00 in NZ; the strip starts at 03:00 sharp.
  const s = strip(FIVE_NZ, NZ);

  it('ticks every hour from the strip start, labelling every third', () => {
    const t = ticks(s);
    expect(t).toHaveLength(24);
    expect(t[0]).toEqual({ pct: 0, label: '03:00', major: false });
    expect(t[1].label).toBeNull();
    expect(t[3]).toMatchObject({ label: '06:00', major: true });
    expect(t[3].pct).toBeCloseTo((180 / 1440) * 100);
    expect(t.filter((x) => x.label).map((x) => x.label)).toEqual([
      '03:00', '06:00', '09:00', '12:00', '15:00', '18:00', '21:00', '00:00',
    ]);
    expect(t.filter((x) => x.major).map((x) => x.label)).toEqual(['06:00', '12:00', '18:00', '00:00']);
  });

  it('starts at the next full hour when the strip starts part way through one', () => {
    const t = ticks(strip(FIVE_NZ + 20 * 60, NZ));
    // The strip starts at 03:20: the first tick is 04:00, forty minutes in.
    expect(t[0].pct).toBeCloseTo((40 / 1440) * 100);
    expect(t).toHaveLength(24);
  });

  it('places a tick for an epoch inside the strip and none outside it', () => {
    expect(tickAt(s, FIVE_NZ)).toBeCloseTo(s.nowPct);
    expect(tickAt(s, s.start - 1)).toBeNull();
    expect(tickAt(s, s.end + 1)).toBeNull();
  });
});

describe('what the rows draw', () => {
  const win = (resets: number | null, read = FIVE_NZ): WakeSessionWindow => ({
    kind: 'session',
    label: null,
    percent: 10,
    resets_at: resets,
    read_at: read,
  });
  const sched = (over: Partial<WakeSchedule> = {}): WakeSchedule => ({
    enabled: true,
    anchor_min: 300,
    days: 127,
    keep_rolling: false,
    wake_pc: false,
    ...over,
  });

  it('prefers the live reading while its reset is ahead, else the kept one', () => {
    const live = win(FIVE_NZ + 3600);
    const kept = win(FIVE_NZ + 7200);
    expect(pickSession(live, kept, FIVE_NZ)).toBe(live);
    expect(pickSession(win(FIVE_NZ - 1), kept, FIVE_NZ)).toBe(kept);
    expect(pickSession(null, kept, FIVE_NZ)).toBe(kept);
    expect(pickSession(undefined, null, FIVE_NZ)).toBeNull();
  });

  it('marks the daemon slots while the draft matches them, else the anchor alone', () => {
    const account = { schedule: sched(), slots: [300, 600, 900] } as unknown as WakeAccount;
    expect(slotsOf(account, sched())).toEqual([300, 600, 900]);
    expect(slotsOf(account, sched({ anchor_min: 330 }))).toEqual([330]);
    expect(slotsOf(account, sched({ keep_rolling: true }))).toEqual([300]);
    expect(slotsOf({ schedule: sched(), slots: [] } as unknown as WakeAccount, sched())).toEqual([300]);
  });

  it('puts the slider fraction on the 15-minute grid, last step at 1', () => {
    expect(MAX_ANCHOR).toBe(1425);
    expect(sliderFrac(0)).toBe(0);
    expect(sliderFrac(1425)).toBe(1);
    expect(sliderFrac(712.5)).toBeCloseTo(0.5);
    expect(sliderFrac(9999)).toBe(1);
  });

  it('says the preview windows, the mode and the account plainly', () => {
    expect(
      windowsText({ windows: [{ start: 300, end: 600 }, { start: 600, end: 900 }], fresh_in_active: null, baseline: null }),
    ).toBe(`05:00${String.fromCharCode(0x2013)}10:00, 10:00${String.fromCharCode(0x2013)}15:00`);
    expect(modeChoice(sched())).toBe('auto');
    expect(modeChoice(sched({ mode: null }))).toBe('auto');
    expect(modeChoice(sched({ mode: 'print' }))).toBe('print');
    expect(accountLabel('@claude')).toBe('claude (own login)');
    expect(accountLabel('work')).toBe('work');
  });
});

describe('element ids', () => {
  it('turns an account key into a safe id part', () => {
    expect(domId('@claude')).toBe('_claude');
    expect(domId('my work')).toBe('my_work');
    expect(domId('work-2')).toBe('work-2');
  });
});
