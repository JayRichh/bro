// Typed wrappers over the diff commands (desktop/src-tauri/src/main.rs,
// git.rs), with their wire shapes exactly as serde writes them, and the
// Diff view's place in history.

import { invoke } from '@tauri-apps/api/core';
import type { DiffFile } from './api';
import type { Against, ReviewDiff } from './diff-base';

export type { DiffFile };

export type DiffMode = 'side' | 'inline' | 'replacement';
export const DIFF_MODES: DiffMode[] = ['side', 'inline', 'replacement'];
export const isDiffMode = (m: unknown): m is DiffMode => DIFF_MODES.includes(m as DiffMode);

/** Where a blob is read from. Serde: tag `kind`, content `rev`. */
export type DiffSide = { kind: 'worktree' } | { kind: 'index' } | { kind: 'commit'; rev: string };

/** One side of a diff, honestly. Serde: tag `state`, content `value`. */
export type Blob =
  | { state: 'text'; value: string }
  | { state: 'missing' }
  /** Bytes on that side, over the 2 MB cap. */
  | { state: 'tooLarge'; value: number }
  | { state: 'binary' };

/** Two ends of a range diff, by ref or SHA. */
export interface DiffRange {
  base: string;
  tip: string;
  /** The pull request the range is from: its merge ref is fetched first, so
   *  its pinned SHAs are there to read. */
  pr?: number;
}

/** `diff_range`: both ends pinned to SHAs; `base` is the merge-base. */
export interface RangeDiff {
  base: string;
  tip: string;
  /** A fetch failed and cached refs were used. */
  offline: boolean;
  files: DiffFile[];
}

/** One `@@` hunk, self-contained: hand it back to `applyHunk` unchanged. */
export interface Hunk {
  header: string;
  patch: string;
  oldStart: number;
  newStart: number;
}

export type HunkAction = 'stage' | 'unstage' | 'discard';

export const diffApi = {
  /** Every changed path in a worktree, against HEAD. */
  files: (worktree: string) => invoke<DiffFile[]>('diff_files', { worktree }),
  /** Against the merge-base with the base branch, or HEAD; null: the base
   *  when there is one. Also the base and the commits since it. */
  review: (worktree: string, against: Against | null) =>
    invoke<ReviewDiff>('diff_review', { worktree, against }),
  range: (worktree: string, base: string, tip: string, pr?: number) =>
    invoke<RangeDiff>('diff_range', { worktree, base, tip, pr: pr ?? null }),
  blob: (worktree: string, side: DiffSide, path: string) =>
    invoke<Blob>('diff_blob', { worktree, side, path }),
  /** Staged: index against HEAD. Not staged: worktree against index. */
  hunks: (worktree: string, path: string, staged: boolean) =>
    invoke<Hunk[]>('diff_hunks', { worktree, path, staged }),
  applyHunk: (worktree: string, hunk: Hunk, action: HunkAction) =>
    invoke<void>('diff_apply_hunk', { worktree, hunk, action }),
};

// The Diff view's place in history: `item` is the worktree id, or for a
// range `worktree \u0000 base \u0000 tip`. Use `openDiffRange` (store) to
// go there, and `parseDiffItem` to read a place back.

const SEP = '\u0000';

export function diffItem(worktree: string, range?: DiffRange | null): string {
  return range ? [worktree, range.base, range.tip].join(SEP) : worktree;
}

export function parseDiffItem(item: string | undefined): { worktree: string | null; range: DiffRange | null } {
  if (!item) return { worktree: null, range: null };
  const [worktree, base, tip] = item.split(SEP);
  return { worktree: worktree || null, range: base && tip ? { base, tip } : null };
}
