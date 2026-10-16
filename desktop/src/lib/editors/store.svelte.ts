// What the window knows about the user's editors, kept so a menu (built
// synchronously, on the spot) never waits on the disk: the installs and
// their icons (asked once at startup, again on Re-scan), what each checkout
// can be opened with (asked whenever the worktree list changes), and which
// checkouts have an editor window open (asked on hover and as a menu opens,
// never polled, and trusted for a few seconds only: unknown is not "no").

import { untrack } from 'svelte';
import { app } from '../store.svelte';
import { editorsApi, type EditorInstall, type EditorsState, type SolutionRow } from './api';

/** How long a window check counts as known. */
export const WINDOWS_FRESH_MS = 5000;

export const editors = $state({
  /** The first scan has answered. */
  loaded: false,
  list: [] as EditorInstall[],
  /** `editor_folder`: auto, vscode, visual-studio, command. */
  folder: 'auto',
  /** `editor_file`: auto, vscode, visual-studio, kete. */
  file: 'auto',
  reuse: false,
  /** Per worktree id: what Visual Studio would open there (absent: not
   *  asked yet). */
  solutions: {} as Record<string, SolutionRow[]>,
  /** Per worktree id: it has an Open command (absent: not asked yet). */
  openCommand: {} as Record<string, boolean>,
  /** Per worktree id: the installs with a window open on it, and when
   *  that was read. */
  windows: {} as Record<string, { at: number; ids: string[] }>,
});

export function applyEditors(s: EditorsState) {
  editors.list = s.editors;
  editors.folder = s.folder;
  editors.file = s.file;
  editors.reuse = s.reuse;
  editors.loaded = true;
}

export const codeInstalls = (): EditorInstall[] => editors.list.filter((e) => e.family === 'code');
export const vsInstalls = (): EditorInstall[] => editors.list.filter((e) => e.family === 'visual-studio');

/** Ask what `ids` (every worktree by default) can be opened with. */
export function refreshTargets(ids: string[] = app.model.worktrees.map((w) => w.id)) {
  if (!ids.length) return;
  editorsApi
    .targets(ids)
    .then((r) => {
      for (const [id, t] of Object.entries(r)) {
        editors.solutions[id] = t.solutions;
        editors.openCommand[id] = t.open_command;
      }
    })
    .catch(() => {});
}

/** Ask which of `ids` have an editor window open; resolves when known. */
export function probeWindows(ids: string[]): Promise<void> {
  if (!ids.length || (editors.loaded && !editors.list.length)) return Promise.resolve();
  return editorsApi
    .windows(ids)
    .then((r) => {
      const at = Date.now();
      // A checkout the answer leaves out stays unknown.
      for (const id of ids) if (r[id]) editors.windows[id] = { at, ids: r[id] };
    })
    .catch(() => {});
}

/** The installs with a window open on `worktree`, or null while that isn't
 *  known (never asked, or asked too long ago). */
export function openWindows(worktree: string, now = Date.now()): string[] | null {
  const w = editors.windows[worktree];
  return w && now - w.at < WINDOWS_FRESH_MS ? w.ids : null;
}

export async function rescanEditors(): Promise<EditorsState> {
  const s = await editorsApi.rescan();
  applyEditors(s);
  refreshTargets();
  return s;
}

/** Load the installs, then follow the worktree list: what each checkout
 *  opens with is asked again whenever a worktree comes or goes. */
export function startEditors(): () => void {
  editorsApi
    .list()
    .then(applyEditors)
    .catch(() => (editors.loaded = true));
  return $effect.root(() => {
    $effect(() => {
      const ids = app.model.worktrees.map((w) => `${w.id}:${w.path}`).join('|');
      if (ids) untrack(() => refreshTargets());
    });
  });
}
