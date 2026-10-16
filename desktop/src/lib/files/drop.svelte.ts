// A Files row (a file or a folder) dragged onto a session: its row in the
// overview, its board card, or its terminal. The path goes into the
// session's input the way Ctrl+V pastes (TerminalSurface.paste: bracketed
// when the CLI asked for it), never with Enter: relative to the session's
// folder when it is inside it, else whole, quoted when it has a space
// (tree.ts `insertText`). A row or card whose terminal isn't on screen
// opens it first, as a click would, and the path follows once its replay
// has been read. The keyboard way: Copy path in the row's menu, then
// paste.
//
// The drag has its own type and state, apart from the session, worktree
// and repo drags (reorder.svelte.ts `drag`), so their reorder marks and
// moves never answer a path. A text field takes it as plain text.

import type { SessionRef } from '../api';
import { app, openSession, sessionKey, sessionWorktree, toast } from '../store.svelte';
import { terminalFor, type TerminalSurface } from '../menus/surfaces';
import { insertText } from './tree';

export const DRAG_PATH = 'application/x-kete-path';

export interface DraggedPath {
  worktree: string;
  /** From the checkout's root. */
  path: string;
  /** On disk. */
  abs: string;
  folder: boolean;
}

/** The path in flight (`getData` is empty while dragging over a target). */
export const pathDrag = $state<{ item: DraggedPath | null }>({ item: null });

export function startPathDrag(e: DragEvent, item: DraggedPath) {
  if (!e.dataTransfer) return;
  e.dataTransfer.setData(DRAG_PATH, JSON.stringify(item));
  e.dataTransfer.setData('text/plain', item.abs);
  e.dataTransfer.effectAllowed = 'copy';
  pathDrag.item = item;
}

export function endPathDrag() {
  pathDrag.item = null;
}

const carries = (e: DragEvent) => !!e.dataTransfer && Array.from(e.dataTransfer.types).includes(DRAG_PATH);

function dragged(e: DragEvent): DraggedPath | null {
  try {
    const raw = e.dataTransfer?.getData(DRAG_PATH);
    if (raw) return JSON.parse(raw) as DraggedPath;
  } catch {
    /* the state has it */
  }
  return pathDrag.item;
}

const nameOf = (s: SessionRef) =>
  'Agent' in s ? (app.model.agents.find((a) => a.id === s.Agent)?.name ?? 'The session') : (app.model.terminals.find((t) => t.id === s.Terminal)?.name ?? 'The terminal');

function alive(s: SessionRef): boolean {
  if ('Agent' in s) {
    const a = app.model.agents.find((x) => x.id === s.Agent);
    return !!a && a.alive && !a.archived;
  }
  return !!app.model.terminals.find((t) => t.id === s.Terminal)?.alive;
}

/** Until `probe` gives something, or `ms` pass. */
async function until<T>(probe: () => T | null, ms: number): Promise<T | null> {
  const end = Date.now() + ms;
  for (;;) {
    const v = probe();
    if (v || Date.now() > end) return v;
    await new Promise((r) => setTimeout(r, 50));
  }
}

/** Type `item`'s path into session `s`, through `pane` when the drop was
 *  on its terminal, else opening it first. */
export async function insertPath(s: SessionRef, item: DraggedPath, pane?: TerminalSurface): Promise<void> {
  const name = nameOf(s);
  if (!alive(s)) return void toast(`${name} is not running: restart it, then drop the path again`);
  const cwd = app.model.worktrees.find((w) => w.id === sessionWorktree(s))?.path ?? null;
  const text = insertText(item.abs, cwd);
  const key = sessionKey(s);
  if (!pane) openSession(s);
  const at = await until(() => (pane?.ready() ? pane : terminalFor(key, sessionKey)), 8000);
  if (!at) return void toast(`Could not reach ${name} to type the path: copy it from the row's menu and paste it there`);
  at.paste(text);
  at.focus();
}

/** Svelte action: drops of a path on `node` go to the session `target()`
 *  names (null: none here), through `pane()` when `node` is its terminal.
 *  An ember outline marks the drop while a path is over it, as a session
 *  dragged onto a worktree marks it. */
export function pathDrop(node: HTMLElement, arg: { target: () => SessionRef | null; pane?: () => TerminalSurface | null }) {
  let a = arg;
  let lit = false;
  const light = (on: boolean) => {
    if (on === lit) return;
    lit = on;
    node.style.outline = on ? '2px solid var(--color-ember)' : '';
    node.style.outlineOffset = on ? '-2px' : '';
  };
  const over = (e: DragEvent) => {
    if (!carries(e) || !a.target()) return;
    e.preventDefault();
    if (e.dataTransfer) e.dataTransfer.dropEffect = 'copy';
    light(true);
  };
  const leave = (e: DragEvent) => {
    const to = e.relatedTarget as Node | null;
    if (!to || !node.contains(to)) light(false);
  };
  const drop = (e: DragEvent) => {
    if (!carries(e)) return;
    light(false);
    const s = a.target();
    if (!s) return;
    // Taken here: no text dropped into the terminal's own input too.
    e.preventDefault();
    const item = dragged(e);
    endPathDrag();
    if (item) void insertPath(s, item, a.pane?.() ?? undefined);
  };
  const off = () => light(false);
  node.addEventListener('dragenter', over);
  node.addEventListener('dragover', over);
  node.addEventListener('dragleave', leave);
  node.addEventListener('drop', drop);
  window.addEventListener('dragend', off, true);
  return {
    update: (next: typeof arg) => void (a = next),
    destroy: () => {
      node.removeEventListener('dragenter', over);
      node.removeEventListener('dragover', over);
      node.removeEventListener('dragleave', leave);
      node.removeEventListener('drop', drop);
      window.removeEventListener('dragend', off, true);
    },
  };
}
