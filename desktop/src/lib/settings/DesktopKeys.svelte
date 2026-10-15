<script lang="ts">
  // Settings, Hotkeys: this window's own keys, rebindable. Each row is one
  // of the shortcut table's (its default chords from the verb registry);
  // Add key takes the next chord pressed, a chord's x drops it, Reset puts
  // the defaults back. A chord another row already answers in an
  // overlapping place is held for a second press, which moves it here. Saved under
  // config.json's `keybindings`, beside the terminal app's.
  import { api } from '../api';
  import { toast } from '../store.svelte';
  import { SHORTCUTS, applyOverrides, chordOf, clashes, defaultKeys, isFixed, isModifierKey, keyLabel, type Shortcut, type ShortcutId } from '../shortcuts';
  import { matches } from './settings';

  let { filter = '' }: { filter?: string } = $props();

  const SCOPE_WORD: Record<string, string> = {
    anywhere: 'anywhere',
    fields: 'not in a terminal',
    global: 'not while typing',
    row: 'on a focused row',
    overview: 'overview, fixed',
    board: 'board, fixed',
  };

  /** The row taking a chord, and a chord held for a clash's confirm. */
  let capture = $state<{ id: ShortcutId; chord?: string; clash?: Shortcut[] } | null>(null);
  let busy = $state(false);

  const groups = $derived.by(() => {
    const out: { group: string; rows: Shortcut[] }[] = [];
    for (const s of SHORTCUTS) {
      if (!matches(filter, s.label, s.group, ...s.keys, ...s.keys.map(keyLabel))) continue;
      let g = out.find((x) => x.group === s.group);
      if (!g) out.push((g = { group: s.group, rows: [] }));
      g.rows.push(s);
    }
    return out;
  });

  const changed = (s: Shortcut) => s.keys.join(' ') !== defaultKeys(s.id).join(' ');

  async function save(id: ShortcutId, keys: string[] | null) {
    busy = true;
    try {
      applyOverrides(await api.desktopKeySet(id, keys));
    } catch (e) {
      toast(`Could not save the key: ${e}`);
    } finally {
      busy = false;
    }
  }

  async function take(s: Shortcut, chord: string) {
    if (s.keys.includes(chord)) return void (capture = null);
    const clash = clashes(s.id, chord);
    // Held for a second press; a home view's walking key stays the view's.
    const held = capture?.chord === chord && !clash.some((c) => isFixed(c.scope));
    if (clash.length && !held) return void (capture = { id: s.id, chord, clash });
    capture = null;
    // Confirmed: the chord moves here from the rows that had it.
    for (const c of clash) await save(c.id, c.keys.filter((x) => x !== chord));
    await save(s.id, [...s.keys, chord]);
  }

  function onKey(e: KeyboardEvent) {
    if (!capture) return;
    // Every key goes to the capture, none to the window's shortcuts.
    e.preventDefault();
    e.stopPropagation();
    if (e.key === 'Escape') return void (capture = null);
    if (isModifierKey(e.key)) return;
    const s = SHORTCUTS.find((x) => x.id === capture?.id);
    if (s) take(s, chordOf(e));
  }

  const btn = 'h-6 border border-rule bg-transparent px-2 text-[12px] text-ink enabled:hover:border-rule-strong disabled:text-faint';
</script>

<svelte:window onkeydowncapture={onKey} />

{#each groups as g (g.group)}
  <section class="pt-5" aria-label="This window: {g.group}">
    <h2 class="m-0 border-b border-rule-strong px-6 pb-1.5 font-mono text-[12px] font-semibold tracking-wide text-ink">{g.group}</h2>
    <ul class="m-0 list-none p-0">
      {#each g.rows as s (s.id)}
        {@const fixed = isFixed(s.scope)}
        {@const taking = capture?.id === s.id}
        <li class="grid grid-cols-[minmax(0,1fr)_auto] items-center gap-x-6 gap-y-1 border-b border-rule px-6 py-2 max-[720px]:grid-cols-1">
          <div class="min-w-0">
            <span class="text-[14px] text-ink">{s.label}</span>
            {#if changed(s)}<span class="ml-2 font-mono text-[11px] text-muted">changed</span>{/if}
            <p class="m-0 truncate font-mono text-[11px] text-muted">{SCOPE_WORD[s.scope] ?? s.scope}</p>
          </div>
          <div class="flex flex-wrap items-center justify-end gap-1 max-[720px]:justify-start">
            {#each s.keys as k (k)}
              <span class="flex items-center border border-rule">
                <kbd class="px-1.5 font-mono text-[11px] leading-5 text-ink">{keyLabel(k)}</kbd>
                {#if !fixed}
                  <button
                    class="h-5 border-0 border-l border-rule bg-transparent px-1 font-mono text-[11px] text-muted enabled:hover:text-ink"
                    disabled={busy}
                    title="Unbind {keyLabel(k)} from {s.label}"
                    aria-label="Unbind {keyLabel(k)} from {s.label}"
                    onclick={() => save(s.id, s.keys.filter((x) => x !== k))}>&times;</button
                  >
                {/if}
              </span>
            {:else}
              <span class="font-mono text-[11px] text-muted">unbound</span>
            {/each}
            {#if !fixed}
              <button
                class="{btn} {taking ? 'border-ink' : ''}"
                disabled={busy}
                aria-pressed={taking}
                title={taking ? 'Press the chord to add; Esc cancels' : `Add a key to ${s.label}: press it next`}
                onclick={() => (capture = taking ? null : { id: s.id })}>{#if taking}Press a key&hellip;{:else}Add key{/if}</button
              >
              {#if changed(s)}
                <button class={btn} disabled={busy} title="Put {s.label} back to {defaultKeys(s.id).map(keyLabel).join(' or ') || 'unbound'}" onclick={() => save(s.id, null)}
                  >Reset</button
                >
              {/if}
            {/if}
          </div>
          {#if taking && capture?.clash?.length}
            <p class="col-span-full m-0 flex items-center gap-2 text-[12px] text-ink" role="alert">
              <span class="size-1.5 flex-none bg-fail" aria-hidden="true"></span>
              <span class="min-w-0">
                {keyLabel(capture.chord ?? '')} already runs {capture.clash.map((c) => c.label).join(', ')}.
                {#if capture.clash.some((c) => isFixed(c.scope))}The view keeps it: press another, or Esc.{:else}Press it again to move it here, or Esc.{/if}
              </span>
            </p>
          {/if}
        </li>
      {/each}
    </ul>
  </section>
{/each}
