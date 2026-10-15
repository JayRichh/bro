// The orchestrator's verbs in the window: open its view, start one, pin or
// unpin a session, send it a prompt, and the reviews it asks for (a notice
// that never takes the focus, then the review sheet). The model reads are
// in orchestrator.ts; the menus and the palette call in here.

import { api, type Agent, type ReviewOpened, type SessionRef } from './api';
import { app, markSeen, toast } from './store.svelte';
import { nav } from './shell/history.svelte';
import { ask, ui, type MenuEntry, type ReviewTab } from './ui/ui.svelte';
import { keysOf } from './shortcuts';
import { canProvision, isOrchestrator, orchestratorOf, RELOAD_HINT } from './orchestrator';
import type { PaletteCommand } from './palette';

export const REVIEW_TABS: ReviewTab[] = ['terminal', 'diff', 'history', 'pr'];

/** Reviews the orchestrator asked for that the user has not opened yet. */
export const notices = $state({ list: [] as { id: number; sessions: string[]; tab: ReviewTab }[] });
let nextNotice = 1;

/** The orchestrator view: its session beside the tree, or the Start form. */
export function openOrchestrator() {
  const o = orchestratorOf(app.model);
  if (o) {
    const s: SessionRef = { Agent: o.id };
    app.session = s;
    markSeen(s);
  }
  nav.go({ view: 'orchestrator' });
}

/** Leave the provisioning requests unsent to a daemon that cannot take them. */
function guard(): boolean {
  if (canProvision(app.model)) return true;
  toast(app.model.connected ? RELOAD_HINT : 'Not connected to the kete daemon.');
  return false;
}

/** Open the Start orchestrator form (or say why not). */
export function askStartOrchestrator() {
  if (!guard()) return;
  ui.startOrchestrator = true;
}

export async function startOrchestrator(spec: {
  kind: string;
  profile: string | null;
  model: string | null;
  effort: string | null;
  project?: string | null;
  name?: string | null;
}): Promise<boolean> {
  if (!guard()) return false;
  try {
    const id = await api.startOrchestrator({ ...spec, replace: !!orchestratorOf(app.model) });
    ui.startOrchestrator = false;
    if (id) app.session = { Agent: id };
    nav.go({ view: 'orchestrator' });
    return true;
  } catch (e) {
    toast(`Could not start the orchestrator: ${e}`);
    return false;
  }
}

export async function makeOrchestrator(a: Agent) {
  if (!guard()) return;
  const now = orchestratorOf(app.model);
  if (now && now.id !== a.id) {
    const r = await ask.confirm({
      title: 'Make orchestrator',
      message: `Make "${a.name}" the orchestrator? "${now.name}" keeps running as an ordinary session.`,
      confirm: 'Make orchestrator',
    });
    if (!r.ok) return;
  }
  try {
    await api.setOrchestrator(a.id);
    toast(`${a.name} is the orchestrator. F10 or 0 opens it.`);
  } catch (e) {
    toast(`Could not pin it: ${e}`);
  }
}

export async function unpinOrchestrator() {
  if (!guard()) return;
  const now = orchestratorOf(app.model);
  if (!now) return;
  try {
    await api.clearOrchestrator();
    toast(`${now.name} is an ordinary session again.`);
  } catch (e) {
    toast(`Could not unpin it: ${e}`);
  }
}

/** The quick prompt's "to orchestrator": a follow-up turn, no new session. */
export async function sendToOrchestrator(text: string): Promise<boolean> {
  const o = orchestratorOf(app.model);
  if (!o) {
    toast('There is no orchestrator. Start one first (0).');
    return false;
  }
  try {
    await api.followUp(o.id, text);
    return true;
  } catch (e) {
    toast(`Could not send: ${e}`);
    return false;
  }
}

/** Open the review sheet on agent sessions. */
export function openReview(sessions: string[], tab: ReviewTab = 'terminal') {
  const live = sessions.filter((id) => app.model.agents.some((a) => a.id === id));
  if (!live.length) return toast('Those sessions are gone.');
  ui.palette = false;
  ui.help = false;
  ui.review = { sessions: live, at: 0, tab };
}

export function closeReview() {
  ui.review = null;
}

/** A `ReviewOpened` broadcast: a notice with Open, never a focus grab. */
export function onReviewOpened(r: ReviewOpened) {
  const sessions = r.sessions.flatMap((s: SessionRef) => ('Agent' in s ? [s.Agent] : []));
  if (!sessions.length) return;
  const tab = (r.tabs.find((t) => REVIEW_TABS.includes(t as ReviewTab)) as ReviewTab | undefined) ?? 'terminal';
  notices.list = [...notices.list.filter((n) => n.sessions.join() !== sessions.join()), { id: nextNotice++, sessions, tab }];
}

export function dismissNotice(id: number) {
  notices.list = notices.list.filter((n) => n.id !== id);
}

export function openNotice(id: number) {
  const n = notices.list.find((x) => x.id === id);
  dismissNotice(id);
  if (n) openReview(n.sessions, n.tab);
}

/** The notice's words: "Orchestrator wants you to review fix-login". */
export function noticeText(sessions: string[]): string {
  const names = sessions.map((id) => app.model.agents.find((a) => a.id === id)?.name ?? 'a session');
  const what = names.length > 2 ? `${names.slice(0, 2).join(', ')} and ${names.length - 2} more` : names.join(' and ');
  return `Orchestrator wants you to review ${what}`;
}

/** Menu rows for a session: pin or unpin, and Review. */
export function orchestratorMenuItems(a: Agent): MenuEntry[] {
  if (a.archived) return [];
  const pin = isOrchestrator(a)
    ? { label: 'Unpin orchestrator', run: () => void unpinOrchestrator() }
    : { label: 'Make orchestrator', run: () => void makeOrchestrator(a) };
  return [{ label: 'Review\u2026', run: () => openReview([a.id]) }, pin];
}

type Command = PaletteCommand & { run: () => unknown };

/** Palette rows: open, start (or replace), unpin. */
export function orchestratorCommands(): Command[] {
  const o = orchestratorOf(app.model);
  const keys = [...keysOf('orchestrator'), ...keysOf('orchestrator_f10')];
  const pending: Command[] = notices.list.map((n) => ({
    id: `orch-review-${n.id}`,
    label: noticeText(n.sessions).replace('Orchestrator wants you to review', 'Open the review of'),
    group: 'Orchestrator',
    words: 'review asked',
    run: () => openNotice(n.id),
  }));
  return [...pending, ...pinCommands(o, keys)];
}

function pinCommands(o: Agent | undefined, keys: string[]): Command[] {
  if (!o)
    return [{ id: 'orch-start', label: 'Start orchestrator\u2026', group: 'Orchestrator', keys, words: 'coordinator pin', run: askStartOrchestrator }];
  return [
    { id: 'orch-open', label: `Open orchestrator (${o.name})`, group: 'Orchestrator', keys, words: 'coordinator chat', run: openOrchestrator },
    { id: 'orch-replace', label: 'Start a new orchestrator\u2026', group: 'Orchestrator', words: 'replace', run: askStartOrchestrator },
    { id: 'orch-unpin', label: 'Unpin orchestrator', group: 'Orchestrator', words: 'clear demote', run: unpinOrchestrator },
  ];
}
