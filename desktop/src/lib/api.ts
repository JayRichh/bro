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
  /** The session that started this one with `kete spawn`; null for a root. */
  parent: string | null;
}
/** A Claude in-session subagent, hung off the session it runs inside. */
export interface Subagent {
  /** Unique within its parent session only. */
  id: string;
  parent_agent: string;
  /** `general-purpose`, `Explore`, a custom agent; empty when unknown. */
  kind: string;
  /** The task the parent gave it; empty when unknown. */
  description: string;
  started_at: number;
  ended_at: number | null;
  status: 'running' | 'done' | 'failed' | 'ended_unknown';
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
  subagents: Subagent[];
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
  /** The Agents tab's quick prompt harness; empty from an old backend. */
  default_harness?: string;
}

/** Where a settings write lands, as the Rust side's `schema::Target`. */
export type SettingTarget =
  | { scope: 'global'; key: string }
  | { scope: 'agent'; harness: string; field: string }
  | { scope: 'project'; path: string; field: string }
  | { scope: 'desktop'; key: string };
export type SettingControl =
  | { type: 'toggle' }
  | { type: 'choice'; options: string[]; labels: string[] }
  | { type: 'text'; placeholder: string };
export interface SettingRow {
  target: SettingTarget;
  label: string;
  hint: string;
  control: SettingControl;
  value: boolean | string;
  default: boolean | string;
  restart: string | null;
  is_new: boolean;
}
export interface SettingGroup {
  title: string;
  subtitle: string | null;
  terminal: boolean;
  rows: SettingRow[];
}
export interface HotkeyRow {
  id: string;
  label: string;
  hint: string;
  group: string;
  defaults: string[];
  bound: string[];
  overridden: boolean;
}
export interface SettingsTab {
  id: string;
  title: string;
  note: string | null;
  groups: SettingGroup[];
  hotkeys: HotkeyRow[];
}
export interface SettingsSchema {
  tabs: SettingsTab[];
}
export interface DesktopPrefs {
  theme: string;
  diff_mode: string;
  home: string;
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
  stopRun: (worktree: string) => invoke<void>('stop_run', { worktree }),
  /** Worktree id to the command its Run button starts; absent: none. */
  runCommands: () => invoke<Record<string, string>>('run_commands'),
  settingsSchema: () => invoke<SettingsSchema>('settings_schema'),
  settingsSet: (target: SettingTarget, value: boolean | string) => invoke<void>('settings_set', { target, value }),
  settingsReset: () => invoke<void>('settings_reset'),
  desktopPrefs: () => invoke<DesktopPrefs>('desktop_prefs'),
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
