// Delete every session in one checkout: the terminal app's Shift+D on its
// sessions panel (crates/nebula-tui/src/event_loop/actions.rs), offered
// from a worktree's menu here. What dies is what the overview shows for
// the checkout: its sessions (archived ones only while they are shown)
// and its terminals, never another worktree's.

import { api, type Model } from './api';
import { app, toast } from './store.svelte';
import { ask } from './ui/ui.svelte';

/** How many names the confirm lists before "and N more", as the TUI's. */
export const LISTED = 8;

export interface Doomed {
  agents: { id: string; name: string }[];
  terminals: { id: string; name: string }[];
}

/** The rows a delete-all in `worktree` takes. */
export function checkoutSessions(m: Model, worktree: string, showArchived: boolean): Doomed {
  return {
    agents: m.agents
      .filter((a) => a.worktree_id === worktree && (showArchived || !a.archived))
      .map((a) => ({ id: a.id, name: a.name })),
    terminals: m.terminals
      .filter((t) => t.worktree_id === worktree)
      .map((t) => ({ id: t.id, name: t.run_command ? 'run' : t.name })),
  };
}

/** The confirm's listing: the first few names, then a count of the rest. */
export function listing(names: string[], max = LISTED): string[] {
  if (names.length <= max) return names;
  return [...names.slice(0, max), `\u2026 and ${names.length - max} more`];
}

/** Ask, then delete every session and close every terminal in `worktree`. */
export async function deleteAllSessions(worktree: string) {
  const w = app.model.worktrees.find((x) => x.id === worktree);
  if (!w) return;
  const doomed = checkoutSessions(app.model, worktree, app.showArchived);
  const names = [...doomed.agents, ...doomed.terminals].map((x) => x.name);
  if (!names.length) return toast('No sessions to delete here.');
  const r = await ask.confirm({
    title: `Delete all ${names.length} session${names.length === 1 ? '' : 's'}`,
    message: `Delete every session in ${w.branch}? Their history goes away, and the terminals' shells are killed. The worktree stays.`,
    detail: listing(names),
    confirm: 'Delete all',
    danger: true,
  });
  if (!r.ok) return;
  // Only what was listed: a session started while the dialog was up stays.
  const gone = new Set([...doomed.agents, ...doomed.terminals].map((x) => x.id));
  const s = app.session;
  if (s && gone.has('Agent' in s ? s.Agent : s.Terminal)) app.session = null;
  const failed: string[] = [];
  for (const a of doomed.agents) await api.deleteAgent(a.id).catch(() => failed.push(a.name));
  for (const t of doomed.terminals) await api.closeTerminal(t.id).catch(() => failed.push(t.name));
  if (failed.length) toast(`Could not delete: ${failed.join(', ')}`);
}
