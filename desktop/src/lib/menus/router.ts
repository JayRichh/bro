// The window's one right-click router. One `contextmenu` listener on the
// document, in the capture phase, stops WebView2's own menu (Back,
// Refresh, Print, Inspect) on every right-click whatever the target, then
// opens Kete's menu for what is under the pointer (hit.ts says what that
// is). src-tauri's quiet.rs turns the browser menu off at the source too;
// this is the backstop, and the only place menus open from the mouse.
//
// A menu comes from a builder:
// - per hit kind (`setBuilder('text', ...)`): the text field, terminal,
//   viewer, selection and background menus (builtin.ts registers them);
// - per row kind, for a `[data-target]` row (`setRowBuilder('agent', ...)`):
//   `agent:7` goes to the `agent` builder with id `7`;
// - per element (`use:ownMenu={...}`), for a surface whose menu is its own.
//
// The menu opens once the event has run its course: xterm's own handler
// selects the word under the pointer first (`rightClickSelectsWord`), and
// nothing here stops the event, so that still happens.

import { closeMenu, openMenu, ui, type MenuEntry } from '../ui/ui.svelte';
import { menuPoint, resolveHit, targetKind, type Hit, type HitKind, type MenuNode } from './hit';

export type Point = { x: number; y: number };

/** A menu to open. `keep`: the focus stays where it is (a field, the
 *  terminal, an editor) and the menu takes the keys from the window, as a
 *  native menu does: the selection stays lit, a field's blur handlers
 *  don't fire, the shell sees no focus change. `later`: rows that need a
 *  moment (the clipboard read that decides Paste) replace the first ones
 *  when they come, if the menu is still up. */
export type Opened = {
  items: MenuEntry[];
  label?: string;
  keep?: boolean;
  later?: Promise<MenuEntry[] | null>;
};

type HitOf<K extends HitKind> = Extract<Hit, { kind: K }>;
export type Builder<K extends HitKind = HitKind> = (hit: HitOf<K>, at: Point) => Opened | null;
/** A row's menu: `id` is the target past its kind (`7` of `agent:7`). */
export type RowBuilder = (id: string, el: HTMLElement, target: string) => Opened | null;
/** An element's own menu. */
export type OwnMenu = (at: Point, el: HTMLElement) => Opened | null;

const builders: { [K in HitKind]?: Builder<K> } = {};
const rowBuilders = new Map<string, RowBuilder>();
const owned = new WeakMap<object, OwnMenu>();

export function setBuilder<K extends HitKind>(kind: K, build: Builder<K>) {
  (builders as Record<HitKind, unknown>)[kind] = build;
}

export function setRowBuilder(kind: string, build: RowBuilder) {
  rowBuilders.set(kind, build);
}

/** Svelte action: a right-click anywhere in `node` opens `build`'s menu
 *  (hit.ts rule 5). */
export function ownMenu(node: HTMLElement, build: OwnMenu) {
  owned.set(node, build);
  return {
    update: (next: OwnMenu) => void owned.set(node, next),
    destroy: () => void owned.delete(node),
  };
}

// Rows: the target's kind's builder, the clicked control (or the row)
// focused first so the row keys and Esc's focus return land on it.
setBuilder('row', (hit) => {
  const build = rowBuilders.get(targetKind(hit.target));
  if (!build) return null;
  (hit.focus as HTMLElement | null)?.focus?.();
  return build(hit.target.slice(targetKind(hit.target).length + 1), hit.el as HTMLElement, hit.target);
});
setBuilder('own', (hit, at) => {
  (hit.focus as HTMLElement | null)?.focus?.();
  return owned.get(hit.el)?.(at, hit.el as HTMLElement) ?? null;
});

/** Open the menu for `hit` at `at`; false when it has none. */
export function openHit(hit: Hit, at: Point): boolean {
  const build = builders[hit.kind] as Builder | undefined;
  const o = build?.(hit, at);
  if (!o?.items.length) return false;
  openMenu(o.items, at, o.label, { keep: o.keep });
  const opened = ui.menu;
  o.later
    ?.then((items) => {
      if (items?.length && opened && ui.menu === opened) opened.items = items;
    })
    .catch(() => {});
  return true;
}

/** The last right-click as the router saw it: the e2e suite reads it
 *  (`window.__keteContextMenu`) to know WebView2's menu was stopped. */
export const lastContextMenu = { count: 0, kind: '' as HitKind | '', prevented: false };

type Walkable = MenuNode & { parentElement?: Walkable | null };

/** `n` and the elements around it, innermost first: a path as
 *  `composedPath()` gives one. */
function ancestors(n: Walkable | null | undefined): Walkable[] {
  const out: Walkable[] = [];
  for (let at = n; at; at = at.parentElement) out.push(at);
  return out;
}

/** The menu's own layers: its boxes and the scrim behind them. */
const isLayer = (n: Walkable) => n.getAttribute?.('data-menu-scrim') != null || n.getAttribute?.('role') === 'menu';

/** Listen on `doc`; returns the stop. */
export function installMenuRouter(
  doc: Pick<Document, 'addEventListener' | 'removeEventListener' | 'getSelection'> & { elementsFromPoint?: (x: number, y: number) => Element[] } = document,
): () => void {
  // The page's selection covers `n` (hit.ts rule 4): a real selection,
  // not a caret.
  const selected = (n: MenuNode) => {
    const sel = doc.getSelection();
    if (!sel || sel.isCollapsed || !sel.toString().trim()) return false;
    return sel.containsNode(n as Node, true);
  };
  const own = (n: MenuNode) => owned.has(n);
  const note = (kind: HitKind, e: MouseEvent) => {
    lastContextMenu.count++;
    lastContextMenu.kind = kind;
    lastContextMenu.prevented = e.defaultPrevented;
  };
  const onMenu = (e: MouseEvent) => {
    e.preventDefault();
    const path = e.composedPath();
    // A right-click on the scrim behind an open menu (anywhere outside it):
    // that menu goes and the one for what is under the pointer opens, in
    // one click. The Menu key's late event lands on the focused element,
    // not the scrim, so a keyboard open is never doubled.
    const first = path[0] as Walkable | undefined;
    if (ui.menu && first && first.getAttribute?.('data-menu-scrim') != null) {
      const under = (doc.elementsFromPoint?.(e.clientX, e.clientY) as unknown as Walkable[] | undefined)?.find(
        (n) => !ancestors(n).some(isLayer),
      );
      closeMenu();
      const hit = resolveHit(ancestors(under), { selected, own });
      note(hit.kind, e);
      const at = menuPoint(e, 'el' in hit ? (hit.el as Element | null) : null);
      setTimeout(() => openHit(hit, at), 0);
      return;
    }
    const hit = resolveHit(path, { menuOpen: !!ui.menu, selected, own });
    note(hit.kind, e);
    if (hit.kind === 'menu') return;
    const at = menuPoint(e, 'el' in hit ? (hit.el as Element | null) : null);
    setTimeout(() => openHit(hit, at), 0);
  };
  doc.addEventListener('contextmenu', onMenu, true);
  (globalThis as { __keteContextMenu?: typeof lastContextMenu }).__keteContextMenu = lastContextMenu;
  return () => doc.removeEventListener('contextmenu', onMenu, true);
}

