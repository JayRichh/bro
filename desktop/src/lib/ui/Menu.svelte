<script lang="ts">
  // The context menu: square, ruled, ink for the row under the keys.
  // Arrows walk it, Enter runs, Esc or a click outside closes. A
  // searchable one (the verbs menu, `?`) has a field over its rows: typing
  // narrows them, the arrows still walk, and its pinned last row (All
  // keys) stays.
  import { tick } from 'svelte';
  import { closeMenu, ui, type MenuEntry, type MenuItem } from './ui.svelte';
  import { keyLabel } from '../shortcuts';
  import { fuzzyMatch } from '../fuzzy';
  import { filterRows } from '../verbs';

  let box = $state<HTMLDivElement>();
  let field = $state<HTMLInputElement>();
  let at = $state(-1);
  let pos = $state({ x: 0, y: 0 });
  let query = $state('');

  function isItem(e: MenuEntry): e is MenuItem {
    return typeof e === 'object' && 'run' in e;
  }

  // With a query, only the rows that match (headings and rules go: the
  // list is ranked, not grouped).
  const items = $derived.by((): MenuEntry[] => {
    const all = ui.menu?.items ?? [];
    if (!ui.menu?.search || !query.trim()) return all;
    return filterRows(all.filter(isItem), query, (q, s) => fuzzyMatch(q, s)?.score ?? null);
  });
  const actionable = $derived(items.map((e, i) => (isItem(e) && !e.disabled ? i : -1)).filter((i) => i >= 0));

  // Place inside the window and take the keys when it opens.
  $effect(() => {
    const m = ui.menu;
    if (!m) return;
    at = -1;
    query = '';
    pos = { x: m.x, y: m.y };
    tick().then(() => {
      if (!box) return;
      const r = box.getBoundingClientRect();
      pos = {
        x: Math.max(4, Math.min(m.x, window.innerWidth - r.width - 4)),
        y: Math.max(4, Math.min(m.y, window.innerHeight - r.height - 4)),
      };
      if (m.search) {
        field?.focus();
        at = actionable[0] ?? -1;
      } else if (!m.keep) box.focus();
    });
  });

  // A menu that keeps the focus where it was (a field's, the terminal's)
  // takes every key from the window while it is up, as a native menu
  // does, and goes when the window does.
  $effect(() => {
    if (!ui.menu?.keep) return;
    const take = (e: KeyboardEvent) => {
      e.preventDefault();
      onKey(e);
    };
    window.addEventListener('keydown', take, true);
    window.addEventListener('blur', closeMenu);
    return () => {
      window.removeEventListener('keydown', take, true);
      window.removeEventListener('blur', closeMenu);
    };
  });

  /** A click on a kept menu, or outside it, leaves the focus put. */
  function keepFocus(e: MouseEvent) {
    if (ui.menu?.keep) e.preventDefault();
  }

  // The row under the keys stays in view in a long list.
  $effect(() => {
    const i = at;
    if (i >= 0) box?.querySelector(`[data-i="${i}"]`)?.scrollIntoView({ block: 'nearest' });
  });

  // A query lands on its best row.
  $effect(() => {
    if (ui.menu?.search && query.trim()) at = actionable[0] ?? -1;
  });

  function run(e: MenuItem) {
    if (e.disabled) return;
    closeMenu();
    e.run();
  }

  function step(d: number) {
    if (!actionable.length) return;
    const i = actionable.indexOf(at);
    at = actionable[i < 0 ? (d > 0 ? 0 : actionable.length - 1) : (i + d + actionable.length) % actionable.length];
  }

  function onKey(e: KeyboardEvent) {
    e.stopPropagation();
    // In the search field the letters are the query's: only the arrows
    // walk, and Esc clears what is typed before it closes.
    const typing = !!ui.menu?.search;
    if (e.key === 'Escape') {
      e.preventDefault();
      if (typing && query) query = '';
      else closeMenu();
    } else if (e.key === 'ArrowDown' || (!typing && e.key === 'j')) (e.preventDefault(), step(1));
    else if (e.key === 'ArrowUp' || (!typing && e.key === 'k')) (e.preventDefault(), step(-1));
    else if (e.key === 'Enter' || (!typing && e.key === ' ')) {
      e.preventDefault();
      const it = items[at];
      if (it && isItem(it)) run(it);
    } else if (e.key === 'Tab') (e.preventDefault(), closeMenu());
  }
</script>

{#if ui.menu}
  <!-- svelte-ignore a11y_no_static_element_interactions, a11y_click_events_have_key_events -->
  <!-- Above the sheets and dialogs (z-40/50): a field's menu in a dialog
       shows over it. -->
  <div class="fixed inset-0 z-[55]" onmousedown={(e) => (keepFocus(e), closeMenu())} oncontextmenu={(e) => (e.preventDefault(), closeMenu())}></div>
  <div
    bind:this={box}
    onmousedown={keepFocus}
    class="fixed z-[60] flex max-h-[80vh] min-w-[220px] flex-col border border-rule-strong bg-bg text-[13px] text-ink outline-none {ui.menu.search
      ? 'w-[min(360px,92vw)]'
      : 'max-w-[340px]'}"
    style="left: {pos.x}px; top: {pos.y}px"
    role="menu"
    tabindex="-1"
    aria-label={ui.menu.label}
    onkeydown={onKey}
  >
    {#if ui.menu.search}
      <div class="flex flex-none items-center gap-2 border-b border-rule px-3 py-1.5">
        <span class="min-w-0 truncate font-mono text-[11px] text-muted">{ui.menu.label}</span>
        <input
          bind:this={field}
          bind:value={query}
          class="min-w-0 flex-1 border-0 bg-transparent p-0 text-[13px] text-ink outline-none placeholder:text-faint"
          placeholder="Type to narrow"
          aria-label="Narrow the verbs"
          spellcheck="false"
          autocomplete="off"
        />
      </div>
    {/if}
    <div class="min-h-0 overflow-y-auto py-1">
      {#each items as entry, i (i)}
        {#if entry === 'sep'}
          <div class="my-1 h-px bg-rule" role="separator"></div>
        {:else if 'heading' in entry}
          <div class="px-3 pt-1.5 pb-0.5 font-mono text-[11px] text-muted">{entry.heading}</div>
        {:else}
          <button
            role="menuitem"
            data-i={i}
            disabled={entry.disabled}
            title={entry.title}
            class="flex w-full items-center gap-4 border-0 px-3 py-1 text-left {at === i ? 'bg-ink text-bg' : 'bg-transparent'} {entry.danger && at !== i
              ? 'text-fail'
              : ''} disabled:text-faint"
            onmouseenter={() => (at = entry.disabled ? -1 : i)}
            onclick={() => run(entry)}
          >
            <span class="min-w-0 flex-1 truncate">{entry.label}</span>
            {#if entry.keys?.length}
              <span class="flex-none font-mono text-[11px] {at === i ? 'text-bg' : 'text-muted'}">{keyLabel(entry.keys[0])}</span>
            {/if}
          </button>
        {/if}
      {/each}
    </div>
  </div>
{/if}
