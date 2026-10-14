<script lang="ts">
  import { app, closeDiff, projectOf, saveDesktopPref, sessionWorktree } from './store.svelte';
  import { nav } from './shell/history.svelte';
  import { isDiffMode, type DiffMode } from './diff-api';
  import { placeOf } from './status';
  import DiffPane from './DiffPane.svelte';

  let pane: DiffPane | undefined = $state();

  const worktree = $derived(
    app.diffWorktree ?? sessionWorktree(app.session) ?? app.model.worktrees[0]?.id ?? null,
  );
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
      closeDiff();
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

<DiffPane bind:this={pane} {worktree} range={app.diffRange} bind:mode={() => mode, setMode} keys={active}>
  {#snippet lead()}
    <select
      class="h-7 max-w-[240px] min-w-[132px] border border-rule bg-bg px-1.5 font-mono text-[12px] text-ink"
      aria-label="Worktree"
      value={worktree ?? ''}
      onchange={(e) => pickWorktree((e.target as HTMLSelectElement).value)}
    >
      {#each app.model.worktrees as w (w.id)}
        <option value={w.id}>{placeOf(app.model, w.id).project} / {w.branch}</option>
      {/each}
    </select>
  {/snippet}
  {#snippet trail()}
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
