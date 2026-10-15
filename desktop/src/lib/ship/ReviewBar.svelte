<script lang="ts">
  // A review's verbs, above its diff: the session opened from a Review or
  // Ship card, its worktree read against the base below. Reply, Open PR (or
  // View PR), Merge while a PR is open, Archive, Delete worktree. The row
  // is the session's (`data-target`), so its keys work from here too:
  // Space replies, A archives, ? lists every verb. The terminal app's
  // review row's Alt chords work too, anywhere in the review (the diff
  // editor included): Alt+P the pull request, Alt+M the merge, Alt+A the
  // archive, Alt+W the worktree's delete.
  import type { Agent } from '../api';
  import { api } from '../api';
  import { app, toast } from '../store.svelte';
  import { archiveAgent, deleteWorktree, followUp } from '../actions';
  import { keyLabel, keysOf, reviewKeyFor, withKey, type ShortcutId } from '../shortcuts';
  import { nav } from '../shell/history.svelte';
  import { layerOpen } from '../ui/ui.svelte';
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

  /** The PR verb: its page with one open, the composer without. */
  function prVerb() {
    if (verbs.pr.kind === 'view') viewPr();
    else if (verbs.pr.why) toast(verbs.pr.why);
    else openComposer(agent.worktree_id, agent.id);
  }

  function mergeVerb() {
    if (!verbs.merge) toast('No open pull request to merge.');
    else if (verbs.merge.why) toast(verbs.merge.why);
    else openMerge(agent.worktree_id, agent.id);
  }

  function deleteVerb() {
    if (!verbs.deleteWorktree) toast('The project\u2019s own checkout is never deleted from here.');
    else deleteWorktree(agent.worktree_id);
  }

  const RUN: Partial<Record<ShortcutId, () => unknown>> = {
    review_pr: prVerb,
    review_merge: mergeVerb,
    review_archive: () => !agent.archived && archiveAgent(agent.id),
    review_delete_worktree: deleteVerb,
  };

  // The Alt chords, read wherever the review has the keys: not in a field
  // or a terminal, not under a menu or dialog.
  function onKey(e: KeyboardEvent) {
    if (nav.current.view !== 'diff' || layerOpen() || e.defaultPrevented) return;
    const t = e.target instanceof Element ? e.target : null;
    if (t?.closest('input, textarea, select, .xterm')) return;
    const s = reviewKeyFor(e);
    const run = s && RUN[s.id];
    if (!run) return;
    e.preventDefault();
    run();
  }

  /** The key a verb's button names, as the help prints it. */
  const keyOf = (id: ShortcutId) => {
    const k = keysOf(id)[0];
    return k ? keyLabel(k) : '';
  };

  const btn =
    'inline-flex h-6 flex-none items-center gap-1.5 border border-rule bg-transparent px-2 text-[12px] text-ink enabled:hover:border-rule-strong disabled:text-faint focus-visible:border-ink';
  const cap = 'font-mono text-[11px] text-faint @max-[560px]:hidden';
</script>

<svelte:window onkeydown={onKey} />

<div
  class="@container flex min-h-9 flex-none flex-wrap items-center gap-x-3 gap-y-1 border-b border-rule bg-bg px-3 py-1.5 outline-none focus-visible:outline-2 focus-visible:-outline-offset-2 focus-visible:outline-rule-strong"
  data-target="agent:{agent.id}"
  data-review-bar
  role="toolbar"
  tabindex="-1"
  aria-label="Review {agent.name}"
>
  <!-- The name and its chips keep their width: the buttons wrap below them. -->
  <span class="flex min-w-[min(100%,max-content)] flex-1 items-baseline gap-2">
    <span class="truncate text-[13px] font-medium text-ink">{agent.name}</span>
    {#if lane}<span class="flex-none font-mono text-[11px] text-muted">{lane}</span>{/if}
    {#if commits}<span class="flex-none font-mono text-[11px] text-muted" title="Commits on {where.branch} since its base">{commits}</span>{/if}
    {#if pr}<span class="flex-none font-mono text-[11px] {pr.state === 'merged' ? 'text-merged' : 'text-ink'}" title={pr.title}>{prChip(pr).word}</span>{/if}
  </span>
  <span class="flex min-w-0 flex-initial flex-wrap items-center gap-1.5">
    {#if !agent.archived}
      <button class={btn} title={withKey('Reply: the follow-up prompt', 'follow_up')} onclick={() => followUp(agent.id)}>
        Reply<kbd class={cap}>Space</kbd>
      </button>
    {/if}
    {#if verbs.pr.kind === 'view'}
      <button class={btn} title={withKey(`View #${pr?.number} on its forge`, 'review_pr')} onclick={viewPr}>
        View PR<kbd class={cap}>{keyOf('review_pr')}</kbd>
      </button>
    {:else}
      <button
        class={btn}
        disabled={!!verbs.pr.why}
        title={verbs.pr.why ?? withKey('Open a pull request for this worktree\u2019s branch (or drop the card on Ship)', 'review_pr')}
        onclick={() => openComposer(agent.worktree_id, agent.id)}>Open PR&hellip;<kbd class={cap}>{keyOf('review_pr')}</kbd></button
      >
    {/if}
    {#if verbs.merge}
      <button
        class={btn}
        disabled={!!verbs.merge.why}
        title={verbs.merge.why ?? withKey('Merge its pull request, after a confirm (or drop the card on Done)', 'review_merge')}
        onclick={() => openMerge(agent.worktree_id, agent.id)}>Merge&hellip;<kbd class={cap}>{keyOf('review_merge')}</kbd></button
      >
    {/if}
    {#if !agent.archived}
      <button class={btn} title={withKey('Archive the session', ['archive', 'review_archive'])} onclick={() => archiveAgent(agent.id)}>
        Archive<kbd class={cap}>{keyOf('review_archive')}</kbd>
      </button>
    {/if}
    {#if verbs.deleteWorktree}
      <button
        class="{btn} text-fail enabled:hover:border-fail"
        title={withKey(`Delete worktree ${where.branch}, behind a confirm that shows what it holds`, 'review_delete_worktree')}
        onclick={() => deleteWorktree(agent.worktree_id)}>Delete worktree&hellip;<kbd class={cap}>{keyOf('review_delete_worktree')}</kbd></button
      >
    {/if}
  </span>
</div>
