<script lang="ts">
  // A repo's checkouts as one table: every worktree, empty or not, with
  // its git state, sessions, pull request, run and last activity. Sorts
  // by any column (the root stays on top), and sweeps the merged and clean
  // ones in one confirm. Each row has the overview group's verbs.
  import { onMount, untrack } from 'svelte';
  import { app, toast } from '../store.svelte';
  import { api } from '../api';
  import { nav } from '../shell/history.svelte';
  import { openMenu } from '../ui/ui.svelte';
  import { ownMenu } from '../menus/router';
  import { age } from '../status';
  import { newWorktree } from '../actions';
  import RunButton from '../overview/RunButton.svelte';
  import { firstDir, gitSignal, groupOf, prLabel, sortRows, tableRow, type SortDir, type SortKey } from './groups';
  import { refreshGit, rows, watchGit } from './rows.svelte';
  import { sweepWorktrees, verbTitle, verbsFor, worktreeMenu } from './verbs';
  import VerbBar from '../VerbBar.svelte';

  let { repo }: { repo: string } = $props();

  const project = $derived(app.model.projects.find((p) => p.id === repo));
  const groups = $derived(app.model.worktrees.filter((w) => w.project_id === repo).map((w) => groupOf(app.model, w)));
  const cutting = $derived(rows.pending.filter((p) => p.project === repo));

  // The sort is the view's state, so back and forward find it as left.
  const ui = $derived(nav.ui('worktrees', repo));
  const sortKey = $derived((ui.sort as SortKey | undefined) ?? 'activity');
  const sortDir = $derived((ui.dir as SortDir | undefined) ?? firstDir(sortKey));
  const table = $derived(sortRows(groups.map((g) => tableRow(g, rows.git[g.worktree.id])), sortKey, sortDir));

  function sortBy(k: SortKey) {
    if (k === sortKey) ui.dir = sortDir === 'asc' ? 'desc' : 'asc';
    else {
      ui.sort = k;
      ui.dir = firstDir(k);
    }
  }

  onMount(watchGit);
  const gitSig = $derived(gitSignal(app.model));
  $effect(() => {
    if (gitSig) untrack(() => refreshGit(groups.map((g) => g.worktree.id)));
  });

  let body: HTMLDivElement;
  function onKey(e: KeyboardEvent, id: string) {
    if (e.target !== e.currentTarget || e.ctrlKey || e.altKey || e.metaKey) return;
    const g = groups.find((x) => x.worktree.id === id);
    if (!g) return;
    const all = [...body.querySelectorAll<HTMLElement>('[data-row]')];
    const i = all.indexOf(e.currentTarget as HTMLElement);
    const run = (v: string) => verbsFor(g.worktree).find((x) => x.id === v)?.run();
    if (e.key === 'j' || e.key === 'ArrowDown') all[Math.min(all.length - 1, i + 1)]?.focus();
    else if (e.key === 'k' || e.key === 'ArrowUp') all[Math.max(0, i - 1)]?.focus();
    else if (e.key === 'Enter' || e.key === ' ') openMenu(worktreeMenu(g.worktree), e.currentTarget as HTMLElement, g.worktree.branch);
    else if (e.key === 's' && !e.shiftKey) run('session');
    else if (e.key === 'd' && !e.shiftKey) run('diff');
    else if (e.key === 'Delete' && !g.worktree.is_main) run('delete');
    else return;
    e.preventDefault();
    e.stopPropagation();
  }

  const COLS: { key: SortKey; label: string; hide?: string; right?: boolean }[] = [
    { key: 'branch', label: 'Worktree' },
    { key: 'changes', label: 'Changes' },
    { key: 'ahead', label: 'Ahead / behind', hide: '@max-[720px]:hidden' },
    { key: 'sessions', label: 'Sessions', hide: '@max-[620px]:hidden' },
    { key: 'pr', label: 'Pull request', hide: '@max-[820px]:hidden' },
    { key: 'activity', label: 'Last activity', right: true },
  ];
  const GRID =
    'grid grid-cols-[minmax(0,2fr)_88px_88px_112px_minmax(120px,1fr)_minmax(0,1fr)_96px_24px] items-center gap-x-3 ' +
    '@max-[820px]:grid-cols-[minmax(0,2fr)_88px_88px_112px_minmax(0,1fr)_96px_24px] ' +
    '@max-[720px]:grid-cols-[minmax(0,2fr)_88px_112px_minmax(0,1fr)_96px_24px] ' +
    '@max-[620px]:grid-cols-[minmax(0,1fr)_88px_96px_24px]';
  const cell = 'min-w-0 truncate font-mono text-[12px] text-muted';
</script>

<section class="@container flex min-h-0 min-w-0 flex-1 flex-col">
  <header class="flex flex-none flex-wrap items-baseline gap-x-4 gap-y-2 px-6 pt-6 pb-3">
    <h1 class="m-0 text-[22px] font-semibold tracking-tight text-ink">Worktrees</h1>
    <span class="font-mono text-[12px] text-muted">{project?.name ?? ''} &middot; {groups.length} checkout{groups.length === 1 ? '' : 's'}</span>
    <span class="flex-1"></span>
    <button
      class="h-7 border border-rule bg-transparent px-2.5 text-[13px] text-ink hover:border-rule-strong disabled:text-faint"
      title="New worktree in this repo (Shift+N)"
      disabled={!app.model.connected}
      onclick={() => newWorktree(repo)}>New worktree</button
    >
    <button
      class="h-7 border border-rule bg-transparent px-2.5 text-[13px] text-ink hover:border-rule-strong"
      title="Read every checkout's git state again now"
      onclick={() => refreshGit(groups.map((g) => g.worktree.id), true).then(() => toast('Git state read again.'))}>Refresh</button
    >
    <button
      class="h-7 border border-fail bg-transparent px-2.5 text-[13px] text-fail hover:bg-fail hover:text-bg disabled:border-rule disabled:text-faint disabled:hover:bg-transparent"
      title="Delete the worktrees with nothing in them whose PR merged or whose commits are all on a remote. Asks first, listing them; git refuses any with uncommitted work. Also on the repo's menu."
      disabled={!app.model.connected}
      onclick={() => sweepWorktrees(repo, groups)}>Delete merged &amp; clean&hellip;</button
    >
  </header>

  <div class="min-h-0 flex-1 overflow-y-auto pb-8" role="table" aria-label="Worktrees" bind:this={body}>
    <div class="{GRID} border-y border-rule-strong py-1.5 pr-4 pl-6 text-[12px] font-medium text-muted" role="row">
      {#each COLS as c (c.key)}
        {#if c.key === 'activity'}<span class="@max-[620px]:hidden" role="columnheader">Run</span>{/if}
        <span
          class="min-w-0 {c.hide ?? ''} {c.right ? 'text-right' : ''}"
          role="columnheader"
          aria-sort={sortKey === c.key ? (sortDir === 'asc' ? 'ascending' : 'descending') : 'none'}
        >
          <button
            class="max-w-full truncate border-0 bg-transparent p-0 text-[12px] font-medium hover:text-ink {sortKey === c.key ? 'text-ink' : 'text-muted'}"
            title="Sort by {c.label.toLowerCase()}"
            onclick={() => sortBy(c.key)}
            >{c.label}{#if sortKey === c.key}<span aria-hidden="true"> {sortDir === 'asc' ? '\u2191' : '\u2193'}</span>{/if}</button
          >
        </span>
      {/each}
      <span class="sr-only" role="columnheader">Actions</span>
    </div>

    {#each table as r (r.worktree.id)}
      {@const w = r.worktree}
      {@const g = r.git}
      <!-- svelte-ignore a11y_no_noninteractive_tabindex -->
      <div
        class="{GRID} group/row min-h-9 border-b border-rule pr-4 pl-6 outline-none hover:bg-surface focus-visible:outline-2 focus-visible:-outline-offset-2 focus-visible:outline-rule-strong"
        role="row"
        tabindex="0"
        data-row
        data-target="worktree:{w.id}"
        title={w.path}
        use:ownMenu={() => ({ items: worktreeMenu(w), label: w.branch })}
        onkeydown={(e) => onKey(e, w.id)}
      >
        <span class="flex min-w-0 items-center gap-2">
          <span class="w-3 flex-none text-center font-mono text-[13px] text-muted" aria-hidden="true">{w.is_main ? '\u2302' : '\u21b3'}</span>
          <span class="min-w-0 truncate font-mono text-[13px] font-medium text-ink">{w.branch}</span>
          {#if w.is_main}<span class="flex-none text-[11px] text-faint">root</span>{/if}
        </span>
        <span class="{cell} {r.changes ? 'text-git-modified' : ''}" title={g ? `${g.changed} changed, ${g.untracked} untracked` : 'Reading git\u2026'}
          >{r.changes === null ? '\u2026' : r.changes ? `${r.changes} changed` : 'clean'}</span
        >
        <span class="{cell} @max-[720px]:hidden" title={g?.upstream ? `Against ${g.upstream}` : 'No upstream'}
          >{#if !g}&hellip;{:else if g.ahead === null}<span class="text-faint">no upstream</span>{:else}&uarr;{g.ahead} &darr;{g.behind}{/if}</span
        >
        <span class="{cell} @max-[620px]:hidden"
          >{r.sessions || r.terminals.length
            ? [r.sessions && `${r.sessions} session${r.sessions === 1 ? '' : 's'}`, r.terminals.length && `${r.terminals.length} term`].filter(Boolean).join(', ')
            : 'empty'}</span
        >
        <span class="min-w-0 @max-[820px]:hidden">
          {#if r.pr}
            {@const pr = r.pr}
            <button
              class="max-w-full truncate border border-rule bg-transparent px-1 font-mono text-[12px] leading-4 hover:border-rule-strong {pr.state === 'merged'
                ? 'text-merged'
                : pr.state === 'closed'
                  ? 'text-muted line-through'
                  : 'text-ink'}"
              title="{pr.title}. Open it."
              onclick={() => api.openUrl(pr.url).catch((e) => toast(String(e)))}>{prLabel(pr)}</button
            >
          {:else}<span class="font-mono text-[12px] text-faint">none</span>{/if}
        </span>
        <span class="flex min-w-0 items-center gap-2 @max-[620px]:hidden">
          <RunButton worktree={w} label="" />
          {#each r.ports as port (port)}
            <button
              class="border-0 bg-transparent p-0 font-mono text-[12px] text-muted hover:text-ink hover:underline hover:underline-offset-4"
              title="Open http://localhost:{port}"
              onclick={() => api.openUrl(`http://localhost:${port}`).catch((e) => toast(String(e)))}>:{port}</button
            >
          {/each}
        </span>
        <span class="{cell} text-right" title={r.last ? new Date(r.last).toLocaleString() : 'No activity seen'}>{r.last ? age(r.last, app.now) : '\u2014'}</span>
        <span class="relative flex items-center justify-end">
          <!-- Over the row's right end on hover or focus, so every row keeps the same columns. -->
          <span
            class="pointer-events-none absolute top-1/2 right-7 flex -translate-y-1/2 items-center gap-0.5 bg-surface opacity-0 group-focus-within/row:pointer-events-auto group-focus-within/row:opacity-100 group-hover/row:pointer-events-auto group-hover/row:opacity-100 @max-[620px]:hidden"
          >
            {#each verbsFor(w).filter((v) => ['session', 'terminal', 'diff', 'delete'].includes(v.id)) as v (v.id)}
              <button
                class="h-6 border border-transparent bg-transparent px-1.5 text-[12px] hover:border-rule {v.danger ? 'text-fail' : 'text-muted hover:text-ink'}"
                title={verbTitle(v)}
                onclick={() => v.run()}>{v.short}</button
              >
            {/each}
          </span>
          <button
            class="grid size-6 flex-none place-items-center border border-transparent bg-transparent p-0 text-muted hover:border-rule hover:text-ink"
            title="Worktree actions (Enter on the row, or right-click)"
            aria-label="{w.branch} actions"
            aria-haspopup="menu"
            onclick={(e) => openMenu(worktreeMenu(w), e.currentTarget as HTMLElement, w.branch)}
          >
            <svg width="12" height="12" viewBox="0 0 12 12" aria-hidden="true"
              ><rect x="1" y="5" width="2" height="2" fill="currentColor" /><rect x="5" y="5" width="2" height="2" fill="currentColor" /><rect
                x="9"
                y="5"
                width="2"
                height="2"
                fill="currentColor"
              /></svg
            >
          </button>
        </span>
      </div>
    {/each}
    {#each cutting as p (p.key)}
      <div class="flex min-h-9 items-center gap-2 border-b border-rule pr-4 pl-6 text-muted" role="status">
        <span class="w-3 flex-none text-center font-mono text-[13px]" aria-hidden="true">&#8627;</span>
        <span class="min-w-0 truncate font-mono text-[13px]">{p.branch}</span>
        <span class={cell}>git is making the worktree&hellip;</span>
      </div>
    {/each}
  </div>
  <!-- The focused worktree's verbs, with their keys. -->
  <VerbBar />
</section>
