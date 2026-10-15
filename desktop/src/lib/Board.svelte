<script lang="ts">
  import { app, markSeen, openSession, openDiff, toast } from './store.svelte';
  import { LANES, lanes, toneOf, harnessLabel, lastPrompt, age, placeOf, prWord, type Lane } from './status';
  import Status from './Status.svelte';
  import { retract } from './motion';
  import Empty from './Empty.svelte';
  import type { Agent } from './api';
  import { agentMenu, archiveAgent, followUp, unarchiveAgent } from './actions';
  import { layerOpen, openMenu } from './ui/ui.svelte';
  import { withKey } from './shortcuts';
  import Icon from './ui/Icon.svelte';
  import { drag, dropCard, endDrag, leftFor, offsetIn, startDrag } from './reorder.svelte';
  import { dropZone, laneDrop, type Place } from './order';

  let { active = true }: { active?: boolean } = $props();

  // Drags on the board. A card dropped on a lane does what the lane means
  // where the user can cause it (order.ts `laneDrop`): Unread to Done marks
  // it seen, the Archived zone (shown while dragging) archives it, a drop
  // beside a card of its own checkout reorders them; a status the agent or
  // the pull request owns is refused with the reason. The keys: Alt+Up /
  // Alt+Down reorder, A archives, the menu's Mark seen.
  /** The lane under the pointer, and the card it would land beside. */
  let target = $state<{ lane: Lane | 'archived'; card?: { id: string; place: Place } } | null>(null);
  const dragged = $derived(drag.item?.kind === 'agent' && drag.item.lane ? drag.item : null);

  function overLane(e: DragEvent, lane: Lane | 'archived') {
    if (!dragged) return;
    e.preventDefault();
    if (e.dataTransfer) e.dataTransfer.dropEffect = 'move';
    if (target?.lane !== lane || target.card) target = { lane };
  }

  function overCard(e: DragEvent, lane: Lane, a: Agent) {
    if (!dragged || dragged.lane !== lane || dragged.id === a.id) return;
    e.preventDefault();
    e.stopPropagation();
    if (e.dataTransfer) e.dataTransfer.dropEffect = 'move';
    // Only beside a card of its own checkout: the order is per checkout.
    if (a.worktree_id !== dragged.worktree) return void (target = { lane });
    const [y, h] = offsetIn(e);
    target = { lane, card: { id: a.id, place: dropZone(y, h, { insert: true, onto: false }) as Place } };
  }

  async function dropOn(e: DragEvent, lane: Lane | 'archived', over?: Agent) {
    const d = dragged;
    const at = target;
    target = null;
    if (!d?.lane) return;
    e.preventDefault();
    e.stopPropagation();
    endDrag();
    const what = laneDrop(d.lane, lane);
    if (what.act === 'refuse') return toast(what.reason);
    if (what.act === 'seen') return markSeen({ Agent: d.id });
    if (what.act === 'archive') return archiveAgent(d.id);
    if (at?.card) return dropCard(d.worktree, d.id, at.card.id, at.card.place);
    if (over && over.id !== d.id && over.worktree_id !== d.worktree)
      toast(`Cards reorder beside cards of their own worktree (${placeOf(app.model, d.worktree).branch}); move the session (M) to change worktree.`);
  }

  function endBoardDrag() {
    target = null;
    endDrag();
  }

  /** A lane's look while a card is over it: ember for a drop that acts. */
  function laneMark(lane: Lane): string {
    if (!dragged || target?.lane !== lane || target.card || dragged.lane === lane) return '';
    const act = laneDrop(dragged.lane!, lane).act;
    return act === 'refuse' ? 'bg-surface' : 'outline-2 -outline-offset-2 outline-ember';
  }
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
    // The card takes the focus, so its row keys (r, a, Del\u2026) reach it.
    queueMicrotask(() => {
      const el = document.querySelector<HTMLElement>('[data-card-sel]');
      el?.focus();
      el?.scrollIntoView({ block: 'nearest' });
    });
  }

  function onKey(e: KeyboardEvent) {
    if (!active || layerOpen()) return;
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
      // A card's own action button takes its Enter itself.
      if (t.closest('[data-card-tools]')) return;
      const a = selectedAgent();
      if (a) openSession({ Agent: a.id });
    } else if (e.key === 'd' && !e.ctrlKey) {
      const a = selectedAgent();
      if (a) diffCard(a);
    }
  }

  /** D, or a card's Diff: the card becomes the session, its worktree diffed. */
  function diffCard(a: Agent) {
    app.session = { Agent: a.id };
    markSeen(app.session);
    openDiff(a.worktree_id);
  }

  const tool =
    'grid size-6 place-items-center border-0 border-l border-rule bg-transparent p-0 text-muted first:border-l-0 hover:bg-surface hover:text-ink';

  function isSel(lane: Lane, row: number) {
    return cursor !== null && LANES[cursor.lane].id === lane && cursor.row === row;
  }
</script>

<svelte:window onkeydown={onKey} />

<!-- The alternate home: lanes are ruled columns, sessions are ruled rows. -->
{#if total === 0}
  <Empty />
{:else}
  <div class="relative h-full min-h-0" data-board>
  <div
    class="grid h-full min-h-0 grid-cols-[repeat(5,minmax(220px,1fr))] overflow-x-auto"
    role="grid"
    aria-label="Sessions by status"
  >
    {#each LANES as lane, li (lane.id)}
      {@const cards = board[lane.id]}
      <section
        class="flex min-h-0 min-w-0 flex-col border-r border-rule last:border-r-0 {laneMark(lane.id)}"
        aria-label={lane.title}
        ondragenter={(e) => overLane(e, lane.id)}
        ondragover={(e) => overLane(e, lane.id)}
        ondragleave={(e) => leftFor(e) && target?.lane === lane.id && (target = null)}
        ondrop={(e) => dropOn(e, lane.id)}
      >
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
            <!-- The card's verbs show on hover or focus; the card keys (F, A, D,
                 Shift+F10) do the same from the card. -->
            {@const mark = dragged && target?.card?.id === a.id ? target.card.place : null}
            <div
              class="group relative {dragged?.id === a.id ? 'opacity-60' : ''}"
              data-target="agent:{a.id}"
              role="presentation"
              draggable="true"
              ondragstart={(e) => startDrag(e, { kind: 'agent', id: a.id, worktree: a.worktree_id, lane: lane.id })}
              ondragend={endBoardDrag}
              ondragenter={(e) => overCard(e, lane.id, a)}
              ondragover={(e) => overCard(e, lane.id, a)}
              ondrop={(e) => dropOn(e, lane.id, a)}
            >
            {#if mark}
              <span class="pointer-events-none absolute inset-x-0 z-10 h-px bg-ember {mark === 'before' ? '-top-px' : '-bottom-px'}" aria-hidden="true"></span>
            {/if}
            <button
              title="{a.name}: {withKey('drag to reorder beside cards of its worktree', ['move_up', 'move_down'])}, or onto a lane, Archived or a repo in the rail"
              data-card-sel={sel || undefined}
              oncontextmenu={(e) => {
                e.preventDefault();
                cursor = { lane: li, row: ri };
                (e.currentTarget as HTMLElement).focus();
                openMenu(agentMenu(a), { x: e.clientX, y: e.clientY }, a.name);
              }}
              class="relative flex w-full flex-col gap-1 border-0 border-b border-rule px-4 py-2.5 text-left focus-visible:-outline-offset-2
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
              <Status tone={lane.id === 'pr' ? 'pr' : lane.id === 'done' && pr?.state === 'merged' ? 'merged' : toneOf(a)} />
              <span class="truncate text-[13px] text-muted">{harnessLabel(a)}{a.profile ? ` \u00b7 ${a.profile}` : ''}</span>
              <span class="truncate font-mono text-[12px] text-muted"
                >{where.project} / {where.branch}{#if pr}<span class="ml-2 {pr.state === 'merged' ? 'text-merged' : pr.state === 'closed' ? 'text-muted' : 'text-ink'}">{prWord(pr)} #{pr.number}</span>{/if}</span
              >
              {#if prompt}<span class="line-clamp-2 text-[13px] text-muted">{prompt}</span>{/if}
            </button>
            <div
              data-card-tools
              class="absolute top-2 right-3 hidden border border-rule bg-bg group-focus-within:flex group-hover:flex"
              role="toolbar"
              aria-label="{a.name} actions"
            >
              <button class={tool} title={withKey('Diff its worktree', 'board_diff')} aria-label="Diff {a.name}" onclick={() => diffCard(a)}><Icon name="diff" size={12} /></button>
              {#if a.archived}
                <button class={tool} title={withKey('Unarchive', 'unarchive')} aria-label="Unarchive {a.name}" onclick={() => unarchiveAgent(a.id)}><Icon name="unarchive" size={12} /></button>
              {:else}
                <button class={tool} title={withKey('Follow-up prompt', 'follow_up')} aria-label="Follow-up to {a.name}" onclick={() => followUp(a.id)}><Icon name="reply" size={12} /></button>
                <button class={tool} title={withKey('Archive', 'archive')} aria-label="Archive {a.name}" onclick={() => archiveAgent(a.id)}><Icon name="archive" size={12} /></button>
              {/if}
              <button
                class={tool}
                title={withKey('Actions: restart, move, rename, delete', 'row_menu')}
                aria-label="{a.name} actions"
                aria-haspopup="menu"
                onclick={(e) => {
                  cursor = { lane: li, row: ri };
                  openMenu(agentMenu(a), e.currentTarget as HTMLElement, a.name);
                }}><Icon name="dots" size={12} /></button
              >
            </div>
            </div>
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
  {#if dragged}
    <!-- Only while a card is dragged: drop it here to archive it (A on the card). -->
    <div
      class="absolute inset-x-4 bottom-4 z-20 flex h-12 items-center justify-center gap-2 border bg-bg text-[13px] {target?.lane === 'archived'
        ? 'border-ember text-ink'
        : 'border-rule-strong text-muted'}"
      role="region"
      aria-label="Archive"
      ondragenter={(e) => overLane(e, 'archived')}
      ondragover={(e) => overLane(e, 'archived')}
      ondragleave={(e) => leftFor(e) && target?.lane === 'archived' && (target = null)}
      ondrop={(e) => dropOn(e, 'archived')}
    >
      <Icon name="archive" size={12} />Drop here to archive
    </div>
  {/if}
  </div>
{/if}

