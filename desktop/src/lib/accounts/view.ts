// The Accounts view and the account verbs, the pure half (tested in
// view.test.ts): the add form's name check, the spread's words, which
// accounts a session can move to and what the move says, and a project's
// default account. The writes go through `api`; the rules they follow
// live in `nebula_core::profiles` and `nebula_core::accounts`.

import type { AccountRow, Agent, Launchables, Model, Project } from '../api';
import { accountKey, sessionHarness } from './usage';

/** The first daemon protocol that moves a session to another account
 *  (`nebula_core::SWITCH_ACCOUNT_PROTOCOL`). */
export const SWITCH_ACCOUNT_PROTOCOL = 57;

/** Does the connected daemon take a move to another account? */
export function canSwitch(m: Pick<Model, 'connected' | 'protocol_version'>): boolean {
  return m.connected && m.protocol_version >= SWITCH_ACCOUNT_PROTOCOL;
}

/** Why a new profile's name cannot be used, or null: the rules of
 *  `Profile::problem`, and unique across every harness (it names the
 *  profile's home). */
export function nameProblem(name: string, taken: string[]): string | null {
  const n = name.trim();
  if (!n) return 'Give it a name.';
  if (n.length > 40 || !/^[A-Za-z0-9_-]+$/.test(n)) return 'Letters, digits, - and _ only (40 at most).';
  if (taken.some((t) => t.trim() === n)) return `"${n}" is taken: names are unique across harnesses.`;
  return null;
}

/** A free name to start the add form on: `claude-2`, `claude-3`, ... */
export function suggestName(harness: string, taken: string[]): string {
  const base = harness.trim().replace(/[^A-Za-z0-9_-]/g, '') || 'account';
  const have = new Set(taken.map((t) => t.trim()));
  for (let i = 2; ; i++) if (!have.has(`${base}-${i}`)) return `${base}-${i}`;
}

export interface SpreadInfo {
  id: 'manual' | 'failover' | 'least_used';
  label: string;
  /** What it does to a launch that names no account. */
  text: string;
}

/** The ACCOUNT SPREAD choices, as `nebula_core::accounts` runs them. */
export const SPREADS: SpreadInfo[] = [
  {
    id: 'manual',
    label: 'Manual',
    text: 'Each harness runs on its starred default, or its own login when none is starred.',
  },
  {
    id: 'failover',
    label: 'Failover',
    text: 'The default until its 5-hour or weekly use reaches the threshold, then the next account switched on.',
  },
  {
    id: 'least_used',
    label: 'Least used',
    text: 'Whichever account switched on has used the least; one never measured counts as unused.',
  },
];

/** Thresholds offered for failover (`accounts::THRESHOLDS`). */
export const THRESHOLDS = ['50', '70', '80', '90', '95'];

/** A project's default account for `harness`, by the project's repo path. */
export function projectAccount(launch: Pick<Launchables, 'project_accounts'>, repoPath: string | undefined, harness: string): string | null {
  if (!repoPath) return null;
  return launch.project_accounts?.[repoPath]?.[harness.trim()]?.trim() || null;
}

/** The projects that start `name`'s harness on it by default. */
export function projectsDefaultingTo(launch: Pick<Launchables, 'project_accounts'>, projects: Project[], harness: string, name: string): Project[] {
  return projects.filter((p) => projectAccount(launch, p.repo_path, harness) === name.trim());
}

/** One place a session can move to: a profile of its harness, or the
 *  harness's own login (`profile` null). */
export interface SwitchTarget {
  profile: string | null;
  label: string;
  /** Switched off: auto never picks it, but a move may name it. */
  off: boolean;
  /** It is not signed in (or its login ran out): the move still goes, and
   *  the CLI asks for a login in the session. */
  needsLogin: boolean;
}

/** Where `agent` can move: every usable profile of its harness but the one
 *  it runs on, enabled ones first, then its harness's own login unless it
 *  already runs there. Rows come from `accounts_list`. */
export function switchTargets(agent: Pick<Agent, 'kind' | 'custom_harness' | 'profile'>, rows: AccountRow[]): SwitchTarget[] {
  const harness = sessionHarness(agent);
  const current = accountKey(agent.profile, harness);
  const profiles: SwitchTarget[] = rows
    .filter((r) => r.profile && !r.problem && r.harness === harness && r.key !== current)
    .map((r) => ({
      profile: r.name,
      label: `${r.name}${r.default ? ' (default)' : ''}${r.disabled ? ' (off)' : ''}${r.login === 'needs_login' ? ' (not signed in)' : ''}`,
      off: r.disabled,
      needsLogin: r.login === 'needs_login',
    }));
  profiles.sort((a, b) => Number(a.off) - Number(b.off));
  const own = `@${harness}`;
  if (current !== own) {
    const row = rows.find((r) => r.key === own);
    profiles.push({
      profile: null,
      label: `${harness}'s own login`,
      off: false,
      needsLogin: row?.login === 'needs_login',
    });
  }
  return profiles;
}

/** What a session runs on, for its header chip: the profile, or `own login`. */
export function accountLabel(agent: Pick<Agent, 'profile'>): string {
  return agent.profile?.trim() || 'own login';
}

/** The move's confirm, true to what the daemon does: a fresh start, since
 *  the conversation lives in the old account's config home. */
export function switchConfirm(agent: Pick<Agent, 'name' | 'profile' | 'kind' | 'custom_harness' | 'alive'>, to: SwitchTarget) {
  const harness = sessionHarness(agent);
  const from = agent.profile?.trim() || `${harness}'s own login`;
  const where = to.profile ?? `${harness}'s own login`;
  const lines = [
    `Each account keeps its conversations in its own config home, so ${harness} cannot resume this one there.`,
    'The worktree, its files and the session row stay; the transcript so far stays with the old account.',
  ];
  if (to.needsLogin) lines.push(`${where} is not signed in: the new session asks for a login first.`);
  if (to.off) lines.push(`${where} is switched off: auto never picks it, but this move names it.`);
  return {
    title: `Move "${agent.name}" to ${where}`,
    message: `${agent.alive ? 'Stop the agent and start' : 'Start'} it fresh on ${where} (from ${from}), in the same worktree. It does not continue the conversation.`,
    detail: lines,
    confirm: 'Restart on this account',
  };
}

/** Is the add sheet's account done: a row for it that says signed in. */
export function signedIn(rows: AccountRow[], name: string): boolean {
  return rows.some((r) => r.profile && r.name === name.trim() && r.login === 'logged_in');
}

/** The view's order: profiles grouped by harness (harnesses in first-seen
 *  order, the default first in each), then the own logins in use. */
export function viewRows(rows: AccountRow[]): AccountRow[] {
  const harnesses: string[] = [];
  for (const r of rows) if (!harnesses.includes(r.harness)) harnesses.push(r.harness);
  const out: AccountRow[] = [];
  for (const h of harnesses) {
    const mine = rows.filter((r) => r.harness === h);
    const profiles = mine.filter((r) => r.profile);
    profiles.sort((a, b) => Number(!!b.flaggedDefault || b.default) - Number(!!a.flaggedDefault || a.default));
    out.push(...profiles, ...mine.filter((r) => !r.profile));
  }
  return out;
}

/** The sessions on a row, counted live and total (archived ones too, for
 *  the remove warning). */
export function sessionCounts(row: Pick<AccountRow, 'key'>, agents: Agent[]): { live: number; all: number } {
  const on = agents.filter((a) => accountKey(a.profile, sessionHarness(a)) === row.key);
  return { live: on.filter((a) => !a.archived).length, all: on.length };
}

/** Why the profile `name` cannot be renamed now, or null: every
 *  unarchived session naming it blocks (`profiles::rename_blocker`): a
 *  running one lives in its home, a stopped one would fail to resume, and
 *  only the daemon could change the name a session stores. */
export function renameBlocker(name: string, agents: Pick<Agent, 'name' | 'profile' | 'archived' | 'alive'>[]): string | null {
  const on = agents.filter((a) => !a.archived && a.profile?.trim() === name.trim());
  if (!on.length) return null;
  const live = on.filter((a) => a.alive).map((a) => a.name);
  const stopped = on.filter((a) => !a.alive).map((a) => a.name);
  const parts = [
    live.length ? `${live.length} running (${live.join(', ')})` : '',
    stopped.length ? `${stopped.length} stopped that would resume on it (${stopped.join(', ')})` : '',
  ].filter(Boolean);
  return `${name} has sessions: ${parts.join('; ')}. Move them to another account (or archive them), then rename.`;
}
