// The branch switcher's rules, apart from its window: which rows a query
// shows (and when the typed name is a branch to create), which branches
// it refuses, how often it fetches, and the two-press discard. As the
// terminal app's `c` (crates/nebula-tui/src/branch_switch.rs).

import { rank } from '../fuzzy';
import type { Branch } from './api';

export type Row = { kind: 'branch'; branch: Branch; positions: number[] } | { kind: 'create'; name: string };

/** The least time between two fetches of one checkout. */
export const FETCH_GAP_MS = 60_000;

/** A typed name as a branch name: trimmed, spaces as hyphens. */
export function branchName(input: string): string {
  return input.trim().replace(/\s+/g, '-');
}

/** The local branch a row lands on: `origin/x` lands on `x`. */
export function localName(b: { name: string; remote: boolean }): string {
  if (!b.remote) return b.name;
  const i = b.name.indexOf('/');
  return i < 0 ? b.name : b.name.slice(i + 1);
}

/** The rows for `query`: the matching branches, best first, then a row
 *  that creates the typed name when no branch is called that. */
export function rows(list: Branch[], query: string): Row[] {
  const out: Row[] = rank(query, list, (b) => b.name).map((r) => ({ kind: 'branch', branch: r.item, positions: r.positions }));
  const name = branchName(query);
  if (name && !list.some((b) => b.name === name || localName(b) === name)) out.push({ kind: 'create', name });
  return out;
}

/** Why a row can't be switched to, or null. git keeps a branch in one
 *  checkout at a time, so one another worktree holds is refused. */
export function refusal(b: Branch): string | null {
  if (b.current) return `Already on ${b.name}.`;
  if (b.checked_out_at) return `${b.name} is checked out in another worktree (${b.checked_out_at}): git keeps a branch in one checkout at a time.`;
  return null;
}

/** Whether a checkout last fetched at `last` (ms) may fetch again now. */
export function fetchDue(last: number | undefined, now: number): boolean {
  return last === undefined || now - last >= FETCH_GAP_MS;
}

/** How uncommitted changes travel, in the order offered: the safe,
 *  reversible one first, so Enter stashes. */
export type Choice = 'stash' | 'bring' | 'discard';
export const CHOICES: { id: Choice; key: string; label: string; detail: (to: string) => string }[] = [
  { id: 'stash', key: 's', label: 'Stash & switch', detail: () => 'stash them in a kete entry: git stash list shows it' },
  { id: 'bring', key: 'b', label: 'Bring changes along', detail: (to) => `keep them on ${to}: refused if they collide with it` },
  { id: 'discard', key: 'd', label: 'Discard & switch', detail: () => 'throw away tracked changes; untracked and new files stay' },
];

/** A press on the changes question. Discard needs a second press: the
 *  first only arms it, and any other choice disarms it. */
export function press(armed: boolean, choice: Choice): { armed: boolean; run: boolean } {
  if (choice !== 'discard') return { armed: false, run: true };
  return armed ? { armed: false, run: true } : { armed: true, run: false };
}
