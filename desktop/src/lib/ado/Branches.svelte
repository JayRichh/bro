<script lang="ts">
  import { onMount, tick } from 'svelte';
  import { app, openDiff, openDiffRange } from '../store.svelte';
  import { ownsKey } from './tasks-api';
  import { nav } from '../shell/history.svelte';
  import Chip from './Chip.svelte';
  import { composeItem } from './prs-api';
  import { branchesApi, filterBranches, shortAge, syncWords, type BranchRow, type BranchTree } from './branches-api';

  let { repo }: { repo: string } = $props();

  /** The target "Diff vs main" compares against. */
  const TARGET = 'main';

  const ui = $derived(nav.ui('branches', repo));
  const worktree = $derived(
    (app.model.worktrees.find((w) => w.project_id === repo && w.is_main) ?? app.model.worktrees.find((w) => w.project_id === repo))?.id ??
      null,
  );
  const query = $derived(typeof ui.q === 'string' ? ui.q : '');
  /** The selected branch is the place's item, so the breadcrumb and back/forward carry it. */
  const selectedName = $derived(nav.current.view === 'branches' && nav.current.item ? nav.current.item : null);

  let tree = $state<BranchTree | null>(null);
  let loading = $state(false);
  let error = $state<string | null>(null);
  let fetching = $state(false);
  let fetchError = $state<string | null>(null);
  let filterBox: HTMLInputElement | undefined = $state();
  let listBox: HTMLDivElement | undefined = $state();

  // Per-branch action state: which action runs, and the last failure.
  let busy = $state<{ branch: string; action: 'checkout' | 'delete' } | null>(null);
  let actionError = $state<{ branch: string; message: string } | null>(null);
  let confirmDelete = $state<string | null>(null);

  async function load() {
    if (!worktree) return;
    loading = true;
    error = null;
    try {
      tree = await branchesApi.list(worktree);
    } catch (e) {
      error = String(e);
    } finally {
      loading = false;
    }
  }
  onMount(() => {
    load().then(() => {
      if (selectedName) reveal(selectedName);
    });
  });

  async function fetchAll() {
    if (!worktree || fetching) return;
    fetching = true;
    fetchError = null;
    try {
      await branchesApi.fetch(worktree);
      await load();
    } catch (e) {
      fetchError = String(e);
    } finally {
      fetching = false;
    }
  }

  const shown = $derived(tree ? filterBranches(tree, query) : null);
  const allRows = $derived(tree ? tree.people.flatMap(([, rows]) => rows) : []);
  const selected = $derived(selectedName ? (allRows.find((r) => r.name === selectedName) ?? null) : null);

  const openMap = $derived((ui.open && typeof ui.open === 'object' ? ui.open : {}) as Record<string, boolean>);
  /** Mine open, others closed, unless toggled; a filter opens every group it keeps. */
  function isOpen(person: string): boolean {
    if (query.trim()) return true;
    return openMap[person] ?? person === tree?.me;
  }
  function toggle(person: string) {
    ui.open = { ...openMap, [person]: !isOpen(person) };
  }

  const visible = $derived(shown ? shown.people.flatMap(([p, rows]) => (isOpen(p) ? rows : [])) : []);

  function select(name: string) {
    nav.replace({ view: 'branches', repo, item: name });
    confirmDelete = null;
    if (actionError?.branch !== name) actionError = null;
  }

  function reveal(name: string) {
    const person = allRows.find((r) => r.name === name)?.person;
    if (person && !isOpen(person)) ui.open = { ...openMap, [person]: true };
    tick().then(() => listBox?.querySelector(`[data-branch="${CSS.escape(name)}"]`)?.scrollIntoView({ block: 'nearest' }));
  }

  function move(by: number) {
    if (!visible.length) return;
    const i = visible.findIndex((r) => r.name === selectedName);
    const next = visible[Math.max(0, Math.min(visible.length - 1, i < 0 ? 0 : i + by))];
    select(next.name);
    reveal(next.name);
  }

  function onKey(e: KeyboardEvent) {
    if (e.ctrlKey || e.altKey || e.metaKey) return;
    const t = e.target as HTMLElement;
    if (e.key === 'Escape' && t === filterBox) {
      ui.q = '';
      filterBox?.blur();
      return;
    }
    if (ownsKey(t, e.key)) return;
    if (e.key === '/') {
      e.preventDefault();
      filterBox?.focus();
    } else if (e.key === 'ArrowDown' || e.key === 'j') {
      e.preventDefault();
      move(1);
    } else if (e.key === 'ArrowUp' || e.key === 'k') {
      e.preventDefault();
      move(-1);
    } else if (e.key === 'f') {
      e.preventDefault();
      fetchAll();
    }
  }

  async function run(branch: string, action: 'checkout' | 'delete', work: () => Promise<void>) {
    busy = { branch, action };
    actionError = null;
    try {
      await work();
    } catch (e) {
      actionError = { branch, message: String(e) };
    } finally {
      busy = null;
    }
  }

  const checkout = (r: BranchRow) =>
    run(r.name, 'checkout', async () => {
      const id = await branchesApi.checkout(worktree!, r.name);
      await load();
      // Land in the new checkout: its session when one runs there, else its diff.
      const session = allRows.find((x) => x.name === r.name)?.session;
      if (session) nav.go({ view: 'session', repo, item: `a:${session}` });
      else openDiff(id);
    });

  const deleteLocal = (r: BranchRow) =>
    run(r.name, 'delete', async () => {
      await branchesApi.deleteLocal(worktree!, r.name);
      confirmDelete = null;
      await load();
      if (!allRows.some((x) => x.name === r.name)) nav.replace({ view: 'branches', repo });
    });

  /** The full Diff view on the branch against the target. A branch with no
   *  worktree of its own is read in the repo's main one (any will do: the
   *  range reads commits, not files on disk). */
  function diffVsMain(r: BranchRow) {
    const wt = r.worktree ?? worktree;
    if (!wt) return;
    // A remote-only branch is read from the remote; a local one as it is.
    const tip = r.local ? r.name : `refs/remotes/origin/${r.name}`;
    openDiffRange(wt, { base: TARGET, tip });
  }

  /** The sync badge's square: trouble filled, one-sided hollow, in step olive. */
  function syncSquare(r: BranchRow): string {
    if (r.gone) return 'bg-fail';
    if (!r.local || !r.remote) return 'border border-muted';
    if (r.ahead || r.behind) return 'bg-unread';
    return 'bg-done';
  }

  const action =
    'h-8 border border-rule bg-transparent px-3 text-left text-[13px] text-ink enabled:hover:border-rule-strong disabled:text-faint';
</script>

<svelte:window onkeydown={onKey} />

{#snippet sync(r: BranchRow)}
  <span class="inline-flex min-w-0 items-center gap-1.5">
    <span class="size-1.5 flex-none {syncSquare(r)}" aria-hidden="true"></span>
    <span class="truncate font-mono text-[12px] {r.gone ? 'text-fail' : 'text-muted'}">{syncWords(r)}</span>
  </span>
{/snippet}

{#snippet chips(r: BranchRow)}
  {#if r.task !== null}<Chip kind="task" label={String(r.task)} place={{ view: 'tasks', repo, item: String(r.task) }} />{/if}
  {#if r.pr}<Chip kind="pr" label={`${r.pr.id}${r.pr.is_draft ? ' draft' : ''}`} place={{ view: 'prs', repo, item: String(r.pr.id) }} />{/if}
  {#if r.session}
    <Chip kind="session" label={app.model.agents.find((a) => a.id === r.session)?.name ?? r.session} place={{ view: 'session', repo, item: `a:${r.session}` }} />
  {/if}
{/snippet}

<section class="flex min-h-0 min-w-0 flex-1 flex-col" aria-labelledby="branches-title">
  <header class="flex-none px-6 pt-6">
    <div class="flex items-baseline gap-4">
      <h1 id="branches-title" class="m-0 text-[28px] leading-tight font-semibold tracking-tight text-ink">Branches</h1>
      {#if tree}<span class="font-mono text-[12px] text-muted">{allRows.length} branches, {tree.people.length} people</span>{/if}
    </div>
    <div class="mt-4 flex items-center gap-3">
      <button
        class="h-7 border border-rule bg-transparent px-2.5 text-[13px] text-ink enabled:hover:border-rule-strong disabled:text-faint"
        onclick={fetchAll}
        disabled={fetching || !worktree}
        title="Fetch all remotes, with prune (F)">{fetching ? 'Fetching' : 'Fetch'}</button
      >
      {#if fetchError}
        <span class="flex min-w-0 items-center gap-1.5 text-[13px] text-ink" role="alert">
          <span class="size-1.5 flex-none bg-fail" aria-hidden="true"></span><span class="font-medium text-fail">Fetch failed.</span>
          <span class="truncate text-muted" title={fetchError}>{fetchError}</span>
        </span>
      {/if}
      <span class="flex-1"></span>
      <label class="flex h-7 w-72 items-center border border-rule focus-within:border-rule-strong">
        <span class="sr-only">Filter branches</span>
        <input
          bind:this={filterBox}
          class="h-full min-w-0 flex-1 border-0 bg-transparent px-2.5 font-mono text-[12px] text-ink outline-none"
          placeholder="Filter, or ga/ for a person"
          value={query}
          oninput={(e) => (ui.q = (e.currentTarget as HTMLInputElement).value)}
        />
        <kbd class="mr-1.5 border border-rule px-1 font-mono text-[11px] leading-4 text-muted">/</kbd>
      </label>
    </div>
  </header>

  <div class="mt-4 flex min-h-0 flex-1 border-t border-rule">
    <div class="min-h-0 min-w-0 flex-1 overflow-y-auto pb-8" bind:this={listBox}>
      {#if error && !tree}
        <div class="px-6 py-6">
          <p class="m-0 flex items-center gap-2 text-[14px] text-ink">
            <span class="size-1.5 flex-none bg-fail" aria-hidden="true"></span><span class="font-medium text-fail">Failed</span>
            {error}
          </p>
          <button class="mt-3 h-7 border border-rule-strong bg-transparent px-2.5 text-[13px] text-ink hover:bg-surface" onclick={load}>Try again</button>
        </div>
      {:else if !shown}
        <p class="m-0 px-6 py-6 font-mono text-[12px] text-muted">{worktree ? 'Reading branches' : 'This repo has no checkout the daemon knows.'}</p>
      {:else}
        {#if error}
          <p class="m-0 flex items-center gap-2 border-b border-rule px-6 py-2 text-[13px] text-ink" role="alert">
            <span class="size-1.5 flex-none bg-fail" aria-hidden="true"></span><span class="font-medium text-fail">Failed</span>
            <span class="min-w-0 flex-1 truncate" title={error}>Could not reread the branches; this is the last list. {error}</span>
            <button class="flex-none border-0 bg-transparent p-0 text-[13px] text-ink underline underline-offset-4 hover:no-underline disabled:text-faint" onclick={load} disabled={loading}
              >{loading ? 'Reading' : 'Try again'}</button
            >
          </p>
        {/if}
        {#each shown.people as [person, rows] (person)}
          {@const open = isOpen(person)}
          {@const mine = person === shown.me}
          <section class="pt-4" aria-label="{person} branches">
            <h2 class="m-0 border-b border-rule-strong">
              <button
                class="flex w-full items-baseline gap-3 border-0 bg-transparent px-6 pb-1.5 text-left text-ink hover:bg-surface"
                aria-expanded={open}
                onclick={() => toggle(person)}
              >
                <svg class="flex-none self-center text-muted {open ? '' : '-rotate-90'}" width="10" height="10" viewBox="0 0 10 10" aria-hidden="true"
                  ><path d="M1.5 3.5L5 7l3.5-3.5" fill="none" stroke="currentColor" stroke-width="1.4" /></svg
                >
                <span class="font-mono text-[15px] font-semibold">{person}</span>
                {#if mine}<span class="text-[12px] text-muted">you</span>{/if}
                <span class="flex-1"></span>
                <span class="font-mono text-[12px] text-muted">{rows.length}</span>
              </button>
            </h2>
            {#if open}
              <ul class="m-0 list-none p-0">
                {#each rows as r (r.name)}
                  {@const on = selectedName === r.name}
                  {@const short = r.person !== 'other' && r.name.startsWith(r.person + '/') ? r.name.slice(r.person.length + 1) : r.name}
                  <li
                    data-branch={r.name}
                    class="relative grid grid-cols-[minmax(0,1fr)_104px_minmax(0,180px)_56px] items-center gap-x-3 border-b border-rule px-6 py-2 {on
                      ? 'bg-surface'
                      : 'hover:bg-surface'}"
                  >
                    <!-- The name's button covers the whole row; the chips sit above it. -->
                    <button
                      class="min-w-0 truncate border-0 bg-transparent p-0 text-left font-mono text-[13px] leading-5 text-ink after:absolute after:inset-0 after:content-[''] {on
                        ? 'font-semibold'
                        : ''}"
                      aria-current={on ? 'true' : undefined}
                      title={r.name}
                      onclick={() => select(r.name)}
                    >
                      {#if r.person !== 'other'}<span class="text-faint">{r.person}/</span>{/if}{short}
                    </button>
                    {@render sync(r)}
                    <span class="min-w-0 truncate text-[12px] text-muted">{r.last_author}</span>
                    <span class="text-right font-mono text-[12px] text-muted" title="Last commit">{shortAge(r.last_commit_age)}</span>
                    {#if r.task !== null || r.pr || r.session}
                      <span class="relative z-10 col-span-4 mt-1.5 flex min-w-0 flex-wrap items-center gap-1.5">{@render chips(r)}</span>
                    {/if}
                  </li>
                {/each}
              </ul>
            {/if}
          </section>
        {:else}
          <div class="px-6 py-6 text-[14px] text-muted">
            {#if query}
              <p class="m-0">No branch matches <span class="font-mono text-[13px] text-ink">{query}</span>.</p>
              <button class="mt-2 border-0 bg-transparent p-0 text-[13px] text-ink underline underline-offset-4 hover:no-underline" onclick={() => (ui.q = '')}
                >Clear the filter</button
              >
            {:else}
              <p class="m-0">No branches yet.</p>
            {/if}
          </div>
        {/each}
      {/if}
    </div>

    {#if selected}
      {@const r = selected}
      {@const doing = busy?.branch === r.name ? busy.action : null}
      {@const failed = actionError?.branch === r.name ? actionError.message : null}
      <aside class="flex min-h-0 w-[440px] flex-none flex-col border-l border-rule-strong" aria-label="Branch {r.name}">
        <div class="flex-none border-b border-rule px-5 pt-4 pb-4">
          <h2 class="m-0 font-mono text-[15px] leading-snug font-semibold break-all text-ink select-text">{r.name}</h2>
          <dl class="m-0 mt-3 grid grid-cols-[88px_minmax(0,1fr)] gap-x-3 gap-y-1 text-[13px]">
            <dt class="text-muted">Sync</dt>
            <dd class="m-0">{@render sync(r)}</dd>
            <dt class="text-muted">Last commit</dt>
            <dd class="m-0 truncate text-ink"><span class="font-mono text-[12px]">{shortAge(r.last_commit_age)}</span> ago, {r.last_author}</dd>
            <dt class="text-muted">Where</dt>
            <dd class="m-0 text-ink">
              {r.local && r.remote ? 'Local and remote' : r.local ? 'Local only' : 'Remote only'}{r.worktree ? ', checked out' : ''}
            </dd>
          </dl>
          {#if r.task !== null || r.pr || r.session}
            <div class="mt-3 flex flex-wrap items-center gap-1.5">{@render chips(r)}</div>
          {/if}
        </div>

        <div class="min-h-0 flex-1 overflow-y-auto px-5 pt-4 pb-6">
          <div class="grid grid-cols-2 gap-2">
            {#if r.worktree}
              {@const wt = r.worktree}
              <button class={action} onclick={() => openDiff(wt)}>Open its worktree diff</button>
            {:else}
              <button class={action} onclick={() => checkout(r)} disabled={!!busy}>{doing === 'checkout' ? 'Checking out' : 'Check out to worktree'}</button>
            {/if}
            <button class={action} onclick={() => diffVsMain(r)} disabled={!r.worktree && !worktree}>Diff vs {TARGET}</button>
            <button class={action} onclick={() => nav.go({ view: 'prs', repo, item: composeItem(r.name, r.task) })} disabled={!!r.pr}
              >{r.pr ? 'PR already open' : 'Draft PR'}</button
            >
            {#if r.local}
              <button
                class="{action} {confirmDelete === r.name ? 'border-fail text-fail' : ''}"
                onclick={() => (confirmDelete = confirmDelete === r.name ? null : r.name)}
                disabled={!!busy || !!r.worktree}
                title={r.worktree ? 'Checked out in a worktree: remove the worktree first' : 'Delete the local branch; the remote copy stays'}>Delete local</button
              >
            {/if}
          </div>

          {#if confirmDelete === r.name}
            <div class="mt-3 border border-fail px-3 py-2.5" role="alertdialog" aria-label="Confirm delete">
              <p class="m-0 text-[13px] text-ink">
                Delete <span class="font-mono text-[12px]">{r.name}</span> on this machine?
                {r.remote ? 'The remote copy stays.' : 'It has no remote copy, so its commits go with it.'}
              </p>
              <div class="mt-2 flex gap-2">
                <button class="h-7 border-0 bg-fail px-2.5 text-[13px] font-medium text-bg disabled:bg-raised disabled:text-faint" onclick={() => deleteLocal(r)} disabled={!!busy}
                  >{doing === 'delete' ? 'Deleting' : 'Delete local branch'}</button
                >
                <button class="h-7 border border-rule bg-transparent px-2.5 text-[13px] text-ink hover:border-rule-strong" onclick={() => (confirmDelete = null)}>Keep it</button>
              </div>
            </div>
          {/if}

          {#if failed}
            <p class="m-0 mt-3 flex items-start gap-2 text-[13px] text-ink select-text" role="alert">
              <span class="mt-1.5 size-1.5 flex-none bg-fail" aria-hidden="true"></span>
              <span><span class="font-medium text-fail">Failed.</span> {failed}</span>
            </p>
          {/if}
        </div>
      </aside>
    {/if}
  </div>
</section>
