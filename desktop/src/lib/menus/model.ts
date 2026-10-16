// The context menu's behaviour, without the page: which row the keys are
// on, what a key does (walk, open a submenu, close one level, run), what
// typing narrows the rows to, where a submenu goes in the window, and
// whether the pointer is on its way into an open submenu. Menu.svelte
// draws it; the tests drive it.
//
// One level of submenu only: the menu matrix's rule (\u00a71.4) is that
// submenus never nest, so a child's own children are ignored.

import type { MenuEntry, MenuItem } from '../ui/ui.svelte';
import { filterRows } from '../verbs';

export function isItem(e: MenuEntry | undefined): e is MenuItem {
  return typeof e === 'object' && e !== null && 'run' in e;
}

/** A row that opens a submenu. */
export function hasChildren(e: MenuEntry | undefined): e is MenuItem & { children: MenuEntry[] } {
  return isItem(e) && !!e.children?.length;
}

const nothing = () => {};

/** A submenu row: `Title \u25b8` over `children`. It runs nothing itself. */
export function submenu(label: string, children: MenuEntry[], extra: Partial<MenuItem> = {}): MenuItem {
  return { label, run: nothing, ...extra, children };
}

/** The separator between a submenu's title and a child's words in a
 *  flattened row: `Open in \u203a VS Code`. */
export const PATH_SEP = ' \u203a ';

/** Every row that runs something, a submenu's rows in its place with
 *  their path for words (`Open in \u203a VS Code`), headings and rules gone:
 *  what type-ahead searches and the palette lists. A child keeps its
 *  whole words in `full` (the palette's label). */
export function leafItems(entries: readonly MenuEntry[]): MenuItem[] {
  const out: MenuItem[] = [];
  for (const e of entries) {
    if (!isItem(e)) continue;
    if (!e.children?.length) {
      out.push(e);
      continue;
    }
    for (const c of e.children) {
      if (!isItem(c) || c.children?.length) continue;
      const path = `${e.label}${PATH_SEP}${c.label}`;
      out.push({ ...c, label: path, full: c.full ?? path });
    }
  }
  return out;
}

/** The rows the keys can land on: live items, a submenu row included. */
export function actionable(rows: readonly MenuEntry[]): number[] {
  const out: number[] = [];
  rows.forEach((e, i) => isItem(e) && !e.disabled && out.push(i));
  return out;
}

/** `d` rows on from `at` among the live ones, round the ends; from none,
 *  the first (down) or the last (up). */
export function step(rows: readonly MenuEntry[], at: number, d: number): number {
  const live = actionable(rows);
  if (!live.length) return -1;
  const i = live.indexOf(at);
  if (i < 0) return d > 0 ? live[0] : live[live.length - 1];
  return live[(i + d + live.length) % live.length];
}

export const first = (rows: readonly MenuEntry[]) => actionable(rows)[0] ?? -1;
export const last = (rows: readonly MenuEntry[]) => actionable(rows).at(-1) ?? -1;

/** The rows a typed query leaves: every runnable row, submenus' rows in
 *  their place (`Open in \u203a VS Code`), whose words hold the query's
 *  letters in order, best first; the pinned last row (All keys) always. No
 *  query: the menu as it is. */
export function narrow(
  entries: readonly MenuEntry[],
  query: string,
  match: (q: string, s: string) => number | null,
): MenuEntry[] {
  if (!query.trim()) return [...entries];
  return filterRows(leafItems(entries), query, match);
}

/** A key typed into a menu's query, or null for one the menu walks with.
 *  Before anything is typed, j and k walk and Space runs (as they always
 *  have); once a query has begun they are letters too. */
export function typedChar(e: { key: string; ctrlKey?: boolean; altKey?: boolean; metaKey?: boolean }, query: string): string | null {
  if (e.ctrlKey || e.altKey || e.metaKey || [...e.key].length !== 1) return null;
  if (!query && (e.key === 'j' || e.key === 'k' || e.key === ' ')) return null;
  return e.key;
}

/** Where the keys are: the active row of the menu, the row whose submenu
 *  is open (-1: none), the active row in that submenu, and which of the
 *  two lists has the keys. */
export interface Nav {
  at: number;
  open: number;
  subAt: number;
  inSub: boolean;
}

export const NAV0: Nav = { at: -1, open: -1, subAt: -1, inSub: false };

/** The keys a menu answers, by what they do. */
export type NavKey = 'down' | 'up' | 'home' | 'end' | 'right' | 'left' | 'enter' | 'space' | 'escape' | 'tab';

const KEY_OF: Record<string, NavKey> = {
  ArrowDown: 'down',
  ArrowUp: 'up',
  Home: 'home',
  End: 'end',
  ArrowRight: 'right',
  ArrowLeft: 'left',
  Enter: 'enter',
  ' ': 'space',
  Escape: 'escape',
  Tab: 'tab',
};

/** A key event's menu meaning; `walk`: j and k walk too (no query, no
 *  search field). */
export function navKeyOf(key: string, walk: boolean): NavKey | null {
  if (walk && key === 'j') return 'down';
  if (walk && key === 'k') return 'up';
  return KEY_OF[key] ?? null;
}

/** What a key did: the keys' new place, a row to run, or the whole menu
 *  to close. */
export interface NavOut {
  nav: Nav;
  run?: MenuItem;
  close?: true;
}

/** Open `rows[at]`'s submenu with the keys in it, on its first live row. */
export function openSub(nav: Nav, rows: readonly MenuEntry[], at = nav.at): Nav {
  const r = rows[at];
  if (!hasChildren(r) || r.disabled) return nav;
  return { at, open: at, subAt: first(r.children), inSub: true };
}

/** Close the open submenu; the keys go back to its row. */
export function closeSub(nav: Nav): Nav {
  return nav.open < 0 ? nav : { at: nav.open, open: -1, subAt: -1, inSub: false };
}

/** Key `k` on a menu showing `rows`. \u2192 / Enter / Space open a submenu; \u2190
 *  or Esc close one level; Enter or Space on a row runs it; a \u25b8 row never
 *  runs. Home and End jump. */
export function navKey(nav: Nav, k: NavKey, rows: readonly MenuEntry[]): NavOut {
  if (k === 'tab') return { nav, close: true };
  const sub = nav.open >= 0 && hasChildren(rows[nav.open]) ? rows[nav.open] : null;
  if (nav.inSub && sub) {
    const kids = (sub as MenuItem).children!;
    switch (k) {
      case 'down':
      case 'up':
        return { nav: { ...nav, subAt: step(kids, nav.subAt, k === 'down' ? 1 : -1) } };
      case 'home':
        return { nav: { ...nav, subAt: first(kids) } };
      case 'end':
        return { nav: { ...nav, subAt: last(kids) } };
      case 'left':
      case 'escape':
        return { nav: closeSub(nav) };
      case 'enter':
      case 'space': {
        const c = kids[nav.subAt];
        return isItem(c) && !c.disabled && !c.children?.length ? { nav, run: c } : { nav };
      }
      default:
        return { nav };
    }
  }
  switch (k) {
    case 'down':
    case 'up':
      return { nav: { ...NAV0, at: step(rows, nav.at, k === 'down' ? 1 : -1) } };
    case 'home':
      return { nav: { ...NAV0, at: first(rows) } };
    case 'end':
      return { nav: { ...NAV0, at: last(rows) } };
    case 'right':
      return { nav: openSub(nav, rows) };
    case 'enter':
    case 'space': {
      const r = rows[nav.at];
      if (hasChildren(r)) return { nav: openSub(nav, rows) };
      return isItem(r) && !r.disabled ? { nav, run: r } : { nav };
    }
    case 'left':
      return { nav: closeSub(nav) };
    case 'escape':
      return nav.open >= 0 ? { nav: closeSub(nav) } : { nav, close: true };
    default:
      return { nav };
  }
}

export interface Rect {
  left: number;
  top: number;
  right: number;
  bottom: number;
}

/** Where a submenu of `size` goes beside its row: to the right of the
 *  menu, its first row level with the row; flipped to the left where the
 *  window ends, and up where it would run off the bottom. `gap`: the
 *  menu's border and top padding, so the rows line up. */
export function placeSub(
  row: Rect,
  menu: Rect,
  size: { width: number; height: number },
  view: { width: number; height: number },
  gap = 5,
): { x: number; y: number; left: boolean } {
  const edge = 4;
  let x = menu.right - 1;
  let left = false;
  if (x + size.width > view.width - edge) {
    x = menu.left - size.width + 1;
    left = true;
  }
  x = Math.max(edge, Math.min(x, view.width - size.width - edge));
  let y = row.top - gap;
  if (y + size.height > view.height - edge) y = Math.max(edge, row.bottom + gap - size.height);
  y = Math.max(edge, Math.min(y, view.height - size.height - edge));
  return { x, y, left };
}

type Pt = { x: number; y: number };

function side(p: Pt, a: Pt, b: Pt): number {
  return (p.x - b.x) * (a.y - b.y) - (a.x - b.x) * (p.y - b.y);
}

/** Is `p` inside triangle `a b c` (on an edge counts). */
export function inTriangle(p: Pt, a: Pt, b: Pt, c: Pt): boolean {
  const d1 = side(p, a, b);
  const d2 = side(p, b, c);
  const d3 = side(p, c, a);
  const neg = d1 < 0 || d2 < 0 || d3 < 0;
  const pos = d1 > 0 || d2 > 0 || d3 > 0;
  return !(neg && pos);
}

/** The pointer, moving from `from` to `to`, is on its way into the open
 *  submenu `sub` (on the menu's right, or its left when it flipped): it is
 *  inside the triangle from where it was to the submenu's near edge. A
 *  diagonal move across other rows keeps the submenu then. */
export function aimsAt(from: Pt, to: Pt, sub: Rect, left: boolean): boolean {
  const x = left ? sub.right : sub.left;
  return inTriangle(to, from, { x, y: sub.top - 8 }, { x, y: sub.bottom + 8 });
}
