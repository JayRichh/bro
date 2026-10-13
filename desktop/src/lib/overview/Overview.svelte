<script lang="ts">
  import { tick } from 'svelte';
  import { app } from '../store.svelte';
  import Empty from '../Empty.svelte';
  import RepoRow from './RepoRow.svelte';
  import { repoRows } from './repos';

  let { active = true }: { active?: boolean } = $props();
  let scroller: HTMLDivElement;

  const rows = $derived(repoRows(app.model));

  // Only a repo picked in the rail scrolls the overview (`revealRepo`, which
  // nothing else sets). Session clicks, strip jumps, Esc and back/forward
  // change `nav.current.repo` too, and must leave the scroll alone.
  let revealed = app.reveal.seq;
  $effect(() => {
    const { repo, seq } = app.reveal;
    if (seq === revealed || !repo || !scroller) return;
    revealed = seq;
    tick().then(() => {
      const section = scroller.querySelector(`[data-repo="${CSS.escape(repo)}"]`);
      const selected = section?.querySelector('[data-row][aria-selected="true"]');
      if (selected) selected.scrollIntoView({ block: 'nearest' });
      else section?.scrollIntoView({ block: 'start' });
    });
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
