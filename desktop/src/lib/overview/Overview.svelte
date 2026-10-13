<script lang="ts">
  import { tick } from 'svelte';
  import { app } from '../store.svelte';
  import { nav } from '../shell/history.svelte';
  import Empty from '../Empty.svelte';
  import RepoRow from './RepoRow.svelte';
  import { repoRows } from './repos';

  let { active = true }: { active?: boolean } = $props();
  let scroller: HTMLDivElement;

  const rows = $derived(repoRows(app.model));

  // Picking a repo in the rail brings its row into view.
  $effect(() => {
    const repo = nav.current.repo;
    if (!repo || !scroller) return;
    tick().then(() => scroller.querySelector(`[data-repo="${CSS.escape(repo)}"]`)?.scrollIntoView({ block: 'start' }));
  });

  /** j/k and the arrows walk the session rows; Enter opens (on the row). */
  function onKey(e: KeyboardEvent) {
    if (!active || e.ctrlKey || e.altKey || e.metaKey) return;
    const t = e.target as HTMLElement;
    if (t.closest('input, textarea, select, .xterm, .monaco-editor')) return;
    const down = e.key === 'ArrowDown' || e.key === 'j';
    const up = e.key === 'ArrowUp' || e.key === 'k';
    if (!down && !up) return;
    const all = [...scroller.querySelectorAll<HTMLElement>('[data-row]')];
    if (!all.length) return;
    e.preventDefault();
    const i = all.indexOf(document.activeElement as HTMLElement);
    const next = i < 0 ? all[0] : all[Math.max(0, Math.min(all.length - 1, i + (down ? 1 : -1)))];
    next.focus();
    next.scrollIntoView({ block: 'nearest' });
  }
</script>

<svelte:window onkeydown={onKey} />

<div class="@container h-full min-h-0 overflow-y-auto pb-8" bind:this={scroller}>
  {#if !rows.length}
    <Empty />
  {:else}
    {#each rows as row (row.project.id)}
      <RepoRow {row} />
    {/each}
  {/if}
</div>
