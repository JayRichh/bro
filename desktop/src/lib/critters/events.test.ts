// Deriving the world's app events from the daemon model: each transition
// fires once, the first snapshot and newly seen ids fire nothing, archived
// sessions don't count.
import { describe, expect, it } from 'vitest';
import type { Agent, AgentStatus, PrInfo } from '../api';
import { allIdle, changes, snap, type Snap } from './events';

function agent(id: string, status: AgentStatus, archived = false): Agent {
  return {
    id,
    worktree_id: 'w1',
    name: id,
    status,
    archived,
    unseen: false,
    status_changed_at: 0,
    kind: 'claude',
    custom_harness: null,
    model: null,
    profile: null,
    alive: status !== 'terminated',
    recent_prompts: [],
    role: 'worker',
    sort_order: 0,
    parent: null,
  };
}

function pr(state?: PrInfo['state']): PrInfo {
  return { number: 1, url: 'https://example.test/pr/1', title: 'A change', draft: false, state };
}

const agents = (...list: [string, AgentStatus][]): Snap => snap({ agents: list.map(([id, s]) => agent(id, s)), prs: {} });
const prs = (rec: Record<string, PrInfo['state']>): Snap =>
  snap({ agents: [], prs: Object.fromEntries(Object.entries(rec).map(([k, s]) => [k, pr(s)])) });

describe('snap', () => {
  it('maps live agents to their status and PRs to their state', () => {
    const s = snap({
      agents: [agent('a', 'running'), agent('b', 'needs_feedback'), agent('gone', 'finished', true)],
      prs: { 'r#1': pr(), 'r#2': pr('merged'), 'r#3': pr('closed') },
    });
    expect(s.agents).toEqual({ a: 'running', b: 'needs_feedback' });
    expect(s.prs).toEqual({ 'r#1': 'open', 'r#2': 'merged', 'r#3': 'closed' });
  });
});

describe('changes', () => {
  it('the first snapshot fires nothing', () => {
    const next = snap({ agents: [agent('a', 'needs_feedback')], prs: { 'r#1': pr('merged') } });
    expect(changes(null, next)).toEqual([]);
  });

  it('a session that starts needing you fires once', () => {
    for (const was of ['fresh', 'running', 'finished'] as const) {
      const a = agents(['a', was]);
      const b = agents(['a', 'needs_feedback']);
      expect(changes(a, b), was).toEqual([{ kind: 'needsYou', agent: 'a' }]);
      // Still waiting on the next snapshot: nothing new.
      expect(changes(b, b), was).toEqual([]);
    }
  });

  it('a session that finishes its turn fires once', () => {
    const a = agents(['a', 'running']);
    const b = agents(['a', 'finished']);
    expect(changes(a, b)).toEqual([{ kind: 'turnDone', agent: 'a' }]);
    expect(changes(b, b)).toEqual([]);
  });

  it('only running to finished is a finished turn', () => {
    for (const was of ['fresh', 'needs_feedback', 'terminated', 'disconnected'] as const) {
      expect(changes(agents(['a', was]), agents(['a', 'finished'])), was).toEqual([]);
    }
    // And only a finish, not any other way out of running.
    for (const now of ['terminated', 'disconnected', 'running'] as const) {
      expect(changes(agents(['a', 'running']), agents(['a', now])), now).toEqual([]);
    }
  });

  it('a PR that merges fires once', () => {
    for (const was of ['open', 'closed'] as const) {
      const a = prs({ 'r#1': was });
      const b = prs({ 'r#1': 'merged' });
      expect(changes(a, b), was).toEqual([{ kind: 'prMerged', key: 'r#1' }]);
      expect(changes(b, b), was).toEqual([]);
    }
    // An older backend leaves the state out: that is open.
    expect(changes(prs({ 'r#1': undefined }), prs({ 'r#1': 'merged' }))).toEqual([{ kind: 'prMerged', key: 'r#1' }]);
  });

  it('a PR closing or reopening fires nothing', () => {
    expect(changes(prs({ 'r#1': 'open' }), prs({ 'r#1': 'closed' }))).toEqual([]);
    expect(changes(prs({ 'r#1': 'merged' }), prs({ 'r#1': 'open' }))).toEqual([]);
  });

  it('ids that only one snapshot holds fire nothing', () => {
    // The scope widened: a waiting session and a merged PR come into view.
    const wide = snap({
      agents: [agent('a', 'running'), agent('new', 'needs_feedback'), agent('done', 'finished')],
      prs: { 'r#1': pr(), 'r#2': pr('merged') },
    });
    const narrow = snap({ agents: [agent('a', 'running')], prs: { 'r#1': pr() } });
    expect(changes(narrow, wide)).toEqual([]);
    // And narrowing again fires nothing either.
    expect(changes(wide, narrow)).toEqual([]);
  });

  it('archived sessions are ignored', () => {
    const a = snap({ agents: [agent('a', 'running')], prs: {} });
    const archived = snap({ agents: [agent('a', 'finished', true)], prs: {} });
    expect(changes(a, archived)).toEqual([]);
    // Un-archiving later is a session coming into view, not a transition.
    expect(changes(archived, snap({ agents: [agent('a', 'needs_feedback')], prs: {} }))).toEqual([]);
    // A session archived while waiting never fires.
    const waiting = snap({ agents: [agent('a', 'needs_feedback', true)], prs: {} });
    expect(changes(a, waiting)).toEqual([]);
  });

  it('fires every change, agents in order and then PRs', () => {
    const a = snap({
      agents: [agent('x', 'running'), agent('y', 'running'), agent('z', 'fresh')],
      prs: { 'r#1': pr(), 'r#2': pr() },
    });
    const b = snap({
      agents: [agent('x', 'finished'), agent('y', 'needs_feedback'), agent('z', 'running')],
      prs: { 'r#1': pr('merged'), 'r#2': pr('merged') },
    });
    expect(changes(a, b)).toEqual([
      { kind: 'turnDone', agent: 'x' },
      { kind: 'needsYou', agent: 'y' },
      { kind: 'prMerged', key: 'r#1' },
      { kind: 'prMerged', key: 'r#2' },
    ]);
  });
});

describe('allIdle', () => {
  it('is true with nothing running', () => {
    expect(allIdle({ agents: [] })).toBe(true);
    expect(allIdle({ agents: [agent('a', 'fresh'), agent('b', 'finished'), agent('c', 'needs_feedback')] })).toBe(true);
  });

  it('is false while any live session runs', () => {
    expect(allIdle({ agents: [agent('a', 'finished'), agent('b', 'running')] })).toBe(false);
  });

  it('ignores archived sessions', () => {
    expect(allIdle({ agents: [agent('a', 'running', true), agent('b', 'finished')] })).toBe(true);
  });
});
