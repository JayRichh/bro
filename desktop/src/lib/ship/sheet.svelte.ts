// What the ship sheet runs: read where a worktree stands (`ship_status`),
// open its pull request from the composer, merge it after the confirm,
// and the buttons a ship error carries (sign in, install gh, finish in
// ADO). The forge is whichever the remote is: gh for GitHub, kete-ado for
// Azure DevOps, both behind the same three commands.

import { api } from '../api';
import { app, openSession, toast } from '../store.svelte';
import { worktreeApi } from '../worktrees/api';
import { signin } from '../ado/signin.svelte';
import { shipSheet } from './state.svelte';
import { asShipError, createBlocked, newPrOf, seedForm, type ErrorAction } from './ship';

let seq = 0;

/** Read the worktree's ship status into the sheet; the composer's form
 *  starts from it once per opening. */
async function load(seed: boolean) {
  const mine = ++seq;
  shipSheet.loading = true;
  shipSheet.error = null;
  try {
    const st = await api.shipStatus(shipSheet.worktree, shipSheet.session);
    if (mine !== seq) return;
    shipSheet.status = st;
    shipSheet.error = st.error;
    if (seed) shipSheet.form = seedForm(st);
  } catch (e) {
    if (mine === seq) shipSheet.error = asShipError(e);
  } finally {
    if (mine === seq) shipSheet.loading = false;
  }
}

function open(mode: 'compose' | 'merge', worktree: string, session: string | null) {
  shipSheet.mode = mode;
  shipSheet.worktree = worktree;
  shipSheet.session = session;
  shipSheet.status = null;
  shipSheet.error = null;
  shipSheet.note = null;
  shipSheet.busy = false;
  shipSheet.form = { title: '', body: '', base: '', draft: false };
  shipSheet.merge = { method: 'squash', deleteBranch: true };
  void load(mode === 'compose');
}

/** The PR composer for `worktree`, its title and body suggested from the
 *  session's words and the commits. */
export function openComposer(worktree: string, session: string | null = null) {
  open('compose', worktree, session);
}

/** The merge confirm for `worktree`'s open pull request. */
export function openMerge(worktree: string, session: string | null = null) {
  open('merge', worktree, session);
}

export function closeShip() {
  seq++;
  shipSheet.mode = null;
}

/** Ask for the pull requests again, so the card moves without waiting
 *  for the next poll. */
function nudgePrs() {
  worktreeApi.refreshPrs().catch(() => {});
}

export async function createPr() {
  const st = shipSheet.status;
  if (shipSheet.busy || createBlocked(shipSheet.form, st)) return;
  shipSheet.busy = true;
  shipSheet.error = null;
  try {
    const made = await api.shipCreatePr(shipSheet.worktree, newPrOf(shipSheet.form, st));
    nudgePrs();
    closeShip();
    toast(
      (made.existing ? `Pushed to the open pull request #${made.pr.number}` : `Opened pull request #${made.pr.number}`) +
        (made.note ? `. ${made.note}` : '.'),
    );
  } catch (e) {
    // The form stays as typed.
    shipSheet.error = asShipError(e);
  } finally {
    shipSheet.busy = false;
  }
}

export async function mergePr() {
  if (shipSheet.busy || !shipSheet.status?.pr) return;
  shipSheet.busy = true;
  shipSheet.error = null;
  try {
    const done = await api.shipMerge(shipSheet.worktree, { ...shipSheet.merge });
    nudgePrs();
    const n = done.pr.number;
    const what = done.state === 'queued' ? `Pull request #${n} is queued to merge behind its checks` : `Merged pull request #${n}`;
    const branch = done.branchDeleted ? '; its remote branch is deleted' : '';
    closeShip();
    toast(`${what}${branch}.${done.note ? ` ${done.note}` : ''}`);
  } catch (e) {
    shipSheet.error = asShipError(e);
  } finally {
    shipSheet.busy = false;
  }
}

/** An error's button: sign in (ADO's sign-in, or `gh auth login` in a
 *  terminal shown in the pane), or the page to read. */
export async function runErrorAction(a: ErrorAction) {
  if (a.kind === 'open') return api.openUrl(a.url).catch((e) => toast(String(e)));
  const wt = shipSheet.worktree;
  const forge = a.forge ?? shipSheet.status?.forge ?? null;
  if (forge === 'ado') {
    if (await signin.signIn(wt)) void load(false);
    else if (signin.error) toast(`Could not sign in: ${signin.error}`);
    return;
  }
  try {
    const terminal = await api.shipGhSignIn(wt);
    closeShip();
    openSession({ Terminal: terminal });
    toast('Sign in to GitHub in this terminal, then open the pull request again.');
  } catch (e) {
    toast(`Could not start gh auth login: ${e}`);
  }
}

/** The worktree's place, for the sheet's title. */
export function sheetPlace(): string {
  const w = app.model.worktrees.find((x) => x.id === shipSheet.worktree);
  const p = w && app.model.projects.find((x) => x.id === w.project_id);
  return w ? `${p?.name ?? ''} / ${w.branch}` : '';
}
