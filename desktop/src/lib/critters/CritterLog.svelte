<script lang="ts">
  // CRITTER LOG: every species that can visit, grouped by where it comes
  // from. One that has visited shows its idle sprite and its tally; the
  // rest are silhouettes and "???". Read from critters.json each time it
  // opens (sheet.svelte.ts); the terminal app fills the same file.
  import { tick } from 'svelte';
  import { decodeFrame, FLAGS, ORIGIN_LABELS, ORIGINS, SPECIES, type SpeciesRow } from './cast.gen';
  import { closeCritterLog, logSheet } from './sheet.svelte';

  /** CSS px per sprite px, the overlay's "medium". */
  const SCALE = 3;
  // Species that only arrive from another critter never get a Seen note.
  const LISTED = SPECIES.filter((s) => !(s.flags & FLAGS.noSpawn));
  // Every sprite stands on the same line, so the names line up under them.
  const ART_H = Math.max(...LISTED.map((s) => s.sprite.h)) * SCALE;

  let box = $state<HTMLDivElement>();
  $effect(() => {
    if (logSheet.open) tick().then(() => box?.focus());
  });

  function onKey(e: KeyboardEvent) {
    e.stopPropagation();
    if (e.key === 'Escape') (e.preventDefault(), closeCritterLog());
  }

  const groups = $derived(
    ORIGINS.map((origin) => {
      const cells = LISTED.filter((s) => s.origin === origin).map((sp) => {
        const e = logSheet.log.species[sp.id];
        return { sp, e, found: (e?.seen ?? 0) > 0 };
      });
      return { origin, label: ORIGIN_LABELS[origin], cells, found: cells.filter((c) => c.found).length };
    }).filter((g) => g.cells.length > 0),
  );
  const found = $derived(groups.reduce((n, g) => n + g.found, 0));
  const lamps = $derived(logSheet.log.lamps);

  /** The unseen silhouettes' colour: the theme's faint ink, read when drawn
   *  so dark mode follows. */
  function faintInk(): string {
    return getComputedStyle(document.documentElement).getPropertyValue('--color-faint').trim() || '#8a8178';
  }

  /** Draw the species' idle frame 0 into the canvas: its own colours if it
   *  has been found, else one flat ink. Each sprite pixel is a whole number
   *  of device pixels, so the edges stay hard (README, "Units"). */
  function paint(sp: SpeciesRow, isFound: boolean) {
    return (canvas: HTMLCanvasElement) => {
      const { w, h, palette, frames } = sp.sprite;
      const dpr = window.devicePixelRatio || 1;
      const k = Math.max(1, Math.round(SCALE * dpr));
      canvas.width = w * k;
      canvas.height = h * k;
      canvas.style.width = `${(w * k) / dpr}px`;
      canvas.style.height = `${(h * k) / dpr}px`;
      const ctx = canvas.getContext('2d');
      if (!ctx) return;
      ctx.imageSmoothingEnabled = false;
      const flat = isFound ? null : faintInk();
      if (isFound && sp.flags & FLAGS.ghostly) ctx.globalAlpha = 0.8;
      const px = decodeFrame(frames.idle[0]);
      for (let i = 0; i < px.length; i++) {
        const v = px[i];
        if (v === 0) continue;
        ctx.fillStyle = flat ?? palette[v - 1];
        ctx.fillRect((i % w) * k, Math.floor(i / w) * k, k, k);
      }
    };
  }
</script>

{#if logSheet.open}
  <!-- svelte-ignore a11y_no_static_element_interactions, a11y_click_events_have_key_events -->
  <div class="fixed inset-0 z-40 bg-[rgb(28_25_23/0.18)]" onmousedown={closeCritterLog}></div>
  <div
    bind:this={box}
    class="fixed top-[8%] left-1/2 z-50 flex max-h-[84vh] w-[min(760px,94vw)] -translate-x-1/2 flex-col border border-rule-strong bg-bg text-ink outline-none"
    role="dialog"
    aria-modal="true"
    aria-labelledby="critter-log-title"
    tabindex="-1"
    onkeydown={onKey}
  >
    <header class="flex flex-none items-baseline gap-4 border-b border-rule-strong px-5 py-3">
      <h2 id="critter-log-title" class="m-0 text-[17px] font-semibold">Critter log</h2>
      <span class="font-mono text-[11px] text-muted"
        >{#if logSheet.loaded}{found} of {LISTED.length} found{#if lamps > 0} &middot; {lamps} {lamps === 1 ? 'lamp' : 'lamps'}{/if}{/if}</span
      >
      <span class="flex-1"></span>
      <button
        class="flex-none border border-rule bg-transparent px-1.5 font-mono text-[11px] leading-5 text-muted hover:border-rule-strong hover:text-ink"
        title="Close (Esc)"
        aria-label="Close Critter log"
        onclick={closeCritterLog}>Esc</button
      >
    </header>
    <div class="min-h-0 overflow-y-auto px-5 pb-5">
      {#if logSheet.loaded}
        {#if found === 0}
          <p class="m-0 pt-4 text-[13px] text-muted">No visitors yet: they turn up along the bottom of the window now and then.</p>
        {/if}
        {#each groups as g (g.origin)}
          <section class="pt-4" aria-label={g.label}>
            <h3 class="m-0 border-b border-rule-strong pb-1 text-[14px] font-semibold">
              {g.label}
              <span class="ml-1 font-mono text-[11px] font-normal text-muted">{g.found} of {g.cells.length}</span>
            </h3>
            <ul class="m-0 grid list-none grid-cols-[repeat(auto-fill,minmax(120px,1fr))] border-l border-rule p-0">
              {#each g.cells as c (c.sp.id)}
                <li
                  class="flex min-w-0 flex-col gap-1 border-r border-b border-rule px-2.5 py-2 break-words {c.found
                    ? 'bg-bg text-ink'
                    : 'bg-surface text-faint'}"
                >
                  <div class="flex items-end" style:height="{ART_H}px">
                    <canvas class="block [image-rendering:pixelated]" aria-hidden="true" {@attach paint(c.sp, c.found)}></canvas>
                  </div>
                  {#if c.found && c.e}
                    <div class="text-[13px] font-semibold">{c.sp.name}</div>
                    <div class="font-mono text-[11px] leading-4 text-muted">
                      <div>seen {c.e.seen}</div>
                      {#if c.e.pets > 0}<div>petted {c.e.pets}</div>{/if}
                      {#if c.e.fed > 0}<div>fed {c.e.fed}</div>{/if}
                      {#if c.e.name}<div class="text-ink">"{c.e.name}"</div>{/if}
                      {#if c.e.first_seen}<div>first seen <span class="whitespace-nowrap">{c.e.first_seen}</span></div>{/if}
                    </div>
                  {:else}
                    <div class="text-[13px] font-semibold" aria-hidden="true">???</div>
                    <span class="sr-only">Not found yet</span>
                  {/if}
                </li>
              {/each}
            </ul>
          </section>
        {/each}
      {/if}
    </div>
  </div>
{/if}
