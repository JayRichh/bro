import { describe, expect, it } from 'vitest';
import type { Agent } from '../api';
import { turnMarks, turnedWorktrees } from './turns';

const agent = (id: string, worktree_id: string, extra: Partial<Agent> = {}): Agent =>
  ({ id, worktree_id, status: 'running', status_changed_at: 1, archived: false, unseen: false, ...extra }) as Agent;

describe('turnedWorktrees', () => {
  it('names nothing on the first look: the views read every checkout as they mount', () => {
    expect(turnedWorktrees(null, [agent('a', 'w1')])).toEqual([]);
  });

  it('names the worktree of a session whose turn ended, so its commits are read now', () => {
    const before = turnMarks([agent('a', 'w1'), agent('b', 'w2')]);
    const after = [agent('a', 'w1', { status: 'finished', status_changed_at: 2 }), agent('b', 'w2')];
    expect(turnedWorktrees(before, after)).toEqual(['w1']);
  });

  it("names a new session's worktree, and each worktree once", () => {
    const before = turnMarks([agent('a', 'w1')]);
    const after = [agent('a', 'w1', { status_changed_at: 5 }), agent('b', 'w1'), agent('c', 'w3')];
    expect(turnedWorktrees(before, after)).toEqual(['w1', 'w3']);
  });

  it('leaves a model that only moved elsewhere alone', () => {
    const list = [agent('a', 'w1', { unseen: true })];
    expect(turnedWorktrees(turnMarks(list), [agent('a', 'w1', { unseen: false })])).toEqual([]);
  });
});
