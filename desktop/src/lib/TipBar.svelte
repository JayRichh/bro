<script lang="ts">
  // The FIRST-USE TIP: one line along the foot of the window the first time
  // a concept is on screen (learn.ts), with Got it and Don't show tips.
  // One at a time; each shows once, in either app.
  import { gotIt, learn, tipsOff } from './learn.svelte';
  import { tipText } from './learn';
  import { keyLabel } from './shortcuts';
  import { verbKeys } from './verbs';
  import type { ActionId } from './actions.gen';

  const text = $derived(
    learn.tip
      ? tipText(learn.tip, (id) => {
          const k = verbKeys(id as ActionId)[0];
          return k ? keyLabel(k) : null;
        })
      : '',
  );
  const btn =
    'h-6 flex-none border border-rule bg-transparent px-2 text-[12px] text-ink hover:border-rule-strong focus-visible:border-ink';
</script>

{#if learn.tip && text}
  <div class="flex min-h-9 flex-none items-center gap-3 border-t border-rule bg-bg px-3 py-1" role="status" aria-label="Tip">
    <span class="flex-none font-mono text-[11px] font-semibold tracking-wide text-ember-ink uppercase">Tip</span>
    <span class="min-w-0 flex-1 text-[13px] text-ink">{text}</span>
    <button class={btn} title="Hide this tip; it won't show again" onclick={gotIt}>Got it</button>
    <button class="{btn} text-muted" title="No more tips (Settings, General, Show tips turns them back on)" onclick={tipsOff}
      >Don't show tips</button
    >
  </div>
{/if}
