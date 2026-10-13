<script lang="ts">
  import { TONE_LABEL, type Tone } from './status';

  let {
    tone,
    word = true,
    class: extra = '',
  }: { tone: Tone; word?: boolean; class?: string } = $props();

  // A 6px square plus a word: state never rests on colour alone. Filled for
  // a state that happened, hollow for one that is waiting or over.
  const SQUARE: Record<Tone, string> = {
    needs: 'bg-ember',
    failed: 'bg-fail',
    unread: 'bg-unread',
    running: 'bg-running',
    done: 'bg-done',
    merged: 'bg-merged',
    pr: 'border border-merged',
    idle: 'border border-muted',
    stopped: 'bg-faint',
    shell: 'border border-muted',
  };
  const WORD: Partial<Record<Tone, string>> = {
    needs: 'text-ember-ink font-medium',
    failed: 'text-fail font-medium',
  };
</script>

<span class="inline-flex flex-none items-center gap-1.5 {extra}" title={word ? undefined : TONE_LABEL[tone]}>
  <span class="size-1.5 flex-none {SQUARE[tone]}" aria-hidden={word}></span>
  {#if word}
    <span class="text-[12px] leading-none whitespace-nowrap {WORD[tone] ?? 'text-muted'}">{TONE_LABEL[tone]}</span>
  {:else}
    <span class="sr-only">{TONE_LABEL[tone]}</span>
  {/if}
</span>
