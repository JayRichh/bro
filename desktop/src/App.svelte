<script lang="ts">
  import { onMount } from 'svelte';
  import {
    app,
    boot,
    focusQuickPrompt,
    follow,
    goHome,
    openDiff,
    openSession,
    sessionWorktree,
    type ThemePref,
  } from './lib/store.svelte';
  import { api } from './lib/api';
  import { getCurrentWindow } from '@tauri-apps/api/window';
  import { nav } from './lib/shell/history.svelte';
  import Rail from './lib/shell/Rail.svelte';
  import Breadcrumb from './lib/shell/Breadcrumb.svelte';
  import Board from './lib/Board.svelte';
  import Session from './lib/Session.svelte';
  import Diff from './lib/Diff.svelte';
  import QuickPrompt from './lib/QuickPrompt.svelte';

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
  const split = $derived(view === 'session' || view === 'diff');
  const adoView = $derived(view === 'tasks' || view === 'branches' || view === 'prs');
  const ADO_TITLE: Record<string, string> = { tasks: 'Tasks', branches: 'Branches', prs: 'Pull requests' };

  // Session and Diff stay mounted once opened, so a terminal keeps its
  // scrollback and the diff editor its place while you look elsewhere.
  let sessionEverOpened = $state(false);
  let diffEverOpened = $state(false);
  $effect(() => {
    if (view === 'session') sessionEverOpened = true;
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

  function cycleTheme() {
    const order: ThemePref[] = ['system', 'light', 'dark'];
    app.themePref = order[(order.indexOf(app.themePref) + 1) % order.length];
  }

  function toggleHome() {
    app.home = app.home === 'overview' ? 'board' : 'overview';
    goHome(nav.current.repo);
  }

  function onKey(e: KeyboardEvent) {
    const t = e.target as HTMLElement;
    const inTerm = !!t.closest('.xterm');
    const typing = inTerm || !!t.closest('input, textarea, select, .monaco-editor');
    // History: global, except inside a terminal, where Alt+arrows belong to the shell.
    if (e.altKey && !e.ctrlKey && !e.shiftKey && !e.metaKey && !inTerm) {
      if (e.key === 'ArrowLeft') return (e.preventDefault(), nav.back());
      if (e.key === 'ArrowRight') return (e.preventDefault(), nav.forward());
    }
    if (e.ctrlKey && e.shiftKey && !e.altKey) {
      const k = e.key.toUpperCase();
      const act: Record<string, () => void> = {
        B: () => ((app.home = 'board'), goHome(nav.current.repo)),
        O: () => ((app.home = 'overview'), goHome(nav.current.repo)),
        T: () => app.session && openSession(app.session),
        D: () => openDiff(app.diffWorktree ?? sessionWorktree(app.session)),
        N: () => focusQuickPrompt(),
        P: () => focusQuickPrompt(),
        L: cycleTheme,
      };
      if (act[k]) {
        e.preventDefault();
        act[k]();
      }
      return;
    }
    if (typing || e.ctrlKey || e.altKey || e.metaKey) return;
    if (e.key === 'K' && e.shiftKey) return (e.preventDefault(), toggleHome());
    if (e.key === 'p' || e.key === 'n') return (e.preventDefault(), focusQuickPrompt());
    if (e.key === 'Escape' && split) return goHome(nav.current.repo);
    if (e.key === '1') goHome(nav.current.repo);
    else if (e.key === '2' && app.session) openSession(app.session);
    else if (e.key === '3') openDiff(app.diffWorktree ?? sessionWorktree(app.session));
  }

  /** The mouse's back and forward buttons. */
  function onMouseUp(e: MouseEvent) {
    if (e.button === 3) (e.preventDefault(), nav.back());
    else if (e.button === 4) (e.preventDefault(), nav.forward());
  }

  /** Command-line `--view`, `--session`, `--theme`, `--diff`. */
  async function applyStartup() {
    const s = await api.startup().catch(() => null);
    if (!s) return;
    if (s.theme === 'light' || s.theme === 'dark' || s.theme === 'system') app.themePref = s.theme;
    if (s.diff_mode === 'inline' || s.diff_mode === 'side') app.diffMode = s.diff_mode;
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
      <div class="flex h-6 flex-none border border-rule" role="radiogroup" aria-label="Home view">
        {#each [{ id: 'overview', label: 'Overview' }, { id: 'board', label: 'Board' }] as h, i (h.id)}
          {@const on = app.home === h.id}
          <button
            role="radio"
            aria-checked={on}
            class="border-0 px-2 text-[12px] {i ? 'border-l border-rule' : ''} {on ? 'bg-ink text-bg' : 'bg-transparent text-muted hover:text-ink'}"
            title="{h.label} (Shift+K switches)"
            onclick={() => {
              app.home = h.id as 'overview' | 'board';
              goHome(nav.current.repo);
            }}>{h.label}</button
          >
        {/each}
      </div>
    </header>

    <div class="relative flex min-h-0 flex-1" bind:this={body}>
      {#if adoView}
        <section class="min-w-0 flex-1 px-6 pt-6">
          <h1 class="m-0 text-[28px] font-semibold tracking-tight text-ink">{ADO_TITLE[view]}</h1>
          <p class="mt-2 max-w-[60ch] text-[14px] text-muted">
            This view is not in this build yet. The repo's Azure DevOps remote is set up, so it will appear here once it ships.
          </p>
        </section>
      {:else}
        <div class="relative min-w-0 flex-1">
          <Board active={!split} />
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
          style="width: {app.splitWidth ? `${app.splitWidth}px` : '56%'}"
        >
          {#if sessionEverOpened}
            <div class="absolute inset-0 {view === 'session' ? 'visible' : 'invisible'}"><Session /></div>
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

{#if app.toast}
  <div
    class="fixed right-4 bottom-14 z-30 max-w-[460px] border border-rule-strong bg-bg px-3 py-2 text-[13px] text-ink select-text"
    role="alert"
  >
    {app.toast}
  </div>
{/if}
