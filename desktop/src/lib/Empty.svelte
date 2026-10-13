<script lang="ts">
  import { app } from './store.svelte';
  import { api } from './api';

  const noProjects = $derived(app.model.projects.length === 0);
  const primary =
    'mt-2.5 inline-flex items-center gap-1.5 rounded-[7px] border-0 bg-focus px-3 py-[7px] font-semibold text-on-focus hover:brightness-110';
</script>

<div class="flex h-full flex-col items-center justify-center gap-2.5 p-6 text-center">
  <img src="/bro-256.png" alt="" width="88" height="88" draggable="false" class="mb-2 rounded-[20px] shadow-pop" />
  <h1 class="m-0 text-[22px] font-semibold tracking-tight text-fg-strong">Sweet as agent management.</h1>
  {#if !app.model.connected}
    <p class="m-0 max-w-[46ch] text-[13.5px] text-muted">
      {app.booted ? 'Not connected to the bro daemon.' : 'Finding the bro daemon...'}
    </p>
    {#if app.model.error}<p class="m-0 max-w-[70ch] font-mono text-[11.5px] text-red select-text">{app.model.error}</p>{/if}
    {#if app.booted}
      <button class={primary} onclick={() => api.retry()}>Start the daemon</button>
    {/if}
  {:else if noProjects}
    <p class="m-0 max-w-[46ch] text-[13.5px] text-muted">Add a git repo as a project, then start a session in it.</p>
    <p class="mt-1 text-xs text-muted">
      In the sidebar, <strong class="font-semibold text-fg">Add project</strong> takes a folder path.
      <code class="rounded bg-sunk px-[5px] py-px font-mono text-[11.5px] text-fg">bro add .</code> from a terminal works too.
    </p>
  {:else}
    <p class="m-0 max-w-[46ch] text-[13.5px] text-muted">
      No agent sessions yet. Every one you start shows up here, sorted by what it needs from you.
    </p>
    <button class={primary} onclick={() => (app.quickPrompt = true)}>
      New session <kbd>Ctrl</kbd><kbd>Shift</kbd><kbd>N</kbd>
    </button>
  {/if}
</div>
