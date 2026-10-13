// App-wide state as Svelte runes: the daemon model the Rust side pushes,
// and what the window is looking at. Where the window is lives in
// `nav` (shell/history.svelte.ts); this file follows it.

import { api, type Launchables, type Model, type SessionRef } from './api';
import { ado, type AdoContextInfo } from './ado/api';
import { nav, type Place } from './shell/history.svelte';

export type ThemePref = 'system' | 'light' | 'dark';
/** What the main column shows when no view is open: the overview or the board. */
export type Home = 'overview' | 'board';
export type QuickPreset = { project?: string; harness?: string; profile?: string };

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
  subagents: [],
};

export const app = $state({
  model: empty as Model,
  booted: false,
  home: 'overview' as Home,
  /** The session the Session and Diff views show. */
  session: null as SessionRef | null,
  /** The worktree the Diff view reads, when no session picks it. */
  diffWorktree: null as string | null,
  diffMode: 'side' as 'side' | 'inline',
  /** Bumped to ask the quick prompt to take focus. */
  quickFocus: 0,
  /** Harness and profile the quick prompt should switch to, once. */
  quickPreset: null as QuickPreset | null,
  /** Width of the right split (session, diff) in px. */
  splitWidth: 0,
  themePref: 'system' as ThemePref,
  dark: false,
  now: Date.now(),
  toast: null as string | null,
  launch: { harnesses: [], profiles: [] } as Launchables,
  /** Per project id: its ADO remote, null when none, absent until asked. */
  adoCtx: {} as Record<string, AdoContextInfo | null>,
  /** A repo picked in the rail, for the overview to scroll to; seq bumps per pick. */
  reveal: { repo: null as string | null, seq: 0 },
});

export function sessionKey(s: SessionRef | null): string {
  if (!s) return '';
  return 'Agent' in s ? `a:${s.Agent}` : `t:${s.Terminal}`;
}

export function sessionFromKey(k: string | undefined): SessionRef | null {
  if (!k) return null;
  if (k.startsWith('a:')) return { Agent: k.slice(2) };
  if (k.startsWith('t:')) return { Terminal: k.slice(2) };
  return null;
}

/** The worktree a session lives in. */
export function sessionWorktree(s: SessionRef | null): string | null {
  if (!s) return null;
  const m = app.model;
  if ('Agent' in s) return m.agents.find((a) => a.id === s.Agent)?.worktree_id ?? null;
  return m.terminals.find((t) => t.id === s.Terminal)?.worktree_id ?? null;
}

/** The project a worktree belongs to. */
export function projectOf(worktree: string | null): string | undefined {
  if (!worktree) return undefined;
  return app.model.worktrees.find((w) => w.id === worktree)?.project_id;
}

export function openSession(s: SessionRef) {
  app.session = s;
  nav.go({ view: 'session', repo: projectOf(sessionWorktree(s)), item: sessionKey(s) });
  if ('Agent' in s) api.markSeen(s.Agent).catch(() => {});
}

export function openDiff(worktree: string | null) {
  app.diffWorktree = worktree;
  nav.go({ view: 'diff', repo: projectOf(worktree), item: worktree ?? undefined });
}

/** Back to the overview (or the board), closing any split. */
export function goHome(repo?: string) {
  nav.go({ view: app.home, repo });
}

/** A repo picked in the rail: go home and scroll its row into view. */
export function revealRepo(repo: string) {
  goHome(repo);
  app.reveal = { repo, seq: app.reveal.seq + 1 };
}

/** Make the window's selection follow a place reached by back or forward. */
export function follow(p: Place) {
  if (p.view === 'overview' || p.view === 'board') app.home = p.view;
  if (p.view === 'session') {
    const s = sessionFromKey(p.item);
    if (s) app.session = s;
  } else if (p.view === 'diff') {
    app.diffWorktree = p.item ?? null;
  }
}

/** Ask the quick prompt for focus, optionally switching harness and profile. */
export function focusQuickPrompt(preset?: QuickPreset) {
  if (preset) app.quickPreset = preset;
  app.quickFocus += 1;
}

let toastTimer: ReturnType<typeof setTimeout> | undefined;
export function toast(msg: string) {
  app.toast = msg;
  clearTimeout(toastTimer);
  toastTimer = setTimeout(() => (app.toast = null), 5000);
}

/** Look up each project's ADO remote once (each lookup runs git). */
const asked = new Set<string>();
/** A failed lookup is asked again, at most this often. */
const RETRY_MS = 30_000;
const failedAt = new Map<string, number>();
function askAdo(m: Model) {
  for (const p of m.projects) {
    if (asked.has(p.id)) continue;
    if (Date.now() - (failedAt.get(p.id) ?? 0) < RETRY_MS) continue;
    const main =
      m.worktrees.find((w) => w.project_id === p.id && w.is_main) ?? m.worktrees.find((w) => w.project_id === p.id);
    if (!main) continue;
    asked.add(p.id);
    ado
      .context(main.id)
      .then((ctx) => {
        app.adoCtx[p.id] = ctx;
        if (ctx) nav.seed(p.id, ctx.ui);
      })
      .catch(() => {
        // A failed lookup is not "no ADO remote": ask again later.
        asked.delete(p.id);
        failedAt.set(p.id, Date.now());
        app.adoCtx[p.id] = null;
      });
  }
}

/** View state goes to ado.json for repos that live on ADO; others keep it in memory. */
nav.setSaver((view, repo, state) => {
  const ctx = app.adoCtx[repo];
  if (ctx) ado.saveUi(ctx.repoKey, view, state).catch(() => {});
});

function take(m: Model) {
  app.model = m;
  askAdo(m);
}

export async function boot() {
  try {
    take(await api.state());
  } catch {
    /* the push below catches up */
  }
  await api.onState(take);
  app.booted = true;
  api
    .launchables()
    .then((l) => (app.launch = l))
    .catch(() => {});
  setInterval(() => (app.now = Date.now()), 15_000);
}
