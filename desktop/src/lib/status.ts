// What a session's status means on screen: its kanban lane, its dot, its
// words. Lane rules mirror the TUI's (`launcher::lane_of`): the most urgent
// that applies, an open pull request only once nothing else needs you.

import type { Agent, Model, PrInfo, Subagent, TerminalTab } from './api';

export type Lane = 'needs' | 'running' | 'unseen' | 'done' | 'pr';

/** A session's state on screen: a square plus a word, never colour alone. */
export type Tone =
  | 'needs'
  | 'failed'
  | 'unread'
  | 'running'
  | 'done'
  | 'merged'
  | 'pr'
  | 'idle'
  | 'stopped'
  | 'shell';

export const LANES: { id: Lane; title: string; tone: Tone }[] = [
  { id: 'needs', title: 'Needs you', tone: 'needs' },
  { id: 'running', title: 'Running', tone: 'running' },
  { id: 'unseen', title: 'Unread', tone: 'unread' },
  { id: 'done', title: 'Done', tone: 'done' },
  { id: 'pr', title: 'PR open', tone: 'pr' },
];

export function laneOf(a: Agent, pr: PrInfo | undefined): Lane {
  if (a.status === 'needs_feedback') return 'needs';
  if (a.status === 'running') return 'running';
  if (a.unseen) return 'unseen';
  if (pr) return 'pr';
  return 'done';
}

export function toneOf(a: Agent): Tone {
  switch (a.status) {
    case 'needs_feedback':
      return 'needs';
    case 'running':
      return 'running';
    case 'finished':
      return a.unseen ? 'unread' : 'done';
    case 'terminated':
    case 'disconnected':
      return a.unseen ? 'unread' : 'stopped';
    default:
      return 'idle';
  }
}

export function subagentTone(s: Subagent): Tone {
  switch (s.status) {
    case 'running':
      return 'running';
    case 'done':
      return 'done';
    case 'failed':
      return 'failed';
    default:
      return 'stopped';
  }
}

export function terminalTone(t: TerminalTab): Tone {
  return t.alive ? 'shell' : 'stopped';
}

export const TONE_LABEL: Record<Tone, string> = {
  needs: 'needs you',
  failed: 'failed',
  unread: 'unread',
  running: 'running',
  done: 'done',
  merged: 'merged',
  pr: 'PR open',
  idle: 'idle',
  stopped: 'stopped',
  shell: 'shell',
};

const HARNESS: Record<string, string> = {
  claude: 'Claude',
  codex: 'Codex',
  cursor: 'Cursor',
  pi: 'Pi',
  muse: 'Muse',
  grok: 'Grok',
  opencode: 'OpenCode',
  copilot: 'Copilot',
  custom: 'Custom',
};

export function harnessLabel(a: Agent): string {
  if (a.kind === 'custom' && a.custom_harness) return a.custom_harness;
  return HARNESS[a.kind] ?? a.kind;
}

export function lastPrompt(a: Agent): string | null {
  let best: { text: string; submitted_at: number } | null = null;
  for (const p of a.recent_prompts) if (!best || p.submitted_at >= best.submitted_at) best = p;
  return best?.text ?? null;
}

export function age(ms: number, now: number): string {
  if (!ms) return '';
  const s = Math.max(0, Math.round((now - ms) / 1000));
  if (s < 45) return 'now';
  const m = Math.round(s / 60);
  if (m < 60) return `${m}m`;
  const h = Math.round(m / 60);
  if (h < 48) return `${h}h`;
  return `${Math.round(h / 24)}d`;
}

export interface Placed {
  project: string;
  branch: string;
  isMain: boolean;
  path: string;
}

export function placeOf(m: Model, worktreeId: string): Placed {
  const w = m.worktrees.find((x) => x.id === worktreeId);
  const p = w && m.projects.find((x) => x.id === w.project_id);
  return {
    project: p?.name ?? '?',
    branch: w?.branch ?? '?',
    isMain: w?.is_main ?? false,
    path: w?.path ?? '',
  };
}

export function lanes(m: Model): Record<Lane, Agent[]> {
  const out: Record<Lane, Agent[]> = { needs: [], running: [], unseen: [], done: [], pr: [] };
  const visible = m.agents.filter((a) => !a.archived && a.role !== 'orchestrator');
  for (const a of visible) out[laneOf(a, m.prs[a.worktree_id])].push(a);
  for (const l of Object.values(out)) l.sort((x, y) => y.status_changed_at - x.status_changed_at);
  return out;
}
