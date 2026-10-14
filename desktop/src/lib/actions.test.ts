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
