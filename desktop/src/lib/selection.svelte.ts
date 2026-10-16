// The row the keys act on, remembered past the moment it has the focus:
// a menu, the palette or the verb bar's own buttons take the focus, and
// what they act on is still the row that had it. App.svelte notes every
// focus outside the layers; a focus that lands on another row moves it,
// one that lands anywhere else (the header, a pane) lets it go, and the
// verb bar keeps it.

import { closestTarget, type Target } from './actions';

export const selection = $state({ target: null as Target | null });

/** The verb bar: focus on its buttons keeps the row it acts on. */
export const VERB_BAR_SEL = '[data-verb-bar]';

/** Note where the focus went (outside the layers). */
export function noteFocus(el: Element | null) {
  if (!el || el.closest(VERB_BAR_SEL)) return;
  const t = closestTarget(el);
  const now = selection.target;
  if (t?.kind !== now?.kind || t?.id !== now?.id || (t?.kind === 'agent' && now?.kind === 'agent' && t.viaSub !== now.viaSub))
    selection.target = t;
}
