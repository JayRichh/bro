// The branch switcher's state and git calls: open on a repo's root
// checkout, list its branches (a cached list paints at once), fetch in the
// background at most once a minute per checkout and list again when it
// lands, and run one switch at a time. The window it draws in is
// BranchSwitch.svelte; the rules it follows are branches.ts.

import { app, toast } from '../store.svelte';
import { ask, ui } from '../ui/ui.svelte';
import { worktreeApi, type Branch, type Carry, type Change, type Outcome } from './api';
import { fetchDue, localName } from './branches';

export type Stage =
  | { kind: 'pick' }
  /** The checkout has changes: how should they travel to `target`? */
  | { kind: 'dirty'; target: { name: string; remote: boolean }; files: Change[]; keys: string[]; armed: boolean }
  | { kind: 'working'; what: string };

export const sw = $state({
  project: '',
  worktree: '',
  branches: [] as Branch[],
  /** The checkout's uncommitted change count; null when git couldn't say. */
  changes: null as number | null,
  listed: false,
  fetching: false,
  stage: { kind: 'pick' } as Stage,
  status: null as { text: string; error: boolean } | null,
});

/** Each checkout's last listing, so a reopen paints at once. */
const lists = new Map<string, Branch[]>();
/** When each checkout last started a fetch (ms). */
const fetched = new Map<string, number>();

/** The repo's root checkout. */
function rootOf(project: string) {
  return app.model.worktrees.find((w) => w.project_id === project && w.is_main);
}

/** A repo whose root checkout the switcher can move: not an Azure DevOps
 *  one (its branches live in the ADO views). */
export function canSwitchBranch(project: string): boolean {
  return !app.adoCtx[project] && !!rootOf(project);
}

export function openBranchSwitch(project: string) {
  const root = rootOf(project);
  if (!root) return toast('This repo has no root checkout to switch.');
  sw.project = project;
  sw.worktree = root.id;
  sw.branches = lists.get(root.id) ?? [];
  sw.listed = lists.has(root.id);
  sw.changes = null;
  sw.stage = { kind: 'pick' };
  sw.status = null;
  ui.branchSwitch = true;
  list(root.id);
  maybeFetch(root.id);
}

/** From the palette with no repo in view: pick one first. */
export async function pickBranchSwitch() {
  const items = app.model.projects.filter((p) => canSwitchBranch(p.id)).map((p) => ({ id: p.id, label: p.name, hint: rootOf(p.id)?.branch }));
  if (!items.length) return toast('No repo here can switch branches.');
  const p = items.length === 1 ? items[0].id : await ask.pick({ title: 'Switch branch in', items });
  if (p) openBranchSwitch(p);
}

export function closeBranchSwitch() {
  if (sw.stage.kind === 'working') return;
  ui.branchSwitch = false;
}

async function list(worktree: string) {
  try {
    const r = await worktreeApi.branchList(worktree);
    lists.set(worktree, r.branches);
    if (sw.worktree !== worktree) return;
    sw.branches = r.branches;
    sw.changes = r.changes;
    sw.listed = true;
  } catch (e) {
    if (sw.worktree === worktree) sw.status = { text: String(e), error: true };
  }
}

function maybeFetch(worktree: string) {
  const now = Date.now();
  if (!fetchDue(fetched.get(worktree), now)) return;
  fetched.set(worktree, now);
  if (sw.worktree === worktree) sw.fetching = true;
  worktreeApi
    .branchFetch(worktree)
    .then((ok) => {
      if (sw.worktree !== worktree) return;
      if (!ok) sw.status = { text: 'Fetch failed: showing the branches already here.', error: false };
      return list(worktree);
    })
    .catch(() => {})
    .finally(() => {
      if (sw.worktree === worktree) sw.fetching = false;
    });
}

/** Switch to `target`, its changes travelling as `carry` says. */
export async function runSwitch(target: { name: string; remote: boolean }, carry: Carry) {
  const worktree = sw.worktree;
  const prev = sw.stage;
  sw.stage = { kind: 'working', what: carry.kind === 'create' ? `Creating ${target.name}\u2026` : `Switching to ${localName(target)}\u2026` };
  sw.status = null;
  let out: Outcome;
  try {
    out = await worktreeApi.branchSwitch(worktree, target, carry);
  } catch (e) {
    sw.stage = prev.kind === 'working' ? { kind: 'pick' } : prev;
    sw.status = { text: String(e), error: true };
    return;
  }
  if (sw.worktree !== worktree) return;
  switch (out.kind) {
    case 'switched':
      ui.branchSwitch = false;
      sw.stage = { kind: 'pick' };
      toast(`On ${out.branch}${out.note ? `: ${out.note}` : '.'}`);
      list(worktree);
      return;
    case 'dirty':
      sw.stage = { kind: 'dirty', target, files: out.files, keys: out.keys, armed: false };
      if (carry.kind === 'discard') sw.status = { text: 'The changes moved since they were shown: check them again.', error: true };
      return;
    case 'failed':
      // Back to where it was asked from, the checkout as it was.
      sw.stage = prev.kind === 'dirty' ? { ...prev, armed: false } : { kind: 'pick' };
      sw.status = { text: out.message, error: true };
      return;
    case 'stopped':
      // The checkout changed: the question's list is stale.
      sw.stage = { kind: 'pick' };
      sw.status = { text: out.message, error: true };
      list(worktree);
      return;
  }
}
