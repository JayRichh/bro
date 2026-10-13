import { describe, expect, it } from 'vitest';
import type { Agent, Model } from './api';
import { buildItems, labelMarks, search } from './palette';

const agent = (id: string, worktree: string, status: Agent['status'], extra: Partial<Agent> = {}): Agent => ({
  id,
  worktree_id: worktree,
  name: id,
  status,
  archived: false,
  unseen: false,
  status_changed_at: 0,
  kind: 'claude',
  custom_harness: null,
  model: null,
  profile: null,
  alive: true,
  recent_prompts: [],
  role: 'worker',
  sort_order: 0,
  parent: null,
  ...extra,
});

const model: Model = {
  connected: true,
  error: null,
  daemon_pid: 1,
  protocol_version: 1,
  runtime_dir: '',
  projects: [
    { id: 'p1', name: 'ledger-api', repo_path: '/l', sort_order: 0 },
    { id: 'p2', name: 'storefront', repo_path: '/s', sort_order: 1 },
  ],
  worktrees: [
    { id: 'w1', project_id: 'p1', path: '/l', branch: 'main', is_main: true, sort_order: 0 },
    { id: 'w2', project_id: 'p1', path: '/l2', branch: 'gst-rounding', is_main: false, sort_order: 1 },
    { id: 'w3', project_id: 'p2', path: '/s', branch: 'main', is_main: true, sort_order: 0 },
  ],
  agents: [
    agent('old-done', 'w1', 'finished', { status_changed_at: 10 }),
    agent('new-done', 'w3', 'finished', { status_changed_at: 50 }),
    agent('busy', 'w2', 'running', { status_changed_at: 5 }),
    agent('asks', 'w3', 'needs_feedback', { status_changed_at: 1 }),
    agent('unread', 'w1', 'finished', { unseen: true }),
    agent('gone', 'w1', 'finished', { archived: true }),
  ],
  terminals: [],
  ports: [],
  prs: { w2: { number: 42, url: 'https://x/pr/42', title: 'Round GST per invoice', draft: false } },
  subagents: [],
};

const commands = [
  { id: 'settings', label: 'Open settings', group: 'Window', keys: ['ctrl+,'] },
  { id: 'theme', label: 'Cycle theme', group: 'Window' },
];

describe('the palette', () => {
  const items = buildItems(model, commands);

  it('lists sessions in attention order before a query, then actions, never archived', () => {
    const rows = search(items, '').map((r) => r.item.label);
    expect(rows).toEqual(['asks', 'busy', 'unread', 'new-done', 'old-done', 'Open settings', 'Cycle theme']);
  });

  it('reaches repos, worktrees and PRs once there is a query', () => {
    const kinds = (q: string) => search(items, q).map((r) => r.item.target.kind);
    expect(kinds('gst')).toContain('worktree');
    expect(kinds('gst')).toContain('session');
    expect(kinds('#42')).toEqual(['pr']);
    // Ties go to the session that wants you, as the terminal app's do.
    const store = search(items, 'storefront').map((r) => r.item.target);
    expect(store[0]).toEqual({ kind: 'session', id: 'asks' });
    expect(store).toContainEqual({ kind: 'repo', id: 'p2' });
  });

  it('narrows a session by any part of its path', () => {
    const rows = search(items, 'ledger busy');
    expect(rows[0].item.label).toBe('busy');
    expect(rows[0].item.crumb).toBe('ledger-api');
  });

  it('finds actions by label and marks the matched label chars', () => {
    const [top] = search(items, 'setti');
    expect(top.item.target).toEqual({ kind: 'action', id: 'settings' });
    expect([...labelMarks(top.item, top.positions)].sort((a, b) => a - b)).toEqual([5, 6, 7, 8, 9]);
  });

  it('marks only the label part of a path row', () => {
    const [top] = search(items, 'asks');
    expect([...labelMarks(top.item, top.positions)]).toEqual([0, 1, 2, 3]);
  });
});
