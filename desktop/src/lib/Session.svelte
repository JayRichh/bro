<script lang="ts">
  import { app, openDiff, sessionWorktree } from './store.svelte';
  import { toneOf, terminalTone, harnessLabel, placeOf, TONE_LABEL, age, lastPrompt } from './status';
  import StatusDot from './StatusDot.svelte';
  import Terminal from './Terminal.svelte';

  const s = $derived(app.session);
  const agent = $derived(s && 'Agent' in s ? app.model.agents.find((a) => a.id === s.Agent) : undefined);
  const terminal = $derived(
    s && 'Terminal' in s ? app.model.terminals.find((t) => t.id === s.Terminal) : undefined,
  );
  const wt = $derived(sessionWorktree(s));
  const where = $derived(wt ? placeOf(app.model, wt) : null);
  const tone = $derived(agent ? toneOf(agent) : terminal ? terminalTone(terminal) : 'idle');
  const prompt = $derived(agent ? lastPrompt(agent) : null);
</script>

{#if s && (agent || terminal)}
  <div class="flex h-full min-h-0 flex-col">
    <div class="flex h-9 min-w-0 flex-none items-center gap-2 border-b border-line-soft pr-2.5 pl-3.5 text-xs whitespace-nowrap">
      <StatusDot {tone} />
      <span class="text-[13px] font-semibold text-fg-strong">{agent?.name ?? terminal?.name}</span>
      <span class="text-muted">{TONE_LABEL[tone]}</span>
      {#if agent}
        <span class="h-3.5 w-px bg-line"></span>
        <span class="font-medium">{harnessLabel(agent)}</span>
        {#if agent.profile}
          <span class="rounded-full border border-line-soft bg-sunk px-1.5 text-[10.5px] leading-4">{agent.profile}</span>
        {/if}
        <span class="font-mono text-[11px] text-muted tabular-nums">{age(agent.status_changed_at, app.now)}</span>
      {/if}
      {#if where}
        <span class="h-3.5 w-px bg-line"></span>
        <span class="font-mono text-[11.5px] text-muted" title={where.path}
          >{where.project}<span class="px-0.5 text-faint">/</span><span class="text-purple">{where.branch}</span></span
        >
      {/if}
      {#if prompt}
        <span class="min-w-0 truncate text-muted" title={prompt}>&ldquo;{prompt}&rdquo;</span>
      {/if}
      <span class="flex-1"></span>
      <button
        class="inline-flex items-center gap-1.5 rounded-md border border-line bg-raised px-[9px] py-[3px] text-xs text-fg hover:border-focus hover:text-fg-strong"
        onclick={() => openDiff(wt)}
        title="Diff this worktree (Ctrl+Shift+D)"
      >
        <svg width="13" height="13" viewBox="0 0 16 16" aria-hidden="true"><path d="M5 2v7M1.5 5.5h7M8 13.5h7" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" /></svg>
        Diff
      </button>
    </div>
    <div class="min-h-0 flex-1">
      <Terminal session={s} dark={app.dark} />
    </div>
  </div>
{:else}
  <div class="grid h-full place-items-center text-muted">
    <p>Pick a session on the board or in the sidebar.</p>
  </div>
{/if}
