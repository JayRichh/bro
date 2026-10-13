<script lang="ts">
  import { focusQuickPrompt, toast } from '../store.svelte';
  import { api } from '../api';
  import type { RepoRow } from './repos';
  import SessionTree from './SessionTree.svelte';

  let { row }: { row: RepoRow } = $props();

  const others = $derived(row.worktrees.length - 1);
  const label = 'font-mono text-[12px] text-muted whitespace-nowrap';
</script>

<section class="pt-6" data-repo={row.project.id} aria-labelledby="repo-{row.project.id}">
  <header class="flex min-w-0 items-baseline gap-4 border-b border-rule-strong pr-4 pb-2 pl-6">
    <h2 id="repo-{row.project.id}" class="m-0 min-w-0 truncate text-[15px] font-semibold text-ink" title={row.project.repo_path}>
      {row.name}
    </h2>
    <span class="flex-1"></span>
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
            class="{label} border-0 bg-transparent p-0 text-ink hover:underline hover:underline-offset-4"
            title="{p.pr.title} ({p.branch})"
            onclick={() => api.openUrl(p.pr.url).catch((e) => toast(String(e)))}
            >{p.pr.draft ? 'draft ' : ''}PR #{p.pr.number}</button
          >
        {/each}
      {:else}
        <span class={label}>no PR</span>
      {/if}
    </span>
  </header>
  {#if row.forest.length || row.terminals.length}
    <SessionTree repo={row.project.id} forest={row.forest} terminals={row.terminals} />
  {:else}
    <p class="m-0 flex items-center gap-3 border-b border-rule py-2 pl-6 text-[13px] text-muted">
      No sessions.
      <button
        class="border-0 bg-transparent p-0 text-ink underline underline-offset-4 hover:text-ember-ink"
        onclick={() => focusQuickPrompt({ project: row.project.id })}>Start one here</button
      >
    </p>
  {/if}
</section>
