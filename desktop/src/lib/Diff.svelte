<script lang="ts">
  import { onDestroy, onMount } from 'svelte';
  import { app, sessionWorktree, toast } from './store.svelte';
  import { nav } from './shell/history.svelte';
  import { api, type DiffFile } from './api';
  import { fileIcon, languageOf } from './icons';
  import { placeOf } from './status';
  import { defineMonacoTheme, termFamily } from './theme';
  import type * as Monaco from 'monaco-esm/editor/editor.api.js';

  let monaco: typeof Monaco | null = null;
  let host: HTMLDivElement;
  let editor: Monaco.editor.IStandaloneDiffEditor | null = null;
  let files = $state<DiffFile[]>([]);
  let current = $state<DiffFile | null>(null);
  let loading = $state(false);
  let error = $state<string | null>(null);
  let note = $state<string | null>(null);

  const worktree = $derived(
    app.diffWorktree ?? sessionWorktree(app.session) ?? app.model.worktrees[0]?.id ?? null,
  );
  const where = $derived(worktree ? placeOf(app.model, worktree) : null);
  const totals = $derived(
    files.reduce((t, f) => ({ add: t.add + (f.added ?? 0), del: t.del + (f.removed ?? 0) }), { add: 0, del: 0 }),
  );

  async function load(wt: string | null) {
    if (!wt) return;
    loading = true;
    error = null;
    try {
      const list = await api.diffFiles(wt);
      if (wt !== worktree) return;
      files = list;
      const keep = current && list.find((f) => f.path === current!.path);
      await show(keep ?? list[0] ?? null);
    } catch (e) {
      error = String(e);
      files = [];
      await show(null);
    } finally {
      loading = false;
    }
  }

  /** Bumped per pick: a pair that arrives after a newer pick is dropped.
   *  (Comparing `current` to `f` cannot work: `$state` stores a proxy.) */
  let pick = 0;

  async function show(f: DiffFile | null) {
    const mine = ++pick;
    current = f;
    note = null;
    if (!editor || !monaco) return;
    const old = editor.getModel();
    if (!f || !worktree) {
      editor.setModel(null);
    } else {
      try {
        const pair = await api.diffPair(worktree, f);
        if (mine !== pick) return;
        note = pair.note;
        const lang = languageOf(f.path);
        editor.setModel({
          original: monaco.editor.createModel(pair.original, lang),
          modified: monaco.editor.createModel(pair.modified, lang),
        });
      } catch (e) {
        toast(String(e));
      }
    }
    old?.original.dispose();
    old?.modified.dispose();
  }

  function move(d: number) {
    if (!files.length) return;
    const i = current ? files.findIndex((f) => f.path === current!.path) : -1;
    const next = files[Math.max(0, Math.min(files.length - 1, i + d))];
    if (next !== current) show(next);
    queueMicrotask(() => document.querySelector('[data-file-cur]')?.scrollIntoView({ block: 'nearest' }));
  }

  function onKey(e: KeyboardEvent) {
    if (nav.current.view !== 'diff') return;
    const t = e.target as HTMLElement;
    if (t.closest('input, textarea, select, .monaco-editor')) return;
    if (e.ctrlKey || e.altKey || e.metaKey) return;
    if (e.key === 'ArrowDown' || e.key === 'j') (e.preventDefault(), move(1));
    else if (e.key === 'ArrowUp' || e.key === 'k') (e.preventDefault(), move(-1));
    else if (e.key === 's') app.diffMode = 'side';
    else if (e.key === 'i') app.diffMode = 'inline';
    else if (e.key === 'r') load(worktree);
  }

  onMount(async () => {
    await document.fonts.load('12px ' + termFamily()).catch(() => {});
    monaco = (await import('./monaco')).monaco;
    editor = monaco.editor.createDiffEditor(host, {
      theme: 'vs-dark',
      readOnly: true,
      originalEditable: false,
      automaticLayout: true,
      renderSideBySide: app.diffMode === 'side',
      useInlineViewWhenSpaceIsLimited: false,
      fontFamily: termFamily(),
      fontSize: 12.5,
      lineHeight: 19,
      fontLigatures: false,
      minimap: { enabled: false },
      scrollBeyondLastLine: false,
      renderOverviewRuler: true,
      hideUnchangedRegions: { enabled: true, contextLineCount: 3, minimumLineCount: 4 },
      ignoreTrimWhitespace: false,
      renderIndicators: true,
      padding: { top: 6 },
      stickyScroll: { enabled: false },
      scrollbar: { verticalScrollbarSize: 10, horizontalScrollbarSize: 10 },
    });
    defineMonacoTheme(monaco, app.dark);
    load(worktree);
  });

  let loadedFor: string | null = null;
  $effect(() => {
    const wt = worktree;
    if (editor && wt !== loadedFor) {
      loadedFor = wt;
      current = null;
      load(wt);
    }
  });
  $effect(() => {
    const side = app.diffMode === 'side';
    editor?.updateOptions({ renderSideBySide: side });
  });
  // Re-read the tokens once the new data-theme has applied.
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
    return i < 0 ? { dir: '', base: path } : { dir: path.slice(0, i + 1), base: path.slice(i + 1) };
  }
  const STATUS_COLOR: Record<string, string> = {
    M: 'text-git-modified', A: 'text-git-added', U: 'text-git-added', D: 'text-git-deleted', R: 'text-git-renamed', C: 'text-git-conflict',
  };
  const STATUS_WORD: Record<string, string> = {
    M: 'Modified', A: 'Added', D: 'Deleted', R: 'Renamed', U: 'Untracked', C: 'Conflicted',
  };
</script>

<svelte:window onkeydown={onKey} />

<div class="flex h-full min-h-0 flex-col">
  <div class="flex h-10 min-w-0 flex-none items-center gap-3 border-b border-rule pr-2 pl-3 text-[13px]">
    <select
      class="h-7 max-w-[260px] border border-rule bg-bg px-1.5 font-mono text-[12px] text-ink"
      aria-label="Worktree"
      value={worktree ?? ''}
      onchange={(e) => (app.diffWorktree = (e.target as HTMLSelectElement).value)}
    >
      {#each app.model.worktrees as w (w.id)}
        <option value={w.id}>{placeOf(app.model, w.id).project} / {w.branch}</option>
      {/each}
    </select>
    {#if files.length}
      <span class="font-mono text-[12px] whitespace-nowrap text-muted"
        >{files.length} {files.length === 1 ? 'file' : 'files'}
        <span class="text-git-added">+{totals.add}</span> <span class="text-git-deleted">&minus;{totals.del}</span></span
      >
    {/if}
    <span class="flex-1"></span>
    {#if current}
      <span class="min-w-0 truncate font-mono text-[12px] text-muted [direction:rtl]" title={current.path}>{current.path}</span>
    {/if}
    <div class="flex h-7 flex-none border border-rule" role="radiogroup" aria-label="Diff layout">
      {#each [{ id: 'side', label: 'Side by side', key: 'S' }, { id: 'inline', label: 'Inline', key: 'I' }] as m, i (m.id)}
        {@const on = app.diffMode === m.id}
        <button
          role="radio"
          aria-checked={on}
          class="inline-flex items-center gap-1.5 border-0 px-2 text-[12px]
            {i ? 'border-l border-rule' : ''} {on ? 'bg-ink text-bg' : 'bg-transparent text-muted hover:text-ink'}"
          onclick={() => (app.diffMode = m.id as 'side' | 'inline')}
          title="{m.label} ({m.key})"
        >
          <svg width="13" height="13" viewBox="0 0 16 16" aria-hidden="true"
            ><rect x="1.5" y="2.5" width="13" height="11" fill="none" stroke="currentColor" stroke-width="1.3" /><path
              d={m.id === 'side' ? 'M8 2.5v11' : 'M4 6.5h8M4 9.5h8'}
              stroke="currentColor"
              stroke-width="1.3"
            /></svg
          >
          {m.label}
        </button>
      {/each}
    </div>
    <button
      class="grid size-7 place-items-center border-0 bg-transparent text-muted hover:bg-surface hover:text-ink"
      onclick={() => load(worktree)}
      title="Refresh (R)"
      aria-label="Refresh"
    >
      <svg width="13" height="13" viewBox="0 0 16 16" class={loading ? 'animate-spin' : ''}
        ><path d="M13.5 8A5.5 5.5 0 1 1 11.8 4M13.5 2v3h-3" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round" /></svg
      >
    </button>
  </div>
  <div class="grid min-h-0 flex-1 grid-cols-[272px_1fr]">
    <ul class="m-0 list-none overflow-y-auto border-r border-rule bg-bg p-0" aria-label="Changed files">
      {#each files as f (f.path)}
        {@const p = split(f.path)}
        {@const cur = current?.path === f.path}
        <li>
          <button
            data-file-cur={cur || undefined}
            class="flex min-h-7 w-full items-center gap-2 border-0 border-b border-rule px-3 py-1 text-left text-[13px]
              focus-visible:-outline-offset-2 {cur ? 'bg-raised' : 'bg-transparent hover:bg-surface'}"
            onclick={() => show(f)}
            title={f.from ? `${f.from} -> ${f.path}` : f.path}
          >
            <img class="flex-none" src={fileIcon(f.path, app.dark)} alt="" width="16" height="16" draggable="false" />
            <span class="min-w-0 truncate"
              ><span class={f.status === 'D' ? 'text-muted line-through decoration-git-deleted' : 'text-ink'}>{p.base}</span><span
                class="ml-1.5 text-[12px] text-muted">{p.dir}</span
              ></span
            >
            <span class="ml-auto flex flex-none gap-1 font-mono text-[12px]">
              {#if f.added}<span class="text-git-added">+{f.added}</span>{/if}
              {#if f.removed}<span class="text-git-deleted">&minus;{f.removed}</span>{/if}
            </span>
            <span class="w-3 flex-none text-center font-mono text-[12px] font-bold {STATUS_COLOR[f.status]}" title={STATUS_WORD[f.status]}
              >{f.status}</span
            >
          </button>
        </li>
      {/each}
      {#if !files.length && !loading}
        <li class="px-3 py-3 text-[13px] text-muted">
          {#if error}<span class="text-fail">{error}</span>{:else if worktree}Working tree clean. Nothing to diff against HEAD.{:else}No worktree to diff.{/if}
        </li>
      {/if}
    </ul>
    <div class="relative min-h-0 min-w-0">
      <div class="absolute inset-0" bind:this={host}></div>
      {#if note}<div class="absolute inset-0 grid place-items-center bg-bg text-[13px] text-muted">{note}</div>{/if}
      {#if !current && !loading}
        <div class="absolute inset-0 grid place-items-center bg-bg text-[13px] text-muted">{where ? `${where.project} / ${where.branch}` : ''}</div>
      {/if}
    </div>
  </div>
</div>
