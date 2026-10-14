// The orchestrator, read from the daemon model: which session holds the
// role, the cross-repo tree of sessions it started, what in that tree
// wants you, and what wants you elsewhere. Pure, so the rules are tested
// without a window; the view and the commands sit on top.

import type { Agent, Model } from './api';
import { buildForest, attentionRank, walkAll, type Node, type Status } from './overview/tree';

/** The first daemon protocol that can start, pin and unpin an orchestrator. */
export const ORCHESTRATOR_PROTOCOL = 54;

/** What the window says when the daemon is too old for it. */
export const RELOAD_HINT = 'The running kete daemon is older than this window. Run `kete reload`, then try again.';

/** The session holding the orchestrator role, if one does (at most one). */
export function orchestratorOf(m: Model): Agent | undefined {
  return m.agents.find((a) => a.role === 'orchestrator' && !a.archived);
}

export function isOrchestrator(a: Agent | undefined | null): boolean {
  return !!a && a.role === 'orchestrator';
}

/** Whether the daemon can be sent Start, Set and Clear orchestrator. */
export function canProvision(m: Model): boolean {
  return m.connected && m.protocol_version >= ORCHESTRATOR_PROTOCOL;
}

/** The orchestrator's tree across every repo: it, the sessions it started
 *  (in any repo), theirs, and their subagents. Null when there is none. */
export function orchestratorTree(m: Model): Node | null {
  const orch = orchestratorOf(m);
  if (!orch) return null;
  const agents = m.agents.filter((a) => !a.archived);
  const ids = new Set(agents.map((a) => a.id));
  const forest = buildForest(
    agents,
    m.subagents.filter((s) => ids.has(s.parent_agent)),
  );
  const find = (ns: Node[]): Node | null => {
    for (const n of ns) {
      if (n.agent?.id === orch.id) return n;
      const hit = find(n.children);
      if (hit) return hit;
    }
    return null;
  };
  const root = find(forest);
  if (!root) return null;
  // Its own subtree, re-rooted at depth 0 for drawing.
  const setDepth = (n: Node, d: number) => {
    n.depth = d;
    n.children.forEach((c) => setDepth(c, d + 1));
  };
  setDepth(root, 0);
  return root;
}

/** Statuses the attention counts show, most urgent first. */
export const ATTENTION: Status[] = ['needs', 'failed', 'unread', 'running'];

/** How many of the orchestrator's descendants are in each attention state. */
export function descendantCounts(root: Node | null): Partial<Record<Status, number>> {
  const out: Partial<Record<Status, number>> = {};
  if (!root) return out;
  for (const n of walkAll(root.children)) if (ATTENTION.includes(n.status)) out[n.status] = (out[n.status] ?? 0) + 1;
  return out;
}

/** Sessions outside the orchestrator's tree that want you (needs you,
 *  failed, unread), most urgent first, then newest. */
export function elsewhere(m: Model, root: Node | null): Node[] {
  const mine = new Set(root ? walkAll([root]).map((n) => n.key) : []);
  const agents = m.agents.filter((a) => !a.archived);
  const ids = new Set(agents.map((a) => a.id));
  const all = walkAll(buildForest(agents, m.subagents.filter((s) => ids.has(s.parent_agent))));
  return all
    .filter((n) => n.kind === 'session' && !mine.has(n.key) && attentionRank({ ...n, children: [] }) <= 2)
    .sort(
      (a, b) =>
        attentionRank({ ...a, children: [] }) - attentionRank({ ...b, children: [] }) ||
        (b.agent?.status_changed_at ?? 0) - (a.agent?.status_changed_at ?? 0),
    );
}

/** Words for the counts line: `2 need you \u00b7 1 running`. */
export function countWords(c: Partial<Record<Status, number>>): { status: Status; text: string }[] {
  const word: Partial<Record<Status, (n: number) => string>> = {
    needs: (n) => (n === 1 ? 'needs you' : 'need you'),
    failed: () => 'failed',
    unread: () => 'unread',
    running: () => 'running',
  };
  return ATTENTION.filter((s) => c[s]).map((s) => ({ status: s, text: `${c[s]} ${word[s]!(c[s]!)}` }));
}
