<script lang="ts">
  // The in-window dialog: a prompt (one or more fields), a confirm, or a
  // pick list. Square and ruled; Enter confirms (Ctrl+Enter in a
  // multi-line field), Esc cancels.
  import { tick } from 'svelte';
  import { ui } from './ui.svelte';
  import { rank } from '../fuzzy';
  import { firstFocus } from './focus';

  let values = $state<Record<string, string>>({});
  let checked = $state(false);
  let query = $state('');
  let at = $state(0);
  let box = $state<HTMLDivElement>();

  const d = $derived(ui.dialog);
  const picks = $derived(d?.kind === 'pick' ? rank(query, d.items, (i) => `${i.label} ${i.hint ?? ''}`) : []);

  $effect(() => {
    const spec = ui.dialog;
    if (!spec) return;
    values = spec.kind === 'prompt' ? Object.fromEntries(spec.fields.map((f) => [f.key, f.value ?? ''])) : {};
    checked = spec.kind === 'confirm' && !!spec.checked;
    query = '';
    at = 0;
    tick().then(() => {
      const first = box ? firstFocus(box) : null;
      first?.focus();
      if (first instanceof HTMLInputElement) first.select();
    });
  });

  function cancel() {
    const spec = ui.dialog;
    if (!spec) return;
    if (spec.kind === 'prompt') spec.resolve(null);
    else if (spec.kind === 'confirm') spec.resolve({ ok: false, checked: false });
    else spec.resolve(null);
  }

  function confirm() {
    const spec = ui.dialog;
    if (!spec) return;
    if (spec.kind === 'prompt') spec.resolve({ ...values });
    else if (spec.kind === 'confirm') spec.resolve({ ok: true, checked });
    else if (picks[at]) spec.resolve(picks[at].item.id);
  }

  function onKey(e: KeyboardEvent) {
    e.stopPropagation();
    if (e.key === 'Escape') return (e.preventDefault(), cancel());
    // Tab stays in the dialog: it is modal, and the page behind it would
    // take keys meant for these fields.
    if (e.key === 'Tab' && box) {
      const all = [...box.querySelectorAll<HTMLElement>('input, textarea, button:not([disabled])')];
      const i = all.indexOf(document.activeElement as HTMLElement);
      if (e.shiftKey && i <= 0) (e.preventDefault(), all[all.length - 1]?.focus());
      else if (!e.shiftKey && i === all.length - 1) (e.preventDefault(), all[0]?.focus());
      return;
    }
    const multi = (e.target as HTMLElement).tagName === 'TEXTAREA';
    if (e.key === 'Enter' && (!multi || e.ctrlKey)) return (e.preventDefault(), confirm());
    if (d?.kind === 'pick' && (e.key === 'ArrowDown' || e.key === 'ArrowUp')) {
      e.preventDefault();
      at = Math.max(0, Math.min(picks.length - 1, at + (e.key === 'ArrowDown' ? 1 : -1)));
    }
  }

  const btn = 'h-7 flex-none border px-3 text-[13px]';
  const field =
    'w-full border border-rule bg-bg px-2 py-1 text-[13px] text-ink outline-none placeholder:text-faint hover:border-rule-strong focus:border-rule-strong';
</script>

{#if d}
  <!-- svelte-ignore a11y_no_static_element_interactions, a11y_click_events_have_key_events -->
  <div class="fixed inset-0 z-40 bg-[rgb(28_25_23/0.18)]" onmousedown={cancel}></div>
  <div
    bind:this={box}
    class="fixed top-[18%] left-1/2 z-50 w-[min(520px,92vw)] -translate-x-1/2 border border-rule-strong bg-bg text-ink"
    role="dialog"
    aria-modal="true"
    aria-labelledby="dialog-title"
    tabindex="-1"
    onkeydown={onKey}
  >
    <h2 id="dialog-title" class="m-0 border-b border-rule px-4 py-2.5 text-[15px] font-semibold">{d.title}</h2>
    <div class="flex flex-col gap-3 px-4 py-3">
      {#if d.kind === 'prompt'}
        {#if d.note}<p class="m-0 text-[13px] text-muted">{d.note}</p>{/if}
        {#each d.fields as f (f.key)}
          <label class="flex flex-col gap-1">
            <span class="text-[12px] font-medium text-muted">{f.label}</span>
            {#if f.multiline}
              <textarea class="{field} min-h-24 resize-y {f.mono ? 'font-mono text-[12px]' : ''}" placeholder={f.placeholder} spellcheck="false" bind:value={values[f.key]}
              ></textarea>
            {:else}
              <input class="{field} {f.mono ? 'font-mono text-[12px]' : ''}" placeholder={f.placeholder} spellcheck="false" bind:value={values[f.key]} />
            {/if}
          </label>
        {/each}
      {:else if d.kind === 'confirm'}
        <p class="m-0 text-[14px]">{d.message}</p>
        {#if d.detail?.length}
          <ul class="m-0 max-h-40 list-none overflow-y-auto border border-rule p-0">
            {#each d.detail as line, i (i)}<li class="border-b border-rule px-2 py-1 font-mono text-[12px] text-muted last:border-b-0">{line}</li>{/each}
          </ul>
        {/if}
        {#if d.check}
          <label class="flex items-center gap-2 text-[13px] text-ink">
            <input type="checkbox" class="size-3.5 accent-[var(--color-ink)]" bind:checked />
            {d.check}
          </label>
        {/if}
      {:else}
        <input class="{field} font-mono text-[12px]" placeholder="Filter" spellcheck="false" bind:value={query} oninput={() => (at = 0)} />
        <ul class="m-0 max-h-72 list-none overflow-y-auto border border-rule p-0" role="listbox" aria-label={d.title}>
          {#each picks as p, i (p.item.id)}
            <li role="option" aria-selected={i === at}>
              <button
                class="flex w-full items-baseline gap-3 border-0 border-b border-rule px-2 py-1.5 text-left last:border-b-0 {i === at ? 'bg-ink text-bg' : 'bg-transparent text-ink hover:bg-surface'}"
                onmouseenter={() => (at = i)}
                onclick={() => d.kind === 'pick' && d.resolve(p.item.id)}
              >
                <span class="min-w-0 flex-1 truncate text-[13px]">{p.item.label}</span>
                {#if p.item.hint}<span class="flex-none font-mono text-[11px] {i === at ? 'text-bg' : 'text-muted'}">{p.item.hint}</span>{/if}
              </button>
            </li>
          {:else}
            <li class="px-2 py-1.5 text-[13px] text-muted">Nothing matches.</li>
          {/each}
        </ul>
      {/if}
    </div>
    <div class="flex items-center justify-end gap-2 border-t border-rule px-4 py-2.5">
      {#if d.kind === 'prompt' && d.fields.some((f) => f.multiline)}
        <span class="mr-auto font-mono text-[11px] text-muted">Ctrl+Enter to send</span>
      {/if}
      <button class="{btn} border-rule bg-transparent text-ink hover:border-rule-strong" onclick={cancel}>Cancel</button>
      {#if d.kind !== 'pick'}
        <button
          data-autofocus
          class="{btn} border-transparent font-medium {d.kind === 'confirm' && d.danger ? 'bg-fail text-bg' : 'bg-ink text-bg'}"
          onclick={confirm}>{d.confirm}</button
        >
      {/if}
    </div>
  </div>
{/if}
