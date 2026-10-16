// The clipboard for the menus' Copy and Paste rows. Reading asks WebView2,
// which src-tauri's quiet.rs allows for this app's own pages; a read that
// doesn't come back at once (another app holding the clipboard) counts as
// not known, so a menu never waits on it.

import { toast } from '../store.svelte';

/** The clipboard's text: '' when it holds none, null when it can't be read. */
export async function readClipboard(): Promise<string | null> {
  try {
    return await navigator.clipboard.readText();
  } catch {
    return null;
  }
}

/** Whether the clipboard holds text; null when that isn't known within
 *  `ms`. */
export function clipboardHasText(ms = 300): Promise<boolean | null> {
  return Promise.race([
    readClipboard().then((t) => (t === null ? null : t.length > 0)),
    new Promise<null>((r) => setTimeout(() => r(null), ms)),
  ]);
}

/** Put `text` on the clipboard; `said`, a path, is toasted as copied (a
 *  plain Copy says nothing, as the browser's didn't). */
export async function copyText(text: string, said?: string): Promise<void> {
  try {
    await navigator.clipboard.writeText(text);
    if (said) toast(`Copied ${said}`);
  } catch (e) {
    toast(`Could not copy: ${e}`);
  }
}
