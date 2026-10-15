<script lang="ts">
  import { app, openSession } from '../store.svelte';
  import { repoRows, tally } from './repos';
  import { isEverything, scopeModel } from '../scope';
  import type { Node, Status } from './tree';

  // Tight: a split view whose breadcrumb wants the room. Under 1180px of
  // header only what is hot (needs you, failed) keeps its place; so too
  // when the scope chip names more than a repo and takes the room.
  let { tight = false }: { tight?: boolean } = $props();

  // What the scope admits (cards' `in_scope`; subagents with their parent).
  const counts = $derived(tally(repoRows(scopeModel(app.model))));
  const everything = $derived(isEverything(app.model.scope));
  const sc = $derived(app.model.scope);
  const fold = $derived(tight || !!(sc?.worktree || sc?.session || sc?.account));
  const ITEMS: { status: Status; word: (n: number) => string }[] = [
    { status: 'needs', word: (n) => (n === 1 ? 'needs you' : 'need you') },
    { status: 'failed', word: () => 'failed' },
    { status: 'running', word: () => 'running' },
    { status: 'unread', word: () => 'unread' },
  ];
  // `failed` only shows when something failed.
  const shown = $derived(ITEMS.filter((i) => i.status !== 'failed' || counts.failed?.length));

  /** Each press opens the next session in that state, round and round. */
  const turn: Partial<Record<Status, number>> = {};
  function jump(status: Status) {
    const list = counts[status] ?? [];
    if (!list.length) return;
    const i = (turn[status] ?? -1) + 1;
    turn[status] = i % list.length;
    open(list[turn[status]!]);
  }
  function open(n: Node) {
    const id = n.agent?.id ?? n.sub?.parent_agent;
    if (id) openSession({ Agent: id });
  }
</script>

<div class="flex flex-none items-center gap-4 whitespace-nowrap" role="group" aria-label={everything ? 'Across all repos' : 'In the scope'}>
  {#each shown as it, i (it.status)}
    {@const n = counts[it.status]?.length ?? 0}
    {@const hot = n > 0 && (it.status === 'needs' || it.status === 'failed')}
    {@const show = fold && !hot ? 'hidden @min-[1180px]:inline-flex' : 'inline-flex'}
    {#if i}<span class="text-faint {fold ? 'hidden @min-[1180px]:inline' : ''}" aria-hidden="true">&middot;</span>{/if}
    <button
      class="{show} items-baseline gap-1.5 border-0 bg-transparent p-0 text-[14px] enabled:hover:underline enabled:hover:underline-offset-4
        {hot ? (it.status === 'needs' ? 'text-ember-ink' : 'text-fail') : n ? 'text-ink' : 'text-muted'}"
      disabled={!n}
      onclick={() => jump(it.status)}
      title={n ? `Open the next session that is ${it.word(1)}` : undefined}
    >
      <span class="font-bold tabular-nums">{n}</span>
      <span class={hot ? 'font-medium' : ''}>{it.word(n)}</span>
    </button>
  {/each}
</div>
