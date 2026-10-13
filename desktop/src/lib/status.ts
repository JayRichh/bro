// What a session's status means on screen: its kanban lane, its dot, its
// words. Lane rules mirror the TUI's (`launcher::lane_of`): the most urgent
// that applies, an open pull request only once nothing else needs you.

import type { Agent, Model, PrInfo, TerminalTab } from './api';

export type Lane = 'needs' | 'running' | 'unseen' | 'done' | 'pr';

export const LANES: { id: Lane; title: string; tone: Tone }[] = [
  { id: 'needs', title: 'Needs input', tone: 'needs' },
  { id: 'running', title: 'Running', tone: 'running' },
  { id: 'unseen', title: 'Unseen', tone: 'unseen' },
  { id: 'done', title: 'Done', tone: 'done' },
  { id: 'pr', title: 'PR open', tone: 'pr' },
];

/** The dot: colour plus glyph, so state never rests on colour alone. */
export type Tone = 'needs' | 'running' | 'unseen' | 'done' | 'idle' | 'dead' | 'pr' | 'shell';

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
      return a.unseen ? 'unseen' : 'done';
    case 'terminated':
    case 'disconnected':
      return 'dead';
    default:
      return 'idle';
  }
}

export function terminalTone(t: TerminalTab): Tone {
  return t.alive ? 'shell' : 'dead';
}

export const TONE_LABEL: Record<Tone, string> = {
  needs: 'Needs input',
  running: 'Running',
  unseen: 'Done, unread',
  done: 'Done',
  idle: 'Idle',
  dead: 'Stopped',
  pr: 'PR open',
  shell: 'Shell running',
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
