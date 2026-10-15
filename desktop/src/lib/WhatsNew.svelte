<script lang="ts">
  // WHAT'S NEW: the last few releases in a few lines each, and the keys
  // that changed. Shown once after an upgrade (src-tauri learn.rs records
  // it), and from Help's header or the palette after.
  import { tick } from 'svelte';
  import { KEYS_CHANGED, WHATS_NEW } from './actions.gen';
  import { learn } from './learn.svelte';
  import { ui } from './ui/ui.svelte';
  import { keyLabel, keysOf } from './shortcuts';

  let box = $state<HTMLDivElement>();
  $effect(() => {
    if (learn.whatsNew) tick().then(() => box?.focus());
  });

  function close() {
    learn.whatsNew = false;
  }
  function allKeys() {
    learn.whatsNew = false;
    ui.help = true;
  }
  function onKey(e: KeyboardEvent) {
    e.stopPropagation();
    if (e.key === 'Escape') (e.preventDefault(), close());
    else if (e.key === 'F1') (e.preventDefault(), allKeys());
  }
  const help = $derived(keysOf('help_f1')[0]);
  const span = $derived(
    WHATS_NEW.length > 1 ? `${WHATS_NEW[WHATS_NEW.length - 1].version} to ${WHATS_NEW[0].version}` : (WHATS_NEW[0]?.version ?? ''),
  );
</script>

{#if learn.whatsNew}
  <!-- svelte-ignore a11y_no_static_element_interactions, a11y_click_events_have_key_events -->
  <div class="fixed inset-0 z-40 bg-[rgb(28_25_23/0.18)]" onmousedown={close}></div>
  <div
    bind:this={box}
    class="fixed top-[8%] left-1/2 z-50 flex max-h-[84vh] w-[min(760px,94vw)] -translate-x-1/2 flex-col border border-rule-strong bg-bg text-ink outline-none"
    role="dialog"
    aria-modal="true"
    aria-labelledby="whats-new-title"
    tabindex="-1"
    onkeydown={onKey}
  >
    <header class="flex flex-none items-baseline gap-4 border-b border-rule-strong px-5 py-3">
      <h2 id="whats-new-title" class="m-0 text-[17px] font-semibold">What's new in {span}</h2>
      <span class="flex-1"></span>
      <button
        class="flex-none border border-rule bg-transparent px-1.5 font-mono text-[11px] leading-5 text-muted hover:border-rule-strong hover:text-ink"
        title="Close (Esc)"
        aria-label="Close What's new"
        onclick={close}>Esc</button
      >
    </header>
    <div class="min-h-0 overflow-y-auto px-5 pb-5">
      {#each WHATS_NEW as r (r.version)}
        <section class="pt-4" aria-label={r.version}>
          <h3 class="m-0 flex items-baseline gap-3 border-b border-rule-strong pb-1 text-[14px] font-semibold">
            <span class="font-mono text-[12px] text-ember-ink">{r.version}</span>{r.title}
          </h3>
          <ul class="m-0 list-none p-0">
            {#each r.items as item (item)}
              <li class="border-b border-rule py-1.5 text-[13px]">{item}</li>
            {/each}
          </ul>
        </section>
      {/each}
      <section class="pt-4" aria-label="Keys changed">
        <h3 class="m-0 border-b border-rule-strong pb-1 text-[14px] font-semibold">
          Keys changed in 0.49 <span class="ml-1 font-mono text-[11px] font-normal text-muted">keys you rebound are kept</span>
        </h3>
        <table class="w-full border-collapse text-[13px]">
          <thead>
            <tr class="text-left font-mono text-[11px] text-muted">
              <th class="border-b border-rule py-1 pr-3 font-normal">Verb</th>
              <th class="border-b border-rule py-1 pr-3 font-normal">Terminal app</th>
              <th class="border-b border-rule py-1 font-normal">Desktop</th>
            </tr>
          </thead>
          <tbody>
            {#each KEYS_CHANGED as k (k.verb)}
              <tr>
                <td class="border-b border-rule py-1.5 pr-3">{k.verb}</td>
                <td class="border-b border-rule py-1.5 pr-3 font-mono text-[12px]">{k.tui}</td>
                <td class="border-b border-rule py-1.5 font-mono text-[12px]">{k.desktop}</td>
              </tr>
            {/each}
          </tbody>
        </table>
      </section>
      <p class="m-0 pt-4 text-[13px] text-muted">
        Every key:
        <button
          class="border border-rule bg-transparent px-1.5 text-[12px] text-ink hover:border-rule-strong"
          title="Keyboard shortcuts{help ? ` (${keyLabel(help)})` : ''}"
          onclick={allKeys}>Keyboard shortcuts{#if help}<kbd class="ml-1.5 font-mono text-[11px] text-faint">{keyLabel(help)}</kbd>{/if}</button
        >
      </p>
    </div>
  </div>
{/if}
