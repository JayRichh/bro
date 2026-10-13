// The window's half of the bridge: the model the Rust side mirrors from the
// daemon, and typed wrappers over its commands. The wire format to the
// daemon lives in Rust (nebula-core); nothing here speaks MessagePack.

import { Channel, invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';

export type AgentStatus =
  | 'fresh'
  | 'running'
  | 'finished'
  | 'needs_feedback'
  | 'terminated'
  | 'disconnected';

export interface Project {
  id: string;
  name: string;
  repo_path: string;
  sort_order: number;
}
export interface Worktree {
  id: string;
  project_id: string;
  path: string;
  branch: string;
  is_main: boolean;
  sort_order: number;
}
export interface PromptEntry {
  text: string;
  submitted_at: number;
}
export interface Agent {
  id: string;
  worktree_id: string;
  name: string;
  status: AgentStatus;
  archived: boolean;
  unseen: boolean;
  status_changed_at: number;
  kind: string;
  custom_harness: string | null;
  model: string | null;
  profile: string | null;
  alive: boolean;
  recent_prompts: PromptEntry[];
  role: string;
  sort_order: number;
}
export interface TerminalTab {
  id: string;
  worktree_id: string;
  name: string;
  sort_order: number;
  alive: boolean;
  run_command: string | null;
}
export interface WorktreePorts {
  worktree: string;
  block: { first: number; size: number } | null;
  live: number[];
}
export interface PrInfo {
  number: number;
  url: string;
  title: string;
  draft: boolean;
}
export interface Model {
  connected: boolean;
  error: string | null;
  daemon_pid: number;
  protocol_version: number;
  runtime_dir: string;
  projects: Project[];
  worktrees: Worktree[];
  agents: Agent[];
  terminals: TerminalTab[];
  ports: WorktreePorts[];
  prs: Record<string, PrInfo>;
}

/** `SessionRef` exactly as serde spells it: externally tagged. */
export type SessionRef = { Agent: string } | { Terminal: string };

export type PtyEvent =
  | { kind: 'scrollback'; data: string }
  | { kind: 'output'; data: string }
  | { kind: 'exited'; code: number | null }
  | { kind: 'refused'; message: string }
  | { kind: 'kitty_flags'; flags: number };

export interface DiffFile {
  path: string;
  from: string | null;
  status: 'M' | 'A' | 'D' | 'R' | 'U' | 'C';
  added: number | null;
  removed: number | null;
}
export interface DiffPair {
  original: string;
  modified: string;
  note: string | null;
}
export interface Launchables {
  harnesses: { id: string; label: string }[];
  profiles: { name: string; harness: string; default: boolean }[];
}
export interface Startup {
  view: string | null;
  session: string | null;
  theme: string | null;
  diff_mode: string | null;
}

export const api = {
  state: () => invoke<Model>('get_state'),
  onState: (f: (m: Model) => void) => listen<Model>('kete://state', (e) => f(e.payload)),
  retry: () => invoke<void>('retry_connect'),
  attach: (session: SessionRef, cols: number, rows: number, onEvent: Channel<PtyEvent>) =>
    invoke<void>('attach', { session, cols, rows, onEvent }),
  detach: (session: SessionRef) => invoke<void>('detach', { session }),
  input: (session: SessionRef, data: string, binary = false) =>
    invoke<void>('input', { session, data, binary }),
  resize: (session: SessionRef, cols: number, rows: number) =>
    invoke<void>('resize', { session, cols, rows }),
  markSeen: (agent: string) => invoke<void>('mark_seen', { agent }),
  createAgent: (spec: { worktree: string; kind: string; profile?: string | null; prompt?: string }) =>
    invoke<string>('create_agent', { spec }),
  createTerminal: (worktree: string) => invoke<string>('create_terminal', { worktree }),
  addProject: (path: string) => invoke<string>('add_project', { path }),
  startRun: (worktree: string) => invoke<string>('start_run', { worktree }),
  diffFiles: (worktree: string) => invoke<DiffFile[]>('diff_files', { worktree }),
  diffPair: (worktree: string, file: DiffFile) => invoke<DiffPair>('diff_pair', { worktree, file }),
  openUrl: (url: string) => invoke<void>('open_url', { url }),
  launchables: () => invoke<Launchables>('launchables'),
  startup: () => invoke<Startup>('startup'),
};

export { Channel };

export function decodeB64(data: string): Uint8Array {
  const bin = atob(data);
  const out = new Uint8Array(bin.length);
  for (let i = 0; i < bin.length; i++) out[i] = bin.charCodeAt(i);
  return out;
}
