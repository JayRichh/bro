// The menus for the window's other targets, read off the model: a GitHub
// pull request chip (`pr:<worktree>`), a port chip (`port:<worktree>:<port>`),
// a board lane's header (`lane:<lane>`), an attention count
// (`attention:<status>`), the orchestrator (`orchestrator:rail`), a
// worktree's where its sessions aren't on screen (`data-menu-jump`), the
// background (empty space anywhere), and, for the surfaces that own them
// (`ownMenu`), the breadcrumb's views and a file's (the diff's file list,
// the opened files' tabs). The rows are rows.ts's; this half reads the
// state and runs the verbs. builtin.ts imports it, so the router knows them
// from the start.

import { api, type Agent } from '../api';
import { app, openSession, toast } from '../store.svelte';
import {
  archiveDone,
  isDone,
  markAllSeen,
  menuFor,
  parseTarget,
  refreshPrs,
  registryRows,
  revealWorktree,
  runTerminalOf,
  targetLabel,
  type VerbTable,
} from '../actions';
import { backgroundMenu, currentRepo, jumpAttention } from '../commands';
import { nav } from '../shell/history.svelte';
import { notices, dismissNotice, openOrchestrator, askStartOrchestrator, openReview, unpinOrchestrator } from '../orch.svelte';
import { orchestratorOf } from '../orchestrator';
import { openMerge } from '../ship/sheet.svelte';
import { repoRows, tally } from '../overview/repos';
import { scopeModel } from '../scope';
import type { Status } from '../overview/tree';
import { keysOf } from '../shortcuts';
import { verbKeys } from '../verbs';
import type { MenuEntry, MenuItem } from '../ui/ui.svelte';
import { copyText } from './clipboard';
import { hasChildren, leafItems } from './model';
import { setBuilder, setRowBuilder, type Opened } from './router';
import { attentionItems, fileItems, laneItems, orchestratorItems, portItems, prItems, viewItems, type LaneId, type PrStanding } from './rows';
import { joinPath } from './surfaces';
// The account rows' menu (`account:<key>`): its own module, registered here.
import '../accounts/account-menu';

const m = () => app.model;
const copy = (text: string, said: string) => void copyText(text, said);
const openUrl = (url: string) => void api.openUrl(url).catch((e) => toast(String(e)));

/** The live sessions in checkout `w`, the one that changed last first. */
function sessionsIn(w: string): Agent[] {
  return m()
    .agents.filter((a) => a.worktree_id === w && !a.archived)
    .sort((a, b) => b.status_changed_at - a.status_changed_at);
}

// ---- the registry's rows, from a crumb or the Worktrees view ----

// A worktree's menu opened somewhere its sessions aren't on screen (the
// Worktrees view, the session header's branch) lists them to open.
setRowBuilder('worktree', (id, el, target) => {
  const t = parseTarget(target);
  if (!t) return null;
  return { items: menuFor(t, { jump: !!el.closest('[data-menu-jump]') }), label: targetLabel(t) };
});

// ---- pull requests (\u00a75.10), ports (\u00a75.13) ----

/** A checkout's pull request's standing. */
export function standingOf(pr: { state?: string; draft: boolean }): PrStanding {
  return pr.state === 'merged' ? 'merged' : pr.state === 'closed' ? 'closed' : pr.draft ? 'draft' : 'open';
}

/** The GitHub pull request of checkout `wt`, its menu. */
export function githubPrMenu(wt: string): Opened | null {
  const pr = m().prs[wt];
  const w = m().worktrees.find((x) => x.id === wt);
  if (!pr || !w) return null;
  const items = prItems(
    {
      number: pr.number,
      url: pr.url,
      standing: standingOf(pr),
      branch: w.branch,
      sessions: sessionsIn(wt).map((a) => ({ id: a.id, name: a.name })),
      worktree: { id: w.id, branch: w.branch },
      canCheckout: false,
    },
    {
      open: () => openUrl(pr.url),
      goSession: (id) => openSession({ Agent: id }),
      goWorktree: () => revealWorktree(w),
      copy,
      merge: () => openMerge(wt, null),
      refresh: refreshPrs,
    },
  );
  return { items, label: `PR #${pr.number}` };
}
setRowBuilder('pr', githubPrMenu);

/** A port chip's menu: `id` is `<worktree>:<port>`. */
export function portMenu(id: string): Opened | null {
  const cut = id.lastIndexOf(':');
  const wt = id.slice(0, cut);
  const port = Number(id.slice(cut + 1));
  if (!Number.isFinite(port)) return null;
  const run = wt ? runTerminalOf(wt) : undefined;
  const runnable = !!wt && (!!app.runCommands[wt] || !!run);
  const items = portItems(
    { port, runTerminal: !!run, running: runnable ? !!run : null },
    {
      open: openUrl,
      copy,
      showRun: () => run && openSession({ Terminal: run.id }),
      toggleRun: () =>
        void (run ? api.stopRun(wt) : api.startRun(wt)).catch((e) => toast(`Could not ${run ? 'stop' : 'start'} the run: ${e}`)),
    },
  );
  return { items, label: `localhost:${port}` };
}
setRowBuilder('port', portMenu);

// ---- board lanes (\u00a75.16) ----

const LANE_IDS: LaneId[] = ['needs', 'running', 'unseen', 'pr', 'done'];
const LANE_WORDS: Record<LaneId, string> = { needs: 'Needs you', running: 'Running', unseen: 'Review', pr: 'Ship', done: 'Done' };
/** Where Open next running got to, round the lane. */
let runningAt = -1;

/** A board lane header's menu. */
export function laneMenu(id: string): Opened | null {
  const lane = id as LaneId;
  if (!LANE_IDS.includes(lane)) return null;
  const agents = () => (m().lanes[lane] ?? []).flatMap((x) => m().agents.filter((a) => a.id === x));
  const list = agents();
  const items = laneItems(
    { lane, count: list.length, unread: list.filter((a) => a.unseen).length, done: list.filter(isDone).length },
    {
      next: () => {
        if (lane === 'needs') return jumpAttention(1);
        const ids = m().lanes[lane] ?? [];
        if (!ids.length) return;
        runningAt = (runningAt + 1) % ids.length;
        openSession({ Agent: ids[runningAt] });
      },
      markSeen: () => markAllSeen(agents()),
      refresh: refreshPrs,
      archiveDone: () => archiveDone(agents, 'in the Done lane'),
    },
    { next: verbKeys('next_attention') },
  );
  return { items, label: LANE_WORDS[lane] };
}
setRowBuilder('lane', laneMenu);

// ---- the attention strip (\u00a75.19) ----

/** An attention count's menu: the sessions it counts. */
export function attentionMenu(id: string): Opened | null {
  const status = id as Status;
  const nodes = tally(repoRows(scopeModel(m())))[status] ?? [];
  const ids = [...new Set(nodes.map((n) => n.agent?.id ?? n.sub?.parent_agent).filter((x): x is string => !!x))];
  const agents = ids
    .map((x) => m().agents.find((a) => a.id === x))
    .filter((a): a is Agent => !!a)
    .sort((a, b) => b.status_changed_at - a.status_changed_at);
  const where = (a: Agent) => {
    const w = m().worktrees.find((x) => x.id === a.worktree_id);
    const p = m().projects.find((x) => x.id === w?.project_id);
    return [p?.name, w?.branch].filter(Boolean).join(' / ');
  };
  const items = attentionItems(
    { sessions: agents.map((a) => ({ id: a.id, name: a.name, where: where(a) })), unread: status === 'unread' },
    { open: (x) => openSession({ Agent: x }), markSeen: () => markAllSeen(agents) },
  );
  return items.length ? { items, label: status } : null;
}
setRowBuilder('attention', attentionMenu);

// ---- the orchestrator (\u00a75.14) ----

/** Every review the orchestrator asked for and nobody opened yet: opened
 *  together, the notices gone. */
function reviewWaiting() {
  const all = [...new Set(notices.list.flatMap((n) => n.sessions))];
  for (const n of [...notices.list]) dismissNotice(n.id);
  if (all.length) openReview(all);
}

/** The orchestrator's menu: the rail entry's carries its session's rows
 *  (`withSession`), flat (submenus never nest), less its checkout's. */
export function orchestratorMenu(withSession: boolean): Opened {
  const o = orchestratorOf(m());
  let session: MenuEntry[] | undefined;
  if (o && withSession) {
    const rows = menuFor({ kind: 'agent', id: o.id }).filter((e) => !(hasChildren(e) && /^(Worktree|Checkout) /.test(e.label)));
    session = leafItems(rows).filter((e) => e.label !== 'Unpin orchestrator');
  }
  const waiting = new Set(notices.list.flatMap((n) => n.sessions)).size;
  const items = orchestratorItems(
    { name: o?.name ?? null, waiting, session },
    { open: openOrchestrator, reviewWaiting, start: askStartOrchestrator, unpin: () => void unpinOrchestrator() },
    { open: [...keysOf('orchestrator'), ...keysOf('orchestrator_f10')] },
  );
  return { items, label: o ? `Orchestrator ${o.name}` : 'Orchestrator' };
}
setRowBuilder('orchestrator', (id) => orchestratorMenu(id === 'rail'));

// ---- view crumbs (\u00a75.18) ----

/** The views to go to: the background's View \u25b8 rows, and a repo's Azure
 *  DevOps views when it has them. */
export function viewMenu(): MenuEntry[] {
  const view = backgroundMenu().find((e) => hasChildren(e) && e.label === 'View') as MenuItem | undefined;
  const rows: MenuEntry[] = [...(view?.children ?? [])];
  const repo = currentRepo();
  if (repo && app.adoCtx[repo]) {
    const here = nav.current.view;
    rows.push(
      'sep',
      ...viewItems(
        (['tasks', 'branches', 'prs'] as const).map((v) => ({
          label: v === 'tasks' ? 'Tasks' : v === 'branches' ? 'Branches' : 'Pull requests',
          on: here === v,
          run: () => nav.go({ view: v, repo }),
        })),
      ),
    );
  }
  return rows;
}

// ---- the background (\u00a75.15) ----

// Empty space anywhere: the window's own verbs. Not inside a dialog or a
// sheet, whose space is theirs.
setBuilder('none', (hit) => {
  const el = hit.el as Element | null;
  if (el?.closest?.('[role="dialog"], [aria-modal="true"]')) return null;
  return { items: backgroundMenu(), label: 'Kete' };
});

// ---- files (\u00a75.8) ----

/** A file, as its menu's registry rows need it: its path on disk, and
 *  within its checkout. */
export interface FileRef {
  abs: string;
  rel: string | null;
}

/** The registry's rows for a file. lane/editors owns these verbs in the
 *  desktop (Open in Kete, Open in VS Code, Visual Studio, Show in
 *  Explorer, the copy verbs); until its rows are in the verb table,
 *  Copy path and Copy relative path (a clipboard write) and Open with
 *  default app where the file was announced (`kete open`) stand in here,
 *  each as the verb it is, so its row takes their place when it lands. */
export function fileTable(f: FileRef, opts: { openDefault?: () => void } = {}): VerbTable {
  const t: VerbTable = { copy_path: { run: () => copy(f.abs, f.abs), note: f.abs } };
  if (f.rel) t.copy_relative_path = { run: () => copy(f.rel!, f.rel!), note: f.rel };
  if (opts.openDefault) t.open_with_default = { run: opts.openDefault };
  return t;
}

/** A changed file in the diff: its registry rows, then Stage / Unstage
 *  and Discard changes where the diff reads the working tree. */
export function diffFileMenu(
  worktree: string,
  file: { path: string; status: string },
  stage: { unstaged: boolean; staged: boolean } | null,
  verbs: { stage(): void; unstage(): void; discard(): void },
): Opened | null {
  const root = m().worktrees.find((w) => w.id === worktree)?.path;
  if (!root) return null;
  const abs = joinPath(root, file.path);
  const items = fileItems(
    registryRows('file', fileTable({ abs, rel: file.path })),
    { path: file.path, deleted: file.status === 'D', untracked: file.status === 'U', stage },
    verbs,
  );
  return { items, label: file.path };
}

/** A file a session opened (`kete open`): its registry rows. */
export function openedFileMenu(path: string, root: string | null, openDefault: () => void): Opened {
  const rel = root && path.toLowerCase().startsWith(root.toLowerCase()) ? path.slice(root.length).replace(/^[\\/]+/, '') : null;
  const none = () => {};
  const items = fileItems(registryRows('file', fileTable({ abs: path, rel }, { openDefault })), { path, deleted: false, untracked: false, stage: null }, {
    stage: none,
    unstage: none,
    discard: none,
  });
  return { items, label: path };
}
