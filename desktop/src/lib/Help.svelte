<script lang="ts">
  // ? or F1: every shortcut the window answers, from the one table App.svelte
  // dispatches through, so this list can't drift from the keys.
  import { ui } from './ui/ui.svelte';
  import { helpGroups, keyLabel, type Scope } from './shortcuts';

  const groups = helpGroups();
  const WHERE: Record<Scope, string> = {
    anywhere: 'anywhere',
    fields: 'not in a terminal',
    global: '',
    row: '',
  };

  function onKey(e: KeyboardEvent) {
    e.stopPropagation();
    if (e.key === 'Escape' || e.key === '?' || e.key === 'F1') (e.preventDefault(), (ui.help = false));
  }
</script>

{#if ui.help}
  <!-- svelte-ignore a11y_no_static_element_interactions, a11y_click_events_have_key_events -->
  <div class="fixed inset-0 z-40 bg-[rgb(28_25_23/0.18)]" onmousedown={() => (ui.help = false)}></div>
  <!-- svelte-ignore a11y_autofocus -->
  <div
    class="fixed top-[8%] left-1/2 z-50 flex max-h-[84vh] w-[min(860px,94vw)] -translate-x-1/2 flex-col border border-rule-strong bg-bg text-ink outline-none"
    role="dialog"
    aria-modal="true"
    aria-labelledby="help-title"
    tabindex="-1"
    autofocus
    onkeydown={onKey}
  >
    <header class="flex flex-none items-baseline gap-4 border-b border-rule-strong px-5 py-3">
      <h2 id="help-title" class="m-0 text-[17px] font-semibold">Keyboard shortcuts</h2>
      <span class="font-mono text-[12px] text-muted">right-click a row for its actions</span>
      <span class="flex-1"></span>
      <kbd class="border border-rule px-1.5 font-mono text-[11px] leading-5 text-muted">Esc</kbd>
    </header>
    <div class="min-h-0 gap-x-8 overflow-y-auto px-5 pb-5 md:columns-2">
      {#each groups as g (g.group)}
        <section class="break-inside-avoid pt-4" aria-label={g.group}>
          <h3 class="m-0 border-b border-rule-strong pb-1 text-[13px] font-semibold">
            {g.group}{#if g.group === 'Sessions'}<span class="ml-2 font-mono text-[11px] font-normal text-muted">on a focused row or card</span>{/if}
          </h3>
          <ul class="m-0 list-none p-0">
            {#each g.rows as s (s.label)}
              <li class="flex items-center gap-3 border-b border-rule py-1.5">
                <span class="min-w-0 flex-1 text-[13px]">
                  {s.label}
                  {#if s.scope && WHERE[s.scope]}<span class="ml-1 font-mono text-[11px] text-muted">{WHERE[s.scope]}</span>{/if}
                </span>
                <span class="flex flex-none gap-1">
                  {#each s.keys as k (k)}<kbd class="border border-rule px-1.5 font-mono text-[11px] leading-5">{keyLabel(k)}</kbd>{/each}
                </span>
              </li>
            {/each}
          </ul>
        </section>
      {/each}
    </div>
  </div>
{/if}
