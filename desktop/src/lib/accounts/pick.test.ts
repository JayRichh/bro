import { describe, expect, it } from 'vitest';
import { AUTO, autoLabel, initialProfile, profileOptions, removeWarning, stillOffered, type LaunchProfile } from './pick';

const p = (name: string, extra: Partial<LaunchProfile> = {}): LaunchProfile => ({ name, harness: 'claude', default: false, disabled: false, ...extra });

describe('the quick prompt profile picker', () => {
  it('starts on auto while a spread is on, so the spread chooses', () => {
    const list = [p('work', { default: true }), p('home')];
    expect(initialProfile(list, 'failover')).toBe(AUTO);
    expect(initialProfile(list, 'least_used')).toBe(AUTO);
  });
  it('under manual starts on the default in force, else auto', () => {
    expect(initialProfile([p('work', { default: true }), p('home')], 'manual')).toBe('work');
    expect(initialProfile([p('work', { default: true, disabled: true })], 'manual')).toBe(AUTO);
    expect(initialProfile([p('home')], undefined)).toBe(AUTO);
    expect(initialProfile([], 'manual')).toBe(AUTO);
  });
  it('keeps auto and listed names, drops a vanished one', () => {
    const list = [p('work')];
    expect(stillOffered(AUTO, list)).toBe(true);
    expect(stillOffered('work', list)).toBe(true);
    expect(stillOffered('gone', list)).toBe(false);
  });
  it('the auto option says the spread and what it would pick', () => {
    const list = [p('work'), p('home')];
    expect(autoLabel(list, 'failover', { spread: 'failover', profile: 'home', line: 'claude: home (failover: work at 92%)' })).toBe(
      'auto (failover \u2192 home)',
    );
    expect(autoLabel(list, 'least_used', { spread: 'least_used', profile: 'work', line: '' })).toBe('auto (least used \u2192 work)');
    expect(autoLabel(list, 'manual', { spread: 'manual', profile: 'work', line: '' })).toBe('auto (default \u2192 work)');
    expect(autoLabel(list, 'manual', { spread: 'manual', profile: null, line: '' })).toBe('auto (own login)');
    expect(autoLabel(list, 'failover', { spread: 'failover', profile: null, line: '' })).toBe('auto (failover \u2192 own login)');
    expect(autoLabel(list, 'failover', null)).toBe('auto (failover)');
    expect(autoLabel([], 'failover', null)).toBe('own login');
  });
  it('lists auto first, then each profile marked default or off', () => {
    const opts = profileOptions([p('work', { default: true }), p('old', { disabled: true })], 'failover', {
      spread: 'failover',
      profile: 'work',
      line: 'claude: work (failover, 12% used)',
    });
    expect(opts.map((o) => [o.value, o.label])).toEqual([
      [AUTO, 'auto (failover \u2192 work)'],
      ['work', 'work (default)'],
      ['old', 'old (off)'],
    ]);
    expect(opts[0].title).toContain('Now: claude: work (failover, 12% used)');
    expect(opts[2].title).toContain('auto never picks it');
  });
});

describe('removing a profile', () => {
  it('says the sessions under it stop being able to start, and counts them', () => {
    const agents = [
      { profile: 'work', archived: false },
      { profile: 'work', archived: false },
      { profile: 'work', archived: true },
      { profile: 'home', archived: false },
      { profile: null, archived: false },
    ];
    const text = removeWarning('work', agents);
    expect(text).toContain('2 sessions run under it, 1 archived one names it');
    expect(text).toContain('profile `work` is no longer defined');
    expect(text).not.toContain('default login');
    expect(removeWarning('spare', agents)).toBe('Remove the profile "spare"? No session runs under it.');
    expect(removeWarning('home', agents)).toContain('1 session runs under it.');
  });
});
