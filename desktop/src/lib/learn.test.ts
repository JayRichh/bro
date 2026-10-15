import { beforeEach, describe, expect, it, vi } from 'vitest';

const calls: { seen: string[]; tips: boolean[] } = { seen: [], tips: [] };
let state = { showTips: true, seen: [] as string[], whatsNew: false, version: '0.52' };
vi.mock('./api', () => ({
  api: {
    learnState: async () => state,
    learnSeen: async (id: string) => void calls.seen.push(id),
    learnTips: async (on: boolean) => void calls.tips.push(on),
  },
}));

import { EMPTY, TIPS } from './actions.gen';
import { LANE_EMPTY, emptyCopy, fill, offer, tipDue, tipText, type TipState } from './learn';
import { gotIt, learn, loadLearn, offerTip, setShowTips, tipsOff } from './learn.svelte';
import { RARE, SHORTCUTS, TASKS, taskGroups, type Shortcut } from './shortcuts';

const base: TipState = { showTips: true, seen: [], tip: null, whatsNew: false, loaded: true };

describe('first-use tips', () => {
  it('shows a tip once, then never again', () => {
    const up = offer(base, 'verbs');
    expect(up?.tip).toBe('verbs');
    expect(up?.seen).toEqual(['verbs']);
    expect(tipDue({ ...up!, tip: null }, 'verbs')).toBe(false);
    expect(tipDue({ ...up!, tip: null }, 'scope')).toBe(true);
  });

  it('holds one tip at a time, and the one that waits is not marked seen', () => {
    const up = offer(base, 'lanes')!;
    expect(offer(up, 'drag')).toBeNull();
    expect(up.seen).not.toContain('drag');
    expect(offer({ ...up, tip: null }, 'drag')?.tip).toBe('drag');
  });

  it('shows nothing with tips off, over What s new, or before the state is read', () => {
    expect(offer({ ...base, showTips: false }, 'verbs')).toBeNull();
    expect(offer({ ...base, whatsNew: true }, 'verbs')).toBeNull();
    expect(offer({ ...base, loaded: false }, 'verbs')).toBeNull();
    expect(offer(base, 'no-such-tip')).toBeNull();
  });

  it('names the bound key, or the words when the verb has none', () => {
    expect(fill('{verbs|The verbs menu} lists it.', () => '?')).toBe('? lists it.');
    expect(fill('{verbs|The verbs menu} lists it.', () => null)).toBe('The verbs menu lists it.');
    expect(fill('{d} then', () => null)).toBe('d then');
    for (const t of TIPS) expect(tipText(t.id, () => '?')).not.toMatch(/[{}|]/);
    expect(tipText('nope', () => '?')).toBe('');
  });
});

describe('the window s tip state', () => {
  beforeEach(() => {
    calls.seen = [];
    calls.tips = [];
    state = { showTips: true, seen: [], whatsNew: false, version: '0.52' };
    Object.assign(learn, { showTips: true, seen: [], tip: null, whatsNew: false, loaded: false });
  });

  it('opens What s new when the config says it is due, and only then', async () => {
    state.whatsNew = true;
    await loadLearn();
    expect(learn.whatsNew).toBe(true);
    learn.whatsNew = false;
    state.whatsNew = false;
    await loadLearn();
    expect(learn.whatsNew).toBe(false);
  });

  it('writes a shown tip to the shared config, and remembers the other app s', async () => {
    state.seen = ['scope'];
    await loadLearn();
    offerTip('scope');
    expect(learn.tip).toBeNull();
    offerTip('verbs');
    expect(learn.tip).toBe('verbs');
    expect(calls.seen).toEqual(['verbs']);
    gotIt();
    expect(learn.tip).toBeNull();
    offerTip('verbs');
    expect(learn.tip).toBeNull();
  });

  it("Don't show tips turns them off in the config", async () => {
    await loadLearn();
    offerTip('drag');
    tipsOff();
    expect(learn.tip).toBeNull();
    expect(calls.tips).toEqual([false]);
    offerTip('lanes');
    expect(learn.tip).toBeNull();
    setShowTips(true);
    offerTip('lanes');
    expect(learn.tip).toBe('lanes');
  });
});

describe('empty states', () => {
  it('has a line for every board lane and the views', () => {
    for (const id of Object.values(LANE_EMPTY)) expect(emptyCopy(id).text).not.toBe('');
    for (const id of ['board', 'overview', 'no_repos', 'no_sessions', 'accounts', 'search']) expect(emptyCopy(id).text).not.toBe('');
    expect(emptyCopy('lane_running').verb).toBe('quick_prompt');
    expect(EMPTY.every((e) => e.text.length < 100)).toBe(true);
  });
});

describe('help by task', () => {
  const table = (): Shortcut[] => SHORTCUTS.map((s) => ({ ...s, keys: [...s.keys] }));

  it('groups the keys by task, in order', () => {
    const groups = taskGroups(table());
    expect(groups.map((g) => g.group)).toEqual([...TASKS]);
  });

  it('keeps rare and unbound keys behind Show all', () => {
    const t = table();
    const everyday = taskGroups(t).flatMap((g) => g.rows.map((r) => r.label));
    const all = taskGroups(t, true).flatMap((g) => g.rows.map((r) => r.label));
    expect(all.length).toBeGreaterThan(everyday.length);
    expect(everyday).toContain('Repo 1 to 9 on the rail');
    expect(everyday).not.toContain('Repo 2 on the rail');
    expect(all).toContain('Repo 2 on the rail');
    for (const id of RARE) {
      const label = t.find((s) => s.id === id)?.label;
      if (label && !t.some((s) => s.label === label && !RARE.has(s.id) && s.keys.length)) expect(everyday).not.toContain(label);
    }
    // An unbound row (Restart ships with no key) waits behind Show all.
    expect(everyday).not.toContain('Restart the agent');
    expect(all).toContain('Restart the agent');
  });

  it('follows a rebind: a key the user unbinds leaves the everyday list', () => {
    const t = table();
    t.find((s) => s.id === 'archive')!.keys = [];
    expect(taskGroups(t).flatMap((g) => g.rows.map((r) => r.label))).not.toContain('Archive');
  });

  it('puts the review row and the diff under Review & ship', () => {
    const review = taskGroups(table()).find((g) => g.group === 'Review & ship')!;
    const labels = review.rows.map((r) => r.label).join('\n');
    expect(labels).toMatch(/merge/i);
    expect(labels).toMatch(/Diff/);
  });
});
