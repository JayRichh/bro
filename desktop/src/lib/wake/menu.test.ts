import { describe, expect, it, vi } from 'vitest';
import type { AccountRow } from '../api';
import type { MenuEntry, MenuItem } from '../ui/ui.svelte';

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
// The account's full menu: menuFor reaches it once this module has loaded,
// as menus/targets.ts loads it in the window.
await import('../accounts/account-menu');

const row = (key: string, harness: string): AccountRow =>
  ({ key, name: key, harness, profile: true, default: false, disabled: false, useDefault: false, login: 'logged_in', loginHint: null, usage: null }) as AccountRow;

const isItem = (e: MenuEntry): e is MenuItem => typeof e === 'object' && 'label' in e;
const menu = (key: string) => actions.menuFor({ kind: 'account', id: key }).filter(isItem);
const labels = (key: string) => menu(key).map((e) => e.label);
const item = (key: string, label: RegExp | string) => {
  const r = menu(key).find((e) => (typeof label === 'string' ? e.label === label : label.test(e.label)));
  if (!r) throw new Error(`no row ${label} in ${labels(key).join(', ')}`);
  return r;
};

describe("an account's menu", () => {
  it('offers Wake up now and the schedule on a Claude account, after Switch on', async () => {
    app.accounts = [row('work', 'claude'), row('cx', 'codex')];
    const words = labels('work');
    const at = (l: string) => words.indexOf(l);
    expect(at('Switch on')).toBeGreaterThan(-1);
    expect(at('Wake up now')).toBe(at('Switch on') + 1);
    expect(at('Wake-up schedule\u2026')).toBe(at('Wake up now') + 1);
    expect(at('Edit\u2026')).toBeGreaterThan(at('Wake-up schedule\u2026'));
    expect(actions.runVerb({ kind: 'account', id: 'work' }, 'wake_account')).toBe(true);
    await vi.waitFor(() => expect(woke).toEqual(['work']));
    item('work', 'Wake-up schedule\u2026').run();
    expect(nav.current).toMatchObject({ view: 'whakaoho', item: 'work' });
  });

  it('keeps wake-up to Claude accounts and one wake at a time', () => {
    app.accounts = [row('work', 'claude'), row('cx', 'codex')];
    expect(labels('cx')).not.toContain('Wake up now');
    expect(labels('cx')).not.toContain('Wake-up schedule\u2026');
    expect(actions.runVerb({ kind: 'account', id: '@codex' }, 'wake_account')).toBe(false);
    wake.waking['work'] = true;
    expect(item('work', 'Wake up now')).toMatchObject({ disabled: true, title: 'Waking now: up to two minutes.' });
    delete wake.waking['work'];
  });

  it('asks for a login first, waits for the switch, and says when a window is live', () => {
    app.accounts = [{ ...row('work', 'claude'), login: 'needs_login', loginHint: 'kete profile login work' }];
    expect(item('work', 'Wake up now')).toMatchObject({ disabled: true, title: 'Log in first: kete profile login work' });
    expect(actions.runVerb({ kind: 'account', id: 'work' }, 'wake_account')).toBe(false);
    app.accounts = [{ ...row('work', 'claude'), disabled: true }];
    expect(item('work', 'Wake up now')).toMatchObject({ disabled: true, title: 'Switched off: switch it on to wake it.' });
    expect(item('work', 'Wake-up schedule\u2026')).not.toHaveProperty('disabled');
    app.accounts = [row('work', 'claude')];
    const resets = Math.floor(Date.now() / 1000) + 3600;
    app.model = {
      ...app.model,
      accounts: {
        work: {
          account: 'work',
          harness: 'claude',
          windows: [{ kind: 'session', label: null, percent: 30, resets_at: resets, read_at: resets - 600 }],
          source: 'statusline',
          at: Date.now(),
          plan: null,
        },
      },
    };
    const live = item('work', /^Wake up now/);
    expect(live).not.toHaveProperty('disabled');
    expect(live.label).toMatch(/^Wake up now \(window live until \d\d:\d\d\)$/);
  });

  it('round-trips as a target key', () => {
    expect(actions.parseTarget(actions.targetKey({ kind: 'account', id: '@claude' }))).toEqual({ kind: 'account', id: '@claude' });
  });
});
