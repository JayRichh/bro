// The registry's menus for a file and a folder (`file:<worktree>:<rel>`,
// `folder:<worktree>:<rel>`; `file::<path>` for a file a session opened
// outside any checkout), built from their verbs (verbs.ts) in the
// registry's order and words. A surface that lists files puts the target
// on its row and the router (menus/router.ts) opens this menu on a
// right-click. A surface with rows of its own for a file (Diff this file,
// Stage, Mark reviewed) builds its menu from `fileItems` plus those.

import type { MenuEntry } from '../ui/ui.svelte';
import { registryRows } from '../actions';
import { layout } from '../menus/layout';
import { setRowBuilder } from '../menus/router';
import { app } from '../store.svelte';
import { showFiles, showWorktreeFile } from '../files/files.svelte';
import { fileVerbs, folderVerbs, parsePathTarget, setFileViewer, touch, underRoot, type PathTarget } from './verbs';

const nameOf = (p: string) => p.split(/[\\/]/).filter(Boolean).pop() ?? p;

/** A file's registry rows. */
export function fileItems(t: PathTarget, opts: { deleted?: boolean; line?: number | null } = {}): MenuEntry[] {
  touch(t.worktree);
  return layout(registryRows('file', fileVerbs(t, opts)), { fold: false });
}

/** A folder's registry rows. */
export function folderItems(t: PathTarget): MenuEntry[] {
  touch(t.worktree);
  return layout(registryRows('folder', folderVerbs(t)), { fold: false });
}

/** Register the `file:` and `folder:` row menus. A row says a file is gone
 *  from disk with `data-deleted`, and the line to open at with
 *  `data-line`. */
export function installEditorMenus() {
  // Open in Kete: the read-only sheet `kete open` raises, on this file.
  setFileViewer((t, label) => {
    const w = t.worktree ? app.model.worktrees.find((x) => x.id === t.worktree) : undefined;
    if (w) showWorktreeFile(w.path, w.id, t.path, underRoot(w.path, t.path));
    else showFiles({ root: '', files: [{ path: t.path, label }] });
  });
  setRowBuilder('file', (rest, el) => {
    const t = parsePathTarget(rest);
    if (!t) return null;
    const line = Number(el.dataset.line);
    return {
      items: fileItems(t, { deleted: el.dataset.deleted === 'true', line: Number.isFinite(line) && line > 0 ? line : null }),
      label: nameOf(t.path),
    };
  });
  setRowBuilder('folder', (rest) => {
    const t = parsePathTarget(rest);
    return t ? { items: folderItems(t), label: nameOf(t.path) } : null;
  });
}
