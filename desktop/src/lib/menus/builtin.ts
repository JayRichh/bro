// The menus the router opens by itself: a text field's, the terminal's, a
// Monaco viewer's, selected text's, and the registry's menu for a
// `[data-target]` row. Each reads its state off the page here, and its
// rows come from items.ts. `installMenus` (App.svelte) registers them and
// starts the router.
//
// Field, terminal and viewer menus keep the focus where it is (`keep`), so
// their verbs act on the focused field or pane directly.

import { app, toast } from '../store.svelte';
import { menuFor, parseTarget } from '../actions';
import type { MenuEntry } from '../ui/ui.svelte';
import { clipboardHasText, copyText, readClipboard } from './clipboard';
import { fieldItems, selectionItems, terminalItems, viewerItems, type FieldState } from './items';
import { installMenuRouter, setBuilder, setRowBuilder, type Opened } from './router';
import { joinPath, terminalAt, viewerAt } from './surfaces';
import { lineEditor, openLine } from '../editors/verbs';

type Field = HTMLInputElement | HTMLTextAreaElement | HTMLElement;

const isControl = (el: Field): el is HTMLInputElement | HTMLTextAreaElement =>
  el instanceof HTMLInputElement || el instanceof HTMLTextAreaElement;

/** A contenteditable's root (the hit can be a span inside it). */
function editRoot(el: HTMLElement): HTMLElement {
  let at = el;
  while (at.parentElement?.isContentEditable) at = at.parentElement;
  return at;
}

/** The field's selected text. An email or number input has no
 *  `selectionStart`; the page's selection holds its text then. */
function selectedText(el: Field): string {
  if (isControl(el) && el.selectionStart != null && el.selectionEnd != null) return el.value.slice(el.selectionStart, el.selectionEnd);
  return document.getSelection()?.toString() ?? '';
}

function canUndo(): boolean {
  try {
    return document.queryCommandEnabled('undo');
  } catch {
    return true;
  }
}

function fieldState(el: Field, clipboard: boolean | null): FieldState {
  const control = isControl(el);
  return {
    editable: control ? !el.readOnly : el.isContentEditable,
    secret: el instanceof HTMLInputElement && el.type === 'password',
    selected: selectedText(el).length > 0,
    empty: control ? el.value.length === 0 : !(el.textContent ?? '').length,
    canUndo: canUndo(),
    clipboard,
  };
}

/** Run an editing command on the focused field; false when the page
 *  refused it. */
function exec(cmd: string, value?: string): boolean {
  try {
    return document.execCommand(cmd, false, value);
  } catch {
    return false;
  }
}

function fieldVerbs(el: Field) {
  const focus = () => document.activeElement !== el && el.focus();
  return {
    undo: () => (focus(), exec('undo')),
    cut: () => {
      focus();
      if (exec('cut')) return;
      const text = selectedText(el);
      void copyText(text).then(() => exec('delete'));
    },
    copy: () => {
      focus();
      if (!exec('copy')) void copyText(selectedText(el));
    },
    paste: async () => {
      const text = await readClipboard();
      if (text == null) return toast('Could not read the clipboard. Ctrl+V pastes.');
      focus();
      // As typed: the field's undo history and its `input` event.
      if (exec('insertText', text) || !isControl(el)) return;
      const start = el.selectionStart ?? el.value.length;
      el.setRangeText(text, start, el.selectionEnd ?? start, 'end');
      el.dispatchEvent(new Event('input', { bubbles: true }));
    },
    selectAll: () => {
      focus();
      if (isControl(el)) el.select();
      else exec('selectAll');
    },
  };
}

/** Paste's row, settled once the clipboard has been read. */
function later(build: (clipboard: boolean | null) => MenuEntry[]): Promise<MenuEntry[] | null> {
  return clipboardHasText().then((has) => (has === false ? build(false) : null));
}

setBuilder('text', (hit): Opened => {
  const raw = hit.el as HTMLElement;
  const el = raw.isContentEditable ? editRoot(raw) : raw;
  // A keyboard open is already in it; a right-click focused it.
  if (document.activeElement !== el) el.focus();
  const verbs = fieldVerbs(el);
  const build = (clipboard: boolean | null) => fieldItems(fieldState(el, clipboard), verbs);
  return { items: build(null), label: 'Edit', keep: true, later: later(build) };
});

setBuilder('selection', () => {
  const text = document.getSelection()?.toString() ?? '';
  return text ? { items: selectionItems(() => void copyText(text)), label: 'Selection', keep: true } : null;
});

setBuilder('terminal', (hit): Opened | null => {
  const t = terminalAt(hit.el as Element);
  if (!t) return null;
  // Read now: the pointer moves off the link on its way to the menu.
  const link = t.link();
  const folder = t.folder();
  const verbs = {
    copy: () => {
      // As Ctrl+C with a selection: copied, then cleared, so the next
      // Ctrl+C interrupts.
      void copyText(t.selection());
      t.clearSelection();
    },
    paste: async () => {
      const text = await readClipboard();
      if (text == null) return toast('Could not read the clipboard. Ctrl+V pastes.');
      t.focus();
      t.paste(text);
    },
    selectAll: () => t.selectAll(),
    clear: () => t.clear(),
    copyFolder: () => folder && void copyText(folder, folder),
    openLink: () => link && t.openLink(link),
    copyLink: () => link && void copyText(link, link),
  };
  const build = (clipboard: boolean | null) =>
    terminalItems({ selected: t.hasSelection(), live: t.live(), clipboard, folder, link }, verbs);
  return { items: build(null), label: 'Terminal', keep: true, later: later(build) };
});

setBuilder('viewer', (hit, at): Opened | null => {
  const el = hit.el as Element;
  const v = viewerAt(el);
  const ed = v?.editorAt(el);
  if (!v || !ed) return null;
  const { editor, old } = ed;
  const model = editor.getModel();
  const path = v.path();
  const line = editor.getTargetAtClientPoint(at.x, at.y)?.position?.lineNumber ?? editor.getPosition()?.lineNumber ?? null;
  const text = () =>
    model
      ? (editor.getSelections() ?? [])
          .filter((s) => !s.isEmpty())
          .map((s) => model.getValueInRange(s))
          .join('\n')
      : '';
  const items = viewerItems(
    { selected: !!text(), path, line, old, editor: lineEditor() },
    {
      copy: () => void copyText(text()),
      selectAll: () => {
        if (!model) return;
        editor.focus();
        editor.setSelection(model.getFullModelRange());
      },
      copyPath: (withLine) => {
        if (!path) return;
        const value = withLine ? `${path}:${line}` : path;
        void copyText(value, value);
      },
      openLine: () => path && openLine(path, line),
    },
  );
  return { items, label: 'Viewer', keep: true };
});

// The registry's rows: the same menu as the row's Menu key (Shift+F10),
// named for the row.
function nameOf(kind: string, id: string): string | undefined {
  const m = app.model;
  if (kind === 'agent' || kind === 'subagent') return m.agents.find((a) => a.id === id)?.name;
  if (kind === 'terminal') return m.terminals.find((t) => t.id === id)?.name;
  if (kind === 'worktree') return m.worktrees.find((w) => w.id === id)?.branch;
  if (kind === 'project') return m.projects.find((p) => p.id === id)?.name;
  return undefined;
}
for (const kind of ['agent', 'subagent', 'terminal', 'worktree', 'project']) {
  setRowBuilder(kind, (id, _el, target) => {
    const t = parseTarget(target);
    return t ? { items: menuFor(t), label: nameOf(kind, id) } : null;
  });
}

/** Start the router with the menus above; returns the stop. */
export function installMenus(): () => void {
  return installMenuRouter(document);
}

