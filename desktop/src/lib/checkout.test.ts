import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { Agent, Model, TerminalTab } from './api';

const calls: string[] = [];
vi.mock('./api', () => ({
  api: {
    deleteAgent: async (id: string) => void calls.push(`delete ${id}`),
    closeTerminal: async (id: string) => void calls.push(`close ${id}`),
  },
}));

const { app } = await import('./store.svelte');
const { ui } = await import('./ui/ui.svelte');
const { checkoutSessions, deleteAllSessions, listing } = await import('./checkout');

const agent = (id: string, worktree: string, extra: Partial<Agent> = {}): Agent =>
  ({ id, worktree_id: worktree, name: id, status: 'finished', archived: false, ...extra }) as Agent;
const term = (id: string, worktree: string, run: string | null = null): TerminalTab => ({
  id,
  worktree_id: worktree,
  name: id,
  sort_order: 0,
  alive: true,
  run_command: run,
});

function setModel(agents: Agent[], terminals: TerminalTab[]) {
  app.model = {
    ...app.model,
    agents,
    terminals,
    projects: [],
    worktrees: [{ id: 'w1', project_id: 'p', path: '/w1', branch: 'fix-login', is_main: false, sort_order: 0 }],
  } as Model;
}

async function answer(ok: boolean) {
  for (let i = 0; i < 50 && !ui.dialog; i++) await Promise.resolve();
  const d = ui.dialog;
  if (d?.kind !== 'confirm') throw new Error('no confirm up');
  d.resolve({ ok, checked: false });
  ui.dialog = null;
}

beforeEach(() => {
  calls.length = 0;
  ui.dialog = null;
  app.toast = null;
  app.session = null;
  app.showArchived = false;
});

describe('delete all sessions in a checkout', () => {
  it('takes the checkout own sessions and terminals, archived ones only when shown', () => {
    setModel(
      [agent('a', 'w1'), agent('old', 'w1', { archived: true }), agent('elsewhere', 'w2')],
      [term('t', 'w1'), term('r', 'w1', 'npm run dev'), term('t2', 'w2')],
    );
    const hidden = checkoutSessions(app.model, 'w1', false);
    expect(hidden.agents.map((a) => a.id)).toEqual(['a']);
    expect(hidden.terminals.map((t) => t.name)).toEqual(['t', 'run']);
    expect(checkoutSessions(app.model, 'w1', true).agents.map((a) => a.id)).toEqual(['a', 'old']);
  });

  it('lists the first eight names and counts the rest', () => {
    const names = Array.from({ length: 11 }, (_, i) => `s${i}`);
    expect(listing(names.slice(0, 8))).toHaveLength(8);
    const long = listing(names);
    expect(long).toHaveLength(9);
    expect(long[8]).toBe('… and 3 more');
  });

  it('deletes only what the confirm listed, and nothing when declined', async () => {
    setModel([agent('a', 'w1'), agent('b', 'w1')], [term('t', 'w1')]);
    app.session = { Agent: 'b' };
    let done = deleteAllSessions('w1');
    await answer(false);
    await done;
    expect(calls).toEqual([]);
    expect(app.session).toEqual({ Agent: 'b' });

    done = deleteAllSessions('w1');
    for (let i = 0; i < 50 && !ui.dialog; i++) await Promise.resolve();
    expect(ui.dialog?.kind === 'confirm' && ui.dialog.detail).toEqual(['a', 'b', 't']);
    // A session started while the dialog is up is not on the list.
    setModel([agent('a', 'w1'), agent('b', 'w1'), agent('late', 'w1')], [term('t', 'w1')]);
    await answer(true);
    await done;
    expect(calls).toEqual(['delete a', 'delete b', 'close t']);
    expect(app.session).toBeNull();
  });

  it('says so when the checkout has no sessions', async () => {
    setModel([agent('x', 'w2')], []);
    await deleteAllSessions('w1');
    expect(ui.dialog).toBeNull();
    expect(app.toast).toBe('No sessions to delete here.');
  });
});
