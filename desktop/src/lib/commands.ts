// The window's actions by shortcut id: what a key from the shortcut table
// does, and what the command palette offers besides jumping. App.svelte
// dispatches keys through `runShortcut`; the palette lists `paletteCommands`.

import { api } from './api';
import { nav } from './shell/history.svelte';
import {
  app,
  focusQuickPrompt,
  closeDiff,
  goHome,
  openDiff,
  openSession,
  openSettings,
  projectOf,
  sessionWorktree,
  setHome,
  setThemePref,
  toast,
  type ThemePref,
} from './store.svelte';
import { keysOf, type ShortcutId } from './shortcuts';
import { menuFor, newWorktree, parseTarget, refreshPrs, runRowKey, type Target } from './actions';
import { pickBranchSwitch } from './worktrees/switcher.svelte';
import { ask, openMenu, ui, type MenuItem } from './ui/ui.svelte';
import { attentionSessions, stepRing, type PaletteCommand } from './palette';
import { askStartOrchestrator, openOrchestrator, orchestratorCommands } from './orch.svelte';
import { orchestratorOf } from './orchestrator';
import { openAccounts } from './accounts/actions';
import { openAddAccount } from './accounts/add.svelte';
import { moveStep, rootOf, stepInFor } from './reorder.svelte';

/** 0 / F10: the orchestrator, or the Start form when there is none. */
function orchestratorKey() {
  if (orchestratorOf(app.model)) openOrchestrator();
  else askStartOrchestrator();
}

export function cycleTheme() {
  const order: ThemePref[] = ['system', 'light', 'dark'];
  setThemePref(order[(order.indexOf(app.themePref) + 1) % order.length]);
}

export function toggleHome() {
  setHome(app.home === 'overview' ? 'board' : 'overview', nav.current.repo);
}

export function toggleArchived() {
  app.showArchived = !app.showArchived;
  if (app.home !== 'overview' || (nav.current.view !== 'overview' && nav.current.view !== 'board')) setHome('overview', nav.current.repo);
}

/** `.` / `,`: open the next (or previous) session in attention order. */
export function jumpAttention(step: 1 | -1) {
  const s = app.session;
  const next = stepRing(attentionSessions(app.model), s && 'Agent' in s ? s.Agent : null, step);
  if (next) openSession({ Agent: next });
  else toast('No sessions to jump to.');
}

/** The repo the window is on: the one in view, else the open session's. */
export function currentRepo(): string | undefined {
  return nav.current.repo ?? projectOf(sessionWorktree(app.session));
}

/** The session row or card with the focus, if any. */
export function focusedTarget(): Target | null {
  const el = (document.activeElement as HTMLElement | null)?.closest<HTMLElement>('[data-target]');
  return parseTarget(el?.dataset.target);
}

/** The session the window shows, as a target. */
export function currentTarget(): Target | null {
  const s = app.session;
  if (!s) return null;
  return 'Agent' in s ? { kind: 'agent', id: s.Agent } : { kind: 'terminal', id: s.Terminal };
}

/** The checkout a diff opened from here reads: the focused row's (a
 *  session, card, worktree or repo), the session's in a session view, the
 *  repo's root in its own views (Worktrees, the ADO ones), else the one
 *  last diffed or the open session's. */
export function diffTarget(): string | null {
  const t = focusedTarget();
  const m = app.model;
  const focused =
    t?.kind === 'agent'
      ? m.agents.find((a) => a.id === t.id)?.worktree_id
      : t?.kind === 'terminal'
        ? m.terminals.find((x) => x.id === t.id)?.worktree_id
        : t?.kind === 'worktree'
          ? t.id
          : t?.kind === 'project'
            ? rootOf(t.id)
            : undefined;
  if (focused) return focused;
  const v = nav.current.view;
  if (v === 'session' || v === 'orchestrator') return sessionWorktree(app.session) ?? app.diffWorktree;
  const repo = nav.current.repo;
  if (repo && v !== 'overview' && v !== 'board' && v !== 'diff') return rootOf(repo) ?? null;
  return app.diffWorktree ?? sessionWorktree(app.session) ?? (repo ? (rootOf(repo) ?? null) : null);
}

/** Ctrl+Shift+D and the header's Diff: the diff for the checkout in
 *  context, or, from the diff, back to where it was opened. */
export function toggleDiff() {
  if (nav.current.view === 'diff') closeDiff();
  else openDiff(diffTarget());
}

const split = () => nav.current.view === 'session' || nav.current.view === 'diff' || nav.current.view === 'orchestrator';

/** Ctrl+Shift+M and the panes' Full pane button: the split's right side
 *  takes the window, or comes back down. Nothing to do with no split up. */
export function toggleMaximize(): boolean {
  if (!split()) return false;
  app.paneMax = !app.paneMax;
  return true;
}

/** P, N, Ctrl+Shift+N: the quick prompt, on the focused row's repo when a
 *  row, card or repo header has the focus. */
function quickPrompt() {
  const repo = focusedRepo();
  focusQuickPrompt(repo ? { project: repo } : undefined);
}

/** Run the action a shortcut names. False when it does nothing here. */
export function runShortcut(id: ShortcutId): boolean {
  const go: Partial<Record<ShortcutId, () => unknown>> = {
    palette: () => ((ui.help = false), (ui.palette = true)),
    palette_alt: () => ((ui.help = false), (ui.palette = true)),
    help: () => ((ui.palette = false), (ui.help = !ui.help)),
    help_f1: () => ((ui.palette = false), (ui.help = !ui.help)),
    settings: () => openSettings(),
    back: () => nav.back(),
    forward: () => nav.forward(),
    home_board: () => setHome('board', nav.current.repo),
    home_overview: () => setHome('overview', nav.current.repo),
    toggle_home: toggleHome,
    open_session: () => app.session && openSession(app.session),
    open_diff: toggleDiff,
    quick_prompt: quickPrompt,
    quick_prompt_alt: quickPrompt,
    cycle_theme: cycleTheme,
    go_home: () => goHome(nav.current.repo),
    go_session: () => app.session && openSession(app.session),
    go_diff: () => openDiff(app.diffWorktree ?? sessionWorktree(app.session)),
    orchestrator: orchestratorKey,
    orchestrator_f10: orchestratorKey,
    accounts: openAccounts,
    close_split: () => {
      if (!split() && nav.current.view !== 'settings' && nav.current.view !== 'accounts') return false;
      // A full pane comes back down first; the next Esc closes the split.
      if (app.paneMax && split()) return void (app.paneMax = false);
      if (nav.current.view === 'diff') return closeDiff();
      goHome(nav.current.repo);
    },
    toggle_archived: toggleArchived,
    maximize: toggleMaximize,
    next_attention: () => jumpAttention(1),
    prev_attention: () => jumpAttention(-1),
    new_worktree: () => newWorktree(focusedRepo() ?? currentRepo()),
  };
  const f = go[id];
  if (f) return f() !== false;
  // Row keys act on the focused row or card.
  const t = focusedTarget();
  if (!t) return false;
  if (id === 'move_up' || id === 'move_down') return moveStep(t, id === 'move_up' ? -1 : 1, stepInFor(document.activeElement));
  if (id === 'row_menu') {
    const el = document.activeElement as HTMLElement;
    openMenu(menuFor(t), el);
    return true;
  }
  return runRowKey(id, t);
}

function focusedRepo(): string | undefined {
  const t = focusedTarget();
  if (!t) return undefined;
  if (t.kind === 'project') return t.id;
  if (t.kind === 'worktree') return projectOf(t.id);
  if (t.kind === 'agent') return projectOf(app.model.agents.find((a) => a.id === t.id)?.worktree_id ?? null);
  return projectOf(app.model.terminals.find((x) => x.id === t.id)?.worktree_id ?? null);
}

type Command = PaletteCommand & { run: () => unknown };

/** With the Diff view up on a worktree that has a base branch: compare with
 *  the other one (the header's toggle). No key of its own. */
export function diffAgainstCommand(
  cmd: (id: string, label: string, group: string, run: () => unknown, key?: ShortcutId, words?: string) => Command,
): Command[] {
  const shown = app.diffShown;
  if (nav.current.view !== 'diff' || app.diffRange || !shown?.base) return [];
  const toBase = shown.against === 'head';
  const label = toBase ? `Diff: compare with ${shown.base.branch}` : 'Diff: compare with HEAD';
  return [cmd('diff-against', label, 'Window', () => (app.diffAgainst = toBase ? 'base' : 'head'), undefined, 'merge-base base head against branch committed')];
}

/** Everything the palette offers besides jumping: the window's actions,
 *  then the shown session's own verbs. */
export function paletteCommands(): Command[] {
  const cmd = (id: string, label: string, group: string, run: () => unknown, key?: ShortcutId, words?: string): Command => ({
    id,
    label,
    group,
    run,
    keys: key ? keysOf(key) : undefined,
    words,
  });
  const out: Command[] = [
    cmd('new-session', 'New session', 'Window', () => focusQuickPrompt(), 'quick_prompt', 'quick prompt start agent'),
    cmd('new-worktree', 'New worktree\u2026', 'Window', () => newWorktree(currentRepo()), 'new_worktree', 'branch'),
    cmd('overview', 'Show the overview', 'Window', () => setHome('overview', nav.current.repo), 'home_overview'),
    cmd('board', 'Show the board', 'Window', () => setHome('board', nav.current.repo), 'home_board', 'kanban'),
    cmd('archived', app.showArchived ? 'Hide archived sessions' : 'Show archived sessions', 'Window', toggleArchived, 'toggle_archived'),
    cmd('next-attention', 'Next session needing you', 'Window', () => jumpAttention(1), 'next_attention', 'attention jump feedback'),
    cmd('prev-attention', 'Previous session needing you', 'Window', () => jumpAttention(-1), 'prev_attention', 'attention jump feedback'),
    cmd('diff', 'Diff the worktree', 'Window', () => openDiff(diffTarget()), 'open_diff'),
    ...diffAgainstCommand(cmd),
    ...(split() ? [cmd('maximize', app.paneMax ? 'Bring the pane back down' : 'Full pane', 'Window', toggleMaximize, 'maximize', 'maximize zoom full screen')] : []),
    cmd('theme', `Cycle theme (now ${app.themePref})`, 'Window', cycleTheme, 'cycle_theme', 'dark light system'),
    cmd('settings', 'Settings', 'Window', () => openSettings(), 'settings', 'preferences config'),
    cmd('presets', 'Agent presets', 'Window', () => openSettings('presets'), undefined, 'prefix postfix'),
    cmd('accounts', 'Accounts', 'Window', openAccounts, 'accounts', 'profiles login usage spread subscription'),
    cmd('add-account', 'Add an account\u2026', 'Window', () => openAddAccount(), undefined, 'profile login new subscription'),
    cmd('profiles', 'Profiles', 'Window', openAccounts, undefined, 'accounts login'),
    cmd('help', 'Keyboard shortcuts', 'Window', () => (ui.help = true), 'help_f1', 'help keys'),
    cmd('add-repo', 'Add a repo\u2026', 'Window', () => addRepo(), undefined, 'project folder'),
    ...orchestratorCommands(),
    // With a repo in view these come from its menu, below.
    ...(currentRepo()
      ? []
      : [
          cmd('refresh-prs', 'Refresh pull requests', 'Window', refreshPrs, undefined, 'pr github merged'),
          cmd('switch-branch', 'Switch branch\u2026', 'Window', () => pickBranchSwitch(), 'switch_branch', 'checkout git'),
        ]),
  ];
  const t = currentTarget();
  if (t) {
    const name =
      t.kind === 'agent' ? app.model.agents.find((a) => a.id === t.id)?.name : app.model.terminals.find((x) => x.id === t.id)?.name;
    let i = 0;
    for (const e of menuFor(t)) {
      if (typeof e !== 'object' || !('run' in e)) continue;
      const it = e as MenuItem;
      if (it.disabled) continue;
      out.push({ id: `session-${i++}`, label: it.label, group: `Session ${name ?? ''}`.trim(), keys: it.keys, run: it.run });
    }
  }
  const repo = currentRepo();
  if (repo) {
    const name = app.model.projects.find((p) => p.id === repo)?.name ?? 'repo';
    let i = 0;
    for (const e of menuFor({ kind: 'project', id: repo })) {
      if (typeof e !== 'object' || !('run' in e)) continue;
      const it = e as MenuItem;
      out.push({ id: `repo-${i++}`, label: it.label, group: `Repo ${name}`, keys: it.keys, run: it.run });
    }
  }
  return out;
}

async function addRepo() {
  const v = await ask.prompt({
    title: 'Add a repo',
    fields: [{ key: 'path', label: 'Folder', placeholder: 'C:\\path\\to\\repo', mono: true }],
    confirm: 'Add',
  });
  if (v?.path.trim()) await api.addProject(v.path).catch((e) => toast(String(e)));
}
