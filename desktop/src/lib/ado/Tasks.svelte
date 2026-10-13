<script lang="ts">
  import { onMount, tick } from 'svelte';
  import { app, toast } from '../store.svelte';
  import { api } from '../api';
  import { nav } from '../shell/history.svelte';
  import Chip from './Chip.svelte';
  import { composeItem } from './prs-api';
  import StartTask from './StartTask.svelte';
  import {
    ageWords,
    filterTasks,
    ownsKey,
    remainingLabel,
    scopeFromKey,
    scopeKey,
    staleNotice,
    tasksApi,
    type Iteration,
    type TaskList,
    type TaskScope,
    type WorkItem,
  } from './tasks-api';
  import { branchesApi, type BranchRow, type BranchTree } from './branches-api';

  let { repo }: { repo: string } = $props();

  const ui = $derived(nav.ui('tasks', repo));
  /** Any checkout of the repo will do; the main one is always there. */
  const worktree = $derived(
    (app.model.worktrees.find((w) => w.project_id === repo && w.is_main) ?? app.model.worktrees.find((w) => w.project_id === repo))?.id ??
      null,
  );
  const scope = $derived(scopeFromKey(ui.scope));
  const query = $derived(typeof ui.q === 'string' ? ui.q : '');
  const selectedId = $derived(typeof ui.sel === 'number' ? ui.sel : null);

  let list = $state<TaskList | null>(null);
  let loading = $state(false);
  let error = $state<string | null>(null);
  let iterations = $state<Iteration[]>([]);
  let teams = $state<string[]>([]);
  let teamBusy = $state(false);
  let branches = $state<BranchTree | null>(null);
  let branchesFailed = $state(false);
  let starting = $state<WorkItem | null>(null);
  let filterBox: HTMLInputElement | undefined = $state();
  let listBox: HTMLDivElement | undefined = $state();

  // A task chip elsewhere lands here with the task's id as the place's item.
  $effect(() => {
    const item = nav.current.view === 'tasks' ? Number(nav.current.item) : NaN;
    if (Number.isInteger(item) && item > 0) ui.sel = item;
  });

  let seq = 0;
  async function load(wt: string, s: TaskScope) {
    const mine = ++seq;
    loading = true;
    error = null;
    try {
      const l = await tasksApi.tasks(wt, s);
      if (mine !== seq) return;
      list = l;
    } catch (e) {
      if (mine !== seq) return;
      error = String(e);
    } finally {
      if (mine === seq) loading = false;
    }
  }

  $effect(() => {
    if (worktree) load(worktree, scope);
  });

  async function loadSide(wt: string) {
    tasksApi
      .iterations(wt)
      .then((i) => (iterations = i))
      .catch(() => {});
    tasksApi
      .teams(wt)
      .then((t) => (teams = t))
      .catch(() => {});
    branchesApi
      .list(wt)
      .then((b) => {
        branches = b;
        branchesFailed = false;
      })
      // Start then has no initials to prefix with, and says so.
      .catch(() => (branchesFailed = true));
  }
  onMount(() => {
    if (worktree) loadSide(worktree);
  });

  const groups = $derived(list ? filterTasks(list.groups, query) : []);
  const flat = $derived(groups.flatMap((g) => g.items));
  const all = $derived(list ? list.groups.flatMap((g) => g.items) : []);
  const selected = $derived(selectedId === null ? null : (all.find((i) => i.id === selectedId) ?? null));
  const parentOf = (it: WorkItem) => list?.groups.find((g) => g.items.some((x) => x.id === it.id))?.parent ?? null;
  const stale = $derived(list ? staleNotice(list, app.now) : null);
  const rows = $derived(branches ? branches.people.flatMap(([, r]) => r) : []);
  const linked = (id: number): BranchRow[] => rows.filter((r) => r.task === id);

  function setScope(s: TaskScope) {
    ui.scope = scopeKey(s);
  }

  async function pickTeam(team: string, picker: HTMLSelectElement) {
    if (!worktree) return;
    const before = typeof ui.team === 'string' ? ui.team : '';
    teamBusy = true;
    try {
      await tasksApi.setTeam(worktree, team);
      ui.team = team;
      iterations = await tasksApi.iterations(worktree).catch(() => []);
      await load(worktree, scope);
    } catch (e) {
      // The team was not saved: the picker goes back to what is in use.
      picker.value = before;
      error = `Could not change the team: ${String(e)}`;
    } finally {
      teamBusy = false;
    }
  }

  function select(id: number) {
    ui.sel = id;
  }

  function move(by: number) {
    if (!flat.length) return;
    const i = flat.findIndex((x) => x.id === selectedId);
    const next = flat[Math.max(0, Math.min(flat.length - 1, i < 0 ? 0 : i + by))];
    select(next.id);
    tick().then(() => listBox?.querySelector(`[data-id="${next.id}"]`)?.scrollIntoView({ block: 'nearest' }));
  }

  function onKey(e: KeyboardEvent) {
    if (starting || e.ctrlKey || e.altKey || e.metaKey) return;
    const t = e.target as HTMLElement;
    if (e.key === 'Escape' && t === filterBox) {
      ui.q = '';
      filterBox?.blur();
      return;
    }
    // Typing, or Enter/Space on a focused button, link or radio: not ours.
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
    } else if ((e.key === 's' || e.key === 'Enter') && selected) {
      e.preventDefault();
      starting = selected;
    }
  }

  function fmtDay(d: string | null): string {
    if (!d) return '';
    return new Date(d).toLocaleDateString(undefined, { day: 'numeric', month: 'short', timeZone: 'UTC' });
  }

  /** The state's square: hollow for not started, filled once work is on it. */
  const SQUARE: Record<string, string> = {
    Proposed: 'border border-muted',
    InProgress: 'bg-running',
    Resolved: 'bg-unread',
    Completed: 'bg-done',
    Removed: 'bg-faint',
  };

  const SCOPES: { key: TaskScope; label: string }[] = [
    { key: 'currentSprint', label: 'Current sprint' },
    { key: 'myOpen', label: 'My open' },
    { key: 'sprintAll', label: 'Sprint, everyone' },
  ];
  const iterationPicked = $derived(typeof scope === 'object' ? scope.iteration : '');

  const pick =
    'h-7 w-full min-w-0 appearance-none border border-rule bg-transparent pr-7 pl-2.5 text-[13px] text-ink outline-none hover:border-rule-strong disabled:text-faint';
  const textBtn =
    'border-0 bg-transparent p-0 text-[13px] text-ink underline underline-offset-4 hover:no-underline disabled:text-faint disabled:no-underline';
</script>

<svelte:window onkeydown={onKey} />

{#snippet chevron()}
  <svg class="pointer-events-none absolute top-1/2 right-2.5 -translate-y-1/2 text-muted" width="10" height="10" viewBox="0 0 10 10" aria-hidden="true"
    ><path d="M1.5 3.5L5 7l3.5-3.5" fill="none" stroke="currentColor" stroke-width="1.4" /></svg
  >
{/snippet}

{#snippet stateWord(it: WorkItem)}
  <span class="inline-flex min-w-0 items-center gap-1.5">
    <span class="size-1.5 flex-none {SQUARE[it.stateCategory] ?? 'border border-muted'}" aria-hidden="true"></span>
    <span class="truncate text-[12px] text-muted">{it.state}</span>
  </span>
{/snippet}

<section class="flex min-h-0 min-w-0 flex-1 flex-col" aria-labelledby="tasks-title">
  <header class="flex-none px-6 pt-6">
    <div class="flex items-baseline gap-4">
      <h1 id="tasks-title" class="m-0 text-[28px] leading-tight font-semibold tracking-tight text-ink">Tasks</h1>
      {#if list?.iteration}
        <span class="font-mono text-[12px] text-muted">
          {list.iteration.name}{#if list.iteration.start && list.iteration.finish}, {fmtDay(list.iteration.start)} to {fmtDay(list.iteration.finish)}{/if}
        </span>
      {/if}
      <span class="flex-1"></span>
      <button
        class="h-7 border border-rule bg-transparent px-2.5 text-[13px] text-ink hover:border-rule-strong disabled:text-faint"
        onclick={() => worktree && load(worktree, scope)}
        disabled={loading || !worktree}>{loading ? 'Loading' : 'Refresh'}</button
      >
    </div>

    <!-- Scope bar -->
    <div class="mt-4 flex flex-wrap items-center gap-3">
      <div class="flex h-7 flex-none border border-rule" role="radiogroup" aria-label="Scope">
        {#each SCOPES as s, i (s.label)}
          {@const on = scopeKey(scope) === scopeKey(s.key)}
          <button
            role="radio"
            aria-checked={on}
            class="border-0 px-2.5 text-[13px] {i ? 'border-l border-rule' : ''} {on ? 'bg-ink text-bg' : 'bg-transparent text-muted hover:text-ink'}"
            onclick={() => setScope(s.key)}>{s.label}</button
          >
        {/each}
      </div>
      <span class="relative flex w-44">
        <select
          class="{pick} {iterationPicked ? 'border-rule-strong' : ''}"
          aria-label="Iteration"
          value={iterationPicked}
          onchange={(e) => {
            const v = (e.currentTarget as HTMLSelectElement).value;
            setScope(v ? { iteration: v } : 'currentSprint');
          }}
        >
          <option value="">Pick an iteration</option>
          {#each iterations as it (it.id)}<option value={it.id}>{it.name}{it.timeframe === 'current' ? ' (current)' : ''}</option>{/each}
        </select>
        {@render chevron()}
      </span>
      <span class="relative flex w-44">
        <select
          class={pick}
          aria-label="Team"
          value={typeof ui.team === 'string' ? ui.team : ''}
          disabled={teamBusy || !teams.length}
          onchange={(e) => pickTeam(e.currentTarget.value, e.currentTarget)}
        >
          <option value="">Default team</option>
          {#each teams as t (t)}<option value={t}>{t}</option>{/each}
        </select>
        {@render chevron()}
      </span>
      {#if teamBusy}<span class="font-mono text-[12px] text-muted">Saving team</span>{/if}
      <span class="flex-1"></span>
      <label class="flex h-7 w-72 items-center border border-rule focus-within:border-rule-strong">
        <span class="sr-only">Filter tasks</span>
        <input
          bind:this={filterBox}
          class="h-full min-w-0 flex-1 border-0 bg-transparent px-2.5 text-[13px] text-ink outline-none"
          placeholder="Filter by title or id"
          value={query}
          oninput={(e) => (ui.q = (e.currentTarget as HTMLInputElement).value)}
        />
        <kbd class="mr-1.5 border border-rule px-1 font-mono text-[11px] leading-4 text-muted">/</kbd>
      </label>
    </div>

    {#if list?.cachedAt}
      <p class="m-0 mt-3 flex items-center gap-2 text-[13px] text-muted" role="status">
        <span class="size-1.5 flex-none border border-muted" aria-hidden="true"></span>
        Offline, as of {ageWords(list.cachedAt, app.now)}. This is the last list saved for this scope.
      </p>
    {/if}
    {#if stale}
      <p class="m-0 mt-3 flex items-center gap-3 text-[13px] text-ink" role="status">
        <span class="inline-flex items-center gap-2"><span class="size-1.5 flex-none bg-faint" aria-hidden="true"></span>{stale}.</span>
        {#if list?.next}
          {@const next = list.next}
          <button class={textBtn} onclick={() => setScope({ iteration: next.id })}>Show {next.name}</button>
        {/if}
      </p>
    {/if}
  </header>

  <div class="mt-4 flex min-h-0 flex-1 border-t border-rule">
    <div class="min-h-0 min-w-0 flex-1 overflow-y-auto pb-8" bind:this={listBox}>
      {#if error && !list}
        <div class="px-6 py-6">
          <p class="m-0 flex items-center gap-2 text-[14px] text-ink">
            <span class="size-1.5 flex-none bg-fail" aria-hidden="true"></span><span class="font-medium text-fail">Failed</span>
            {error}
          </p>
          <button class="mt-3 h-7 border border-rule-strong bg-transparent px-2.5 text-[13px] text-ink hover:bg-surface" onclick={() => worktree && load(worktree, scope)}
            >Try again</button
          >
        </div>
      {:else if !list}
        <p class="m-0 px-6 py-6 font-mono text-[12px] text-muted">{worktree ? 'Loading tasks' : 'This repo has no checkout the daemon knows.'}</p>
      {:else}
        {#if error}
          <p class="m-0 flex items-center gap-2 border-b border-rule px-6 py-2 text-[13px] text-ink" role="alert">
            <span class="size-1.5 flex-none bg-fail" aria-hidden="true"></span><span class="font-medium text-fail">Failed</span>
            {error}
          </p>
        {/if}
        {#each groups as g, gi (g.parent?.id ?? `none-${gi}`)}
          <section class="pt-5" aria-label={g.parent ? `${g.parent.kind} ${g.parent.id}` : 'No parent'}>
            <h2 class="m-0 flex min-w-0 items-baseline gap-3 border-b border-rule-strong px-6 pb-1.5 text-[15px] font-semibold text-ink">
              {#if g.parent}
                <span class="flex-none font-mono text-[12px] font-normal text-muted">{g.parent.kind} {g.parent.id}</span>
                <span class="min-w-0 truncate">{g.parent.title}</span>
                <span class="flex-1"></span>
                <span class="flex-none font-mono text-[12px] font-normal text-muted">{g.items.length} {g.items.length === 1 ? 'item' : 'items'}</span>
              {:else}
                <span class="min-w-0 truncate">No parent</span>
              {/if}
            </h2>
            <ul class="m-0 list-none p-0">
              {#each g.items as it (it.id)}
                {@const on = selectedId === it.id}
                <li>
                  <button
                    data-id={it.id}
                    class="grid h-9 w-full grid-cols-[64px_minmax(0,1fr)_120px_96px_48px] items-center gap-3 border-0 border-b border-rule px-6 text-left text-[14px] {on
                      ? 'bg-surface'
                      : 'bg-transparent hover:bg-surface'}"
                    aria-current={on ? 'true' : undefined}
                    onclick={() => select(it.id)}
                    ondblclick={() => (starting = it)}
                  >
                    <span class="font-mono text-[12px] text-muted">{it.id}</span>
                    <span class="min-w-0 truncate text-ink {on ? 'font-medium' : ''}">{it.title}</span>
                    {@render stateWord(it)}
                    <span class="truncate font-mono text-[12px] text-muted">{it.boardColumn ?? ''}</span>
                    <span class="text-right font-mono text-[12px] text-muted">{remainingLabel(it.remaining)}</span>
                  </button>
                </li>
              {/each}
            </ul>
          </section>
        {:else}
          <div class="px-6 py-6 text-[14px] text-muted">
            {#if query}
              <p class="m-0">No task matches "{query}".</p>
              <button class="{textBtn} mt-2" onclick={() => (ui.q = '')}>Clear the filter</button>
            {:else if scope === 'currentSprint'}
              <p class="m-0">Nothing in {list.iteration?.name ?? 'this sprint'} is assigned to you.</p>
              <button class="{textBtn} mt-2" onclick={() => setScope('sprintAll')}>Show everyone's open work</button>
            {:else}
              <p class="m-0">No tasks in this scope.</p>
            {/if}
          </div>
        {/each}
      {/if}
    </div>

    {#if starting && worktree}
      <aside class="flex min-h-0 w-[480px] flex-none flex-col border-l border-rule-strong" aria-label="Start task {starting.id}">
        <StartTask
          {worktree}
          {repo}
          item={starting}
          initials={branches?.me ?? ''}
          initialsUnknown={branchesFailed}
          taken={rows.map((r) => r.name)}
          onclose={() => (starting = null)}
          onstarted={() => {
            if (worktree) loadSide(worktree);
          }}
        />
      </aside>
    {:else if selected}
      {@const parent = parentOf(selected)}
      {@const links = linked(selected.id)}
      <aside class="flex min-h-0 w-[440px] flex-none flex-col border-l border-rule-strong" aria-label="Task {selected.id}">
        <div class="flex-none border-b border-rule px-5 pt-4 pb-4">
          <p class="m-0 flex items-baseline gap-3 font-mono text-[12px] text-muted">
            <span>{selected.kind} {selected.id}</span>
            {#if parent}<span class="min-w-0 truncate">in {parent.kind} {parent.id}</span>{/if}
          </p>
          <h2 class="m-0 mt-1 text-[18px] leading-snug font-semibold text-ink">{selected.title}</h2>
          <dl class="m-0 mt-3 grid grid-cols-[88px_minmax(0,1fr)] gap-x-3 gap-y-1 text-[13px]">
            <dt class="text-muted">State</dt>
            <dd class="m-0">{@render stateWord(selected)}</dd>
            {#if selected.boardColumn}<dt class="text-muted">Column</dt>
              <dd class="m-0 font-mono text-[12px] text-ink">{selected.boardColumn}</dd>{/if}
            <dt class="text-muted">Assigned</dt>
            <dd class="m-0 truncate text-ink">{selected.assignedTo ?? 'Nobody'}</dd>
            {#if selected.remaining !== null}<dt class="text-muted">Remaining</dt>
              <dd class="m-0 font-mono text-[12px] text-ink">{remainingLabel(selected.remaining)}</dd>{/if}
          </dl>
          <div class="mt-4 flex items-center gap-2">
            <button class="h-8 border-0 bg-ink px-3 text-[13px] font-medium text-bg hover:bg-muted" onclick={() => (starting = selected)} title="Start (S)"
              >Start task</button
            >
            <button
              class="h-8 border border-rule bg-transparent px-3 text-[13px] text-ink hover:border-rule-strong"
              onclick={() => api.openUrl(selected.url).catch((e) => toast(`Could not open the task in the browser: ${String(e)}`))}>Open in ADO</button
            >
          </div>
        </div>
        <div class="min-h-0 flex-1 overflow-y-auto px-5 pb-6">
          <h3 class="m-0 mt-5 text-[13px] font-semibold text-ink">Linked</h3>
          {#if links.length}
            <ul class="m-0 mt-2 flex list-none flex-col gap-1.5 p-0">
              {#each links as b (b.name)}
                <li class="flex min-w-0 flex-wrap items-center gap-1.5">
                  <Chip kind="branch" label={b.name} place={{ view: 'branches', repo, item: b.name }} />
                  {#if b.pr}<Chip kind="pr" label={`${b.pr.id}${b.pr.is_draft ? ' draft' : ''}`} place={{ view: 'prs', repo, item: String(b.pr.id) }} />{/if}
                  {#if !b.pr}
                    <button
                      class="border border-rule bg-transparent px-1.5 py-px text-[12px] leading-[18px] text-ink hover:border-rule-strong hover:bg-surface"
                      onclick={() => nav.go({ view: 'prs', repo, item: composeItem(b.name, selected.id) })}
                      title="Draft a pull request from {b.name} linked to this task">Draft PR</button
                    >
                  {/if}
                  {#if b.session}<Chip kind="session" label={app.model.agents.find((a) => a.id === b.session)?.name ?? b.session} place={{ view: 'session', repo, item: `a:${b.session}` }} />{/if}
                </li>
              {/each}
            </ul>
          {:else}
            <p class="m-0 mt-1 text-[13px] text-muted">No branch, PR or session yet. Start the task to make one.</p>
          {/if}
          <h3 class="m-0 mt-6 text-[13px] font-semibold text-ink">Description</h3>
          <p class="m-0 mt-1 max-w-[65ch] text-[13px] whitespace-pre-wrap text-ink select-text {selected.descriptionMd ? '' : 'text-muted'}">
            {selected.descriptionMd || 'No description.'}
          </p>
          <h3 class="m-0 mt-6 text-[13px] font-semibold text-ink">Acceptance criteria</h3>
          <p class="m-0 mt-1 max-w-[65ch] text-[13px] whitespace-pre-wrap text-ink select-text {selected.acceptanceMd ? '' : 'text-muted'}">
            {selected.acceptanceMd || 'None written.'}
          </p>
        </div>
      </aside>
    {/if}
  </div>
</section>
