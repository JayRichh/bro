<script lang="ts">
  import { app, focusQuickPrompt, toast } from '../store.svelte';
  import { api } from '../api';
  import type { RepoRow } from './repos';
  import SessionTree from './SessionTree.svelte';
  import RunButton from './RunButton.svelte';
  import { projectMenu } from '../actions';
  import { openMenu } from '../ui/ui.svelte';
  import { prWord } from '../status';

  let { row }: { row: RepoRow } = $props();

  const others = $derived(row.worktrees.length - 1);
  // Run buttons: the main checkout always, a linked one while it has
  // sessions or a live run, so a repo with many worktrees stays readable.
  const runnable = $derived(
    row.worktrees.filter(
      (w) =>
        w.is_main ||
        app.model.agents.some((a) => a.worktree_id === w.id && !a.archived) ||
        app.model.terminals.some((t) => t.worktree_id === w.id && t.run_command && t.alive),
    ),
  );
  const label = 'font-mono text-[12px] text-muted whitespace-nowrap';
</script>

<section class="pt-6" data-repo={row.project.id} aria-labelledby="repo-{row.project.id}">
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <header
    class="flex min-w-0 items-baseline gap-4 border-b border-rule-strong pr-4 pb-2 pl-6"
    oncontextmenu={(e) => (e.preventDefault(), openMenu(projectMenu(row.project.id), { x: e.clientX, y: e.clientY }, row.name))}
  >
    <h2 id="repo-{row.project.id}" class="m-0 min-w-0 truncate text-[15px] font-semibold text-ink" title={row.project.repo_path}>
      {row.name}
    </h2>
    <span class="flex-1"></span>
    {#each runnable as w (w.id)}
      <RunButton worktree={w} label={w.is_main ? '' : w.branch} />
    {/each}
    <span class="flex min-w-0 items-baseline justify-end gap-4 overflow-hidden">
      {#if row.main}<span class="{label} truncate text-ink" title="Main worktree branch">{row.main.branch}</span>{/if}
      <span class={label}>{others > 0 ? `${others + 1} worktrees` : '1 worktree'}</span>
      {#each row.ports as port (port)}
        <button
          class="{label} border-0 bg-transparent p-0 hover:text-ink hover:underline hover:underline-offset-4"
          title="Open http://localhost:{port}"
          onclick={() => api.openUrl(`http://localhost:${port}`).catch((e) => toast(String(e)))}>:{port}</button
        >
      {/each}
      {#if row.prs.length}
        {#each row.prs as p (p.pr.number)}
          <button
            class="{label} border-0 bg-transparent p-0 hover:underline hover:underline-offset-4 {p.pr.state === 'merged'
              ? 'text-merged'
              : p.pr.state === 'closed'
                ? 'text-muted line-through'
                : 'text-ink'}"
            title="{p.pr.title} ({p.branch}, {prWord(p.pr)})"
            onclick={() => api.openUrl(p.pr.url).catch((e) => toast(String(e)))}
            >{prWord(p.pr).replace(/PR$/, '')}PR #{p.pr.number}</button
          >
        {/each}
      {:else}
        <span class={label}>no PR</span>
      {/if}
    </span>
    <button
      class="grid size-6 flex-none place-items-center self-center border border-transparent bg-transparent p-0 text-muted hover:border-rule hover:text-ink"
      title="Repo actions: new worktree, switch branch, open, refresh pull requests, rename, locate, remove (or right-click)"
      aria-label="{row.name} actions"
      aria-haspopup="menu"
      data-target="project:{row.project.id}"
      onclick={(e) => openMenu(projectMenu(row.project.id), e.currentTarget as HTMLElement, row.name)}
    >
      <svg width="12" height="12" viewBox="0 0 12 12" aria-hidden="true"><rect x="1" y="5" width="2" height="2" fill="currentColor" /><rect x="5" y="5" width="2" height="2" fill="currentColor" /><rect x="9" y="5" width="2" height="2" fill="currentColor" /></svg>
    </button>
  </header>
  {#if row.forest.length || row.terminals.length}
    <SessionTree repo={row.project.id} forest={row.forest} terminals={row.terminals} />
  {:else}
    <p class="m-0 flex items-center gap-3 border-b border-rule py-2 pl-6 text-[13px] text-muted">
      No sessions.
      <button
        class="border-0 bg-transparent p-0 text-ink underline underline-offset-4 hover:no-underline"
        onclick={() => focusQuickPrompt({ project: row.project.id })}>Start one here</button
      >
    </p>
  {/if}
</section>
