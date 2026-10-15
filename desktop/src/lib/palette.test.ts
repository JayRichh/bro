import { describe, expect, it } from 'vitest';
import type { Agent, Card, Model } from './api';
import { attentionSessions, buildItems, labelMarks, scattered, search, stepRing } from './palette';

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

/** The model's tiers, as the Tauri side fills them (nebula-core's rule). */
const cards = (tiers: Record<string, number>): Record<string, Card> =>
  Object.fromEntries(Object.entries(tiers).map(([id, tier]) => [id, { lane: 'done', tier }]));

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
  cards: cards({ 'old-done': 3, 'new-done': 3, busy: 1, asks: 0, unread: 2, gone: 3 }),
  lanes: { needs: [], running: [], unseen: [], done: [], pr: [] },
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

  it('sorts and words a session by the tier the model gives it, not its own flags', () => {
    // Stopped but still flagged unseen: core says it is not unread.
    const m: Model = {
      ...model,
      agents: [
        agent('stopped', 'w1', 'terminated', { unseen: true, status_changed_at: 99 }),
        agent('unread', 'w1', 'finished', { unseen: true }),
      ],
      cards: cards({ stopped: 3, unread: 2 }),
    };
    const rows = search(buildItems(m, []), '').map((r) => [r.item.label, r.item.hint]);
    expect(rows).toEqual([
      ['unread', 'unread'],
      ['stopped', 'session'],
    ]);
  });

  it('marks only the label part of a path row', () => {
    const [top] = search(items, 'asks');
    expect([...labelMarks(top.item, top.positions)]).toEqual([0, 1, 2, 3]);
  });
});

describe('palette ranking', () => {
  it('puts a name that holds the query whole above one that holds it scattered', () => {
    const m: Model = {
      ...model,
      agents: [
        // Running and newer, so first on attention alone: its name holds
        // g..s..t only as scattered letters.
        agent('ledger-recon-test', 'w2', 'running', { status_changed_at: 90 }),
        agent('gst-rounding', 'w2', 'finished', { status_changed_at: 10 }),
      ],
      cards: cards({ 'ledger-recon-test': 1, 'gst-rounding': 3 }),
    };
    const rows = search(buildItems(m, []), 'gst').filter((r) => r.item.target.kind === 'session');
    expect(rows.map((r) => r.item.label)).toEqual(['gst-rounding', 'ledger-recon-test']);
  });

  it('tells a scattered name from a prefix, word start or substring', () => {
    expect(scattered('gst', 'ledger-recon-test')).toBe(true);
    expect(scattered('gst', 'gst-rounding')).toBe(false);
    expect(scattered('round', 'gst-rounding')).toBe(false);
    expect(scattered('ound', 'gst-rounding')).toBe(false);
    expect(scattered('gst', 'main')).toBe(false);
  });
});

describe('the . and , ring', () => {
  // The terminal app's own case: needs you, running, unread, then the rest
  // newest first with the never-run last; archived sessions are no stop.
  const m: Model = {
    ...model,
    agents: [
      agent('fresh', 'w1', 'fresh'),
      agent('read', 'w1', 'finished', { status_changed_at: 50 }),
      agent('unread', 'w3', 'finished', { unseen: true, status_changed_at: 10 }),
      agent('run', 'w2', 'running', { status_changed_at: 5 }),
      agent('ask', 'w1', 'needs_feedback', { status_changed_at: 1 }),
      agent('gone', 'w1', 'needs_feedback', { archived: true, status_changed_at: 9000 }),
    ],
    cards: cards({ fresh: 3, read: 3, unread: 2, run: 1, ask: 0, gone: 0 }),
  };

  it('is every live session in attention order', () => {
    expect(attentionSessions(m)).toEqual(['ask', 'run', 'unread', 'read', 'fresh']);
  });

  it('steps and wraps, entering at the first or the last', () => {
    const ring = attentionSessions(m);
    expect(stepRing(ring, 'run', 1)).toBe('unread');
    expect(stepRing(ring, 'fresh', 1)).toBe('ask');
    expect(stepRing(ring, 'ask', -1)).toBe('fresh');
    expect(stepRing(ring, null, 1)).toBe('ask');
    expect(stepRing(ring, 'gone', -1)).toBe('fresh');
    expect(stepRing([], 'ask', 1)).toBeNull();
  });
});
