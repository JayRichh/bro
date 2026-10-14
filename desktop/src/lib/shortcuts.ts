// Every keyboard shortcut the window answers, in one table: App.svelte
// dispatches through it, the help overlay (? or F1) lists it, and the
// command palette shows each action's keys from it. Row keys follow the
// terminal app's (r rename, a archive, u unarchive, m move, Shift+P duplicate)
// where the window has no other use for the letter.

export type ShortcutId =
  | 'palette'
  | 'palette_alt'
  | 'help'
  | 'help_f1'
  | 'settings'
  | 'back'
  | 'forward'
  | 'home_board'
  | 'home_overview'
  | 'toggle_home'
  | 'open_session'
  | 'open_diff'
  | 'quick_prompt'
  | 'quick_prompt_alt'
  | 'cycle_theme'
  | 'go_home'
  | 'go_session'
  | 'go_diff'
  | 'orchestrator'
  | 'orchestrator_f10'
  | 'close_split'
  | 'toggle_archived'
  | 'new_worktree'
  | 'rename'
  | 'archive'
  | 'unarchive'
  | 'delete'
  | 'restart'
  | 'move'
  | 'duplicate'
  | 'follow_up'
  | 'new_terminal'
  | 'open_editor'
  | 'open_remote'
  | 'switch_branch'
  | 'row_menu'
  | 'list_move'
  | 'list_open'
  | 'overview_fold'
  | 'board_lanes'
  | 'board_diff';

/**
 * Where a key counts:
 * - `anywhere`: even while typing in a field or a terminal;
 * - `fields`: while typing in a field, but not inside a terminal (whose
 *   shell owns the key: Ctrl+K is kill-line, Alt+arrows are words);
 * - `global`: only when no field or terminal has the keys;
 * - `row`: like `global`, acting on the focused session row or card;
 * - `overview`, `board`: the home view's own walking keys, handled by
 *   Overview.svelte and Board.svelte (listed here for the help and the
 *   clash test; App.svelte never dispatches them).
 */
export type Scope = 'anywhere' | 'fields' | 'global' | 'row' | 'overview' | 'board';

export interface Shortcut {
  id: ShortcutId;
  /** Chords as `chordOf` spells them: `ctrl+shift+b`, `shift+k`, `?`. */
  keys: string[];
  scope: Scope;
  label: string;
  group: 'General' | 'Views' | 'Sessions' | 'Overview and board';
  /** The diff editor (Monaco) binds this chord itself: it stands down
   *  while the focus is in one. */
  editorOwns?: boolean;
}

export const SHORTCUTS: Shortcut[] = [
  { id: 'palette', keys: ['ctrl+k'], scope: 'fields', label: 'Command palette', group: 'General', editorOwns: true },
  { id: 'palette_alt', keys: ['ctrl+shift+p'], scope: 'anywhere', label: 'Command palette', group: 'General' },
  { id: 'help', keys: ['?'], scope: 'global', label: 'Keyboard shortcuts', group: 'General' },
  { id: 'help_f1', keys: ['f1'], scope: 'anywhere', label: 'Keyboard shortcuts', group: 'General' },
  { id: 'settings', keys: ['ctrl+,'], scope: 'anywhere', label: 'Settings', group: 'General' },
  { id: 'quick_prompt', keys: ['p', 'n'], scope: 'global', label: 'Quick prompt', group: 'General' },
  { id: 'quick_prompt_alt', keys: ['ctrl+shift+n'], scope: 'anywhere', label: 'Quick prompt', group: 'General' },
  { id: 'cycle_theme', keys: ['ctrl+shift+l'], scope: 'anywhere', label: 'Cycle theme: system, light, dark', group: 'General' },
  { id: 'new_worktree', keys: ['shift+n'], scope: 'global', label: 'New worktree in this repo', group: 'General' },

  { id: 'back', keys: ['alt+arrowleft'], scope: 'fields', label: 'Back', group: 'Views' },
  { id: 'forward', keys: ['alt+arrowright'], scope: 'fields', label: 'Forward', group: 'Views' },
  { id: 'home_board', keys: ['ctrl+shift+b'], scope: 'anywhere', label: 'Board', group: 'Views' },
  { id: 'home_overview', keys: ['ctrl+shift+o'], scope: 'anywhere', label: 'Overview', group: 'Views' },
  { id: 'toggle_home', keys: ['shift+k'], scope: 'global', label: 'Switch overview and board', group: 'Views' },
  { id: 'open_session', keys: ['ctrl+shift+t'], scope: 'anywhere', label: 'Show the session', group: 'Views' },
  { id: 'open_diff', keys: ['ctrl+shift+d'], scope: 'anywhere', label: 'Diff the worktree', group: 'Views' },
  { id: 'go_home', keys: ['1'], scope: 'global', label: 'Home', group: 'Views' },
  { id: 'go_session', keys: ['2'], scope: 'global', label: 'Session', group: 'Views' },
  { id: 'go_diff', keys: ['3'], scope: 'global', label: 'Diff', group: 'Views' },
  // F10 as in the terminal app, but never inside a shell (htop and mc quit on it).
  { id: 'orchestrator', keys: ['0'], scope: 'global', label: 'Orchestrator (start one when there is none)', group: 'Views' },
  { id: 'orchestrator_f10', keys: ['f10'], scope: 'fields', label: 'Orchestrator (start one when there is none)', group: 'Views' },
  { id: 'close_split', keys: ['escape'], scope: 'global', label: 'Close the split or Settings', group: 'Views' },
  { id: 'toggle_archived', keys: ['shift+a'], scope: 'global', label: 'Show or hide archived sessions', group: 'Views' },

  { id: 'rename', keys: ['r'], scope: 'row', label: 'Rename', group: 'Sessions' },
  { id: 'archive', keys: ['a'], scope: 'row', label: 'Archive', group: 'Sessions' },
  { id: 'unarchive', keys: ['u'], scope: 'row', label: 'Unarchive', group: 'Sessions' },
  { id: 'delete', keys: ['delete'], scope: 'row', label: 'Delete (asks first)', group: 'Sessions' },
  { id: 'restart', keys: ['shift+r'], scope: 'row', label: 'Restart the agent', group: 'Sessions' },
  { id: 'move', keys: ['m'], scope: 'row', label: 'Move to another worktree', group: 'Sessions' },
  { id: 'duplicate', keys: ['shift+p'], scope: 'row', label: 'Duplicate into the quick prompt', group: 'Sessions' },
  { id: 'follow_up', keys: ['f'], scope: 'row', label: 'Follow-up prompt', group: 'Sessions' },
  { id: 'new_terminal', keys: ['t'], scope: 'row', label: 'New terminal in its worktree', group: 'Sessions' },
  { id: 'open_editor', keys: ['shift+o'], scope: 'row', label: 'Open its worktree in the editor', group: 'Sessions' },
  { id: 'open_remote', keys: ['shift+g'], scope: 'row', label: 'Open the repo remote in the browser', group: 'Sessions' },
  { id: 'switch_branch', keys: ['c'], scope: 'row', label: 'Switch the repo root checkout to another branch', group: 'Sessions' },
  { id: 'row_menu', keys: ['shift+f10', 'contextmenu'], scope: 'row', label: 'Actions menu', group: 'Sessions' },

  { id: 'list_move', keys: ['j', 'k', 'arrowdown', 'arrowup'], scope: 'overview', label: 'Move through the rows (cards on the board)', group: 'Overview and board' },
  { id: 'list_open', keys: ['enter', 'space'], scope: 'overview', label: 'Open the session', group: 'Overview and board' },
  { id: 'overview_fold', keys: ['arrowright', 'arrowleft'], scope: 'overview', label: 'Overview: unfold, fold nested sessions', group: 'Overview and board' },
  { id: 'board_lanes', keys: ['h', 'l'], scope: 'board', label: 'Board: across lanes (also the side arrows)', group: 'Overview and board' },
  { id: 'board_diff', keys: ['d'], scope: 'board', label: 'Board: diff the selected card', group: 'Overview and board' },
];

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

/** A chord as the help overlay and palette print it: `Ctrl+Shift+B`. */
export function keyLabel(chord: string): string {
  const NAMES: Record<string, string> = {
    arrowleft: '\u2190',
    arrowright: '\u2192',
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
