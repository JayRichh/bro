// The Files view's tree, pure: the folders loaded so far (one level each,
// as src-tauri/src/tree.rs lists them), which are open, and the rows that
// makes. With a filter, the rows are the terminal app's tree browser's
// (crates/nebula-tui/src/tree_browser.rs `visible_rows`): every file whose
// path from the checkout's root fuzzy-matches, with the folders above it,
// all open; the folds come back as they were when the filter clears.
// tree.svelte.ts keeps the state per worktree and asks src-tauri.

import { fuzzyMatch } from '../fuzzy';

export type Kind = 'dir' | 'file' | 'link' | 'module';

/** git's marks on a file, or on everything under a folder. */
export interface Marks {
  staged?: boolean;
  changed?: boolean;
  untracked?: boolean;
  added?: boolean;
  deleted?: boolean;
  conflicted?: boolean;
}

/** One row as src-tauri lists it. */
export interface Entry extends Marks {
  /** From the checkout's root, `/`-separated. */
  path: string;
  name: string;
  kind: Kind;
  /** A file's size on disk, where it was read. */
  size?: number;
}

/** `worktree_tree`'s answer. */
export interface Listing {
  dir: string;
  entries: Entry[];
  more: number;
}

/** One folder's rows as loaded: `more` past them, null when a cut-short
 *  whole-subtree listing left that unknown. */
export interface Level {
  entries: Entry[];
  more: number | null;
}

export type Row =
  | { kind: 'entry'; entry: Entry; depth: number; open: boolean; positions: number[] }
  /** A folder opened whose rows haven't come yet. */
  | { kind: 'loading'; dir: string; depth: number }
  /** Rows past a folder's cap: `count` of them, null when not known. */
  | { kind: 'more'; dir: string; depth: number; count: number | null };

/** The folder `path` is in (`''` at the root). */
export function parentOf(path: string): string {
  const i = path.lastIndexOf('/');
  return i < 0 ? '' : path.slice(0, i);
}

const depthOf = (path: string) => path.split('/').length - 1;

/** Opens like a folder: it has rows of its own. */
export const isFolder = (e: Pick<Entry, 'kind'>) => e.kind === 'dir';

/** The rows of the tree as it is folded: each open folder's rows under
 *  it, a loading row where they haven't come, a more row past its cap. */
export function foldedRows(levels: ReadonlyMap<string, Level>, open: ReadonlySet<string>, dir = '', depth = 0): Row[] {
  const out: Row[] = [];
  const level = levels.get(dir);
  if (!level) return dir ? [{ kind: 'loading', dir, depth }] : out;
  for (const e of level.entries) {
    const folder = isFolder(e);
    const isOpen = folder && open.has(e.path);
    out.push({ kind: 'entry', entry: e, depth, open: isOpen, positions: [] });
    if (isOpen) out.push(...foldedRows(levels, open, e.path, depth + 1));
  }
  if (level.more !== 0) out.push({ kind: 'more', dir, depth, count: level.more });
  return out;
}

/** The filtered rows over `all` (every row of the checkout, depth first,
 *  as a whole-subtree listing gives them): each file whose path matches,
 *  with its folders, all open. `positions` light the matched letters of
 *  the name; a hit in the folders above lights nothing on the row. `best`
 *  is the row of the best match. */
export function filteredRows(all: readonly Entry[], query: string): { rows: Row[]; best: number | null; matches: number } {
  const keep = new Set<string>();
  const hits = new Map<string, { score: number; positions: number[] }>();
  for (const e of all) {
    if (isFolder(e)) continue;
    const m = fuzzyMatch(query, e.path);
    if (!m) continue;
    const offset = e.path.length - e.name.length;
    hits.set(e.path, { score: m.score, positions: m.positions.filter((p) => p >= offset).map((p) => p - offset) });
    for (let p = e.path; p; p = parentOf(p)) {
      if (keep.has(p)) break;
      keep.add(p);
    }
  }
  const rows: Row[] = [];
  let best: { score: number; row: number } | null = null;
  for (const e of all) {
    if (!keep.has(e.path)) continue;
    const hit = hits.get(e.path);
    if (hit && (!best || hit.score > best.score)) best = { score: hit.score, row: rows.length };
    rows.push({ kind: 'entry', entry: e, depth: depthOf(e.path), open: isFolder(e), positions: hit?.positions ?? [] });
  }
  return { rows, best: best?.row ?? null, matches: hits.size };
}

/** A whole-subtree listing of `l.dir` as levels: each folder with rows in
 *  it, complete, except, when the listing was cut short, the folders on
 *  the way to its last row (their `more` not known). A folder none of
 *  whose rows came stays unloaded. */
export function levelsOf(l: Listing): Map<string, Level> {
  const out = new Map<string, Level>([[l.dir, { entries: [], more: 0 }]]);
  for (const e of l.entries) {
    const dir = parentOf(e.path);
    let level = out.get(dir);
    if (!level) out.set(dir, (level = { entries: [], more: 0 }));
    level.entries.push(e);
  }
  const last = l.entries[l.entries.length - 1];
  if (l.more > 0 && last) {
    for (let d = parentOf(last.path); ; d = parentOf(d)) {
      const level = out.get(d);
      if (level) level.more = null;
      if (d === l.dir || !d) break;
    }
    // The last row a folder: its own rows were cut, so it stays unloaded.
    if (isFolder(last)) out.delete(last.path);
  }
  return out;
}

/** The folders under `dir` (and `dir` itself) among `levels`' rows. */
export function foldersUnder(levels: ReadonlyMap<string, Level>, dir: string): string[] {
  const out: string[] = dir ? [dir] : [];
  const walk = (d: string) => {
    for (const e of levels.get(d)?.entries ?? []) {
      if (!isFolder(e)) continue;
      out.push(e.path);
      walk(e.path);
    }
  };
  walk(dir);
  return out;
}

/** Folded right now under `dir` (or `dir` itself): something Expand all
 *  would open, unknown counting as yes. */
export function canExpand(levels: ReadonlyMap<string, Level>, open: ReadonlySet<string>, dir: string): boolean {
  if (!open.has(dir) || !levels.has(dir)) return true;
  const level = levels.get(dir)!;
  if (level.more !== 0) return true;
  return level.entries.some((e) => isFolder(e) && canExpand(levels, open, e.path));
}

/** Open right now under `dir` (or `dir` itself): something Collapse all
 *  would fold. */
export function canCollapse(open: ReadonlySet<string>, dir: string): boolean {
  if (open.has(dir)) return true;
  const inside = `${dir}/`;
  for (const p of open) if (p.startsWith(inside)) return true;
  return false;
}

/** Something to look at in the diff: any of git's marks. */
export function hasChanges(m: Marks): boolean {
  return !!(m.staged || m.changed || m.untracked || m.added || m.deleted || m.conflicted);
}

/** The one letter a row shows for its marks (the diff list's letters),
 *  and its words; null for none. A folder shows the strongest of its. */
export function markOf(m: Marks): { letter: string; words: string } | null {
  if (m.conflicted) return { letter: 'C', words: 'Conflicted' };
  if (m.deleted) return { letter: 'D', words: m.staged ? 'Deleted, with staged changes' : 'Deleted' };
  if (m.untracked) return { letter: 'U', words: 'Untracked' };
  if (m.added) return { letter: 'A', words: m.changed ? 'Added, with changes not staged' : 'Added' };
  if (m.changed) return { letter: 'M', words: m.staged ? 'Modified, partly staged' : 'Modified' };
  if (m.staged) return { letter: 'M', words: 'Modified, staged' };
  return null;
}

/** The Open in Kete cap (src-tauri/src/files.rs `MAX_BYTES`). */
export const VIEWER_MAX_BYTES = 2 * 1024 * 1024;

/** `abs` from the session's folder `cwd`: the rest of it, `/`-separated,
 *  when it is inside (`.` for the folder itself), else null. Windows paths
 *  compare without case. */
export function relativeTo(cwd: string, abs: string): string | null {
  const norm = (p: string) => p.replace(/\\/g, '/').replace(/\/+$/, '');
  const win = /^[a-z]:/i.test(cwd) || cwd.includes('\\');
  const c = norm(cwd);
  const a = norm(abs);
  const eq = (x: string, y: string) => (win ? x.toLowerCase() === y.toLowerCase() : x === y);
  if (eq(a, c)) return '.';
  if (a.length > c.length && a[c.length] === '/' && eq(a.slice(0, c.length), c)) return a.slice(c.length + 1);
  return null;
}

/** What a dropped path types into a session: relative to the session's
 *  folder when it is inside it, else whole; quoted when it has a space;
 *  a space after, so the next word or path doesn't run into it. */
export function insertText(abs: string, cwd: string | null): string {
  const rel = cwd ? relativeTo(cwd, abs) : null;
  const p = rel ?? abs;
  return `${/\s/.test(p) ? `"${p}"` : p} `;
}
