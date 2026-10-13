// The composer's own pre-submit nudge for TnT's PR policy: a linked task
// and at least one reviewer. The "n/a" escapes turn a block into a note
// and never block. The server's policy verdict is shown separately, as
// it comes back.

export interface PolicyForm {
  /** The linked task's id, or null. */
  task: number | null;
  noTask: boolean;
  reviewers: readonly unknown[];
  noReviewer: boolean;
}

export interface Preflight {
  ok: boolean;
  notes: string[];
}

export const NOTE = {
  linkTask: 'Link a task (policy)',
  noTask: 'No task (n/a)',
  addReviewer: 'Add a reviewer (policy)',
  noReviewer: 'No reviewer (n/a)',
} as const;

/** Notes that block Create, as opposed to escape notes. */
export const BLOCKING: ReadonlySet<string> = new Set([NOTE.linkTask, NOTE.addReviewer]);

export function preflight(form: PolicyForm): Preflight {
  const notes: string[] = [];
  let ok = true;
  if (form.noTask) notes.push(NOTE.noTask);
  else if (form.task == null) {
    notes.push(NOTE.linkTask);
    ok = false;
  }
  if (form.noReviewer) notes.push(NOTE.noReviewer);
  else if (!form.reviewers.length) {
    notes.push(NOTE.addReviewer);
    ok = false;
  }
  return { ok, notes };
}
