<script lang="ts">
  // The orchestrator's review requests, as notices that wait: they never
  // take the focus (you may be typing in a session), and stay until you
  // open or dismiss them.
  import { dismissNotice, noticeText, notices, openNotice } from '../orch.svelte';
</script>

{#if notices.list.length}
  <div class="fixed right-4 bottom-28 z-30 flex w-[min(460px,90vw)] flex-col gap-2" role="status" aria-live="polite">
    {#each notices.list.slice(-3) as n (n.id)}
      <div class="flex items-center gap-3 border border-rule-strong bg-bg py-1.5 pr-1.5 pl-3 text-[13px] text-ink">
        <span class="font-mono text-[12px] text-ink" aria-hidden="true">&#9670;</span>
        <span class="min-w-0 flex-1">{noticeText(n.sessions)}</span>
        <button class="h-7 flex-none border-0 bg-ink px-3 font-medium text-bg" onclick={() => openNotice(n.id)}>Open</button>
        <button
          class="grid size-7 flex-none place-items-center border-0 bg-transparent text-muted hover:text-ink"
         
          onclick={() => dismissNotice(n.id)}
          aria-label="Dismiss"
          title="Dismiss"
        >
          <svg width="10" height="10" viewBox="0 0 12 12" aria-hidden="true"><path d="M2 2l8 8M10 2l-8 8" stroke="currentColor" stroke-width="1.4" /></svg>
        </button>
      </div>
    {/each}
  </div>
{/if}
