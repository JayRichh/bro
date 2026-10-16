// The window's other targets' menus read off a model (menus/targets.ts):
// a pull request chip, a port chip, a lane header, an attention count, the
// orchestrator, a view crumb, a diff file row, an opened file's tab, and
// the background through the router's `none`.

import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { Agent, Model } from '../api';

vi.mock('../api', () => ({
  api: {
    boardScope: vi.fn(() => Promise.resolve()),
    markSeen: vi.fn(() => Promise.resolve()),
    openUrl: vi.fn(() => Promise.resolve()),
    stopRun: vi.fn(() => Promise.resolve()),
    startRun: vi.fn(() => Promise.resolve()),
  },
}));
// No DOM here: openMenu tells an element from a point with instanceof.
(globalThis as { HTMLElement?: unknown }).HTMLElement ??= class {};

const { api } = await import('../api');
const { app } = await import('../store.svelte');
const { ui, closeMenu } = await import('../ui/ui.svelte');
const t = await import('./targets');
const { openHit } = await import('./router');
const { notices } = await import('../orch.svelte');
import type { MenuEntry, MenuItem } from '../ui/ui.svelte';

const labels = (es: readonly MenuEntry[] | undefined) => (es ?? []).map((e) => (e === 'sep' ? '---' : 'heading' in e ? `# ${e.heading}` : e.children ? `${e.label} >` : e.label));
const row = (es: readonly MenuEntry[] | undefined, label: string) =>
  (es ?? []).find((e): e is MenuItem => typeof e === 'object' && 'run' in e && e.label === label);

const agent = (id: string, extra: Partial<Agent> = {}): Agent =>
  ({ id, worktree_id: 'w1', name: id, status: 'finished', archived: false, unseen: false, alive: true, status_changed_at: 0, kind: 'claude', role: 'worker', parent: null, ...extra }) as Agent;

function model(extra: Partial<Model> = {}): Model {
  return {
    ...app.model,
    connected: true,
    protocol_version: 62,
    projects: [{ id: 'p', name: 'kete', repo_path: 'C:\\code\\kete', sort_order: 0 }],
    worktrees: [
      { id: 'w0', project_id: 'p', path: 'C:\\code\\kete', branch: 'main', is_main: true, sort_order: 0 },
      { id: 'w1', project_id: 'p', path: 'C:\\code\\kete-x', branch: 'feat-x', is_main: false, sort_order: 1 },
    ],
    agents: [agent('a', { status: 'running', status_changed_at: 5 }), agent('b', { unseen: true, status_changed_at: 9 }), agent('c')],
    terminals: [{ id: 't1', worktree_id: 'w1', name: 'run', sort_order: 0, alive: true, run_command: 'npm run dev' }],
    ports: [{ worktree: 'w1', block: null, live: [5173] }],
    prs: { w1: { number: 42, url: 'https://github.com/o/r/pull/42', title: 'Fix', draft: false, state: 'open' } },
    subagents: [],
    cards: {},
    lanes: { needs: [], running: ['a'], unseen: ['b'], done: ['c'], pr: [] },
    hidden: { needs: 0, running: 0, unseen: 0, done: 0, pr: 0 },
    scope: {},
    ...extra,
  } as Model;
}

beforeEach(() => {
  app.model = model();
  app.runCommands = { w1: 'npm run dev' };
  app.adoCtx = {};
  notices.list = [];
  closeMenu();
  vi.mocked(api.openUrl).mockClear();
});

describe('a pull request chip', () => {
  it('opens it, goes to its sessions and checkout, copies it, merges and refreshes', () => {
    const o = t.githubPrMenu('w1');
    expect(o?.label).toBe('PR #42');
    // Nine rows and more: the three Copy rows fold (a local menu folds
    // from eight).
    expect(labels(o?.items)).toEqual([
      'Open in browser',
      '---',
      'Go to session b',
      'Go to session a',
      'Go to session c',
      'Go to worktree feat-x',
      '---',
      'Copy >',
      '---',
      'Merge\u2026',
      'Refresh pull requests',
    ]);
    row(o?.items, 'Open in browser')!.run();
    expect(api.openUrl).toHaveBeenCalledWith('https://github.com/o/r/pull/42');
  });

  it('is gone with its pull request', () => {
    app.model = model({ prs: {} });
    expect(t.githubPrMenu('w1')).toBeNull();
  });
});

describe('a port chip', () => {
  it('opens and copies the URL, shows and stops its run', () => {
    const o = t.portMenu('w1:5173');
    expect(o?.label).toBe('localhost:5173');
    expect(labels(o?.items)).toEqual(['Open localhost:5173 in browser', '---', 'Show run terminal', '---', 'Copy URL', '---', 'Stop run']);
    row(o?.items, 'Stop run')!.run();
    expect(api.stopRun).toHaveBeenCalledWith('w1');
  });

  it('has no run rows for a checkout with no run command and nothing running', () => {
    app.model = model({ terminals: [] });
    app.runCommands = {};
    expect(labels(t.portMenu('w1:5173')?.items)).toEqual(['Open localhost:5173 in browser', '---', 'Copy URL']);
  });
});

describe('a lane header', () => {
  it('gives each lane its verb', () => {
    expect(labels(t.laneMenu('unseen')?.items)).toEqual(['Mark all seen']);
    expect(labels(t.laneMenu('done')?.items)).toEqual(['Archive done sessions\u2026']);
    expect(labels(t.laneMenu('pr')?.items)).toEqual(['Refresh pull requests']);
    const needs = t.laneMenu('needs');
    expect(row(needs?.items, 'Open next needing you')).toMatchObject({ disabled: true, title: 'Nothing needs you' });
    expect(t.laneMenu('nope')).toBeNull();
  });

  it('marks the Review lane seen', () => {
    row(t.laneMenu('unseen')?.items, 'Mark all seen')!.run();
    expect(api.markSeen).toHaveBeenCalledWith('b');
  });
});

describe('an attention count', () => {
  it('lists its sessions, the last changed first, and ends unread with Mark all seen', () => {
    app.model = model({ agents: [agent('b', { unseen: true, status_changed_at: 1 }), agent('d', { unseen: true, status_changed_at: 7 })] });
    const o = t.attentionMenu('unread');
    expect(labels(o?.items)).toEqual(['d', 'b', '---', 'Mark all seen']);
    expect(row(o?.items, 'd')?.title).toBe('kete / feat-x');
  });

  it('has no menu with nothing counted', () => {
    expect(t.attentionMenu('needs')).toBeNull();
  });
});

describe('the orchestrator', () => {
  it('offers to start one when none is pinned', () => {
    expect(labels(t.orchestratorMenu(true).items)).toEqual(['Start an orchestrator\u2026']);
  });

  it("opens it, reviews what it asked for, carries its session's rows flat, and unpins", () => {
    app.model = model({ agents: [agent('o', { role: 'orchestrator', worktree_id: 'w0', name: 'boss' }), agent('a')] });
    notices.list = [{ id: 1, sessions: ['a'], tab: 'terminal' }];
    const items = t.orchestratorMenu(true).items;
    expect(labels(items)).toEqual(['Open orchestrator', 'Review waiting (1)\u2026', '---', 'Session boss >', '---', 'Start a new orchestrator\u2026', 'Unpin orchestrator']);
    const session = row(items, 'Session boss')!.children!;
    // Flat: no submenu inside, no checkout rows, no second Unpin.
    expect(session.some((e) => typeof e === 'object' && 'children' in e && !!e.children)).toBe(false);
    expect(labels(session)).toContain('Open');
    expect(labels(session).some((l) => l.startsWith('Checkout'))).toBe(false);
    expect(labels(session)).not.toContain('Unpin orchestrator');
    // The panel's background: no session rows, no unpin.
    expect(labels(t.orchestratorMenu(false).items)).toEqual(['Open orchestrator', 'Review all waiting (1)\u2026', '---', 'Start a new orchestrator\u2026']);
  });
});

describe('a view crumb', () => {
  it("lists the background's views, and an Azure DevOps repo's own", async () => {
    const { nav } = await import('../shell/history.svelte');
    nav.go({ view: 'worktrees', repo: 'p' });
    app.adoCtx = { p: { org: 'o' } as never };
    const views = labels(t.viewMenu());
    expect(views).toContain('Overview');
    expect(views).toContain('Worktrees');
    expect(views.slice(-3)).toEqual(['Tasks', 'Branches', 'Pull requests']);
    nav.go({ view: 'overview' });
  });
});

describe('files', () => {
  it("copies a diff file's path on disk and in its checkout, and stages where the diff reads the working tree", () => {
    const verbs = { stage: vi.fn(), unstage: vi.fn(), discard: vi.fn() };
    const o = t.diffFileMenu('w1', { path: 'src/a.ts', status: 'M' }, { unstaged: true, staged: true }, verbs);
    expect(o?.label).toBe('src/a.ts');
    expect(labels(o?.items)).toEqual(['Copy path', 'Copy relative path', '---', 'Stage file', 'Unstage file', '---', 'Discard changes\u2026']);
    expect(row(o?.items, 'Copy path')?.title).toBe('C:\\code\\kete-x\\src\\a.ts');
    // Read against the base: no staging.
    expect(labels(t.diffFileMenu('w1', { path: 'src/a.ts', status: 'M' }, null, verbs)?.items)).toEqual(['Copy path', 'Copy relative path']);
  });

  it("gives an opened file's tab its path and its own app", () => {
    const open = vi.fn();
    const o = t.openedFileMenu('C:\\code\\kete\\README.md', 'C:\\code\\kete', open);
    expect(labels(o.items)).toEqual(['Open with default app', '---', 'Copy path', 'Copy relative path']);
    expect(row(o.items, 'Copy relative path')?.title).toBe('README.md');
    row(o.items, 'Open with default app')!.run();
    expect(open).toHaveBeenCalled();
  });
});

describe('the background', () => {
  it('opens on empty space, not inside a dialog', () => {
    const space = { closest: () => null };
    expect(openHit({ kind: 'none', el: space as never }, { x: 1, y: 2 })).toBe(true);
    expect(ui.menu?.label).toBe('Kete');
    closeMenu();
    const inDialog = { closest: (sel: string) => (sel.includes('dialog') ? {} : null) };
    expect(openHit({ kind: 'none', el: inDialog as never }, { x: 1, y: 2 })).toBe(false);
    expect(ui.menu).toBeNull();
  });
});
