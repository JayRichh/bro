// What a worktree row offers, as buttons and as its right-click menu:
// the overview's worktree groups and the Worktrees view share this list.
// Each verb is the one the session menus already run (actions.ts); this
// adds the ones only a checkout has: a session started there, its folder,
// its path, and the bulk delete of merged or clean checkouts, which goes
// through the same non-forced `git worktree remove` the single delete
// does without Force, so git still refuses anything with work in it.

import { api, type Worktree } from '../api';
import { app, focusQuickPrompt, openDiff, toast } from '../store.svelte';
import { ask, type MenuEntry } from '../ui/ui.svelte';
import { keyLabel, keysOf, type ShortcutId } from '../shortcuts';
import { deleteWorktree, moveItems, newTerminal, openEditor, openRemote } from '../actions';
import { canSwitchBranch, openBranchSwitch } from './switcher.svelte';
import { worktreeApi, type CheckoutState } from './api';
import { stateLines } from './offer';
import { sweepCandidates, sweepReason, type Sweep, type WorktreeGroup } from './groups';
import { refreshGit } from './rows.svelte';
import { menuLabel } from '../verbs';

/** A registry verb's words on a worktree: one set for both apps. */
const say = (id: Parameters<typeof menuLabel>[0]) => menuLabel(id, 'worktree');

async function attempt(what: string, f: () => Promise<unknown>) {
  try {
    await f();
  } catch (e) {
    toast(`${what}: ${e}`);
  }
}

export function newSessionIn(w: Worktree) {
  focusQuickPrompt({ project: w.project_id, worktree: w.id, newWorktree: false });
}

export function revealFolder(w: Worktree) {
  return attempt('Could not open the folder', () => worktreeApi.reveal(w.id));
}

export async function copyPath(w: Worktree) {
  try {
    await navigator.clipboard.writeText(w.path);
    toast(`Copied ${w.path}`);
  } catch (e) {
    toast(`Could not copy the path: ${e}`);
  }
}

export function isRunning(w: Worktree): boolean {
  return app.model.terminals.some((t) => t.worktree_id === w.id && !!t.run_command && t.alive);
}

export function toggleRun(w: Worktree) {
  const running = isRunning(w);
  return attempt(running ? 'Could not stop the run' : 'Could not start the run', () =>
    running ? api.stopRun(w.id) : api.startRun(w.id),
  );
}

/** One verb a worktree row shows: its label, the key that runs it on a
 *  focused row (when there is one), and whether it is for this row. */
export type Verb = {
  id: string;
  label: string;
  /** Short word for the hover buttons. */
  short: string;
  run: () => unknown;
  key?: ShortcutId;
  /** A key the row itself answers (not in the shortcut table). */
  rowKey?: string;
  danger?: boolean;
};

export function verbsFor(w: Worktree): Verb[] {
  const out: Verb[] = [
    { id: 'session', label: say('quick_prompt'), short: 'session', run: () => newSessionIn(w), rowKey: 's' },
    { id: 'terminal', label: say('new_terminal'), short: 'terminal', run: () => newTerminal(w.id), key: 'new_terminal' },
    { id: 'diff', label: say('git_diff'), short: 'diff', run: () => openDiff(w.id), rowKey: 'd' },
  ];
  if (w.is_main && canSwitchBranch(w.project_id))
    out.push({ id: 'branch', label: say('switch_branch'), short: 'branch', run: () => openBranchSwitch(w.project_id), key: 'switch_branch' });
  out.push(
    { id: 'editor', label: say('open_worktree'), short: 'editor', run: () => openEditor(w.id), key: 'open_editor' },
    { id: 'folder', label: 'Open folder', short: 'folder', run: () => revealFolder(w) },
    { id: 'copy', label: 'Copy path', short: 'copy path', run: () => copyPath(w) },
  );
  if (app.runCommands[w.id] || isRunning(w))
    out.push({ id: 'run', label: isRunning(w) ? 'Stop run' : say('toggle_run'), short: isRunning(w) ? 'stop' : 'run', run: () => toggleRun(w) });
  if (!w.is_main)
    out.push({ id: 'delete', label: say('delete_worktree'), short: 'delete', run: () => deleteWorktree(w.id), rowKey: 'delete', danger: true });
  return out;
}

/** The row's right-click menu. */
export function worktreeMenu(w: Worktree): MenuEntry[] {
  const verbs = verbsFor(w);
  const entries: MenuEntry[] = [{ heading: `${w.is_main ? 'checkout' : 'worktree'} ${w.branch}` }];
  for (const v of verbs) {
    if (v.id === 'delete') entries.push('sep');
    entries.push({ label: v.label, run: () => void v.run(), keys: v.key ? keysOf(v.key) : v.rowKey ? [v.rowKey] : undefined, danger: v.danger });
    if (v.id === 'copy') {
      entries.push({ label: say('open_repo'), run: () => void openRemote(w.id), keys: keysOf('open_remote') });
      // The keyboard's drag of the group (the root stays first).
      if (!w.is_main) entries.push(...moveItems({ kind: 'worktree', id: w.id }));
    }
  }
  return entries;
}

/** The tooltip a verb button carries: what it does, and its key. */
export function verbTitle(v: Verb): string {
  const k = v.key ? keysOf(v.key)[0] : v.rowKey;
  return k ? `${v.label} (${keyLabel(k)} on the row)` : v.label;
}

/** Delete the repo's merged and clean checkouts, after one confirm that
 *  lists them. Each goes without Force: one that picked up work since the
 *  confirm is refused by git and said so. */
export async function sweepWorktrees(project: string, groups: WorktreeGroup[]) {
  const candidates = sweepCandidates(groups);
  if (!candidates.length) return toast('No worktree here is free to delete: each has a session or terminal in it.');
  toast('Reading the worktrees\u2026');
  const states = await Promise.all(
    candidates.map((g) => worktreeApi.checkoutState(g.worktree.id).catch(() => null as CheckoutState | null)),
  );
  const picked: { g: WorktreeGroup; why: Sweep; state: CheckoutState }[] = [];
  candidates.forEach((g, i) => {
    const why = sweepReason(g, states[i]);
    if (why && states[i]) picked.push({ g, why, state: states[i]! });
  });
  if (!picked.length)
    return toast('Nothing to clean: every free worktree has uncommitted work, unpushed commits or an open PR.');
  const archived = (g: WorktreeGroup) => app.model.agents.filter((a) => a.worktree_id === g.worktree.id && a.archived).length;
  const r = await ask.confirm({
    title: `Delete ${picked.length} worktree${picked.length === 1 ? '' : 's'}`,
    message:
      'These have no session or terminal, no uncommitted work, and their commits are on a remote or their PR merged. Their folders go; the branches stay.',
    detail: picked.map(({ g, why, state }) => {
      const a = archived(g);
      return `${g.worktree.branch}  \u00b7  ${why === 'merged' ? 'PR merged' : 'clean'}  \u00b7  ${stateLines(state).join(', ')}${a ? `  \u00b7  ${a} archived session${a === 1 ? '' : 's'} go with it` : ''}`;
    }),
    confirm: 'Delete',
    danger: true,
  });
  if (!r.ok) return;
  let done = 0;
  const failed: string[] = [];
  for (const { g } of picked) {
    // A session started there while the confirm was up keeps it.
    const now = app.model.worktrees.find((w) => w.id === g.worktree.id);
    const live =
      app.model.agents.some((a) => a.worktree_id === g.worktree.id && !a.archived) ||
      app.model.terminals.some((t) => t.worktree_id === g.worktree.id);
    if (!now || live) continue;
    try {
      await api.deleteWorktree(g.worktree.id, false);
      done += 1;
    } catch (e) {
      failed.push(`${g.worktree.branch}: ${e}`);
    }
  }
  toast(
    `Deleted ${done} worktree${done === 1 ? '' : 's'}.` + (failed.length ? ` Kept ${failed.length}: ${failed.join('; ')}` : ''),
  );
  refreshGit(
    app.model.worktrees.filter((w) => w.project_id === project).map((w) => w.id),
    true,
  );
}
