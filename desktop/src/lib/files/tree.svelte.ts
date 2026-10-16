// The Files view's state, per worktree, for as long as the window runs
// (never saved): the folders loaded (tree.ts levels), which are open, the
// whole listing the filter searches once it is asked for, and where the
// keys were. src-tauri/src/tree.rs lists; this asks it, and opens and
// closes the view as Diff's opener does (store.svelte.ts `openDiff`).

import { invoke } from '@tauri-apps/api/core';
import { SvelteMap, SvelteSet } from 'svelte/reactivity';
import { app, goHome, projectOf, toast } from '../store.svelte';
import { nav } from '../shell/history.svelte';
import { foldersUnder, levelsOf, type Entry, type Level, type Listing } from './tree';

export const treeApi = {
  /** One folder's rows from `offset`, or with `deep` everything under it. */
  list: (worktree: string, dir: string, deep = false, offset = 0) => invoke<Listing>('worktree_tree', { worktree, dir, deep, offset }),
};

export interface FileTree {
  worktree: string;
  levels: SvelteMap<string, Level>;
  open: SvelteSet<string>;
  /** Every row of the checkout, depth first, for the filter; null until
   *  the filter first asks. */
  all: Entry[] | null;
  /** Rows the whole listing left out past its cap. */
  allMore: number;
  /** Calls in flight. */
  busy: number;
  /** Why the root couldn't be listed. */
  error: string | null;
  /** The row the keys are on (its path). */
  cursor: string | null;
  filter: string;
  /** Where the list was scrolled to. */
  scroll: number;
}

const trees = new Map<string, FileTree>();

/** Checkout `worktree`'s tree, made on first ask. */
export function treeOf(worktree: string): FileTree {
  const had = trees.get(worktree);
  if (had) return had;
  const t: FileTree = $state({
    worktree,
    levels: new SvelteMap(),
    open: new SvelteSet(),
    all: null,
    allMore: 0,
    busy: 0,
    error: null,
    cursor: null,
    filter: '',
    scroll: 0,
  });
  trees.set(worktree, t);
  return t;
}

async function busy<T>(t: FileTree, f: () => Promise<T>): Promise<T> {
  t.busy += 1;
  try {
    return await f();
  } finally {
    t.busy -= 1;
  }
}

/** Read folder `dir` (`''` the root) again, or its next page past
 *  `offset`. A failure on the root says so in the view; on a folder, as a
 *  toast, the folder left as it was. */
export async function loadLevel(t: FileTree, dir: string, offset = 0): Promise<void> {
  try {
    const l = await busy(t, () => treeApi.list(t.worktree, dir, false, offset));
    const had = t.levels.get(dir);
    t.levels.set(dir, offset && had ? { entries: [...had.entries.slice(0, offset), ...l.entries], more: l.more } : { entries: l.entries, more: l.more });
    if (!dir) t.error = null;
  } catch (e) {
    if (!dir) t.error = String(e);
    else toast(`Could not list ${dir}: ${e}`);
  }
}

/** Open or fold folder `dir`, reading it the first time it opens. */
export function setOpen(t: FileTree, dir: string, open: boolean) {
  if (!open) return void t.open.delete(dir);
  t.open.add(dir);
  if (!t.levels.has(dir)) void loadLevel(t, dir);
}

/** Expand all: `dir` and every folder under it open, read in one call. */
export async function expandAll(t: FileTree, dir: string): Promise<void> {
  try {
    const l = await busy(t, () => treeApi.list(t.worktree, dir, true));
    for (const [k, v] of levelsOf(l)) t.levels.set(k, v);
    for (const d of foldersUnder(t.levels, dir)) t.open.add(d);
    if (l.more) toast(`Opened the first ${l.entries.length.toLocaleString()} rows of ${dir || 'the checkout'}: ${l.more.toLocaleString()} more stay folded`);
  } catch (e) {
    toast(`Could not list ${dir || 'the checkout'}: ${e}`);
  }
}

/** Collapse all: `dir` and every folder under it folded (`''`: all). */
export function collapseAll(t: FileTree, dir: string) {
  for (const p of [...t.open]) if (!dir || p === dir || p.startsWith(`${dir}/`)) t.open.delete(p);
}

/** The whole listing the filter searches, read once (and again on
 *  refresh); its folders join the levels too. */
export function loadAll(t: FileTree): Promise<void> {
  const going = reading.get(t.worktree);
  if (going) return going;
  const p = (async () => {
    try {
      const l = await busy(t, () => treeApi.list(t.worktree, '', true));
      t.all = l.entries;
      t.allMore = l.more;
      if (!l.more) for (const [k, v] of levelsOf(l)) t.levels.set(k, v);
    } catch (e) {
      // Not asked again until the filter changes or a refresh.
      t.all = t.all ?? [];
      toast(`Could not list the checkout: ${e}`);
    } finally {
      reading.delete(t.worktree);
    }
  })();
  reading.set(t.worktree, p);
  return p;
}
/** The whole listings being read, by worktree. */
const reading = new Map<string, Promise<void>>();

/** Read again what is on screen: the root, each open folder already read,
 *  and the filter's listing once it has been asked for. */
export async function refresh(t: FileTree): Promise<void> {
  const dirs = ['', ...[...t.open].filter((d) => t.levels.has(d))];
  await Promise.all([...dirs.map((d) => loadLevel(t, d)), ...(t.all ? [loadAll(t)] : [])]);
}

// ---- the view ----

/** The Files view on checkout `worktree` (null: it asks which). */
export function openFilesView(worktree: string | null) {
  nav.go({ view: 'files', repo: projectOf(worktree), item: worktree ?? undefined });
}

/** Leave the Files view for wherever it was opened from, or home. */
export function closeFilesView() {
  const repo = nav.current.repo;
  const home = app.home;
  while (nav.canBack && nav.current.view === 'files') nav.back();
  if (nav.current.view === 'files') goHome(repo);
  // Overview | Board picked while the files were up: land on that one.
  else if ((nav.current.view === 'overview' || nav.current.view === 'board') && nav.current.view !== home) nav.replace({ ...nav.current, view: home });
}

/** Another checkout picked in the view: no new place in history. */
export function pickFilesWorktree(worktree: string) {
  nav.replace({ view: 'files', repo: projectOf(worktree), item: worktree });
}
