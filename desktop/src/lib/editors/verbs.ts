// The editor and copy verbs on a checkout (a session's, a worktree's, a
// repo's main one), a folder or a file: the rows the menus list (Open in VS
// Code, Open in Visual Studio, Show in Explorer, Dock editor beside Kete,
// Copy path, Copy branch name, Open in Kete, Open with default app, Copy
// relative path) and what each runs. The state rules are the menu
// matrix's (section 5.1, section 5.5, section 5.7, section 5.8): an editor's row only where it is
// installed (every Code-family install its own row, the default checked);
// Visual Studio only where the checkout has a solution or project; Dock
// greyed out once the window check says no editor is open there; nothing
// opened from a deleted file; never a program handed to its default app.
//
// The tables are pure (`checkoutTable`, `fileTable`, `folderTable` over
// `Facts`), so the tests build facts; the rest reads the editors store and
// calls src-tauri.

import type { AppliesTo } from '../actions.gen';
import type { VerbImpl, VerbTable } from '../actions';
import type { Worktree } from '../api';
import { app, toast } from '../store.svelte';
import { ask } from '../ui/ui.svelte';
import { copyText } from '../menus/clipboard';
import { menuLabel } from '../verbs';
import { editorsApi, type EditorInstall, type OpenAsk, type SolutionRow } from './api';
import { codeInstalls, editors, openWindows, probeWindows, refreshTargets, vsInstalls } from './store.svelte';

/** Why Dock is greyed out (nebula-core's `DOCK_NO_WINDOW`). */
export const DOCK_NO_WINDOW = 'No VS Code or Visual Studio window is open on this checkout';
/** Why Open with default app is greyed out (nebula-core's `RUNS_PROGRAM`). */
export const RUNS_PROGRAM = "Kete doesn't run programs: use Show in Explorer";

/** What the tables decide on. */
export interface Facts {
  /** The installs have been scanned (until then a VS Code row is offered:
   *  unknown is not "no"). */
  loaded: boolean;
  /** Code-family installs, the default first. */
  code: EditorInstall[];
  /** Visual Studio instances, the default first. */
  vs: EditorInstall[];
  /** `editor_folder`. */
  folder: string;
  /** What Visual Studio would open in the checkout; undefined until asked. */
  solutions: SolutionRow[] | undefined;
  /** The checkout has an Open command; undefined until asked. */
  openCommand: boolean | undefined;
  /** Installs with a window open on the checkout; null while unknown. */
  windows: string[] | null;
  /** Dock can move windows here (Windows). */
  dockable: boolean;
}

/** What a checkout opens in by default: the hover icon, the checked row. */
export type FolderDefault = { kind: 'editor'; editor: EditorInstall } | { kind: 'command' };

/** `editor_folder` on a checkout: `command` the Open command (where the
 *  checkout has one); `vscode` VS Code (its default install);
 *  `visual-studio` Visual Studio where the checkout has a solution; `auto`,
 *  or a choice with nothing for it here, VS Code, then Visual Studio, then
 *  the Open command. */
export function folderDefault(f: Pick<Facts, 'code' | 'vs' | 'folder' | 'solutions'> & { openCommand?: boolean }): FolderDefault {
  const code = f.code[0];
  const vs = f.solutions?.length ? f.vs[0] : undefined;
  if (f.folder === 'command' && f.openCommand !== false) return { kind: 'command' };
  if (f.folder === 'visual-studio' && vs) return { kind: 'editor', editor: vs };
  if (code) return { kind: 'editor', editor: code };
  if (vs) return { kind: 'editor', editor: vs };
  return { kind: 'command' };
}

/** `editor_file`: `kete` Kete's viewer, else the editor it names (or VS
 *  Code for `auto`), else Kete's viewer when that one isn't installed.
 *  Visual Studio only where `solutions` says the checkout has one. */
export function fileDefault(f: Pick<Facts, 'code' | 'vs' | 'solutions'> & { file: string }): EditorInstall | 'kete' {
  if (f.file === 'kete') return 'kete';
  const vs = f.solutions?.length ? f.vs[0] : undefined;
  if (f.file === 'visual-studio' && vs) return vs;
  return f.code[0] ?? 'kete';
}

const isDefault = (d: FolderDefault, e: EditorInstall) => d.kind === 'editor' && d.editor.id === e.id;

/** "Open in X" for an install: the registry's whole words for the one
 *  install of a family, its own name where there are several or it isn't
 *  the family's namesake (Cursor). */
function openLabel(e: EditorInstall, all: EditorInstall[], on: AppliesTo, id: 'open_in_vscode' | 'open_in_visual_studio', more = ''): string | undefined {
  const plain = id === 'open_in_vscode' ? e.short === 'VS Code' : all.length === 1;
  return plain && !more ? undefined : `${plain ? menuLabel(id, on) : `Open in ${e.short}`}${more}`;
}

/** What each row runs on a checkout. */
export interface CheckoutActs {
  /** Open the checkout in this Code-family install (null: VS Code's
   *  default, before the scan has answered). */
  code(e: EditorInstall | null): void;
  /** Open its solution in this Visual Studio (a pick when there are
   *  several). */
  vs(e: EditorInstall): void;
  reveal(): void;
  dock(): void;
  copyPath(): void;
  copyBranch(): void;
}

/** The editor and copy rows of a checkout, on a thing of kind `on`. */
export function checkoutTable(f: Facts, on: AppliesTo, act: CheckoutActs): VerbTable {
  const t: VerbTable = {};
  const d = folderDefault(f);
  const [code, ...moreCode] = f.code;
  if (code) {
    t.open_in_vscode = {
      run: () => act.code(code),
      label: openLabel(code, f.code, on, 'open_in_vscode'),
      word: code.short,
      icon: code.icon ?? undefined,
      checked: isDefault(d, code),
      also: moreCode.map((e) => ({ run: () => act.code(e), label: `Open in ${e.short}`, word: e.short, icon: e.icon ?? undefined, checked: isDefault(d, e) })),
    };
  } else if (!f.loaded) {
    t.open_in_vscode = { run: () => act.code(null) };
  }
  const several = (f.solutions?.length ?? 0) > 1 ? '\u2026' : '';
  if (f.vs.length && f.solutions?.length) {
    const [vs, ...moreVs] = f.vs;
    t.open_in_visual_studio = {
      run: () => act.vs(vs),
      label: openLabel(vs, f.vs, on, 'open_in_visual_studio', several),
      word: `${f.vs.length > 1 ? vs.short : 'Visual Studio'}${several}`,
      icon: vs.icon ?? undefined,
      checked: isDefault(d, vs),
      also: moreVs.map((e) => ({
        run: () => act.vs(e),
        label: `Open in ${e.short}${several}`,
        word: `${e.short}${several}`,
        icon: e.icon ?? undefined,
        checked: isDefault(d, e),
      })),
    };
  }
  t.reveal_in_explorer = { run: act.reveal };
  if (f.dockable && (!f.loaded || f.code.length || f.vs.length)) {
    const none = f.windows !== null && !f.windows.length;
    t.dock_editor = { run: act.dock, ...(none ? { disabled: true, why: DOCK_NO_WINDOW } : {}) };
  }
  t.copy_path = { run: act.copyPath };
  t.copy_branch = { run: act.copyBranch };
  return t;
}

/** The checkout's Open command row: left out once the checkout is known
 *  to have none (the matrix's "Open command only when the project or
 *  .kete.json has one"), checked when it is what opens the checkout. */
export function openCommandRow(f: Pick<Facts, 'code' | 'vs' | 'folder' | 'solutions' | 'openCommand'>, run: () => unknown): VerbImpl | undefined {
  if (f.openCommand === false) return undefined;
  return { run, checked: folderDefault(f).kind === 'command' };
}

/** What a file is, as its rows need it. */
export interface FileFacts extends Facts {
  /** `editor_file`: which row is checked as what opens a file. */
  file: string;
  /** Gone from disk (a deleted file in a diff). */
  deleted: boolean;
  /** A program or script (Open with default app refuses it). */
  program: boolean;
  /** In a checkout (it has a relative path). */
  inCheckout: boolean;
}

export interface FileActs {
  view(): void;
  code(e: EditorInstall | null): void;
  vs(e: EditorInstall): void;
  openDefault(): void;
  reveal(): void;
  copyPath(): void;
  copyRelative(): void;
}

/** A file's registry rows (section 5.8). */
export function fileTable(f: FileFacts, act: FileActs): VerbTable {
  const t: VerbTable = {};
  if (!f.deleted) {
    // The row checked is what opens a file by default (`editor_file`).
    const d = fileDefault(f);
    t.open_file_in_kete = { run: act.view, checked: d === 'kete' };
    const [code, ...moreCode] = f.code;
    if (code)
      t.open_in_vscode = {
        run: () => act.code(code),
        label: openLabel(code, f.code, 'file', 'open_in_vscode'),
        word: code.short,
        icon: code.icon ?? undefined,
        checked: d === code,
        also: moreCode.map((e) => ({ run: () => act.code(e), label: `Open in ${e.short}`, word: e.short, icon: e.icon ?? undefined, checked: false })),
      };
    else if (!f.loaded) t.open_in_vscode = { run: () => act.code(null) };
    if (f.vs.length && f.solutions?.length) {
      const vs = f.vs[0];
      t.open_in_visual_studio = { run: () => act.vs(vs), icon: vs.icon ?? undefined, word: 'Visual Studio', checked: d === vs };
    }
    t.open_with_default = f.program ? { run: act.openDefault, disabled: true, why: RUNS_PROGRAM } : { run: act.openDefault };
    t.reveal_in_explorer = { run: act.reveal };
  }
  t.copy_path = { run: act.copyPath };
  if (f.inCheckout) t.copy_relative_path = { run: act.copyRelative };
  return t;
}

export interface FolderActs {
  code(e: EditorInstall | null): void;
  reveal(): void;
  copyPath(): void;
  copyRelative(): void;
}

/** A folder's registry rows (section 5.7). */
export function folderTable(f: Facts, act: FolderActs): VerbTable {
  const t: VerbTable = {};
  const [code, ...moreCode] = f.code;
  if (code)
    t.open_in_vscode = {
      run: () => act.code(code),
      label: openLabel(code, f.code, 'folder', 'open_in_vscode'),
      word: code.short,
      icon: code.icon ?? undefined,
      also: moreCode.map((e) => ({ run: () => act.code(e), label: `Open in ${e.short}`, word: e.short, icon: e.icon ?? undefined })),
    };
  else if (!f.loaded) t.open_in_vscode = { run: () => act.code(null) };
  t.reveal_in_explorer = { run: act.reveal };
  t.copy_path = { run: act.copyPath };
  t.copy_relative_path = { run: act.copyRelative };
  return t;
}

// ---- the store and the calls ----

/** Dock works on this OS (Windows). */
export const DOCKABLE = typeof navigator !== 'undefined' && /Windows/i.test(navigator.userAgent ?? '');

/** The facts for checkout `worktree` from the store (null: no checkout). */
export function factsFor(worktree: string | null): Facts {
  return {
    loaded: editors.loaded,
    code: codeInstalls(),
    vs: vsInstalls(),
    folder: editors.folder,
    solutions: worktree ? editors.solutions[worktree] : undefined,
    openCommand: worktree ? editors.openCommand[worktree] : undefined,
    windows: worktree ? openWindows(worktree) : null,
    dockable: DOCKABLE,
  };
}

/** A menu for a thing in `worktree` is opening: read whether an editor
 *  window is open there, and what Visual Studio would open, for this menu's
 *  next opening and the hover icons. */
export function touch(worktree: string | null | undefined) {
  if (!worktree) return;
  void probeWindows([worktree]);
  refreshTargets([worktree]);
}

const worktreeOf = (id: string): Worktree | undefined => app.model.worktrees.find((w) => w.id === id);

/** Ask src-tauri to open; it says what opened, or why not. */
export async function openWith(a: OpenAsk) {
  try {
    const r = await editorsApi.open(a);
    toast(`Opening ${r.what} in ${r.editor}`);
  } catch (e) {
    toast(`Could not open: ${e}`);
  }
}

const KIND_WORD: Record<SolutionRow['kind'], string> = { slnx: 'solution', sln: 'solution', csproj: 'C# project', vcxproj: 'C++ project' };

/** Open checkout `worktree`'s solution in Visual Studio `e`: the one there
 *  is, or the one picked when there are several (the one last opened in
 *  this repo first). */
export async function openSolution(worktree: string, e: EditorInstall) {
  const all = editors.solutions[worktree] ?? [];
  let solution: string | null = all[0]?.rel ?? null;
  if (all.length > 1) {
    solution = await ask.pick({
      title: `Open in ${e.short}`,
      items: all.map((s, i) => ({ id: s.rel, label: s.rel, hint: i === 0 ? `${KIND_WORD[s.kind]}, first` : KIND_WORD[s.kind] })),
    });
    if (!solution) return;
  }
  await openWith({ worktree, editor: e.id, solution });
}

function failed(what: string) {
  return (e: unknown) => toast(`${what}: ${e}`);
}

/** The editor and copy verbs of checkout `w`, on a thing of kind `on`. */
export function checkoutVerbs(w: Worktree, on: AppliesTo): VerbTable {
  return checkoutTable(factsFor(w.id), on, {
    code: (e) => void openWith({ worktree: w.id, editor: e?.id ?? 'vscode' }),
    vs: (e) => void openSolution(w.id, e),
    reveal: () => void editorsApi.reveal(w.id).catch(failed('Could not show it')),
    dock: () => void dockEditor(w.id),
    copyPath: () => void copyText(w.path, w.path),
    copyBranch: () => void copyText(w.branch, w.branch),
  });
}

/** Dock: the checkout's editor window beside this one. */
export async function dockEditor(worktree: string) {
  try {
    await editorsApi.dock(worktree);
  } catch (e) {
    toast(String(e));
  }
}

/** Open checkout `w` in what opens it by default: the hover icon. `command`
 *  runs the Open command (`runCommand`). */
export function openDefault(w: Worktree, runCommand: () => unknown) {
  const d = folderDefault(factsFor(w.id));
  if (d.kind === 'command') return void runCommand();
  if (d.editor.family === 'visual-studio') return void openSolution(w.id, d.editor);
  return void openWith({ worktree: w.id, editor: d.editor.id });
}

/** `rel` under the checkout at `root`, with its separators. */
export function underRoot(root: string, rel: string): string {
  const sep = root.includes('\\') && !root.includes('/') ? '\\' : '/';
  return root.replace(/[\\/]+$/, '') + sep + rel.split(/[\\/]+/).filter(Boolean).join(sep);
}

/** Program and script extensions Open with default app refuses
 *  (nebula-core's `runs_program`). */
const PROGRAMS = new Set([
  'exe', 'com', 'bat', 'cmd', 'ps1', 'psm1', 'msi', 'msix', 'appx', 'lnk', 'pif', 'scr', 'vbs', 'vbe', 'js', 'jse', 'wsf', 'wsh', 'hta',
  'cpl', 'msc', 'jar', 'reg', 'sh', 'application', 'appref-ms', 'url',
]);

export function runsProgram(path: string): boolean {
  const name = path.split(/[\\/]/).pop() ?? '';
  const dot = name.lastIndexOf('.');
  return dot > 0 && PROGRAMS.has(name.slice(dot + 1).toLowerCase());
}

/** A `file:` or `folder:` row target: `file:<worktree>:<rel>`, or
 *  `file::<absolute path>` for a file outside any checkout (one a session
 *  opened). */
export interface PathTarget {
  worktree: string | null;
  /** Relative to the checkout (`/`-separated), or absolute with no
   *  checkout. */
  path: string;
}

export function parsePathTarget(rest: string): PathTarget | null {
  const i = rest.indexOf(':');
  if (i < 0) return null;
  const worktree = rest.slice(0, i) || null;
  const path = rest.slice(i + 1);
  return path ? { worktree, path } : null;
}

export function pathTargetKey(kind: 'file' | 'folder', t: PathTarget): string {
  return `${kind}:${t.worktree ?? ''}:${t.path}`;
}

/** Open a file in Kete's own viewer: a checkout's through its worktree, any
 *  other one a session opened as it is. */
let viewFile: (t: PathTarget, label: string) => void = () => {};
export function setFileViewer(f: (t: PathTarget, label: string) => void) {
  viewFile = f;
}

/** A file's registry rows. `deleted`: gone from disk (a diff's deleted
 *  file). `line`: the line VS Code goes to (a diff's hunk). */
export function fileVerbs(t: PathTarget, opts: { deleted?: boolean; line?: number | null } = {}): VerbTable {
  const w = t.worktree ? worktreeOf(t.worktree) : undefined;
  const abs = w ? underRoot(w.path, t.path) : t.path;
  const where = w ? { worktree: w.id, path: t.path } : { worktree: null, path: t.path };
  return fileTable(
    { ...factsFor(w?.id ?? null), file: editors.file, deleted: !!opts.deleted, program: runsProgram(t.path), inCheckout: !!w },
    {
      view: () => viewFile(t, t.path.split(/[\\/]/).pop() ?? t.path),
      code: (e) => void openWith({ ...where, editor: e?.id ?? 'vscode', line: opts.line ?? null }),
      vs: (e) => void openWith({ ...where, editor: e.id }),
      openDefault: () => void editorsApi.openDefault(where.worktree, where.path).catch(failed('Could not open it')),
      reveal: () => void editorsApi.reveal(where.worktree, where.path).catch(failed('Could not show it')),
      copyPath: () => void copyText(abs, abs),
      copyRelative: () => void copyText(t.path, t.path),
    },
  );
}

/** A folder's registry rows. */
export function folderVerbs(t: PathTarget): VerbTable {
  const w = t.worktree ? worktreeOf(t.worktree) : undefined;
  const abs = w ? underRoot(w.path, t.path) : t.path;
  const where = w ? { worktree: w.id, path: t.path } : { worktree: null, path: t.path };
  return folderTable(factsFor(w?.id ?? null), {
    code: (e) => void openWith({ ...where, editor: e?.id ?? 'vscode' }),
    reveal: () => void editorsApi.reveal(where.worktree, where.path).catch(failed('Could not show it')),
    copyPath: () => void copyText(abs, abs),
    copyRelative: () => void copyText(t.path, t.path),
  });
}

/** The editor a viewer's "Open this line in VS Code" names: the default
 *  Code-family install (VS Code before the scan answers); null with none
 *  installed. */
export function lineEditor(): string | null {
  const code = codeInstalls()[0];
  if (editors.loaded && !code) return null;
  return code?.short ?? 'VS Code';
}

/** Open the file at `path` (absolute, in a checkout or one a session
 *  opened) at `line` in the default Code-family install. */
export function openLine(path: string, line: number | null) {
  void openWith({ path, line, editor: codeInstalls()[0]?.id ?? 'vscode' });
}
