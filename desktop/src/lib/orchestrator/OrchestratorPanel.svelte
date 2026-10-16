<script lang="ts">
  // The orchestrator view's left side: every session it started, in any
  // repo, as one tree with a repo / branch chip per row; what in that tree
  // wants you; and what wants you elsewhere. A click opens the review sheet
  // on that session, so the orchestrator's chat stays on screen beside it.
  import { app } from '../store.svelte';
  import { age, placeOf } from '../status';
  import Status from '../Status.svelte';
  import { flatten, type Node } from '../overview/tree';
  import { countWords, descendantCounts, elsewhere, orchestratorTree } from '../orchestrator';
  import { openReview } from '../orch.svelte';
  // A row's right-click menu comes from its data-target (menus/router.ts):
  // a subagent's row is its parent session's, said so (`subagent:`); the
  // panel's empty space is the orchestrator's own menu.
  import { ownMenu } from '../menus/router';
  import { orchestratorMenu } from '../menus/targets';
  import OrchBadge from './OrchBadge.svelte';

  const root = $derived(orchestratorTree(app.model));
  const rows = $derived(root ? flatten(root.children, new Set(), new Set()).map((r) => r.node) : []);
  const counts = $derived(countWords(descendantCounts(root)));
  const away = $derived(elsewhere(app.model, root));

  function agentId(n: Node): string | undefined {
    return n.agent?.id ?? n.sub?.parent_agent;
  }
  function review(n: Node) {
    const id = agentId(n);
    if (id) openReview([id]);
  }
  function chip(n: Node): string {
    if (!n.agent) return '';
    const p = placeOf(app.model, n.agent.worktree_id);
    return `${p.project} / ${p.branch}`;
  }

  const ROW =
    'group relative grid min-h-9 w-full cursor-default grid-cols-[92px_minmax(0,1fr)_minmax(0,auto)_36px] items-center border-0 border-b border-rule bg-transparent pr-3 text-left outline-none hover:bg-surface focus-visible:outline-2 focus-visible:-outline-offset-2 focus-visible:outline-rule-strong';
</script>

{#snippet row(n: Node, depth: number)}
  {@const needs = n.status === 'needs'}
  <button
    class={ROW}
    data-row
    data-target={n.sub ? `subagent:${n.sub.parent_agent}` : n.agent ? `agent:${n.agent.id}` : undefined}
    onclick={() => review(n)}
    title="Review {n.label}"
  >
    {#if needs}<span class="absolute inset-y-0 left-0 w-[3px] bg-ember" aria-hidden="true"></span>{/if}
    <span class="pl-4"><Status tone={n.status} /></span>
    <span class="relative flex min-w-0 items-center self-stretch" style="padding-left: {depth * 16}px">
      {#each { length: depth } as _, lvl (lvl)}
        <span class="absolute inset-y-0 w-px bg-rule" style="left: {lvl * 16 + 4}px" aria-hidden="true"></span>
      {/each}
      <span class="min-w-0 truncate py-1.5 {n.kind === 'subagent' ? 'text-[13px]' : 'text-[14px] font-medium'} {needs ? 'text-ember-ink' : 'text-ink'}"
        >{n.label}</span
      >
    </span>
    <span class="ml-2 max-w-[220px] truncate border border-rule px-1 font-mono text-[11px] leading-4 text-muted">{chip(n)}</span>
    <span class="text-right font-mono text-[12px] text-muted">{age(n.agent?.status_changed_at ?? n.sub?.started_at ?? 0, app.now)}</span>
  </button>
{/snippet}

<section class="@container flex h-full min-h-0 flex-col" aria-label="The orchestrator's sessions" use:ownMenu={() => orchestratorMenu(false)}>
  <header class="flex flex-none flex-col gap-1.5 border-b border-rule px-4 py-3">
    <div class="flex items-center gap-2">
      <h1 class="m-0 text-[17px] font-semibold tracking-tight text-ink">{root?.label ?? 'Orchestrator'}</h1>
      <OrchBadge />
      {#if root}<Status tone={root.status} class="ml-auto" />{/if}
    </div>
    <p class="m-0 text-[13px] text-muted">
      {#if !rows.length}
        Nothing started yet. Ask it for something on the right.
      {:else if counts.length}
        {#each counts as c, i (c.status)}
          {#if i}<span class="mx-1.5 text-faint" aria-hidden="true">&middot;</span>{/if}<span
            class={c.status === 'needs' ? 'font-medium text-ember-ink' : c.status === 'failed' ? 'text-fail' : 'text-ink'}>{c.text}</span
          >
        {/each}
        <span class="ml-1 text-faint">in its tree</span>
      {:else}
        {rows.filter((r) => r.kind === 'session').length} sessions, all quiet
      {/if}
    </p>
  </header>

  <div class="min-h-0 flex-1 overflow-y-auto pb-6">
    <h2 class="m-0 px-4 pt-4 pb-1.5 text-[12px] font-medium text-muted">Started by the orchestrator</h2>
    <div role="list">
      {#each rows as n (n.key)}{@render row(n, n.depth)}{/each}
    </div>
    {#if away.length}
      <h2 class="m-0 px-4 pt-5 pb-1.5 text-[12px] font-medium text-muted">Wants you elsewhere</h2>
      <div role="list">
        {#each away as n (n.key)}{@render row(n, 0)}{/each}
      </div>
    {/if}
  </div>
</section>
