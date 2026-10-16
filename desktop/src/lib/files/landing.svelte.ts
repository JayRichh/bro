// Diff this file (the Files tree's row, its D key): the Diff view on the
// file's checkout, landed on that file once the view has its list.
// DiffPane.svelte takes the landing.

import { openDiff } from '../store.svelte';

export const diffLanding = $state({ worktree: '', path: '', seq: 0 });

/** Open the diff of `worktree` on `path`. */
export function diffFile(worktree: string, path: string) {
  diffLanding.worktree = worktree;
  diffLanding.path = path;
  diffLanding.seq += 1;
  openDiff(worktree);
}

/** The file the diff of `worktree` should land on, taken once. */
export function takeLanding(worktree: string | null): string | null {
  if (!worktree || !diffLanding.path || diffLanding.worktree !== worktree) return null;
  const p = diffLanding.path;
  diffLanding.path = '';
  return p;
}
