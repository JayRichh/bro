<script lang="ts">
  // The Appearance group's actions, above its rows: import a VS Code theme,
  // take VS Code's fonts, Kete's own look back; the imported themes, each
  // removable; and why VS Code (live) is showing One Dark, when it is.
  // Each row below (theme, fonts, sizes) is the schema's.
  import { onMount } from 'svelte';
  import { api } from '../api';
  import { app } from '../store.svelte';
  import { look } from '../look.svelte';
  import { importVsCodeTheme, lookIsKetes, removeImported, resetAppearance, useVsCodeFonts } from './appearance';

  let { onchange }: { onchange: () => void } = $props();

  /** Which VS Code the look reads; null: none found, '' until asked. */
  let vscode = $state<string | null>('');
  let busy = $state(false);

  onMount(() => {
    api
      .lookVscodeInstall()
      .then((v) => (vscode = v))
      .catch(() => (vscode = null));
  });

  async function run(action: () => Promise<boolean>) {
    busy = true;
    try {
      if (await action()) onchange();
    } finally {
      busy = false;
    }
  }

  const imported = $derived(Object.entries(look.prefs.themes).sort((a, b) => a[1].label.localeCompare(b[1].label)));
  const ketes = $derived(lookIsKetes());
  const btn =
    'h-7 flex-none border border-rule bg-transparent px-2.5 text-[13px] whitespace-nowrap text-ink enabled:hover:border-rule-strong disabled:text-faint';
</script>

<li class="flex flex-col gap-2 border-b border-rule px-6 py-3">
  <div class="flex flex-wrap items-center gap-2">
    <button
      class={btn}
      disabled={busy}
      title={vscode ? `Pick one of the themes ${vscode} has installed, or a theme file, and wear it` : "Pick a VS Code theme's .json file and wear it"}
      onclick={() => run(importVsCodeTheme)}>Import a VS Code theme&hellip;</button
    >
    <button
      class={btn}
      disabled={busy || !vscode}
      title={vscode
        ? `Take ${vscode}'s editor font for code, its terminal font (the editor's where unset) for terminals and diffs, and their sizes`
        : vscode === null
          ? "VS Code's settings weren't found on this machine"
          : 'Looking for VS Code'}
      onclick={() => run(useVsCodeFonts)}>Use VS Code's fonts</button
    >
    <button
      class={btn}
      disabled={busy || ketes}
      title={ketes ? "This is Kete's own look already" : 'Workshop following the system, Geist, Geist Mono and JetBrains Mono at their sizes; imported themes stay'}
      onclick={() => run(resetAppearance)}>Reset appearance to Kete defaults</button
    >
  </div>
  {#if imported.length}
    <div class="flex flex-wrap items-center gap-x-2 gap-y-1.5">
      <span class="font-mono text-[12px] text-muted">imported</span>
      {#each imported as [id, palette] (id)}
        {@const worn = app.themePref === `import:${id}`}
        <span class="inline-flex h-6 max-w-[220px] items-center border {worn ? 'border-rule-strong' : 'border-rule'}">
          <span class="flex flex-none border-r border-rule" aria-hidden="true">
            {#each ['color-bg', 'color-ink', 'color-syn-keyword', 'color-syn-string'] as k (k)}
              <span class="block h-[22px] w-1.5" style:background-color={palette.tokens[k]}></span>
            {/each}
          </span>
          <span class="min-w-0 truncate px-1.5 text-[12px] text-ink" title={worn ? `${palette.label} (worn now)` : palette.label}>{palette.label}</span>
          <button
            class="h-full flex-none border-0 border-l border-rule bg-transparent px-1.5 text-[12px] text-muted enabled:hover:text-ink disabled:text-faint"
            disabled={busy}
            aria-label="Remove imported theme {palette.label}"
            title="Remove {palette.label} from the themes"
            onclick={() => run(() => removeImported(id))}>&times;</button
          >
        </span>
      {/each}
    </div>
  {/if}
  {#if app.themePref === 'vscode' && look.liveProblem}
    <p class="m-0 flex items-center gap-1.5 font-mono text-[11px] text-muted">
      <span class="size-1.5 flex-none border border-faint" aria-hidden="true"></span>{look.liveProblem}
    </p>
  {:else if app.themePref === 'vscode' && look.live}
    <p class="m-0 font-mono text-[11px] text-muted">VS Code (live) is wearing {look.live.label}</p>
  {/if}
</li>
