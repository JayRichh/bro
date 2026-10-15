// The account verbs the window offers outside the Accounts view too: move
// a session to another account (its menu, its header chip), a repo's
// default account (its menu), running a login in a terminal. The words and
// choices come from view.ts; this half asks, writes and says what happened.

import { api, type AccountRow, type Agent } from '../api';
import { app, openSession, refreshAccounts, refreshLaunchables, sessionWorktree, toast } from '../store.svelte';
import { nav } from '../shell/history.svelte';
import { ask, type MenuEntry } from '../ui/ui.svelte';
import { keysOf, withKey } from '../shortcuts';
import { sessionHarness } from './usage';
import { canSwitch, projectAccount, switchConfirm, switchTargets, type SwitchTarget } from './view';
import { watchSignIn } from './add.svelte';

/** Why the move is not offered, for a disabled control's tooltip. */
export const SWITCH_UNAVAILABLE = 'The running kete daemon is older than this window: restart it (kete kill, then reopen) to move sessions between accounts.';

/** The Accounts view. */
export function openAccounts() {
  nav.go({ view: 'accounts' });
}

const agentOf = (id: string) => app.model.agents.find((a) => a.id === id);

/** Confirm, then restart `agent` fresh on `to`. */
export async function confirmSwitch(agent: Agent, to: SwitchTarget) {
  const c = switchConfirm(agent, to);
  const r = await ask.confirm(c);
  if (!r.ok) return;
  try {
    await api.accountSwitch(agent.id, to.profile);
    toast(`${agent.name} restarted on ${to.profile ?? `${sessionHarness(agent)}'s own login`}.`);
    refreshAccounts();
  } catch (e) {
    toast(`Could not move it: ${e}`);
  }
}

/** The move's choices as menu items (the header chip's menu). */
export function accountMenu(id: string): MenuEntry[] {
  const a = agentOf(id);
  if (!a) return [];
  const head: MenuEntry = { heading: `${sessionHarness(a)} \u00b7 runs on ${a.profile?.trim() || 'its own login'}` };
  if (!canSwitch(app.model)) return [head, { label: 'Move to account\u2026 (daemon too old)', run: () => toast(SWITCH_UNAVAILABLE), disabled: true }];
  const targets = switchTargets(a, app.accounts);
  const items: MenuEntry[] = targets.map((t) => ({ label: `Restart on ${t.label}\u2026`, run: () => confirmSwitch(a, t) }));
  if (!items.length) items.push({ label: `No other ${sessionHarness(a)} account: add one`, run: openAccounts });
  return [head, ...items, 'sep', { label: 'Accounts\u2026', run: openAccounts, keys: keysOf('accounts') }];
}

/** "Move to account...": pick where, then confirm. */
export async function moveToAccount(id: string) {
  const a = agentOf(id);
  if (!a) return;
  if (!canSwitch(app.model)) return toast(SWITCH_UNAVAILABLE);
  if (!app.accounts.length) await api.accounts().then((r) => (app.accounts = r)).catch(() => {});
  const targets = switchTargets(a, app.accounts);
  if (!targets.length) return toast(`There is no other ${sessionHarness(a)} account. Add one in ${withKey('Accounts', 'accounts')}.`);
  const own = '\u0000own';
  const pick = await ask.pick({
    title: `Move "${a.name}" to (restarts fresh)`,
    items: targets.map((t) => ({
      id: t.profile ?? own,
      label: t.label,
      hint: t.off ? 'off' : t.needsLogin ? 'needs login' : undefined,
    })),
  });
  if (pick == null) return;
  const to = targets.find((t) => (t.profile ?? own) === pick);
  const now = agentOf(id);
  if (to && now) await confirmSwitch(now, to);
}

/** A repo's "Default account...": per harness with accounts, pick one (or
 *  none, back to the spread). */
export async function pickProjectAccount(project: string) {
  const p = app.model.projects.find((x) => x.id === project);
  if (!p) return;
  const profiles = app.launch.profiles;
  if (!profiles.length) return toast(`No accounts to pick from yet. Add one in ${withKey('Accounts', 'accounts')}.`);
  const harnesses = [...new Set(profiles.map((x) => x.harness))];
  const CLEAR = '\u0000clear\u0000';
  const clear = (h: string) => `${CLEAR}${h}`;
  const items: { id: string; label: string; hint?: string }[] = [];
  for (const h of harnesses) {
    const now = projectAccount(app.launch, p.repo_path, h);
    for (const x of profiles.filter((y) => y.harness === h)) {
      items.push({
        id: `${h}\u0000${x.name}`,
        label: `${h}: ${x.name}${x.disabled ? ' (off)' : ''}`,
        hint: x.name === now ? 'current' : undefined,
      });
    }
    if (now) items.push({ id: clear(h), label: `${h}: no project default (the spread picks)` });
  }
  const pick = await ask.pick({ title: `Default account for ${p.name}`, items });
  if (pick == null) return;
  if (pick.startsWith(CLEAR)) return setProjectAccount(project, pick.slice(CLEAR.length), null);
  const [harness, name] = pick.split('\u0000');
  await setProjectAccount(project, harness, name);
}

/** Write a project's default account for a harness (null clears). */
export async function setProjectAccount(project: string, harness: string, profile: string | null) {
  const p = app.model.projects.find((x) => x.id === project);
  try {
    await api.projectSetAccount(project, harness, profile);
    toast(
      profile
        ? `${harness} sessions started in ${p?.name ?? 'the repo'} without a pick now run on ${profile}.`
        : `${p?.name ?? 'The repo'} has no default ${harness} account now: the account spread picks.`,
    );
    refreshLaunchables();
  } catch (e) {
    toast(`Could not save it: ${e}`);
  }
}

/** The worktree a login terminal opens in: the open session's, else the
 *  first repo's main checkout. */
export function loginWorktree(): string | null {
  const open = sessionWorktree(app.session);
  if (open) return open;
  const first = app.model.projects[0]?.id;
  const wts = app.model.worktrees.filter((w) => w.project_id === first);
  return (wts.find((w) => w.is_main) ?? wts[0])?.id ?? null;
}

/** Run an account's login command in a new terminal and show it; for a
 *  profile, the docked panel then watches the row until it signs in. */
export async function runLogin(r: Pick<AccountRow, 'key' | 'name' | 'profile'>): Promise<boolean> {
  const wt = loginWorktree();
  if (!wt) {
    toast('Add a repo first: the login runs in a terminal in one. Or copy the command.');
    return false;
  }
  try {
    const terminal = await api.accountLoginRun(wt, r.key);
    openSession({ Terminal: terminal });
    if (r.profile) watchSignIn(r.name);
    else toast(`Signing ${r.name} in: finish in the terminal, then the row updates.`);
    return true;
  } catch (e) {
    toast(String(e));
    return false;
  }
}
