// Every keyboard shortcut the window answers, in one table: App.svelte
// dispatches through it, the help overlay (? or F1) lists it, and the
// command palette shows each action's keys from it. The rows and their
// default chords come from the verb registry (crates/nebula-config's
// actions.rs, through the generated actions.gen.ts); the user's own
// bindings from Settings, Hotkeys are laid over them at startup and on
// each change (`applyOverrides`). Row keys follow the terminal app's (r
// rename, a archive, u unarchive, m move, Shift+P duplicate) where the
// window has no other use for the letter.

import { DESK_KEYS, type ActionId, type DeskGroup, type DeskKeyId, type DeskScope } from './actions.gen';
import { SHORTCUTS } from './keytable.svelte';

/** A key row's id: the window's own, and its config key. Some read as a
 *  terminal app verb of another meaning (`palette` here is its
 *  `command_palette`), so they are never the registry's `ActionId`. */
export type ShortcutId = DeskKeyId;

/**
 * Where a key counts:
 * - `anywhere`: even while typing in a field or a terminal;
 * - `fields`: while typing in a field, but not inside a terminal (whose
 *   shell owns the key: Ctrl+K is kill-line, Alt+arrows are words);
 * - `global`: only when no field or terminal has the keys;
 * - `row`: like `global`, acting on the focused session row or card;
 * - `overview`, `board`: the home view's own walking keys, handled by
 *   Overview.svelte and Board.svelte (listed here for the help and the
 *   clash test; App.svelte never dispatches them, and they can't be
 *   rebound);
 * - `review`: the review row's Alt chords, handled by the review row
 *   itself (ReviewBar.svelte) while a review's diff is up, even inside
 *   the diff editor, as the terminal app's docked review takes them;
 *   fixed too.
 */
export type Scope = DeskScope;

export interface Shortcut {
  id: ShortcutId;
  /** Chords as `chordOf` spells them: `ctrl+shift+b`, `shift+k`, `?`. */
  keys: string[];
  scope: Scope;
  label: string;
  group: DeskGroup;
  /** The diff editor (Monaco) binds this chord itself: it stands down
   *  while the focus is in one. */
  editorOwns?: boolean;
  /** The user set this row's keys (a `desktop.*` entry in keybindings). */
  mine?: boolean;
}

// The live table, kept in a rune module so a tooltip drawn from it
// redraws when the user rebinds a key.
export { SHORTCUTS };

/** A key row's default chords. */
export function defaultKeys(id: ShortcutId): string[] {
  return [...(DESK_KEYS.find((k) => k.id === id)?.chords ?? [])];
}

/** The registry verb a key row runs. */
export function actionOf(id: ShortcutId): ActionId | undefined {
  return DESK_KEYS.find((k) => k.id === id)?.action;
}

/** The home views' walking keys and the review row's: listed, never
 *  rebound. */
export function isFixed(scope: Scope): boolean {
  return scope === 'overview' || scope === 'board' || scope === 'review';
}

/** Lay the user's bindings (key id to chords; an empty list unbinds) over
 *  the defaults. An id this build doesn't know is ignored. */
export function applyOverrides(over: Partial<Record<string, string[]>>, table: Shortcut[] = SHORTCUTS) {
  const theirs = new Set<string>();
  for (const s of table) {
    const k = DESK_KEYS.find((d) => d.id === s.id);
    if (!k) continue;
    const mine = isFixed(k.scope) ? undefined : over[s.id];
    if (mine) theirs.add(s.id);
    s.mine = !!mine;
    s.keys = [...(mine ?? k.chords)];
  }
  // A chord the user gave a row is theirs: a row still on its defaults
  // gives it up where their scopes meet, so a default added in a later
  // release (D, Z, Space, the digits) never shadows a binding saved before.
  for (const s of table) {
    if (theirs.has(s.id) || isFixed(s.scope)) continue;
    s.keys = s.keys.filter((c) => !table.some((o) => theirs.has(o.id) && o.keys.includes(c) && overlaps(s.scope, o.scope)));
  }
}

const CONTEXTS: KeyContext[] = [
  { inTerminal: false, typing: false, onRow: false },
  { inTerminal: false, typing: false, onRow: true },
  { inTerminal: false, typing: true, onRow: false },
  { inTerminal: true, typing: true, onRow: false },
];

/** Two scopes a chord could both fire in: some key context takes both,
 *  or one is a home view's walking key and the other a window key (which
 *  would take it from the view). */
export function overlaps(a: Scope, b: Scope): boolean {
  if (isFixed(a) || isFixed(b)) return true;
  return CONTEXTS.some((ctx) => inScope(a, ctx) && inScope(b, ctx));
}

/** The other rows `chord` would clash with if `id` took it. */
export function clashes(id: ShortcutId, chord: string, table: Shortcut[] = SHORTCUTS): Shortcut[] {
  const me = table.find((s) => s.id === id);
  if (!me) return [];
  return table.filter((s) => s.id !== id && s.keys.includes(chord) && overlaps(me.scope, s.scope));
}

/** The chord a key event spells: modifiers in ctrl, alt, shift order, then
 *  the key lower-cased. A printable non-letter (`?`, `,`) carries no shift:
 *  the character already says it. */
export function chordOf(e: Pick<KeyboardEvent, 'key' | 'ctrlKey' | 'altKey' | 'shiftKey' | 'metaKey'>): string {
  let key = e.key.toLowerCase();
  if (key === ' ') key = 'space';
  const printable = key.length === 1;
  const letter = printable && /[a-z]/.test(key);
  const mods = [e.ctrlKey || e.metaKey ? 'ctrl' : '', e.altKey ? 'alt' : '', e.shiftKey && (!printable || letter) ? 'shift' : ''];
  return [...mods.filter(Boolean), key].join('+');
}

/** Where the key event is aimed. */
export interface KeyContext {
  /** Inside an xterm pane. */
  inTerminal: boolean;
  /** Inside a Monaco editor (the diff), which owns its Ctrl chords:
   *  Ctrl+K starts its own chords there. */
  inEditor?: boolean;
  /** In an input, textarea, select or editor (a terminal counts too). */
  typing: boolean;
  /** A session row or card has the focus. */
  onRow: boolean;
}

export function inScope(scope: Scope, ctx: KeyContext): boolean {
  switch (scope) {
    case 'anywhere':
      return true;
    case 'fields':
      return !ctx.inTerminal;
    case 'global':
      return !ctx.typing;
    case 'row':
      return !ctx.typing && ctx.onRow;
    case 'overview':
    case 'board':
    case 'review':
      return false;
  }
}

/** The shortcut a key event triggers here, if any. */
export function shortcutFor(
  e: Pick<KeyboardEvent, 'key' | 'ctrlKey' | 'altKey' | 'shiftKey' | 'metaKey'>,
  ctx: KeyContext,
  table: Shortcut[] = SHORTCUTS,
): Shortcut | undefined {
  const chord = chordOf(e);
  return table.find((s) => s.keys.includes(chord) && inScope(s.scope, ctx) && !(ctx.inEditor && s.editorOwns));
}

/** The review row's key a key event is, if any: Alt and a letter. The
 *  letter the key types (`e.key`), so a layout's own letters hold; where
 *  Alt types a symbol instead (Option+P is a pi on a Mac), the letter on
 *  the physical key (`e.code`). */
export function reviewKeyFor(
  e: Pick<KeyboardEvent, 'key' | 'code' | 'ctrlKey' | 'altKey' | 'shiftKey' | 'metaKey'>,
  table: Shortcut[] = SHORTCUTS,
): Shortcut | undefined {
  if (!e.altKey || e.ctrlKey || e.metaKey || e.shiftKey) return undefined;
  const letter = /^[a-z]$/i.test(e.key) ? e.key.toLowerCase() : /^Key[A-Z]$/.test(e.code) ? e.code.slice(3).toLowerCase() : null;
  if (!letter) return undefined;
  const chord = `alt+${letter}`;
  return table.find((s) => s.scope === 'review' && s.keys.includes(chord));
}

/** A chord as the help overlay and palette print it: `Ctrl+Shift+B`. */
export function keyLabel(chord: string): string {
  const NAMES: Record<string, string> = {
    arrowleft: '\u2190',
    arrowright: '\u2192',
    arrowup: '\u2191',
    arrowdown: '\u2193',
    escape: 'Esc',
    delete: 'Del',
    contextmenu: 'Menu',
    space: 'Space',
  };
  return chord
    .split('+')
    .map((p) => NAMES[p] ?? (p.length === 1 ? p.toUpperCase() : p[0].toUpperCase() + p.slice(1)))
    .join('+');
}

/** The keys of an action, for a palette row or a menu item. */
export function keysOf(id: ShortcutId, table: Shortcut[] = SHORTCUTS): string[] {
  return table.find((s) => s.id === id)?.keys ?? [];
}

/** A tooltip naming an action's keys from the table: `Restart (Shift+R)`,
 *  so a button's hint can't drift from the key it promises. */
export function withKey(label: string, ids: ShortcutId | ShortcutId[], table: Shortcut[] = SHORTCUTS): string {
  const keys = [ids].flat().flatMap((id) => keysOf(id, table));
  return keys.length ? `${label} (${keys.map(keyLabel).join(' or ')})` : label;
}

/** A help row: one action's keys, merged across the table entries that
 *  share its label (Ctrl+K and Ctrl+Shift+P are one palette). Its scope
 *  is the narrowest of them, cleared when one works anywhere. */
export interface HelpRow {
  label: string;
  keys: string[];
  scope: Scope | null;
}

/** The table grouped for the help overlay, in table order. */
export function helpGroups(table: Shortcut[] = SHORTCUTS): { group: string; rows: HelpRow[] }[] {
  const out: { group: string; rows: HelpRow[] }[] = [];
  for (const s of table) {
    let g = out.find((x) => x.group === s.group);
    if (!g) out.push((g = { group: s.group, rows: [] }));
    const same = g.rows.find((r) => r.label === s.label);
    if (same) {
      same.keys.push(...s.keys);
      if (s.scope === 'anywhere' || same.scope !== s.scope) same.scope = null;
    } else g.rows.push({ label: s.label, keys: [...s.keys], scope: s.scope });
  }
  return out;
}

/** A key that only modifies the next: a capture waits past it. */
export function isModifierKey(key: string): boolean {
  return key === 'Control' || key === 'Shift' || key === 'Alt' || key === 'Meta' || key === 'AltGraph' || key === 'OS';
}

/** Whether `chord` may go on a row of `scope` (nebula-config's
 *  `typing_safe`): a row that counts while a field or terminal has the
 *  keys takes only a chord typing never sends, one with Ctrl or Alt, or an
 *  F key. Shift alone is a capital letter. */
export function typingSafe(scope: Scope, chord: string): boolean {
  if (scope !== 'anywhere' && scope !== 'fields') return true;
  const parts = chord.split('+');
  const key = parts[parts.length - 1];
  const n = /^f(\d{1,2})$/.exec(key);
  const fkey = !!n && Number(n[1]) >= 1 && Number(n[1]) <= 24;
  return parts.slice(0, -1).some((m) => m === 'ctrl' || m === 'alt') || fkey;
}
