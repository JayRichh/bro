<script lang="ts">
  // Why a Claude session's last turn stopped short, as one small chip: when
  // it resumes while it waits out a usage limit (still running, so never
  // ember), what it needs from you, or the API error it ended on. Nothing
  // at all while the session has no note. Informational only.
  import type { StopNote } from './api';
  import { app } from './store.svelte';
  import { stopLabel, stopTitle, stopTone, type StopTone } from './stop';

  let { note, class: extra = '' }: { note?: StopNote | null; class?: string } = $props();

  // Whole class names, so Tailwind sees them. The word is always there:
  // the colour never carries the state alone.
  const TONE: Record<StopTone, string> = {
    waiting: 'border-rule text-running',
    needs: 'border-ember text-ember-ink',
    error: 'border-fail text-fail',
  };
</script>

{#if note}
  <span
    class="inline-flex flex-none items-center border px-1 font-mono text-[11px] leading-4 whitespace-nowrap {TONE[stopTone(note)]} {extra}"
    title={stopTitle(note, app.now)}>{stopLabel(note, app.now)}</span
  >
{/if}
