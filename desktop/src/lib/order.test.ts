import { describe, expect, it } from 'vitest';
import type { Agent, Model, Project, Worktree } from './api';
import {
  agentIds,
  boardIds,
  dropZone,
  hasUserOrder,
  laneDrop,
  placeId,
  projectIds,
  refill,
  rootFirst,
  stepId,
  treeSiblings,
  worktreeIds,
} from './order';
import { lanes } from './status';
import { orderRepos, repoRows } from './overview/repos';
import { worktreeGroups } from './worktrees/groups';

const wt = (id: string, extra: Partial<Worktree> = {}): Worktree => ({
  id,
  project_id: 'p',
  path: `C:/r/${id}`,
  branch: id,
  is_main: false,
  sort_order: 0,
  ...extra,
});
const agent = (id: string, worktree_id = 'w', extra: Partial<Agent> = {}): Agent =>
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
    role: 'worker',
    ...extra,
  }) as Agent;
const project = (id: string, sort_order: number): Project => ({ id, name: id, repo_path: `C:/${id}`, sort_order });
const model = (extra: Partial<Model>): Model =>
  ({
    connected: true,
    projects: [project('p', 0)],
    worktrees: [],
    agents: [],
    terminals: [],
    ports: [],
    prs: {},
    subagents: [],
    ...extra,
  }) as Model;

describe('placeId', () => {
  it('moves a row before or after another', () => {
    expect(placeId(['a', 'b', 'c', 'd'], 'd', 'b', 'before')).toEqual(['a', 'd', 'b', 'c']);
    expect(placeId(['a', 'b', 'c', 'd'], 'a', 'c', 'after')).toEqual(['b', 'c', 'a', 'd']);
  });
  it('is null when nothing changes or a row is missing', () => {
    expect(placeId(['a', 'b'], 'a', 'b', 'before')).toBeNull();
    expect(placeId(['a', 'b'], 'a', 'a', 'after')).toBeNull();
    expect(placeId(['a', 'b'], 'x', 'b', 'after')).toBeNull();
  });
});

describe('stepId', () => {
  it('steps past the next row shown, whatever lies between in the scope', () => {
    // b and d are shown; c (archived, hidden) sits between them.
    expect(stepId(['a', 'b', 'c', 'd'], ['b', 'd'], 'd', -1)).toEqual(['a', 'd', 'b', 'c']);
    expect(stepId(['a', 'b', 'c', 'd'], ['b', 'd'], 'b', 1)).toEqual(['a', 'c', 'd', 'b']);
  });
  it('is null at either end', () => {
    expect(stepId(['a', 'b'], ['a', 'b'], 'a', -1)).toBeNull();
    expect(stepId(['a', 'b'], ['a', 'b'], 'b', 1)).toBeNull();
  });
});

describe('the order rule', () => {
  it('a scope is ordered once a row is above 0', () => {
    expect(hasUserOrder([{ sort_order: 0 }, { sort_order: 0 }])).toBe(false);
    expect(hasUserOrder([{ sort_order: 0 }, { sort_order: 1 }])).toBe(true);
  });

  it('worktrees: root first, then recency until the user orders them', () => {
    const m = model({
      worktrees: [wt('old'), wt('root', { is_main: true }), wt('new')],
      agents: [agent('x', 'old', { status_changed_at: 10 }), agent('y', 'new', { status_changed_at: 20 })],
    });
    expect(worktreeIds(m, 'p')).toEqual(['root', 'new', 'old']);
    m.worktrees = [wt('old', { sort_order: 1 }), wt('root', { is_main: true }), wt('new', { sort_order: 2 })];
    expect(worktreeIds(m, 'p')).toEqual(['root', 'old', 'new']);
  });

  it('worktree groups follow the same rule', () => {
    const m = model({
      worktrees: [wt('root', { is_main: true }), wt('a'), wt('b'), wt('empty')],
      agents: [agent('x', 'a', { status_changed_at: 10 }), agent('y', 'b', { status_changed_at: 20 })],
    });
    expect(worktreeGroups(m, 'p').map((g) => g.worktree.id)).toEqual(['b', 'a']);
    m.worktrees = [wt('root', { is_main: true }), wt('a', { sort_order: 1 }), wt('b', { sort_order: 2 }), wt('empty', { sort_order: 3 })];
    expect(worktreeGroups(m, 'p').map((g) => g.worktree.id)).toEqual(['a', 'b']);
  });

  it('a worktree order keeps the root first', () => {
    const m = model({ worktrees: [wt('root', { is_main: true }), wt('a')] });
    expect(rootFirst(m, ['a', 'root'])).toEqual(['root', 'a']);
  });

  it('sessions: sort_order, then oldest status change', () => {
    const m = model({ agents: [agent('late', 'w', { status_changed_at: 9 }), agent('early', 'w', { status_changed_at: 1 })] });
    expect(agentIds(m, 'w')).toEqual(['early', 'late']);
    m.agents = [agent('late', 'w', { status_changed_at: 9 }), agent('early', 'w', { status_changed_at: 1, sort_order: 1 })];
    expect(agentIds(m, 'w')).toEqual(['late', 'early']);
  });

  it('repos: the rail order in the overview once there are slots', () => {
    const m = model({
      projects: [project('zeta', 0), project('alpha', 1)],
      worktrees: [wt('z', { project_id: 'zeta', is_main: true }), wt('a', { project_id: 'alpha', is_main: true })],
      agents: [agent('asks', 'a', { status: 'needs_feedback' })],
    });
    expect(projectIds(m)).toEqual(['zeta', 'alpha']);
    expect(repoRows(m).map((r) => r.project.id)).toEqual(['zeta', 'alpha']);
    // All at 0 (no slots): attention, then name.
    m.projects = [project('zeta', 0), project('alpha', 0)];
    expect(orderRepos(repoRows(m)).map((r) => r.project.id)).toEqual(['alpha', 'zeta']);
  });
});

describe('tree siblings', () => {
  it('are the sessions under the same parent in the same checkout', () => {
    const m = model({
      agents: [
        agent('lead'),
        agent('kid1', 'w', { parent: 'lead', status_changed_at: 1 }),
        agent('kid2', 'w', { parent: 'lead', status_changed_at: 2 }),
        agent('solo', 'w', { status_changed_at: 3 }),
        agent('gone', 'w', { archived: true, status_changed_at: 4 }),
        agent('away', 'other', { parent: 'lead' }),
      ],
    });
    expect(treeSiblings(m, 'kid2', false)).toEqual(['kid1', 'kid2']);
    expect(treeSiblings(m, 'solo', false)).toEqual(['lead', 'solo']);
    expect(treeSiblings(m, 'solo', true)).toEqual(['lead', 'solo', 'gone']);
    expect(treeSiblings(m, 'gone', false)).toEqual([]);
    // Its parent lives in another checkout: a root there.
    expect(treeSiblings(m, 'away', false)).toEqual(['away']);
  });

  it('a child of a hidden archived parent stands as a root', () => {
    const m = model({ agents: [agent('p', 'w', { archived: true }), agent('c', 'w', { parent: 'p' }), agent('r')] });
    expect(treeSiblings(m, 'c', false)).toEqual(['c', 'r']);
    expect(treeSiblings(m, 'c', true)).toEqual(['c']);
  });
});

describe('dropZone', () => {
  it('halves for an insert only, quarters with a drop onto it', () => {
    expect(dropZone(2, 10, { insert: true, onto: false })).toBe('before');
    expect(dropZone(6, 10, { insert: true, onto: false })).toBe('after');
    expect(dropZone(1, 10, { insert: true, onto: true })).toBe('before');
    expect(dropZone(5, 10, { insert: true, onto: true })).toBe('onto');
    expect(dropZone(9, 10, { insert: true, onto: true })).toBe('after');
    expect(dropZone(9, 10, { insert: false, onto: true })).toBe('onto');
    expect(dropZone(9, 10, { insert: false, onto: false })).toBeNull();
  });
});

describe('laneDrop', () => {
  it('does what the lane means where the user can cause it', () => {
    expect(laneDrop('unseen', 'done')).toEqual({ act: 'seen' });
    expect(laneDrop('running', 'archived')).toEqual({ act: 'archive' });
    expect(laneDrop('done', 'done')).toEqual({ act: 'reorder' });
  });
  it('refuses a status the agent or the pull request owns, with the reason', () => {
    for (const from of ['needs', 'running', 'pr'] as const) {
      const d = laneDrop(from, 'done');
      expect(d.act).toBe('refuse');
    }
    expect(laneDrop('needs', 'done')).toMatchObject({ reason: expect.stringContaining('Needs you') });
    for (const to of ['needs', 'running', 'unseen', 'pr'] as const) expect(laneDrop('done', to).act).toBe('refuse');
  });
});

describe('board lanes', () => {
  it('newest first across checkouts, the user order inside an ordered one', () => {
    const cards = [
      agent('a1', 'A', { status_changed_at: 50, sort_order: 2 }),
      agent('b1', 'B', { status_changed_at: 40 }),
      agent('a2', 'A', { status_changed_at: 30, sort_order: 0 }),
      agent('a3', 'A', { status_changed_at: 10, sort_order: 1 }),
    ];
    expect(refill(cards, () => false).map((a) => a.id)).toEqual(['a1', 'b1', 'a2', 'a3']);
    // A's cards take back A's slots (0, 2, 3) in its order: a2, a3, a1.
    expect(refill(cards, (w) => w === 'A').map((a) => a.id)).toEqual(['a2', 'b1', 'a3', 'a1']);
  });

  it('lanes() reads which checkouts are ordered from the model', () => {
    const m = model({
      agents: [
        agent('x', 'A', { status_changed_at: 20, sort_order: 1 }),
        agent('y', 'A', { status_changed_at: 10, sort_order: 0 }),
        agent('z', 'B', { status_changed_at: 30 }),
      ],
    });
    expect(lanes(m).done.map((a) => a.id)).toEqual(['z', 'y', 'x']);
  });
});

describe('board moves', () => {
  it('start from the cards as they stand: newest first until the checkout is ordered', () => {
    const m = model({
      agents: [agent('old', 'A', { status_changed_at: 1 }), agent('mid', 'A', { status_changed_at: 2 }), agent('new', 'A', { status_changed_at: 3 })],
    });
    expect(boardIds(m, 'A')).toEqual(['new', 'mid', 'old']);
    // Alt+Down on the top card: one place down, the rest kept as shown.
    expect(stepId(boardIds(m, 'A'), ['new', 'mid', 'old'], 'new', 1)).toEqual(['mid', 'new', 'old']);
    m.agents = [agent('old', 'A', { sort_order: 0 }), agent('mid', 'A', { sort_order: 2 }), agent('new', 'A', { sort_order: 1 })];
    expect(boardIds(m, 'A')).toEqual(['old', 'new', 'mid']);
  });
});
