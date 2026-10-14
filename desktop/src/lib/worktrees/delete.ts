// The worktree half of a session or terminal delete: when the delete
// empties a linked worktree, its confirm says so and offers (or, with
// Delete emptied worktree on, announces) deleting the checkout too.

import { api } from '../api';
import { app } from '../store.svelte';
import { worktreeApi, type CheckoutState } from './api';
import { dirty, offerLine, stateLines, worktreeOffer, type WorktreeOffer } from './offer';

export interface Prepared {
  offer: WorktreeOffer;
  state: CheckoutState | null;
  deleteEmpty: boolean;
}

/** The offer for deleting `doomed` out of `worktree`, with the setting
 *  and the checkout's git state read now; null when there is none. */
export async function prepareOffer(worktree: string, liveTaken: number, doomed: string[]): Promise<Prepared | null> {
  // The setting only decides how it is put: skip the reads when no offer can apply.
  if (!worktreeOffer(app.model, worktree, liveTaken, doomed, false)) return null;
  const s = await worktreeApi.settings().catch(() => ({ delete_empty_worktree: false }));
  const offer = worktreeOffer(app.model, worktree, liveTaken, doomed, s.delete_empty_worktree);
  if (!offer) return null;
  const state = await worktreeApi.checkoutState(worktree).catch(() => null);
  return { offer, state, deleteEmpty: s.delete_empty_worktree };
}

/** Whether the setting may delete the checkout without asking: never
 *  when it holds uncommitted work, or its state could not be read. */
function silent(p: Prepared): boolean {
  return p.offer.auto && !!p.state && !dirty(p.state);
}

/** What the confirm adds: a sentence, the checkout's state, and the box
 *  unless the setting already decided. The box starts ticked (Enter
 *  deletes both, as the terminal app's does) only for a clean checkout:
 *  uncommitted work is never one Enter away from gone. */
export function offerConfirm(p: Prepared): { line: string; detail: string[]; check?: string; checked?: boolean } {
  const b = p.offer.worktree.branch;
  const detail = [p.offer.worktree.path, ...stateLines(p.state)];
  if (silent(p)) return { line: offerLine(p.offer), detail };
  const risky = !p.state || dirty(p.state);
  return {
    line: offerLine({ ...p.offer, auto: false }),
    detail,
    check: risky ? `Delete worktree ${b} from disk too, uncommitted changes and all` : `Delete worktree ${b} from disk too`,
    checked: !risky,
  };
}

/** After the dialog: the offer if it still holds for the model as it is
 *  now (a session started there meanwhile cancels it) and was taken. */
export function stillOffered(p: Prepared | null, checked: boolean, liveTaken: number, doomed: string[]): WorktreeOffer | null {
  const auto = !!p && silent(p);
  if (!p || !(auto || checked)) return null;
  const now = worktreeOffer(app.model, p.offer.worktree.id, liveTaken, doomed, p.deleteEmpty);
  // Went without a question, but something archived turned up there since.
  if (now && !(now.auto && auto) && !checked) return null;
  return now;
}

/** Delete the emptied worktree, forced as the terminal app's is: the
 *  confirm showed what it held. */
export function deleteEmptied(o: WorktreeOffer | null): Promise<void> | void {
  if (o) return api.deleteWorktree(o.worktree.id, true);
}
