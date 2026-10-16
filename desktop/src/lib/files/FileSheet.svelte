<script lang="ts">
  // The files a session opened with `kete open`, one tab each, read-only in
  // Monaco: the terminal app's FILE TABS. Tab / Shift+Tab or [ and ] switch
  // file, 1-9 pick one, Esc closes; Open hands the file to its own app.
  import { onDestroy, onMount, tick } from 'svelte';
  import { app, toast } from '../store.svelte';
  import { languageOf, fileIcon } from '../icons';
  import { defineMonacoTheme, termFamily } from '../theme';
  import { closeFiles, filesApi, openTabDefault, readTab, sheet, showFiles, stepTab, type FileText } from './files.svelte';
  import { registerViewer } from '../menus/surfaces';
  import type * as Monaco from 'monaco-esm/editor/editor.api.js';

  let box = $state<HTMLElement>();
  let host = $state<HTMLDivElement>();
  let monaco: typeof Monaco | null = null;
  let editor: Monaco.editor.IStandaloneCodeEditor | null = null;
  let shown = $state<{ path: string; file: FileText | null }>({ path: '', file: null });

  const tab = $derived(sheet.open ? sheet.tabs[Math.min(sheet.at, sheet.tabs.length - 1)] : undefined);

  let unlisten: (() => void) | undefined;
  onMount(() => {
    filesApi
      .onOpened((o) => {
        showFiles(o);
        // Take the keys unless the user is typing in a shell or a field.
        const at = document.activeElement;
        if (!at?.closest('.xterm, input, textarea, select')) tick().then(() => box?.focus());
      })
      .then((u) => (unlisten = u))
      .catch(() => {});
  });

  // Read the shown tab's file each time it is shown: an agent that opened
  // it may still be writing it.
  $effect(() => {
    const t = tab;
    const path = t?.path;
    if (!t || !path) return;
    shown = { path, file: null };
    readTab(t)
      .then((file) => shown.path === path && (shown = { path, file }))
      .catch((e) => shown.path === path && (shown = { path, file: { text: null, note: String(e) } }));
  });

  // The editor comes up with the first file to show, and stays.
  $effect(() => {
    const f = shown.file;
    const el = host;
    if (!f?.text || !el) return;
    void show(el, shown.path, f.text);
  });

  async function show(el: HTMLDivElement, path: string, text: string) {
    const m = (await import('../monaco')) as typeof import('../monaco');
    if (!monaco) {
      monaco = m.monaco;
      defineMonacoTheme(monaco, app.dark);
    }
    if (shown.path !== path || !sheet.open) return;
    // The sheet's body is new each time it opens: the editor goes with it.
    if (!editor) editor = monaco.editor.create(el, { ...m.readingOptions(termFamily()), renderLineHighlight: 'line' });
    const old = editor.getModel();
    editor.setModel(monaco.editor.createModel(text, languageOf(path)));
    old?.dispose();
  }

  $effect(() => {
    const dark = app.dark;
    if (monaco) defineMonacoTheme(monaco, dark);
  });

  function dropEditor() {
    editor?.getModel()?.dispose();
    editor?.dispose();
    editor = null;
  }

  // Closed: the editor and its model go; the next open makes a new one.
  $effect(() => {
    if (!sheet.open) {
      dropEditor();
      shown = { path: '', file: null };
    }
  });

  onDestroy(() => {
    unlisten?.();
    dropEditor();
  });

  // The viewer's right-click menu (menus/builtin.ts): Copy path:line is
  // this tab's file.
  $effect(() => {
    const el = host;
    if (!el) return;
    return registerViewer(el, {
      editorAt: (n) => (editor?.getDomNode()?.contains(n) ? { editor, old: false } : null),
      path: () => shown.path || null,
    });
  });

  function onKey(e: KeyboardEvent) {
    e.stopPropagation();
    if (e.ctrlKey || e.altKey || e.metaKey) return;
    if (e.key === 'Escape') (e.preventDefault(), closeFiles());
    else if (e.key === 'Tab') (e.preventDefault(), stepTab(e.shiftKey ? -1 : 1));
    else if (e.target instanceof Element && e.target.closest('.monaco-editor')) return;
    else if (e.key === ']' || e.key === '[') (e.preventDefault(), stepTab(e.key === ']' ? 1 : -1));
    else if (/^[1-9]$/.test(e.key) && Number(e.key) <= sheet.tabs.length) (e.preventDefault(), (sheet.at = Number(e.key) - 1));
  }

  function openDefault() {
    if (tab) openTabDefault(tab).catch((e) => toast(`Could not open it: ${e}`));
  }

  const tool = 'inline-flex h-7 items-center gap-1.5 border border-rule bg-transparent px-2 text-[13px] text-ink hover:border-rule-strong';
</script>

{#if sheet.open && tab}
  <!-- svelte-ignore a11y_no_static_element_interactions, a11y_click_events_have_key_events -->
  <div class="fixed inset-0 z-40 bg-[rgb(28_25_23/0.18)]" onmousedown={closeFiles}></div>
  <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
  <div
    bind:this={box}
    class="fixed inset-y-0 right-0 z-50 flex w-[min(1000px,82vw)] flex-col border-l border-rule-strong bg-bg text-ink outline-none"
    role="dialog"
    aria-modal="true"
    aria-label="Opened files"
    tabindex="-1"
    onkeydown={onKey}
  >
    <header class="flex flex-none flex-col gap-1 border-b border-rule px-4 py-2">
      <div class="flex min-w-0 items-center gap-3">
        <span class="font-mono text-[11px] text-muted">OPENED</span>
        <span class="min-w-0 truncate font-mono text-[12px] text-muted" title={sheet.root}>{sheet.root}</span>
        <span class="flex-1"></span>
        <button class={tool} onclick={openDefault} title="Open it with the app your system uses for it">Open</button>
        <button
          class="grid size-7 place-items-center border border-rule bg-transparent text-ink hover:border-rule-strong"
          onclick={closeFiles}
          title="Close (Esc)"
          aria-label="Close the files"
        >
          <svg width="12" height="12" viewBox="0 0 12 12" aria-hidden="true"><path d="M2 2l8 8M10 2l-8 8" stroke="currentColor" stroke-width="1.4" /></svg>
        </button>
      </div>
      <div class="-mb-2 flex min-w-0 items-end gap-0 overflow-x-auto" role="tablist" aria-label="Files">
        {#each sheet.tabs as t, i (t.path)}
          {@const on = i === sheet.at}
          <button
            role="tab"
            aria-selected={on}
            title={t.path}
            class="-mb-px inline-flex flex-none items-center gap-1.5 border-0 border-b-2 bg-transparent px-3 py-1.5 font-mono text-[12px] {on
              ? 'border-ink text-ink'
              : 'border-transparent text-muted hover:text-ink'}"
            onclick={() => (sheet.at = i)}
          >
            <img class="flex-none" src={fileIcon(t.path, app.dark)} alt="" width="14" height="14" draggable="false" />
            {t.label}
          </button>
        {/each}
      </div>
    </header>
    <div class="relative min-h-0 flex-1" role="tabpanel">
      {#if !shown.file}
        <p class="p-6 text-[13px] text-muted">Reading&hellip;</p>
      {:else if !shown.file.text}
        <p class="p-6 text-[14px] text-muted">{shown.file.note}</p>
      {:else if shown.file.note}
        <p class="m-0 h-8 border-b border-rule px-4 py-1.5 text-[12px] text-muted">{shown.file.note}</p>
      {/if}
      <div class="absolute inset-x-0 bottom-0 {shown.file?.note ? 'top-8' : 'top-0'} {shown.file?.text ? '' : 'hidden'}" bind:this={host}></div>
    </div>
  </div>
{/if}
