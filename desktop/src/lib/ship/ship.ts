// Ship on the board, the pure half: what a drop's verdict means for the
// window, what the PR composer starts from and sends, what a ship error
// offers as its button, and which review verbs a card has. The sheet
// (ShipSheet.svelte) and the board call these; the rules for which drop
// does what are nebula-core's (`Card.drops`), never worked out here.

import type { DropTarget, DropVerb, ForgeKind, Lane, PrInfo, ShipError, ShipNewPr, ShipStatus } from '../api';

// ---- drops ----

/** The verdict for letting a card go on `to`, from the drops snapshotted
 *  when the drag began; a model without one refuses. */
export function dropVerbOf(drops: Partial<Record<DropTarget, DropVerb>> | undefined, to: DropTarget): DropVerb {
  return drops?.[to] ?? { act: 'refuse', reason: 'This card cannot go there.' };
}

/** How a lane looks while a card is over it: `act` (ember: a drop does
 *  something), `refuse` (dimmed, the reason shown), or `none` (its own
 *  lane, where only a drop beside a card reorders). */
export function laneLook(drops: Partial<Record<DropTarget, DropVerb>> | undefined, from: Lane, to: DropTarget): 'act' | 'refuse' | 'none' {
  if (to === from) return 'none';
  const v = dropVerbOf(drops, to);
  if (v.act === 'refuse') return 'refuse';
  return v.act === 'reorder' ? 'none' : 'act';
}

/** What a drop says it will do, on the lane while the card is over it. */
export function dropHint(v: DropVerb): string {
  switch (v.act) {
    case 'mark_seen':
      return 'Drop to mark it seen';
    case 'open_pr':
      return 'Drop to open a pull request';
    case 'merge_pr':
      return 'Drop to merge its pull request';
    case 'archive':
      return 'Drop to archive it';
    case 'archive_and_offer_worktree_delete':
      return 'Drop to archive it (and offer its worktree)';
    case 'refuse':
      return v.reason;
    case 'reorder':
      return '';
  }
}

// ---- errors ----

/** A ship command's rejection as a {@link ShipError}: the commands reject
 *  with one, anything else (the bridge, a string) becomes `other`. */
export function asShipError(e: unknown): ShipError {
  if (e && typeof e === 'object' && 'kind' in e && 'message' in e) {
    const x = e as Partial<ShipError>;
    return { kind: x.kind ?? 'other', forge: x.forge ?? null, message: String(x.message ?? ''), url: x.url ?? null };
  }
  const message = e instanceof Error ? e.message : String(e);
  return { kind: 'other', forge: null, message, url: null };
}

export const GH_INSTALL_URL = 'https://cli.github.com/';

/** The button an error carries: sign in, how to install gh, or the page
 *  to finish on. None for the rest: the message says it. */
export type ErrorAction =
  | { kind: 'signIn'; label: string; forge: ForgeKind | null }
  | { kind: 'open'; label: string; url: string };

export function errorAction(e: ShipError): ErrorAction | null {
  switch (e.kind) {
    case 'signIn':
      return { kind: 'signIn', label: 'Sign in', forge: e.forge };
    case 'noTool':
      return { kind: 'open', label: 'How to install gh', url: GH_INSTALL_URL };
    case 'mergeInAdo':
      return e.url ? { kind: 'open', label: 'Open in ADO', url: e.url } : null;
    default:
      return e.url ? { kind: 'open', label: 'Open', url: e.url } : null;
  }
}

export function forgeName(f: ForgeKind | null | undefined): string {
  return f === 'ado' ? 'Azure DevOps' : f === 'github' ? 'GitHub' : 'no forge';
}

// ---- the composer ----

export interface ComposerForm {
  title: string;
  body: string;
  /** The branch it goes into; the review base to start. */
  base: string;
  draft: boolean;
}

/** What the composer starts from: the suggestion, else the branch. */
export function seedForm(st: ShipStatus): ComposerForm {
  return {
    title: st.suggestion?.title ?? st.branch ?? '',
    body: st.suggestion?.body ?? '',
    base: st.base ?? '',
    draft: false,
  };
}

/** The request the form sends: the base only when it was changed (null
 *  takes the review base). */
export function newPrOf(form: ComposerForm, st: ShipStatus | null): ShipNewPr {
  const base = form.base.trim();
  return {
    title: form.title.trim(),
    body: form.body,
    draft: form.draft,
    base: base && base !== st?.base ? base : null,
  };
}

/** An open (or draft) pull request: one a merge can take. */
export function isOpen(pr: { state?: string } | null | undefined): boolean {
  return !!pr && (!pr.state || pr.state === 'open');
}

/** Why Create can't go yet; null when it can. */
export function createBlocked(form: ComposerForm, st: ShipStatus | null): string | null {
  if (!st) return 'Reading the branch\u2026';
  if (!st.forge) return 'The remote is on neither GitHub nor Azure DevOps.';
  if (!st.branch) return 'The worktree is on a detached HEAD: check out a branch first.';
  if (isOpen(st.pr)) return 'This branch already has an open pull request.';
  if (!form.title.trim()) return 'A pull request needs a title.';
  if (st.base && st.branch === (form.base.trim() || st.base)) return 'The branch is the base: a pull request needs a branch of its own.';
  return null;
}

// ---- the review verbs ----

export interface ReviewVerbs {
  /** Open the composer, or view the open PR; `why` greys it out. */
  pr: { kind: 'open' | 'view'; why: string | null };
  /** Merge the open PR: null with none open; `why` greys it out. */
  merge: { why: string | null } | null;
  /** A linked worktree: the safe delete is offered. */
  deleteWorktree: boolean;
}

/** Why a checkout on its base branch opens no pull request. */
export const ON_BASE_BRANCH = 'The checkout is on the base branch: a pull request needs a branch of its own.';

/** The verbs a session's review row and menu offer, from its pull request
 *  (`model.prs`), core's verdict for a drop on Ship, whether its
 *  checkout is the repo's root, and whether it is on the base branch (its
 *  git state's `on_default_branch`; unknown counts as not, and the
 *  composer still refuses). The terminal app leaves Create out there. */
export function reviewVerbs(pr: PrInfo | null | undefined, toShip: DropVerb | undefined, isMain: boolean, onBase = false): ReviewVerbs {
  const open = isOpen(pr);
  // Only the base branch rules a pull request out for sure; the other
  // refusals (the agent's lane, nothing pushed yet) leave the composer to
  // say what the forge says.
  const why = toShip?.act === 'refuse' && /base branch/.test(toShip.reason) ? toShip.reason : onBase ? ON_BASE_BRANCH : null;
  return {
    pr: open ? { kind: 'view', why: null } : { kind: 'open', why },
    merge: open ? { why: pr?.draft ? 'Its pull request is a draft: mark it ready for review before it can merge.' : null } : null,
    deleteWorktree: !isMain,
  };
}

/** A pull request's chip on a card: `PR #12`, `draft #12`, `merged #12`. */
export function prChip(pr: PrInfo): { word: string; tone: 'open' | 'draft' | 'merged' | 'closed' } {
  if (pr.state === 'merged') return { word: `merged #${pr.number}`, tone: 'merged' };
  if (pr.state === 'closed') return { word: `closed #${pr.number}`, tone: 'closed' };
  if (pr.draft) return { word: `draft #${pr.number}`, tone: 'draft' };
  return { word: `PR #${pr.number}`, tone: 'open' };
}

/** `3 commits` past the base; null for none. */
export function commitsWord(n: number | null | undefined): string | null {
  if (!n) return null;
  return `${n} commit${n === 1 ? '' : 's'}`;
}
