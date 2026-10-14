// A session delete that empties a linked worktree, and what a worktree
// delete would throw away: the words the confirms show. Mirrors the
// terminal app's `with_worktree_offer` (event_loop/commands.rs), with one
// difference: the terminal app never asks while Show all worktrees is on,
// because its grid keeps a band for the emptied checkout to delete it
// from later. The window has no such band, so it always asks, and Delete
// emptied worktree on skips the question as it does there.

import type { Model, Worktree } from '../api';
import type { CheckoutState } from './api';

/** Live cards a worktree holds: its unarchived sessions and its terminals. */
export function liveCardsIn(m: Model, worktree: string): number {
  return (
    m.agents.filter((a) => a.worktree_id === worktree && !a.archived).length +
    m.terminals.filter((t) => t.worktree_id === worktree).length
  );
}

export interface WorktreeOffer {
  worktree: Worktree;
  /** Delete emptied worktree is on and nothing archived is left: it goes
   *  without a question. */
  auto: boolean;
  /** Archived sessions still filed there; their history goes with it. */
  archived: number;
}

/**
 * Whether deleting `doomed` (session or terminal ids) out of `worktree`
 * empties it, `liveTaken` being how many of its live cards they are (zero
 * for an archived session, which still offers when nothing live is left,
 * as the terminal app does). Null for the main checkout, a worktree that
 * keeps a live card, or one gone.
 */
export function worktreeOffer(
  m: Model,
  worktree: string,
  liveTaken: number,
  doomed: string[],
  deleteEmpty: boolean,
): WorktreeOffer | null {
  const w = m.worktrees.find((x) => x.id === worktree);
  if (!w || w.is_main || liveCardsIn(m, worktree) !== liveTaken) return null;
  const archived = m.agents.filter((a) => a.worktree_id === worktree && a.archived && !doomed.includes(a.id)).length;
  return { worktree: w, auto: deleteEmpty && archived === 0, archived };
}

/** The confirm's sentence about the worktree. */
export function offerLine(o: WorktreeOffer): string {
  const b = o.worktree.branch;
  if (o.auto) return `Worktree ${b} goes with it: nothing else is left there (Delete emptied worktree is on).`;
  if (o.archived)
    return `Nothing live is left in worktree ${b}. Its ${o.archived} archived session${o.archived === 1 ? '' : 's'} would go with it if you delete it too.`;
  return `Nothing else is left in worktree ${b}.`;
}

const plural = (n: number, one: string, many = `${one}s`) => `${n} ${n === 1 ? one : many}`;

/** What a checkout holds, for a delete confirm's detail lines. `null`:
 *  git could not say (the folder is gone, say). */
export function stateLines(s: CheckoutState | null): string[] {
  if (!s) return ['Could not read its git status.'];
  const out: string[] = [];
  const files = [s.changed && plural(s.changed, 'uncommitted change'), s.untracked && plural(s.untracked, 'untracked file')].filter(Boolean);
  out.push(files.length ? files.join(', ') : 'No uncommitted changes');
  if (s.unpushed) out.push(`${plural(s.unpushed, 'commit')} on no remote (the branch keeps them)`);
  return out;
}

/** Changes a non-forced `git worktree remove` would refuse over. */
export function dirty(s: CheckoutState | null): boolean {
  return !!s && s.changed + s.untracked > 0;
}

/** The cards a worktree delete takes, as a detail line; null for none. */
export function cardsLine(sessions: number, terminals: number): string | null {
  const parts = [sessions && plural(sessions, 'session'), terminals && plural(terminals, 'terminal')].filter(Boolean);
  return parts.length ? `${parts.join(', ')} in it` : null;
}
