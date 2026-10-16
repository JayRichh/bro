// Why a Claude session's last turn stopped short, in a tone, a few words
// and a tooltip. This mirrors crates/nebula-core/src/stop.rs (`class`,
// `reason_label`, `StopNote::tone`, `StopNote::label`): the terminal app
// reads the same file, and both must say the same words. The one
// difference is the clock: Rust has no local zone to print, so a reset
// only the account knew reads `resumes in 2h 5m` there and `resumes 3:45pm`
// here. Pure, so the node tests reach it.

import type { StopNote } from './api';

/** How a note reads: the badge's colour. Waiting is the session carrying
 *  on by itself (never ember); needs is you; error is a turn that ended. */
export type StopTone = 'waiting' | 'needs' | 'error';

/** An epoch-ms instant as the reader's local clock time. */
export type Clock = (ms: number) => string;

const RATE_LIMIT = 'rate_limit';
const QUOTA_RESUME_STALE = 'quota_auto_resume_stale';
const QUOTA_RESUME_DISABLED = 'quota_auto_resume_disabled';

/** How far off a reset may be and Claude still waits for it on its own;
 *  further out, it opens its usage-limit menu instead. */
const AUTO_WAIT_MAX_SECS = 24 * 3600;

const localClock: Clock = (ms) => new Date(ms).toLocaleTimeString([], { hour: 'numeric', minute: '2-digit' });

/** Errors only the user can fix: core's `StopClass::NeedsYou`. */
const NEEDS_YOU = new Set([
  'authentication_failed',
  'billing_error',
  'account_on_hold',
  'oauth_org_not_allowed',
  'cloud_credential_error',
  'model_not_found',
  QUOTA_RESUME_STALE,
  QUOTA_RESUME_DISABLED,
]);

// Maps, not objects: the reason is free text, and "constructor" is not a
// word of ours.
/** The short words for a reason on a card: core's `reason_label`. */
const WORDS = new Map([
  [RATE_LIMIT, 'usage limit'],
  ['authentication_failed', 'logged out'],
  ['billing_error', 'billing'],
  ['account_on_hold', 'account on hold'],
  ['oauth_org_not_allowed', 'org not allowed'],
  ['cloud_credential_error', 'credentials'],
  ['model_not_found', 'model not found'],
  ['overloaded', 'overloaded'],
  ['max_output_tokens', 'output limit'],
  ['invalid_request', 'bad request'],
  [QUOTA_RESUME_STALE, 'limit reset'],
  [QUOTA_RESUME_DISABLED, 'limit wait ended'],
]);

/** What to do about a needs-you reason, for the tooltip. */
const INSTRUCTION = new Map([
  ['authentication_failed', 'Logged out: run /login in the session'],
  ['billing_error', 'Billing problem on this account'],
  ['account_on_hold', 'Account on hold'],
  ['oauth_org_not_allowed', 'This organisation is not allowed'],
  ['cloud_credential_error', 'Cloud credentials failed'],
  ['model_not_found', 'Model not found: pick another with /model'],
  [QUOTA_RESUME_STALE, 'Usage limit reset while the computer slept: press Enter in the session to continue'],
  [QUOTA_RESUME_DISABLED, 'Claude stopped waiting for the usage limit: send a prompt to continue'],
]);

const reasonWords = (reason: string) => WORDS.get(reason) ?? 'API error';

/** Claude is waiting out a usage limit and continues on its own. */
const isWaiting = (n: StopNote) => n.reason === RATE_LIMIT && n.waiting;

/** A limit whose reset was beyond a day when it was noted: Claude opened
 *  its usage-limit menu instead of waiting. */
function isMenu(n: StopNote): boolean {
  return !n.waiting && n.resets_at != null && n.resets_at - Math.trunc(n.at / 1000) > AUTO_WAIT_MAX_SECS;
}

/** The reset as clock time, when the account's reading is still ahead. */
function resetClock(n: StopNote, nowMs: number, clock: Clock): string | null {
  return n.resets_at != null && n.resets_at * 1000 > nowMs ? clock(n.resets_at * 1000) : null;
}

/** How the note reads: core's `StopNote::tone`. */
export function stopTone(n: StopNote): StopTone {
  if (n.reason === RATE_LIMIT) return n.waiting ? 'waiting' : isMenu(n) ? 'needs' : 'error';
  return NEEDS_YOU.has(n.reason) ? 'needs' : 'error';
}

/** The note's words for a chip: `resumes 3:45pm` (Claude's own clock text,
 *  else the account's reset as local time), `waiting on limit`,
 *  `weekly limit`, or the reason's words. Core's `StopNote::label`. */
export function stopLabel(n: StopNote, nowMs: number, clock: Clock = localClock): string {
  if (isWaiting(n)) {
    if (n.resumes != null) return `resumes ${n.resumes}`;
    const at = resetClock(n, nowMs, clock);
    return at ? `resumes ${at}` : 'waiting on limit';
  }
  if (n.reason === RATE_LIMIT && isMenu(n)) return 'weekly limit';
  return reasonWords(n.reason);
}

/** One sentence for the chip's tooltip, then Claude's own words when it
 *  gave any. */
export function stopTitle(n: StopNote, nowMs: number = Date.now(), clock: Clock = localClock): string {
  let line: string;
  if (isWaiting(n)) {
    line = `Usage limit: Claude continues on its own at ${n.resumes ?? resetClock(n, nowMs, clock) ?? 'the reset'}`;
  } else if (n.reason === RATE_LIMIT && isMenu(n)) {
    line = "Usage limit: Claude's options menu is open in the session";
  } else if (NEEDS_YOU.has(n.reason)) {
    line = INSTRUCTION.get(n.reason) ?? reasonWords(n.reason);
  } else {
    const words = reasonWords(n.reason);
    line = words === 'API error' ? words : `API error: ${words}`;
  }
  return n.detail ? `${line} \u00B7 ${n.detail}` : line;
}
