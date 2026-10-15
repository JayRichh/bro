// A repo's checkouts as the overview's worktree groups and the Worktrees
// view's rows, as the terminal app's grid bands them
// (`nebula-tui` launcher.rs `bands`): the root checkout first, then the
// rest most recently worked in first; an empty checkout only with Show
// all worktrees on, and never the root's empty band alone. Pure, so the
// rules are tested without a window.

import type { Agent, Model, PrInfo, TerminalTab, Worktree } from '../api';
import { buildForest, type Node } from '../overview/tree';
import type { CheckoutState, WorktreeGit } from './api';
import { compareWorktrees, hasUserOrder } from '../order';

export type WorktreeGroup = {
  worktree: Worktree;
  /** Its sessions, nested as the overview nests them. */
  forest: Node[];
  terminals: TerminalTab[];
  /** Sessions listed (archived ones too while the overview shows them). */
  sessions: number;
  /** Live cards: unarchived sessions and terminals. */
  live: number;
  pr: PrInfo | undefined;
  /** Ports something listens on there. */
  ports: number[];
  /** Its run command's terminal is up. */
  running: boolean;
  /** When a session there last changed state (ms); 0: never. */
  activity: number;
};

export type GroupOpts = {
  /** List archived sessions too (the overview's Archived). */
  archived?: boolean;
  /** Show all worktrees: an empty checkout keeps its group. */
  showAll?: boolean;
};

/** One checkout's group, empty or not. */
export function groupOf(m: Model, w: Worktree, archived = false): WorktreeGroup {
  const all = m.agents.filter((a) => a.worktree_id === w.id);
  const agents = all.filter((a) => archived || !a.archived);
  const ids = new Set(agents.map((a) => a.id));
  // A session started from another checkout (or repo) carries a "from"
  // cue rather than standing as a root from nowhere.
  const outside = new Map<string, Agent>(m.agents.filter((a) => a.worktree_id !== w.id).map((a) => [a.id, a]));
  const terminals = m.terminals.filter((t) => t.worktree_id === w.id).sort((a, b) => a.sort_order - b.sort_order);
  const ports = [...new Set(m.ports.filter((p) => p.worktree === w.id).flatMap((p) => p.live))].sort((a, b) => a - b);
  return {
    worktree: w,
    forest: buildForest(
      agents,
      m.subagents.filter((s) => ids.has(s.parent_agent)),
      outside,
    ),
    terminals,
    sessions: agents.length,
    live: all.filter((a) => !a.archived).length + terminals.length,
    pr: m.prs[w.id],
    ports,
    running: terminals.some((t) => !!t.run_command && t.alive),
    activity: all.reduce((t, a) => Math.max(t, a.status_changed_at), 0),
  };
}

/** A pull request as its chip reads: `PR #12 merged`. */
export function prLabel(pr: PrInfo): string {
  const state = pr.state === 'merged' || pr.state === 'closed' ? pr.state : pr.draft ? 'draft' : 'open';
  return `PR #${pr.number} ${state}`;
}

/** Whether a group has nothing in it to list. */
export function isEmpty(g: WorktreeGroup): boolean {
  return !g.forest.length && !g.terminals.length;
}

/** The root first, then the user's order when the repo's worktrees have
 *  one (`ordered`, read across all of them, shown or not), else the most
 *  recently worked in; a checkout never worked in keeps its place in the
 *  repo's order, at the bottom. The rule is order.ts's. */
export function orderGroups(groups: WorktreeGroup[], ordered = false): WorktreeGroup[] {
  const by = compareWorktrees(ordered, (w) => groups.find((g) => g.worktree.id === w.id)?.activity ?? 0);
  return [...groups].sort((a, b) => by(a.worktree, b.worktree));
}

/** The overview's groups for a repo. Empty when nothing would show but
 *  the root's empty group: the repo row says "No sessions" instead. */
export function worktreeGroups(m: Model, project: string, opts: GroupOpts = {}): WorktreeGroup[] {
  const all = m.worktrees.filter((w) => w.project_id === project);
  const groups = all.map((w) => groupOf(m, w, opts.archived));
  const shown = groups.filter((g) => !isEmpty(g) || opts.showAll);
  if (shown.every((g) => g.worktree.is_main && isEmpty(g))) return [];
  return orderGroups(shown, hasUserOrder(all));
}

/** Changes when the checkouts' git state is worth reading again: a
 *  checkout came or went, or a session somewhere changed state (a turn
 *  ended, most likely having touched files). */
export function gitSignal(m: Model): string {
  const last = m.agents.reduce((t, a) => Math.max(t, a.status_changed_at), 0);
  return `${m.worktrees.map((w) => w.id).join(',')}|${last}`;
}

// ---- folding ----

export const repoKey = (project: string) => `repo:${project}`;
export const worktreeKey = (worktree: string) => `wt:${worktree}`;

/** `keys` with `key` folded (`on`) or unfolded. */
export function setKey(keys: string[], key: string, on: boolean): string[] {
  const rest = keys.filter((k) => k !== key);
  return on ? [...rest, key] : rest;
}

/** Keys still naming a repo or checkout the model has: a deleted one's
 *  fold is not kept for ever. */
export function pruneKeys(keys: string[], m: Model): string[] {
  const live = new Set([...m.projects.map((p) => repoKey(p.id)), ...m.worktrees.map((w) => worktreeKey(w.id))]);
  return keys.filter((k) => live.has(k));
}

// ---- the Worktrees view ----

export type SortKey = 'branch' | 'changes' | 'ahead' | 'sessions' | 'pr' | 'activity';
export type SortDir = 'asc' | 'desc';

export type TableRow = WorktreeGroup & {
  git: WorktreeGit | undefined;
  /** Uncommitted files, tracked and not; null until git is read. */
  changes: number | null;
  /** The later of a session's state change and HEAD's commit (ms). */
  last: number;
};

export function tableRow(g: WorktreeGroup, git: WorktreeGit | undefined): TableRow {
  return {
    ...g,
    git,
    changes: git ? git.changed + git.untracked : null,
    last: Math.max(g.activity, (git?.committed ?? 0) * 1000),
  };
}

const PR_RANK: Record<string, number> = { open: 0, merged: 1, closed: 2 };

function sortValue(r: TableRow, key: SortKey): number | string {
  switch (key) {
    case 'branch':
      return r.worktree.branch.toLowerCase();
    case 'changes':
      return r.changes ?? -1;
    case 'ahead':
      return r.git?.ahead ?? -1;
    case 'sessions':
      return r.sessions + r.terminals.length;
    case 'pr':
      return r.pr ? PR_RANK[r.pr.state ?? 'open'] : 3;
    case 'activity':
      return r.last;
  }
}

/** Rows by `key`; the root checkout stays on top whatever the sort, as
 *  in the overview. Ties fall back to the branch name. */
export function sortRows(rows: TableRow[], key: SortKey, dir: SortDir): TableRow[] {
  const sign = dir === 'asc' ? 1 : -1;
  return [...rows].sort((a, b) => {
    const root = Number(b.worktree.is_main) - Number(a.worktree.is_main);
    if (root) return root;
    const x = sortValue(a, key);
    const y = sortValue(b, key);
    const c = typeof x === 'string' ? x.localeCompare(y as string) : x - (y as number);
    return sign * c || a.worktree.branch.localeCompare(b.worktree.branch);
  });
}

/** What a column sorts by first: names A to Z, the rest biggest first. */
export function firstDir(key: SortKey): SortDir {
  return key === 'branch' || key === 'pr' ? 'asc' : 'desc';
}

// ---- bulk delete ----

export type Sweep = 'merged' | 'clean';

/** Why a checkout may go in a bulk delete, or null when it stays: never
 *  the root, never one with a live session or terminal, never one with
 *  uncommitted work or (for "clean") commits no remote holds. `state` is
 *  read fresh just before the confirm. */
export function sweepReason(g: WorktreeGroup, state: CheckoutState | null): Sweep | null {
  if (g.worktree.is_main || g.live > 0 || !state) return null;
  if (state.changed + state.untracked > 0) return null;
  if (g.pr?.state === 'merged') return 'merged';
  if (state.unpushed === 0 && !(g.pr && (g.pr.state ?? 'open') === 'open')) return 'clean';
  return null;
}

/** The checkouts a sweep would even ask git about: linked, nothing live. */
export function sweepCandidates(groups: WorktreeGroup[]): WorktreeGroup[] {
  return groups.filter((g) => !g.worktree.is_main && g.live === 0);
}
