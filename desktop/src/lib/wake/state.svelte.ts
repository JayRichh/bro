// Whakaoho's state in the window: the overview the view draws, what is
// being saved or woken right now, and the verbs other surfaces call (the
// account menu's Wake up now and Wake-up schedule..., the Accounts view's
// link, the palette).

import { api } from '../api';
import { nav } from '../shell/history.svelte';
import { toast } from '../store.svelte';
import type { WakeAccount, WakeNowResult, WakeOverview, WakeSchedule } from './types';

export const wake = $state({
  overview: null as WakeOverview | null,
  /** Why the last overview failed, if it did. */
  error: null as string | null,
  loading: false,
  /** Accounts with a Wake now in flight. */
  waking: {} as Record<string, boolean>,
  /** The last Wake now's line per account, until the next overview. */
  result: {} as Record<string, string>,
});

/** Read the overview again (`kete wake status --json`). */
export async function refreshWake(): Promise<void> {
  wake.loading = true;
  try {
    wake.overview = await api.wakeOverview();
    wake.error = null;
  } catch (e) {
    wake.error = String(e);
  } finally {
    wake.loading = false;
  }
}

/** The Whakaoho view, scrolled to `account` when one is named. */
export function openWake(account?: string) {
  nav.go({ view: 'whakaoho', item: account });
}

/** What a Wake now came to, in a line. */
export function wakeLine(r: WakeNowResult | undefined): string {
  if (!r) return 'Wake-up: no answer';
  const err = r.report?.error;
  if (err) return `Wake-up failed: ${err}`;
  if (r.decision === 'skip') return `Wake-up skipped: ${r.detail ?? r.reason}`;
  if (r.mode === 'print') return 'Woken with claude -p (print mode)';
  return r.report?.read ? 'Woken: the window started' : 'Woken: no usage reading came back yet';
}

/** Wake `account` now, whatever its schedule: up to a couple of minutes
 *  (a hidden Claude session), then the overview again. */
export async function wakeNowFor(account: string): Promise<void> {
  if (wake.waking[account]) return;
  wake.waking[account] = true;
  toast(`Waking ${account}\u2026`);
  try {
    const out = await api.wakeNow(account);
    const line = wakeLine(out.find((r) => r.account === account) ?? out[0]);
    wake.result[account] = line;
    toast(`${account}: ${line}`);
  } catch (e) {
    wake.result[account] = `Wake-up failed: ${e}`;
    toast(`${account}: wake-up failed: ${e}`);
  } finally {
    delete wake.waking[account];
    void refreshWake();
  }
}

// EDITING. A schedule changed in the view shows (and the timeline
// previews it) at once from a local draft; the save follows after a quiet
// spell and goes through one queue, because every save rewrites
// config.local.json and the scheduled task and two at a time would race.
// The draft stays until its own save has landed and the overview is read
// again, so the 60 s poll can't snap a slider back mid-drag.

export const wakeEdit = $state({
  /** The schedule being edited, by account: wins over the overview's. */
  draft: {} as Record<string, WakeSchedule>,
  /** Accounts with a save in flight. */
  saving: {} as Record<string, boolean>,
  /** What the last save said (the task's status line), for a few seconds. */
  line: {} as Record<string, string>,
  /** The account whose hour slider has focus: its estimated windows show
   *  on the timeline even while its switch is off. */
  active: null as string | null,
});

/** How long an edit must sit still before it is saved. */
export const SAVE_DELAY = 500;

const timers = new Map<string, ReturnType<typeof setTimeout>>();
const lineTimers = new Map<string, ReturnType<typeof setTimeout>>();
const revs = new Map<string, number>();
let queue: Promise<void> = Promise.resolve();

/** The schedule to show for an account: the draft, else the saved one. */
export function scheduleOf(a: WakeAccount): WakeSchedule {
  return wakeEdit.draft[a.account] ?? a.schedule;
}

function say(account: string, text: string, ms = 6000) {
  wakeEdit.line[account] = text;
  clearTimeout(lineTimers.get(account));
  lineTimers.set(
    account,
    setTimeout(() => {
      delete wakeEdit.line[account];
    }, ms),
  );
}

/** Change part of an account's schedule: shown at once, saved after
 *  `delay` ms without another change (0: now). */
export function editWake(a: WakeAccount, patch: Partial<WakeSchedule>, delay = SAVE_DELAY): void {
  const key = a.account;
  wakeEdit.draft[key] = { ...scheduleOf(a), ...patch };
  revs.set(key, (revs.get(key) ?? 0) + 1);
  clearTimeout(timers.get(key));
  timers.delete(key);
  if (delay <= 0) void flushWake(key);
  else
    timers.set(
      key,
      setTimeout(() => {
        timers.delete(key);
        void flushWake(key);
      }, delay),
    );
}

/** Save an account's draft now (after any save already queued). */
export function flushWake(account: string): Promise<void> {
  clearTimeout(timers.get(account));
  timers.delete(account);
  queue = queue.then(() => saveNow(account));
  return queue;
}

async function saveNow(account: string): Promise<void> {
  const draft = wakeEdit.draft[account];
  if (!draft) return;
  const rev = revs.get(account);
  wakeEdit.saving[account] = true;
  try {
    say(account, await api.wakeSet(account, $state.snapshot(draft)));
    await refreshWake();
  } catch (e) {
    say(account, `Could not save: ${e}`, 12_000);
    toast(`Could not save ${account}'s wake-up: ${e}`);
  } finally {
    delete wakeEdit.saving[account];
    // Changed again meanwhile: that edit has its own save coming.
    if (revs.get(account) === rev) delete wakeEdit.draft[account];
  }
}
