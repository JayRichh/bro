// The command palette's rows, built from the daemon model like the
// terminal app's `/` palette (crates/nebula-tui/src/palette.rs): one flat
// row per repo, worktree, session and open pull request, each searched by
// its full path (`repo/branch/name`) and drawn as a dim crumb before its
// own name. Before a query it lists the sessions, the ones that want you
// first, then the window's actions; typing reaches everything.

import type { Model } from './api';
import { fuzzyMatch, rank } from './fuzzy';
import { prWord } from './status';

export type PaletteTarget =
  | { kind: 'repo'; id: string }
  | { kind: 'worktree'; id: string }
  | { kind: 'session'; id: string }
  | { kind: 'pr'; url: string; worktree: string }
  | { kind: 'action'; id: string };

export interface PaletteItem {
  target: PaletteTarget;
  /** What the query runs over. */
  text: string;
  /** Drawn dim before the label: the repo a row lives in. */
  crumb: string;
  label: string;
  /** Drawn at the right: kind, status, keys. */
  hint: string;
  /** Chords for an action row. */
  keys?: string[];
  /** 0 needs you, 1 running, 2 unread, 3 the rest. */
  tier: number;
  /** Newer first within a tier. */
  stamp: number;
  /** Shown before a query. */
  idle: boolean;
}

export interface PaletteCommand {
  id: string;
  label: string;
  /** Searched too: what else the action is called. */
  words?: string;
  keys?: string[];
  group: string;
}

function tierOf(status: string, unseen: boolean): number {
  if (status === 'needs_feedback') return 0;
  if (status === 'running') return 1;
  if (unseen) return 2;
  return 3;
}

export function buildItems(m: Model, commands: PaletteCommand[]): PaletteItem[] {
  const out: PaletteItem[] = [];
  const projectName = (id: string) => m.projects.find((p) => p.id === id)?.name ?? '';
  for (const a of m.agents) {
    if (a.archived) continue;
    const w = m.worktrees.find((x) => x.id === a.worktree_id);
    const repo = w ? projectName(w.project_id) : '';
    out.push({
      target: { kind: 'session', id: a.id },
      text: [repo, w?.branch, a.name].filter(Boolean).join('/'),
      crumb: repo,
      label: a.name,
      hint: a.status === 'needs_feedback' ? 'needs you' : a.status === 'running' ? 'running' : a.unseen ? 'unread' : 'session',
      tier: tierOf(a.status, a.unseen),
      stamp: a.status_changed_at,
      idle: true,
    });
  }
  for (const p of m.projects) {
    const stamp = Math.max(
      0,
      ...m.agents.filter((a) => m.worktrees.some((w) => w.id === a.worktree_id && w.project_id === p.id)).map((a) => a.status_changed_at),
    );
    out.push({ target: { kind: 'repo', id: p.id }, text: p.name, crumb: '', label: p.name, hint: 'repo', tier: 3, stamp, idle: false });
  }
  for (const w of m.worktrees) {
    const repo = projectName(w.project_id);
    const stamp = Math.max(0, ...m.agents.filter((a) => a.worktree_id === w.id).map((a) => a.status_changed_at));
    out.push({
      target: { kind: 'worktree', id: w.id },
      text: `${repo}/${w.branch}`,
      crumb: repo,
      label: w.branch,
      hint: w.is_main ? 'checkout' : 'worktree',
      tier: 3,
      stamp,
      idle: false,
    });
  }
  for (const [wid, pr] of Object.entries(m.prs)) {
    const w = m.worktrees.find((x) => x.id === wid);
    const repo = w ? projectName(w.project_id) : '';
    out.push({
      target: { kind: 'pr', url: pr.url, worktree: wid },
      text: `${repo}/#${pr.number} ${pr.title}`,
      crumb: repo,
      label: `#${pr.number} ${pr.title}`,
      hint: prWord(pr),
      tier: 3,
      stamp: 0,
      idle: false,
    });
  }
  for (const c of commands) {
    out.push({
      target: { kind: 'action', id: c.id },
      text: `${c.label} ${c.words ?? ''} ${c.group}`.trim(),
      crumb: c.group,
      label: c.label,
      hint: '',
      keys: c.keys,
      tier: 4,
      stamp: 0,
      idle: true,
    });
  }
  return out;
}

/** The rows for `query`: best match first, ties in attention order then
 *  newest; before a query, the sessions (attention order) then actions. */
export function search(items: PaletteItem[], query: string): { item: PaletteItem; positions: number[] }[] {
  const pool = query.trim() ? items : items.filter((i) => i.idle);
  const order = new Map<PaletteItem, number>();
  [...pool].sort((a, b) => a.tier - b.tier || b.stamp - a.stamp).forEach((it, i) => order.set(it, i));
  // Among equal scores, a row whose own name holds the query only as
  // scattered letters ("gst" in ledger-recon-test, matched through its
  // gst-rounding worktree) goes after the rest: its name is a coincidence.
  const demoted = (i: PaletteItem) => (scattered(query, i.label) ? pool.length : 0);
  return rank(query, pool, (i) => i.text, (i) => demoted(i) + (order.get(i) ?? 0));
}

/** `label` matches `query` only as a subsequence: some term is not a
 *  contiguous run in it (a prefix, word start or any substring is one). */
export function scattered(query: string, label: string): boolean {
  const terms = query.toLowerCase().split(/\s+/).filter(Boolean);
  if (!terms.length || !fuzzyMatch(query, label)) return false;
  const l = label.toLowerCase();
  return terms.some((t) => !l.includes(t));
}

/** Map positions in `text` onto the label for highlighting (the label is
 *  the tail of the text for path rows, its head for action rows). */
export function labelMarks(item: PaletteItem, positions: number[]): Set<number> {
  const at = item.target.kind === 'action' ? 0 : item.text.length - item.label.length;
  const out = new Set<number>();
  for (const p of positions) if (p >= at && p < at + item.label.length) out.add(p - at);
  return out;
}

/** The ring `.` and `,` step through, as the terminal app's
 *  (`attention_sessions`): every live session in the palette's idle order
 *  (needs you, running, unread, then newest first, never-run last),
 *  archived ones left out, from every repo. */
export function attentionSessions(m: Model): string[] {
  const rows = buildItems(m, []).filter((i) => i.target.kind === 'session');
  rows.sort((a, b) => a.tier - b.tier || b.stamp - a.stamp);
  return rows.map((i) => (i.target as { id: string }).id);
}

/** The session `step` places along `ring` from `current`, wrapping; from
 *  outside the ring `.` lands on the first and `,` on the last. */
export function stepRing(ring: string[], current: string | null, step: 1 | -1): string | null {
  if (!ring.length) return null;
  const at = current ? ring.indexOf(current) : -1;
  if (at < 0) return step > 0 ? ring[0] : ring[ring.length - 1];
  return ring[(at + step + ring.length) % ring.length];
}
