// The editor verbs: which rows a checkout, a folder and a file list, in which
// words, checked or greyed out, and what each runs. The tables are pure over
// `Facts`, so most of this builds facts and installs by hand; the last block
// goes through the real menus (`actions.menuFor`) with the editors store
// filled in and src-tauri's calls mocked, so nothing reaches Tauri.

import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { Agent, Model, TerminalTab, Worktree } from '../api';
import type { AppliesTo } from '../actions.gen';
import type { VerbTable } from '../actions';
import type { MenuEntry, MenuItem } from '../ui/ui.svelte';
import type { EditorInstall, Family, SolutionRow } from './api';
import type { Facts, FileFacts } from './verbs';

const mocks = vi.hoisted(() => ({
  windows: vi.fn(() => Promise.resolve({})),
  targets: vi.fn(() => Promise.resolve({})),
  open: vi.fn(() => Promise.resolve({ editor: 'VS Code', what: 'demo' })),
  dock: vi.fn(() => Promise.resolve()),
  reveal: vi.fn(() => Promise.resolve()),
  openDefault: vi.fn(() => Promise.resolve()),
}));
vi.mock('../api', () => ({ api: {} }));
vi.mock('./api', () => ({ editorsApi: mocks }));

// Dock is read off the user agent once, when verbs.ts loads: be Windows
// before it does.
vi.stubGlobal('navigator', { userAgent: 'Mozilla/5.0 (Windows NT 10.0; Win64; x64)' });

const verbs = await import('./verbs');
const { editors } = await import('./store.svelte');
const { app } = await import('../store.svelte');
const actions = await import('../actions');
const { leafItems } = await import('../menus/model');

// ---- fixtures ----

function install(id: string, family: Family, short: string, extra: Partial<EditorInstall> = {}): EditorInstall {
  return {
    id,
    family,
    short,
    name: short,
    version: '1.0.0',
    edition: null,
    year: null,
    prerelease: false,
    exe: `C:\\apps\\${id}.exe`,
    dir: `C:\\apps\\${id}`,
    source: 'test',
    icon: `data:image/png;base64,${id}`,
    ...extra,
  };
}

const vscode = install('code', 'code', 'VS Code');
const cursor = install('cursor', 'code', 'Cursor');
const vs22 = install('vs-2022', 'visual-studio', 'Visual Studio 2022');
const vs19 = install('vs-2019', 'visual-studio', 'Visual Studio 2019');

const sln = (rel = 'Api.sln'): SolutionRow => ({ rel, kind: 'sln' });

function facts(o: Partial<Facts> = {}): Facts {
  return { loaded: true, code: [], vs: [], folder: 'auto', solutions: undefined, openCommand: undefined, windows: null, dockable: true, ...o };
}

function fileFacts(o: Partial<FileFacts> = {}): FileFacts {
  return { ...facts(), file: 'auto', deleted: false, program: false, inCheckout: true, ...o };
}

const checkoutActs = () => ({ code: vi.fn(), vs: vi.fn(), reveal: vi.fn(), dock: vi.fn(), copyPath: vi.fn(), copyBranch: vi.fn() });
const fileActs = () => ({ view: vi.fn(), code: vi.fn(), vs: vi.fn(), openDefault: vi.fn(), reveal: vi.fn(), copyPath: vi.fn(), copyRelative: vi.fn() });
const folderActs = () => ({ code: vi.fn(), reveal: vi.fn(), copyPath: vi.fn(), copyRelative: vi.fn() });

/** A checkout's table over `o`, with its acts to look at. */
function table(o: Partial<Facts> = {}, on: AppliesTo = 'worktree') {
  const act = checkoutActs();
  return { t: verbs.checkoutTable(facts(o), on, act), act };
}

function must<T>(v: T | undefined | null): T {
  if (v === undefined || v === null) throw new Error('expected a value');
  return v;
}

/** The words of a table's rows, in the registry's menu order for `on`. */
const labelsOf = (on: AppliesTo, t: VerbTable) => actions.flatRows(on, t).map((r) => r.label);

// ---- folderDefault ----

describe('folderDefault', () => {
  const def = (o: Partial<Facts>) => verbs.folderDefault(facts(o));

  it('command falls back to the editors where the checkout has no Open command', () => {
    expect(def({ folder: 'command', openCommand: false, code: [vscode] })).toEqual({ kind: 'editor', editor: vscode });
    expect(def({ folder: 'command', openCommand: true, code: [vscode] })).toEqual({ kind: 'command' });
    // Not asked yet: the choice stands.
    expect(def({ folder: 'command', code: [vscode] })).toEqual({ kind: 'command' });
  });

  it('auto opens in the first Code install', () => {
    expect(def({ code: [vscode, cursor], vs: [vs22], solutions: [sln()] })).toEqual({ kind: 'editor', editor: vscode });
  });

  it('auto with no Code install and a solution opens in Visual Studio', () => {
    expect(def({ vs: [vs22, vs19], solutions: [sln()] })).toEqual({ kind: 'editor', editor: vs22 });
  });

  it('auto with no Code install and no solution runs the Open command', () => {
    expect(def({ vs: [vs22], solutions: [] })).toEqual({ kind: 'command' });
    expect(def({ vs: [vs22], solutions: undefined })).toEqual({ kind: 'command' });
    expect(def({})).toEqual({ kind: 'command' });
  });

  it('command runs the Open command even with VS Code installed', () => {
    expect(def({ folder: 'command', code: [vscode], vs: [vs22], solutions: [sln()] })).toEqual({ kind: 'command' });
  });

  it('visual-studio opens Visual Studio where there is a solution and VS Code where there is not', () => {
    const base = { folder: 'visual-studio', code: [vscode], vs: [vs22] };
    expect(def({ ...base, solutions: [sln()] })).toEqual({ kind: 'editor', editor: vs22 });
    expect(def({ ...base, solutions: [] })).toEqual({ kind: 'editor', editor: vscode });
    expect(def({ ...base, solutions: undefined })).toEqual({ kind: 'editor', editor: vscode });
  });

  it('vscode with only Visual Studio installed and a solution still lands on Visual Studio', () => {
    expect(def({ folder: 'vscode', vs: [vs22], solutions: [sln()] })).toEqual({ kind: 'editor', editor: vs22 });
    expect(def({ folder: 'vscode', code: [vscode], vs: [vs22], solutions: [sln()] })).toEqual({ kind: 'editor', editor: vscode });
    expect(def({ folder: 'something-else', code: [vscode] })).toEqual({ kind: 'editor', editor: vscode });
  });
});

// ---- fileDefault ----

describe('fileDefault', () => {
  const file = (choice: string, o: Partial<Pick<Facts, 'code' | 'vs' | 'solutions'>> = {}) =>
    verbs.fileDefault({ code: o.code ?? [], vs: o.vs ?? [], solutions: o.solutions, file: choice });

  it('kete opens in Kete\'s viewer whatever is installed', () => {
    expect(file('kete', { code: [vscode], vs: [vs22], solutions: [sln()] })).toBe('kete');
  });

  it('auto opens in the first Code install, or in Kete when there is none', () => {
    expect(file('auto', { code: [vscode, cursor] })).toEqual(vscode);
    expect(file('auto', {})).toBe('kete');
    expect(file('auto', { vs: [vs22], solutions: [sln()] })).toBe('kete');
  });

  it('visual-studio opens Visual Studio with solutions and the Code install without', () => {
    const base = { code: [vscode], vs: [vs22] };
    expect(file('visual-studio', { ...base, solutions: [sln()] })).toEqual(vs22);
    expect(file('visual-studio', { ...base, solutions: [] })).toEqual(vscode);
    expect(file('visual-studio', { ...base, solutions: undefined })).toEqual(vscode);
  });
});

// ---- checkoutTable ----

describe('checkoutTable', () => {
  it('lists VS Code in the registry words with its icon, checked, and every other Code install as an also row', () => {
    const { t, act } = table({ code: [vscode, cursor] });
    const row = must(t.open_in_vscode);
    expect(row.label).toBeUndefined();
    expect(row.word).toBe('VS Code');
    expect(row.icon).toBe(vscode.icon);
    expect(row.checked).toBe(true);
    expect(row.also).toHaveLength(1);
    const also = must(row.also?.[0]);
    expect(also.label).toBe('Open in Cursor');
    expect(also.word).toBe('Cursor');
    expect(also.icon).toBe(cursor.icon);
    expect(also.checked).toBe(false);
    row.run();
    expect(act.code).toHaveBeenLastCalledWith(vscode);
    also.run();
    expect(act.code).toHaveBeenLastCalledWith(cursor);
    expect(act.code).toHaveBeenCalledTimes(2);
  });

  it('unchecks the Code installs when the folder opens in the Open command', () => {
    const { t } = table({ code: [vscode, cursor], folder: 'command' });
    const row = must(t.open_in_vscode);
    expect(row.checked).toBe(false);
    expect(row.also?.map((a) => a.checked)).toEqual([false]);
  });

  it('turns the row\'s label to Open in Cursor when Cursor is the only Code install', () => {
    const { t, act } = table({ code: [cursor] });
    const row = must(t.open_in_vscode);
    expect(row.label).toBe('Open in Cursor');
    expect(row.word).toBe('Cursor');
    expect(row.icon).toBe(cursor.icon);
    expect(row.checked).toBe(true);
    expect(row.also).toEqual([]);
    row.run();
    expect(act.code).toHaveBeenCalledWith(cursor);
  });

  it('makes Cursor the default row and VS Code the also row when Cursor comes first', () => {
    const { t } = table({ code: [cursor, vscode] });
    const row = must(t.open_in_vscode);
    expect(row.label).toBe('Open in Cursor');
    expect(row.checked).toBe(true);
    const also = must(row.also?.[0]);
    expect(also.label).toBe('Open in VS Code');
    expect(also.word).toBe('VS Code');
    expect(also.checked).toBe(false);
  });

  it('before the scan answers offers VS Code running the default, no Visual Studio, and Dock', () => {
    const { t, act } = table({ loaded: false, solutions: [sln()] });
    const row = must(t.open_in_vscode);
    expect(row.label).toBeUndefined();
    expect(row.word).toBeUndefined();
    expect(row.icon).toBeUndefined();
    row.run();
    expect(act.code).toHaveBeenCalledWith(null);
    expect(t.open_in_visual_studio).toBeUndefined();
    const dock = must(t.dock_editor);
    expect(dock.disabled).toBeUndefined();
  });

  it('after a scan that found nothing drops the editor rows and Dock but keeps Explorer and the copies', () => {
    const { t, act } = table({ loaded: true });
    expect(t.open_in_vscode).toBeUndefined();
    expect(t.open_in_visual_studio).toBeUndefined();
    expect(t.dock_editor).toBeUndefined();
    expect(Object.keys(t)).toEqual(['reveal_in_explorer', 'copy_path', 'copy_branch']);
    must(t.reveal_in_explorer).run();
    must(t.copy_path).run();
    must(t.copy_branch).run();
    expect(act.reveal).toHaveBeenCalledOnce();
    expect(act.copyPath).toHaveBeenCalledOnce();
    expect(act.copyBranch).toHaveBeenCalledOnce();
  });

  it('leaves Visual Studio out while the checkout\'s solutions are unknown or empty', () => {
    const base = { code: [vscode], vs: [vs22] };
    expect(table({ ...base, solutions: undefined }).t.open_in_visual_studio).toBeUndefined();
    expect(table({ ...base, solutions: [] }).t.open_in_visual_studio).toBeUndefined();
    expect(table({ solutions: [sln()] }).t.open_in_visual_studio).toBeUndefined();
  });

  it('offers Visual Studio in the registry words with one solution', () => {
    const { t, act } = table({ code: [vscode], vs: [vs22], solutions: [sln()] });
    const row = must(t.open_in_visual_studio);
    expect(row.label).toBeUndefined();
    expect(row.word).toBe('Visual Studio');
    expect(row.icon).toBe(vs22.icon);
    expect(row.checked).toBe(false);
    expect(row.also).toEqual([]);
    row.run();
    expect(act.vs).toHaveBeenCalledWith(vs22);
  });

  it('ends the Visual Studio label and word with an ellipsis when there are several solutions to pick from', () => {
    const { t } = table({ code: [vscode], vs: [vs22], solutions: [sln('A.sln'), sln('B.sln')] });
    const row = must(t.open_in_visual_studio);
    expect(row.label).toBe('Open in Visual Studio\u2026');
    expect(row.word).toBe('Visual Studio\u2026');
    expect(labelsOf('worktree', t)).toContain('Open in Visual Studio\u2026');
  });

  it('names the year on the default Visual Studio row when there are two installs, the other an also row', () => {
    const { t, act } = table({ code: [vscode], vs: [vs22, vs19], solutions: [sln()] });
    const row = must(t.open_in_visual_studio);
    expect(row.label).toBe('Open in Visual Studio 2022');
    expect(row.word).toBe('Visual Studio 2022');
    const also = must(row.also?.[0]);
    expect(row.also).toHaveLength(1);
    expect(also.label).toBe('Open in Visual Studio 2019');
    expect(also.word).toBe('Visual Studio 2019');
    expect(also.icon).toBe(vs19.icon);
    row.run();
    expect(act.vs).toHaveBeenLastCalledWith(vs22);
    also.run();
    expect(act.vs).toHaveBeenLastCalledWith(vs19);
  });

  it('carries the ellipsis onto every Visual Studio row when there are two installs and several solutions', () => {
    const { t } = table({ vs: [vs22, vs19], solutions: [sln('A.sln'), sln('B.sln')] });
    const row = must(t.open_in_visual_studio);
    expect(row.label).toBe('Open in Visual Studio 2022\u2026');
    expect(row.word).toBe('Visual Studio 2022\u2026');
    expect(row.also?.map((a) => a.label)).toEqual(['Open in Visual Studio 2019\u2026']);
    expect(row.also?.map((a) => a.word)).toEqual(['Visual Studio 2019\u2026']);
  });

  it('checks Visual Studio only when the folder opens in it and the checkout has a solution', () => {
    const base = { code: [vscode], vs: [vs22], solutions: [sln()] };
    const auto = table({ ...base, folder: 'auto' }).t;
    expect(must(auto.open_in_visual_studio).checked).toBe(false);
    expect(must(auto.open_in_vscode).checked).toBe(true);

    const picked = table({ ...base, folder: 'visual-studio' }).t;
    expect(must(picked.open_in_visual_studio).checked).toBe(true);
    expect(must(picked.open_in_vscode).checked).toBe(false);

    const noSolution = table({ ...base, solutions: [], folder: 'visual-studio' }).t;
    expect(noSolution.open_in_visual_studio).toBeUndefined();
    expect(must(noSolution.open_in_vscode).checked).toBe(true);
  });

  it('checks the solution\'s Visual Studio under auto when no Code install is there to be the default', () => {
    const { t } = table({ vs: [vs22], solutions: [sln()] });
    expect(t.open_in_vscode).toBeUndefined();
    expect(must(t.open_in_visual_studio).checked).toBe(true);
  });

  it('enables Dock while the window check has not answered', () => {
    const { t, act } = table({ code: [vscode], windows: null });
    const dock = must(t.dock_editor);
    expect(dock.disabled).toBeUndefined();
    expect(dock.why).toBeUndefined();
    dock.run();
    expect(act.dock).toHaveBeenCalledOnce();
  });

  it('greys Dock out with the reason once no editor window is open on the checkout', () => {
    const { t } = table({ code: [vscode], windows: [] });
    const dock = must(t.dock_editor);
    expect(dock.disabled).toBe(true);
    expect(dock.why).toBe(verbs.DOCK_NO_WINDOW);
    expect(dock.why).toBe('No VS Code or Visual Studio window is open on this checkout');
    // The menu row says why as its tooltip.
    const row = actions.flatRows('worktree', t).find((r) => r.label === 'Dock editor beside Kete');
    expect(row?.disabled).toBe(true);
    expect(row?.title).toBe(verbs.DOCK_NO_WINDOW);
  });

  it('enables Dock when a window lists an install', () => {
    const { t } = table({ code: [vscode], windows: ['code'] });
    const dock = must(t.dock_editor);
    expect(dock.disabled).toBeUndefined();
    expect(dock.why).toBeUndefined();
  });

  it('offers Dock for any installed editor, solution or not, and leaves it out where Dock cannot work', () => {
    expect(table({ vs: [vs22], solutions: [] }).t.dock_editor).toBeDefined();
    expect(table({ code: [vscode], dockable: false }).t.dock_editor).toBeUndefined();
    expect(table({ loaded: false, dockable: false }).t.dock_editor).toBeUndefined();
  });

  it('puts the rows in the registry\'s menu order in the registry\'s words', () => {
    const { t } = table({ code: [vscode, cursor], vs: [vs22], solutions: [sln()] });
    expect(labelsOf('worktree', t)).toEqual([
      'Open in VS Code',
      'Open in Cursor',
      'Open in Visual Studio',
      'Show in Explorer',
      'Dock editor beside Kete',
      'Copy path',
      'Copy branch name',
    ]);
  });
});

// ---- openCommandRow ----

describe('openCommandRow', () => {
  it('is left out once the checkout is known to have no Open command', () => {
    expect(verbs.openCommandRow(facts({ openCommand: false, code: [vscode] }), vi.fn())).toBeUndefined();
  });

  it('is offered while unknown or known to exist, and runs what it is given', () => {
    const run = vi.fn();
    for (const openCommand of [undefined, true]) {
      const row = must(verbs.openCommandRow(facts({ openCommand, code: [vscode] }), run));
      row.run();
    }
    expect(run).toHaveBeenCalledTimes(2);
  });

  it('is unchecked while an editor opens the checkout', () => {
    const row = must(verbs.openCommandRow(facts({ openCommand: true, code: [vscode] }), vi.fn()));
    expect(row.checked).toBe(false);
  });

  it('is checked only when the Open command is what opens the checkout', () => {
    expect(must(verbs.openCommandRow(facts({ openCommand: true, folder: 'command', code: [vscode] }), vi.fn())).checked).toBe(true);
    // Nothing installed, so the Open command is the fallback.
    expect(must(verbs.openCommandRow(facts({ openCommand: undefined }), vi.fn())).checked).toBe(true);
    expect(must(verbs.openCommandRow(facts({ openCommand: true, vs: [vs22], solutions: [sln()] }), vi.fn())).checked).toBe(false);
  });
});

// ---- fileTable ----

describe('fileTable', () => {
  it('checks the row that opens a file by default', () => {
    const checked = (o: Partial<FileFacts>) =>
      Object.entries(verbs.fileTable(fileFacts({ code: [vscode, cursor], vs: [vs22], solutions: [sln()], ...o }), fileActs()))
        .filter(([, v]) => v?.checked)
        .map(([id]) => id);
    expect(checked({ file: 'auto' })).toEqual(['open_in_vscode']);
    expect(checked({ file: 'kete' })).toEqual(['open_file_in_kete']);
    expect(checked({ file: 'visual-studio' })).toEqual(['open_in_visual_studio']);
    // Visual Studio with no solution here: VS Code.
    expect(checked({ file: 'visual-studio', solutions: [] })).toEqual(['open_in_vscode']);
    // Nothing installed: Kete's own viewer.
    expect(checked({ file: 'auto', code: [], vs: [] })).toEqual(['open_file_in_kete']);
  });

  it('lists a normal file\'s rows in the registry\'s file menu order', () => {
    const t = verbs.fileTable(fileFacts({ code: [vscode, cursor], vs: [vs22], solutions: [sln()] }), fileActs());
    expect(labelsOf('file', t)).toEqual([
      'Open in Kete',
      'Open in VS Code',
      'Open in Cursor',
      'Open in Visual Studio',
      'Open with default app',
      'Show in Explorer',
      'Copy path',
      'Copy relative path',
    ]);
  });

  it('runs Open in Kete in the viewer', () => {
    const act = fileActs();
    must(verbs.fileTable(fileFacts(), act).open_file_in_kete).run();
    expect(act.view).toHaveBeenCalledOnce();
  });

  it('offers a deleted file only its copies, the relative one inside a checkout', () => {
    const base = { deleted: true, code: [vscode], vs: [vs22], solutions: [sln()] };
    expect(Object.keys(verbs.fileTable(fileFacts({ ...base, inCheckout: true }), fileActs()))).toEqual(['copy_path', 'copy_relative_path']);
    expect(Object.keys(verbs.fileTable(fileFacts({ ...base, inCheckout: false }), fileActs()))).toEqual(['copy_path']);
  });

  it('greys Open with default app out for a program or script, and says why', () => {
    expect(verbs.RUNS_PROGRAM).toBe("Kete doesn't run programs: use Show in Explorer");
    for (const name of ['setup.exe', 'run.ps1', 'a.lnk']) {
      const act = fileActs();
      const row = must(verbs.fileTable(fileFacts({ program: verbs.runsProgram(name) }), act).open_with_default);
      expect(row.disabled, name).toBe(true);
      expect(row.why, name).toBe(verbs.RUNS_PROGRAM);
    }
  });

  it('enables Open with default app for a normal file and runs it', () => {
    const act = fileActs();
    const row = must(verbs.fileTable(fileFacts({ program: verbs.runsProgram('notes.md') }), act).open_with_default);
    expect(row.disabled).toBeUndefined();
    expect(row.why).toBeUndefined();
    row.run();
    expect(act.openDefault).toHaveBeenCalledOnce();
  });

  it('offers Copy relative path only inside a checkout', () => {
    const act = fileActs();
    expect(verbs.fileTable(fileFacts({ inCheckout: false }), act).copy_relative_path).toBeUndefined();
    const row = must(verbs.fileTable(fileFacts({ inCheckout: true }), act).copy_relative_path);
    row.run();
    expect(act.copyRelative).toHaveBeenCalledOnce();
  });

  it('offers Open in Visual Studio only with Visual Studio installed and a solution in the checkout', () => {
    const vsRow = (o: Partial<FileFacts>) => verbs.fileTable(fileFacts(o), fileActs()).open_in_visual_studio;
    expect(vsRow({ vs: [vs22], solutions: undefined })).toBeUndefined();
    expect(vsRow({ vs: [vs22], solutions: [] })).toBeUndefined();
    expect(vsRow({ vs: [], solutions: [sln()] })).toBeUndefined();
    const act = fileActs();
    const row = must(verbs.fileTable(fileFacts({ vs: [vs22], solutions: [sln()] }), act).open_in_visual_studio);
    expect(row.word).toBe('Visual Studio');
    expect(row.icon).toBe(vs22.icon);
    row.run();
    expect(act.vs).toHaveBeenCalledWith(vs22);
  });

  it('offers VS Code on a file with every Code install, and the default one before the scan answers', () => {
    const act = fileActs();
    const row = must(verbs.fileTable(fileFacts({ code: [vscode, cursor] }), act).open_in_vscode);
    expect(row.label).toBeUndefined();
    expect(row.word).toBe('VS Code');
    expect(row.also?.map((a) => a.label)).toEqual(['Open in Cursor']);
    row.run();
    must(row.also?.[0]).run();
    expect(act.code.mock.calls).toEqual([[vscode], [cursor]]);

    const early = fileActs();
    must(verbs.fileTable(fileFacts({ loaded: false }), early).open_in_vscode).run();
    expect(early.code).toHaveBeenCalledWith(null);
    expect(verbs.fileTable(fileFacts({ loaded: true }), fileActs()).open_in_vscode).toBeUndefined();
  });
});

// ---- folderTable ----

describe('folderTable', () => {
  it('lists VS Code with its also rows, Explorer and both copies, and nothing else', () => {
    const act = folderActs();
    const t = verbs.folderTable(facts({ code: [vscode, cursor], vs: [vs22], solutions: [sln()] }), act);
    expect(Object.keys(t)).toEqual(['open_in_vscode', 'reveal_in_explorer', 'copy_path', 'copy_relative_path']);
    expect(labelsOf('folder', t)).toEqual(['Open in VS Code', 'Open in Cursor', 'Show in Explorer', 'Copy path', 'Copy relative path']);
    const row = must(t.open_in_vscode);
    expect(row.word).toBe('VS Code');
    expect(row.icon).toBe(vscode.icon);
    row.run();
    must(row.also?.[0]).run();
    must(t.reveal_in_explorer).run();
    must(t.copy_path).run();
    must(t.copy_relative_path).run();
    expect(act.code.mock.calls).toEqual([[vscode], [cursor]]);
    expect(act.reveal).toHaveBeenCalledOnce();
    expect(act.copyPath).toHaveBeenCalledOnce();
    expect(act.copyRelative).toHaveBeenCalledOnce();
  });

  it('turns the VS Code row to Cursor when Cursor is the only Code install', () => {
    const t = verbs.folderTable(facts({ code: [cursor] }), folderActs());
    expect(must(t.open_in_vscode).label).toBe('Open in Cursor');
  });

  it('offers VS Code running the default before the scan answers, and none after a scan that found nothing', () => {
    const act = folderActs();
    must(verbs.folderTable(facts({ loaded: false }), act).open_in_vscode).run();
    expect(act.code).toHaveBeenCalledWith(null);
    const after = verbs.folderTable(facts({ loaded: true }), folderActs());
    expect(after.open_in_vscode).toBeUndefined();
    expect(Object.keys(after)).toEqual(['reveal_in_explorer', 'copy_path', 'copy_relative_path']);
  });
});

// ---- parsePathTarget and pathTargetKey ----

describe('parsePathTarget', () => {
  it('splits a checkout\'s file at the first colon', () => {
    expect(verbs.parsePathTarget('w1:src/a.ts')).toEqual({ worktree: 'w1', path: 'src/a.ts' });
  });

  it('reads a leading colon as no checkout and keeps the path\'s own colon', () => {
    expect(verbs.parsePathTarget(':C:\\code\\a.ts')).toEqual({ worktree: null, path: 'C:\\code\\a.ts' });
    expect(verbs.parsePathTarget('w1:C:\\code\\a.ts')).toEqual({ worktree: 'w1', path: 'C:\\code\\a.ts' });
  });

  it('is null with no colon or an empty path', () => {
    expect(verbs.parsePathTarget('src/a.ts')).toBeNull();
    expect(verbs.parsePathTarget('')).toBeNull();
    expect(verbs.parsePathTarget('w1:')).toBeNull();
    expect(verbs.parsePathTarget(':')).toBeNull();
  });

  it('round trips through pathTargetKey', () => {
    const inCheckout = must(verbs.parsePathTarget('w1:src/a.ts'));
    expect(verbs.pathTargetKey('file', inCheckout)).toBe('file:w1:src/a.ts');
    expect(verbs.pathTargetKey('folder', inCheckout)).toBe('folder:w1:src/a.ts');
    const outside = must(verbs.parsePathTarget(':C:\\code\\a.ts'));
    expect(verbs.pathTargetKey('file', outside)).toBe('file::C:\\code\\a.ts');
    // The router hands over what follows the kind and its colon.
    expect(verbs.parsePathTarget(verbs.pathTargetKey('file', outside).slice('file:'.length))).toEqual(outside);
    expect(verbs.parsePathTarget(verbs.pathTargetKey('file', inCheckout).slice('file:'.length))).toEqual(inCheckout);
  });
});

// ---- runsProgram ----

describe('runsProgram', () => {
  it('refuses programs and scripts by extension, in any case', () => {
    for (const name of ['setup.exe', 'RUN.BAT', 'run.ps1', 'a.lnk', 'deploy.sh', 'site.url', 'C:\\tools\\Setup.EXE', 'bin/run.cmd', 'a.js']) {
      expect(verbs.runsProgram(name), name).toBe(true);
    }
  });

  it('lets other files through, including dotfiles, bare names and compound extensions', () => {
    for (const name of ['a.ts', 'data.json', 'README.md', '.bashrc', 'Makefile', 'archive.tar.gz', 'file.', 'src/lib/']) {
      expect(verbs.runsProgram(name), name).toBe(false);
    }
  });

  it('reads the extension off the last path part, not off a folder with a dot in its name', () => {
    expect(verbs.runsProgram('C:\\a.b\\file')).toBe(false);
    expect(verbs.runsProgram('tools.exe/readme')).toBe(false);
    expect(verbs.runsProgram('tools.exe/run.exe')).toBe(true);
  });
});

// ---- underRoot ----

describe('underRoot', () => {
  it('does not double a separator the relative path starts with', () => {
    expect(verbs.underRoot('C:\\a', '/b')).toBe('C:\\a\\b');
    expect(verbs.underRoot('/home/me', '//b/c')).toBe('/home/me/b/c');
  });

  it('keeps backslashes under a Windows root', () => {
    expect(verbs.underRoot('C:\\code\\app', 'src/a.ts')).toBe('C:\\code\\app\\src\\a.ts');
    expect(verbs.underRoot('C:\\code\\app', 'a.ts')).toBe('C:\\code\\app\\a.ts');
  });

  it('does not double a trailing separator', () => {
    expect(verbs.underRoot('C:\\code\\app\\', 'src/a.ts')).toBe('C:\\code\\app\\src\\a.ts');
    expect(verbs.underRoot('C:\\code\\app\\\\', 'src/a.ts')).toBe('C:\\code\\app\\src\\a.ts');
    expect(verbs.underRoot('/home/me/app/', 'src/a.ts')).toBe('/home/me/app/src/a.ts');
  });

  it('keeps forward slashes under a posix root, and under a Windows root written with them', () => {
    expect(verbs.underRoot('/home/me/app', 'src/a.ts')).toBe('/home/me/app/src/a.ts');
    expect(verbs.underRoot('/home/me/app', 'src\\a.ts')).toBe('/home/me/app/src/a.ts');
    expect(verbs.underRoot('C:/code/app', 'src/a.ts')).toBe('C:/code/app/src/a.ts');
  });
});

// ---- through the real menus ----

describe('the registry menus with the editor rows', () => {
  const main: Worktree = { id: 'w1', project_id: 'p', path: '/p', branch: 'main', is_main: true, sort_order: 0 };
  const agent = (id: string, extra: Partial<Agent> = {}): Agent =>
    ({ id, worktree_id: 'w1', name: id, status: 'finished', archived: false, unseen: false, status_changed_at: 0, kind: 'claude', ...extra }) as Agent;
  const term = (id: string): TerminalTab => ({ id, worktree_id: 'w1', name: id, sort_order: 0, alive: true, run_command: null });

  function setModel(o: { agents?: Agent[]; terminals?: TerminalTab[] } = {}) {
    app.model = {
      ...app.model,
      projects: [{ id: 'p', name: 'demo', repo_path: '/p', sort_order: 0 }],
      worktrees: [main],
      agents: o.agents ?? [],
      terminals: o.terminals ?? [],
      cards: {},
      prs: {},
    } as unknown as Model;
  }

  function loadEditors(o: { list?: EditorInstall[]; solutions?: Record<string, SolutionRow[]>; openCommand?: Record<string, boolean> } = {}) {
    editors.loaded = true;
    editors.list = o.list ?? [vscode, vs22];
    editors.solutions = o.solutions ?? { w1: [sln()] };
    editors.openCommand = o.openCommand ?? { w1: false };
  }

  // Every row in its whole words, a submenu's (Open in \u25b8, Copy \u25b8) in
  // its place: the menu matrix folds the editors and the copies.
  const whole = (i: MenuItem) => i.full ?? i.label;
  const words = (menu: MenuEntry[]) => leafItems(menu).map(whole);
  const rowNamed = (menu: MenuEntry[], label: string): MenuItem | undefined => leafItems(menu).find((i) => whole(i) === label);

  const EDITOR_ROWS = ['Open in VS Code', 'Open in Visual Studio', 'Show in Explorer', 'Copy path', 'Copy branch name'];

  beforeEach(() => {
    editors.loaded = false;
    editors.list = [];
    editors.folder = 'auto';
    editors.file = 'auto';
    editors.reuse = false;
    editors.solutions = {};
    editors.openCommand = {};
    editors.windows = {};
    for (const m of Object.values(mocks)) m.mockClear();
    app.toast = null;
    setModel();
  });

  it('gives a live session the editor and copy rows once, and an archived session none', () => {
    setModel({ agents: [agent('a'), agent('old', { archived: true })] });
    loadEditors();
    const live = words(actions.menuFor({ kind: 'agent', id: 'a' }));
    for (const w of EDITOR_ROWS) expect(live, w).toContain(w);
    // The checkout's own rows do not repeat what the session offered.
    expect(live.filter((w) => w === 'Open in VS Code')).toHaveLength(1);
    const archived = words(actions.menuFor({ kind: 'agent', id: 'old' }));
    for (const w of EDITOR_ROWS) expect(archived, w).not.toContain(w);
  });

  it('gives a worktree the same rows plus Dock, and the Open command only where the checkout has one', () => {
    setModel({ agents: [agent('a')] });
    expect(verbs.DOCKABLE).toBe(true);
    loadEditors({ openCommand: { w1: false } });
    const menu = () => words(actions.menuFor({ kind: 'worktree', id: 'w1' }));
    for (const w of [...EDITOR_ROWS, 'Dock editor beside Kete']) expect(menu(), w).toContain(w);
    expect(menu()).not.toContain('Open in editor');
    editors.openCommand = { w1: true };
    expect(menu()).toContain('Open in editor');
    // Not asked yet is not "no".
    editors.openCommand = {};
    expect(menu()).toContain('Open in editor');
  });

  it('gives the repo VS Code and Copy path but no branch name to copy', () => {
    setModel({ agents: [agent('a')] });
    loadEditors();
    const repo = words(actions.menuFor({ kind: 'project', id: 'p' }));
    expect(repo).toContain('Open in VS Code');
    expect(repo).toContain('Open in Visual Studio');
    expect(repo).toContain('Copy path');
    expect(repo).not.toContain('Copy branch name');
  });

  it('starts a terminal\'s menu with Open and Copy path', () => {
    setModel({ terminals: [term('t')] });
    loadEditors();
    expect(words(actions.menuFor({ kind: 'terminal', id: 't' })).slice(0, 2)).toEqual(['Open', 'Copy path']);
  });

  it('offers no Visual Studio anywhere when the checkout has no solution, known empty or not asked yet', () => {
    setModel({ agents: [agent('a')], terminals: [term('t')] });
    const cases: Record<string, SolutionRow[]>[] = [{ w1: [] }, {}];
    for (const solutions of cases) {
      loadEditors({ solutions });
      const menus = [
        actions.menuFor({ kind: 'agent', id: 'a' }),
        actions.menuFor({ kind: 'worktree', id: 'w1' }),
        actions.menuFor({ kind: 'project', id: 'p' }),
        actions.menuFor({ kind: 'terminal', id: 't' }),
      ];
      for (const menu of menus) expect(words(menu).some((w) => w.startsWith('Open in Visual Studio'))).toBe(false);
    }
  });

  it('carries the editor\'s icon, its word inside Open in and its checked state on the VS Code row', () => {
    setModel({ agents: [agent('a')] });
    loadEditors();
    const menu = actions.menuFor({ kind: 'worktree', id: 'w1' });
    const openIn = must(menu.find((e): e is MenuItem => typeof e === 'object' && 'label' in e && e.label === 'Open in'));
    expect(leafItems(openIn.children ?? []).map((i) => i.label)).toEqual(expect.arrayContaining(['VS Code', 'Visual Studio']));
    const kid = (label: string) => must((openIn.children ?? []).find((e): e is MenuItem => typeof e === 'object' && 'label' in e && e.label === label));
    const row = kid('VS Code');
    expect(row.full).toBe('Open in VS Code');
    expect(row.icon).toEqual(vscode.icon ? { src: vscode.icon } : undefined);
    expect(row.checked).toBe(true);
    const vs = kid('Visual Studio');
    expect(vs.full).toBe('Open in Visual Studio');
    expect(vs.icon).toEqual(vs22.icon ? { src: vs22.icon } : undefined);
    expect(vs.checked).toBe(false);
  });

  it('asks src-tauri to open that install when a row runs', () => {
    setModel({ agents: [agent('a')] });
    loadEditors();
    const menu = actions.menuFor({ kind: 'worktree', id: 'w1' });
    must(rowNamed(menu, 'Open in VS Code')).run();
    expect(mocks.open).toHaveBeenLastCalledWith({ worktree: 'w1', editor: 'code' });
    must(rowNamed(menu, 'Open in Visual Studio')).run();
    expect(mocks.open).toHaveBeenLastCalledWith({ worktree: 'w1', editor: 'vs-2022', solution: 'Api.sln' });
  });

  it('still offers VS Code before the first scan has answered, and no editor rows after a scan that found none', () => {
    setModel({ agents: [agent('a')] });
    editors.loaded = false;
    const early = words(actions.menuFor({ kind: 'worktree', id: 'w1' }));
    expect(early).toContain('Open in VS Code');
    expect(early).not.toContain('Open in Visual Studio');
    expect(early).toContain('Dock editor beside Kete');

    loadEditors({ list: [] });
    const none = words(actions.menuFor({ kind: 'worktree', id: 'w1' }));
    expect(none).not.toContain('Open in VS Code');
    expect(none).not.toContain('Open in Visual Studio');
    expect(none).not.toContain('Dock editor beside Kete');
    for (const w of ['Show in Explorer', 'Copy path', 'Copy branch name']) expect(none, w).toContain(w);
  });
});
