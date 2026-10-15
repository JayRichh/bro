// The Add account sheet's state: closed, the form (a modal: page keys
// stand down), or signing in (a small panel docked over the window while
// the login runs in a terminal, so the terminal stays usable). Kept out of
// ui.svelte.ts so the sheet and the verbs that open it share one place.

export type AddPhase = 'closed' | 'form' | 'signing';

export const addSheet = $state({
  phase: 'closed' as AddPhase,
  /** The harness the form starts on (a rail row, an own login's Add). */
  harness: '',
  /** Start with "adopt my current login" ticked. */
  adopt: false,
  /** The account being signed in, in the signing phase. */
  name: '',
});

/** Open the form, optionally on a harness and with adopt ticked. */
export function openAddAccount(preset: { harness?: string; adopt?: boolean } = {}) {
  addSheet.harness = preset.harness ?? '';
  addSheet.adopt = preset.adopt ?? false;
  addSheet.name = '';
  addSheet.phase = 'form';
}

/** Show the docked sign-in panel for `name` (its login runs in a terminal). */
export function watchSignIn(name: string) {
  addSheet.name = name;
  addSheet.phase = 'signing';
}

export function closeAddAccount() {
  addSheet.phase = 'closed';
}
