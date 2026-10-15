import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { Agent, Model } from './api';

const calls: string[] = [];
let failRestart = false;
vi.mock('./api', () => {
  const rec = (name: string) => async (id: string) => {
    calls.push(`${name} ${id}`);
  };
  return {
    api: {
      archiveAgent: rec('archive'),
      unarchiveAgent: rec('unarchive'),
      deleteAgent: rec('delete'),
      boardScope: async (s: { repo?: string; worktree?: string }) => {
        calls.push(`scope ${s.repo}/${s.worktree}`);
      },
      restartAgent: async (id: string) => {
        calls.push(`restart ${id}`);
        if (failRestart) throw 'gone';
      },
    },
  };
});

const { app } = await import('./store.svelte');
const { ui } = await import('./ui/ui.svelte');
const actions = await import('./actions');

const agent = (id: string, extra: Partial<Agent> = {}): Agent =>
  ({
    id,
    worktree_id: 'w1',
    name: id,
    status: 'finished',
    archived: false,
    unseen: false,
    status_changed_at: 0,
    kind: 'claude',
    ...extra,
  }) as Agent;

function setAgents(agents: Agent[]) {
  app.model = { ...app.model, agents, worktrees: [], projects: [], terminals: [] } as Model;
}

/** Answer the dialog that is up (or wait for one). */
async function answer(ok: boolean) {
  for (let i = 0; i < 50 && !ui.dialog; i++) await Promise.resolve();
  const d = ui.dialog;
  if (d?.kind !== 'confirm') throw new Error('no confirm up');
  d.resolve({ ok, checked: false });
  ui.dialog = null;
}

beforeEach(() => {
  calls.length = 0;
  failRestart = false;
  ui.dialog = null;
  app.toast = null;
});

describe('session verbs', () => {
  it('says a restart is under way only when it was', async () => {
    setAgents([agent('a')]);
    failRestart = true;
    await actions.restartAgent('a');
    expect(app.toast).toContain('Could not restart');
    failRestart = false;
    await actions.restartAgent('a');
    expect(app.toast).toBe('Restarting a.');
  });

  it('unarchive brings back the thread archive took', async () => {
    setAgents([agent('root', { archived: true }), agent('kid', { parent: 'root', archived: true }), agent('other', { archived: true })]);
    await actions.unarchiveAgent('root');
    expect(calls).toEqual(['unarchive root', 'unarchive kid']);
  });

  it('delete takes what is nested under the session when confirmed, not when asked', async () => {
    setAgents([agent('root')]);
    const done = actions.deleteAgent('root');
    // A session spawns under it while the dialog is up.
    setAgents([agent('root'), agent('late', { parent: 'root' })]);
    await answer(true);
    await done;
    expect(calls).toEqual(['delete late', 'delete root']);
  });

  it('archive from a subagent row always asks, and names the parent', async () => {
    app.launch = { ...app.launch, ask_before_archive: false };
    setAgents([agent('root')]);
    const done = actions.archiveAgent('root', true);
    for (let i = 0; i < 50 && !ui.dialog; i++) await Promise.resolve();
    expect(ui.dialog?.kind === 'confirm' && ui.dialog.message).toContain('parent session');
    await answer(false);
    await done;
    expect(calls).toEqual([]);
  });

  it('a subagent row menu says it acts on the parent session', () => {
    setAgents([agent('root')]);
    const menu = actions.menuFor({ kind: 'agent', id: 'root', viaSub: true });
    expect(menu[0]).toEqual({ heading: 'Parent session: root' });
    expect(actions.parseTarget('subagent:root')).toEqual({ kind: 'agent', id: 'root', viaSub: true });
  });
});

describe('menus from the registry', () => {
  const words = (menu: ReturnType<typeof actions.menuFor>) =>
    menu.flatMap((e) => (typeof e === 'object' && 'label' in e ? [e.label] : []));

  it('a session menu says what the terminal app says, in its order, keys shown', () => {
    setAgents([agent('a')]);
    const menu = actions.menuFor({ kind: 'agent', id: 'a' });
    const w = words(menu);
    expect(w.slice(0, 3)).toEqual(['Open', 'Follow-up prompt\u2026', 'Restart agent']);
    expect(w).toContain('Rename\u2026');
    expect(w.indexOf('Archive')).toBeLessThan(w.indexOf('Delete\u2026'));
    const rename = menu.find((e) => typeof e === 'object' && 'label' in e && e.label === 'Rename\u2026');
    expect((rename as { keys?: string[] } | undefined)?.keys).toEqual(['r']);
  });

  it('a terminal closes, a repo is removed from Kete', () => {
    app.model = {
      ...app.model,
      agents: [],
      terminals: [{ id: 't', worktree_id: 'w1', name: 'shell', sort_order: 0, alive: true, run_command: null }],
      worktrees: [{ id: 'w1', project_id: 'p', path: '/p', branch: 'main', is_main: true, sort_order: 0 }],
      projects: [{ id: 'p', name: 'demo', repo_path: '/p', sort_order: 0 }],
    } as unknown as Model;
    expect(words(actions.menuFor({ kind: 'terminal', id: 't' })).slice(0, 3)).toEqual(['Open', 'Rename\u2026', 'Close terminal\u2026']);
    const repo = words(actions.menuFor({ kind: 'project', id: 'p' }));
    expect(repo[0]).toBe('New session\u2026');
    expect(repo).toContain('Remove from Kete\u2026');
    expect(repo.indexOf('Locate folder\u2026')).toBe(repo.indexOf('Remove from Kete\u2026') - 1);
  });

  it('offers the pull request verbs and Focus here under the shared ids, in the terminal app words', () => {
    const base = {
      ...app.model,
      agents: [agent('a')],
      terminals: [],
      worktrees: [{ id: 'w1', project_id: 'p', path: '/p/w1', branch: 'feat', is_main: false, sort_order: 0 }],
      projects: [{ id: 'p', name: 'demo', repo_path: '/p', sort_order: 0 }],
      cards: {},
    } as unknown as Model;
    // No pull request yet: the composer, no page and no merge.
    app.model = { ...base, prs: {} } as Model;
    let w = words(actions.menuFor({ kind: 'agent', id: 'a' }));
    expect(w).toContain('Create pull request\u2026');
    expect(w).not.toContain('Open pull request');
    expect(w).not.toContain('Merge pull request\u2026');
    expect(w).toContain('Focus here');
    // One open: its page and its merge, once each (no window-only extra).
    const pr = { number: 7, url: 'https://x/7', title: 't', state: 'open', draft: false };
    app.model = { ...base, prs: { w1: pr } } as unknown as Model;
    w = words(actions.menuFor({ kind: 'agent', id: 'a' }));
    expect(w).toContain('Open pull request');
    expect(w).not.toContain('Create pull request\u2026');
    expect(w.filter((l) => l.startsWith('Merge')).length).toBe(1);
    expect(w.filter((l) => l === 'Focus here').length).toBe(1);
    expect(w).not.toContain('Merge PR\u2026');
    // Focus here narrows the chip to the checkout.
    expect(actions.runVerb({ kind: 'worktree', id: 'w1' }, 'focus_scope')).toBe(true);
    expect(calls).toEqual(['scope p/w1']);
  });

  it('runs a verb on its target as the menu row does, and refuses one that means nothing there', async () => {
    setAgents([agent('a', { archived: true })]);
    expect(actions.runVerb({ kind: 'agent', id: 'a' }, 'archive')).toBe(false);
    expect(actions.runVerb({ kind: 'agent', id: 'a' }, 'unarchive')).toBe(true);
    await Promise.resolve();
    expect(calls).toEqual(['unarchive a']);
  });
});
