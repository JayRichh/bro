<script lang="ts">
  import { onMount } from 'svelte';
  import { app, boot, openDiff, openSession, sessionWorktree, type ThemePref, type View } from './lib/store.svelte';
  import { api } from './lib/api';
  import { getCurrentWindow } from '@tauri-apps/api/window';
  import { lanes } from './lib/status';
  import Sidebar from './lib/Sidebar.svelte';
  import Board from './lib/Board.svelte';
  import Session from './lib/Session.svelte';
  import Diff from './lib/Diff.svelte';
  import QuickPrompt from './lib/QuickPrompt.svelte';
  import StatusDot from './lib/StatusDot.svelte';

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

  const waiting = $derived(lanes(app.model).needs.length);
  let diffEverOpened = $state(false);
  $effect(() => {
    if (app.view === 'diff') diffEverOpened = true;
  });

  const TABS: { id: View; label: string; key: string }[] = [
    { id: 'board', label: 'Board', key: 'B' },
    { id: 'session', label: 'Session', key: 'T' },
    { id: 'diff', label: 'Diff', key: 'D' },
  ];

  function cycleTheme() {
    const order: ThemePref[] = ['system', 'light', 'dark'];
    app.themePref = order[(order.indexOf(app.themePref) + 1) % order.length];
  }

  function onKey(e: KeyboardEvent) {
    const t = e.target as HTMLElement;
    const typing = !!t.closest('input, textarea, select, .xterm, .monaco-editor');
    if (e.ctrlKey && e.shiftKey && !e.altKey) {
      const k = e.key.toUpperCase();
      const act: Record<string, () => void> = {
        B: () => (app.view = 'board'),
        T: () => (app.view = 'session'),
        D: () => openDiff(app.diffWorktree ?? sessionWorktree(app.session)),
        N: () => (app.quickPrompt = true),
        P: () => (app.quickPrompt = true),
        L: cycleTheme,
      };
      if (act[k]) {
        e.preventDefault();
        act[k]();
      }
      return;
    }
    if (typing || app.quickPrompt || e.ctrlKey || e.altKey || e.metaKey) return;
    if (e.key === '1') app.view = 'board';
    else if (e.key === '2') app.view = 'session';
    else if (e.key === '3') openDiff(app.diffWorktree ?? sessionWorktree(app.session));
    else if (e.key === 'n') {
      e.preventDefault();
      app.quickPrompt = true;
    }
  }

  /** Command-line `--view`, `--session`, `--theme`, `--diff`. */
  async function applyStartup() {
    const s = await api.startup().catch(() => null);
    if (!s) return;
    if (s.theme === 'light' || s.theme === 'dark' || s.theme === 'system') app.themePref = s.theme;
    if (s.diff_mode === 'inline' || s.diff_mode === 'side') app.diffMode = s.diff_mode;
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
    if (s.view === 'board' || s.view === 'session') app.view = s.view;
    else if (s.view === 'diff') openDiff(sessionWorktree(app.session));
  }

  onMount(() => {
    boot();
    applyStartup();
  });
</script>

<svelte:window onkeydown={onKey} />

<div class="grid h-full grid-cols-[252px_1fr]">
  <Sidebar />
  <main class="relative flex min-h-0 min-w-0 flex-col">
    <div class="flex h-11 flex-none items-center gap-2 border-b border-line-soft pr-3 pl-2.5">
      <div class="flex h-full gap-0.5" role="tablist">
        {#each TABS as tab (tab.id)}
          {@const on = app.view === tab.id}
          <button
            role="tab"
            aria-selected={on}
            class="relative inline-flex items-center gap-[7px] border-0 bg-transparent px-3 text-[12.5px] font-medium hover:text-fg-strong
              {on ? 'text-fg-strong after:absolute after:inset-x-2.5 after:-bottom-px after:h-0.5 after:rounded-sm after:bg-focus' : 'text-muted'}"
            onclick={() => (tab.id === 'diff' ? openDiff(app.diffWorktree ?? sessionWorktree(app.session)) : (app.view = tab.id))}
            title="{tab.label} (Ctrl+Shift+{tab.key})"
          >
            {tab.label}
            {#if tab.id === 'board' && waiting}
              <span class="inline-flex items-center gap-[3px] text-[11px] font-semibold text-st-needs" title="{waiting} waiting on you"
                ><StatusDot tone="needs" size={11} /><span class="tabular-nums">{waiting}</span></span
              >
            {/if}
          </button>
        {/each}
      </div>
      <span class="flex-1"></span>
      <button
        class="grid size-7 place-items-center rounded-md border-0 bg-transparent text-muted hover:bg-hover hover:text-fg-strong"
        onclick={cycleTheme}
        title="Theme: {app.themePref} (Ctrl+Shift+L)"
        aria-label="Theme: {app.themePref}"
      >
        {#if app.themePref === 'system'}
          <svg width="14" height="14" viewBox="0 0 16 16"><circle cx="8" cy="8" r="5.5" fill="none" stroke="currentColor" stroke-width="1.4" /><path d="M8 2.5a5.5 5.5 0 0 1 0 11z" fill="currentColor" /></svg>
        {:else if app.themePref === 'light'}
          <svg width="14" height="14" viewBox="0 0 16 16"><circle cx="8" cy="8" r="3" fill="none" stroke="currentColor" stroke-width="1.4" /><path d="M8 1.5v1.6M8 12.9v1.6M1.5 8h1.6M12.9 8h1.6M3.4 3.4l1.1 1.1M11.5 11.5l1.1 1.1M3.4 12.6l1.1-1.1M11.5 4.5l1.1-1.1" stroke="currentColor" stroke-width="1.4" stroke-linecap="round" /></svg>
        {:else}
          <svg width="14" height="14" viewBox="0 0 16 16"><path d="M13.5 9.6A5.8 5.8 0 0 1 6.4 2.5a5.8 5.8 0 1 0 7.1 7.1z" fill="none" stroke="currentColor" stroke-width="1.4" stroke-linejoin="round" /></svg>
        {/if}
      </button>
      <button
        class="inline-flex items-center gap-1.5 rounded-md border border-focus/70 bg-focus/15 py-1 pr-1.5 pl-2.5 text-xs font-semibold text-fg-strong enabled:hover:bg-focus/25 disabled:opacity-45"
        onclick={() => (app.quickPrompt = true)}
        disabled={!app.model.connected || !app.model.projects.length}
      >
        <svg width="12" height="12" viewBox="0 0 16 16" aria-hidden="true"><path d="M8 3v10M3 8h10" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" /></svg>
        New session
        <kbd class="ml-0.5">N</kbd>
      </button>
    </div>
    <div class="relative min-h-0 flex-1">
      <div class="absolute inset-0 {app.view === 'board' ? 'visible' : 'invisible'}"><Board /></div>
      <div class="absolute inset-0 {app.view === 'session' ? 'visible' : 'invisible'}"><Session /></div>
      {#if diffEverOpened}
        <div class="absolute inset-0 {app.view === 'diff' ? 'visible' : 'invisible'}"><Diff /></div>
      {/if}
    </div>
    {#if !app.model.connected && app.booted && app.model.projects.length}
      <div
        class="absolute bottom-3.5 left-1/2 flex max-w-[80%] -translate-x-1/2 items-center gap-2.5 rounded-lg border border-red bg-raised py-1.5 pr-2 pl-3 text-xs shadow-pop"
        role="status"
      >
        Lost the daemon: {app.model.error ?? 'reconnecting...'}
        <button class="rounded-[5px] border-0 bg-red px-[9px] py-[3px] font-semibold text-on-accent" onclick={() => api.retry()}>Start it</button>
      </div>
    {/if}
  </main>
</div>

{#if app.quickPrompt}<QuickPrompt />{/if}
{#if app.toast}
  <div
    class="fixed right-4 bottom-4 z-30 max-w-[460px] rounded-lg border border-t-2 border-line border-t-red bg-raised px-3 py-[9px] text-xs shadow-pop select-text"
    role="alert"
  >
    {app.toast}
  </div>
{/if}
