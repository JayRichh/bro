// Typed wrappers over the Tasks commands (`ado_teams`, `ado_iterations`,
// `ado_tasks`, `ado_set_team`, `ado_task_prompt`, `ado_start_task`), plus
// the pure helpers the Tasks view and the Start dialog share. Field names
// are what serde writes for the Rust structs in `src-tauri/src/ado/tasks.rs`.

import { invoke } from '@tauri-apps/api/core';
import { promptingInvoke } from './signin.svelte';

/**
 * Which tasks to list. Serde's external tagging: the unit variants are bare
 * strings, a chosen iteration is `{ iteration: id }`.
 */
export type TaskScope = 'currentSprint' | 'myOpen' | 'sprintAll' | { iteration: string };

export interface Iteration {
  id: string;
  name: string;
  path: string;
  start: string | null;
  finish: string | null;
  timeframe: string | null;
}

export interface WorkItem {
  id: number;
  kind: string;
  title: string;
  state: string;
  /** `Proposed`, `InProgress`, `Resolved`, `Completed`, `Removed`, or empty. */
  stateCategory: string;
  assignedTo: string | null;
  remaining: number | null;
  parent: number | null;
  boardColumn: string | null;
  descriptionMd: string;
  acceptanceMd: string;
  url: string;
}

export interface TaskGroup {
  parent: WorkItem | null;
  items: WorkItem[];
}

export interface TaskList {
  iteration: Iteration | null;
  stale: boolean;
  next: Iteration | null;
  groups: TaskGroup[];
  /** Set when ADO could not be reached or signed in to: when this saved list was saved, Unix ms. */
  cachedAt: number | null;
  /** Why the saved list is shown: signed out (`auth`) or unreachable (`offline`). */
  cachedBecause?: 'auth' | 'offline' | null;
  /** The failure behind the saved list, in words. */
  cachedReason?: string | null;
}

/** The agent Start spawns; `worktree` stays empty, the backend fills it. */
export interface SpawnSpec {
  worktree: string;
  kind: string;
  profile: string | null;
  prompt: string | null;
}

export interface StartTask {
  id: number;
  branch: string;
  /** None cuts from the daemon's default base. */
  base: string | null;
  assignToMe: boolean;
  activate: boolean;
  spawn: SpawnSpec | null;
}

export interface StepProblem {
  /** `link`, `read`, `assign`, `activate` or `spawn`. */
  step: string;
  message: string;
}

export interface StartedTask {
  worktreeId: string;
  agentId: string | null;
  problems: StepProblem[];
}

export const tasksApi = {
  teams: (worktree: string) => promptingInvoke<string[]>('ado_teams', { worktree }),
  iterations: (worktree: string) => promptingInvoke<Iteration[]>('ado_iterations', { worktree }),
  tasks: (worktree: string, scope: TaskScope) => promptingInvoke<TaskList>('ado_tasks', { worktree, scope }),
  /** Empty goes back to the project's default team. */
  setTeam: (worktree: string, team: string) => invoke<void>('ado_set_team', { worktree, team }),
  prompt: (worktree: string, id: number) => promptingInvoke<string>('ado_task_prompt', { worktree, id }),
  /** Not counted as signing in: making the worktree takes time of its own. */
  start: (worktree: string, req: StartTask) => invoke<StartedTask>('ado_start_task', { worktree, req }),
};

// ---- pure helpers ----

/** Where keys are text: a view shortcut never fires there. */
export const TYPING_SEL = 'input, textarea, select, [contenteditable], .xterm, .monaco-editor';
/** Controls that act on Enter and Space themselves. */
export const CONTROL_SEL = 'button, a, summary, [role="button"], [role="radio"], [role="checkbox"], [role="tab"]';

/**
 * Whether `key` belongs to the focused element rather than to a view's
 * window-level shortcuts: any key while typing, and Enter or Space on a
 * focused control (so Enter presses the button, never starts a task).
 */
export function ownsKey(target: { closest(sel: string): unknown } | null | undefined, key: string): boolean {
  if (!target || typeof target.closest !== 'function') return false;
  if (target.closest(TYPING_SEL)) return true;
  return (key === 'Enter' || key === ' ') && !!target.closest(CONTROL_SEL);
}

/** The scope as the scope bar names it. */
export function scopeLabel(scope: TaskScope, iterations: Iteration[] = []): string {
  if (typeof scope === 'object') {
    const it = iterations.find((i) => i.id === scope.iteration);
    return it ? it.name : 'Chosen iteration';
  }
  switch (scope) {
    case 'currentSprint':
      return 'Current sprint';
    case 'myOpen':
      return 'My open';
    case 'sprintAll':
      return 'Sprint, everyone';
  }
}

/** A stable key for a scope (selects, saved view state). */
export function scopeKey(scope: TaskScope): string {
  return typeof scope === 'object' ? `iteration:${scope.iteration}` : scope;
}

export function scopeFromKey(key: unknown): TaskScope {
  if (key === 'myOpen' || key === 'sprintAll' || key === 'currentSprint') return key;
  if (typeof key === 'string' && key.startsWith('iteration:') && key.length > 10) return { iteration: key.slice(10) };
  return 'currentSprint';
}

const DAY = 86_400_000;

/** Days from `YYYY-MM-DD` (the start of `date`) to `now`, whole days, UTC. */
function daysSince(date: string, now: number): number {
  const d = Date.parse(`${date.slice(0, 10)}T00:00:00Z`);
  const today = Date.parse(`${new Date(now).toISOString().slice(0, 10)}T00:00:00Z`);
  return Math.round((today - d) / DAY);
}

/**
 * The line over a stale sprint: "Sprint 42 ended 3 days ago". Null when the
 * list is not stale. Days count from the finish date, in UTC.
 */
export function staleNotice(list: Pick<TaskList, 'stale' | 'iteration'>, now: number = Date.now()): string | null {
  if (!list.stale || !list.iteration) return null;
  const name = list.iteration.name;
  const days = list.iteration.finish ? daysSince(list.iteration.finish, now) : 0;
  if (days <= 0) return `${name} has ended`;
  return `${name} ended ${days} ${days === 1 ? 'day' : 'days'} ago`;
}

/** "3 min ago", "2 h ago", "4 days ago": how old a saved list is. */
export function ageWords(at: number, now: number = Date.now()): string {
  const s = Math.max(0, Math.round((now - at) / 1000));
  if (s < 60) return 'just now';
  const m = Math.round(s / 60);
  if (m < 60) return `${m} min ago`;
  const h = Math.round(m / 60);
  if (h < 24) return `${h} h ago`;
  const d = Math.round(h / 24);
  return d === 1 ? 'yesterday' : `${d} days ago`;
}

/**
 * The line over a saved list: signed out and offline read differently, so
 * a sign-in problem is never passed off as the network. Null for a live list.
 */
export function cachedNotice(
  list: Pick<TaskList, 'cachedAt' | 'cachedBecause'>,
  now: number = Date.now(),
): string | null {
  if (!list.cachedAt) return null;
  const age = ageWords(list.cachedAt, now);
  if (list.cachedBecause === 'auth') return `Signed out. Showing the list from ${age}.`;
  return `Offline, as of ${age}. This is the last list saved for this scope.`;
}

/** Remaining work as the row shows it: `4 h`, `0.5 h`, or nothing. */
export function remainingLabel(r: number | null): string {
  if (r === null || r === undefined) return '';
  const n = Math.round(r * 10) / 10;
  return `${n} h`;
}

/** Subsequence match of `q` in `s`, case-insensitive; empty `q` matches. */
export function fuzzy(s: string, q: string): boolean {
  const hay = s.toLowerCase();
  const needle = q.toLowerCase().replace(/\s+/g, '');
  let i = 0;
  for (const c of hay) {
    if (i < needle.length && c === needle[i]) i++;
  }
  return i === needle.length;
}

/** Groups whose items match `q` across id and title; a matching parent keeps all its items. */
export function filterTasks(groups: TaskGroup[], q: string): TaskGroup[] {
  const query = q.trim();
  if (!query) return groups;
  const words = query.split(/\s+/);
  const hit = (w: WorkItem) => words.every((word) => String(w.id).startsWith(word.replace(/^#/, '')) || fuzzy(w.title, word));
  const out: TaskGroup[] = [];
  for (const g of groups) {
    if (g.parent && hit(g.parent)) {
      out.push(g);
      continue;
    }
    const items = g.items.filter(hit);
    if (items.length) out.push({ parent: g.parent, items });
  }
  return out;
}

// ---- branch names ----

/** The longest slug, as `ado::naming::SLUG_MAX`. */
export const SLUG_MAX = 40;

const FOLD: Record<string, string> = {};
for (const [to, from] of Object.entries({
  a: '\u00e0\u00e1\u00e2\u00e3\u00e4\u00e5\u0101\u0105\u0103',
  ae: '\u00e6',
  c: '\u00e7\u0107\u010d',
  d: '\u010f\u0111\u00f0',
  e: '\u00e8\u00e9\u00ea\u00eb\u0113\u0119\u011b',
  i: '\u00ec\u00ed\u00ee\u00ef\u012b',
  l: '\u0142',
  n: '\u00f1\u0144\u0148',
  o: '\u00f2\u00f3\u00f4\u00f5\u00f6\u00f8\u014d\u0151',
  oe: '\u0153',
  r: '\u0159',
  s: '\u015b\u0161\u015f',
  ss: '\u00df',
  t: '\u0165\u0163',
  th: '\u00fe',
  u: '\u00f9\u00fa\u00fb\u00fc\u016b\u016f\u0171',
  y: '\u00fd\u00ff',
  z: '\u017a\u017c\u017e',
}))
  for (const c of from) FOLD[c] = to;

/**
 * Kebab-case ASCII slug of a title, at most 40 characters, cut at a `-`.
 * The same rules as `ado::naming::slug`.
 */
export function slugify(title: string): string {
  let out = '';
  let dash = false;
  for (const c of title.toLowerCase()) {
    const piece = /[a-z0-9]/.test(c) ? c : FOLD[c];
    if (piece) {
      if (dash && out) out += '-';
      dash = false;
      out += piece;
    } else dash = true;
  }
  if (out.length > SLUG_MAX) {
    let cut = out.slice(0, SLUG_MAX);
    if (out[SLUG_MAX] !== '-') {
      const i = cut.lastIndexOf('-');
      if (i > 0) cut = cut.slice(0, i);
    }
    out = cut.replace(/-+$/, '');
  }
  return out;
}

/** `{initials}/{slug}` for a task, or the slug alone with no initials. */
export function suggestBranch(initials: string, title: string): string {
  const slug = slugify(title);
  return initials ? `${initials}/${slug}` : slug;
}

/**
 * Why `name` cannot be a new branch, or null. The `git check-ref-format
 * --branch` rules plus a clash with `taken`; the backend checks again.
 */
export function branchProblem(name: string, taken: string[] = []): string | null {
  if (!name.trim()) return 'Name the branch.';
  if (name !== name.trim()) return 'No spaces at either end.';
  if (taken.includes(name)) return `A branch named ${name} already exists.`;
  if (name.startsWith('-')) return 'A branch cannot start with "-".';
  if (name === '@') return 'A branch cannot be just "@".';
  if (/[\s~^:?*[\\]/.test(name) || /[\x00-\x1f\x7f]/.test(name))
    return 'No spaces or any of ~ ^ : ? * [ \\ in a branch name.';
  if (name.includes('..')) return 'No ".." in a branch name.';
  if (name.includes('@{')) return 'No "@{" in a branch name.';
  if (name.includes('//') || name.startsWith('/') || name.endsWith('/')) return 'No empty part between slashes.';
  if (name.endsWith('.')) return 'A branch cannot end with ".".';
  for (const part of name.split('/')) {
    if (part.startsWith('.')) return 'No part may start with ".".';
    if (part.endsWith('.lock')) return 'No part may end with ".lock".';
  }
  return null;
}
