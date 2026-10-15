// Pick a thing, see its verbs: the registry's words, menus and leading
// verbs (actions.gen.ts, from crates/nebula-config's actions.rs), read the
// way the terminal app reads them, so a verb is called the same in both
// apps and the verb bar leads with what the TUI's footer does. Pure: what
// a verb does on a row lives in actions.ts; this file only says which
// verbs, in which order, by which words and keys.

import { ACTIONS, DESK_KEYS, MENU_LABELS, MENUS, PRIMARY, PRIMARY_SHOWN, type ActionId, type AppliesTo, type DeskKeyId } from './actions.gen';
import { keysOf, type Shortcut } from './shortcuts';
import { SHORTCUTS } from './keytable.svelte';

export { PRIMARY_SHOWN };

/** The registry's kind for a window target's. */
export type TargetKind = 'agent' | 'terminal' | 'worktree' | 'project';
export const KIND_OF: Record<TargetKind, AppliesTo> = {
  agent: 'session',
  terminal: 'terminal',
  worktree: 'worktree',
  project: 'repo',
};

/** What a menu row for verb `id` on a thing of kind `on` says: the
 *  kind's own words first, then the verb's, then its label. */
export function menuLabel(id: ActionId, on: AppliesTo): string {
  return (
    MENU_LABELS.find((l) => l.id === id && l.on === on)?.label ??
    MENU_LABELS.find((l) => l.id === id && l.on === null)?.label ??
    ACTIONS.find((a) => a.id === id)?.label ??
    id
  );
}

/** The verb's one word, for the verb bar. */
export function wordOf(id: ActionId): string {
  return ACTIONS.find((a) => a.id === id)?.word ?? id;
}

/** Whether the verb destroys something (its row goes red, it asks). */
export function destructive(id: ActionId): boolean {
  return !!ACTIONS.find((a) => a.id === id)?.destructive;
}

/** A kind's menu, in the registry's order. */
export function menuIds(on: AppliesTo): readonly ActionId[] {
  return MENUS[on] ?? [];
}

/** The window's key rows for a verb, the row a single key reaches first
 *  (`d` before Ctrl+Shift+D), so a menu shows the shortest. */
export function deskKeysOf(id: ActionId): DeskKeyId[] {
  return [...(ACTIONS.find((a) => a.id === id)?.desktop ?? [])] as DeskKeyId[];
}

/** A verb's chords as the window has them bound now, shortest first.
 *  The review row's Alt chords count in a review only: a menu row, which
 *  works anywhere, does not name them. */
export function verbKeys(id: ActionId, table: Shortcut[] = SHORTCUTS): string[] {
  const reviewOnly = (k: DeskKeyId) => DESK_KEYS.find((d) => d.id === k)?.scope === 'review';
  const all = deskKeysOf(id)
    .filter((k) => !reviewOnly(k))
    .flatMap((k) => keysOf(k, table));
  return [...new Set(all)].sort((a, b) => a.length - b.length);
}

/** The verbs the bar (and the TUI's footer) leads with for a kind: the
 *  first `n` of its PRIMARY list that `can` run on the thing now. */
export function topVerbs(on: AppliesTo, can: (id: ActionId) => boolean, n = PRIMARY_SHOWN): ActionId[] {
  return (PRIMARY[on] ?? []).filter(can).slice(0, n);
}

/** What the verb bar draws for each verb: its word, its key. */
export interface BarVerb {
  id: ActionId;
  word: string;
  /** The first chord, as `keyLabel` prints it later; empty when unbound. */
  key: string;
  danger: boolean;
}

/** The bar's buttons for `on`, from `can` (what the window can run on the
 *  focused thing now), keys from the live table. */
export function barVerbs(on: AppliesTo, can: (id: ActionId) => boolean, table: Shortcut[] = SHORTCUTS, n = PRIMARY_SHOWN): BarVerb[] {
  return topVerbs(on, can, n).map((id) => ({ id, word: wordOf(id), key: verbKeys(id, table)[0] ?? '', danger: destructive(id) }));
}

/** Rows a searchable menu keeps for `query`: every row whose label holds
 *  the query's letters in order, best first; the pinned last row (All
 *  keys) always. */
export function filterRows<T extends { label: string; pinned?: boolean }>(rows: T[], query: string, match: (q: string, s: string) => number | null): T[] {
  const q = query.trim();
  const pinned = rows.filter((r) => r.pinned);
  const rest = rows.filter((r) => !r.pinned);
  if (!q) return [...rest, ...pinned];
  const hits = rest
    .map((r, i) => ({ r, i, s: match(q, r.label) }))
    .filter((h): h is { r: T; i: number; s: number } => h.s !== null)
    .sort((a, b) => b.s - a.s || a.i - b.i)
    .map((h) => h.r);
  return [...hits, ...pinned];
}
