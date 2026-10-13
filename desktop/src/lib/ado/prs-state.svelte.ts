// What the PR views keep while you look elsewhere: each repo's lists, the
// composer's drafts per branch (a failed create never loses typing), and
// the last create's note for the detail to show.

import type { CreateResult, PrLists, Reviewer } from './prs-api';

export interface ListState {
  data: PrLists | null;
  error: string | null;
  loading: boolean;
  /** Unix ms of the last good load. */
  at: number;
}

export interface ComposeDraft {
  seeded: boolean;
  seedError: string | null;
  target: string;
  title: string;
  summary: string;
  changes: string;
  testing: string;
  task: { id: number; title: string } | null;
  noTask: boolean;
  reviewers: Reviewer[];
  noReviewer: boolean;
  draft: boolean;
  busy: boolean;
  outcome: CreateResult | null;
  /** The task the create asked to link, kept for Retry. */
  linkTask: number | null;
  error: string | null;
}

export const prsState = $state({
  lists: {} as Record<string, ListState>,
  drafts: {} as Record<string, ComposeDraft>,
  /** Per repo: the PR just created and its note, for the list's badge and
   *  the detail's first look. `pending` until the list has reloaded once
   *  to show it, on the Mine tab. */
  created: {} as Record<string, CreatedPr>,
});

export interface CreatedPr {
  id: number;
  note: string | null;
  pending: boolean;
}

export function freshDraft(): ComposeDraft {
  return {
    seeded: false,
    seedError: null,
    target: '',
    title: '',
    summary: '',
    changes: '',
    testing: '',
    task: null,
    noTask: false,
    reviewers: [],
    noReviewer: false,
    draft: true,
    busy: false,
    outcome: null,
    linkTask: null,
    error: null,
  };
}

/** The PR description: the template's four sections, in order. */
export function describe(d: ComposeDraft): string {
  const task = d.noTask ? 'No task (n/a)' : d.task ? `#${d.task.id} ${d.task.title}`.trim() : '';
  const part = (h: string, body: string) => (body.trim() ? `## ${h}\n${body.trim()}\n` : `## ${h}\n`);
  return [part('Summary', d.summary), part('Changes', d.changes), part('Testing', d.testing), part('Task', task)].join('\n');
}
