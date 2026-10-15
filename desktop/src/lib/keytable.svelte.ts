// The window's live key table: the registry's default rows (actions.gen.ts)
// with the user's bindings laid over them by shortcuts.ts's
// applyOverrides. Deep state, so whatever reads a row's keys (a tooltip's
// withKey, the help, a menu) follows a rebind.

import { DESK_KEYS, type DeskKey } from './actions.gen';
import type { Shortcut } from './shortcuts';

function rowOf(k: DeskKey): Shortcut {
  return {
    id: k.id,
    keys: [...k.chords],
    scope: k.scope,
    label: k.label,
    group: k.group,
    ...(k.editorOwns ? { editorOwns: true } : {}),
  };
}

/** The live table: the registry's defaults with the user's bindings over
 *  them. Changed in place, so every caller's default argument sees it. */
export const SHORTCUTS: Shortcut[] = $state(DESK_KEYS.map(rowOf));
