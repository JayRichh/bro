// The window's transient layers: the context menu, the in-window dialog
// (a prompt, a confirm or a pick list), the command palette and the
// keyboard help. One of each at a time; Esc closes the top one.

import { sheet as files } from '../files/files.svelte';

export type MenuItem = {
  label: string;
  run: () => void;
  /** Chords from the shortcut table, shown at the right. */
  keys?: string[];
  danger?: boolean;
  disabled?: boolean;
};
export type MenuEntry = MenuItem | { heading: string } | 'sep';

export type Field = {
  key: string;
  label: string;
  value?: string;
  placeholder?: string;
  mono?: boolean;
  multiline?: boolean;
};

export type DialogSpec =
  | {
      kind: 'prompt';
      title: string;
      note?: string;
      fields: Field[];
      confirm: string;
      resolve: (v: Record<string, string> | null) => void;
    }
  | {
      kind: 'confirm';
      title: string;
      message: string;
      /** Extra lines (names going away, say). */
      detail?: string[];
      confirm: string;
      danger?: boolean;
      /** An opt-in box (force a delete). */
      check?: string;
      resolve: (v: { ok: boolean; checked: boolean }) => void;
    }
  | {
      kind: 'pick';
      title: string;
      items: { id: string; label: string; hint?: string }[];
      resolve: (id: string | null) => void;
    };

export const ui = $state({
  menu: null as { x: number; y: number; items: MenuEntry[]; label: string } | null,
  dialog: null as DialogSpec | null,
  palette: false,
  help: false,
  /** The review sheet (the orchestrator asked, or a session's Review). */
  review: null as ReviewSheet | null,
  /** The Start orchestrator form. */
  startOrchestrator: false,
});

export type ReviewTab = 'terminal' | 'diff' | 'history' | 'pr';
export type ReviewSheet = { sessions: string[]; at: number; tab: ReviewTab };

/** Open a context menu at the pointer (or under an element). */
export function openMenu(items: MenuEntry[], at: { x: number; y: number } | HTMLElement, label = 'Actions') {
  if (!items.length) return;
  let x: number, y: number;
  if (at instanceof HTMLElement) {
    const r = at.getBoundingClientRect();
    x = r.left;
    y = r.bottom;
  } else ({ x, y } = at);
  ui.menu = { x, y, items, label };
}

export function closeMenu() {
  ui.menu = null;
}

function settle<T>(make: (resolve: (v: T) => void) => DialogSpec): Promise<T> {
  return new Promise<T>((resolve) => {
    ui.dialog = make((v: T) => {
      ui.dialog = null;
      resolve(v);
    });
  });
}

export const ask = {
  prompt: (o: { title: string; note?: string; fields: Field[]; confirm?: string }) =>
    settle<Record<string, string> | null>((resolve) => ({ kind: 'prompt', confirm: 'Save', ...o, resolve })),
  confirm: (o: { title: string; message: string; detail?: string[]; confirm: string; danger?: boolean; check?: string }) =>
    settle<{ ok: boolean; checked: boolean }>((resolve) => ({ kind: 'confirm', ...o, resolve })),
  pick: (o: { title: string; items: { id: string; label: string; hint?: string }[] }) =>
    settle<string | null>((resolve) => ({ kind: 'pick', ...o, resolve })),
};

/** Is any layer up (so page keys stand down)? */
export function layerOpen(): boolean {
  return !!(ui.menu || ui.dialog || ui.palette || ui.help || ui.review || ui.startOrchestrator || files.open);
}
