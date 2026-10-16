// What can be done to a session, a terminal, a worktree or a repo: one
// list per target, used by the context menus (overview rows, board cards,
// the session header, repo headers), the row keys and the command
// palette. Destructive verbs confirm in the window first, as the terminal
// app's do: delete always, archive when Confirm on archive is on or the
// session has sessions nested under it (they go with it).

import { api, type Agent, type Model, type TerminalTab, type Worktree } from './api';
import { app, focusQuickPrompt, markSeen, openDiff, openSession, projectOf, setScope, toast } from './store.svelte';
import { ask, type MenuEntry, type MenuItem } from './ui/ui.svelte';
import type { ShortcutId } from './shortcuts';
import { makeOrchestrator, orchestratorMenuItems, unpinOrchestrator } from './orch.svelte';
import { orchestratorOf } from './orchestrator';
import type { ActionId, AppliesTo } from './actions.gen';
import { destructive, menuIds, menuLabel, verbKeys } from './verbs';
import { worktreeApi } from './worktrees/api';
import { cardsLine, dirty, stateLines } from './worktrees/offer';
import { deleteEmptied, offerConfirm, prepareOffer, stillOffered } from './worktrees/delete';
import { canSwitchBranch, openBranchSwitch } from './worktrees/switcher.svelte';
import { checkoutSessions, deleteAllSessions } from './checkout';
import { moveToAccount, pickProjectAccount, SWITCH_UNAVAILABLE } from './accounts/actions';
import { canSwitch } from './accounts/view';
import { rows as gitRows, whileCutting } from './worktrees/rows.svelte';
import { moveStep, stepInFor, type StepIn } from './reorder.svelte';
import { openComposer, openMerge } from './ship/sheet.svelte';
import { reviewVerbs } from './ship/ship';
import { repoScope, worktreeScope } from './scope';
import { groupOf } from './worktrees/groups';

export type Target =
  /** `viaSub`: picked from one of its subagents' rows, so it acts on
   *  this, the parent session, and says so. */
  | { kind: 'agent'; id: string; viaSub?: boolean }
  | { kind: 'terminal'; id: string }
  | { kind: 'worktree'; id: string }
  | { kind: 'project'; id: string };

/** A target as a string, for `data-session` attributes. */
export function targetKey(t: Target): string {
  return `${t.kind === 'agent' && t.viaSub ? 'subagent' : t.kind}:${t.id}`;
}
export function parseTarget(k: string | null | undefined): Target | null {
  if (!k) return null;
  const i = k.indexOf(':');
  const kind = k.slice(0, i);
  const id = k.slice(i + 1);
  if (kind === 'agent' || kind === 'terminal' || kind === 'worktree' || kind === 'project') return { kind, id };
  if (kind === 'subagent') return { kind: 'agent', id, viaSub: true };
  return null;
}

const m = (): Model => app.model;
const agentOf = (id: string) => m().agents.find((a) => a.id === id);
const terminalOf = (id: string) => m().terminals.find((t) => t.id === id);
const worktreeOf = (id: string | undefined) => m().worktrees.find((w) => w.id === id);

/** Every session started (`kete spawn`) under `id`, at any depth. */
export function descendants(model: Model, id: string): Agent[] {
  const out: Agent[] = [];
  const walk = (p: string) => {
    for (const a of model.agents) {
      if (a.parent === p && a.id !== id && !out.includes(a)) {
        out.push(a);
        walk(a.id);
      }
    }
  };
  walk(id);
  return out;
}

async function attempt(what: string, f: () => Promise<unknown>) {
  try {
    await f();
  } catch (e) {
    toast(`${what}: ${e}`);
  }
}

// ---- session verbs ----

export async function renameAgent(id: string) {
  const a = agentOf(id);
  if (!a) return;
  const v = await ask.prompt({ title: 'Rename session', fields: [{ key: 'name', label: 'Name', value: a.name }], confirm: 'Rename' });
  if (v && v.name.trim() && v.name.trim() !== a.name) await attempt('Could not rename', () => api.renameAgent(id, v.name));
}

/** Archive a session and the thread nested under it. `viaSub`: asked
 *  from a subagent's row, which acts on its parent, so it always confirms
 *  and says it is the parent going. */
export async function archiveAgent(id: string, viaSub = false): Promise<boolean> {
  const a = agentOf(id);
  if (!a || a.archived) return false;
  const kids = descendants(m(), id).filter((k) => !k.archived);
  if (viaSub || kids.length || app.launch.ask_before_archive !== false) {
    const r = await ask.confirm({
      title: kids.length ? `Archive thread \u00b7 ${kids.length + 1} sessions` : 'Archive session',
      message:
        (viaSub ? `This archives the parent session, not the subagent. ` : '') +
        (kids.length
          ? `Archive "${a.name}" and everything nested under it? U brings them back.`
          : `Archive "${a.name}"? Its agent stops; U brings it back.`),
      detail: kids.length ? [a.name, ...kids.map((k) => k.name)] : undefined,
      confirm: 'Archive',
    });
    if (!r.ok) return false;
  }
  // The model may have moved while the dialog was up: act on it as it is now.
  const now = agentOf(id);
  if (!now || now.archived) return false;
  const thread = descendants(m(), id).filter((k) => !k.archived);
  let done = false;
  await attempt('Could not archive', async () => {
    for (const x of [now, ...thread]) await api.archiveAgent(x.id);
    done = true;
  });
  return done;
}

/** Until the model shows `ok`, at most `ms`; whether it did. */
async function until(ok: () => boolean, ms = 3000): Promise<boolean> {
  for (let waited = 0; !ok() && waited < ms; waited += 50) await new Promise((r) => setTimeout(r, 50));
  return ok();
}

/** A Done card let go on Archived (core's `ArchiveAndOfferWorktreeDelete`):
 *  archive it, then, when that leaves its linked worktree with nothing live,
 *  offer the worktree's safe delete (worktrees/delete.ts), as a session
 *  delete does. Uncommitted work there needs its box ticked. */
export async function archiveAndOfferWorktree(id: string) {
  const a = agentOf(id);
  if (!a || a.archived) return;
  const { worktree_id: wt, name } = a;
  if (!(await archiveAgent(id))) return;
  // The offer counts live cards off the model: wait for the archive to land.
  if (!(await until(() => !!agentOf(id)?.archived))) return;
  const p = await prepareOffer(wt, 0, []);
  if (!p) return;
  const o = offerConfirm(p);
  if (!o.check) {
    // Delete emptied worktree is on and the checkout is clean: no question.
    toast(o.line);
    await attempt('Could not delete the worktree', async () => deleteEmptied(stillOffered(p, false, 0, [])));
    return;
  }
  const risky = !o.checked;
  const r = await ask.confirm({
    title: 'Delete its worktree too?',
    message: `Archived "${name}". ${o.line}`,
    detail: o.detail,
    confirm: 'Delete worktree',
    danger: true,
    ...(risky ? { check: o.check, checked: false } : {}),
  });
  if (!r.ok || (risky && !r.checked)) return;
  await attempt('Could not delete the worktree', async () => deleteEmptied(stillOffered(p, true, 0, [])));
}

/** Bring back an archived session and the thread archived with it. */
export async function unarchiveAgent(id: string) {
  const a = agentOf(id);
  if (!a?.archived) return;
  const thread = descendants(m(), id).filter((k) => k.archived);
  await attempt('Could not unarchive', async () => {
    for (const x of [a, ...thread]) await api.unarchiveAgent(x.id);
  });
}

export async function deleteAgent(id: string) {
  const a = agentOf(id);
  if (!a) return;
  const kids = descendants(m(), id);
  // Emptying a linked worktree offers it too (worktrees/delete.ts).
  const taken = (xs: Agent[]) => xs.filter((x) => x.worktree_id === a.worktree_id && !x.archived).length;
  const doomed = [a, ...kids].map((x) => x.id);
  const p = await prepareOffer(a.worktree_id, taken([a, ...kids]), doomed);
  const o = p && offerConfirm(p);
  const r = await ask.confirm({
    title: kids.length ? `Delete thread \u00b7 ${kids.length + 1} sessions` : 'Delete session',
    message:
      (kids.length
        ? `Delete "${a.name}" and everything nested under it? Their history goes too.`
        : `Delete "${a.name}"? Its history goes too; this can't be undone.`) + (o ? ` ${o.line}` : ''),
    detail: kids.length || o ? [...(kids.length ? [a.name, ...kids.map((k) => k.name)] : []), ...(o?.detail ?? [])] : undefined,
    confirm: 'Delete',
    danger: true,
    check: o?.check,
    checked: o?.checked,
  });
  if (!r.ok) return;
  // Sessions spawned while the dialog was up go too: recount now.
  const now = agentOf(id);
  if (!now) return;
  const gone = descendants(m(), id);
  const wt = stillOffered(p, r.checked, taken([now, ...gone]), [now, ...gone].map((x) => x.id));
  if (app.session && 'Agent' in app.session && [now, ...gone].some((x) => x.id === (app.session as { Agent: string }).Agent))
    app.session = null;
  let done = false;
  await attempt('Could not delete', async () => {
    for (const x of [...gone].reverse().concat(now)) await api.deleteAgent(x.id);
    done = true;
  });
  if (done) await attempt('Could not delete the worktree', async () => deleteEmptied(wt));
}

export async function restartAgent(id: string) {
  const a = agentOf(id);
  if (!a || a.archived) return;
  await attempt('Could not restart', async () => {
    await api.restartAgent(id);
    toast(`Restarting ${a.name}.`);
  });
}

export async function moveAgent(id: string) {
  const a = agentOf(id);
  if (!a) return;
  const name = (w: Worktree) => m().projects.find((p) => p.id === w.project_id)?.name ?? '';
  const items = m()
    .worktrees.filter((w) => w.id !== a.worktree_id)
    .map((w) => ({ id: w.id, label: `${name(w)} / ${w.branch}`, hint: w.is_main ? 'main checkout' : undefined }));
  if (!items.length) return toast('There is no other worktree to move it to. Make one first.');
  const to = await ask.pick({ title: `Move "${a.name}" to`, items });
  if (to) await attempt('Could not move', () => api.moveAgent(id, to));
}

export function duplicateAgent(id: string) {
  const a = agentOf(id);
  if (!a) return;
  focusQuickPrompt({
    project: projectOf(a.worktree_id),
    worktree: a.worktree_id,
    harness: a.custom_harness ?? a.kind,
    profile: a.profile ?? undefined,
    model: a.model,
    effort: a.effort ?? null,
    newWorktree: false,
  });
}

export async function followUp(id: string) {
  const a = agentOf(id);
  if (!a) return;
  const v = await ask.prompt({
    title: `Follow-up to ${a.name}`,
    note: a.alive ? undefined : 'This session is not running: restart it first, or open it to start it.',
    fields: [{ key: 'text', label: 'Message', multiline: true, placeholder: 'What next?' }],
    confirm: 'Send',
  });
  if (v?.text.trim()) await attempt('Could not send', () => api.followUp(id, v.text));
}

export function seeAgent(id: string) {
  markSeen({ Agent: id });
}

// ---- terminals ----

export async function renameTerminal(id: string) {
  const t = terminalOf(id);
  if (!t) return;
  const v = await ask.prompt({ title: 'Rename terminal', fields: [{ key: 'name', label: 'Name', value: t.name }], confirm: 'Rename' });
  if (v && v.name.trim() && v.name.trim() !== t.name) await attempt('Could not rename', () => api.renameTerminal(id, v.name));
}

export async function closeTerminal(id: string) {
  const t = terminalOf(id);
  if (!t) return;
  // Its worktree's last card: the confirm offers the worktree too.
  const p = await prepareOffer(t.worktree_id, 1, [id]);
  const o = p && offerConfirm(p);
  let checked = false;
  if (t.alive || o) {
    const r = await ask.confirm({
      title: 'Close terminal',
      message:
        (t.alive ? `Close "${t.run_command ? 'run' : t.name}"? Its shell and anything running in it are killed.` : `Close "${t.name}"?`) +
        (o ? ` ${o.line}` : ''),
      detail: o?.detail,
      confirm: 'Close',
      danger: true,
      check: o?.check,
      checked: o?.checked,
    });
    if (!r.ok) return;
    checked = r.checked;
  }
  const wt = stillOffered(p, checked, 1, [id]);
  let done = false;
  await attempt('Could not close', async () => {
    await api.closeTerminal(id);
    done = true;
  });
  if (done) await attempt('Could not delete the worktree', async () => deleteEmptied(wt));
}

// ---- worktrees and repos ----

export async function newWorktree(project: string | undefined) {
  if (!project) return toast('Pick a repo first.');
  const p = m().projects.find((x) => x.id === project);
  const suggestion = await api.suggestBranch(project).catch(() => '');
  const v = await ask.prompt({
    title: `New worktree in ${p?.name ?? 'repo'}`,
    note: 'Spaces become hyphens. Leave the branch empty to take the suggested name.',
    fields: [
      { key: 'branch', label: 'Branch', placeholder: suggestion, mono: true },
      { key: 'base', label: 'From (optional: a branch or commit; empty uses the default base)', placeholder: 'origin/main', mono: true },
    ],
    confirm: 'Create',
  });
  if (!v) return;
  toast('Making the worktree\u2026');
  await attempt('Could not make the worktree', async () => {
    const branch = v.branch.trim() || suggestion;
    const id = await whileCutting(project, branch, () => api.createWorktree(project, branch, v.base.trim() || null));
    toast(`Worktree ready: ${worktreeOf(id)?.branch ?? v.branch}`);
    focusQuickPrompt({ project, worktree: id, newWorktree: false });
  });
}

export async function deleteWorktree(id: string) {
  const w = worktreeOf(id);
  if (!w || w.is_main) return;
  const live = m().agents.filter((a) => a.worktree_id === id && !a.archived).length;
  const terms = m().terminals.filter((t) => t.worktree_id === id).length;
  // What a non-forced delete would refuse over, so Force is an informed pick.
  const state = await worktreeApi.checkoutState(id).catch(() => null);
  const cards = cardsLine(live, terms);
  const r = await ask.confirm({
    title: 'Delete worktree',
    message: `Delete the worktree on ${w.branch}? Its folder is removed; the branch stays.`,
    detail: [w.path, ...(cards ? [cards] : []), ...stateLines(state)],
    confirm: 'Delete',
    danger: true,
    check: dirty(state)
      ? 'Force: delete with its uncommitted changes (without this, git refuses)'
      : 'Force: delete even with uncommitted changes',
  });
  if (r.ok) await attempt('Could not delete the worktree', () => api.deleteWorktree(id, r.checked));
}

export async function openEditor(worktree: string | undefined) {
  if (!worktree) return;
  await attempt('Could not open', async () => {
    let r = await api.openWorktree(worktree);
    if (!r.opened) {
      // From the repo's .kete.json, not the user's setting: anyone who can
      // push wrote it. Show it once per repo and command before it runs.
      const ok = await ask.confirm({
        title: 'Run the repo\'s open command?',
        message: 'This command comes from the repo\'s .kete.json, not your settings. Run it? You won\'t be asked again for this repo unless it changes.',
        detail: [r.command],
        confirm: 'Run',
      });
      if (!ok.ok) return;
      const shown = r.command;
      r = await api.openWorktree(worktree, shown);
      if (!r.opened) {
        toast("The repo's open command changed before it ran. Open again to review it.");
        return;
      }
    }
    toast(`Opened with: ${r.command}`);
  });
}

/** Ask every checkout's pull request again now. */
export function refreshPrs() {
  worktreeApi
    .refreshPrs()
    .then(() => toast('Asking for pull requests\u2026'))
    .catch((e) => toast(`Could not refresh: ${e}`));
}

export async function openRemote(worktree: string | undefined) {
  if (!worktree) return;
  await attempt('Could not open the remote', () => api.openRemote(worktree));
}

export async function newTerminal(worktree: string | undefined) {
  if (!worktree) return;
  await attempt('Could not open a terminal', async () => {
    const id = await api.createTerminal(worktree);
    if (id) openSession({ Terminal: id });
  });
}

function mainOf(project: string): Worktree | undefined {
  return m().worktrees.find((w) => w.project_id === project && w.is_main) ?? m().worktrees.find((w) => w.project_id === project);
}

export async function renameProject(id: string) {
  const p = m().projects.find((x) => x.id === id);
  if (!p) return;
  const v = await ask.prompt({
    title: 'Rename repo',
    note: 'Only the row changes; the folder on disk stays. Empty puts the folder name back.',
    fields: [{ key: 'name', label: 'Name', value: p.name }],
    confirm: 'Rename',
  });
  if (v && v.name.trim() !== p.name) await attempt('Could not rename', () => api.renameProject(id, v.name));
}

export async function locateProject(id: string) {
  const p = m().projects.find((x) => x.id === id);
  if (!p) return;
  const v = await ask.prompt({
    title: `Locate ${p.name}`,
    note: 'Point the repo at the folder it lives in now, after you moved or renamed it on disk. Nothing is moved.',
    fields: [{ key: 'path', label: 'Folder', value: p.repo_path, mono: true }],
    confirm: 'Locate',
  });
  if (v && v.path.trim() !== p.repo_path) await attempt('Could not locate', () => api.setProjectPath(id, v.path));
}

export async function removeProject(id: string) {
  const p = m().projects.find((x) => x.id === id);
  if (!p) return;
  const r = await ask.confirm({
    title: 'Remove repo',
    message: `Remove ${p.name} from Kete? Its sessions go from the list; nothing on disk is touched.`,
    detail: [p.repo_path],
    confirm: 'Remove',
    danger: true,
  });
  if (r.ok) await attempt('Could not remove', () => api.removeProject(id));
}

// ---- menus ----

// Every menu is built in the registry's order (MENUS) from the verbs the
// window can run on the thing (`verbTable`), in the registry's words
// (`menuLabel`), each with its key: the same rows, words and order as the
// terminal app's. The window's own extras (Mark seen, Review, Default
// account, Locate folder) sit among the registry's rows.

/** What the window does for a registry verb on one thing. */
interface VerbImpl {
  run: () => unknown;
  /** Words where the verb turns (Run or Stop run, Unpin orchestrator). */
  label?: string;
  disabled?: boolean;
  /** Why it is disabled: the row's tooltip, and what its key says. */
  why?: string;
}
export type VerbTable = Partial<Record<ActionId, VerbImpl>>;

const entry = (id: ActionId, on: AppliesTo, v: VerbImpl): MenuItem => ({
  label: v.label ?? menuLabel(id, on),
  run: () => void v.run(),
  keys: verbKeys(id),
  danger: destructive(id),
  ...(v.disabled ? { disabled: true } : {}),
  ...(v.disabled && v.why ? { title: v.why } : {}),
});

/** The registry's rows of `on`'s menu that `table` has, in its order. */
function rows(on: AppliesTo, table: VerbTable): MenuItem[] {
  return menuIds(on).flatMap((id) => {
    const v = table[id];
    return v ? [entry(id, on, v)] : [];
  });
}

/** A checkout's pull request verbs, as the terminal app has them: its
 *  page while it has one (open_pull_request), the composer while none is
 *  open (create_pull_request), the merge while one is (merge_pull_request).
 *  `session`: the card it is asked from, whose drop verdict on Ship says
 *  whether the branch is the base. */
function prVerbs(worktree: string, session?: string): VerbTable {
  const pr = m().prs[worktree];
  const onBase = !!gitRows.git[worktree]?.on_default_branch;
  const rv = reviewVerbs(pr, session ? m().cards[session]?.drops?.pr : undefined, !!worktreeOf(worktree)?.is_main, onBase);
  const t: VerbTable = {};
  if (pr) t.open_pull_request = { run: () => api.openUrl(pr.url).catch((e) => toast(String(e))) };
  if (rv.pr.kind === 'open')
    t.create_pull_request = {
      run: () => openComposer(worktree, session ?? null),
      disabled: !!rv.pr.why,
      why: rv.pr.why ?? undefined,
      // On the base branch, as the terminal app says by leaving it out.
      label: onBase ? `${menuLabel('create_pull_request', session ? 'session' : 'worktree')} (on the base branch)` : undefined,
    };
  if (rv.merge) t.merge_pull_request = { run: () => openMerge(worktree, session ?? null), disabled: !!rv.merge.why };
  return t;
}

/** Focus here: the scope chip narrowed to the checkout (a session's or a
 *  worktree's) or the repo, the account narrowing kept. */
function focusVerb(on: { worktree: string } | { repo: string }): VerbImpl {
  return {
    run: () => {
      const s = m().scope ?? {};
      setScope('repo' in on ? repoScope(s, on.repo) : worktreeScope(m(), s, on.worktree));
    },
  };
}

/** A session's verbs as the window runs them: none an archived one does
 *  not have, no reorder from a subagent's row (it acts on its parent). */
function agentVerbs(a: Agent, viaSub = false, where: StepIn = stepInFor()): VerbTable {
  if (a.archived)
    return {
      unarchive: { run: () => unarchiveAgent(a.id) },
      duplicate_session: { run: () => duplicateAgent(a.id) },
      delete: { run: () => deleteAgent(a.id) },
    };
  const pinned = orchestratorOf(m())?.id === a.id;
  const t: VerbTable = {
    activate: { run: () => openSession({ Agent: a.id }) },
    follow_up: { run: () => followUp(a.id) },
    git_diff: { run: () => openDiff(a.worktree_id) },
    restart: { run: () => restartAgent(a.id) },
    duplicate_session: { run: () => duplicateAgent(a.id) },
    move_session: { run: () => moveAgent(a.id) },
    // A fresh restart on another login: the conversation stays behind.
    move_to_account: canSwitch(m())
      ? { run: () => moveToAccount(a.id) }
      : { run: () => toast(SWITCH_UNAVAILABLE), label: `${menuLabel('move_to_account', 'session')} (daemon too old)`, disabled: true },
    rename: { run: () => renameAgent(a.id) },
    // Make orchestrator only while none is pinned (the menu matrix):
    // replacing one is rare, and Unpin on it comes first.
    pin_orchestrator: pinned
      ? { run: unpinOrchestrator, label: 'Unpin orchestrator' }
      : orchestratorOf(m())
        ? undefined
        : { run: () => makeOrchestrator(a) },
    archive: { run: () => archiveAgent(a.id, viaSub), label: viaSub ? 'Archive parent session\u2026' : undefined },
    delete: { run: () => deleteAgent(a.id) },
  };
  // Its worktree's pull request, and the scope chip on its checkout.
  Object.assign(t, prVerbs(a.worktree_id, a.id));
  t.focus_scope = focusVerb({ worktree: a.worktree_id });
  if (!viaSub) {
    t.reorder_up = { run: () => moveStep({ kind: 'agent', id: a.id }, -1, where) };
    t.reorder_down = { run: () => moveStep({ kind: 'agent', id: a.id }, 1, where) };
  }
  return t;
}

function terminalVerbs(x: TerminalTab): VerbTable {
  return {
    activate: { run: () => openSession({ Terminal: x.id }) },
    rename: { run: () => renameTerminal(x.id) },
    delete: { run: () => closeTerminal(x.id) },
  };
}

/** Run or stop `w`'s run command, when it has one. */
function runVerbOf(w: Worktree | undefined): VerbImpl | undefined {
  if (!w || !app.runCommands[w.id]) return undefined;
  const running = m().terminals.some((t) => t.worktree_id === w.id && t.run_command && t.alive);
  return {
    run: () => attempt(running ? 'Could not stop' : 'Could not run', () => (running ? api.stopRun(w.id) : api.startRun(w.id))),
    label: running ? 'Stop run' : undefined,
  };
}

function worktreeVerbs(w: Worktree): VerbTable {
  const doomed = checkoutSessions(m(), w.id, app.showArchived);
  const count = doomed.agents.length + doomed.terminals.length;
  const project = w.project_id;
  const t: VerbTable = {
    quick_prompt: { run: () => focusQuickPrompt({ project, worktree: w.id, newWorktree: false }) },
    new_terminal: { run: () => newTerminal(w.id) },
    toggle_run: runVerbOf(w),
    git_diff: { run: () => openDiff(w.id) },
    open_worktree: { run: () => openEditor(w.id) },
    open_repo: { run: () => openRemote(w.id) },
    ...prVerbs(w.id),
    focus_scope: focusVerb({ worktree: w.id }),
  };
  if (w.is_main && canSwitchBranch(project)) t.switch_branch = { run: () => openBranchSwitch(project) };
  if (!w.is_main) {
    t.reorder_up = { run: () => moveStep({ kind: 'worktree', id: w.id }, -1) };
    t.reorder_down = { run: () => moveStep({ kind: 'worktree', id: w.id }, 1) };
    t.delete_worktree = { run: () => deleteWorktree(w.id) };
  }
  if (count) t.delete_all = { run: () => deleteAllSessions(w.id) };
  return t;
}

function projectVerbs(project: string): VerbTable {
  const main = mainOf(project);
  const t: VerbTable = {
    quick_prompt: { run: () => focusQuickPrompt({ project }) },
    new_worktree: { run: () => newWorktree(project) },
    toggle_run: runVerbOf(main),
    open_worktree: { run: () => openEditor(main?.id) },
    open_repo: { run: () => openRemote(main?.id) },
    refresh_pull_requests: { run: refreshPrs },
    focus_scope: focusVerb({ repo: project }),
    rename: { run: () => renameProject(project) },
    reorder_up: { run: () => moveStep({ kind: 'project', id: project }, -1) },
    reorder_down: { run: () => moveStep({ kind: 'project', id: project }, 1) },
    remove_project: { run: () => removeProject(project) },
  };
  if (canSwitchBranch(project)) t.switch_branch = { run: () => openBranchSwitch(project) };
  // The Worktrees view's sweep, with a linked checkout to look at. Its
  // module imports this one, so it loads when asked.
  const linked = m().worktrees.filter((w) => w.project_id === project && !w.is_main);
  if (linked.length)
    t.sweep_worktrees = {
      run: () =>
        import('./worktrees/verbs').then((v) =>
          v.sweepWorktrees(
            project,
            linked.map((w) => groupOf(m(), w)),
          ),
        ),
    };
  return t;
}

/** What the window can do to `t` now, verb by verb. */
export function verbTable(t: Target): VerbTable {
  switch (t.kind) {
    case 'agent': {
      const a = agentOf(t.id);
      return a ? agentVerbs(a, t.viaSub) : {};
    }
    case 'terminal': {
      const x = terminalOf(t.id);
      return x ? terminalVerbs(x) : {};
    }
    case 'worktree': {
      const w = worktreeOf(t.id);
      return w ? worktreeVerbs(w) : {};
    }
    case 'project':
      return projectVerbs(t.id);
  }
}

/** Run verb `id` on `t`, as its menu row does. False when it means
 *  nothing there. */
export function runVerb(t: Target, id: ActionId): boolean {
  const v = verbTable(t)[id];
  if (v?.disabled && v.why) toast(v.why);
  if (!v || v.disabled) return false;
  void v.run();
  return true;
}

function worktreeItems(w: Worktree | undefined): MenuEntry[] {
  if (!w) return [];
  return [{ heading: `${w.is_main ? 'checkout' : 'worktree'} ${w.branch}` }, ...rows('worktree', worktreeVerbs(w))];
}

/** Move up / Move down, the keyboard's (and the menu's) drag. */
export function moveItems(t: { kind: string; id: string }, where: StepIn = 'tree'): MenuEntry[] {
  const on: AppliesTo = t.kind === 'project' ? 'repo' : t.kind === 'worktree' ? 'worktree' : 'session';
  return [entry('reorder_up', on, { run: () => moveStep(t, -1, where) }), entry('reorder_down', on, { run: () => moveStep(t, 1, where) })];
}

/** `where`: the board orders a lane's cards, the overview a tree's rows;
 *  by default wherever the focus is as the menu opens. */
export function agentMenu(a: Agent, viaSub = false, where: StepIn = stepInFor()): MenuEntry[] {
  const w = worktreeOf(a.worktree_id);
  // From a subagent's row every verb is the parent's: say so up top.
  const head: MenuEntry[] = viaSub ? [{ heading: `Parent session: ${a.name}` }] : [];
  const verbs = rows('session', agentVerbs(a, viaSub, where));
  if (a.archived) return [...head, ...verbs];
  // The window's own, ahead of the rows that take the session away.
  const extra: MenuEntry[] = [
    ...(a.unseen ? [{ label: 'Mark seen', run: () => seeAgent(a.id) }] : []),
    ...orchestratorMenuItems(a).filter((e) => typeof e === 'object' && 'label' in e && e.label === 'Review\u2026'),
  ];
  const at = verbs.findIndex((e) => e.danger || e.label === menuLabel('archive', 'session') || e.label === 'Archive parent session\u2026');
  const cut = at < 0 ? verbs.length : at;
  // The checkout's rows go on after, less the verbs the session's already
  // offered (its pull request, Focus here), as the terminal app's do.
  const said = new Set(verbs.flatMap((e) => (typeof e === 'object' && 'label' in e ? [e.label] : [])));
  const checkout = worktreeItems(w).filter((e) => !(typeof e === 'object' && 'label' in e && said.has(e.label)));
  return [...head, ...verbs.slice(0, cut), ...extra, ...verbs.slice(cut), 'sep', ...checkout];
}

export function terminalMenu(t: TerminalTab): MenuEntry[] {
  return [...rows('terminal', terminalVerbs(t)), 'sep', ...worktreeItems(worktreeOf(t.worktree_id))];
}

export function projectMenu(project: string): MenuEntry[] {
  const linked = m()
    .worktrees.filter((w) => w.project_id === project && !w.is_main)
    .sort((a, b) => a.sort_order - b.sort_order);
  const out: MenuEntry[] = rows('repo', projectVerbs(project));
  // The window's own: the repo's default account after the reload, the
  // folder's location before the remove.
  const at = (id: ActionId) => out.findIndex((e) => typeof e === 'object' && 'label' in e && e.label === menuLabel(id, 'repo'));
  const refresh = at('refresh_pull_requests');
  out.splice(refresh < 0 ? out.length : refresh + 1, 0, { label: 'Default account\u2026', run: () => void pickProjectAccount(project) });
  const remove = at('remove_project');
  out.splice(remove < 0 ? out.length : remove, 0, { label: 'Locate folder\u2026', run: () => void locateProject(project) });
  if (linked.length) {
    out.push('sep', { heading: 'worktrees' });
    for (const w of linked) {
      out.push({ label: `Open ${w.branch} in editor`, run: () => void openEditor(w.id) });
      out.push({ label: `Delete ${w.branch}\u2026`, run: () => void deleteWorktree(w.id), danger: true });
    }
  }
  return out;
}

export function menuFor(t: Target): MenuEntry[] {
  switch (t.kind) {
    case 'agent': {
      const a = agentOf(t.id);
      return a ? agentMenu(a, t.viaSub) : [];
    }
    case 'terminal': {
      const x = terminalOf(t.id);
      return x ? terminalMenu(x) : [];
    }
    case 'worktree':
      return worktreeItems(worktreeOf(t.id));
    case 'project':
      return projectMenu(t.id);
  }
}

/** A row key from the shortcut table, on its target. True when handled. */
export function runRowKey(id: ShortcutId, t: Target): boolean {
  const a = t.kind === 'agent' ? agentOf(t.id) : undefined;
  const term = t.kind === 'terminal' ? terminalOf(t.id) : undefined;
  const wt = a?.worktree_id ?? term?.worktree_id ?? (t.kind === 'worktree' ? t.id : t.kind === 'project' ? mainOf(t.id)?.id : undefined);
  const table: Partial<Record<ShortcutId, () => unknown>> = a
    ? {
        rename: () => renameAgent(a.id),
        archive: () => archiveAgent(a.id, t.kind === 'agent' && t.viaSub),
        unarchive: () => unarchiveAgent(a.id),
        delete: () => deleteAgent(a.id),
        restart: () => restartAgent(a.id),
        move: () => moveAgent(a.id),
        duplicate: () => duplicateAgent(a.id),
        follow_up: () => followUp(a.id),
      }
    : term
      ? { rename: () => renameTerminal(term.id), delete: () => closeTerminal(term.id) }
      : {};
  if (t.kind === 'project') table.rename = () => renameProject(t.id);
  Object.assign(table, {
    new_terminal: () => newTerminal(wt),
    open_editor: () => openEditor(wt),
    open_remote: () => openRemote(wt),
    // The row's repo: only its root checkout switches.
    switch_branch: () => {
      const p = m().worktrees.find((w) => w.id === wt)?.project_id;
      if (p && canSwitchBranch(p)) openBranchSwitch(p);
      else toast('Switch branch is for repos without an Azure DevOps remote: their branches live in the ADO views.');
    },
    // Shift+V, as in the terminal app: the checkout's pull request page.
    open_pr: () => {
      const pr = wt ? m().prs[wt] : undefined;
      if (pr) api.openUrl(pr.url).catch((e) => toast(String(e)));
      else toast('No pull request on this branch yet.');
    },
  });
  const f = table[id];
  if (!f) return false;
  f();
  return true;
}

/** The label a session goes by in lists: `repo / branch / name`. */
export function sessionPath(model: Model, a: Agent): string {
  const w = model.worktrees.find((x) => x.id === a.worktree_id);
  const p = model.projects.find((x) => x.id === w?.project_id);
  return [p?.name, w && !w.is_main ? w.branch : null, a.name].filter(Boolean).join(' / ');
}
