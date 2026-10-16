import { describe, expect, it, vi } from 'vitest';
import type { AccountRow } from '../api';

const woke: string[] = [];
vi.mock('../api', () => ({
  api: {
    wakeNow: async (account: string) => {
      woke.push(account);
      return [{ account, decision: 'fire', reason: 'forced', detail: null, mode: 'interactive', report: null }];
    },
    wakeOverview: async () => null,
  },
}));

const { app } = await import('../store.svelte');
const { nav } = await import('../shell/history.svelte');
const actions = await import('../actions');
const { wake } = await import('./state.svelte');

const row = (key: string, harness: string): AccountRow =>
  ({ key, name: key, harness, profile: true, default: false, disabled: false, useDefault: false, login: 'logged_in', loginHint: null, usage: null }) as AccountRow;

const labels = (key: string) =>
  actions.menuFor({ kind: 'account', id: key }).flatMap((e) => (typeof e === 'object' && 'label' in e ? [e.label] : []));

describe("an account's menu", () => {
  it('offers Wake up now and the schedule on a Claude account', async () => {
    app.accounts = [row('work', 'claude'), row('cx', 'codex')];
    expect(labels('work')).toEqual(['Wake up now', 'Wake-up schedule\u2026']);
    expect(labels('@claude')).toEqual(['Wake up now', 'Wake-up schedule\u2026']);
    expect(actions.runVerb({ kind: 'account', id: 'work' }, 'wake_account')).toBe(true);
    await vi.waitFor(() => expect(woke).toEqual(['work']));
    const schedule = actions.menuFor({ kind: 'account', id: 'work' })[1];
    if (typeof schedule === 'object' && 'run' in schedule) schedule.run();
    expect(nav.current).toMatchObject({ view: 'whakaoho', item: 'work' });
  });

  it('keeps wake-up to Claude accounts and one wake at a time', () => {
    app.accounts = [row('cx', 'codex')];
    const menu = actions.menuFor({ kind: 'account', id: 'cx' });
    expect(menu).toHaveLength(1);
    expect(menu[0]).toMatchObject({ label: 'Wake up now', disabled: true });
    expect(actions.runVerb({ kind: 'account', id: '@codex' }, 'wake_account')).toBe(false);
    wake.waking['@claude'] = true;
    expect(actions.menuFor({ kind: 'account', id: '@claude' })[0]).toMatchObject({ disabled: true });
    delete wake.waking['@claude'];
  });

  it('round-trips as a target key', () => {
    expect(actions.parseTarget(actions.targetKey({ kind: 'account', id: '@claude' }))).toEqual({ kind: 'account', id: '@claude' });
  });
});
