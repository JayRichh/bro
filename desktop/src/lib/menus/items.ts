// The rows of the menus that replaced WebView2's own: a text field's, the
// terminal's, a Monaco viewer's and selected page text's. Pure: what is
// shown and what is greyed out comes from a state the caller reads
// (builtin.ts reads it off the page), so the tests build states.

import type { MenuEntry, MenuItem } from '../ui/ui.svelte';

/** A text field, as its menu needs it. `clipboard`: whether the clipboard
 *  holds text; null while that isn't known (Paste stays on). */
export interface FieldState {
  /** Typing changes it (not read-only). */
  editable: boolean;
  /** A password: never copied out. */
  secret: boolean;
  selected: boolean;
  empty: boolean;
  canUndo: boolean;
  clipboard: boolean | null;
}

export interface FieldVerbs {
  undo(): void;
  cut(): void;
  copy(): void;
  paste(): void;
  selectAll(): void;
}

const NOTHING_SELECTED = 'Nothing is selected';
const READ_ONLY = 'This field is read-only';
const CLIPBOARD_EMPTY = 'The clipboard holds no text';

/** A row, greyed out with `why` as its tooltip when `off`. */
function row(label: string, keys: string[], run: () => void, off: string | false): MenuItem {
  return off ? { label, keys, run, disabled: true, title: off } : { label, keys, run };
}

/** Undo, Cut, Copy, Paste, Select all, as the browser's menu had them. */
export function fieldItems(s: FieldState, v: FieldVerbs): MenuEntry[] {
  const ro = !s.editable && READ_ONLY;
  const sel = !s.selected && NOTHING_SELECTED;
  const secret = s.secret && 'A password is never copied';
  return [
    row('Undo', ['ctrl+z'], v.undo, ro || (!s.canUndo && 'Nothing to undo')),
    'sep',
    row('Cut', ['ctrl+x'], v.cut, ro || secret || sel),
    row('Copy', ['ctrl+c'], v.copy, secret || sel),
    row('Paste', ['ctrl+v'], v.paste, ro || (s.clipboard === false && CLIPBOARD_EMPTY)),
    'sep',
    row('Select all', ['ctrl+a'], v.selectAll, s.empty && 'The field is empty'),
  ];
}

/** The terminal under the pointer. `live`: its session is attached and
 *  running (Paste writes to it). `folder`: its worktree's path. `link`:
 *  the URL under the pointer, as the web-links addon found it. */
export interface TermState {
  selected: boolean;
  live: boolean;
  clipboard: boolean | null;
  folder: string | null;
  link: string | null;
}

export interface TermVerbs {
  copy(): void;
  paste(): void;
  selectAll(): void;
  clear(): void;
  copyFolder(): void;
  openLink(): void;
  copyLink(): void;
}

/** Copy, Paste, Select all, Clear, Copy folder path, and Open link / Copy
 *  link while one is under the pointer. */
export function terminalItems(s: TermState, v: TermVerbs): MenuEntry[] {
  const out: MenuEntry[] = [
    row('Copy', ['ctrl+c'], v.copy, !s.selected && NOTHING_SELECTED),
    row('Paste', ['ctrl+v'], v.paste, (!s.live && 'The session is not running') || (s.clipboard === false && CLIPBOARD_EMPTY)),
    'sep',
    { label: 'Select all', run: v.selectAll },
    { label: 'Clear', run: v.clear, title: 'Clear what this pane shows; the session keeps running' },
  ];
  if (s.folder) out.push('sep', { label: 'Copy folder path', run: v.copyFolder, title: s.folder });
  if (s.link) out.push('sep', { label: 'Open link', run: v.openLink, title: s.link }, { label: 'Copy link', run: v.copyLink, title: s.link });
  return out;
}

/** A Monaco viewer (the diff, a file `kete open` showed). `path`: the
 *  file's path on disk; `line`: the line under the pointer; `old`: the old
 *  side of a diff, whose lines are not the file's on disk. */
export interface ViewerState {
  selected: boolean;
  path: string | null;
  line: number | null;
  old: boolean;
  /** The editor "Open this line in" names (VS Code, Cursor); null or
   *  absent with none installed. */
  editor?: string | null;
}

export interface ViewerVerbs {
  copy(): void;
  selectAll(): void;
  copyPath(withLine: boolean): void;
  /** Open the file at the line in that editor. */
  openLine?(): void;
}

/** Copy, Select all, then Copy path:line (Copy path on a diff's old side,
 *  or with no line), then Open this line in VS Code (not from a diff's old
 *  side, whose lines are not the file's now). */
export function viewerItems(s: ViewerState, v: ViewerVerbs): MenuEntry[] {
  const out: MenuEntry[] = [
    row('Copy', ['ctrl+c'], v.copy, !s.selected && NOTHING_SELECTED),
    { label: 'Select all', keys: ['ctrl+a'], run: v.selectAll },
  ];
  if (s.path) {
    const line = s.line != null && !s.old;
    out.push('sep', {
      label: line ? 'Copy path:line' : 'Copy path',
      run: () => v.copyPath(line),
      title: line ? `${s.path}:${s.line}` : s.old ? `${s.path} (the old side's lines are not the file's now)` : s.path,
    });
  }
  if (s.path && s.editor && !s.old && v.openLine) {
    const line = s.line != null;
    out.push({
      label: line ? `Open this line in ${s.editor}` : `Open in ${s.editor}`,
      run: v.openLine,
      title: line ? `${s.path}:${s.line}` : s.path,
    });
  }
  return out;
}

/** Selected page text (an error line, a review's history): Copy. */
export function selectionItems(copy: () => void): MenuEntry[] {
  return [{ label: 'Copy', keys: ['ctrl+c'], run: copy }];
}
