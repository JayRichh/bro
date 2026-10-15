<script lang="ts">
  import { app, focusQuickPrompt, toast } from '../store.svelte';
  import { api } from '../api';
  import type { RepoRow } from './repos';
  import SessionTree from './SessionTree.svelte';
  import RunButton from './RunButton.svelte';
  import { newWorktree, openEditor, projectMenu } from '../actions';
  import { canSwitchBranch, openBranchSwitch } from '../worktrees/switcher.svelte';
  import { withKey } from '../shortcuts';
  import Icon from '../ui/Icon.svelte';
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
  const verb =
    'grid size-6 flex-none place-items-center self-center border border-transparent bg-transparent p-0 text-muted hover:border-rule hover:text-ink';
  const main = $derived(row.worktrees.find((w) => w.is_main) ?? row.worktrees[0]);
  const switchable = $derived(canSwitchBranch(row.project.id));
</script>

<section class="pt-6" data-repo={row.project.id} aria-labelledby="repo-{row.project.id}">
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <!-- The header is the repo's row: with one of its verbs focused, the row
       keys (Shift+N, C, Shift+O, P) act on this repo. -->
  <header
    data-target="project:{row.project.id}"
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
    <span class="flex flex-none items-center gap-0.5 self-center" role="toolbar" aria-label="{row.name}">
      <button class={verb} title={withKey(`New session in ${row.name}`, 'quick_prompt')} aria-label="New session in {row.name}" onclick={() => focusQuickPrompt({ project: row.project.id })}
        ><Icon name="plus" size={12} /></button
      >
      <button class="{verb} @max-[640px]:hidden" title={withKey('New worktree', 'new_worktree')} aria-label="New worktree in {row.name}" onclick={() => newWorktree(row.project.id)}
        ><Icon name="branch" size={12} /></button
      >
      {#if switchable}
        <button
          class="{verb} @max-[640px]:hidden"
          title={withKey(`Switch the root checkout's branch${row.main ? ` (on ${row.main.branch})` : ''}`, 'switch_branch')}
          aria-label="Switch branch in {row.name}"
          onclick={() => openBranchSwitch(row.project.id)}><Icon name="switch" size={12} /></button
        >
      {/if}
      <button class="{verb} @max-[640px]:hidden" title={withKey('Open in the editor', 'open_editor')} aria-label="Open {row.name} in the editor" onclick={() => openEditor(main?.id)}
        ><Icon name="editor" size={12} /></button
      >
    </span>
    <button
      class={verb}
      title="Repo actions: open remote, refresh pull requests, rename, locate, remove, worktrees (Shift+F10, or right-click)"
      aria-label="{row.name} actions"
      aria-haspopup="menu"
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
