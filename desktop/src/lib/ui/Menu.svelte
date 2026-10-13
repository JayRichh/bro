<script lang="ts">
  // The context menu: square, ruled, ink for the row under the keys.
  // Arrows walk it, Enter runs, Esc or a click outside closes.
  import { tick } from 'svelte';
  import { closeMenu, ui, type MenuEntry, type MenuItem } from './ui.svelte';
  import { keyLabel } from '../shortcuts';

  let box = $state<HTMLDivElement>();
  let at = $state(-1);
  let pos = $state({ x: 0, y: 0 });

  const items = $derived(ui.menu?.items ?? []);
  const actionable = $derived(items.map((e, i) => (isItem(e) && !e.disabled ? i : -1)).filter((i) => i >= 0));

  function isItem(e: MenuEntry): e is MenuItem {
    return typeof e === 'object' && 'run' in e;
  }

  // Place inside the window and take the keys when it opens.
  $effect(() => {
    const m = ui.menu;
    if (!m) return;
    at = -1;
    pos = { x: m.x, y: m.y };
    tick().then(() => {
      if (!box) return;
      const r = box.getBoundingClientRect();
      pos = {
        x: Math.max(4, Math.min(m.x, window.innerWidth - r.width - 4)),
        y: Math.max(4, Math.min(m.y, window.innerHeight - r.height - 4)),
      };
      box.focus();
    });
  });

  function run(e: MenuItem) {
    if (e.disabled) return;
    closeMenu();
    e.run();
  }

  function step(d: number) {
    if (!actionable.length) return;
    const i = actionable.indexOf(at);
    at = actionable[(i < 0 ? (d > 0 ? 0 : actionable.length - 1) : (i + d + actionable.length) % actionable.length)];
  }

  function onKey(e: KeyboardEvent) {
    e.stopPropagation();
    if (e.key === 'Escape') (e.preventDefault(), closeMenu());
    else if (e.key === 'ArrowDown' || e.key === 'j') (e.preventDefault(), step(1));
    else if (e.key === 'ArrowUp' || e.key === 'k') (e.preventDefault(), step(-1));
    else if (e.key === 'Enter' || e.key === ' ') {
      e.preventDefault();
      const it = items[at];
      if (it && isItem(it)) run(it);
    } else if (e.key === 'Tab') (e.preventDefault(), closeMenu());
  }
</script>

{#if ui.menu}
  <!-- svelte-ignore a11y_no_static_element_interactions, a11y_click_events_have_key_events -->
  <div class="fixed inset-0 z-40" onmousedown={closeMenu} oncontextmenu={(e) => (e.preventDefault(), closeMenu())}></div>
  <div
    bind:this={box}
    class="fixed z-50 min-w-[220px] max-w-[340px] border border-rule-strong bg-bg py-1 text-[13px] text-ink outline-none"
    style="left: {pos.x}px; top: {pos.y}px"
    role="menu"
    tabindex="-1"
    aria-label={ui.menu.label}
    onkeydown={onKey}
  >
    {#each items as entry, i (i)}
      {#if entry === 'sep'}
        <div class="my-1 h-px bg-rule" role="separator"></div>
      {:else if 'heading' in entry}
        <div class="px-3 pt-1.5 pb-0.5 font-mono text-[11px] text-muted">{entry.heading}</div>
      {:else}
        <button
          role="menuitem"
          disabled={entry.disabled}
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
{/if}
