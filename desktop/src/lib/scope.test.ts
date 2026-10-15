import { describe, expect, it } from 'vitest';
import type { Agent, Card, Model, Scope, Subagent, TerminalTab, Worktree } from './api';
import {
  accountScope,
  dropPart,
  inScope,
  isEverything,
  pruneScope,
  repoScope,
  sameScope,
  scopedRepo,
  scopedWorktree,
  scopeModel,
  scopeParts,
  upTo,
  worktreeScope,
} from './scope';

const wt = (id: string, project_id: string, is_main = false): Worktree => ({ id, project_id, path: id, branch: `b-${id}`, is_main, sort_order: 0 });
const agent = (id: string, worktree_id: string): Agent => ({ id, worktree_id, name: `n-${id}`, archived: false, parent: null }) as Agent;
const card = (in_scope: boolean): Card => ({ lane: 'done', tier: 3, in_scope, drops: {} as Card['drops'] });

function model(scope: Scope, inside: string[] = []): Model {
  const agents = [agent('a', 'w1'), agent('b', 'w2'), agent('c', 'w3')];
  return {
    connected: true,
    projects: [
      { id: 'p', name: 'alpha', repo_path: 'p', sort_order: 0 },
      { id: 'q', name: 'beta', repo_path: 'q', sort_order: 1 },
    ],
    worktrees: [wt('w1', 'p', true), wt('w2', 'p'), wt('w3', 'q', true)],
    agents,
    terminals: [{ id: 't', worktree_id: 'w2' } as TerminalTab],
    ports: [],
    prs: {},
    subagents: [{ id: 's', parent_agent: 'a' } as Subagent, { id: 's2', parent_agent: 'c' } as Subagent],
    cards: Object.fromEntries(agents.map((a) => [a.id, card(isEverything(scope) || inside.includes(a.id))])),
    lanes: { needs: [], running: [], unseen: [], done: [], pr: [] },
    hidden: { needs: 0, running: 0, unseen: 0, done: 0, pr: 0 },
    scope,
  } as unknown as Model;
}

describe('scope words', () => {
  it('reads null and missing fields as anything', () => {
    expect(isEverything({})).toBe(true);
    expect(isEverything({ repo: null, worktree: null, session: null, account: null })).toBe(true);
    expect(isEverything({ account: '@claude' })).toBe(false);
    expect(sameScope({ repo: 'p', worktree: null }, { repo: 'p' })).toBe(true);
  });

  it('names the chip outermost first, a worktree with its repo', () => {
    const m = model({ worktree: 'w2', account: 'work' });
    expect(scopeParts(m).map((p) => `${p.kind}:${p.label}`)).toEqual(['repo:alpha', 'worktree:b-w2', 'account:work']);
    expect(scopeParts(model({}))).toEqual([]);
  });

  it('sets a repo or a worktree, keeping the account', () => {
    const m = model({});
    expect(repoScope({ worktree: 'w2', account: 'x' }, 'q')).toEqual({ repo: 'q', account: 'x' });
    expect(worktreeScope(m, { account: 'x' }, 'w2')).toEqual({ repo: 'p', worktree: 'w2', account: 'x' });
    expect(accountScope({ repo: 'p' }, null)).toEqual({ repo: 'p' });
  });

  it('takes a segment off with what sits under it', () => {
    const s = { repo: 'p', worktree: 'w2', session: 'b', account: 'x' };
    expect(dropPart(s, 'repo')).toEqual({ account: 'x' });
    expect(dropPart(s, 'worktree')).toEqual({ repo: 'p', account: 'x' });
    expect(dropPart(s, 'account')).toEqual({ repo: 'p', worktree: 'w2', session: 'b' });
    expect(upTo(s, 'repo')).toEqual({ repo: 'p', account: 'x' });
  });

  it('prunes what the model no longer has, never while disconnected', () => {
    expect(pruneScope(model({ repo: 'p', worktree: 'gone' }))).toEqual({ repo: 'p' });
    expect(pruneScope(model({ repo: 'zz', worktree: 'w1' }))).toEqual({});
    expect(pruneScope(model({ repo: 'p', worktree: 'w1' }))).toBeNull();
    expect(pruneScope({ ...model({ repo: 'zz' }), connected: false })).toBeNull();
  });

  it('points the diff and the quick prompt at the scoped checkout', () => {
    expect(scopedWorktree(model({ session: 'c' }))).toBe('w3');
    expect(scopedRepo(model({ session: 'c' }))).toBe('q');
    expect(scopedRepo(model({ repo: 'p' }))).toBe('p');
    expect(scopedWorktree(model({ repo: 'p' }))).toBeNull();
  });
});

describe('scopeModel', () => {
  it('is the model itself with nothing narrowed', () => {
    const m = model({});
    expect(scopeModel(m)).toBe(m);
  });

  it('narrows to a repo: its worktrees, sessions, terminals, subagents with their parent', () => {
    const s = scopeModel(model({ repo: 'p' }, ['a', 'b']));
    expect(s.projects.map((p) => p.id)).toEqual(['p']);
    expect(s.worktrees.map((w) => w.id)).toEqual(['w1', 'w2']);
    expect(s.agents.map((a) => a.id)).toEqual(['a', 'b']);
    expect(s.subagents.map((x) => x.id)).toEqual(['s']);
    expect(s.terminals.map((t) => t.id)).toEqual(['t']);
  });

  it('narrows to an account by the cards, keeping only the worktrees it works in', () => {
    const s = scopeModel(model({ account: 'x' }, ['c']));
    expect(s.agents.map((a) => a.id)).toEqual(['c']);
    expect(s.worktrees.map((w) => w.id)).toEqual(['w3']);
    expect(s.projects.map((p) => p.id)).toEqual(['q']);
    expect(s.terminals).toEqual([]);
    expect(s.subagents.map((x) => x.id)).toEqual(['s2']);
  });

  it('counts a session with no card yet in', () => {
    const m = model({ repo: 'p' });
    delete (m.cards as Record<string, Card>).a;
    expect(inScope(m, 'a')).toBe(true);
  });
});
