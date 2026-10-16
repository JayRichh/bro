// The log's window-side half: the local date, the port of `record`, the
// reads, and the command arguments the Rust side expects.
import { afterEach, describe, expect, it, vi } from 'vitest';

const invoke = vi.hoisted(() => vi.fn());
vi.mock('@tauri-apps/api/core', () => ({ invoke }));

import { critterApi, EMPTY_LOG, hiddenToday, names, record, today, type CritterLog, type LogNote } from './log';

afterEach(() => {
  invoke.mockReset();
  vi.useRealTimers();
});

describe('today', () => {
  it('pads the month and the day', () => {
    expect(today(new Date(2026, 0, 5, 9, 30))).toBe('2026-01-05');
    expect(today(new Date(2026, 9, 16, 12))).toBe('2026-10-16');
    expect(today(new Date(2026, 11, 31, 12))).toBe('2026-12-31');
  });

  it('is the local day at both ends of it, not the UTC one', () => {
    // Built from local parts, so these hold in any time zone. toISOString
    // would give the 15th for the first in NZ (UTC+13) and the 17th for
    // the second anywhere west of Greenwich.
    expect(today(new Date(2026, 9, 16, 0, 5))).toBe('2026-10-16');
    expect(today(new Date(2026, 9, 16, 23, 55))).toBe('2026-10-16');
  });

  it('defaults to now', () => {
    vi.useFakeTimers({ toFake: ['Date'] });
    vi.setSystemTime(new Date(2027, 1, 3, 20, 0));
    expect(today()).toBe('2027-02-03');
  });
});

describe('record', () => {
  const day = '2026-10-16';

  it('counts a visit and keeps the first date on the next one', () => {
    const one = record(EMPTY_LOG, { kind: 'seen', species: 'kiwi' }, day);
    expect(one.species.kiwi).toEqual({ seen: 1, pets: 0, fed: 0, first_seen: day });
    const two = record(one, { kind: 'seen', species: 'kiwi' }, '2026-10-17');
    expect(two.species.kiwi.seen).toBe(2);
    expect(two.species.kiwi.first_seen).toBe(day);
  });

  it('counts pets, feeds and lamps', () => {
    let log = record(EMPTY_LOG, { kind: 'petted', species: 'cat' }, day);
    log = record(log, { kind: 'petted', species: 'cat' }, day);
    log = record(log, { kind: 'fed', species: 'cat' }, day);
    log = record(log, { kind: 'lamp' }, day);
    log = record(log, { kind: 'lamp' }, day);
    // Petting a species before it was ever seen starts its row without a first date.
    expect(log.species.cat).toEqual({ seen: 0, pets: 2, fed: 1 });
    expect(log.lamps).toBe(2);
  });

  it('sets the given name, and a new one replaces it', () => {
    let log = record(EMPTY_LOG, { kind: 'named', species: 'kiwi', name: 'Bob' }, day);
    expect(log.species.kiwi.name).toBe('Bob');
    log = record(log, { kind: 'named', species: 'kiwi', name: 'Pip' }, day);
    expect(log.species.kiwi.name).toBe('Pip');
  });

  it('returns a new log and leaves the old one alone', () => {
    const before: CritterLog = { version: 1, species: { kiwi: { seen: 1, pets: 0, fed: 0, first_seen: day } }, lamps: 0 };
    const snapshot = JSON.stringify(before);
    const notes: LogNote[] = [
      { kind: 'seen', species: 'kiwi' },
      { kind: 'petted', species: 'kiwi' },
      { kind: 'fed', species: 'tui' },
      { kind: 'named', species: 'kiwi', name: 'Bob' },
      { kind: 'lamp' },
    ];
    for (const n of notes) {
      const after = record(before, n, day);
      expect(after).not.toBe(before);
      expect(JSON.stringify(before)).toBe(snapshot);
    }
  });

  it('keeps the other species, the version and the hidden day', () => {
    const before: CritterLog = {
      version: 1,
      species: { cat: { seen: 4, pets: 1, fed: 0, first_seen: '2026-10-01' } },
      lamps: 3,
      hidden_on: day,
    };
    const after = record(before, { kind: 'seen', species: 'kiwi' }, day);
    expect(after.species.cat).toEqual(before.species.cat);
    expect(after.lamps).toBe(3);
    expect(after.hidden_on).toBe(day);
    expect(after.version).toBe(1);
  });
});

describe('names', () => {
  it('maps species ids to the names given, and skips the unnamed', () => {
    let log = record(EMPTY_LOG, { kind: 'seen', species: 'cat' }, '2026-10-16');
    log = record(log, { kind: 'named', species: 'kiwi', name: 'Bob' }, '2026-10-16');
    log = record(log, { kind: 'named', species: 'tui', name: 'Pip' }, '2026-10-16');
    expect(names(log)).toEqual(
      new Map([
        ['kiwi', 'Bob'],
        ['tui', 'Pip'],
      ]),
    );
    expect(names(EMPTY_LOG).size).toBe(0);
  });
});

describe('hiddenToday', () => {
  it('is true only while the stored day is the given day', () => {
    const log: CritterLog = { ...EMPTY_LOG, hidden_on: '2026-10-16' };
    expect(hiddenToday(log, '2026-10-16')).toBe(true);
    expect(hiddenToday(log, '2026-10-17')).toBe(false);
    expect(hiddenToday(EMPTY_LOG, '2026-10-16')).toBe(false);
  });

  it('reads today by default', () => {
    vi.useFakeTimers({ toFake: ['Date'] });
    vi.setSystemTime(new Date(2026, 9, 16, 22, 0));
    expect(hiddenToday({ ...EMPTY_LOG, hidden_on: '2026-10-16' })).toBe(true);
    vi.setSystemTime(new Date(2026, 9, 17, 0, 1));
    expect(hiddenToday({ ...EMPTY_LOG, hidden_on: '2026-10-16' })).toBe(false);
  });
});

describe('critterApi', () => {
  it('sends the Rust commands their arguments', async () => {
    vi.useFakeTimers({ toFake: ['Date'] });
    vi.setSystemTime(new Date(2026, 9, 16, 12));
    invoke.mockResolvedValue(EMPTY_LOG);
    const notes: LogNote[] = [{ kind: 'seen', species: 'kiwi' }, { kind: 'lamp' }];

    await critterApi.log();
    expect(invoke).toHaveBeenLastCalledWith('critters_log');
    await critterApi.update(notes);
    expect(invoke).toHaveBeenLastCalledWith('critters_log_update', { notes, hide: null, today: '2026-10-16' });
    await critterApi.update([], true);
    expect(invoke).toHaveBeenLastCalledWith('critters_log_update', { notes: [], hide: true, today: '2026-10-16' });
    await critterApi.update([], false);
    expect(invoke).toHaveBeenLastCalledWith('critters_log_update', { notes: [], hide: false, today: '2026-10-16' });
    await critterApi.prefs();
    expect(invoke).toHaveBeenLastCalledWith('critters_prefs');
  });
});
