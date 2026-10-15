import { describe, expect, it } from 'vitest';
import type { Agent, Model, PrInfo, TerminalTab, Worktree } from '../api';
import type { CheckoutState, WorktreeGit } from './api';
import {
  firstDir,
  gitSignal,
  groupOf,
  orderGroups,
  prLabel,
  pruneKeys,
  repoKey,
  setKey,
  sortRows,
  sweepCandidates,
  sweepReason,
  tableRow,
  worktreeGroups,
  worktreeKey,
} from './groups';

const wt = (id: string, extra: Partial<Worktree> = {}): Worktree => ({
  id,
  project_id: 'p',
  path: `C:/r/${id}`,
  branch: id,
  is_main: false,
  sort_order: 0,
  ...extra,
});
const agent = (id: string, worktree_id: string, extra: Partial<Agent> = {}): Agent =>
  ({
    id,
    worktree_id,
    name: id,
    status: 'finished',
    archived: false,
    unseen: false,
    status_changed_at: 0,
    sort_order: 0,
    parent: null,
    recent_prompts: [],
    ...extra,
  }) as Agent;
const term = (id: string, worktree_id: string, extra: Partial<TerminalTab> = {}): TerminalTab => ({
  id,
  worktree_id,
  name: id,
  sort_order: 0,
  alive: true,
  run_command: null,
  ...extra,
});
const pr = (number: number, state: PrInfo['state'], draft = false): PrInfo => ({ number, url: '', title: '', draft, state });
const model = (m: Partial<Model>): Model =>
  ({ projects: [{ id: 'p', name: 'p', repo_path: '', sort_order: 0 }], worktrees: [], agents: [], terminals: [], ports: [], prs: {}, subagents: [], ...m }) as Model;
const ids = (gs: { worktree: Worktree }[]) => gs.map((g) => g.worktree.id);

describe('worktree groups', () => {
  const m = model({
    worktrees: [wt('main', { is_main: true, sort_order: 0 }), wt('old', { sort_order: 1 }), wt('hot', { sort_order: 2 }), wt('idle', { sort_order: 3 })],
    agents: [agent('a', 'old', { status_changed_at: 100 }), agent('b', 'hot', { status_changed_at: 900 }), agent('c', 'main', { status_changed_at: 50 })],
    terminals: [term('t', 'idle', { run_command: 'npm run dev' })],
    ports: [{ worktree: 'idle', block: null, live: [5173, 3000, 5173] }],
    prs: { hot: pr(7, 'open') },
  });

  it('puts the root first, then the most recently worked in', () => {
    expect(ids(worktreeGroups(m, 'p', { showAll: false }))).toEqual(['main', 'hot', 'old', 'idle']);
  });

  it('keeps a checkout never worked in at its place in the repo order', () => {
    const quiet = model({ worktrees: [wt('main', { is_main: true }), wt('b', { sort_order: 2 }), wt('a', { sort_order: 1 })] });
    expect(ids(orderGroups(quiet.worktrees.map((w) => groupOf(quiet, w))))).toEqual(['main', 'a', 'b']);
  });

  it('shows an empty checkout only with Show all worktrees', () => {
    const withEmpty = model({ ...m, worktrees: [...m.worktrees, wt('empty', { sort_order: 4 })] });
    expect(ids(worktreeGroups(withEmpty, 'p', { showAll: false }))).not.toContain('empty');
    expect(ids(worktreeGroups(withEmpty, 'p', { showAll: true }))).toContain('empty');
  });

  it("never shows the root's empty group alone", () => {
    const bare = model({ worktrees: [wt('main', { is_main: true })] });
    expect(worktreeGroups(bare, 'p', { showAll: true })).toEqual([]);
    // With a linked checkout beside it, both show.
    const two = model({ worktrees: [wt('main', { is_main: true }), wt('x')] });
    expect(ids(worktreeGroups(two, 'p', { showAll: true }))).toEqual(['main', 'x']);
  });

  it('lists archived sessions only when asked, but counts them as no live card', () => {
    const arch = model({ worktrees: [wt('main', { is_main: true }), wt('x')], agents: [agent('z', 'x', { archived: true })] });
    expect(ids(worktreeGroups(arch, 'p'))).toEqual([]);
    const g = worktreeGroups(arch, 'p', { archived: true });
    expect(ids(g)).toEqual(['x']);
    expect(g[0]).toMatchObject({ sessions: 1, live: 0 });
  });

  it('carries the pull request, ports and run state', () => {
    const idle = groupOf(m, m.worktrees[3]);
    expect(idle).toMatchObject({ ports: [3000, 5173], running: true, live: 1, sessions: 0 });
    expect(groupOf(m, m.worktrees[2]).pr?.number).toBe(7);
  });

  it('marks a session started from another checkout instead of hiding it', () => {
    const mm = model({ worktrees: [wt('main', { is_main: true }), wt('x')], agents: [agent('parent', 'main'), agent('kid', 'x', { parent: 'parent' })] });
    const [root] = groupOf(mm, mm.worktrees[1]).forest;
    expect(root.from?.id).toBe('parent');
  });

  it('names a pull request by its state', () => {
    expect(prLabel(pr(3, 'open'))).toBe('PR #3 open');
    expect(prLabel(pr(3, 'open', true))).toBe('PR #3 draft');
    expect(prLabel(pr(3, 'merged'))).toBe('PR #3 merged');
    expect(prLabel({ number: 3, url: '', title: '', draft: false })).toBe('PR #3 open');
  });

  it('signals a git re-read when a checkout comes or a session moves', () => {
    const s = gitSignal(m);
    expect(gitSignal(model({ ...m }))).toBe(s);
    expect(gitSignal(model({ ...m, worktrees: [...m.worktrees, wt('new')] }))).not.toBe(s);
    expect(gitSignal(model({ ...m, agents: [...m.agents, agent('d', 'main', { status_changed_at: 1000 })] }))).not.toBe(s);
  });
});

describe('folding', () => {
  it('adds and removes a key once', () => {
    let keys = setKey([], worktreeKey('w'), true);
    keys = setKey(keys, worktreeKey('w'), true);
    expect(keys).toEqual(['wt:w']);
    keys = setKey(keys, repoKey('p'), true);
    expect(setKey(keys, 'wt:w', false)).toEqual(['repo:p']);
  });

  it('drops folds of repos and checkouts that are gone', () => {
    const m = model({ worktrees: [wt('w')] });
    expect(pruneKeys(['wt:w', 'wt:gone', 'repo:p', 'repo:gone'], m)).toEqual(['wt:w', 'repo:p']);
  });
});

describe('the Worktrees table', () => {
  const m = model({
    worktrees: [wt('main', { is_main: true }), wt('b-two'), wt('a-one'), wt('c-three')],
    agents: [agent('x', 'a-one', { status_changed_at: 5_000 }), agent('y', 'c-three', { status_changed_at: 1_000 })],
    prs: { 'b-two': pr(1, 'merged'), 'a-one': pr(2, 'open') },
  });
  const git: Record<string, WorktreeGit> = {
    main: { changed: 0, untracked: 0, upstream: 'origin/main', ahead: 0, behind: 2, committed: 1 },
    'b-two': { changed: 4, untracked: 1, upstream: null, ahead: null, behind: null, committed: 9 },
    'a-one': { changed: 1, untracked: 0, upstream: 'origin/a-one', ahead: 3, behind: 0, committed: 2 },
  };
  const table = m.worktrees.map((w) => tableRow(groupOf(m, w), git[w.id]));

  it('reads changes and last activity from git and the sessions', () => {
    const b = table.find((r) => r.worktree.id === 'b-two')!;
    expect(b.changes).toBe(5);
    expect(b.last).toBe(9_000);
    expect(table.find((r) => r.worktree.id === 'c-three')!.changes).toBeNull();
  });

  it('keeps the root on top whatever the sort', () => {
    for (const key of ['branch', 'changes', 'ahead', 'sessions', 'pr', 'activity'] as const)
      for (const dir of ['asc', 'desc'] as const) expect(sortRows(table, key, dir)[0].worktree.id).toBe('main');
  });

  it('sorts by each column', () => {
    expect(ids(sortRows(table, 'branch', 'asc'))).toEqual(['main', 'a-one', 'b-two', 'c-three']);
    expect(ids(sortRows(table, 'changes', 'desc'))).toEqual(['main', 'b-two', 'a-one', 'c-three']);
    expect(ids(sortRows(table, 'ahead', 'desc'))).toEqual(['main', 'a-one', 'b-two', 'c-three']);
    expect(ids(sortRows(table, 'pr', 'asc'))).toEqual(['main', 'a-one', 'b-two', 'c-three']);
    expect(ids(sortRows(table, 'activity', 'desc'))).toEqual(['main', 'b-two', 'a-one', 'c-three']);
  });

  it('sorts names A to Z first and numbers biggest first', () => {
    expect(firstDir('branch')).toBe('asc');
    expect(firstDir('changes')).toBe('desc');
    expect(firstDir('activity')).toBe('desc');
  });
});

describe('the merged and clean sweep', () => {
  const state = (s: Partial<CheckoutState>): CheckoutState => ({ changed: 0, untracked: 0, unpushed: 0, ...s });
  const m = model({
    worktrees: [wt('main', { is_main: true }), wt('merged'), wt('clean'), wt('busy'), wt('open-pr')],
    agents: [agent('a', 'busy'), agent('old', 'clean', { archived: true })],
    prs: { merged: pr(1, 'merged'), 'open-pr': pr(2, 'open') },
  });
  const g = (id: string) => groupOf(m, m.worktrees.find((w) => w.id === id)!);

  it('only asks git about linked checkouts with nothing live', () => {
    expect(ids(sweepCandidates(m.worktrees.map((w) => groupOf(m, w))))).toEqual(['merged', 'clean', 'open-pr']);
  });

  it('takes a merged one even with commits no remote holds', () => {
    expect(sweepReason(g('merged'), state({ unpushed: 3 }))).toBe('merged');
  });

  it('takes a clean one only when every commit is on a remote', () => {
    expect(sweepReason(g('clean'), state({}))).toBe('clean');
    expect(sweepReason(g('clean'), state({ unpushed: 1 }))).toBeNull();
    expect(sweepReason(g('clean'), state({ unpushed: null }))).toBeNull();
  });

  it('never takes uncommitted work, an open PR, the root, a live one, or an unreadable one', () => {
    expect(sweepReason(g('merged'), state({ untracked: 1 }))).toBeNull();
    expect(sweepReason(g('clean'), state({ changed: 2 }))).toBeNull();
    expect(sweepReason(g('open-pr'), state({}))).toBeNull();
    expect(sweepReason(g('main'), state({}))).toBeNull();
    expect(sweepReason(g('busy'), state({}))).toBeNull();
    expect(sweepReason(g('clean'), null)).toBeNull();
  });
});
