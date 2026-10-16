// How a menu's rows become what it shows (the menu matrix, \u00a71.3-1.5):
// in the registry's order, a rule where the section changes (act, go, open,
// copy, change, organise, away), and a run of a submenu group's rows that
// mean something now folded into one `Title \u25b8` row at the first one's
// place. One alone stays a plain row in its whole words. Inside a submenu
// nothing folds again and no rule is drawn. Pure: actions.ts and rows.ts
// build the rows from what the window can do now.

import { MENU_SECTION_ORDER, type MenuSection } from '../actions.gen';
import type { MenuEntry, MenuItem } from '../ui/ui.svelte';
import { submenu } from './model';

/** One row before layout. */
export interface Row {
  /** The registry verb, or a local row's id (`local:session.review`). */
  id: string;
  /** Its section; null for a row after all of them, behind a rule (the
   *  card's `Worktree \u2039branch\u203a \u25b8`). */
  section: MenuSection | null;
  /** The submenu it folds into: the group, its title, and the row's word
   *  inside it. */
  group?: { id: string; title: string; label: string };
  /** The row in its whole words, as it stands alone. */
  item: MenuItem;
}

export interface LayoutOpts {
  /** Fold groups into submenus (never inside a submenu). */
  fold?: boolean;
  /** Rules between sections (never inside a submenu). */
  rules?: boolean;
  /** Entries ahead of the rows: the state banner. */
  head?: MenuEntry[];
}

/** The rows as a menu. */
export function layout(rows: readonly Row[], o: LayoutOpts = {}): MenuEntry[] {
  const fold = o.fold ?? true;
  const rules = o.rules ?? true;
  const count = new Map<string, number>();
  if (fold) for (const r of rows) if (r.group) count.set(r.group.id, (count.get(r.group.id) ?? 0) + 1);
  const placed: { section: MenuSection | null; item: MenuItem }[] = [];
  const folded = new Set<string>();
  for (const r of rows) {
    const g = r.group;
    if (!g || (count.get(g.id) ?? 0) < 2) {
      placed.push({ section: r.section, item: r.item });
      continue;
    }
    if (folded.has(g.id)) continue;
    folded.add(g.id);
    const kids = rows
      .filter((x) => x.group?.id === g.id)
      .map((x): MenuItem => ({ ...x.item, label: x.group!.label, full: x.item.full ?? x.item.label }));
    placed.push({ section: r.section, item: submenu(g.title, kids) });
  }
  const out: MenuEntry[] = [...(o.head ?? [])];
  let prev: MenuSection | null | undefined;
  for (const p of placed) {
    if (rules && prev !== undefined && p.section !== prev) out.push('sep');
    out.push(p.item);
    prev = p.section;
  }
  return out;
}

const rank = (s: MenuSection | null) => (s === null ? MENU_SECTION_ORDER.length : MENU_SECTION_ORDER.indexOf(s));

/** `rows` with `add` put in: before the first of `before` there is, else
 *  after the last of `after` there is, else at the end of its section. */
export function insertRows(rows: readonly Row[], add: Row | Row[], at: { before?: string[]; after?: string[] } = {}): Row[] {
  const adds = [add].flat();
  if (!adds.length) return [...rows];
  const out = [...rows];
  for (const id of at.before ?? []) {
    const i = out.findIndex((r) => r.id === id);
    if (i >= 0) return (out.splice(i, 0, ...adds), out);
  }
  for (const id of at.after ?? []) {
    let i = out.length - 1;
    while (i >= 0 && out[i].id !== id) i--;
    if (i >= 0) return (out.splice(i + 1, 0, ...adds), out);
  }
  const s = rank(adds[0].section);
  const i = out.findIndex((r) => rank(r.section) > s);
  out.splice(i < 0 ? out.length : i, 0, ...adds);
  return out;
}

/** The rows without the ones whose ids `drop` names. */
export function withoutIds(rows: readonly Row[], drop: Iterable<string>): Row[] {
  const gone = new Set(drop);
  return rows.filter((r) => !gone.has(r.id));
}
