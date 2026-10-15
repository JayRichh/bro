// The window's transient layers: the context menu, the in-window dialog
// (a prompt, a confirm or a pick list), the command palette and the
// keyboard help. One of each at a time; Esc closes the top one.

import { sheet as files } from '../files/files.svelte';
import { addSheet } from '../accounts/add.svelte';
import { shipSheet } from '../ship/state.svelte';

export type MenuItem = {
  label: string;
  run: () => void;
  /** Chords from the shortcut table, shown at the right. */
  keys?: string[];
  danger?: boolean;
  disabled?: boolean;
  /** Kept last whatever a searchable menu's query (the verbs menu's All
   *  keys). */
  pinned?: boolean;
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
      /** The box starts ticked. */
      checked?: boolean;
      resolve: (v: { ok: boolean; checked: boolean }) => void;
    }
  | {
      kind: 'pick';
      title: string;
      items: { id: string; label: string; hint?: string }[];
      resolve: (id: string | null) => void;
    };

export const ui = $state({
  /** `search`: a field over the rows narrows them as you type (the
   *  verbs menu, `?`). */
  menu: null as { x: number; y: number; items: MenuEntry[]; label: string; search?: boolean } | null,
  dialog: null as DialogSpec | null,
  palette: false,
  help: false,
  /** The review sheet (the orchestrator asked, or a session's Review). */
  review: null as ReviewSheet | null,
  /** The Start orchestrator form. */
  startOrchestrator: false,
  /** The branch switcher (worktrees/BranchSwitch.svelte). */
  branchSwitch: false,
});

export type ReviewTab = 'terminal' | 'diff' | 'history' | 'pr';
export type ReviewSheet = { sessions: string[]; at: number; tab: ReviewTab };

/** Open a context menu at the pointer (or under an element). */
export function openMenu(items: MenuEntry[], at: { x: number; y: number } | HTMLElement, label = 'Actions', opts: { search?: boolean } = {}) {
  if (!items.length) return;
  let x: number, y: number;
  if (at instanceof HTMLElement) {
    const r = at.getBoundingClientRect();
    x = r.left;
    y = r.bottom;
  } else ({ x, y } = at);
  ui.menu = { x, y, items, label, ...(opts.search ? { search: true } : {}) };
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
  confirm: (o: { title: string; message: string; detail?: string[]; confirm: string; danger?: boolean; check?: string; checked?: boolean }) =>
    settle<{ ok: boolean; checked: boolean }>((resolve) => ({ kind: 'confirm', ...o, resolve })),
  pick: (o: { title: string; items: { id: string; label: string; hint?: string }[] }) =>
    settle<string | null>((resolve) => ({ kind: 'pick', ...o, resolve })),
};

/** Is any layer up (so page keys stand down)? */
export function layerOpen(): boolean {
  return !!(
    ui.menu ||
    ui.dialog ||
    ui.palette ||
    ui.help ||
    ui.review ||
    ui.startOrchestrator ||
    ui.branchSwitch ||
    files.open ||
    addSheet.phase === 'form' ||
    !!shipSheet.mode
  );
}
