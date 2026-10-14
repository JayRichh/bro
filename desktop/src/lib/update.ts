// The rail's version plate: this build, and a cue when a newer Kete is
// published (src-tauri/src/update.rs, the terminal app's footer check).
// Nothing installs from here: the cue offers `kete upgrade` and the
// release page.

import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';

export interface UpdateStatus {
  version: string;
  latest: string | null;
  url: string;
  command: string;
}

export const updateApi = {
  status: () => invoke<UpdateStatus>('update_status'),
  onUpdate: (f: (s: UpdateStatus) => void) => listen<UpdateStatus>('kete://update', (e) => f(e.payload)),
};

/** The plate's words: `kete v0.46.0`, and `v0.47.0` when that is out. */
export function plate(s: UpdateStatus | null): { version: string; update: string | null } {
  if (!s) return { version: '', update: null };
  return { version: `kete v${s.version}`, update: s.latest ? `v${s.latest}` : null };
}
