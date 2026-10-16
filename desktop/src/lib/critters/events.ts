// The critters' view of the app: which daemon changes the world should react
// to. The model is narrowed by the scope and pushed whole, so an event is a
// difference between two snapshots, and only for sessions and PRs both
// snapshots hold: one that appears because the scope widened is not news.
// Pure (no Tauri), so the node tests can run it.

import type { Model } from '../api';

/** What the diff looks at: session status by agent id, PR state by key. */
export interface Snap {
  agents: Record<string, string>;
  prs: Record<string, string>;
}

/** A session wants you, one finished its turn, a PR merged. */
export type Change = { kind: 'needsYou'; agent: string } | { kind: 'turnDone'; agent: string } | { kind: 'prMerged'; key: string };

/** Live sessions' statuses and every PR's state (absent means open). */
export function snap(m: Pick<Model, 'agents' | 'prs'>): Snap {
  const agents: Record<string, string> = {};
  for (const a of m.agents) {
    if (!a.archived) agents[a.id] = a.status;
  }
  const prs: Record<string, string> = {};
  for (const [key, pr] of Object.entries(m.prs)) prs[key] = pr.state ?? 'open';
  return { agents, prs };
}

/** What happened between two snapshots: agents in `next`'s order, then PRs.
 *  The first snapshot has nothing to compare with, so it fires nothing. */
export function changes(prev: Snap | null, next: Snap): Change[] {
  if (!prev) return [];
  const out: Change[] = [];
  for (const [agent, now] of Object.entries(next.agents)) {
    if (!Object.hasOwn(prev.agents, agent)) continue;
    const was = prev.agents[agent];
    if (was !== 'needs_feedback' && now === 'needs_feedback') out.push({ kind: 'needsYou', agent });
    else if (was === 'running' && now === 'finished') out.push({ kind: 'turnDone', agent });
  }
  for (const [key, now] of Object.entries(next.prs)) {
    if (!Object.hasOwn(prev.prs, key)) continue;
    if (prev.prs[key] !== 'merged' && now === 'merged') out.push({ kind: 'prMerged', key });
  }
  return out;
}

/** Nothing is running: the critters may doze late at night. */
export function allIdle(m: Pick<Model, 'agents'>): boolean {
  return !m.agents.some((a) => !a.archived && a.status === 'running');
}
