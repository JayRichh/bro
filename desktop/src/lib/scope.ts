// The window's scope (nebula-core `Scope`): what the bench is looking at,
// one repo, one worktree, one session or one account, or any mix. The
// Tauri side holds it (`board_scope`) and narrows the board's lanes and
// every card's `in_scope` with core's rule; `model.scope` is the truth.
// This file only reads it: the chip's words, the scope a click asks for,
// and the overview's and attention strip's narrowed model. Pure.

import type { Model, Scope } from './api';

/** Nothing narrowed. Rust sends unset fields as null, the window may
 *  leave them out: both mean "anything". */
export function isEverything(s: Scope | null | undefined): boolean {
  return !s || (!s.repo && !s.worktree && !s.session && !s.account);
}

/** A scope with its unset fields dropped, so two that mean the same
 *  compare the same. */
export function tidy(s: Scope): Scope {
  const out: Scope = {};
  if (s.repo) out.repo = s.repo;
  if (s.worktree) out.worktree = s.worktree;
  if (s.session) out.session = s.session;
  if (s.account) out.account = s.account;
  return out;
}

export function sameScope(a: Scope | null | undefined, b: Scope | null | undefined): boolean {
  const x = tidy(a ?? {});
  const y = tidy(b ?? {});
  return x.repo === y.repo && x.worktree === y.worktree && x.session === y.session && x.account === y.account;
}

export type ScopeKind = 'repo' | 'worktree' | 'session' | 'account';
/** One segment of the chip: `repo > worktree`, a session, an account. */
export interface ScopePart {
  kind: ScopeKind;
  id: string;
  label: string;
}

/** The chip's segments, outermost first; empty for everything. A
 *  worktree scoped without its repo still names the repo first. */
export function scopeParts(m: Model, s: Scope = m.scope): ScopePart[] {
  const out: ScopePart[] = [];
  const wt = s.worktree ? m.worktrees.find((w) => w.id === s.worktree) : undefined;
  const repo = s.repo ?? wt?.project_id;
  if (repo) out.push({ kind: 'repo', id: repo, label: m.projects.find((p) => p.id === repo)?.name ?? '?' });
  if (s.worktree) out.push({ kind: 'worktree', id: s.worktree, label: wt?.branch ?? '?' });
  if (s.session) out.push({ kind: 'session', id: s.session, label: m.agents.find((a) => a.id === s.session)?.name ?? '?' });
  if (s.account) out.push({ kind: 'account', id: s.account, label: s.account });
  return out;
}

/** One repo; the account narrowing stays. */
export function repoScope(s: Scope, repo: string): Scope {
  return tidy({ repo, account: s.account });
}

/** One worktree, in its repo; the account narrowing stays. */
export function worktreeScope(m: Model, s: Scope, worktree: string): Scope {
  const repo = m.worktrees.find((w) => w.id === worktree)?.project_id;
  return tidy({ repo, worktree, account: s.account });
}

/** One account (null: any), the place narrowing kept. */
export function accountScope(s: Scope, account: string | null): Scope {
  return tidy({ ...s, account });
}

/** The scope with one segment taken off: a repo takes its worktree and
 *  session with it, a worktree its session. */
export function dropPart(s: Scope, kind: ScopeKind): Scope {
  const out = tidy(s);
  if (kind === 'repo') (delete out.repo, delete out.worktree, delete out.session);
  if (kind === 'worktree') (delete out.worktree, delete out.session);
  if (kind === 'session') delete out.session;
  if (kind === 'account') delete out.account;
  return out;
}

/** The scope narrowed to `part` and what is outside it: a click on the
 *  chip's repo segment drops the worktree under it. */
export function upTo(s: Scope, kind: ScopeKind): Scope {
  const out = tidy(s);
  if (kind === 'repo') (delete out.worktree, delete out.session);
  if (kind === 'worktree') delete out.session;
  return out;
}

/** The scope with whatever it names that the model no longer has taken
 *  off (a repo removed, a worktree deleted, a session gone); null when it
 *  all still stands. Only while connected: an empty model names nothing. */
export function pruneScope(m: Model, s: Scope = m.scope): Scope | null {
  if (!m.connected || isEverything(s)) return null;
  let out = tidy(s);
  if (out.repo && !m.projects.some((p) => p.id === out.repo)) out = dropPart(out, 'repo');
  if (out.worktree && !m.worktrees.some((w) => w.id === out.worktree)) out = dropPart(out, 'worktree');
  if (out.session && !m.agents.some((a) => a.id === out.session)) out = dropPart(out, 'session');
  return sameScope(out, s) ? null : out;
}

/** The worktree the scope points at: its own, or its session's. */
export function scopedWorktree(m: Model, s: Scope = m.scope): string | null {
  if (s.worktree) return s.worktree;
  if (s.session) return m.agents.find((a) => a.id === s.session)?.worktree_id ?? null;
  return null;
}

/** The repo the scope points at: its own, or its worktree's. */
export function scopedRepo(m: Model, s: Scope = m.scope): string | null {
  if (s.repo) return s.repo;
  const wt = scopedWorktree(m, s);
  return (wt && m.worktrees.find((w) => w.id === wt)?.project_id) || null;
}

/** Whether session `id` is in the scope, as the Tauri side read it; a
 *  session it has no card for yet counts in. */
export function inScope(m: Model, id: string): boolean {
  return m.cards[id]?.in_scope ?? true;
}

/**
 * The model the overview and the attention strip read under the scope:
 * repos and worktrees the scope's place admits, sessions by their card's
 * `in_scope`, subagents with their parent session. With a session or an
 * account scope, only the worktrees that hold one of its sessions, and no
 * terminals (they belong to no account). The model itself when nothing is
 * narrowed.
 */
export function scopeModel(m: Model): Model {
  const s = m.scope;
  if (isEverything(s)) return m;
  const placeOk = (w: { id: string; project_id: string }) => (!s.repo || w.project_id === s.repo) && (!s.worktree || w.id === s.worktree);
  const agents = m.agents.filter((a) => inScope(m, a.id));
  const byPerson = !!(s.session || s.account);
  const held = new Set(agents.map((a) => a.worktree_id));
  const worktrees = m.worktrees.filter((w) => placeOk(w) && (!byPerson || held.has(w.id)));
  const wids = new Set(worktrees.map((w) => w.id));
  const pids = new Set(worktrees.map((w) => w.project_id));
  const kept = agents.filter((a) => wids.has(a.worktree_id));
  const ids = new Set(kept.map((a) => a.id));
  return {
    ...m,
    projects: m.projects.filter((p) => pids.has(p.id) || (!byPerson && !s.worktree && p.id === s.repo)),
    worktrees,
    agents: kept,
    subagents: m.subagents.filter((x) => ids.has(x.parent_agent)),
    terminals: byPerson ? [] : m.terminals.filter((t) => wids.has(t.worktree_id)),
    ports: m.ports.filter((p) => wids.has(p.worktree)),
  };
}

/** Cards each lane leaves out under the scope, all lanes together. */
export function hiddenTotal(m: Model): number {
  return Object.values(m.hidden ?? {}).reduce((n, x) => n + (x ?? 0), 0);
}
