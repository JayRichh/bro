// The ship sheet's state on its own, so ui.svelte.ts can count it as a
// layer without importing what the sheet runs (ship/sheet.svelte.ts).

import type { MergeMethod, ShipError, ShipStatus } from '../api';
import type { ComposerForm } from './ship';

export const shipSheet = $state({
  /** `compose`: open a pull request; `merge`: confirm its merge. */
  mode: null as null | 'compose' | 'merge',
  worktree: '',
  /** The session it was opened for: the suggestion reads its words. */
  session: null as string | null,
  status: null as ShipStatus | null,
  loading: false,
  /** Why the last read or send failed; an inline line with its button. */
  error: null as ShipError | null,
  form: { title: '', body: '', base: '', draft: false } as ComposerForm,
  merge: { method: 'squash' as MergeMethod, deleteBranch: true },
  busy: false,
  /** What came back that is worth a line (a merge queued behind checks). */
  note: null as string | null,
});
