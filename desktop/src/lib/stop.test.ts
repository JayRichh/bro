import { describe, expect, it } from 'vitest';
import type { StopNote } from './api';
import { stopLabel, stopTitle, stopTone } from './stop';

// Epoch ms; the note's own `at`.
const NOW = 1_700_000_000_000;
const NOW_S = NOW / 1000;
const DAY = 86_400;

const note = (reason: string, extra: Partial<StopNote> = {}): StopNote => ({
  reason,
  waiting: false,
  resumes: null,
  resets_at: null,
  detail: null,
  at: NOW,
  ...extra,
});
const waiting = (extra: Partial<StopNote> = {}) => note('rate_limit', { waiting: true, ...extra });

// A clock that does not depend on the machine's zone, and records what it
// was asked to print.
const asked: number[] = [];
const clock = (ms: number) => {
  asked.push(ms);
  return `clock(${ms / 1000})`;
};

describe('every documented error value has a tone and words', () => {
  // [reason, tone, words]: the same table as core's stop.rs.
  const table: [string, 'needs' | 'error', string][] = [
    ['rate_limit', 'error', 'usage limit'],
    ['overloaded', 'error', 'overloaded'],
    ['authentication_failed', 'needs', 'logged out'],
    ['oauth_org_not_allowed', 'needs', 'org not allowed'],
    ['account_on_hold', 'needs', 'account on hold'],
    ['billing_error', 'needs', 'billing'],
    ['invalid_request', 'error', 'bad request'],
    ['model_not_found', 'needs', 'model not found'],
    ['server_error', 'error', 'API error'],
    ['max_output_tokens', 'error', 'output limit'],
    ['cloud_credential_error', 'needs', 'credentials'],
    ['unknown', 'error', 'API error'],
    ['quota_auto_resume_stale', 'needs', 'limit reset'],
    ['quota_auto_resume_disabled', 'needs', 'limit wait ended'],
  ];
  for (const [reason, tone, words] of table) {
    it(`${reason}: ${tone}, "${words}"`, () => {
      expect(stopTone(note(reason))).toBe(tone);
      expect(stopLabel(note(reason), NOW, clock)).toBe(words);
    });
  }
  it('a kind Claude adds later reads as an API error, never a failure', () => {
    expect(stopTone(note('a_kind_claude_adds_later'))).toBe('error');
    expect(stopLabel(note('a_kind_claude_adds_later'), NOW, clock)).toBe('API error');
    expect(stopTitle(note('a_kind_claude_adds_later'), NOW, clock)).toBe('API error');
  });
  it('a reason that is a property name of Object is still just an unknown reason', () => {
    for (const reason of ['constructor', 'toString', '__proto__']) {
      expect(stopTone(note(reason))).toBe('error');
      expect(stopLabel(note(reason), NOW, clock)).toBe('API error');
    }
  });
  it('waiting on something that is not a usage limit changes nothing', () => {
    const n = note('authentication_failed', { waiting: true, resumes: '3:45pm' });
    expect(stopTone(n)).toBe('needs');
    expect(stopLabel(n, NOW, clock)).toBe('logged out');
  });
});

describe('a usage limit Claude waits out', () => {
  it('is waiting, never needs-you', () => {
    expect(stopTone(waiting())).toBe('waiting');
  });
  it("says Claude's own clock text when the screen gave one", () => {
    const n = waiting({ resumes: '3:45pm', resets_at: NOW_S + 3600 });
    asked.length = 0;
    expect(stopLabel(n, NOW, clock)).toBe('resumes 3:45pm');
    expect(asked).toEqual([]);
  });
  it('falls back to the account reset as local clock time', () => {
    const n = waiting({ resets_at: NOW_S + 2 * 3600 + 5 * 60 });
    asked.length = 0;
    expect(stopLabel(n, NOW, clock)).toBe(`resumes clock(${NOW_S + 7500})`);
    expect(asked).toEqual([(NOW_S + 7500) * 1000]);
  });
  it('the default clock prints a time of day', () => {
    expect(stopLabel(waiting({ resets_at: NOW_S + 3600 }), NOW)).toMatch(/^resumes \S/);
  });
  it('says only that it waits when there is no future reset to name', () => {
    expect(stopLabel(waiting(), NOW, clock)).toBe('waiting on limit');
    expect(stopLabel(waiting({ resets_at: NOW_S - 60 }), NOW, clock)).toBe('waiting on limit');
    expect(stopLabel(waiting({ resets_at: NOW_S }), NOW, clock)).toBe('waiting on limit');
  });
});

describe('a usage limit Claude does not wait out', () => {
  it('is the weekly-limit menu when the reset was beyond a day', () => {
    const n = note('rate_limit', { resets_at: NOW_S + 4 * DAY });
    expect(stopTone(n)).toBe('needs');
    expect(stopLabel(n, NOW, clock)).toBe('weekly limit');
  });
  it('is an error when the reset was within a day, or unknown', () => {
    for (const resets_at of [NOW_S + 3600, null]) {
      const n = note('rate_limit', { resets_at });
      expect(stopTone(n)).toBe('error');
      expect(stopLabel(n, NOW, clock)).toBe('usage limit');
    }
  });
  it('measures the day from when it was noted, strictly', () => {
    expect(stopTone(note('rate_limit', { resets_at: NOW_S + DAY }))).toBe('error');
    expect(stopTone(note('rate_limit', { resets_at: NOW_S + DAY + 1 }))).toBe('needs');
  });
});

describe('the tooltip', () => {
  it('says when a waiting limit ends: the screen, the clock, or just "the reset"', () => {
    expect(stopTitle(waiting({ resumes: '3:45pm' }), NOW, clock)).toBe(
      'Usage limit: Claude continues on its own at 3:45pm',
    );
    expect(stopTitle(waiting({ resets_at: NOW_S + 600 }), NOW, clock)).toBe(
      `Usage limit: Claude continues on its own at clock(${NOW_S + 600})`,
    );
    expect(stopTitle(waiting(), NOW, clock)).toBe('Usage limit: Claude continues on its own at the reset');
    expect(stopTitle(waiting({ resets_at: NOW_S - 600 }), NOW, clock)).toBe(
      'Usage limit: Claude continues on its own at the reset',
    );
  });
  it('tells the reader what a needs-you reason wants', () => {
    const words: Record<string, string> = {
      authentication_failed: 'Logged out: run /login in the session',
      billing_error: 'Billing problem on this account',
      account_on_hold: 'Account on hold',
      oauth_org_not_allowed: 'This organisation is not allowed',
      cloud_credential_error: 'Cloud credentials failed',
      model_not_found: 'Model not found: pick another with /model',
      quota_auto_resume_stale: 'Usage limit reset while the computer slept: press Enter in the session to continue',
      quota_auto_resume_disabled: 'Claude stopped waiting for the usage limit: send a prompt to continue',
    };
    for (const [reason, title] of Object.entries(words)) {
      expect(stopTitle(note(reason), NOW, clock)).toBe(title);
    }
  });
  it('names the weekly-limit menu, and calls a limit that just ended the turn an API error', () => {
    expect(stopTitle(note('rate_limit', { resets_at: NOW_S + 4 * DAY }), NOW, clock)).toBe(
      "Usage limit: Claude's options menu is open in the session",
    );
    expect(stopTitle(note('rate_limit'), NOW, clock)).toBe('API error: usage limit');
  });
  it('calls every other stop an API error, with the words when it has them', () => {
    expect(stopTitle(note('overloaded'), NOW, clock)).toBe('API error: overloaded');
    expect(stopTitle(note('max_output_tokens'), NOW, clock)).toBe('API error: output limit');
    expect(stopTitle(note('invalid_request'), NOW, clock)).toBe('API error: bad request');
    expect(stopTitle(note('server_error'), NOW, clock)).toBe('API error');
  });
  it("appends Claude's own words when it gave any", () => {
    const detail = 'Credit balance is too low';
    expect(stopTitle(note('billing_error', { detail }), NOW, clock)).toBe(
      `Billing problem on this account \u00B7 ${detail}`,
    );
    expect(stopTitle(note('overloaded', { detail }), NOW, clock)).toBe(`API error: overloaded \u00B7 ${detail}`);
    expect(stopTitle(waiting({ resumes: '3:45pm', detail }), NOW, clock)).toBe(
      `Usage limit: Claude continues on its own at 3:45pm \u00B7 ${detail}`,
    );
    expect(stopTitle(note('overloaded', { detail: '' }), NOW, clock)).toBe('API error: overloaded');
  });
});
