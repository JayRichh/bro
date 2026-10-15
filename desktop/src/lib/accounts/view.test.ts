import { describe, expect, it } from 'vitest';
import type { AccountRow, Agent, Project } from '../api';
import {
  accountLabel,
  canSwitch,
  nameProblem,
  projectAccount,
  projectsDefaultingTo,
  renameBlocker,
  sessionCounts,
  signedIn,
  suggestName,
  switchConfirm,
  switchTargets,
  viewRows,
  SWITCH_ACCOUNT_PROTOCOL,
  RENAME_PROFILE_PROTOCOL,
  sessionsFollow,
} from './view';

const row = (over: Partial<AccountRow> & { key: string }): AccountRow => ({
  name: over.key.startsWith('@') ? over.key.slice(1) : over.key,
  harness: 'claude',
  profile: !over.key.startsWith('@'),
  default: false,
  disabled: false,
  useDefault: false,
  login: 'logged_in',
  loginHint: null,
  usage: null,
  ...over,
});

const agent = (over: Partial<Agent> = {}): Agent => ({
  id: 'a1',
  worktree_id: 'w1',
  name: 'fix-login',
  status: 'finished',
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
  ...over,
});

describe('the add form', () => {
  it('checks names as Profile::problem does, unique across harnesses', () => {
    expect(nameProblem('', [])).toMatch(/name/);
    expect(nameProblem('a b', [])).toMatch(/Letters/);
    expect(nameProblem('../x', [])).toMatch(/Letters/);
    expect(nameProblem('x'.repeat(41), [])).toMatch(/40/);
    expect(nameProblem('work', ['work'])).toMatch(/taken/);
    expect(nameProblem(' work-2_b ', ['work'])).toBeNull();
  });

  it('suggests the next free name for the harness', () => {
    expect(suggestName('claude', [])).toBe('claude-2');
    expect(suggestName('claude', ['claude-2', 'claude-3'])).toBe('claude-4');
    expect(nameProblem(suggestName('my harness!', []), [])).toBeNull();
  });

  it('is done once the new account reads signed in', () => {
    const rows = [row({ key: 'work', login: 'needs_login' }), row({ key: '@claude' })];
    expect(signedIn(rows, 'work')).toBe(false);
    expect(signedIn([row({ key: 'work' })], ' work ')).toBe(true);
    expect(signedIn(rows, 'claude'), 'an own login is not the new account').toBe(false);
  });
});

describe('moving a session to another account', () => {
  it('needs a 57 daemon', () => {
    expect(SWITCH_ACCOUNT_PROTOCOL).toBe(57);
    expect(canSwitch({ connected: true, protocol_version: 56 })).toBe(false);
    expect(canSwitch({ connected: false, protocol_version: 57 })).toBe(false);
    expect(canSwitch({ connected: true, protocol_version: 57 })).toBe(true);
  });

  it('offers the other profiles of its harness, off ones last, then its own login', () => {
    const rows = [
      row({ key: 'off', disabled: true }),
      row({ key: 'work', default: true }),
      row({ key: 'home', login: 'needs_login' }),
      row({ key: 'gpt', harness: 'codex' }),
      row({ key: 'bad name', problem: 'broken' }),
      row({ key: '@claude' }),
    ];
    const on = switchTargets(agent({ profile: 'work' }), rows);
    expect(on.map((t) => t.profile)).toEqual(['home', 'off', null]);
    expect(on[0]).toMatchObject({ needsLogin: true, label: 'home (not signed in)' });
    expect(on[1]).toMatchObject({ off: true, label: 'off (off)' });
    expect(on[2].label).toBe("claude's own login");
    // On its own login already: no own-login target.
    expect(switchTargets(agent(), rows).map((t) => t.profile)).toEqual(['work', 'home', 'off']);
    // A custom harness matches by its id.
    expect(switchTargets(agent({ kind: 'custom', custom_harness: 'codex' }), rows).map((t) => t.profile)).toEqual(['gpt']);
    expect(switchTargets(agent({ kind: 'codex', profile: 'gpt' }), rows).map((t) => t.profile)).toEqual([null]);
  });

  it('says plainly that the move restarts fresh', () => {
    const c = switchConfirm(agent({ profile: 'work' }), { profile: 'home', label: 'home', off: false, needsLogin: true });
    expect(c.title).toBe('Move "fix-login" to home');
    expect(c.message).toContain('fresh on home (from work)');
    expect(c.message).toContain('does not continue the conversation');
    expect(c.detail.join(' ')).toContain('not signed in');
    const own = switchConfirm(agent({ profile: 'work', alive: false }), { profile: null, label: '', off: false, needsLogin: false });
    expect(own.message.startsWith("Start it fresh on claude's own login")).toBe(true);
  });

  it('labels the chip with the profile or own login', () => {
    expect(accountLabel(agent())).toBe('own login');
    expect(accountLabel(agent({ profile: ' work ' }))).toBe('work');
  });
});

describe('project default accounts', () => {
  const launch = { project_accounts: { '/repo': { claude: 'work', codex: 'gpt' } } };
  const projects: Project[] = [
    { id: 'p1', name: 'app', repo_path: '/repo', sort_order: 0 },
    { id: 'p2', name: 'site', repo_path: '/site', sort_order: 1 },
  ];
  it('are read per harness by repo path', () => {
    expect(projectAccount(launch, '/repo', 'claude')).toBe('work');
    expect(projectAccount(launch, '/repo', 'pi')).toBeNull();
    expect(projectAccount(launch, '/site', 'claude')).toBeNull();
    expect(projectAccount({}, '/repo', 'claude')).toBeNull();
    expect(projectAccount(launch, undefined, 'claude')).toBeNull();
  });
  it('name the projects an account is the default for', () => {
    expect(projectsDefaultingTo(launch, projects, 'claude', 'work').map((p) => p.id)).toEqual(['p1']);
    expect(projectsDefaultingTo(launch, projects, 'claude', 'home')).toEqual([]);
  });
});

describe('the table', () => {
  it('groups by harness, the default first, own logins last in each', () => {
    const rows = [
      row({ key: 'a' }),
      row({ key: 'gpt', harness: 'codex' }),
      row({ key: 'b', default: true }),
      row({ key: '@claude' }),
      row({ key: 'c', flaggedDefault: true, disabled: true, harness: 'codex' }),
    ];
    expect(viewRows(rows).map((r) => r.key)).toEqual(['b', 'a', '@claude', 'c', 'gpt']);
  });
  it('counts sessions live and archived', () => {
    const agents = [agent({ profile: 'work' }), agent({ profile: 'work', archived: true }), agent()];
    expect(sessionCounts({ key: 'work' }, agents)).toEqual({ live: 1, all: 2 });
    expect(sessionCounts({ key: '@claude' }, agents)).toEqual({ live: 1, all: 1 });
  });
});

describe('renameBlocker', () => {
  const a = (name: string, profile: string | null, alive: boolean, archived = false) => ({ name, profile, alive, archived });
  it('names the running and stopped sessions on the account', () => {
    expect(renameBlocker('work', [])).toBeNull();
    expect(renameBlocker('work', [a('old', 'work', false, true), a('x', 'home', true)])).toBeNull();
    const why = renameBlocker('work', [a('fix', 'work', true), a('docs', ' work ', false)]);
    expect(why).toContain('1 running (fix)');
    expect(why).toContain('1 stopped that would resume on it (docs)');
    // A daemon whose sessions follow a rename takes it anyway.
    expect(renameBlocker('work', [a('fix', 'work', true)], true)).toBeNull();
    expect(sessionsFollow({ connected: true, protocol_version: RENAME_PROFILE_PROTOCOL })).toBe(true);
    expect(sessionsFollow({ connected: true, protocol_version: 59 })).toBe(false);
    expect(sessionsFollow({ connected: false, protocol_version: 99 })).toBe(false);
    expect(RENAME_PROFILE_PROTOCOL).toBe(60);
  });
});
