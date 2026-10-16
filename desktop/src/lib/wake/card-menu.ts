// A Whakaoho card's own menu (menu matrix \u00a75.21): one account's wake-up
// rows, opened by a right-click anywhere on its card (`use:ownMenu`, never a
// `data-target`, so the account rows' keys don't act from here). Wake up now
// follows the account menu's rules (actions.ts `wakeNowVerb`): greyed while
// the account is switched off, not signed in, or waking. Pure: WakeCard
// hands in the state and the verbs, so the tests build both.

import type { MenuEntry, MenuHeading } from '../ui/ui.svelte';
import { hhmm, snap } from './timeline';
import type { WakeAccount, WakeSchedule } from './types';

export interface WakeCardState {
  a: WakeAccount;
  /** The schedule as the card shows it (an edit not saved yet included). */
  sched: WakeSchedule;
  waking: boolean;
}

export interface WakeCardVerbs {
  wakeNow(): void;
  setEnabled(on: boolean): void;
  /** Move the hour to `min` (the suggested one, snapped to its step). */
  setHour(min: number): void;
  copyLogin(hint: string): void;
  /** The account's row in the Accounts view. */
  details(): void;
}

const off = (why: string) => ({ disabled: true, title: why });

/** The card's state worth a line: logged out (ember while the wake-up is
 *  on, as the card says), switched off in Accounts. */
function banner(a: WakeAccount, on: boolean): MenuHeading | null {
  if (!a.known) return { heading: 'no such account' };
  if (!a.logged_in) return { heading: 'logged out', ...(on ? { ember: true } : {}) };
  if (a.disabled) return { heading: 'switched off' };
  return null;
}

export function wakeCardItems(s: WakeCardState, v: WakeCardVerbs): MenuEntry[] {
  const { a, sched } = s;
  const head = banner(a, sched.enabled);
  const out: MenuEntry[] = head ? [head] : [];
  // A schedule left for an account that is gone: only switching it off.
  if (!a.known) {
    if (sched.enabled) out.push({ label: 'Wake-up on', checked: true, run: () => v.setEnabled(false), title: 'Switch this schedule off' });
    return out;
  }
  const why = a.disabled
    ? 'Switched off: switch it on to wake it.'
    : !a.logged_in
      ? `Log in first: ${a.login_hint ?? 'sign it in'}`
      : s.waking
        ? 'Waking now: up to two minutes.'
        : null;
  out.push({ label: 'Wake up now', run: v.wakeNow, ...(why ? off(why) : { title: 'Send the tiny prompt now, whatever the schedule' }) });
  out.push({ label: 'Wake-up on', checked: sched.enabled, run: () => v.setEnabled(!sched.enabled) });
  if (a.suggested != null && snap(a.suggested) !== sched.anchor_min) {
    const at = snap(a.suggested);
    out.push({ label: `Use the suggested hour, ${hhmm(at)}`, run: () => v.setHour(at), title: 'The most fresh windows inside your usual hours' });
  }
  if (!a.logged_in && a.login_hint) {
    const hint = a.login_hint;
    out.push({ label: 'Copy sign-in command', run: () => v.copyLogin(hint), title: hint });
  }
  out.push('sep', { label: 'Account details\u2026', run: v.details });
  return out;
}
