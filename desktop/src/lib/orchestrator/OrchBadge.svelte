<script lang="ts">
  // The orchestrator's role mark: a diamond and the word, as the terminal
  // app's card badge. Ink, never ember (ember means "needs you").
  let { short = false }: { short?: boolean } = $props();
</script>

<span
  class="inline-flex flex-none items-center gap-1 border border-rule-strong px-1 font-mono text-[11px] leading-4 text-ink"
  title="The orchestrator: the session that coordinates the others (0 or F10 opens it)"
  ><span aria-hidden="true">&#9670;</span>{#if !short}orchestrator{:else}<span class="sr-only">orchestrator</span>{/if}</span
>
