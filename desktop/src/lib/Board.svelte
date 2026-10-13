<script lang="ts">
  import { app, openSession, openDiff } from './store.svelte';
  import { LANES, lanes, toneOf, harnessLabel, lastPrompt, age, placeOf, type Lane } from './status';
  import Status from './Status.svelte';
  import { retract } from './motion';
  import Empty from './Empty.svelte';
  import type { Agent } from './api';

  let { active = true }: { active?: boolean } = $props();
  const board = $derived(lanes(app.model));
  const total = $derived(LANES.reduce((n, l) => n + board[l.id].length, 0));
  let cursor = $state<{ lane: number; row: number } | null>(null);

  function selectedAgent(): Agent | null {
    if (!cursor) return null;
    return board[LANES[cursor.lane].id][cursor.row] ?? null;
  }

  /** Arrow-key step: across lanes (skipping empty ones) or down a lane. */
  function step(dx: number, dy: number) {
    const ids = LANES.map((l) => board[l.id]);
    if (!cursor) {
      const lane = ids.findIndex((l) => l.length);
      if (lane >= 0) cursor = { lane, row: 0 };
      return;
    }
    let { lane, row } = cursor;
    if (dx) {
      let next = lane;
      do next += dx;
      while (next >= 0 && next < ids.length && !ids[next].length);
      if (next < 0 || next >= ids.length) return;
      lane = next;
      row = Math.min(row, ids[lane].length - 1);
    } else {
      row = Math.max(0, Math.min(ids[lane].length - 1, row + dy));
    }
    cursor = { lane, row };
    queueMicrotask(() => document.querySelector('[data-card-sel]')?.scrollIntoView({ block: 'nearest' }));
  }

  function onKey(e: KeyboardEvent) {
    if (!active) return;
    const t = e.target as HTMLElement;
    if (t.closest('input, textarea, select, .xterm')) return;
    const map: Record<string, [number, number]> = {
      ArrowLeft: [-1, 0], h: [-1, 0], ArrowRight: [1, 0], l: [1, 0],
      ArrowUp: [0, -1], k: [0, -1], ArrowDown: [0, 1], j: [0, 1],
    };
    if (map[e.key] && !e.ctrlKey && !e.altKey) {
      e.preventDefault();
      step(...map[e.key]);
    } else if (e.key === 'Enter') {
      const a = selectedAgent();
      if (a) openSession({ Agent: a.id });
    } else if (e.key === 'd' && !e.ctrlKey) {
      const a = selectedAgent();
      if (a) {
        app.session = { Agent: a.id };
        openDiff(a.worktree_id);
      }
    }
  }

  function isSel(lane: Lane, row: number) {
    return cursor !== null && LANES[cursor.lane].id === lane && cursor.row === row;
  }
</script>

<svelte:window onkeydown={onKey} />

<!-- The alternate home: lanes are ruled columns, sessions are ruled rows. -->
{#if total === 0}
  <Empty />
{:else}
  <div
    class="grid h-full min-h-0 grid-cols-[repeat(5,minmax(220px,1fr))] overflow-x-auto"
    role="grid"
    aria-label="Sessions by status"
  >
    {#each LANES as lane, li (lane.id)}
      {@const cards = board[lane.id]}
      <section class="flex min-h-0 min-w-0 flex-col border-r border-rule last:border-r-0" aria-label={lane.title}>
        <header class="flex h-10 flex-none items-center gap-2 border-b border-rule-strong px-4">
          <h2 class="m-0 text-[15px] font-semibold {lane.id === 'needs' && cards.length ? 'text-ember-ink' : 'text-ink'}">{lane.title}</h2>
          <span class="ml-auto font-mono text-[12px] font-bold {cards.length ? 'text-ink' : 'text-muted'}">{cards.length}</span>
        </header>
        <div class="flex min-h-0 flex-col overflow-y-auto">
          {#each cards as a, ri (a.id)}
            {@const where = placeOf(app.model, a.worktree_id)}
            {@const prompt = lastPrompt(a)}
            {@const pr = app.model.prs[a.worktree_id]}
            {@const sel = isSel(lane.id, ri)}
            {@const alert = lane.id === 'needs'}
            <button
              data-card-sel={sel || undefined}
              class="relative flex flex-col gap-1 border-0 border-b border-rule px-4 py-2.5 text-left focus-visible:-outline-offset-2
                {sel ? 'bg-surface' : 'bg-transparent hover:bg-surface'}"
              onclick={() => {
                cursor = { lane: li, row: ri };
                openSession({ Agent: a.id });
              }}
              onfocus={() => (cursor = { lane: li, row: ri })}
            >
              {#if alert}<span class="absolute inset-y-0 left-0 w-[3px] origin-left bg-ember" out:retract|global aria-hidden="true"></span>{/if}
              <span class="flex min-w-0 items-baseline gap-2">
                <span class="truncate text-[14px] font-medium {alert ? 'text-ember-ink' : 'text-ink'}">{a.name}</span>
                <span class="ml-auto font-mono text-[12px] text-muted">{age(a.status_changed_at, app.now)}</span>
              </span>
              <Status tone={lane.id === 'pr' ? 'pr' : toneOf(a)} />
              <span class="truncate text-[13px] text-muted">{harnessLabel(a)}{a.profile ? ` \u00b7 ${a.profile}` : ''}</span>
              <span class="truncate font-mono text-[12px] text-muted"
                >{where.project} / {where.branch}{#if pr}<span class="ml-2 text-ink">PR #{pr.number}</span>{/if}</span
              >
              {#if prompt}<span class="line-clamp-2 text-[13px] text-muted">{prompt}</span>{/if}
            </button>
          {/each}
          {#if !cards.length}
            <p class="m-0 px-4 py-3 text-[13px] text-muted">
              {lane.id === 'needs' ? 'Nothing needs you.' : 'Nothing here.'}
            </p>
          {/if}
        </div>
      </section>
    {/each}
  </div>
{/if}

