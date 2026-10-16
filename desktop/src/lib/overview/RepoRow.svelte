<script lang="ts">
  import { app, focusQuickPrompt, toast } from '../store.svelte';
  import { api } from '../api';
  import type { RepoRow } from './repos';
  import WorktreeBand from './WorktreeBand.svelte';
  import { repoKey, worktreeGroups } from '../worktrees/groups';
  import { isCollapsed, rows, toggleCollapsed } from '../worktrees/rows.svelte';
  import RunButton from './RunButton.svelte';
  import { newWorktree, openEditor, openTargetMenu } from '../actions';
  import { canSwitchBranch, openBranchSwitch } from '../worktrees/switcher.svelte';
  import { withKey } from '../shortcuts';
  import Icon from '../ui/Icon.svelte';
  import { prWord } from '../status';
  import { drag, dropProject, endDrag, leftFor, offsetIn, startDrag } from '../reorder.svelte';
  import { dropZone, type Place } from '../order';
  import { scopeModel } from '../scope';

  let { row }: { row: RepoRow } = $props();

  const others = $derived(row.worktrees.length - 1);
  // A group per checkout, as the terminal app's bands: the root first,
  // then the most recently worked in; empty ones with Show all worktrees.
  // Only what the scope admits.
  const groups = $derived(
    worktreeGroups(scopeModel(app.model), row.project.id, { archived: app.showArchived, showAll: app.launch.show_all_worktrees !== false }),
  );
  const cutting = $derived(rows.pending.filter((p) => p.project === row.project.id));
  const folded = $derived(isCollapsed(repoKey(row.project.id)));
  // Each group carries its own Run; the header keeps one for a checkout
  // with no group (the root, while it is empty), or any while folded.
  const runnable = $derived(
    row.worktrees.filter(
      (w) =>
        (folded || !groups.some((g) => g.worktree.id === w.id)) &&
        (w.is_main || app.model.terminals.some((t) => t.worktree_id === w.id && t.run_command && t.alive)),
    ),
  );
  const label = 'font-mono text-[12px] text-muted whitespace-nowrap';
  const verb =
    'grid size-6 flex-none place-items-center self-center border border-transparent bg-transparent p-0 text-muted hover:border-rule hover:text-ink';
  const main = $derived(row.worktrees.find((w) => w.is_main) ?? row.worktrees[0]);
  const switchable = $derived(canSwitchBranch(row.project.id));

  // The header drags to reorder the repos (the rail's order too); a repo
  // dropped on it lands above or below by the pointer's half.
  let mark = $state<Place | null>(null);
  function over(e: DragEvent) {
    const d = drag.item;
    if (d?.kind !== 'project' || d.id === row.project.id) return;
    e.preventDefault();
    if (e.dataTransfer) e.dataTransfer.dropEffect = 'move';
    const [y, h] = offsetIn(e);
    mark = dropZone(y, h, { insert: true, onto: false }) as Place;
  }
  function drop(e: DragEvent) {
    const d = drag.item;
    const at = mark;
    mark = null;
    if (d?.kind !== 'project' || !at) return;
    e.preventDefault();
    endDrag();
    void dropProject(d.id, row.project.id, at);
  }
</script>

<section class="pt-6" data-repo={row.project.id} aria-labelledby="repo-{row.project.id}">
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <!-- The header is the repo's row: with one of its verbs focused, the row
       keys (Shift+N, C, Shift+O, P) act on this repo. -->
  <header
    data-target="project:{row.project.id}"
    class="relative flex min-w-0 items-baseline gap-4 border-b border-rule-strong pr-4 pb-2 pl-1.5"
    draggable="true"
    ondragstart={(e) => startDrag(e, { kind: 'project', id: row.project.id })}
    ondragend={() => ((mark = null), endDrag())}
    ondragenter={over}
    ondragover={over}
    ondragleave={(e) => leftFor(e) && (mark = null)}
    ondrop={drop}
  >
    <button
      class="-mr-3.5 grid size-4 flex-none place-items-center self-center border-0 bg-transparent p-0 text-muted hover:text-ink"
      aria-expanded={!folded}
      aria-label="{folded ? 'Unfold' : 'Fold'} {row.name}"
      title="{folded ? 'Unfold' : 'Fold'} {row.name}'s worktrees"
      onclick={() => toggleCollapsed(repoKey(row.project.id))}
    >
      <svg width="10" height="10" viewBox="0 0 10 10" aria-hidden="true"
        ><path d={folded ? 'M3.5 1.5L7 5 3.5 8.5' : 'M1.5 3.5L5 7l3.5-3.5'} fill="none" stroke="currentColor" stroke-width="1.4" /></svg
      >
    </button>
    {#if mark && drag.item}
      <span class="pointer-events-none absolute inset-x-0 z-10 h-px bg-ember {mark === 'before' ? '-top-px' : '-bottom-px'}" aria-hidden="true"></span>
    {/if}
    <h2
      id="repo-{row.project.id}"
      class="m-0 min-w-0 cursor-grab truncate text-[15px] font-semibold text-ink"
      title="{row.project.repo_path}. {withKey('Drag to reorder the repos', ['move_up', 'move_down'])} on its header"
    >
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
          data-target="port:{row.portOf[port] ?? ''}:{port}"
          title="Open http://localhost:{port}; right-click for its menu"
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
            data-target="pr:{p.worktree}"
            title="{p.pr.title} ({p.branch}, {prWord(p.pr)}). Open it; right-click for its menu."
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
      title="{withKey('Repo actions: open remote, refresh pull requests, rename, locate, remove, worktrees', 'row_menu')}; or right-click"
      aria-label="{row.name} actions"
      aria-haspopup="menu"
      onclick={(e) => openTargetMenu({ kind: 'project', id: row.project.id }, e.currentTarget as HTMLElement)}
    >
      <svg width="12" height="12" viewBox="0 0 12 12" aria-hidden="true"><rect x="1" y="5" width="2" height="2" fill="currentColor" /><rect x="5" y="5" width="2" height="2" fill="currentColor" /><rect x="9" y="5" width="2" height="2" fill="currentColor" /></svg>
    </button>
  </header>
  {#if folded}
    <!-- Folded: the header says enough. -->
  {:else if groups.length || cutting.length}
    {#each groups as g (g.worktree.id)}
      <WorktreeBand repo={row.project.id} group={g} />
    {/each}
    {#each cutting as p (p.key)}
      <div class="flex min-h-8 items-center gap-2 border-b border-rule pr-4 pl-6 text-muted" role="status">
        <span class="size-2.5 flex-none" aria-hidden="true"></span>
        <span class="w-3 flex-none text-center font-mono text-[13px]" aria-hidden="true">&#8627;</span>
        <span class="relative min-w-0 truncate font-mono text-[13px]"
          >{p.branch}<span class="absolute inset-x-0 -bottom-0.5 h-px overflow-hidden" aria-hidden="true"
            ><span class="block h-px w-1/3 animate-[kete-run_1.8s_linear_infinite] bg-running"></span></span
          ></span
        >
        <span class="font-mono text-[12px] whitespace-nowrap">git is making the worktree&hellip;</span>
      </div>
    {/each}
  {:else}
    <p class="m-0 flex items-center gap-3 border-b border-rule py-2 pl-6 text-[13px] text-muted">
      No sessions in this repo yet.
      <button
        class="border-0 bg-transparent p-0 text-ink underline underline-offset-4 hover:no-underline"
        title={withKey('New session on this repo: the quick prompt', ['quick_prompt', 'quick_prompt_alt'])}
        onclick={() => focusQuickPrompt({ project: row.project.id })}>Start one here</button
      >
    </p>
  {/if}
</section>
