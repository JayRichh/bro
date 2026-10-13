// One row per repo for the overview, the rail and the attention strip,
// built from the daemon model.

import type { Model, PrInfo, Project, TerminalTab, Worktree } from '../api';
import { buildForest, sortRepos, walkAll, type Node, type Status } from './tree';

export type RepoRow = {
  project: Project;
  name: string;
  forest: Node[];
  terminals: TerminalTab[];
  worktrees: Worktree[];
  main: Worktree | undefined;
  /** Ports something listens on, across the repo's worktrees. */
  ports: number[];
  /** Open PRs by worktree branch. */
  prs: { branch: string; pr: PrInfo }[];
};

export function repoRows(m: Model): RepoRow[] {
  const rows = m.projects.map((project): RepoRow => {
    const worktrees = m.worktrees
      .filter((w) => w.project_id === project.id)
      .sort((a, b) => Number(b.is_main) - Number(a.is_main) || a.sort_order - b.sort_order);
    const wids = new Set(worktrees.map((w) => w.id));
    const agents = m.agents.filter((a) => wids.has(a.worktree_id) && !a.archived);
    const ids = new Set(agents.map((a) => a.id));
    const subs = m.subagents.filter((s) => ids.has(s.parent_agent));
    const ports = [...new Set(m.ports.filter((p) => wids.has(p.worktree)).flatMap((p) => p.live))].sort((a, b) => a - b);
    const prs = worktrees.flatMap((w) => (m.prs[w.id] ? [{ branch: w.branch, pr: m.prs[w.id] }] : []));
    return {
      project,
      name: project.name,
      forest: buildForest(agents, subs),
      terminals: m.terminals.filter((t) => wids.has(t.worktree_id)).sort((a, b) => a.sort_order - b.sort_order),
      worktrees,
      main: worktrees.find((w) => w.is_main) ?? worktrees[0],
      ports,
      prs,
    };
  });
  return sortRepos(rows);
}

/** Nodes by status, across the given rows. */
export function tally(rows: RepoRow[]): Record<Status, Node[]> {
  const out = {} as Record<Status, Node[]>;
  for (const r of rows) for (const n of walkAll(r.forest)) (out[n.status] ??= []).push(n);
  return out;
}
