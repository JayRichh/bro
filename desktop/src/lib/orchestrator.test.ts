import { expect, test } from 'vitest';
import type { Agent, AgentStatus, Model } from './api';
import { canProvision, countWords, descendantCounts, elsewhere, opensOnOrchestrator, orchestratorOf, orchestratorTree, placeKey } from './orchestrator';
import { repoRows } from './overview/repos';

function agent(id: string, worktree: string, parent: string | null = null, status: AgentStatus = 'finished', unseen = false): Agent {
  return {
    id,
    worktree_id: worktree,
    name: id,
    status,
    archived: false,
    unseen,
    status_changed_at: 0,
    kind: 'claude',
    custom_harness: null,
    model: null,
    profile: null,
    alive: true,
    recent_prompts: [],
    role: 'worker',
    sort_order: 0,
    parent,
  };
}

/** Three repos: the orchestrator's own workspace, payments and web. */
function model(agents: Agent[]): Model {
  return {
    connected: true,
    error: null,
    daemon_pid: 1,
    protocol_version: 54,
    runtime_dir: '',
    projects: [
      { id: 'po', name: 'orchestrator', repo_path: '/o', sort_order: 0 },
      { id: 'pp', name: 'payments', repo_path: '/p', sort_order: 1 },
      { id: 'pw', name: 'web', repo_path: '/w', sort_order: 2 },
    ],
    worktrees: [
      { id: 'wo', project_id: 'po', path: '/o', branch: 'main', is_main: true, sort_order: 0 },
      { id: 'wp', project_id: 'pp', path: '/p', branch: 'main', is_main: true, sort_order: 0 },
      { id: 'wp2', project_id: 'pp', path: '/p2', branch: 'gst-rounding', is_main: false, sort_order: 1 },
      { id: 'ww', project_id: 'pw', path: '/w', branch: 'main', is_main: true, sort_order: 0 },
    ],
    agents,
    terminals: [],
    ports: [],
    prs: {},
    subagents: [],
  };
}

function crossRepo(): Model {
  const orch = { ...agent('orch', 'wo', null, 'running'), role: 'orchestrator' };
  return model([
    orch,
    agent('gst', 'wp2', 'orch', 'needs_feedback'),
    agent('gst-tests', 'wp2', 'gst', 'running'),
    agent('hero', 'ww', 'orch', 'finished', true),
    agent('lonely', 'ww', null, 'needs_feedback'),
    agent('quiet', 'wp', null, 'finished'),
  ]);
}

test('the orchestrator is the one session with the role, archived ones aside', () => {
  const m = crossRepo();
  expect(orchestratorOf(m)?.id).toBe('orch');
  m.agents[0].archived = true;
  expect(orchestratorOf(m)).toBeUndefined();
});

test('its tree spans repos: children in payments and web, grandchildren too', () => {
  const root = orchestratorTree(crossRepo())!;
  expect(root.agent?.id).toBe('orch');
  expect(root.depth).toBe(0);
  expect(root.children.map((c) => c.id).sort()).toEqual(['gst', 'hero']);
  const gst = root.children.find((c) => c.id === 'gst')!;
  expect(gst.children.map((c) => c.id)).toEqual(['gst-tests']);
  expect(gst.children[0].depth).toBe(2);
});

test('attention counts cover its descendants only, never itself or strangers', () => {
  const c = descendantCounts(orchestratorTree(crossRepo()));
  // gst needs you, gst-tests runs, hero is unread; orch (running) and
  // lonely (needs you, not its child) do not count.
  expect(c).toEqual({ needs: 1, running: 1, unread: 1 });
  expect(countWords(c).map((w) => w.text)).toEqual(['1 needs you', '1 unread', '1 running']);
});

test('elsewhere lists what wants you outside its tree', () => {
  const m = crossRepo();
  expect(elsewhere(m, orchestratorTree(m)).map((n) => n.id)).toEqual(['lonely']);
});

test('no orchestrator: no tree, nothing counted', () => {
  const m = model([agent('a', 'wp')]);
  expect(orchestratorTree(m)).toBeNull();
  expect(descendantCounts(null)).toEqual({});
});

test('provisioning needs a connected protocol-54 daemon', () => {
  const m = crossRepo();
  expect(canProvision(m)).toBe(true);
  expect(canProvision({ ...m, protocol_version: 53 })).toBe(false);
  expect(canProvision({ ...m, connected: false })).toBe(false);
});

test('the overview: a session whose parent is in another repo says where it came from', () => {
  const rows = repoRows(crossRepo());
  const payments = rows.find((r) => r.project.id === 'pp')!;
  const gst = payments.forest.find((n) => n.id === 'gst')!;
  // Still a root in its own repo, but not a silent one.
  expect(gst.from?.id).toBe('orch');
  // Its own child nests under it as before and carries no cue.
  expect(gst.children[0].id).toBe('gst-tests');
  expect(gst.children[0].from).toBeUndefined();
  // A true root has no cue.
  expect(payments.forest.find((n) => n.id === 'quiet')?.from).toBeUndefined();
  const web = rows.find((r) => r.project.id === 'pw')!;
  expect(web.forest.find((n) => n.id === 'hero')?.from?.id).toBe('orch');
  expect(web.forest.find((n) => n.id === 'lonely')?.from).toBeUndefined();
});

test('a parent in the same repo but archived (hidden) is no cross-repo cue', () => {
  const m = crossRepo();
  m.agents.push({ ...agent('old', 'wp', null), archived: true }, agent('kid', 'wp', 'old'));
  const payments = repoRows(m).find((r) => r.project.id === 'pp')!;
  expect(payments.forest.find((n) => n.id === 'kid')?.from).toBeUndefined();
});

test('the window opens on the orchestrator from the saved pref unless the command line picks somewhere', () => {
  const s = (view: string | null, session: string | null = null) => ({ view, session, theme: null, diff_mode: null });
  expect(opensOnOrchestrator('orchestrator', null)).toBe(true);
  expect(opensOnOrchestrator('orchestrator', s(null))).toBe(true);
  expect(opensOnOrchestrator('orchestrator', s('board'))).toBe(false);
  expect(opensOnOrchestrator('orchestrator', s(null, 'api'))).toBe(false);
  expect(opensOnOrchestrator('home', s('orchestrator'))).toBe(true);
  expect(opensOnOrchestrator(undefined, null)).toBe(false);
});

test('a startup step that waited never moves a window the user has moved since', () => {
  const before = placeKey({ view: 'overview' });
  expect(placeKey({ view: 'overview' })).toBe(before);
  expect(placeKey({ view: 'overview', repo: 'p1' })).not.toBe(before);
  expect(placeKey({ view: 'settings' })).not.toBe(before);
});
