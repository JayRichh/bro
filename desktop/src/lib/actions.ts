// What can be done to a session, a terminal, a worktree or a repo: one
// list per target, used by the context menus (overview rows, board cards,
// the session header, repo headers), the row keys and the command
// palette. Destructive verbs confirm in the window first, as the terminal
// app's do: delete always, archive when Confirm on archive is on or the
// session has sessions nested under it (they go with it).

import { api, type Agent, type Model, type TerminalTab, type Worktree } from './api';
import { app, focusQuickPrompt, markSeen, openDiff, openSession, projectOf, toast } from './store.svelte';
import { ask, type MenuEntry } from './ui/ui.svelte';
import { keysOf, type ShortcutId } from './shortcuts';
import { orchestratorMenuItems } from './orch.svelte';

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
export async function archiveAgent(id: string, viaSub = false) {
  const a = agentOf(id);
  if (!a || a.archived) return;
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
    if (!r.ok) return;
  }
  // The model may have moved while the dialog was up: act on it as it is now.
  const now = agentOf(id);
  if (!now || now.archived) return;
  const thread = descendants(m(), id).filter((k) => !k.archived);
  await attempt('Could not archive', async () => {
    for (const x of [now, ...thread]) await api.archiveAgent(x.id);
  });
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
  const r = await ask.confirm({
    title: kids.length ? `Delete thread \u00b7 ${kids.length + 1} sessions` : 'Delete session',
    message: kids.length
      ? `Delete "${a.name}" and everything nested under it? Their history goes too.`
      : `Delete "${a.name}"? Its history goes too; this can't be undone.`,
    detail: kids.length ? [a.name, ...kids.map((k) => k.name)] : undefined,
    confirm: 'Delete',
    danger: true,
  });
  if (!r.ok) return;
  // Sessions spawned while the dialog was up go too: recount now.
  const now = agentOf(id);
  if (!now) return;
  const gone = descendants(m(), id);
  if (app.session && 'Agent' in app.session && [now, ...gone].some((x) => x.id === (app.session as { Agent: string }).Agent))
    app.session = null;
  await attempt('Could not delete', async () => {
    for (const x of [...gone].reverse().concat(now)) await api.deleteAgent(x.id);
  });
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
  if (t.alive) {
    const r = await ask.confirm({
      title: 'Close terminal',
      message: `Close "${t.run_command ? 'run' : t.name}"? Its shell and anything running in it are killed.`,
      confirm: 'Close',
      danger: true,
    });
    if (!r.ok) return;
  }
  await attempt('Could not close', () => api.closeTerminal(id));
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
    const id = await api.createWorktree(project, v.branch.trim() || suggestion, v.base.trim() || null);
    toast(`Worktree ready: ${worktreeOf(id)?.branch ?? v.branch}`);
    focusQuickPrompt({ project, worktree: id, newWorktree: false });
  });
}

export async function deleteWorktree(id: string) {
  const w = worktreeOf(id);
  if (!w || w.is_main) return;
  const live = m().agents.filter((a) => a.worktree_id === id && !a.archived).length;
  const r = await ask.confirm({
    title: 'Delete worktree',
    message: `Delete the worktree on ${w.branch}? Its folder is removed; the branch stays.`,
    detail: [w.path, ...(live ? [`${live} session${live === 1 ? '' : 's'} in it`] : [])],
    confirm: 'Delete',
    danger: true,
    check: 'Force: delete even with uncommitted changes',
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
        toast('The repo's open command changed before it ran. Open again to review it.');
        return;
      }
    }
    if (r.opened) toast(`Opened with: ${r.command}`);
  });
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

const item = (label: string, run: () => void, id?: ShortcutId, danger = false) => ({
  label,
  run,
  keys: id ? keysOf(id) : undefined,
  danger,
});

function worktreeItems(w: Worktree | undefined): MenuEntry[] {
  if (!w) return [];
  const run = app.runCommands[w.id];
  const running = m().terminals.some((t) => t.worktree_id === w.id && t.run_command && t.alive);
  const out: MenuEntry[] = [{ heading: `${w.is_main ? 'checkout' : 'worktree'} ${w.branch}` }];
  if (run)
    out.push(
      item(running ? 'Stop run' : 'Run', () =>
        attempt(running ? 'Could not stop' : 'Could not run', () => (running ? api.stopRun(w.id) : api.startRun(w.id))),
      ),
    );
  out.push(
    item('New terminal', () => newTerminal(w.id), 'new_terminal'),
    item('Diff', () => openDiff(w.id)),
    item('Open in editor', () => openEditor(w.id), 'open_editor'),
    item('Open remote in browser', () => openRemote(w.id), 'open_remote'),
  );
  if (!w.is_main) out.push(item('Delete worktree\u2026', () => deleteWorktree(w.id), undefined, true));
  return out;
}

export function agentMenu(a: Agent, viaSub = false): MenuEntry[] {
  const w = worktreeOf(a.worktree_id);
  // From a subagent's row every verb is the parent's: say so up top.
  const head: MenuEntry[] = viaSub ? [{ heading: `Parent session: ${a.name}` }] : [];
  if (a.archived)
    return [
      ...head,
      item('Unarchive', () => unarchiveAgent(a.id), 'unarchive'),
      item('Duplicate', () => duplicateAgent(a.id), 'duplicate'),
      'sep',
      item('Delete\u2026', () => deleteAgent(a.id), 'delete', true),
    ];
  const out: MenuEntry[] = [
    ...head,
    item('Open', () => openSession({ Agent: a.id })),
    item('Follow-up prompt\u2026', () => followUp(a.id), 'follow_up'),
    item('Restart agent', () => restartAgent(a.id), 'restart'),
    item('Duplicate', () => duplicateAgent(a.id), 'duplicate'),
    item('Move to\u2026', () => moveAgent(a.id), 'move'),
    item('Rename\u2026', () => renameAgent(a.id), 'rename'),
  ];
  if (a.unseen) out.push(item('Mark seen', () => seeAgent(a.id)));
  out.push(...orchestratorMenuItems(a));
  out.push(item(viaSub ? 'Archive parent session\u2026' : 'Archive', () => archiveAgent(a.id, viaSub), 'archive'), item('Delete\u2026', () => deleteAgent(a.id), 'delete', true));
  return [...out, 'sep', ...worktreeItems(w)];
}

export function terminalMenu(t: TerminalTab): MenuEntry[] {
  return [
    item('Open', () => openSession({ Terminal: t.id })),
    item('Rename\u2026', () => renameTerminal(t.id), 'rename'),
    item('Close terminal\u2026', () => closeTerminal(t.id), 'delete', true),
    'sep',
    ...worktreeItems(worktreeOf(t.worktree_id)),
  ];
}

export function projectMenu(project: string): MenuEntry[] {
  const main = mainOf(project);
  const linked = m()
    .worktrees.filter((w) => w.project_id === project && !w.is_main)
    .sort((a, b) => a.sort_order - b.sort_order);
  const out: MenuEntry[] = [
    item('New session here', () => focusQuickPrompt({ project })),
    item('New worktree\u2026', () => newWorktree(project), 'new_worktree'),
    item('Open in editor', () => openEditor(main?.id)),
    item('Open remote in browser', () => openRemote(main?.id)),
    'sep',
    item('Rename\u2026', () => renameProject(project)),
    item('Locate folder\u2026', () => locateProject(project)),
    item('Remove from Kete\u2026', () => removeProject(project), undefined, true),
  ];
  if (linked.length) {
    out.push('sep', { heading: 'worktrees' });
    for (const w of linked) {
      out.push(item(`Open ${w.branch} in editor`, () => openEditor(w.id)));
      out.push(item(`Delete ${w.branch}\u2026`, () => deleteWorktree(w.id), undefined, true));
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
  Object.assign(table, {
    new_terminal: () => newTerminal(wt),
    open_editor: () => openEditor(wt),
    open_remote: () => openRemote(wt),
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
