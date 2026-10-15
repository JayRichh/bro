// Which checkouts a session's turn just changed: a turn that ends has
// usually committed or edited, and the board's Review and Ship lanes (and
// a card's "N commits") read that off the worktree's git state, which
// otherwise waits for the next half-minute poll.

import type { Agent } from '../api';

/** Each session's status as last seen, by id: what a later model is
 *  compared against. */
export type TurnMarks = Map<string, string>;

export function turnMarks(agents: Agent[]): TurnMarks {
  return new Map(agents.map((a) => [a.id, `${a.status}:${a.status_changed_at}`]));
}

/** The worktrees (each once, in model order) holding a session that is
 *  new since `before` or whose status changed; none on the first look. */
export function turnedWorktrees(before: TurnMarks | null, agents: Agent[]): string[] {
  if (!before) return [];
  const out: string[] = [];
  for (const a of agents) {
    if (before.get(a.id) === `${a.status}:${a.status_changed_at}`) continue;
    if (!out.includes(a.worktree_id)) out.push(a.worktree_id);
  }
  return out;
}
