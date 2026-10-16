<script lang="ts">
  // Ctrl+K: jump to any repo, worktree, session or pull request, or run any
  // window action (and the shown session's own verbs). Flat rows, the repo
  // a row lives in dim before its name, as the terminal app's `/` draws them.
  import { tick } from 'svelte';
  import { app, openDiff, openSession, revealRepo, toast } from './store.svelte';
  import { api } from './api';
  import { ui } from './ui/ui.svelte';
  import { buildItems, labelMarks, search, type PaletteItem } from './palette';
  import { paletteCommands } from './commands';
  import { keyLabel } from './shortcuts';
  import { emptyCopy } from './learn';

  let query = $state('');
  let at = $state(0);
  let input = $state<HTMLInputElement>();
  let list = $state<HTMLUListElement>();

  const commands = $derived(ui.palette ? paletteCommands() : []);
  const items = $derived(ui.palette ? buildItems(app.model, commands) : []);
  const rows = $derived(search(items, query).slice(0, 80));

  $effect(() => {
    if (!ui.palette) return;
    query = '';
    at = 0;
    tick().then(() => input?.focus());
  });
  $effect(() => {
    void query;
    at = 0;
  });

  function close() {
    ui.palette = false;
  }

  function pick(it: PaletteItem) {
    const t = it.target;
    // Found before closing: closing empties `commands` (it is derived from
    // the palette being open), so an action looked up after never ran.
    const cmd = t.kind === 'action' ? commands.find((c) => c.id === t.id) : undefined;
    close();
    if (t.kind === 'session') openSession({ Agent: t.id });
    else if (t.kind === 'repo') revealRepo(t.id);
    else if (t.kind === 'worktree') {
      const a = app.model.agents.find((x) => x.worktree_id === t.id && !x.archived);
      if (a) openSession({ Agent: a.id });
      else openDiff(t.id);
    } else if (t.kind === 'pr') api.openUrl(t.url).catch((e) => toast(String(e)));
    else cmd?.run();
  }

  function onKey(e: KeyboardEvent) {
    e.stopPropagation();
    if (e.key === 'Escape') return (e.preventDefault(), close());
    if (e.key === 'ArrowDown' || e.key === 'ArrowUp' || (e.ctrlKey && (e.key === 'n' || e.key === 'p'))) {
      e.preventDefault();
      const d = e.key === 'ArrowDown' || e.key === 'n' ? 1 : -1;
      at = Math.max(0, Math.min(rows.length - 1, at + d));
      tick().then(() => list?.querySelector('[aria-selected="true"]')?.scrollIntoView({ block: 'nearest' }));
    } else if (e.key === 'Enter') {
      e.preventDefault();
      if (rows[at]) pick(rows[at].item);
    }
  }

  const KIND: Record<string, string> = { session: '', repo: 'repo', worktree: '', pr: '', action: '' };
</script>

{#if ui.palette}
  <!-- svelte-ignore a11y_no_static_element_interactions, a11y_click_events_have_key_events -->
  <div class="fixed inset-0 z-40 bg-[rgb(28_25_23/0.18)]" onmousedown={close}></div>
  <div
    class="fixed top-[12%] left-1/2 z-50 flex max-h-[70vh] w-[min(640px,94vw)] -translate-x-1/2 flex-col border border-rule-strong bg-bg text-ink"
    role="dialog"
    aria-modal="true"
    aria-label="Command palette"
    tabindex="-1"
    onkeydown={onKey}
  >
    <label class="flex h-11 flex-none items-center gap-3 border-b border-rule-strong px-4">
      <span class="font-mono text-[12px] text-muted" aria-hidden="true">&gt;</span>
      <span class="sr-only">Search</span>
      <input
        bind:this={input}
        bind:value={query}
        class="h-full min-w-0 flex-1 border-0 bg-transparent text-[15px] text-ink outline-none placeholder:text-faint"
        placeholder="Jump to a repo, worktree, session or PR, or run an action"
        spellcheck="false"
        role="combobox"
        aria-expanded="true"
        aria-controls="palette-list"
      />
      <button
        type="button"
        class="flex-none border border-rule bg-transparent px-1.5 font-mono text-[11px] leading-5 text-muted hover:border-rule-strong hover:text-ink"
        title="Close (Esc)"
        aria-label="Close the palette"
        onclick={close}>Esc</button
      >
    </label>
    <ul bind:this={list} id="palette-list" class="m-0 min-h-0 flex-1 list-none overflow-y-auto p-0" role="listbox">
      {#each rows as r, i (r.item.target.kind + ':' + ('id' in r.item.target ? r.item.target.id : r.item.target.url))}
        {@const it = r.item}
        {@const marks = labelMarks(it, r.positions)}
        {@const sel = i === at}
        <li role="option" aria-selected={sel}>
          <button
            class="flex w-full items-baseline gap-3 border-0 border-b border-rule px-4 py-1.5 text-left {sel ? 'bg-ink text-bg' : 'bg-transparent hover:bg-surface'}"
            onmouseenter={() => (at = i)}
            onclick={() => pick(it)}
          >
            {#if it.tier === 0}<span class="size-1.5 flex-none self-center bg-ember" aria-hidden="true"></span>{/if}
            <span class="min-w-0 flex-1 truncate text-[14px]">
              {#if it.crumb}<span class="mr-1.5 font-mono text-[12px] {sel ? 'text-bg/70' : 'text-muted'}">{it.crumb}{it.target.kind === 'action' ? '' : ' /'}</span>{/if}{#each [...it.label] as ch, ci (ci)}<span
                  class={marks.has(ci) ? 'font-semibold underline underline-offset-2' : ''}>{ch}</span
                >{/each}
            </span>
            {#if it.keys?.length}
              <span class="flex-none font-mono text-[11px] {sel ? 'text-bg' : 'text-muted'}">{keyLabel(it.keys[0])}</span>
            {:else if it.hint || KIND[it.target.kind]}
              <span class="flex-none font-mono text-[11px] {sel ? 'text-bg' : it.tier === 0 ? 'text-ember-ink' : 'text-muted'}">{it.hint}</span>
            {/if}
          </button>
        </li>
      {:else}
        <li class="px-4 py-3 text-[13px] text-muted">{emptyCopy('search').text}</li>
      {/each}
    </ul>
    <div class="flex flex-none items-center gap-4 border-t border-rule px-4 py-1.5 font-mono text-[11px] text-muted">
      <span>&uarr;&darr; move</span><span>Enter open</span><span>Esc close</span><span class="ml-auto">? keys</span>
    </div>
  </div>
{/if}
