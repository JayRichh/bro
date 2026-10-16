<script lang="ts">
  // The context menu: square, ruled, ink for the row under the keys.
  // Arrows walk it (j and k too), Home and End jump, Enter or Space runs,
  // Esc or a click outside closes. A row with \u25b8 opens its submenu: on
  // hover after a moment (a diagonal move into an open one keeps it), on
  // \u2192 / Enter / Space or a click; \u2190 or Esc closes one level. Typing narrows
  // any menu, a submenu's rows too (`Open in \u203a VS Code`); a searchable one
  // (the verbs menu, `?`) has a field for it. What a key does, what typing
  // leaves and where a submenu goes are menus/model.ts's.
  import { tick, untrack } from 'svelte';
  import { closeMenu, ui, type MenuEntry, type MenuIcon, type MenuItem } from './ui.svelte';
  import { keyLabel } from '../shortcuts';
  import { fuzzyMatch } from '../fuzzy';
  import {
    aimsAt,
    closeSub,
    first,
    hasChildren,
    isItem,
    NAV0,
    narrow,
    navKey,
    navKeyOf,
    openSub,
    placeSub,
    typedChar,
    type Nav,
  } from '../menus/model';

  /** How long the pointer rests on a \u25b8 row before its submenu opens, and
   *  on its way across other rows before an open one gives way. */
  const HOVER_MS = 150;

  let box = $state<HTMLDivElement>();
  let subBox = $state<HTMLDivElement>();
  let field = $state<HTMLInputElement>();
  let nav = $state<Nav>({ ...NAV0 });
  let pos = $state({ x: 0, y: 0 });
  let subPos = $state({ x: 0, y: 0, left: false, shown: false });
  let query = $state('');

  const search = $derived(!!ui.menu?.search);
  const items = $derived(narrow(ui.menu?.items ?? [], query, (q, s) => fuzzyMatch(q, s)?.score ?? null));
  const openAt = $derived(nav.open);
  const sub = $derived(openAt >= 0 && hasChildren(items[openAt]) ? (items[openAt] as MenuItem).children! : null);

  // Place inside the window and take the keys when it opens.
  $effect(() => {
    const m = ui.menu;
    if (!m) return;
    nav = { ...NAV0 };
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
        nav = { ...NAV0, at: first(items) };
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

  // The rows under the keys stay in view in a long list.
  $effect(() => {
    const i = nav.at;
    if (i >= 0) box?.querySelector(`[data-i="${i}"]`)?.scrollIntoView({ block: 'nearest' });
  });
  $effect(() => {
    const i = nav.subAt;
    if (i >= 0) subBox?.querySelector(`[data-i="${i}"]`)?.scrollIntoView({ block: 'nearest' });
  });

  // A query lands on its best row; cleared, the keys start over.
  $effect(() => {
    const q = query.trim();
    untrack(() => (nav = { ...NAV0, at: q || search ? first(items) : -1 }));
  });

  // A submenu opens beside its row, flipped where the window ends; the
  // keys go to it when it was opened by them.
  $effect(() => {
    const open = openAt;
    const kids = sub;
    subPos = { x: 0, y: 0, left: false, shown: false };
    if (open < 0 || !kids) return;
    tick().then(() => {
      const row = box?.querySelector(`[data-i="${open}"]`);
      if (!row || !box || !subBox || nav.open !== open) return;
      const s = subBox.getBoundingClientRect();
      const p = placeSub(row.getBoundingClientRect(), box.getBoundingClientRect(), s, { width: window.innerWidth, height: window.innerHeight });
      subPos = { ...p, shown: true };
      if (nav.inSub && !search && !ui.menu?.keep) subBox.focus();
    });
  });

  function run(e: MenuItem) {
    if (e.disabled || e.children?.length) return;
    closeMenu();
    e.run();
  }

  // ---- the pointer ---------------------------------------------------------

  let timer: ReturnType<typeof setTimeout> | undefined;
  let last = { x: 0, y: 0 };
  let here = { x: 0, y: 0 };
  function onMove(e: MouseEvent) {
    last = here;
    here = { x: e.clientX, y: e.clientY };
  }

  /** The pointer on top row `i`: it takes the row, and a \u25b8 row opens after
   *  a moment. On its way into an open submenu across other rows, the
   *  submenu stays a moment longer. */
  function hoverTop(i: number) {
    clearTimeout(timer);
    const r = items[i];
    if (!isItem(r) || r.disabled) return;
    if (nav.open >= 0 && nav.open !== i && subBox && aimsAt(last, here, subBox.getBoundingClientRect(), subPos.left)) {
      timer = setTimeout(() => hoverTop(i), HOVER_MS);
      return;
    }
    if (nav.open !== i) nav = { ...NAV0, at: i };
    if (hasChildren(r) && nav.open !== i) timer = setTimeout(() => nav.at === i && (nav = { ...openSub(nav, items, i), inSub: false }), HOVER_MS);
  }

  function hoverSub(j: number, c: MenuEntry) {
    clearTimeout(timer);
    if (isItem(c) && !c.disabled) nav = { ...nav, inSub: true, subAt: j };
  }

  /** A click on a \u25b8 row opens it, keys and all; on any other row, runs it. */
  function clickTop(i: number) {
    clearTimeout(timer);
    const r = items[i];
    if (!isItem(r)) return;
    if (hasChildren(r)) nav = nav.open === i ? closeSub(nav) : openSub({ ...nav, at: i }, items, i);
    else run(r);
  }

  // ---- the keys ------------------------------------------------------------

  function onKey(e: KeyboardEvent) {
    e.stopPropagation();
    // In a search field the letters are the query's; elsewhere typing
    // narrows too, once something is typed (j, k and Space walk and run
    // until then).
    if (!search) {
      if (e.key === 'Backspace' && query) return void (e.preventDefault(), (query = query.slice(0, -1)));
      const ch = typedChar(e, query);
      if (ch) return void (e.preventDefault(), (query += ch));
    }
    if (e.key === 'Escape' && query) return void (e.preventDefault(), (query = ''));
    const k = navKeyOf(e.key, !search && !query);
    if (!k) return;
    // The field keeps its caret keys while it holds a query (the rows
    // are a flat list then); Space is a letter there.
    if (search && (k === 'space' || ((k === 'left' || k === 'right') && query))) return;
    e.preventDefault();
    clearTimeout(timer);
    const out = navKey(nav, k, items);
    if (out.close) return closeMenu();
    nav = out.nav;
    if (out.run) run(out.run);
    else if (!nav.inSub && !search && !ui.menu?.keep) box?.focus();
  }

  // ---- drawing -------------------------------------------------------------

  const DOT: Record<string, string> = {
    ember: 'bg-ember',
    ink: 'bg-ink',
    muted: 'bg-muted',
    faint: 'bg-faint',
    done: 'bg-done',
    fail: 'bg-fail',
    merged: 'bg-merged',
    unread: 'bg-unread',
  };

  /** Whether a list draws the icon slot: some row has an icon or a tick. */
  const slotted = (rows: readonly MenuEntry[]) => rows.some((e) => isItem(e) && (e.icon || e.checked !== undefined));
  const topSlot = $derived(slotted(items));
  const subSlot = $derived(sub ? slotted(sub) : false);
  const roleOf = (e: MenuItem) => (e.checked === undefined ? 'menuitem' : e.radio ? 'menuitemradio' : 'menuitemcheckbox');
</script>

{#snippet slot(e: MenuItem, active: boolean)}
  <span class="grid size-4 flex-none place-items-center" aria-hidden="true">
    {#if e.checked}
      {#if e.radio}
        <span class="size-1.5 {active ? 'bg-bg' : 'bg-ink'}"></span>
      {:else}
        <svg width="11" height="11" viewBox="0 0 12 12"><path d="M2 6.5l2.6 2.6L10 3.5" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="square" /></svg>
      {/if}
    {:else if e.icon}
      {@const ic = e.icon as MenuIcon}
      {#if 'dot' in ic}
        <span class="size-1.5 {DOT[ic.dot]}"></span>
      {:else if 'src' in ic}
        <img src={ic.src} alt="" width="14" height="14" draggable="false" />
      {:else}
        <span class="grid size-[14px] place-items-center border font-mono text-[9px] leading-none font-semibold {active ? 'border-bg' : 'border-rule-strong'}"
          >{ic.glyph}</span
        >
      {/if}
    {/if}
  </span>
{/snippet}

{#snippet rows(list: readonly MenuEntry[], level: 0 | 1, active: number, hasSlot: boolean)}
  {#each list as entry, i (i)}
    {#if entry === 'sep'}
      <div class="my-1 h-px bg-rule" role="separator"></div>
    {:else if 'heading' in entry}
      <div
        class="px-3 pt-1.5 pb-0.5 text-[11px] tracking-[0.04em] [font-variant-caps:all-small-caps] {entry.ember ? 'text-ember-ink' : 'text-faint'}"
        role="presentation"
      >
        {entry.heading}
      </div>
    {:else}
      {@const on = active === i}
      {@const opens = level === 0 && !!entry.children?.length}
      <button
        id="kete-menu-{level}-{i}"
        role={roleOf(entry)}
        data-i={i}
        tabindex="-1"
        aria-disabled={entry.disabled || undefined}
        aria-checked={entry.checked === undefined ? undefined : entry.checked}
        aria-haspopup={opens ? 'menu' : undefined}
        aria-expanded={opens ? nav.open === i : undefined}
        title={entry.title}
        class="flex w-full items-center gap-2 border-0 py-1 text-left {hasSlot ? 'pr-3 pl-1.5' : 'px-3'} {on
          ? 'bg-ink text-bg'
          : opens && nav.open === i
            ? 'bg-surface'
            : 'bg-transparent'} {entry.disabled ? (on ? '' : 'text-faint') : entry.danger && !on ? 'text-fail' : ''}"
        onmouseenter={() => (level === 0 ? hoverTop(i) : hoverSub(i, entry))}
        onclick={() => (level === 0 ? clickTop(i) : run(entry))}
      >
        {#if hasSlot}{@render slot(entry, on)}{/if}
        <span class="min-w-0 flex-1 truncate" data-label>{entry.label}</span>
        {#if entry.keys?.length}
          <span class="flex flex-none items-center gap-2 font-mono text-[11px] {on ? 'text-bg' : 'text-muted'}">
            {#each entry.keys as k (k)}<span>{keyLabel(k)}</span>{/each}
          </span>
        {/if}
        {#if opens}
          <svg class="flex-none" width="8" height="8" viewBox="0 0 8 8" aria-hidden="true"><path d="M2.5 1L6 4 2.5 7" fill="none" stroke="currentColor" stroke-width="1.3" /></svg>
        {/if}
      </button>
    {/if}
  {/each}
{/snippet}

{#if ui.menu}
  <!-- svelte-ignore a11y_no_static_element_interactions, a11y_click_events_have_key_events -->
  <!-- Above the sheets and dialogs (z-40/50): a field's menu in a dialog
       shows over it. A right-click on the scrim reopens the menu for what
       is under it (menus/router.ts). -->
  <div class="fixed inset-0 z-[55]" data-menu-scrim onmousedown={(e) => (keepFocus(e), closeMenu())} oncontextmenu={(e) => e.preventDefault()}></div>
  <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
  <div
    bind:this={box}
    onmousedown={keepFocus}
    onmousemove={onMove}
    class="fixed z-[60] flex max-h-[80vh] min-w-[220px] flex-col border border-rule-strong bg-bg text-[13px] text-ink outline-none {search
      ? 'w-[min(360px,92vw)]'
      : 'max-w-[340px]'}"
    style="left: {pos.x}px; top: {pos.y}px"
    role="menu"
    tabindex="-1"
    aria-label={ui.menu.label}
    aria-activedescendant={!nav.inSub && nav.at >= 0 ? `kete-menu-0-${nav.at}` : undefined}
    onkeydown={onKey}
  >
    {#if search}
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
    {:else if query}
      <!-- What typing has narrowed the rows to; Backspace and Esc undo it. -->
      <div class="flex flex-none items-center gap-2 border-b border-rule px-3 py-1 font-mono text-[11px]" role="status">
        <span class="text-faint">narrow</span><span class="min-w-0 truncate text-ink">{query}</span>
      </div>
    {/if}
    <div class="min-h-0 overflow-y-auto py-1">
      {#if query && !items.some(isItem)}
        <p class="m-0 px-3 py-1 text-faint">Nothing matches</p>
      {/if}
      {@render rows(items, 0, nav.inSub ? -1 : nav.at, topSlot)}
    </div>
  </div>
  {#if sub}
    <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
    <div
      bind:this={subBox}
      onmousedown={keepFocus}
      class="fixed z-[61] flex max-h-[80vh] min-w-[200px] max-w-[340px] flex-col border border-rule-strong bg-bg text-[13px] text-ink outline-none"
      style="left: {subPos.x}px; top: {subPos.y}px; visibility: {subPos.shown ? 'visible' : 'hidden'}"
      role="menu"
      tabindex="-1"
      aria-label={isItem(items[nav.open]) ? (items[nav.open] as MenuItem).label : undefined}
      aria-activedescendant={nav.inSub && nav.subAt >= 0 ? `kete-menu-1-${nav.subAt}` : undefined}
      onkeydown={onKey}
    >
      <div class="min-h-0 overflow-y-auto py-1">
        {@render rows(sub, 1, nav.inSub ? nav.subAt : -1, subSlot)}
      </div>
    </div>
  {/if}
{/if}
