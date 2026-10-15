<script lang="ts">
  // The verb bar along the bottom of the overview, the board and the
  // Worktrees view: the focused row's leading verbs (the registry's, as
  // the terminal app's footer shows them) as buttons with their keys, and
  // `? more` for the rest. A click runs what the key runs. Too narrow for
  // the buttons, it folds to one `... verbs` that opens the verbs menu.
  import { app } from './store.svelte';
  import { selection } from './selection.svelte';
  import { runVerb, verbTable } from './actions';
  import { openVerbs, runShortcut, targetName } from './commands';
  import { barVerbs, deskKeysOf, KIND_OF, menuLabel } from './verbs';
  import { keyLabel, keysOf, withKey } from './shortcuts';
  import type { ActionId } from './actions.gen';

  // The row the bar acts on, while it is still there.
  const target = $derived.by(() => {
    const t = selection.target;
    const m = app.model;
    if (!t) return null;
    const there =
      t.kind === 'agent'
        ? m.agents.some((a) => a.id === t.id)
        : t.kind === 'terminal'
          ? m.terminals.some((x) => x.id === t.id)
          : t.kind === 'worktree'
            ? m.worktrees.some((w) => w.id === t.id)
            : m.projects.some((p) => p.id === t.id);
    return there ? t : null;
  });
  const kind = $derived(target ? KIND_OF[target.kind] : 'global');
  const table = $derived(target ? verbTable(target) : null);
  const verbs = $derived(
    barVerbs(kind, (id: ActionId) => (table ? !!table[id] && !table[id]?.disabled : !!deskKeysOf(id)[0])),
  );
  const what = $derived(target ? targetName(target) : 'nothing selected');

  function run(id: ActionId) {
    if (target) runVerb(target, id);
    else {
      const key = deskKeysOf(id)[0];
      if (key) runShortcut(key);
    }
  }

  const btn =
    'inline-flex h-6 flex-none items-center gap-1.5 border border-rule bg-transparent px-2 text-[12px] text-ink hover:border-rule-strong focus-visible:border-ink';
</script>

<div
  class="@container flex h-9 flex-none items-center gap-1.5 border-t border-rule bg-bg px-3"
  data-verb-bar
  role="toolbar"
  aria-label="Verbs for {what}"
>
  <span class="min-w-0 flex-1 truncate font-mono text-[11px] text-muted" title={what}>{what}</span>
  <div class="hidden flex-none items-center gap-1.5 @min-[520px]:flex">
    {#each verbs as v (v.id)}
      <button
        class="{btn} {v.danger ? 'text-fail hover:border-fail' : ''}"
        title={withKey(menuLabel(v.id, kind), deskKeysOf(v.id))}
        onclick={() => run(v.id)}
      >
        <span>{v.word}</span>
        {#if v.key}<kbd class="font-mono text-[11px] text-faint">{keyLabel(v.key)}</kbd>{/if}
      </button>
    {/each}
    <button class={btn} title={withKey('Every verb for this, searchable, and every key', 'verbs')} onclick={openVerbs}>
      <kbd class="font-mono text-[11px] text-faint">{keyLabel(keysOf('verbs')[0] ?? '?')}</kbd>
      <span>more</span>
    </button>
  </div>
  <button
    class="{btn} @min-[520px]:hidden"
    title={withKey('Verbs for this, searchable, and every key', 'verbs')}
    aria-label="Verbs"
    onclick={openVerbs}>&hellip; verbs</button
  >
</div>
