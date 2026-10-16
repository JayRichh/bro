// The user's own editors, over src-tauri/src/editors.rs: which VS Code
// family installs and Visual Studio instances this machine has (with their
// icons), what Visual Studio would open in each checkout, which checkouts
// have an editor window open, and the verbs. Every path the window names is
// a worktree id plus a path inside that checkout; the Rust side checks it
// stays there.

import { invoke } from '@tauri-apps/api/core';
import type { FileText } from '../files/files.svelte';

export type Family = 'code' | 'visual-studio';

/** One detected install (nebula-core's `Editor`, with its icon). */
export interface EditorInstall {
  /** `code`, `code-insiders`, `cursor`, `vs-<instanceId>`. */
  id: string;
  family: Family;
  /** What a menu row calls it: "VS Code", "Cursor", "Visual Studio 2022". */
  short: string;
  /** Its own full name: "Visual Studio Community 2022". */
  name: string;
  version: string;
  /** Visual Studio's edition; a Code install's quality. */
  edition: string | null;
  year: string | null;
  prerelease: boolean;
  exe: string;
  dir: string;
  source: string;
  /** Its icon at 32 px as a data URL; null when it couldn't be read. */
  icon: string | null;
}

export interface EditorsState {
  editors: EditorInstall[];
  /** `editor_folder`: auto, vscode, visual-studio, command. */
  folder: string;
  /** `editor_file`: auto, vscode, visual-studio, kete. */
  file: string;
  reuse: boolean;
}

/** A solution or project Visual Studio would open, from the checkout's root. */
export interface SolutionRow {
  rel: string;
  kind: 'slnx' | 'sln' | 'csproj' | 'vcxproj';
}

export interface OpenAsk {
  /** The checkout; null for a file a session opened (`kete open`). */
  worktree?: string | null;
  /** A folder or file in the checkout (relative, or absolute under it). */
  path?: string | null;
  /** An install's id, or `vscode` / `visual-studio` for that family's default. */
  editor?: string | null;
  line?: number | null;
  column?: number | null;
  /** The solution Visual Studio opens; null: the remembered one, else the best. */
  solution?: string | null;
}

export const editorsApi = {
  list: () => invoke<EditorsState>('editors_list'),
  rescan: () => invoke<EditorsState>('editors_rescan'),
  icon: (id: string, size = 32) => invoke<string | null>('editor_icon', { id, size }),
  /** Per worktree id: what Visual Studio would open there (best first) and
   *  whether it has an Open command. */
  targets: (worktrees: string[]) =>
    invoke<Record<string, { solutions: SolutionRow[]; open_command: boolean }>>('editor_targets', { worktrees }),
  /** Per worktree id: the installs with a window that looks open on it. */
  windows: (worktrees: string[]) => invoke<Record<string, string[]>>('editor_windows', { worktrees }),
  open: (ask: OpenAsk) => invoke<{ editor: string; what: string }>('editor_open', { ask }),
  dock: (worktree: string) => invoke<void>('editor_dock', { worktree }),
  /** Show a checkout (its folder opened), or a folder or file in it (selected). */
  reveal: (worktree: string | null, path: string | null = null) => invoke<void>('path_reveal', { worktree, path }),
  /** Open a file with its default app; programs and scripts are refused. */
  openDefault: (worktree: string | null, path: string) => invoke<void>('path_open_default', { worktree, path }),
  readFile: (worktree: string, path: string) => invoke<FileText>('worktree_file_read', { worktree, path }),
};
