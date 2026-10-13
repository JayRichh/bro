// The overview's session trees: sessions nest under the session that
// spawned them (`parent`), Claude's subagents under the session they run
// inside. Pure, so the ordering rules are tested without a window.

import type { Agent, Subagent } from '../api';
import { toneOf, subagentTone, type Tone } from '../status';

export type Status = Tone;

export type Node = {
  id: string;
  kind: 'session' | 'subagent';
  status: Status;
  label: string;
  children: Node[];
  depth: number;
  /** Unique across the forest (subagent ids repeat across sessions). */
  key: string;
  agent?: Agent;
  sub?: Subagent;
};

/** Children deeper than this start collapsed. */
export const OPEN_DEPTH = 2;

const RANK: Partial<Record<Status, number>> = { needs: 0, failed: 1, unread: 2, running: 3, done: 4 };
/** Rank of a status the strip does not count (idle, stopped, a shell). */
const QUIET = 5;
/** Rank of a repo with no sessions at all. */
const EMPTY = 6;

function ownRank(n: Node): number {
  return RANK[n.status] ?? QUIET;
}

/** needs-you 0, failed 1, unread 2, running 3, done 4: the most urgent in the subtree. */
export function attentionRank(n: Node): number {
  let r = ownRank(n);
  for (const c of n.children) r = Math.min(r, attentionRank(c));
  return r;
}

function forestRank(forest: Node[]): number {
  return forest.reduce((r, n) => Math.min(r, attentionRank(n)), EMPTY);
}

export function buildForest(agents: Agent[], subs: Subagent[]): Node[] {
  const byId = new Map(agents.map((a) => [a.id, a]));
  const nodes = new Map<string, Node>();
  for (const a of agents) {
    nodes.set(a.id, {
      id: a.id,
      kind: 'session',
      status: toneOf(a),
      label: a.name,
      children: [],
      depth: 0,
      key: `a:${a.id}`,
      agent: a,
    });
  }

  /** Whether `a`'s parent chain reaches a real root without looping. */
  function attachable(a: Agent): boolean {
    const seen = new Set([a.id]);
    let p = a.parent;
    while (p) {
      if (seen.has(p)) return false;
      seen.add(p);
      const up = byId.get(p);
      if (!up) return true;
      p = up.parent;
    }
    return true;
  }

  const roots: Node[] = [];
  const ordered = [...agents].sort((x, y) => x.sort_order - y.sort_order || x.status_changed_at - y.status_changed_at);
  for (const a of ordered) {
    const n = nodes.get(a.id)!;
    const parent = a.parent && a.parent !== a.id ? nodes.get(a.parent) : undefined;
    if (parent && attachable(a)) parent.children.push(n);
    else roots.push(n);
  }

  const orderedSubs = [...subs].sort((x, y) => x.started_at - y.started_at);
  for (const s of orderedSubs) {
    const n: Node = {
      id: s.id,
      kind: 'subagent',
      status: subagentTone(s),
      label: s.description || s.kind || 'subagent',
      children: [],
      depth: 0,
      key: `s:${s.parent_agent}:${s.id}`,
      sub: s,
    };
    const parent = nodes.get(s.parent_agent);
    if (parent) parent.children.push(n);
    else roots.push(n);
  }

  const setDepth = (n: Node, d: number) => {
    n.depth = d;
    for (const c of n.children) setDepth(c, d + 1);
  };
  for (const r of roots) setDepth(r, 0);
  return roots;
}

export type Row = {
  node: Node;
  /** Whether its children are showing. */
  expanded: boolean;
  /** Descendants hidden by a collapse, for "+ 7 agents". */
  hidden: number;
};

function count(n: Node): number {
  return n.children.reduce((t, c) => t + 1 + count(c), 0);
}

/**
 * The rows to draw, depth first. A node's children show while they sit at
 * depth OPEN_DEPTH or shallower, unless the user opened (`expanded`) or
 * closed (`collapsed`) that node by id.
 */
export function flatten(forest: Node[], expanded: Set<string>, collapsed: Set<string> = new Set()): Row[] {
  const out: Row[] = [];
  const walk = (n: Node) => {
    const has = n.children.length > 0;
    const open = has && !collapsed.has(n.id) && (expanded.has(n.id) || n.depth + 1 <= OPEN_DEPTH);
    out.push({ node: n, expanded: open, hidden: has && !open ? count(n) : 0 });
    if (open) for (const c of n.children) walk(c);
  };
  for (const r of forest) walk(r);
  return out;
}

/** Every node, depth first. */
export function walkAll(forest: Node[]): Node[] {
  const out: Node[] = [];
  const walk = (n: Node) => {
    out.push(n);
    n.children.forEach(walk);
  };
  forest.forEach(walk);
  return out;
}

/** Repo rows by attention (a needs-you anywhere floats first), then name. */
export function sortRepos<T extends { name: string; forest: Node[] }>(rows: T[]): T[] {
  return rows
    .map((r) => ({ r, rank: forestRank(r.forest) }))
    .sort((a, b) => a.rank - b.rank || a.r.name.localeCompare(b.r.name))
    .map((x) => x.r);
}
