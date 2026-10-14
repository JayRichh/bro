// `kete open <file>` from inside a session: the daemon's FilesOpened, which
// the terminal app raises as its FILE TABS. The window shows the files in
// a read-only sheet (FileSheet.svelte), one tab per file. The Rust side
// serves only files a session opened (src-tauri/src/files.rs).

import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';

export interface OpenedFile {
  path: string;
  /** Relative to the checkout, cut from the left when long. */
  label: string;
}
export interface FilesOpened {
  root: string;
  files: OpenedFile[];
}
/** A file's text, or why there is none to show. */
export interface FileText {
  text: string | null;
  note: string | null;
}

export const filesApi = {
  read: (path: string) => invoke<FileText>('opened_file_read', { path }),
  openDefault: (path: string) => invoke<void>('opened_file_open', { path }),
  onOpened: (f: (o: FilesOpened) => void) => listen<FilesOpened>('kete://files-opened', (e) => f(e.payload)),
};

export const sheet = $state({
  open: false,
  root: '',
  tabs: [] as OpenedFile[],
  at: 0,
});

/** Raise the sheet on `o`, as the TUI does: the new files replace the
 *  last set, the first one shown. Nothing for an empty set. */
export function showFiles(o: FilesOpened) {
  if (!o.files.length) return;
  sheet.root = o.root;
  sheet.tabs = o.files;
  sheet.at = 0;
  sheet.open = true;
}

export function closeFiles() {
  sheet.open = false;
}

/** Step `d` tabs, wrapping. */
export function stepTab(d: number) {
  const n = sheet.tabs.length;
  if (n) sheet.at = (((sheet.at + d) % n) + n) % n;
}
