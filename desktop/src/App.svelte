<script lang="ts">
  import { onMount } from 'svelte';
  import { app, boot, checkAdoStore, follow, loadDesktopPrefs, openDiff, openSession, sessionWorktree, setHome } from './lib/store.svelte';
  import { shortcutFor } from './lib/shortcuts';
  import { focusedTarget, runShortcut, toggleArchived } from './lib/commands';
  import { layerOpen } from './lib/ui/ui.svelte';
  import Menu from './lib/ui/Menu.svelte';
  import Dialog from './lib/ui/Dialog.svelte';
  import Palette from './lib/Palette.svelte';
  import Help from './lib/Help.svelte';
  import { api } from './lib/api';
  import { isDiffMode } from './lib/diff-api';
  import { getCurrentWindow } from '@tauri-apps/api/window';
  import { nav } from './lib/shell/history.svelte';
  import Rail from './lib/shell/Rail.svelte';
  import Breadcrumb from './lib/shell/Breadcrumb.svelte';
  import AttentionStrip from './lib/overview/AttentionStrip.svelte';
  import Overview from './lib/overview/Overview.svelte';
  import Board from './lib/Board.svelte';
  import Session from './lib/Session.svelte';
  import Diff from './lib/Diff.svelte';
  import QuickPrompt from './lib/QuickPrompt.svelte';
  import Tasks from './lib/ado/Tasks.svelte';
  import Branches from './lib/ado/Branches.svelte';
  import Prs from './lib/ado/Prs.svelte';
  import Settings from './lib/settings/Settings.svelte';
  import OrchestratorPanel from './lib/orchestrator/OrchestratorPanel.svelte';
  import StartOrchestrator from './lib/orchestrator/StartOrchestrator.svelte';
  import ReviewSheet from './lib/orchestrator/ReviewSheet.svelte';
  import ReviewNotices from './lib/orchestrator/ReviewNotices.svelte';
  import { onReviewOpened, openOrchestrator } from './lib/orch.svelte';
  import { orchestratorOf } from './lib/orchestrator';

  const media = window.matchMedia('(prefers-color-scheme: dark)');
  let osDark = $state(media.matches);
  media.addEventListener('change', (e) => (osDark = e.matches));

  $effect(() => {
    app.dark = app.themePref === 'system' ? osDark : app.themePref === 'dark';
    document.documentElement.dataset.theme = app.dark ? 'dark' : 'light';
  });
  // The native title bar follows an override too, not only the OS.
  $effect(() => {
    const pref = app.themePref;
    getCurrentWindow()
      .setTheme(pref === 'system' ? null : pref)
      .catch(() => {});
  });

  // Back and forward land on a place; the selection follows it.
  $effect(() => follow(nav.current));

  const view = $derived(nav.current.view);
  const orch = $derived(orchestratorOf(app.model));
  // The orchestrator view is a split too: its tree on the left, its own
  // session in the shared pane on the right (one pane per session, so the
  // terminal is never attached twice).
  const orchView = $derived(view === 'orchestrator');
  const split = $derived(view === 'session' || view === 'diff' || (orchView && !!orch));
  // The orchestrator view keeps the chat primary: a wider right side.
  const rightWidth = $derived(app.splitWidth ? `${app.splitWidth}px` : orchView ? '62%' : '56%');
  // An orchestrator started (or pinned) while its view is up: show it.
  $effect(() => {
    if (orchView && orch && !(app.session && 'Agent' in app.session && app.session.Agent === orch.id)) app.session = { Agent: orch.id };
  });
  const adoView = $derived(view === 'tasks' || view === 'branches' || view === 'prs');
  const ADO_TITLE: Record<string, string> = { tasks: 'Tasks', branches: 'Branches', prs: 'Pull requests' };

  // Each ADO view, as it opens for a repo, asks whether ado.json failed to
  // save. Keyed on view and repo, so picking a row inside does not ask.
  const adoOpen = $derived(adoView ? `${view}\u0000${nav.current.repo ?? ''}` : '');
  $effect(() => {
    if (adoOpen) checkAdoStore();
  });

  // Session and Diff stay mounted once opened, so a terminal keeps its
  // scrollback and the diff editor its place while you look elsewhere.
  let sessionEverOpened = $state(false);
  let diffEverOpened = $state(false);
  $effect(() => {
    if (view === 'session' || (orchView && orch)) sessionEverOpened = true;
    if (view === 'diff') diffEverOpened = true;
  });

  let body: HTMLDivElement;
  function startResize(e: PointerEvent) {
    const handle = e.currentTarget as HTMLElement;
    handle.setPointerCapture(e.pointerId);
    const box = body.getBoundingClientRect();
    const move = (ev: PointerEvent) => {
      app.splitWidth = Math.round(Math.max(380, Math.min(box.width - 360, box.right - ev.clientX)));
    };
    const up = () => {
      handle.removeEventListener('pointermove', move);
      handle.removeEventListener('pointerup', up);
    };
    handle.addEventListener('pointermove', move);
    handle.addEventListener('pointerup', up);
  }
  function resizeKey(e: KeyboardEvent) {
    const w = app.splitWidth || Math.round(body.getBoundingClientRect().width * 0.56);
    if (e.key === 'ArrowLeft') app.splitWidth = w + 40;
    else if (e.key === 'ArrowRight') app.splitWidth = Math.max(380, w - 40);
  }

  // Every key goes through the one shortcut table (lib/shortcuts.ts),
  // which the help overlay lists too.
  function onKey(e: KeyboardEvent) {
    if (layerOpen()) return;
    const t = e.target instanceof Element ? e.target : document.body;
    const inTerminal = !!t.closest('.xterm');
    const typing = inTerminal || !!t.closest('input, textarea, select, .monaco-editor');
    const s = shortcutFor(e, { inTerminal, typing, onRow: !!focusedTarget() });
    if (s && runShortcut(s.id)) e.preventDefault();
  }

  /** The mouse's back and forward buttons. */
  function onMouseUp(e: MouseEvent) {
    if (e.button === 3) (e.preventDefault(), nav.back());
    else if (e.button === 4) (e.preventDefault(), nav.forward());
  }

  /** Command-line `--view`, `--session`, `--theme`, `--diff`. */
  async function applyStartup() {
    // Saved preferences first; the command line overrides them for this
    // run only and is never written back.
    const prefs = await loadDesktopPrefs();
    const s = await api.startup().catch(() => null);
    // Open on the orchestrator (Desktop tab) unless the command line picks
    // a view or session for this run.
    if ((prefs?.startup === 'orchestrator' && !s?.view && !s?.session) || s?.view === 'orchestrator') {
      for (let i = 0; i < 100 && !app.model.connected; i++) await new Promise((r) => setTimeout(r, 100));
      openOrchestrator();
    }
    if (!s) return;
    if (s.theme === 'light' || s.theme === 'dark' || s.theme === 'system') app.themePref = s.theme;
    if (isDiffMode(s.diff_mode)) app.diffMode = s.diff_mode;
    if (s.view === 'board') {
      app.home = 'board';
      nav.replace({ view: 'board' });
    }
    if (s.session || s.view === 'session' || s.view === 'diff') {
      // Wait for the daemon's snapshot before resolving names.
      for (let i = 0; i < 100 && !app.model.connected; i++) await new Promise((r) => setTimeout(r, 100));
      await new Promise((r) => setTimeout(r, 300));
      const m = app.model;
      const want = s.session?.toLowerCase();
      const agent = want ? m.agents.find((a) => a.id.toLowerCase() === want || a.name.toLowerCase() === want) : undefined;
      const term = want && !agent ? m.terminals.find((t) => t.id.toLowerCase() === want || t.name.toLowerCase() === want) : undefined;
      if (agent) openSession({ Agent: agent.id });
      else if (term) openSession({ Terminal: term.id });
    }
    if (s.view === 'diff') openDiff(sessionWorktree(app.session));
  }

  onMount(() => {
    api.onReview(onReviewOpened).catch(() => {});
    boot();
    applyStartup();
  });
</script>

<svelte:window onkeydown={onKey} onmouseup={onMouseUp} />

<div class="grid h-full grid-cols-[220px_minmax(0,1fr)]">
  <Rail />
  <main class="relative flex min-h-0 min-w-0 flex-col">
    <header class="flex h-10 flex-none items-center gap-6 border-b border-rule pr-4 pl-2">
      <Breadcrumb />
      <span class="flex-1"></span>
      <AttentionStrip />
      {#if app.home === 'overview' && !adoView && view !== 'settings' && !orchView}
        <button
          class="h-6 flex-none border px-2 text-[12px] {app.showArchived ? 'border-ink bg-ink text-bg' : 'border-rule bg-transparent text-muted hover:text-ink'}"
          aria-pressed={app.showArchived}
          title="Show archived sessions in the overview (Shift+A)"
          onclick={toggleArchived}>Archived</button
        >
      {/if}
      <div class="flex h-6 flex-none border border-rule" role="radiogroup" aria-label="Home view">
        {#each [{ id: 'overview', label: 'Overview' }, { id: 'board', label: 'Board' }] as h, i (h.id)}
          {@const on = app.home === h.id}
          <button
            role="radio"
            aria-checked={on}
            class="border-0 px-2 text-[12px] {i ? 'border-l border-rule' : ''} {on ? 'bg-ink text-bg' : 'bg-transparent text-muted hover:text-ink'}"
            title="{h.label} (Shift+K switches)"
            onclick={() => setHome(h.id as 'overview' | 'board', nav.current.repo)}>{h.label}</button
          >
        {/each}
      </div>
    </header>

    <div class="relative flex min-h-0 flex-1" bind:this={body}>
      {#if view === 'tasks' && nav.current.repo}
        {#key nav.current.repo}<Tasks repo={nav.current.repo} />{/key}
      {:else if view === 'branches' && nav.current.repo}
        {#key nav.current.repo}<Branches repo={nav.current.repo} />{/key}
      {:else if view === 'prs'}
        <Prs />
      {:else if view === 'settings'}
        <Settings />
      {:else if orchView && !orch}
        <StartOrchestrator inline />
      {:else if adoView}
        <section class="min-w-0 flex-1 px-6 pt-6">
          <h1 class="m-0 text-[28px] font-semibold tracking-tight text-ink">{ADO_TITLE[view]}</h1>
          <p class="mt-2 max-w-[60ch] text-[14px] text-muted">
            This view is not in this build yet. The repo's Azure DevOps remote is set up, so it will appear here once it ships.
          </p>
        </section>
      {:else}
        <div class="relative min-w-0 flex-1">
          {#if orchView}
            <OrchestratorPanel />
          {:else if app.home === 'board'}
            <Board active={!split} />
          {:else}
            <Overview active={!split} />
          {/if}
        </div>
        {#if split}
          <!-- svelte-ignore a11y_no_noninteractive_tabindex, a11y_no_noninteractive_element_interactions -->
          <div class="relative z-10 w-px flex-none bg-rule-strong">
            <div
              class="absolute inset-y-0 -left-[3px] w-[7px] cursor-col-resize hover:bg-rule focus-visible:bg-rule"
              role="separator"
              aria-orientation="vertical"
              aria-label="Resize the split (arrow keys)"
              tabindex="0"
              onpointerdown={startResize}
              onkeydown={resizeKey}
            ></div>
          </div>
        {/if}
        <div
          class="relative min-w-0 flex-none {split ? '' : 'hidden'}"
          style="width: {rightWidth}"
        >
          {#if sessionEverOpened}
            <div class="absolute inset-0 {view === 'session' || orchView ? 'visible' : 'invisible'}"><Session /></div>
          {/if}
          {#if diffEverOpened}
            <div class="absolute inset-0 {view === 'diff' ? 'visible' : 'invisible'}"><Diff /></div>
          {/if}
        </div>
      {/if}
    </div>

    <QuickPrompt />

    {#if !app.model.connected && app.booted && app.model.projects.length}
      <div
        class="absolute bottom-14 left-1/2 flex max-w-[80%] -translate-x-1/2 items-center gap-3 border border-fail bg-bg py-1.5 pr-1.5 pl-3 text-[13px] text-ink"
        role="status"
      >
        Lost the daemon: {app.model.error ?? 'reconnecting'}
        <button class="border-0 bg-fail px-2.5 py-1 font-medium text-bg" onclick={() => api.retry()}>Start it</button>
      </div>
    {/if}
  </main>
</div>

<Menu />
<Dialog />
<Palette />
<Help />
<StartOrchestrator />
<ReviewSheet />
<ReviewNotices />

{#if app.toast}
  <div
    class="fixed right-4 bottom-14 z-30 max-w-[460px] border border-rule-strong bg-bg px-3 py-2 text-[13px] text-ink select-text"
    role="alert"
  >
    {app.toast}
  </div>
{/if}
