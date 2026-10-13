<script lang="ts">
  import { app, focusQuickPrompt } from './store.svelte';
  import { api } from './api';

  const noProjects = $derived(app.model.projects.length === 0);
  const primary =
    'mt-4 inline-flex items-center gap-2 border border-rule-strong bg-ink px-3 py-1.5 text-[14px] font-medium text-bg hover:bg-transparent hover:text-ink';
</script>

<div class="flex h-full flex-col justify-center gap-2 px-6 py-10">
  <h1 class="m-0 text-[28px] font-semibold tracking-tight text-ink">
    {#if !app.model.connected}No daemon yet.{:else if noProjects}Add a repo to start.{:else}No sessions yet.{/if}
  </h1>
  {#if !app.model.connected}
    <p class="m-0 max-w-[60ch] text-[14px] text-muted">
      {app.booted ? 'kete is not connected to its daemon.' : 'Looking for the kete daemon.'}
    </p>
    {#if app.model.error}<p class="m-0 max-w-[70ch] font-mono text-[12px] text-fail select-text">{app.model.error}</p>{/if}
    {#if app.booted}
      <div><button class={primary} onclick={() => api.retry()}>Start the daemon</button></div>
    {/if}
  {:else if noProjects}
    <p class="m-0 max-w-[60ch] text-[14px] text-muted">
      A repo holds your worktrees and the sessions running in them. Use <strong class="font-medium text-ink">Add a repo</strong> in the
      rail, or run <code class="border border-rule bg-surface px-1 font-mono text-[12px] text-ink">kete add .</code> in a terminal.
    </p>
  {:else}
    <p class="m-0 max-w-[60ch] text-[14px] text-muted">
      Every session you start shows up here under its repo, the ones waiting on you first.
    </p>
    <div>
      <button class={primary} onclick={() => focusQuickPrompt()}>New session <kbd>P</kbd></button>
    </div>
  {/if}
</div>
