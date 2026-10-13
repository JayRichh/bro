<script lang="ts">
  import { app, openSession, openDiff } from './store.svelte';
  import { LANES, lanes, toneOf, harnessLabel, lastPrompt, age, placeOf, type Lane } from './status';
  import StatusDot from './StatusDot.svelte';
  import Empty from './Empty.svelte';
  import type { Agent } from './api';

  const board = $derived(lanes(app.model));
  const total = $derived(LANES.reduce((n, l) => n + board[l.id].length, 0));
  let cursor = $state<{ lane: number; row: number } | null>(null);

  const HARNESS_COLOR: Record<string, string> = {
    claude: 'text-h-claude',
    codex: 'text-h-codex',
    copilot: 'text-h-copilot',
    cursor: 'text-h-cursor',
  };

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
    if (app.view !== 'board' || app.quickPrompt) return;
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

{#if total === 0}
  <Empty />
{:else}
  <div
    class="grid h-full min-h-0 grid-cols-[repeat(5,minmax(200px,1fr))] gap-2.5 overflow-x-auto px-3.5 pt-3 pb-3.5"
    role="grid"
    aria-label="Sessions by status"
  >
    {#each LANES as lane, li (lane.id)}
      {@const cards = board[lane.id]}
      <section class="flex min-h-0 min-w-0 flex-col" aria-label={lane.title}>
        <header class="flex items-center gap-[7px] px-1 pt-1 pb-[9px]">
          <StatusDot tone={lane.tone} size={13} />
          <h2 class="m-0 text-xs font-semibold {cards.length ? 'text-fg-strong' : 'text-muted'}">{lane.title}</h2>
          <span class="ml-auto font-mono text-[11px] text-muted tabular-nums">{cards.length}</span>
        </header>
        <div class="-m-0.5 flex min-h-0 flex-col gap-1.5 overflow-y-auto p-0.5 pb-2">
          {#each cards as a, ri (a.id)}
            {@const where = placeOf(app.model, a.worktree_id)}
            {@const prompt = lastPrompt(a)}
            {@const pr = app.model.prs[a.worktree_id]}
            {@const sel = isSel(lane.id, ri)}
            {@const alert = lane.id === 'needs'}
            <button
              data-card-sel={sel || undefined}
              class="flex flex-col gap-[5px] rounded-[7px] border px-2.5 pt-[9px] pb-2.5 text-left transition-colors
                focus-visible:outline-none
                {alert ? 'border-st-needs/40 bg-st-needs/8' : 'border-line-soft bg-raised hover:border-line hover:bg-hover'}
                {sel ? 'border-focus! ring-1 ring-focus' : ''}"
              onclick={() => {
                cursor = { lane: li, row: ri };
                openSession({ Agent: a.id });
              }}
              onfocus={() => (cursor = { lane: li, row: ri })}
            >
              <div class="flex min-w-0 items-center gap-[7px]">
                <StatusDot tone={lane.id === 'pr' ? 'pr' : toneOf(a)} />
                <span class="truncate font-semibold text-fg-strong">{a.name}</span>
                <span class="ml-auto font-mono text-[11px] text-muted tabular-nums">{age(a.status_changed_at, app.now)}</span>
              </div>
              <div class="flex min-w-0 items-center gap-1.5 text-[11.5px]">
                <span class="font-medium {HARNESS_COLOR[a.kind] ?? 'text-fg'}">{harnessLabel(a)}</span>
                {#if a.profile}
                  <span class="rounded-full border border-line-soft bg-sunk px-1.5 text-[10.5px] leading-4 text-fg">{a.profile}</span>
                {/if}
                {#if a.model}<span class="truncate font-mono text-[10.5px] text-muted">{a.model}</span>{/if}
              </div>
              <div class="truncate font-mono text-[11px] text-muted">
                <span>{where.project}</span><span class="px-0.5 text-faint">/</span><span
                  class={where.isMain ? 'text-muted' : 'text-purple'}>{where.branch}</span
                >
                {#if pr}<span class="ml-1.5 text-st-pr">#{pr.number}</span>{/if}
              </div>
              <p class="m-0 line-clamp-2 text-xs leading-[1.38] {prompt ? 'text-fg' : 'text-faint italic'}">
                {prompt ?? 'No prompt yet'}
              </p>
            </button>
          {/each}
          {#if !cards.length}
            <p class="m-0 rounded-[7px] border border-dashed border-line-soft px-2.5 py-3.5 text-center text-[11.5px] text-faint">
              {lane.id === 'needs' ? 'Nobody waiting on you.' : 'Nothing here.'}
            </p>
          {/if}
        </div>
      </section>
    {/each}
  </div>
{/if}
