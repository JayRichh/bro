// The live terminals and Monaco viewers, by their host element, so a
// right-click on one (hit.ts: `terminal`, `viewer`) reaches the instance
// under it: Terminal.svelte, DiffPane.svelte and FileSheet.svelte register
// theirs while they are mounted.

import type * as Monaco from 'monaco-esm/editor/editor.api.js';

/** A terminal pane, as its menu (builtin.ts) works it. */
export interface TerminalSurface {
  /** Its session is attached and running. */
  live(): boolean;
  hasSelection(): boolean;
  selection(): string;
  clearSelection(): void;
  /** Write to the session as Ctrl+V does (bracketed when the CLI asked). */
  paste(text: string): void;
  selectAll(): void;
  clear(): void;
  focus(): void;
  /** The session's worktree folder. */
  folder(): string | null;
  /** The URL under the pointer, while the web-links addon has one. */
  link(): string | null;
  openLink(url: string): void;
}

/** A Monaco viewer: one editor, or a diff's two. */
export interface ViewerSurface {
  /** The editor whose DOM holds `el`; `old` for a diff's original side. */
  editorAt(el: Element): { editor: Monaco.editor.ICodeEditor; old: boolean } | null;
  /** The shown file's path on disk. */
  path(): string | null;
}

const terminals = new WeakMap<Element, TerminalSurface>();
const viewers = new WeakMap<Element, ViewerSurface>();

function nearest<T>(map: WeakMap<Element, T>, el: Element | null): T | null {
  for (let at = el; at; at = at.parentElement) {
    const s = map.get(at);
    if (s) return s;
  }
  return null;
}

/** Register a terminal pane's host; returns the unregister. */
export function registerTerminal(host: Element, s: TerminalSurface): () => void {
  terminals.set(host, s);
  return () => terminals.delete(host);
}

/** Register a viewer's host; returns the unregister. */
export function registerViewer(host: Element, s: ViewerSurface): () => void {
  viewers.set(host, s);
  return () => viewers.delete(host);
}

export const terminalAt = (el: Element | null) => nearest(terminals, el);
export const viewerAt = (el: Element | null) => nearest(viewers, el);

/** `rel` (a diff's repo-relative path) under `root`, with `root`'s
 *  separators: `C:\code\app` + `src/a.ts` is `C:\code\app\src\a.ts`. */
export function joinPath(root: string, rel: string): string {
  const sep = root.includes('\\') && !root.includes('/') ? '\\' : '/';
  return root.replace(/[\\/]+$/, '') + sep + rel.split(/[\\/]+/).join(sep);
}
