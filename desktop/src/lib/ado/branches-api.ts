// Typed wrappers over the Branches commands (`ado_branches`, `ado_fetch`,
// `ado_checkout_worktree`, `ado_delete_local`) and `diff_range`, plus the
// pure helpers the Branches view uses.
//
// `BranchRow`, `BranchTree` and `PrRef` in `src-tauri/src/ado/branches.rs`
// have no `rename_all`, so their fields arrive in snake_case, and
// `people` is a list of `[person, rows]` pairs. The types say so.

import { invoke } from '@tauri-apps/api/core';
import { fuzzy } from './tasks-api';

export interface PrRef {
  id: number;
  status: string;
  is_draft: boolean;
}

export interface BranchRow {
  name: string;
  /** The prefix before `/`, or `other`. */
  person: string;
  local: boolean;
  remote: boolean;
  ahead: number;
  behind: number;
  /** The upstream was deleted on the remote. */
  gone: boolean;
  /** Seconds since the last commit. */
  last_commit_age: number;
  last_author: string;
  task: number | null;
  pr: PrRef | null;
  /** Id of the worktree that has this branch checked out. */
  worktree: string | null;
  /** Id of a live agent session in that worktree. */
  session: string | null;
}

export interface BranchTree {
  /** My initials: my group is first. */
  me: string;
  people: [string, BranchRow[]][];
}

export interface RangeFile {
  path: string;
  from: string | null;
  status: string;
  added: number | null;
  removed: number | null;
}

export interface RangeDiff {
  base: string;
  tip: string;
  /** The remote could not be fetched; the refs are as last fetched. */
  offline: boolean;
  files: RangeFile[];
}

export const branchesApi = {
  list: (worktree: string) => invoke<BranchTree>('ado_branches', { worktree }),
  /** Fetch all remotes, with prune. */
  fetch: (worktree: string) => invoke<void>('ado_fetch', { worktree }),
  /** The worktree's id (an existing one when the branch is checked out). */
  checkout: (worktree: string, branch: string) => invoke<string>('ado_checkout_worktree', { worktree, branch }),
  deleteLocal: (worktree: string, branch: string) => invoke<void>('ado_delete_local', { worktree, branch }),
  diffRange: (worktree: string, base: string, tip: string) => invoke<RangeDiff>('diff_range', { worktree, base, tip }),
};

/** The person group for a branch name. */
export function personOf(name: string): string {
  const i = name.indexOf('/');
  return i > 0 ? name.slice(0, i) : 'other';
}

/**
 * The tree narrowed to `query`. `ga/` keeps only person `ga`; `ga/x` is
 * person `ga` and a fuzzy `x` within; anything else is fuzzy across every
 * branch name. Groups left empty are dropped.
 */
export function filterBranches(tree: BranchTree, query: string): BranchTree {
  const q = query.trim();
  if (!q) return tree;
  const slash = q.indexOf('/');
  const person = slash > 0 ? q.slice(0, slash).toLowerCase() : null;
  const rest = slash > 0 ? q.slice(slash + 1) : q;
  const people: [string, BranchRow[]][] = [];
  for (const [who, rows] of tree.people) {
    if (person !== null && who.toLowerCase() !== person) continue;
    const kept = rest
      ? rows.filter((r) => fuzzy(person !== null ? r.name.slice(r.name.indexOf('/') + 1) : r.name, rest))
      : rows;
    if (kept.length) people.push([who, kept]);
  }
  return { me: tree.me, people };
}

/** The sync badge words: up-arrow 2 down-arrow 1, `local only`, `remote only`, `gone`, or `in sync`. */
export function syncWords(r: Pick<BranchRow, 'local' | 'remote' | 'ahead' | 'behind' | 'gone'>): string {
  if (r.gone) return 'gone';
  if (r.local && !r.remote) return 'local only';
  if (!r.local && r.remote) return 'remote only';
  const parts: string[] = [];
  if (r.ahead) parts.push(`\u2191${r.ahead}`);
  if (r.behind) parts.push(`\u2193${r.behind}`);
  return parts.length ? parts.join(' ') : 'in sync';
}

/** Commit age in short words: `40 s`, `12 min`, `3 h`, `5 d`, `7 wk`. */
export function shortAge(seconds: number): string {
  const s = Math.max(0, Math.round(seconds));
  if (s < 60) return `${s} s`;
  const m = Math.floor(s / 60);
  if (m < 60) return `${m} min`;
  const h = Math.floor(m / 60);
  if (h < 48) return `${h} h`;
  const d = Math.floor(h / 24);
  if (d < 14) return `${d} d`;
  return `${Math.floor(d / 7)} wk`;
}
