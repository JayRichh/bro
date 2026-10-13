<script lang="ts">
  // A pull request's diff: the source against its merge-base with the
  // target, pinned to the SHAs `diff_range` hands back when it opens, so
  // every file reads from the same two commits. Used by the PR detail and
  // the composer's preview.
  import { onDestroy, onMount } from 'svelte';
  import { app } from '../store.svelte';
  import type { DiffFile } from '../api';
  import { fileIcon, languageOf } from '../icons';
  import { defineMonacoTheme, termFamily } from '../theme';
  import { prs, type Blob, type RangeDiff } from './prs-api';
  import type * as Monaco from 'monaco-esm/editor/editor.api.js';

  let {
    worktree,
    base,
    tip,
    layout = 'inline',
  }: { worktree: string; base: string; tip: string; layout?: 'side' | 'inline' } = $props();

  let monaco: typeof Monaco | null = null;
  let host: HTMLDivElement;
  let editor: Monaco.editor.IStandaloneDiffEditor | null = null;
  let range = $state<RangeDiff | null>(null);
  let current = $state<DiffFile | null>(null);
  let loading = $state(false);
  let error = $state<string | null>(null);
  /** Why the open file has no text to compare, when it has none. */
  let fileNote = $state<string | null>(null);
  let mode = $state<'side' | 'inline'>('inline');
  $effect.pre(() => {
    mode = layout;
  });

  const totals = $derived(
    (range?.files ?? []).reduce((t, f) => ({ add: t.add + (f.added ?? 0), del: t.del + (f.removed ?? 0) }), { add: 0, del: 0 }),
  );

  let asked = 0;
  async function load() {
    const mine = ++asked;
    loading = true;
    error = null;
    try {
      const r = await prs.diffRange(worktree, base, tip);
      if (mine !== asked) return;
      range = r;
      await show(r.files[0] ?? null);
    } catch (e) {
      if (mine !== asked) return;
      range = null;
      error = String(e);
      await show(null);
    } finally {
      if (mine === asked) loading = false;
    }
  }

  /** A side's text. Missing is empty only where the file's status says the
   *  side does not exist (added: no base, deleted: no tip). */
  function sideText(b: Blob, f: DiffFile, which: 'base' | 'tip'): string | { note: string } {
    switch (b.state) {
      case 'text':
        return b.value;
      case 'missing':
        if ((which === 'base' && f.status === 'A') || (which === 'tip' && f.status === 'D')) return '';
        return { note: `${f.path} is missing on the ${which === 'base' ? 'base' : 'source'} side. Fetch and reopen.` };
      case 'tooLarge':
        return { note: `${f.path} is too large to show (${Math.round(b.value / 1024)} KB, the cap is 2 MB).` };
      case 'binary':
        return { note: `${f.path} is a binary file.` };
    }
  }

  let pick = 0;
  async function show(f: DiffFile | null) {
    const mine = ++pick;
    current = f;
    fileNote = null;
    if (!editor || !monaco) return;
    const old = editor.getModel();
    if (!f || !range) {
      editor.setModel(null);
    } else {
      try {
        const basePath = f.from ?? f.path;
        const [a, b] = await Promise.all([
          prs.diffBlob(worktree, { kind: 'commit', rev: range.base }, basePath),
          prs.diffBlob(worktree, { kind: 'commit', rev: range.tip }, f.path),
        ]);
        if (mine !== pick) return;
        const left = sideText(a, f, 'base');
        const right = sideText(b, f, 'tip');
        if (typeof left !== 'string' || typeof right !== 'string') {
          fileNote = (typeof left !== 'string' ? left : (right as { note: string })).note;
          editor.setModel(null);
        } else {
          const lang = languageOf(f.path);
          editor.setModel({
            original: monaco.editor.createModel(left, lang),
            modified: monaco.editor.createModel(right, lang),
          });
        }
      } catch (e) {
        if (mine !== pick) return;
        fileNote = String(e);
        editor.setModel(null);
      }
    }
    old?.original.dispose();
    old?.modified.dispose();
  }

  onMount(async () => {
    await document.fonts.load('12px ' + termFamily()).catch(() => {});
    monaco = (await import('../monaco')).monaco;
    editor = monaco.editor.createDiffEditor(host, {
      readOnly: true,
      originalEditable: false,
      automaticLayout: true,
      renderSideBySide: mode === 'side',
      renderOverviewRuler: false,
      useInlineViewWhenSpaceIsLimited: false,
      fontFamily: termFamily(),
      fontSize: 12.5,
      lineHeight: 19,
      fontLigatures: false,
      minimap: { enabled: false },
      scrollBeyondLastLine: false,
      hideUnchangedRegions: { enabled: true, contextLineCount: 3, minimumLineCount: 4 },
      ignoreTrimWhitespace: false,
      padding: { top: 6 },
      stickyScroll: { enabled: false },
      scrollbar: { verticalScrollbarSize: 10, horizontalScrollbarSize: 10 },
    });
    defineMonacoTheme(monaco, app.dark);
    if (current) show(current);
  });

  let loadedFor = '';
  $effect(() => {
    const key = `${worktree}\u0000${base}\u0000${tip}`;
    if (key !== loadedFor && worktree && base && tip) {
      loadedFor = key;
      load();
    }
  });
  $effect(() => {
    const side = mode === 'side';
    editor?.updateOptions({ renderSideBySide: side });
  });
  $effect(() => {
    const dark = app.dark;
    requestAnimationFrame(() => monaco && defineMonacoTheme(monaco, dark));
  });

  onDestroy(() => {
    const m = editor?.getModel();
    editor?.dispose();
    m?.original.dispose();
    m?.modified.dispose();
  });

  function split(path: string) {
    const i = path.lastIndexOf('/');
    return i < 0 ? { dir: '', name: path } : { dir: path.slice(0, i + 1), name: path.slice(i + 1) };
  }
  const STATUS_COLOR: Record<string, string> = {
    M: 'text-git-modified',
    A: 'text-git-added',
    U: 'text-git-added',
    D: 'text-git-deleted',
    R: 'text-git-renamed',
    C: 'text-git-conflict',
  };
  const STATUS_WORD: Record<string, string> = {
    M: 'Modified',
    A: 'Added',
    D: 'Deleted',
    R: 'Renamed',
    U: 'Untracked',
    C: 'Conflicted',
  };
</script>

<div class="flex h-full min-h-0 flex-col">
  <div class="flex h-9 min-w-0 flex-none items-center gap-3 border-b border-rule px-3 text-[12px]">
    {#if range}
      <span class="font-mono whitespace-nowrap text-muted" title="Pinned: merge-base {range.base} to tip {range.tip}"
        >{range.base.slice(0, 8)}..{range.tip.slice(0, 8)}</span
      >
      <span class="font-mono whitespace-nowrap text-muted"
        >{range.files.length} {range.files.length === 1 ? 'file' : 'files'}
        <span class="text-git-added">+{totals.add}</span> <span class="text-git-deleted">&minus;{totals.del}</span></span
      >
      {#if range.offline}<span class="text-muted">Offline: read from cached refs</span>{/if}
    {:else if loading}
      <span class="font-mono text-muted" role="status">Loading the diff</span>
    {/if}
    <span class="flex-1"></span>
    <div class="flex h-6 flex-none border border-rule" role="radiogroup" aria-label="Diff layout">
      {#each [{ id: 'side', label: 'Side by side' }, { id: 'inline', label: 'Inline' }] as m, i (m.id)}
        {@const on = mode === m.id}
        <button
          type="button"
          role="radio"
          aria-checked={on}
          class="border-0 px-2 text-[12px] {i ? 'border-l border-rule' : ''} {on ? 'bg-ink text-bg' : 'bg-transparent text-muted hover:text-ink'}"
          onclick={() => (mode = m.id as 'side' | 'inline')}>{m.label}</button
        >
      {/each}
    </div>
  </div>
  <div class="grid min-h-0 flex-1 grid-cols-[minmax(180px,30%)_1fr]">
    <ul class="m-0 list-none overflow-y-auto border-r border-rule p-0" aria-label="Changed files">
      {#each range?.files ?? [] as f (f.path)}
        {@const p = split(f.path)}
        {@const cur = current?.path === f.path}
        <li>
          <button
            type="button"
            class="flex min-h-7 w-full items-center gap-2 border-0 border-b border-rule px-2.5 py-1 text-left text-[13px] {cur
              ? 'bg-raised'
              : 'bg-transparent hover:bg-surface'}"
            onclick={() => show(f)}
            title={f.from ? `${f.from} -> ${f.path}` : f.path}
            aria-current={cur ? 'true' : undefined}
          >
            <img class="flex-none" src={fileIcon(f.path, app.dark)} alt="" width="16" height="16" draggable="false" />
            <span class="min-w-0 truncate"
              ><span class={f.status === 'D' ? 'text-muted line-through' : 'text-ink'}>{p.name}</span><span
                class="ml-1.5 text-[12px] text-muted">{p.dir}</span
              ></span
            >
            <span class="ml-auto w-3 flex-none text-center font-mono text-[12px] font-bold {STATUS_COLOR[f.status]}"
              title={STATUS_WORD[f.status]}>{f.status}</span
            >
          </button>
        </li>
      {/each}
      {#if !loading && !error && range && !range.files.length}
        <li class="px-3 py-3 text-[13px] text-muted">No changes between the source and its merge-base.</li>
      {/if}
      {#if error}
        <li class="px-3 py-3 text-[13px] text-fail select-text">{error}</li>
      {/if}
    </ul>
    <div class="relative min-h-0 min-w-0">
      <div class="absolute inset-0" bind:this={host}></div>
      {#if fileNote}
        <div class="absolute inset-0 grid place-items-center bg-bg px-6 text-center text-[13px] text-muted">{fileNote}</div>
      {:else if !current}
        <div class="absolute inset-0 bg-bg"></div>
      {/if}
    </div>
  </div>
</div>
