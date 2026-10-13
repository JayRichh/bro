// App-wide state as Svelte runes: the daemon model the Rust side pushes,
// and what the window is looking at.

import { api, type Model, type SessionRef } from './api';

export type View = 'board' | 'session' | 'diff';
export type ThemePref = 'system' | 'light' | 'dark';

const empty: Model = {
  connected: false,
  error: null,
  daemon_pid: 0,
  protocol_version: 0,
  runtime_dir: '',
  projects: [],
  worktrees: [],
  agents: [],
  terminals: [],
  ports: [],
  prs: {},
};

export const app = $state({
  model: empty as Model,
  booted: false,
  view: 'board' as View,
  /** The session the Session and Diff views show. */
  session: null as SessionRef | null,
  /** The worktree the Diff view reads, when no session picks it. */
  diffWorktree: null as string | null,
  diffMode: 'side' as 'side' | 'inline',
  quickPrompt: false,
  themePref: 'system' as ThemePref,
  dark: true,
  now: Date.now(),
  toast: null as string | null,
});

export function sessionKey(s: SessionRef | null): string {
  if (!s) return '';
  return 'Agent' in s ? `a:${s.Agent}` : `t:${s.Terminal}`;
}

/** The worktree a session lives in. */
export function sessionWorktree(s: SessionRef | null): string | null {
  if (!s) return null;
  const m = app.model;
  if ('Agent' in s) return m.agents.find((a) => a.id === s.Agent)?.worktree_id ?? null;
  return m.terminals.find((t) => t.id === s.Terminal)?.worktree_id ?? null;
}

export function openSession(s: SessionRef) {
  app.session = s;
  app.view = 'session';
  if ('Agent' in s) api.markSeen(s.Agent).catch(() => {});
}

export function openDiff(worktree: string | null) {
  app.diffWorktree = worktree;
  app.view = 'diff';
}

let toastTimer: ReturnType<typeof setTimeout> | undefined;
export function toast(msg: string) {
  app.toast = msg;
  clearTimeout(toastTimer);
  toastTimer = setTimeout(() => (app.toast = null), 5000);
}

export async function boot() {
  try {
    app.model = await api.state();
  } catch {
    /* the push below catches up */
  }
  await api.onState((m) => (app.model = m));
  app.booted = true;
  setInterval(() => (app.now = Date.now()), 15_000);
}
