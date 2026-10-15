<script lang="ts">
  // The home views with nothing in them: what goes here and the one thing
  // to do, as a button with its key (the copy is nebula-config's learn.rs,
  // shared with the terminal app).
  import { app, focusQuickPrompt } from './store.svelte';
  import { api } from './api';
  import { addRepo } from './commands';
  import { emptyCopy } from './learn';
  import { keyLabel, withKey } from './shortcuts';
  import { verbKeys } from './verbs';

  const noProjects = $derived(app.model.projects.length === 0);
  const noRepos = emptyCopy('no_repos');
  const start = $derived(emptyCopy(app.home === 'board' ? 'board' : 'overview'));
  const startKey = $derived(verbKeys('quick_prompt')[0]);
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
      {noRepos.text} Or run <code class="border border-rule bg-surface px-1 font-mono text-[12px] text-ink">kete add .</code> in a terminal.
    </p>
    <div>
      <button class={primary} title="Pick a folder to add as a repo (also in the rail and the palette)" onclick={() => addRepo()}>Add a repo</button>
    </div>
  {:else}
    <p class="m-0 max-w-[60ch] text-[14px] text-muted">{start.text}</p>
    <div>
      <button class={primary} title={withKey('New session: the quick prompt', ['quick_prompt', 'quick_prompt_alt'])} onclick={() => focusQuickPrompt()}
        >New session{#if startKey}<kbd class="font-mono text-[12px]">{keyLabel(startKey)}</kbd>{/if}</button
      >
    </div>
  {/if}
</div>
