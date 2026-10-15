// What the Diff view reads a worktree against (desktop/src-tauri main.rs
// `diff_review`, nebula_core::review_base): the merge-base with the base
// branch, so an agent's commits stay in view, or HEAD, where hunks stage.
// Pure: the header's words, the toggle's tooltip and the worktree picker's
// groups.

import type { Model } from './api';
import type { DiffFile } from './diff-api';

export type Against = 'base' | 'head';

export interface ReviewBase {
  sha: string;
  short: string;
  /** The base branch by name: `main`, `develop`. */
  branch: string;
}

export interface ReviewCommit {
  sha: string;
  short: string;
  subject: string;
}

/** `diff_review`: what was read, the base there is, and the commits since it. */
export interface ReviewDiff {
  against: Against;
  base: ReviewBase | null;
  commits: ReviewCommit[];
  files: DiffFile[];
}

/** `vs main (merge-base abc1234)` or `vs HEAD`, as the terminal app says it. */
export function againstLabel(against: Against, base: ReviewBase | null): string {
  return against === 'base' && base ? `vs ${base.branch} (merge-base ${base.short})` : 'vs HEAD';
}

/** The toggle's tooltip: what pressing it does, and why it cannot. */
export function againstTitle(against: Against, base: ReviewBase | null): string {
  if (!base) return 'Compared with HEAD: this checkout has no base branch to compare with (it is on it, detached, or has none)';
  return against === 'base'
    ? `Compare with HEAD instead: only what is not committed yet, with per-hunk Stage and Discard`
    : `Compare with ${base.branch} instead: everything this branch changed since it forked at ${base.short}, committed or not`;
}

/** Staging reads the index against HEAD: in a base diff it has nothing to act on. */
export function canStage(range: boolean, against: Against): boolean {
  return !range && against === 'head';
}

export interface WorktreeGroup {
  project: string;
  name: string;
  worktrees: { id: string; branch: string; isMain: boolean }[];
}

/** The worktree picker's options, by repo in the rail's order, the main
 *  checkout first in each, then the rest in the model's order. */
export function groupWorktrees(m: Model): WorktreeGroup[] {
  const projects = [...m.projects].sort((a, b) => a.sort_order - b.sort_order);
  return projects
    .map((p) => {
      const own = m.worktrees.filter((w) => w.project_id === p.id);
      const ordered = [...own.filter((w) => w.is_main), ...own.filter((w) => !w.is_main)];
      return {
        project: p.id,
        name: p.name,
        worktrees: ordered.map((w) => ({ id: w.id, branch: w.branch, isMain: w.is_main })),
      };
    })
    .filter((g) => g.worktrees.length);
}
