// Typed wrappers over the pull request commands (`ado_*` in prs.rs and
// main.rs), the range diff the PR views read, and `session_ask`. Field
// names match the Rust structs as serde writes them (camelCase). The diff
// types are diff-api's; the PR views draw their diff with DiffPane.

import { invoke } from '@tauri-apps/api/core';
import { promptingInvoke } from './signin.svelte';
import type { Blob, DiffSide, RangeDiff } from '../diff-api';

export interface ReviewerVote {
  id: string;
  name: string;
  /** 10 approved, 5 with suggestions, 0 none, -5 waiting for author, -10 rejected. */
  vote: number;
  /** The vote in words. */
  label: string;
  required: boolean;
}

export interface PrSummary {
  id: number;
  title: string;
  source: string;
  target: string;
  isDraft: boolean;
  status: string;
  createdBy: string;
  reviewers: ReviewerVote[];
  workItems: number[];
  /** Comment threads, system threads left out. */
  threads: number;
  url: string;
  /** `lastMergeSourceCommit`: the source tip the review diff pins to. */
  sourceCommit: string | null;
  /** `lastMergeTargetCommit`: the target at that merge. */
  targetCommit: string | null;
}

export type PolicyState = 'approved' | 'rejected' | 'queued' | 'running' | 'notApplicable' | 'notEvaluated' | 'unavailable';

export interface PolicyRow {
  name: string;
  state: PolicyState;
  blocking: boolean;
}

export interface Comment {
  author: string;
  text: string;
  at: string;
}

export interface Thread {
  id: number;
  status: string;
  file: string | null;
  line: number | null;
  comments: Comment[];
}

export interface PrLists {
  mine: PrSummary[];
  review: PrSummary[];
}

export interface PrDetail {
  pr: PrSummary;
  policies: PolicyRow[];
  threads: Thread[];
}

export interface Reviewer {
  id: string;
  name: string;
  email: string;
}

export interface NewPr {
  source: string;
  target: string;
  title: string;
  description: string;
  draft: boolean;
  reviewers: Reviewer[];
  workItem: number | null;
}

export interface CreateResult {
  kind: 'created' | 'createdTaskNotLinked' | 'existing';
  id: number;
  /** Why the task link failed, for "Created, task not linked". */
  reason: string | null;
  /** A line about co-author lines already on the server, when there are any. */
  note: string | null;
}

export interface ComposeSeed {
  target: string;
  subjects: string[];
  task: number | null;
  taskTitle: string | null;
}

/** The tasks the picker offers. */
export interface WorkItem {
  id: number;
  kind: string;
  title: string;
  state: string;
  stateCategory: string;
  assignedTo: string | null;
  parent: number | null;
}
export interface TaskList {
  groups: { parent: WorkItem | null; items: WorkItem[] }[];
  cachedAt?: number | null;
  cachedBecause?: 'auth' | 'offline' | null;
  cachedReason?: string | null;
}
export type TaskScope = 'currentSprint' | 'myOpen' | 'sprintAll' | { iteration: string };


/** Votes as the backend takes them, in the order the buttons show. */
export const VOTES: { vote: number; label: string }[] = [
  { vote: 10, label: 'Approve' },
  { vote: 5, label: 'Approve with suggestions' },
  { vote: -5, label: 'Wait for author' },
  { vote: -10, label: 'Reject' },
  { vote: 0, label: 'Reset vote' },
];

export const prs = {
  list: (worktree: string) => promptingInvoke<PrLists>('ado_prs', { worktree }),
  detail: (worktree: string, id: number) => promptingInvoke<PrDetail>('ado_pr_detail', { worktree, id }),
  create: (worktree: string, pr: NewPr) => promptingInvoke<CreateResult>('ado_create_pr', { worktree, pr }),
  /** Retry for "Created, task not linked". */
  linkTask: (worktree: string, id: number, task: number) => promptingInvoke<void>('ado_link_pr_task', { worktree, id, task }),
  publish: (worktree: string, id: number) => promptingInvoke<void>('ado_publish_pr', { worktree, id }),
  /** Refused for drafts. */
  vote: (worktree: string, id: number, vote: number) => promptingInvoke<void>('ado_vote', { worktree, id, vote }),
  reply: (worktree: string, id: number, thread: number, text: string) =>
    promptingInvoke<Comment>('ado_reply', { worktree, id, thread, text }),
  searchIdentities: (worktree: string, q: string) => promptingInvoke<Reviewer[]>('ado_search_identities', { worktree, q }),
  recentReviewers: (worktree: string) => invoke<Reviewer[]>('ado_recent_reviewers', { worktree }),
  composeSeed: (worktree: string, branch: string) => promptingInvoke<ComposeSeed>('ado_compose_seed', { worktree, branch }),
  tasks: (worktree: string, scope: TaskScope) => promptingInvoke<TaskList>('ado_tasks', { worktree, scope }),
  diffRange: (worktree: string, base: string, tip: string) => invoke<RangeDiff>('diff_range', { worktree, base, tip }),
  diffBlob: (worktree: string, side: DiffSide, path: string) => invoke<Blob>('diff_blob', { worktree, side, path }),
  /** Ask a session (`kete ask`) and wait for its answer. */
  sessionAsk: (agent: string, question: string) => invoke<string>('session_ask', { agent, question }),
};

/** Where a PRs place points: the list, one PR, or the composer for a branch. */
export type PrsItem =
  | { kind: 'list' }
  | { kind: 'pr'; id: number }
  | { kind: 'compose'; branch: string; task: number | null };

/** `pr:12` (or a bare `12`), `compose:<branch>`, `compose:<branch>:task:<id>`.
 *  Branch names cannot hold `:`, so the split is safe. */
export function parseItem(item: string | undefined): PrsItem {
  if (!item) return { kind: 'list' };
  if (item.startsWith('compose:')) {
    const rest = item.slice('compose:'.length);
    const at = rest.lastIndexOf(':task:');
    if (at >= 0) {
      const id = Number(rest.slice(at + ':task:'.length));
      return { kind: 'compose', branch: rest.slice(0, at), task: Number.isInteger(id) && id > 0 ? id : null };
    }
    return { kind: 'compose', branch: rest, task: null };
  }
  const id = Number(item.startsWith('pr:') ? item.slice(3) : item);
  return Number.isInteger(id) && id > 0 ? { kind: 'pr', id } : { kind: 'list' };
}

export const prItem = (id: number) => `pr:${id}`;
export const composeItem = (branch: string, task?: number | null) =>
  task ? `compose:${branch}:task:${task}` : `compose:${branch}`;

/** A vote's square: olive for approvals (hollow with suggestions), oxide
 *  for pushback (hollow while waiting for the author), hollow muted for none. */
export function voteSquare(vote: number): string {
  if (vote >= 10) return 'bg-done';
  if (vote > 0) return 'border border-done';
  if (vote <= -10) return 'bg-fail';
  if (vote < 0) return 'border border-fail';
  return 'border border-muted';
}

/** A policy row as a square and a word. Pending (queued, running),
 *  approved, rejected and n/a each read differently. */
export function policyMark(state: PolicyState, draft: boolean): { square: string; word: string } {
  switch (state) {
    case 'approved':
      return { square: 'bg-done', word: 'approved' };
    case 'rejected':
      return { square: 'bg-fail', word: 'rejected' };
    case 'running':
      return { square: 'bg-running', word: 'running' };
    case 'queued':
      return { square: 'border border-running', word: 'queued' };
    case 'notApplicable':
      return { square: 'bg-faint', word: 'not applicable' };
    case 'unavailable':
      return { square: 'border border-fail', word: 'unavailable' };
    default:
      return { square: 'border border-muted', word: draft ? 'not evaluated (draft)' : 'not evaluated' };
  }
}

/**
 * The range a PR's review diff reads: the last merge's target and source
 * commits, pinned by SHA; without them, the parents of the PR's merge ref.
 * Never a moving branch name.
 */
export function prRange(pr: Pick<PrSummary, 'id' | 'sourceCommit' | 'targetCommit'>): { base: string; tip: string; pr: number } {
  if (pr.sourceCommit && pr.targetCommit) return { base: pr.targetCommit, tip: pr.sourceCommit, pr: pr.id };
  const merge = `refs/pull/${pr.id}/merge`;
  return { base: `${merge}^1`, tip: `${merge}^2`, pr: pr.id };
}

/** A PR's own state: draft is hollow, active filled. */
export function prMark(pr: PrSummary): { square: string; word: string } {
  if (pr.isDraft) return { square: 'border border-merged', word: 'draft' };
  if (pr.status === 'completed') return { square: 'bg-merged', word: 'merged' };
  if (pr.status === 'abandoned') return { square: 'bg-faint', word: 'abandoned' };
  return { square: 'bg-merged', word: 'active' };
}

