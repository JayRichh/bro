<script lang="ts">
  // The PRs view: Mine and To review on the left, the open PR on the
  // right; or the composer, when the place is `compose:<branch>`.
  import { app } from '../store.svelte';
  import { nav } from '../shell/history.svelte';
  import Composer from './Composer.svelte';
  import Mark from './Mark.svelte';
  import Review from './Review.svelte';
  import { composeItem, parseItem, prItem, prMark, prs, voteSquare, type PrSummary } from './prs-api';
  import { prsState } from './prs-state.svelte';

  const repo = $derived(nav.current.repo ?? '');
  const item = $derived(parseItem(nav.current.item));
  const ctx = $derived(repo ? app.adoCtx[repo] : undefined);
  /** The repo's main checkout: what every PR command runs in. */
  const worktree = $derived.by(() => {
    const ws = app.model.worktrees.filter((w) => w.project_id === repo);
    return (ws.find((w) => w.is_main) ?? ws[0])?.id ?? '';
  });

  const ui = $derived(nav.ui('prs', repo));
  const tab = $derived<'mine' | 'review'>(ui.tab === 'review' ? 'review' : 'mine');
  const list = $derived(prsState.lists[repo]);
  const rows = $derived<PrSummary[]>((tab === 'mine' ? list?.data?.mine : list?.data?.review) ?? []);
  const selected = $derived(item.kind === 'pr' ? item.id : null);

  async function load() {
    const r = repo;
    const wt = worktree;
    if (!r || !wt) return;
    // Re-read after the write: the state holds a proxy of what was put in.
    prsState.lists[r] ??= { data: null, error: null, loading: false, at: 0 };
    const cur = prsState.lists[r];
    cur.loading = true;
    try {
      cur.data = await prs.list(wt);
      cur.error = null;
      cur.at = Date.now();
    } catch (e) {
      cur.error = String(e);
    } finally {
      cur.loading = false;
    }
  }

  let loadedFor = '';
  $effect(() => {
    const k = `${repo}\u0000${worktree}`;
    if (ctx && worktree && k !== loadedFor && item.kind !== 'compose') {
      loadedFor = k;
      load();
    }
  });

  // A PR just created: reload Mine so its row is there, on the Mine tab.
  let reloadedFor = 0;
  $effect(() => {
    const id = prsState.created?.id ?? 0;
    if (id && id !== reloadedFor && ctx && worktree && item.kind !== 'compose') {
      reloadedFor = id;
      ui.tab = 'mine';
      load();
    }
  });

  // "New pull request": a branch name, then the composer for it.
  let composing = $state(false);
  let newBranch = $state('');
  /** Branches checked out in this repo's worktrees, offered first. */
  const branches = $derived(
    app.model.worktrees.filter((w) => w.project_id === repo && !w.is_main && w.branch).map((w) => w.branch),
  );
  function startCompose(e: SubmitEvent) {
    e.preventDefault();
    const b = newBranch.trim().replace(/^refs\/heads\//, '');
    if (!b) return;
    composing = false;
    newBranch = '';
    nav.go({ view: 'prs', repo, item: composeItem(b) });
  }

  function open(pr: PrSummary) {
    nav.go({ view: 'prs', repo, item: prItem(pr.id) });
  }

  /** Arrow keys walk the list, as the other lists do. */
  function onListKey(e: KeyboardEvent) {
    if (e.key !== 'ArrowDown' && e.key !== 'ArrowUp' && e.key !== 'j' && e.key !== 'k') return;
    if (!rows.length) return;
    e.preventDefault();
    const i = rows.findIndex((p) => p.id === selected);
    const step = e.key === 'ArrowDown' || e.key === 'j' ? 1 : -1;
    const next = rows[Math.max(0, Math.min(rows.length - 1, i < 0 ? 0 : i + step))];
    nav.replace({ view: 'prs', repo, item: prItem(next.id) });
    queueMicrotask(() => (document.querySelector('[data-pr-cur]') as HTMLElement | null)?.focus());
  }

  const EMPTY: Record<'mine' | 'review', string> = {
    mine: 'None of your pull requests are open. Draft one from a branch or a task.',
    review: 'Nobody has asked you to review anything right now.',
  };
</script>

{#if !ctx}
  <section class="px-6 pt-6">
    <h1 class="m-0 text-[28px] font-semibold tracking-tight text-ink">Pull requests</h1>
    <p class="mt-2 max-w-[60ch] text-[14px] text-muted">
      {ctx === null ? 'This repo has no Azure DevOps remote, so it has no pull requests here.' : 'Reading the repo.'}
    </p>
  </section>
{:else if item.kind === 'compose'}
  {#key `${repo}\u0000${item.branch}`}
    <div class="min-w-0 flex-1">
      <Composer {worktree} {repo} branch={item.branch} task={item.task} />
    </div>
  {/key}
{:else}
  <div class="grid min-h-0 min-w-0 flex-1 grid-cols-[440px_minmax(0,1fr)]">
    <section class="flex min-h-0 flex-col border-r border-rule" aria-label="Pull requests">
      <header class="flex-none px-6 pt-6">
        <h1 class="m-0 text-[28px] leading-tight font-semibold tracking-tight text-ink">Pull requests</h1>
        {#if composing}
          <form class="mt-3 flex items-center gap-2" onsubmit={startCompose}>
            <!-- svelte-ignore a11y_autofocus -->
            <input
              class="min-w-0 flex-1 border border-rule-strong bg-bg px-2 py-1 font-mono text-[12px] text-ink outline-none"
              bind:value={newBranch}
              placeholder="Source branch, e.g. jari/gst-rounding"
              aria-label="Source branch"
              list="prs-branches"
              spellcheck="false"
              autofocus
              onkeydown={(e) => e.key === 'Escape' && (composing = false)}
            />
            <datalist id="prs-branches">
              {#each branches as b (b)}<option value={b}></option>{/each}
            </datalist>
            <button
              class="inline-flex h-7 flex-none items-center border-0 bg-ink px-2.5 text-[13px] font-medium text-bg enabled:hover:bg-muted disabled:bg-raised disabled:text-faint"
              disabled={!newBranch.trim()}>Compose</button
            >
          </form>
        {/if}
        <div class="mt-4 flex items-center gap-2 border-b border-rule">
          <div class="flex gap-5 self-end" role="tablist" aria-label="Which pull requests">
            {#each [{ id: 'mine', label: 'Mine', n: list?.data?.mine.length }, { id: 'review', label: 'To review', n: list?.data?.review.length }] as t (t.id)}
              {@const on = tab === t.id}
              <button
                role="tab"
                aria-selected={on}
                class="-mb-px inline-flex items-baseline gap-1.5 border-0 border-b-2 bg-transparent px-0 pb-2 text-[14px] {on
                  ? 'border-rule-strong font-medium text-ink'
                  : 'border-bg text-muted hover:text-ink'}"
                onclick={() => (ui.tab = t.id)}
                >{t.label}{#if t.n != null}<span class="font-mono text-[12px] text-muted">{t.n}</span>{/if}</button
              >
            {/each}
          </div>
          <span class="flex-1"></span>
          {#if list?.loading}<span class="mb-1.5 font-mono text-[12px] text-muted" role="status">Loading</span>{/if}
          <button
            class="mb-1.5 inline-flex h-7 items-center border border-rule-strong bg-transparent px-2.5 text-[13px] text-ink hover:bg-surface"
            onclick={() => (composing = !composing)}
            aria-expanded={composing}>New pull request</button
          >
          <button
            class="mb-1.5 grid size-7 place-items-center border-0 bg-transparent text-muted hover:bg-surface hover:text-ink"
            onclick={load}
            title="Refresh"
            aria-label="Refresh"
          >
            <svg width="13" height="13" viewBox="0 0 16 16" aria-hidden="true"
              ><path d="M13.5 8A5.5 5.5 0 1 1 11.8 4M13.5 2v3h-3" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="square" /></svg
            >
          </button>
        </div>
      </header>

      <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
      <ul class="m-0 min-h-0 flex-1 list-none overflow-y-auto p-0" role="list" onkeydown={onListKey}>
        {#each rows as pr (pr.id)}
          {@const cur = pr.id === selected}
          {@const s = prMark(pr)}
          <li>
            <button
              data-pr-cur={cur || undefined}
              class="block w-full border-0 border-b border-rule px-6 py-3 text-left {cur ? 'bg-raised' : 'bg-transparent hover:bg-surface'}"
              onclick={() => open(pr)}
              aria-current={cur ? 'true' : undefined}
            >
              <span class="flex min-w-0 items-baseline gap-3">
                <span class="min-w-0 flex-1 truncate text-[15px] font-semibold text-ink">{pr.title}</span>
                {#if pr.id === prsState.created?.id}<span class="flex-none text-[12px] font-medium text-ink">just created</span>{/if}
                <span class="flex-none font-mono text-[12px] text-muted">{pr.id}</span>
              </span>
              <span class="mt-1 flex min-w-0 items-center gap-3">
                <Mark square={s.square} word={s.word} strong={pr.isDraft} />
                <span class="min-w-0 truncate font-mono text-[12px] text-muted"
                  >{pr.source} <span aria-label="into">&rarr;</span> {pr.target}</span
                >
              </span>
              <span class="mt-2 flex min-w-0 flex-wrap items-center gap-x-3.5 gap-y-1">
                {#each pr.reviewers as r (r.id)}
                  <span class="inline-flex min-w-0 items-center gap-1.5 text-[12px]" title="{r.name}: {r.label}">
                    <span class="size-1.5 flex-none {voteSquare(r.vote)}" aria-hidden="true"></span>
                    <span class="truncate text-ink">{r.name.split(' ')[0]}</span>
                    <span class="whitespace-nowrap text-muted">{r.label}</span>
                  </span>
                {:else}
                  <span class="text-[12px] text-muted">No reviewers</span>
                {/each}
                <span class="ml-auto flex-none font-mono text-[12px] text-muted"
                  >{pr.threads} {pr.threads === 1 ? 'thread' : 'threads'}</span
                >
              </span>
            </button>
          </li>
        {/each}
        {#if list?.data && !rows.length}
          <li class="px-6 py-4 text-[13px] text-muted">{EMPTY[tab]}</li>
        {/if}
        {#if list?.error}
          <li class="px-6 py-4 text-[13px]">
            <p class="m-0 text-fail select-text">{list.error}</p>
            {#if list.data}<p class="m-0 mt-1 text-[12px] text-muted">Showing what was read before.</p>{/if}
          </li>
        {/if}
      </ul>
    </section>

    <section class="min-h-0 min-w-0" aria-label="Pull request">
      {#if selected != null && worktree}
        <Review {worktree} {repo} id={selected} onchanged={load} />
      {:else}
        <p class="m-0 px-6 pt-8 text-[13px] text-muted">Pick a pull request to see its checklist, threads and diff.</p>
      {/if}
    </section>
  </div>
{/if}
