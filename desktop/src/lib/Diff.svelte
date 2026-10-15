<script lang="ts">
  import { app, closeDiff, projectOf, saveDesktopPref, sessionWorktree } from './store.svelte';
  import { nav } from './shell/history.svelte';
  import { isDiffMode, type DiffMode } from './diff-api';
  import { groupWorktrees } from './diff-base';
  import DiffPane from './DiffPane.svelte';
  import { toggleMaximize } from './commands';
  import { withKey } from './shortcuts';
  import Icon from './ui/Icon.svelte';

  let pane: DiffPane | undefined = $state();

  // No worktree named and no session to take one from: the view asks
  // which, rather than diffing whichever checkout happens to be first.
  const worktree = $derived(app.diffWorktree ?? sessionWorktree(app.session) ?? null);
  const groups = $derived(groupWorktrees(app.model));
  const repo = $derived(projectOf(worktree) ?? '');
  // The mode is per repo, kept with the view's other state.
  const ui = $derived(nav.ui('diff', repo));
  const mode = $derived<DiffMode>(isDiffMode(ui.mode) ? ui.mode : app.diffMode);
  const active = $derived(nav.current.view === 'diff');

  function setMode(m: DiffMode) {
    ui.mode = m;
    app.diffMode = m;
    // The layout a repo with none saved yet opens in, from now on.
    saveDesktopPref('diff_mode', m);
  }

  function pickWorktree(id: string) {
    app.diffWorktree = id;
    app.diffRange = null;
    nav.replace({ view: 'diff', repo: projectOf(id), item: id });
  }

  function onKey(e: KeyboardEvent) {
    if (!active) return;
    const t = e.target as HTMLElement;
    // Esc leaves the diff even from inside the editor (the global key skips
    // typing targets), unless a Monaco widget (find, hover) wants it first.
    if (e.key === 'Escape' && !e.defaultPrevented && t.closest('.monaco-editor') && !t.closest('.find-widget, .monaco-hover, .suggest-widget')) {
      e.preventDefault();
      // A full pane comes back down first, as the window's own Esc does.
      if (app.paneMax) app.paneMax = false;
      else closeDiff();
      return;
    }
    if (t.closest('input, textarea, select, .monaco-editor, .xterm')) return;
    if (e.ctrlKey || e.altKey || e.metaKey || e.shiftKey) return;
    if (e.key === 's') setMode('side');
    else if (e.key === 'i') setMode('inline');
    else if (e.key === 'e') setMode('replacement');
    else if (e.key === 'r') pane?.refresh();
  }
</script>

<svelte:window onkeydown={onKey} />

<DiffPane
  bind:this={pane}
  {worktree}
  range={app.diffRange}
  bind:mode={() => mode, setMode}
  bind:against={app.diffAgainst}
  bind:shown={app.diffShown}
  keys={active}
>
  {#snippet lead()}
    <select
      class="h-7 max-w-[240px] min-w-[96px] shrink border border-rule bg-bg px-1.5 font-mono text-[12px] text-ink"
      aria-label="Worktree"
      title="The worktree to diff, by repo"
      value={worktree ?? ''}
      onchange={(e) => pickWorktree((e.target as HTMLSelectElement).value)}
    >
      {#if !worktree}<option value="" disabled>Pick a worktree</option>{/if}
      {#each groups as g (g.project)}
        <optgroup label={g.name}>
          {#each g.worktrees as w (w.id)}
            <option value={w.id}>{w.branch}{w.isMain ? ' (main checkout)' : ''}</option>
          {/each}
        </optgroup>
      {/each}
    </select>
  {/snippet}
  {#snippet pick()}
    <div class="max-w-[560px] px-6 pt-6 text-[13px]">
      <p class="m-0 text-[15px] font-semibold text-ink">Which worktree?</p>
      <p class="mt-1 mb-4 text-muted">Nothing in view names one. Pick the checkout to diff.</p>
      {#each groups as g (g.project)}
        <h2 class="m-0 mt-3 border-b border-rule pb-1 text-[12px] font-normal text-muted">{g.name}</h2>
        <ul class="m-0 list-none p-0">
          {#each g.worktrees as w (w.id)}
            <li>
              <button
                class="flex h-7 w-full items-center gap-2 border-0 border-b border-rule bg-transparent px-2 text-left font-mono text-[12px] text-ink hover:bg-surface"
                onclick={() => pickWorktree(w.id)}
                title="Diff {g.name} / {w.branch}"
                ><span class="min-w-0 truncate">{w.branch}</span>{#if w.isMain}<span class="flex-none text-faint">main checkout</span>{/if}</button
              >
            </li>
          {/each}
        </ul>
      {:else}
        <p class="m-0 text-muted">No worktrees yet. Add a repo first.</p>
      {/each}
    </div>
  {/snippet}
  {#snippet trail()}
    <button
      class="grid size-7 flex-none place-items-center border-0 bg-transparent hover:bg-surface hover:text-ink {app.paneMax ? 'text-ink' : 'text-muted'}"
      onclick={toggleMaximize}
      title={withKey(app.paneMax ? 'Bring the pane back down' : 'Full pane: give the diff the window', 'maximize')}
      aria-label={app.paneMax ? 'Bring the pane back down' : 'Full pane'}
      aria-pressed={app.paneMax}
    >
      <Icon name={app.paneMax ? 'restore' : 'maximize'} size={13} />
    </button>
    <button
      class="ml-1 grid size-7 flex-none place-items-center border border-rule bg-transparent text-ink hover:border-rule-strong"
      onclick={closeDiff}
      title="Close the diff (Esc)"
      aria-label="Close the diff"
    >
      <svg width="12" height="12" viewBox="0 0 12 12" aria-hidden="true"><path d="M2 2l8 8M10 2l-8 8" stroke="currentColor" stroke-width="1.4" /></svg>
    </button>
  {/snippet}
</DiffPane>
