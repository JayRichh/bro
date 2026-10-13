// Typed wrappers over the desktop's `ado_*` commands. Field names match the
// Rust structs as serde writes them (camelCase).

import { invoke } from '@tauri-apps/api/core';

/** A repo's Azure DevOps remote, and the view state saved for it. */
export interface AdoContextInfo {
  host: string;
  org: string;
  project: string;
  repo: string;
  orgUrl: string;
  /** `org/project/repo`, what `adoSaveUi` takes. */
  repoKey: string;
  /** Per-view UI state saved earlier, keyed by view. */
  ui: Record<string, unknown>;
}

export const ado = {
  /** The ADO remote of a worktree (daemon id or path); null when it has none. */
  context: (worktree: string) => invoke<AdoContextInfo | null>('ado_context', { worktree }),
  /** Save one view's UI state for a repo into ado.json. */
  saveUi: (repoKey: string, view: string, json: unknown) =>
    invoke<void>('ado_save_ui', { repoKey, view, json }),
  /** The last ado.json load or write failure, once (reading clears it). */
  storeError: () => invoke<string | null>('ado_store_error'),
};
