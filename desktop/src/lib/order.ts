// The user's order for repos, worktrees and sessions, and what a drag or
// Alt+Up / Alt+Down does with it. Pure, so the rules are tested without a
// window; reorder.svelte.ts sends what these work out.
//
// The order rule (nebula-core `ReorderScope`): a scope the user has ordered
// lays out by `sort_order`; one never ordered keeps each view's own order.
// A scope has a user order once some row's `sort_order` is above 0: the
// daemon leaves every worktree and session at 0 until a `Reorder` writes
// 0..n. So:
// - repos: slots 0..n from the day they were added, so the rail's order
//   is the overview's too (the attention strip, not the row order, is
//   what says a repo needs you);
// - worktrees: the root first, then the user's order, or else the most
//   recently worked in (the terminal app's bands);
// - sessions: the user's order, or else oldest status change first; the
//   board's lanes keep status age across checkouts and the user's order
//   among one checkout's cards (`refill`).

import type { Agent, Model, Worktree } from './api';
import type { Lane } from './status';

/** Whether a scope's rows carry a user order. */
export function hasUserOrder(rows: { sort_order: number }[]): boolean {
  return rows.some((r) => r.sort_order > 0);
}

export type Place = 'before' | 'after';

/** `ids` with `id` moved to sit just before or after `target`; null when
 *  that changes nothing (or either is missing). */
export function placeId(ids: string[], id: string, target: string, place: Place): string[] | null {
  if (id === target || !ids.includes(id) || !ids.includes(target)) return null;
  const rest = ids.filter((x) => x !== id);
  const at = rest.indexOf(target) + (place === 'after' ? 1 : 0);
  const out = [...rest.slice(0, at), id, ...rest.slice(at)];
  return out.every((x, i) => x === ids[i]) ? null : out;
}

/** One step up (-1) or down (1) past the next of `peers` (the rows shown
 *  beside it, in display order), as a new order of the whole scope `ids`.
 *  Null at either end. */
export function stepId(ids: string[], peers: string[], id: string, dir: -1 | 1): string[] | null {
  const i = peers.indexOf(id);
  const next = i < 0 ? undefined : peers[i + dir];
  if (next === undefined) return null;
  return placeId(ids, id, next, dir < 0 ? 'before' : 'after');
}

// ---- each scope as it is shown ----

/** Repos in the order the rail lists them. */
export function projectIds(m: Model): string[] {
  return [...m.projects].sort((a, b) => a.sort_order - b.sort_order).map((p) => p.id);
}

/** When a session in the checkout last changed state (ms); 0: never. */
export function activityOf(m: Model, worktree: string): number {
  return m.agents.reduce((t, a) => (a.worktree_id === worktree ? Math.max(t, a.status_changed_at) : t), 0);
}

/** The worktree comparator for a project: the root first, then the
 *  user's order when `ordered`, else the most recently worked in. */
export function compareWorktrees(ordered: boolean, activity: (w: Worktree) => number) {
  return (a: Worktree, b: Worktree) =>
    Number(b.is_main) - Number(a.is_main) ||
    (ordered
      ? a.sort_order - b.sort_order || activity(b) - activity(a)
      : activity(b) - activity(a) || a.sort_order - b.sort_order);
}

/** A project's checkouts as the overview shows them, empty ones too. */
export function worktreeIds(m: Model, project: string): string[] {
  const all = m.worktrees.filter((w) => w.project_id === project);
  return [...all].sort(compareWorktrees(hasUserOrder(all), (w) => activityOf(m, w.id))).map((w) => w.id);
}

/** A checkout's sessions in their order, archived ones too (the tree
 *  nests them; this is the flat order the daemon keeps). */
export function agentIds(m: Model, worktree: string): string[] {
  return orderedAgents(m.agents.filter((a) => a.worktree_id === worktree)).map((a) => a.id);
}

/** Sessions by `sort_order`, then oldest status change: the tree's order. */
export function orderedAgents(agents: Agent[]): Agent[] {
  return [...agents].sort((x, y) => x.sort_order - y.sort_order || x.status_changed_at - y.status_changed_at);
}

/** A worktree scope's new order keeps the root first: the daemon refuses
 *  one that puts it after a linked worktree. */
export function rootFirst(m: Model, ids: string[]): string[] {
  const roots = new Set(m.worktrees.filter((w) => w.is_main).map((w) => w.id));
  return [...ids.filter((x) => roots.has(x)), ...ids.filter((x) => !roots.has(x))];
}

/** The session a tree row hangs under: its parent when that session is
 *  listed in the same checkout, else none (a root). `shown`: the ids the
 *  overview lists there (archived ones only while it shows them). */
export function treeParent(a: Agent, shown: Map<string, Agent>): string | null {
  const p = a.parent && a.parent !== a.id ? shown.get(a.parent) : undefined;
  return p && p.worktree_id === a.worktree_id ? p.id : null;
}

/** The sessions shown beside `id` in the overview: its checkout, under the
 *  same parent, in order. Empty for a session the overview does not list. */
export function treeSiblings(m: Model, id: string, archived: boolean): string[] {
  const a = m.agents.find((x) => x.id === id);
  if (!a) return [];
  const here = m.agents.filter((x) => x.worktree_id === a.worktree_id && (archived || !x.archived));
  const shown = new Map(here.map((x) => [x.id, x]));
  if (!shown.has(id)) return [];
  const parent = treeParent(a, shown);
  return orderedAgents(here.filter((x) => treeParent(x, shown) === parent)).map((x) => x.id);
}

// ---- drops ----

export type Zone = Place | 'onto';

/**
 * Where on a row a drag would land, from the pointer's `offset` from the
 * row's top: with only an insert, the upper or lower half; with only a
 * drop onto it, all of it; with both, the outer quarters insert and the
 * middle drops onto it. Null when the row takes neither.
 */
export function dropZone(offset: number, height: number, takes: { insert: boolean; onto: boolean }): Zone | null {
  const f = height > 0 ? offset / height : 0.5;
  if (!takes.insert) return takes.onto ? 'onto' : null;
  if (!takes.onto) return f < 0.5 ? 'before' : 'after';
  return f < 0.25 ? 'before' : f > 0.75 ? 'after' : 'onto';
}

/** What dropping a board card on a lane (or the Archived zone) does. */
export type LaneDrop =
  | { act: 'reorder' }
  | { act: 'seen' }
  | { act: 'archive' }
  | { act: 'refuse'; reason: string };

const OWNED: Partial<Record<Lane, string>> = {
  needs: 'Needs you is the agent\u2019s: the card leaves it when you answer the session.',
  running: 'Running is the agent\u2019s: the card lands in Done when the turn ends.',
  pr: 'Its pull request is open: the card leaves PR open when the PR merges or closes.',
};

/** Only moves the user can cause: Unread to Done marks the session seen,
 *  the Archived zone archives it, the same lane reorders. A status the
 *  agent (or the pull request) owns is refused with the reason. */
export function laneDrop(from: Lane, to: Lane | 'archived'): LaneDrop {
  if (to === 'archived') return { act: 'archive' };
  if (from === to) return { act: 'reorder' };
  if (to === 'done') {
    if (from === 'unseen') return { act: 'seen' };
    return { act: 'refuse', reason: OWNED[from] ?? 'That card cannot be marked done.' };
  }
  const why: Record<Exclude<Lane, 'done'>, string> = {
    needs: 'Only the agent puts a card in Needs you, by asking.',
    running: 'Only the agent puts a card in Running, by starting a turn: send it a follow-up (F).',
    unseen: 'Unread comes from a turn finishing while you were away; a card cannot be marked unread.',
    pr: 'PR open follows the worktree\u2019s pull request: open one and the card moves.',
  };
  return { act: 'refuse', reason: why[to] };
}

/**
 * A lane's cards: newest status change first across checkouts, and the
 * user's order among the cards of a checkout that has one. Each such
 * checkout's cards take back the slots they hold, in `sort_order`, so the
 * checkouts stay interleaved by age.
 */
export function refill(cards: Agent[], ordered: (worktree: string) => boolean): Agent[] {
  const out = [...cards].sort((x, y) => y.status_changed_at - x.status_changed_at);
  const slots = new Map<string, number[]>();
  out.forEach((a, i) => {
    if (ordered(a.worktree_id)) slots.set(a.worktree_id, [...(slots.get(a.worktree_id) ?? []), i]);
  });
  for (const at of slots.values()) {
    const mine = at.map((i) => out[i]).sort((x, y) => x.sort_order - y.sort_order);
    at.forEach((i, k) => (out[i] = mine[k]));
  }
  return out;
}

/** A checkout's sessions as the board shows them, for a drop or a step
 *  there: the user's order, or newest first while it has none (the order
 *  its cards stand in down a lane), so the first move keeps the rest put. */
export function boardIds(m: Model, worktree: string): string[] {
  const here = m.agents.filter((a) => a.worktree_id === worktree);
  if (hasUserOrder(here)) return orderedAgents(here).map((a) => a.id);
  return [...here].sort((x, y) => y.status_changed_at - x.status_changed_at).map((a) => a.id);
}

/** The checkouts whose sessions carry a user order. */
export function orderedWorktrees(m: Model): Set<string> {
  return new Set(m.agents.filter((a) => a.sort_order > 0).map((a) => a.worktree_id));
}
