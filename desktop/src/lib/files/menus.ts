// The Files tree's row menus (menu matrix v2, section 5.7 Folder and 5.8
// File): the registry's rows lane/editors builds for a file and a folder
// (Open in Kete, Open in VS Code and each other install, Visual Studio,
// Open with default app, Show in Explorer, Copy path, Copy relative path),
// then the tree's own: Diff this file where git marks the file changed,
// Expand all and Collapse all where they would do something. Short menus:
// never folded (section 1.4). No Stage or Discard here: the tree reads no
// hunks, and the Diff view's file rows carry them.

import type { MenuEntry } from '../ui/ui.svelte';
import { localRow, registryRows } from '../actions';
import { insertRows, layout } from '../menus/layout';
import { fileTable } from '../menus/targets';
import { folderVerbs, touch, underRoot } from '../editors/verbs';
import { keysOf } from '../shortcuts';
import { app } from '../store.svelte';
import { hasChanges, VIEWER_MAX_BYTES, type Entry } from './tree';

/** Why Open in Kete is greyed on a big file (the matrix's words). */
export const TOO_BIG = "Too big for Kete's viewer: open it in VS Code";

export interface TreeFileVerbs {
  /** The Diff view on the checkout, on this file. */
  diff(): void;
}

/** A file row's menu. */
export function treeFileItems(worktree: string, e: Entry, v: TreeFileVerbs): MenuEntry[] {
  const table = fileTable({ worktree, path: e.path }, { deleted: !!e.deleted });
  if (table.open_file_in_kete && (e.size ?? 0) > VIEWER_MAX_BYTES) table.open_file_in_kete = { ...table.open_file_in_kete, disabled: true, why: TOO_BIG };
  let rows = registryRows('file', table);
  // Beside Open in Kete, ahead of the other apps (the terminal app's place).
  if (hasChanges(e)) rows = insertRows(rows, localRow('file.diff', 'go', { label: 'Diff this file', run: v.diff, keys: keysOf('diff') }));
  return layout(rows, { fold: false });
}

export interface TreeFolderVerbs {
  expandAll(): void;
  collapseAll(): void;
}

/** A folder row's menu: Expand all while something under it is folded
 *  (or not read yet), Collapse all while something under it is open. */
export function treeFolderItems(worktree: string, e: Entry, s: { expand: boolean; collapse: boolean }, v: TreeFolderVerbs): MenuEntry[] {
  touch(worktree);
  const table = folderVerbs({ worktree, path: e.path });
  // The copies say what they copy, as a file's do.
  const w = app.model.worktrees.find((x) => x.id === worktree);
  if (w && table.copy_path) table.copy_path = { ...table.copy_path, note: underRoot(w.path, e.path) };
  if (table.copy_relative_path) table.copy_relative_path = { ...table.copy_relative_path, note: e.path };
  let rows = registryRows('folder', table);
  const fold = [
    ...(s.expand ? [localRow('folder.expand', 'change', { label: 'Expand all', run: v.expandAll })] : []),
    ...(s.collapse ? [localRow('folder.collapse', 'change', { label: 'Collapse all', run: v.collapseAll })] : []),
  ];
  if (fold.length) rows = insertRows(rows, fold);
  return layout(rows, { fold: false });
}
