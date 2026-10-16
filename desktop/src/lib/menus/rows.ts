// The window's own menus, the ones with no registry kind (the menu
// matrix, \u00a75.10-5.19): a pull request, a branch and a task (GitHub chips,
// the Azure DevOps lists), a port, a board lane's header, the attention
// strip's counts, the orchestrator, a file and the breadcrumb's views.
// Pure, as items.ts: what shows and what is greyed comes from a state the
// caller reads off the page (menus/targets.ts and the surfaces), what a
// row does from the verbs it hands in, so the tests build both.
//
// The words are the terminal app's for the same rows. A short local menu
// (under eight rows) never folds into submenus (\u00a71.4); Vote \u25b8 always is
// one, since its five rows are one choice.

import type { MenuEntry, MenuItem } from '../ui/ui.svelte';
import { layout, type Row } from './layout';
import { submenu } from './model';
import type { MenuSection } from '../actions.gen';

const row = (id: string, section: MenuSection | null, item: MenuItem, group?: Row['group']): Row => ({
  id,
  section,
  item,
  ...(group ? { group } : {}),
});

/** A greyed row, its reason as its tooltip. */
const off = (item: MenuItem, why: string | false | null | undefined): MenuItem => (why ? { ...item, disabled: true, title: why } : item);

/** Lay a local menu out: it folds only from eight rows up. */
export function localMenu(rows: Row[], head: MenuEntry[] = []): MenuEntry[] {
  return layout(rows, { fold: rows.length >= 8, head });
}

const COPY = (label: string) => ({ id: 'copy', title: 'Copy', label });

// ---- pull requests (\u00a75.10) ----

export type PrStanding = 'open' | 'draft' | 'merged' | 'closed';

/** A pull request as its menu needs it. `number`: GitHub's #42 or Azure
 *  DevOps' id. `sessions`: live sessions on its head branch's checkout;
 *  `worktree`: that checkout. `ado`: an Azure DevOps one, `mine` when you
 *  opened it (Publish, not Vote). */
export interface PrState {
  number: number;
  url: string | null;
  standing: PrStanding;
  branch: string | null;
  sessions: { id: string; name: string }[];
  worktree: { id: string; branch: string } | null;
  /** Kete can cut a worktree on its branch here (Azure DevOps). */
  canCheckout: boolean;
  ado?: { mine: boolean };
}

export interface PrVerbs {
  /** Its page in the browser. */
  open(): void;
  /** Azure DevOps: the in-app review (that row's primary). */
  review?(): void;
  goSession(id: string): void;
  goWorktree(id: string): void;
  checkout?(): void;
  copy(text: string, said: string): void;
  /** Azure DevOps vote: 10 approve, 5 with suggestions, -5 wait, -10 reject, 0 reset. */
  vote?(v: number): void;
  publish?(): void;
  merge?(): void;
  refresh(): void;
}

export const VOTES: { v: number; label: string }[] = [
  { v: 10, label: 'Approve' },
  { v: 5, label: 'Approve with suggestions' },
  { v: -5, label: 'Wait for author' },
  { v: -10, label: 'Reject' },
  { v: 0, label: 'Reset' },
];

const DRAFT_MERGE = 'Draft pull request: publish it first';
const DRAFT_VOTE = 'Draft pull request: votes open once it is published';

/** A pull request's menu. Merged or closed: its page, where its work is,
 *  and Copy only. */
export function prItems(s: PrState, v: PrVerbs): MenuEntry[] {
  const live = s.standing === 'open' || s.standing === 'draft';
  const rows: Row[] = [];
  if (v.review) rows.push(row('pr.review', 'act', { label: 'Review', run: v.review }));
  if (s.url)
    rows.push(
      row('pr.open', 'act', {
        label: s.standing === 'merged' || s.standing === 'closed' ? `Open ${s.standing} pull request` : 'Open in browser',
        run: v.open,
        title: s.url,
      }),
    );
  for (const x of s.sessions.slice(0, 3)) rows.push(row('pr.session', 'go', { label: `Go to session ${x.name}`, run: () => v.goSession(x.id) }));
  if (s.worktree) {
    const w = s.worktree;
    rows.push(row('pr.worktree', 'go', { label: `Go to worktree ${w.branch}`, run: () => v.goWorktree(w.id) }));
  } else if (live && s.canCheckout && v.checkout) rows.push(row('pr.checkout', 'go', { label: 'Check out in new worktree\u2026', run: v.checkout }));
  if (s.url) rows.push(row('pr.copy_link', 'copy', { label: 'Copy link', run: () => v.copy(s.url!, s.url!) }, COPY('Link')));
  rows.push(row('pr.copy_number', 'copy', { label: `Copy number (#${s.number})`, run: () => v.copy(`#${s.number}`, `#${s.number}`) }, COPY(`Number (#${s.number})`)));
  if (s.branch) rows.push(row('pr.copy_branch', 'copy', { label: 'Copy branch name', run: () => v.copy(s.branch!, s.branch!) }, COPY('Branch name')));
  if (live) {
    if (s.ado && !s.ado.mine && v.vote) {
      const draft = s.standing === 'draft';
      rows.push(
        row(
          'pr.vote',
          'change',
          off(
            submenu(
              'Vote',
              VOTES.map((x) => ({ label: x.label, run: () => v.vote!(x.v) })),
            ),
            draft && DRAFT_VOTE,
          ),
        ),
      );
    }
    if (s.ado?.mine && s.standing === 'draft' && v.publish) rows.push(row('pr.publish', 'change', { label: 'Publish', run: v.publish }));
    if (s.worktree && v.merge) rows.push(row('pr.merge', 'change', off({ label: 'Merge\u2026', run: v.merge }, s.standing === 'draft' && DRAFT_MERGE)));
    rows.push(row('pr.refresh', 'change', { label: 'Refresh pull requests', run: v.refresh }));
  }
  return localMenu(rows);
}

// ---- branches (\u00a75.11) ----

/** An Azure DevOps branch row. `worktree`, `session`, `pr`, `task`: what
 *  it is linked to. */
export interface BranchState {
  name: string;
  local: boolean;
  worktree: { id: string; branch: string } | null;
  session: { id: string; name: string } | null;
  pr: { id: number } | null;
  task: number | null;
  /** A worktree or main to diff against exists. */
  canDiff: boolean;
  /** Its web page (Azure DevOps). */
  url: string | null;
}

export interface BranchVerbs {
  goWorktree(id: string): void;
  checkout(): void;
  goSession(id: string): void;
  goPr(id: number): void;
  draftPr(): void;
  goTask(id: number): void;
  diff(): void;
  copy(text: string, said: string): void;
  open(url: string): void;
  deleteLocal(): void;
}

export function branchItems(s: BranchState, v: BranchVerbs): MenuEntry[] {
  const rows: Row[] = [];
  const w = s.worktree;
  rows.push(
    w
      ? row('branch.worktree', 'act', { label: `Go to worktree ${w.branch}`, run: () => v.goWorktree(w.id) })
      : row('branch.checkout', 'act', { label: 'Check out in new worktree\u2026', run: v.checkout }),
  );
  if (s.session) {
    const x = s.session;
    rows.push(row('branch.session', 'go', { label: `Go to session ${x.name}`, run: () => v.goSession(x.id) }));
  }
  if (s.pr) {
    const id = s.pr.id;
    rows.push(row('branch.pr', 'go', { label: `Go to pull request #${id}`, run: () => v.goPr(id) }));
  } else if (s.local) rows.push(row('branch.draft', 'go', { label: 'Draft PR\u2026', run: v.draftPr }));
  if (s.task !== null) {
    const id = s.task;
    rows.push(row('branch.task', 'go', { label: `Go to task #${id}`, run: () => v.goTask(id) }));
  }
  if (s.canDiff) rows.push(row('branch.diff', 'go', { label: 'Diff vs main', run: v.diff }));
  if (s.url) rows.push(row('branch.open', 'open', { label: 'Open in Azure DevOps', run: () => v.open(s.url!), title: s.url }));
  rows.push(row('branch.copy', 'copy', { label: 'Copy name', run: () => v.copy(s.name, s.name) }));
  if (s.local)
    rows.push(
      row(
        'branch.delete',
        'away',
        off({ label: 'Delete local\u2026', run: v.deleteLocal, danger: true }, !!w && 'Checked out in a worktree: remove the worktree first'),
      ),
    );
  return localMenu(rows);
}

// ---- tasks (\u00a75.12) ----

export interface TaskState {
  id: number;
  title: string;
  url: string | null;
  /** Completed or Removed: no new session. */
  closed: boolean;
  session: { id: string; name: string } | null;
  branch: string | null;
  pr: number | null;
}

export interface TaskVerbs {
  start(): void;
  goSession(id: string): void;
  goBranch(name: string): void;
  goPr(id: number): void;
  draftPr(): void;
  open(url: string): void;
  copy(text: string, said: string): void;
}

export function taskItems(s: TaskState, v: TaskVerbs): MenuEntry[] {
  const rows: Row[] = [];
  if (s.session) {
    const x = s.session;
    rows.push(row('task.session', 'act', { label: `Go to session ${x.name}`, run: () => v.goSession(x.id) }));
  } else if (!s.closed) rows.push(row('task.start', 'act', { label: 'Start session\u2026', run: v.start }));
  if (s.branch) {
    const b = s.branch;
    rows.push(row('task.branch', 'go', { label: `Go to branch ${b}`, run: () => v.goBranch(b) }));
  }
  if (s.pr !== null) {
    const id = s.pr;
    rows.push(row('task.pr', 'go', { label: `Go to pull request #${id}`, run: () => v.goPr(id) }));
  } else if (s.branch) rows.push(row('task.draft', 'go', { label: 'Draft PR\u2026', run: v.draftPr }));
  if (s.url) rows.push(row('task.open', 'open', { label: 'Open in Azure DevOps', run: () => v.open(s.url!), title: s.url }));
  rows.push(row('task.copy_id', 'copy', { label: `Copy id (#${s.id})`, run: () => v.copy(`#${s.id}`, `#${s.id}`) }, COPY(`Id (#${s.id})`)));
  if (s.url) rows.push(row('task.copy_link', 'copy', { label: 'Copy link', run: () => v.copy(s.url!, s.url!) }, COPY('Link')));
  rows.push(row('task.copy_title', 'copy', { label: 'Copy title', run: () => v.copy(s.title, 'the title') }, COPY('Title')));
  return localMenu(rows);
}

// ---- ports (\u00a75.13) ----

/** A `:5173` chip: the port, whether its checkout's run terminal is up,
 *  and whether the checkout has a run command (Run / Stop run). */
export interface PortState {
  port: number;
  runTerminal: boolean;
  /** null: no run command, so no Run row. */
  running: boolean | null;
}

export interface PortVerbs {
  open(url: string): void;
  copy(text: string, said: string): void;
  showRun(): void;
  toggleRun(): void;
}

export function portItems(s: PortState, v: PortVerbs): MenuEntry[] {
  const url = `http://localhost:${s.port}`;
  const rows: Row[] = [row('port.open', 'act', { label: `Open localhost:${s.port} in browser`, run: () => v.open(url), title: url })];
  if (s.runTerminal) rows.push(row('port.run_terminal', 'go', { label: 'Show run terminal', run: v.showRun }));
  rows.push(row('port.copy', 'copy', { label: 'Copy URL', run: () => v.copy(url, url) }));
  if (s.running !== null) rows.push(row('port.run', 'change', { label: s.running ? 'Stop run' : 'Run', run: v.toggleRun }));
  return localMenu(rows);
}

// ---- board lanes (\u00a75.16) ----

export type LaneId = 'needs' | 'running' | 'unseen' | 'pr' | 'done';

/** A lane's header: what is in it now. `done`: finished, read sessions in
 *  the Done lane (Archive done sessions takes them). */
export interface LaneState {
  lane: LaneId;
  count: number;
  unread: number;
  done: number;
}

export interface LaneVerbs {
  next(): void;
  markSeen(): void;
  refresh(): void;
  archiveDone(): void;
}

export function laneItems(s: LaneState, v: LaneVerbs, keys: { next?: string[] } = {}): MenuEntry[] {
  switch (s.lane) {
    case 'needs':
      return [off({ label: 'Open next needing you', run: v.next, keys: keys.next }, !s.count && 'Nothing needs you')];
    case 'running':
      return [off({ label: 'Open next running', run: v.next }, !s.count && 'Nothing is running')];
    case 'unseen':
      return [off({ label: 'Mark all seen', run: v.markSeen }, !s.unread && 'Nothing unread here')];
    case 'pr':
      return [{ label: 'Refresh pull requests', run: v.refresh }];
    case 'done':
      return [off({ label: 'Archive done sessions\u2026', run: v.archiveDone }, !s.done && 'No finished, read sessions here')];
  }
}

// ---- the attention strip (\u00a75.19) ----

/** One count's sessions, newest change first. */
export interface AttentionState {
  sessions: { id: string; name: string; where: string }[];
  /** The unread count: the list ends with Mark all seen. */
  unread: boolean;
}

export function attentionItems(s: AttentionState, v: { open(id: string): void; markSeen(): void }): MenuEntry[] {
  const out: MenuEntry[] = s.sessions.map((x) => ({ label: x.name, run: () => v.open(x.id), title: x.where }));
  if (s.unread && s.sessions.length) out.push('sep', { label: 'Mark all seen', run: v.markSeen });
  return out;
}

// ---- the orchestrator (\u00a75.14) ----

export interface OrchState {
  /** The pinned orchestrator's name; null: none yet. */
  name: string | null;
  /** Reviews it asked for that are still waiting. */
  waiting: number;
  /** Its session's own rows (Session \u25b8), flat. */
  session?: MenuEntry[];
}

export interface OrchVerbs {
  open(): void;
  reviewWaiting(): void;
  start(): void;
  unpin(): void;
}

/** The rail entry's menu (`session` given) and the panel background's. */
export function orchestratorItems(s: OrchState, v: OrchVerbs, keys: { open?: string[] } = {}): MenuEntry[] {
  if (!s.name) return [{ label: 'Start an orchestrator\u2026', run: v.start, keys: keys.open }];
  const out: MenuEntry[] = [{ label: 'Open orchestrator', run: v.open, keys: keys.open, title: s.name }];
  if (s.waiting) out.push({ label: `Review ${s.session ? 'waiting' : 'all waiting'} (${s.waiting})\u2026`, run: v.reviewWaiting });
  if (s.session?.length) out.push('sep', submenu(`Session ${s.name}`, s.session));
  out.push('sep', { label: 'Start a new orchestrator\u2026', run: v.start });
  if (s.session) out.push({ label: 'Unpin orchestrator', run: v.unpin });
  return out;
}

// ---- files (\u00a75.8) ----

/** A changed file in the diff, or a file a session opened. `stage`: the
 *  diff reads the working tree against HEAD, so its hunks stage and
 *  discard; null where they don't. */
export interface FileState {
  path: string;
  deleted: boolean;
  untracked: boolean;
  stage: { unstaged: boolean; staged: boolean } | null;
}

export interface FileVerbs {
  stage(): void;
  unstage(): void;
  discard(): void;
}

/** A file's menu: the registry's rows for it (`registry`, from the verb
 *  table: Open in Kete, Open in VS Code, Show in Explorer, Copy path...),
 *  then Stage / Unstage and Discard changes where the diff can. */
export function fileItems(registry: Row[], s: FileState, v: FileVerbs): MenuEntry[] {
  const rows = [...registry];
  if (s.stage) {
    if (s.stage.unstaged || s.untracked)
      rows.push(
        row(
          'file.stage',
          'change',
          off({ label: 'Stage file', run: v.stage }, s.untracked && 'Untracked: there are no hunks to stage yet. Add the file whole first.'),
        ),
      );
    if (s.stage.staged) rows.push(row('file.unstage', 'change', { label: 'Unstage file', run: v.unstage }));
    if (s.stage.unstaged && !s.untracked) rows.push(row('file.discard', 'away', { label: 'Discard changes\u2026', run: v.discard, danger: true }));
  }
  return layout(rows, { fold: false });
}

// ---- the breadcrumb's views (\u00a75.18) ----

/** A view crumb (Overview, Board, Worktrees, Tasks...): the views to go
 *  to, the current one ticked. */
export function viewItems(views: { label: string; on: boolean; run: () => void; keys?: string[] }[]): MenuEntry[] {
  return views.map((x) => ({ label: x.label, run: x.run, checked: x.on, radio: true, ...(x.keys ? { keys: x.keys } : {}) }));
}
