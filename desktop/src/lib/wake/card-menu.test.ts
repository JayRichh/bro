import { describe, expect, it, vi } from 'vitest';
import type { MenuEntry, MenuItem } from '../ui/ui.svelte';
import { wakeCardItems, type WakeCardState } from './card-menu';
import type { WakeAccount, WakeSchedule } from './types';

const sched = (o: Partial<WakeSchedule> = {}): WakeSchedule => ({ enabled: true, anchor_min: 300, ...o }) as WakeSchedule;
const account = (o: Partial<WakeAccount> = {}): WakeAccount =>
  ({ account: 'work', known: true, disabled: false, logged_in: true, login_hint: null, suggested: null, ...o }) as WakeAccount;
const state = (a: Partial<WakeAccount> = {}, s: Partial<WakeSchedule> = {}, waking = false): WakeCardState => ({ a: account(a), sched: sched(s), waking });

const verbs = () => ({ wakeNow: vi.fn(), setEnabled: vi.fn(), setHour: vi.fn(), copyLogin: vi.fn(), details: vi.fn() });
const isItem = (e: MenuEntry): e is MenuItem => typeof e === 'object' && 'label' in e;
const lines = (es: MenuEntry[]) =>
  es.map((e) => (e === 'sep' ? '---' : 'heading' in e ? `# ${e.heading}` : `${e.checked === undefined ? '' : e.checked ? '[v] ' : '[ ] '}${e.disabled ? `(${e.label})` : e.label}`));
const row = (es: MenuEntry[], label: string) => es.filter(isItem).find((e) => e.label === label)!;

describe("a Whakaoho card's menu", () => {
  it('wakes, switches and opens the account', () => {
    const v = verbs();
    const items = wakeCardItems(state(), v);
    expect(lines(items)).toEqual(['Wake up now', '[v] Wake-up on', '---', 'Account details\u2026']);
    row(items, 'Wake up now').run();
    row(items, 'Wake-up on').run();
    row(items, 'Account details\u2026').run();
    expect(v.wakeNow).toHaveBeenCalled();
    expect(v.setEnabled).toHaveBeenCalledWith(false);
    expect(v.details).toHaveBeenCalled();
  });

  it('offers the suggested hour only where it differs', () => {
    const v = verbs();
    const items = wakeCardItems(state({ suggested: 437 }), v);
    row(items, 'Use the suggested hour, 07:15').run();
    expect(v.setHour).toHaveBeenCalledWith(435);
    expect(lines(wakeCardItems(state({ suggested: 300 }), v))).not.toContain('Use the suggested hour, 05:00');
  });

  it('greys Wake up now with the reason, as the account menu does', () => {
    const v = verbs();
    const out = wakeCardItems(state({ logged_in: false, login_hint: 'kete profile login work' }), v);
    expect(out[0]).toEqual({ heading: 'logged out', ember: true });
    expect(row(out, 'Wake up now')).toMatchObject({ disabled: true, title: 'Log in first: kete profile login work' });
    row(out, 'Copy sign-in command').run();
    expect(v.copyLogin).toHaveBeenCalledWith('kete profile login work');
    expect(row(wakeCardItems(state({ disabled: true }), v), 'Wake up now')).toMatchObject({ disabled: true, title: 'Switched off: switch it on to wake it.' });
    expect(row(wakeCardItems(state({}, {}, true), v), 'Wake up now')).toMatchObject({ disabled: true, title: 'Waking now: up to two minutes.' });
  });

  it('only switches off a schedule left for an account that is gone', () => {
    expect(lines(wakeCardItems(state({ known: false }), verbs()))).toEqual(['# no such account', '[v] Wake-up on']);
    expect(lines(wakeCardItems(state({ known: false }, { enabled: false }), verbs()))).toEqual(['# no such account']);
  });
});
