<script lang="ts">
  // A review's verbs, above its diff: the session opened from a Review or
  // Ship card, its worktree read against the base below. Reply, Open PR (or
  // View PR), Merge while a PR is open, Archive, Delete worktree. The row
  // is the session's (`data-target`), so its keys work from here too:
  // Space replies, A archives, ? lists every verb.
  import type { Agent } from '../api';
  import { api } from '../api';
  import { app, toast } from '../store.svelte';
  import { archiveAgent, deleteWorktree, followUp } from '../actions';
  import { withKey } from '../shortcuts';
  import { rows } from '../worktrees/rows.svelte';
  import { LANES, placeOf } from '../status';
  import { openComposer, openMerge } from './sheet.svelte';
  import { commitsWord, prChip, reviewVerbs } from './ship';

  let { agent }: { agent: Agent } = $props();

  const card = $derived(app.model.cards[agent.id]);
  const pr = $derived(app.model.prs[agent.worktree_id] ?? null);
  const where = $derived(placeOf(app.model, agent.worktree_id));
  const verbs = $derived(reviewVerbs(pr, card?.drops?.pr, where.isMain));
  const commits = $derived(commitsWord(rows.git[agent.worktree_id]?.ahead_of_base));
  const lane = $derived(LANES.find((l) => l.id === card?.lane)?.title ?? '');

  function viewPr() {
    if (pr) api.openUrl(pr.url).catch((e) => toast(String(e)));
  }

  const btn =
    'inline-flex h-6 flex-none items-center gap-1.5 border border-rule bg-transparent px-2 text-[12px] text-ink enabled:hover:border-rule-strong disabled:text-faint focus-visible:border-ink';
</script>

<div
  class="@container flex min-h-9 flex-none flex-wrap items-center gap-x-3 gap-y-1 border-b border-rule bg-bg px-3 py-1.5 outline-none focus-visible:outline-2 focus-visible:-outline-offset-2 focus-visible:outline-rule-strong"
  data-target="agent:{agent.id}"
  data-review-bar
  role="toolbar"
  tabindex="-1"
  aria-label="Review {agent.name}"
>
  <span class="flex min-w-0 flex-1 items-baseline gap-2">
    <span class="truncate text-[13px] font-medium text-ink">{agent.name}</span>
    {#if lane}<span class="flex-none font-mono text-[11px] text-muted">{lane}</span>{/if}
    {#if commits}<span class="flex-none font-mono text-[11px] text-muted" title="Commits on {where.branch} since its base">{commits}</span>{/if}
    {#if pr}<span class="flex-none font-mono text-[11px] {pr.state === 'merged' ? 'text-merged' : 'text-ink'}" title={pr.title}>{prChip(pr).word}</span>{/if}
  </span>
  <span class="flex flex-none flex-wrap items-center gap-1.5">
    {#if !agent.archived}
      <button class={btn} title={withKey('Reply: the follow-up prompt', 'follow_up')} onclick={() => followUp(agent.id)}>
        Reply<kbd class="font-mono text-[11px] text-faint">Space</kbd>
      </button>
    {/if}
    {#if verbs.pr.kind === 'view'}
      <button class={btn} title="Open #{pr?.number} on its forge (no key)" onclick={viewPr}>View PR</button>
    {:else}
      <button
        class={btn}
        disabled={!!verbs.pr.why}
        title={verbs.pr.why ?? 'Open a pull request for this worktree\u2019s branch (no key; or drop the card on Ship)'}
        onclick={() => openComposer(agent.worktree_id, agent.id)}>Open PR&hellip;</button
      >
    {/if}
    {#if verbs.merge}
      <button
        class={btn}
        disabled={!!verbs.merge.why}
        title={verbs.merge.why ?? 'Merge its pull request, after a confirm (no key; or drop the card on Done)'}
        onclick={() => openMerge(agent.worktree_id, agent.id)}>Merge&hellip;</button
      >
    {/if}
    {#if !agent.archived}
      <button class={btn} title={withKey('Archive the session', 'archive')} onclick={() => archiveAgent(agent.id)}>Archive</button>
    {/if}
    {#if verbs.deleteWorktree}
      <button
        class="{btn} text-fail enabled:hover:border-fail"
        title="Delete worktree {where.branch}, behind a confirm that shows what it holds (no key)"
        onclick={() => deleteWorktree(agent.worktree_id)}>Delete worktree&hellip;</button
      >
    {/if}
  </span>
</div>
