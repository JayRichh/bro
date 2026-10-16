<script lang="ts">
  // The Files view, beside Diff in the split's right pane: a checkout's
  // files as git lists them (tracked, and untracked but not ignored),
  // folded until opened, each with git's mark. Opened folders are kept per
  // worktree while the window runs. The filter matches the path from the
  // checkout's root, as the terminal app's tree browser does. A row's menu
  // is the matrix's File or Folder menu (files/menus.ts); a row dragged
  // onto a session, its card or its terminal types its path there.
  //
  // Keys on a row: Up/Down (j/k), Home/End, Left/Right fold and open,
  // Enter opens a file in Kete (a folder: opens or folds it), Shift+Enter
  // in your editor, D diffs a changed file, ? its menu with every key, / or
  // F the filter, R reads the tree again.
  import { onMount, tick, untrack } from 'svelte';
  import { app, sessionWorktree, toast } from '../store.svelte';
  import { nav } from '../shell/history.svelte';
  import { groupWorktrees } from '../diff-base';
  import { scopedWorktree } from '../scope';
  import { allKeysItem, toggleMaximize } from '../commands';
  import { closestTarget } from '../actions';
  import { withKey } from '../shortcuts';
  import { fileIcon, folderIcon } from '../icons';
  import { ownMenu, type Opened } from '../menus/router';
  import { fileTable } from '../menus/targets';
  import { openMenu } from '../ui/ui.svelte';
  import Icon from '../ui/Icon.svelte';
  import { fileVerbs, folderVerbs, underRoot } from '../editors/verbs';
  import { canCollapse, canExpand, filteredRows, foldedRows, hasChanges, isFolder, markOf, parentOf, VIEWER_MAX_BYTES, type Entry, type Row } from './tree';
  import { closeFilesView, collapseAll, expandAll, loadAll, loadLevel, pickFilesWorktree, refresh, setOpen, treeOf } from './tree.svelte';
  import { TOO_BIG, treeFileItems, treeFolderItems } from './menus';
  import { diffFile } from './landing.svelte';
  import { endPathDrag, pathDrag, startPathDrag } from './drop.svelte';

  /** A row's height: the list draws only the rows in view. */
  const ROW_H = 26;
  const INDENT = 14;

  // The checkout: the place's, else the scope's or the session's, as the
  // Diff view asks.
  const worktree = $derived(nav.current.view === 'files' ? (nav.current.item ?? scopedWorktree(app.model) ?? sessionWorktree(app.session) ?? null) : null);
  const w = $derived(worktree ? (app.model.worktrees.find((x) => x.id === worktree) ?? null) : null);
  const project = $derived(w ? app.model.projects.find((p) => p.id === w.project_id) : undefined);
  const tree = $derived(w ? treeOf(w.id) : null);
  const groups = $derived(groupWorktrees(app.model));

  const filtering = $derived(!!tree?.filter.trim());
  const shown = $derived.by((): { rows: Row[]; best: number | null; matches: number | null } => {
    if (!tree) return { rows: [], best: null, matches: null };
    if (filtering) return tree.all ? filteredRows(tree.all, tree.filter.trim()) : { rows: [], best: null, matches: null };
    return { rows: foldedRows(tree.levels, tree.open), best: null, matches: null };
  });
  const rows = $derived(shown.rows);

  const rowKey = (r: Row) => (r.kind === 'entry' ? r.entry.path : `\u0000${r.kind}:${r.dir}`);
  const at = $derived.by(() => {
    const i = rows.findIndex((r) => rowKey(r) === tree?.cursor);
    return i < 0 ? 0 : i;
  });

  // ---- reading ----

  // Shown: read the root, or again what was read before.
  $effect(() => {
    const t = tree;
    if (!t) return;
    untrack(() => void (t.levels.has('') ? refresh(t) : loadLevel(t, '')));
  });

  // A session in the checkout changed state (a turn ended, it wrote
  // files): the marks are read again.
  const stamp = $derived(w ? app.model.agents.reduce((m, a) => (a.worktree_id === w.id ? Math.max(m, a.status_changed_at) : m), 0) : 0);
  let stampFor = '';
  let stampSeen = 0;
  $effect(() => {
    const s = stamp;
    const t = tree;
    if (!t) return;
    untrack(() => {
      if (stampFor !== t.worktree) {
        stampFor = t.worktree;
        stampSeen = s;
      } else if (s !== stampSeen) {
        stampSeen = s;
        void refresh(t);
      }
    });
  });

  // The filter searches every row: read once it is first typed in.
  $effect(() => {
    const t = tree;
    if (t && filtering && !t.all) untrack(() => void loadAll(t));
  });

  // A new filter puts the keys on its best match.
  $effect(() => {
    const best = shown.best;
    void tree?.filter;
    untrack(() => {
      if (tree && filtering && best !== null && rows[best]) tree.cursor = rowKey(rows[best]);
    });
  });

  // ---- the list ----

  let list = $state<HTMLDivElement>();
  let filterBox = $state<HTMLInputElement>();
  let scrollTop = $state(0);
  let height = $state(400);
  const first = $derived(Math.max(0, Math.floor(scrollTop / ROW_H) - 8));
  const last = $derived(Math.min(rows.length, Math.ceil((scrollTop + height) / ROW_H) + 8));

  onMount(() => {
    if (!list) return;
    const ro = new ResizeObserver(() => list && (height = list.clientHeight));
    ro.observe(list);
    return () => ro.disconnect();
  });

  // Back on a worktree: where its list was.
  $effect(() => {
    const t = tree;
    if (!t || !list) return;
    untrack(() => {
      tick().then(() => {
        if (list) list.scrollTop = scrollTop = t.scroll;
      });
    });
  });

  function onScroll() {
    if (!list) return;
    scrollTop = list.scrollTop;
    if (tree) tree.scroll = scrollTop;
  }

  /** Put the keys on row `i`, scrolled into view and focused. */
  async function moveTo(i: number, focus = true) {
    if (!tree || !rows.length || !list) return;
    const n = Math.max(0, Math.min(rows.length - 1, i));
    tree.cursor = rowKey(rows[n]);
    const top = n * ROW_H;
    if (top < list.scrollTop) list.scrollTop = top;
    else if (top + ROW_H > list.scrollTop + list.clientHeight) list.scrollTop = top + ROW_H - list.clientHeight;
    scrollTop = list.scrollTop;
    if (!focus) return;
    await tick();
    list.querySelector<HTMLElement>(`[data-index="${n}"]`)?.focus();
  }

  // ---- what a row does ----

  const abs = (e: Entry) => (w ? underRoot(w.path, e.path) : e.path);

  function toggle(e: Entry) {
    if (!tree || filtering || !isFolder(e)) return;
    setOpen(tree, e.path, !tree.open.has(e.path));
  }

  /** Enter on a file: Kete's viewer, or why not. */
  function openInKete(e: Entry) {
    if (!w) return;
    const v = fileTable({ worktree: w.id, path: e.path }, { deleted: !!e.deleted }).open_file_in_kete;
    if (!v) return toast(`${e.name} is deleted: there is nothing on disk to open`);
    if ((e.size ?? 0) > VIEWER_MAX_BYTES) return toast(TOO_BIG);
    v.run();
  }

  /** Shift+Enter: the editor the editors' rows tick for it (VS Code for a
   *  folder, or where Kete's viewer is the file default). */
  function openInEditor(e: Entry) {
    if (!w) return;
    const t = { worktree: w.id, path: e.path };
    if (e.kind === 'dir' || e.kind === 'module') {
      const v = folderVerbs(t).open_in_vscode;
      return v ? void v.run() : toast('No VS Code install found: see Settings, Desktop, Editors');
    }
    const table = fileVerbs(t, { deleted: !!e.deleted });
    const code = table.open_in_vscode;
    const pick = [code, ...(code?.also ?? []), table.open_in_visual_studio].find((v) => v?.checked) ?? code;
    if (!pick) return toast(e.deleted ? `${e.name} is deleted: there is nothing on disk to open` : 'No VS Code install found: see Settings, Desktop, Editors');
    pick.run();
  }

  function activate(r: Row, editor = false) {
    if (r.kind === 'more') return void (tree && loadLevel(tree, r.dir, tree.levels.get(r.dir)?.entries.length ?? 0));
    if (r.kind !== 'entry') return;
    const e = r.entry;
    if (editor) return openInEditor(e);
    if (e.kind === 'dir') return toggle(e);
    if (e.kind === 'module') return toast(`${e.name} is a repo of its own: Shift+Enter opens it in your editor`);
    openInKete(e);
  }

  /** The row's menu (File or Folder, menu matrix v2 section 5.7, 5.8). */
  function menuOf(r: Row): Opened | null {
    if (!tree || !w || r.kind !== 'entry') return null;
    const e = r.entry;
    const t = tree;
    if (e.kind === 'dir' || e.kind === 'module') {
      const folds = e.kind === 'dir' && !filtering;
      return {
        items: treeFolderItems(
          w.id,
          e,
          { expand: folds && canExpand(t.levels, t.open, e.path), collapse: folds && canCollapse(t.open, e.path) },
          { expandAll: () => void expandAll(t, e.path), collapseAll: () => collapseAll(t, e.path) },
        ),
        label: e.name,
      };
    }
    const wt = w.id;
    return { items: treeFileItems(wt, e, { diff: () => diffFile(wt, e.path) }), label: e.name };
  }

  // ---- keys ----

  function onTreeKey(e: KeyboardEvent) {
    const el = (e.target as HTMLElement).closest<HTMLElement>('[data-index]');
    if (!tree || !el || !rows.length) return;
    const r = rows[at];
    const entry = r.kind === 'entry' ? r.entry : null;
    const mod = e.ctrlKey || e.altKey || e.metaKey;
    const page = Math.max(1, Math.floor((list?.clientHeight ?? 400) / ROW_H) - 1);
    let done = true;
    if (mod) done = false;
    else if (e.key === 'ArrowDown' || (e.key === 'j' && !e.shiftKey)) void moveTo(at + 1);
    else if (e.key === 'ArrowUp' || (e.key === 'k' && !e.shiftKey)) void moveTo(at - 1);
    else if (e.key === 'PageDown') void moveTo(at + page);
    else if (e.key === 'PageUp') void moveTo(at - page);
    else if (e.key === 'Home') void moveTo(0);
    else if (e.key === 'End') void moveTo(rows.length - 1);
    else if (e.key === 'ArrowRight') {
      // Open a folded folder; on an open one, its first row.
      if (entry?.kind === 'dir' && !filtering && !tree.open.has(entry.path)) setOpen(tree, entry.path, true);
      else if (entry?.kind === 'dir') void moveTo(at + 1);
    } else if (e.key === 'ArrowLeft') {
      // Fold an open folder; anywhere else, up to the folder it is in.
      if (entry?.kind === 'dir' && !filtering && tree.open.has(entry.path)) setOpen(tree, entry.path, false);
      else {
        const up = r.kind === 'entry' ? parentOf(r.entry.path) : r.dir;
        const i = rows.findIndex((x) => x.kind === 'entry' && x.entry.path === up);
        if (up && i >= 0) void moveTo(i);
      }
    } else if (e.key === 'Enter') activate(r, e.shiftKey);
    else if (e.key === ' ' && !e.shiftKey) {
      if (entry?.kind === 'dir') toggle(entry);
    } else if (e.key === '?') {
      const o = menuOf(r);
      if (o) openMenu([...o.items, 'sep', allKeysItem()], el, o.label, { search: true });
    } else if ((e.key === '/' || e.key === 'f') && !e.shiftKey) filterBox?.focus();
    else if (e.key === 'd' && !e.shiftKey && entry && !isFolder(entry) && entry.kind !== 'module' && hasChanges(entry) && w) diffFile(w.id, entry.path);
    else if (e.key === 'r' && !e.shiftKey) void refresh(tree);
    else if (e.key === 'Escape' && filtering) tree.filter = '';
    else done = false;
    if (done) {
      e.preventDefault();
      e.stopPropagation();
    }
  }

  function onFilterKey(e: KeyboardEvent) {
    if (!tree) return;
    if (e.key === 'Escape') {
      e.preventDefault();
      e.stopPropagation();
      if (tree.filter) tree.filter = '';
      else void moveTo(at);
    } else if (e.key === 'Enter' || e.key === 'ArrowDown') {
      e.preventDefault();
      void moveTo(e.key === 'ArrowDown' && shown.best === null ? 0 : at);
    }
  }

  // / and F from anywhere in the view but a field or a row of the list
  // beside it; R reads the tree again.
  function onWindowKey(e: KeyboardEvent) {
    if (nav.current.view !== 'files' || !tree || e.defaultPrevented || e.ctrlKey || e.altKey || e.metaKey || e.shiftKey) return;
    const t = e.target as HTMLElement;
    if (list?.contains(t) || t.closest('input, textarea, select, .monaco-editor, .xterm, [role="dialog"], [role="menu"], [data-board]') || closestTarget(t)) return;
    if (e.key === '/' || e.key === 'f') {
      e.preventDefault();
      filterBox?.focus();
    } else if (e.key === 'r') {
      e.preventDefault();
      void refresh(tree);
    }
  }

  // ---- drawing ----

  /** The name, its matched letters lit. */
  function parts(name: string, positions: number[]): { text: string; hit: boolean }[] {
    if (!positions.length) return [{ text: name, hit: false }];
    const hit = new Set(positions);
    const out: { text: string; hit: boolean }[] = [];
    for (let i = 0; i < name.length; i++) {
      const h = hit.has(i);
      const prev = out[out.length - 1];
      if (prev && prev.hit === h) prev.text += name[i];
      else out.push({ text: name[i], hit: h });
    }
    return out;
  }

  const MARK_TONE: Record<string, string> = {
    C: 'text-git-conflict',
    D: 'text-git-deleted',
    U: 'text-git-added',
    A: 'text-git-added',
    M: 'text-git-modified',
  };
  const DOT_TONE: Record<string, string> = {
    C: 'bg-git-conflict',
    D: 'bg-git-deleted',
    U: 'bg-git-added',
    A: 'bg-git-added',
    M: 'bg-git-modified',
  };

  function rowTitle(e: Entry): string {
    const mark = markOf(e);
    const what = [e.path, mark?.words, e.kind === 'link' ? 'a symbolic link' : '', e.kind === 'module' ? 'a repo of its own' : ''].filter(Boolean).join(', ');
    const how =
      e.kind === 'dir'
        ? 'Click or Enter opens or folds it, Shift+Enter opens it in your editor.'
        : 'Enter or a double-click opens it in Kete, Shift+Enter in your editor.';
    return `${what}\n${how} Drag it onto a session, its card or its terminal to type its path there. Right-click for its menu.`;
  }

  const tool = 'grid size-7 flex-none place-items-center border-0 bg-transparent text-muted hover:bg-surface hover:text-ink disabled:text-faint disabled:hover:bg-transparent';
</script>

<svelte:window onkeydown={onWindowKey} />

<div class="@container flex h-full min-h-0 flex-col bg-bg">
  <div class="flex min-h-10 min-w-0 flex-none flex-wrap items-center gap-x-2 gap-y-1.5 border-b border-rule py-1.5 pr-2 pl-2 text-[13px]">
    <select
      class="h-7 max-w-[240px] min-w-[96px] shrink border border-rule bg-bg px-1.5 font-mono text-[12px] text-ink"
      aria-label="Worktree"
      title="The worktree whose files to show, by repo"
      value={w?.id ?? ''}
      onchange={(e) => pickFilesWorktree((e.target as HTMLSelectElement).value)}
    >
      {#if !w}<option value="" disabled>Pick a worktree</option>{/if}
      {#each groups as g (g.project)}
        <optgroup label={g.name}>
          {#each g.worktrees as x (x.id)}
            <option value={x.id}>{x.branch}{x.isMain ? ' (main checkout)' : ''}</option>
          {/each}
        </optgroup>
      {/each}
    </select>
    {#if tree}
      <input
        bind:this={filterBox}
        bind:value={tree.filter}
        type="search"
        class="h-7 min-w-[120px] flex-1 border border-rule bg-bg px-2 text-[13px] text-ink placeholder:text-faint"
        placeholder="Filter files"
        aria-label="Filter files"
        title="Filter by the path from the checkout's root, as the terminal app's tree does (/ or F). Esc clears it."
        spellcheck="false"
        autocomplete="off"
        onkeydown={onFilterKey}
      />
    {/if}
    <span class="ml-auto flex flex-none items-center">
      <button class={tool} disabled={!tree || tree.busy > 0} onclick={() => tree && refresh(tree)} title="Read the files and their marks again (R)" aria-label="Read the files again">
        <svg width="14" height="14" viewBox="0 0 16 16" aria-hidden="true"
          ><path d="M13 8a5 5 0 1 1-1.5-3.5M13 2.5v3h-3" fill="none" stroke="currentColor" stroke-width="1.4" /></svg
        >
      </button>
      <button
        class={tool}
        disabled={!tree || !tree.open.size || filtering}
        onclick={() => tree && collapseAll(tree, '')}
        title="Collapse all: fold every folder"
        aria-label="Collapse all"
      >
        <svg width="14" height="14" viewBox="0 0 16 16" aria-hidden="true"
          ><path d="M4.5 6.5L8 3l3.5 3.5M4.5 13L8 9.5l3.5 3.5" fill="none" stroke="currentColor" stroke-width="1.4" /></svg
        >
      </button>
      <button
        class="{tool} {app.paneMax ? 'text-ink' : ''}"
        onclick={toggleMaximize}
        title={withKey(app.paneMax ? 'Bring the pane back down' : 'Full pane: give the files the window', ['maximize_z', 'maximize'])}
        aria-label={app.paneMax ? 'Bring the pane back down' : 'Full pane'}
        aria-pressed={app.paneMax}
      >
        <Icon name={app.paneMax ? 'restore' : 'maximize'} size={13} />
      </button>
      <button
        class="ml-1 grid size-7 flex-none place-items-center border border-rule bg-transparent text-ink hover:border-rule-strong"
        onclick={closeFilesView}
        title={withKey('Close the files: back to where they were opened (Esc)', ['files', 'open_files'])}
        aria-label="Close the files"
      >
        <Icon name="close" size={12} />
      </button>
    </span>
  </div>

  {#if !w}
    <div class="max-w-[560px] overflow-y-auto px-6 pt-6 text-[13px]">
      <p class="m-0 text-[15px] font-semibold text-ink">Which worktree?</p>
      <p class="mt-1 mb-4 text-muted">Nothing in view names one. Pick the checkout whose files to show.</p>
      {#each groups as g (g.project)}
        <h2 class="m-0 mt-3 border-b border-rule pb-1 text-[12px] font-normal text-muted">{g.name}</h2>
        <ul class="m-0 list-none p-0">
          {#each g.worktrees as x (x.id)}
            <li>
              <button
                class="flex h-7 w-full items-center gap-2 border-0 border-b border-rule bg-transparent px-2 text-left font-mono text-[12px] text-ink hover:bg-surface"
                onclick={() => pickFilesWorktree(x.id)}
                title="The files of {g.name} / {x.branch}"
                ><span class="min-w-0 truncate">{x.branch}</span>{#if x.isMain}<span class="flex-none text-faint">main checkout</span>{/if}</button
              >
            </li>
          {/each}
        </ul>
      {:else}
        <p class="m-0 text-muted">No worktrees yet. Add a repo first.</p>
      {/each}
    </div>
  {:else if tree}
    {#if tree.error && !tree.levels.has('')}
      <div class="px-4 pt-4 text-[13px]">
        <p class="m-0 text-fail select-text">{tree.error}</p>
        <button class="mt-2 border border-rule bg-transparent px-2 py-0.5 text-ink hover:border-rule-strong" onclick={() => tree && loadLevel(tree, '')}>Try again</button>
      </div>
    {:else if !tree.levels.has('') || (filtering && !tree.all)}
      <p class="m-0 px-4 pt-4 text-[13px] text-muted">Reading the files&hellip;</p>
    {:else if !rows.length}
      <p class="m-0 px-4 pt-4 text-[13px] text-muted">
        {#if filtering}No file matches "{tree.filter.trim()}".{:else}git lists no files in {project?.name ?? 'this checkout'} / {w.branch} yet.{/if}
      </p>
    {/if}
    <div
      bind:this={list}
      class="relative min-h-0 flex-1 overflow-y-auto {rows.length ? '' : 'hidden'}"
      role="tree"
      tabindex="-1"
      aria-label="Files in {project?.name ?? ''} / {w.branch}"
      onscroll={onScroll}
    >
      <div class="relative" style="height: {rows.length * ROW_H}px">
        {#each rows.slice(first, last) as r, k (rowKey(r))}
          {@const i = first + k}
          {@const cur = i === at}
          {#if r.kind === 'entry'}
            {@const e = r.entry}
            {@const folder = e.kind === 'dir' || e.kind === 'module'}
            {@const mark = markOf(e)}
            <div
              role="treeitem"
              aria-level={r.depth + 1}
              aria-expanded={e.kind === 'dir' ? r.open : undefined}
              aria-selected={cur}
              tabindex={cur ? 0 : -1}
              data-index={i}
              data-target="{folder ? 'folder' : 'file'}:{w.id}:{e.path}"
              data-deleted={e.deleted ? 'true' : undefined}
              class="absolute inset-x-0 flex cursor-default items-center gap-1.5 pr-2 text-[13px] outline-none focus-visible:outline-2 focus-visible:-outline-offset-2 focus-visible:outline-rule-strong
                {cur ? 'bg-raised' : 'hover:bg-surface'} {pathDrag.item?.path === e.path ? 'opacity-60' : ''}"
              style="top: {i * ROW_H}px; height: {ROW_H}px; padding-left: {8 + r.depth * INDENT}px"
              title={rowTitle(e)}
              draggable="true"
              ondragstart={(ev) => w && startPathDrag(ev, { worktree: w.id, path: e.path, abs: abs(e), folder })}
              ondragend={endPathDrag}
              onclick={() => {
                void moveTo(i);
                if (e.kind === 'dir') toggle(e);
              }}
              ondblclick={() => !folder && openInKete(e)}
              onkeydown={onTreeKey}
              use:ownMenu={() => menuOf(r)}
            >
              {#each { length: r.depth } as _, lvl (lvl)}
                <span class="pointer-events-none absolute inset-y-0 w-px bg-rule" style="left: {14 + lvl * INDENT}px" aria-hidden="true"></span>
              {/each}
              {#if e.kind === 'dir'}
                <svg width="10" height="10" viewBox="0 0 10 10" aria-hidden="true" class="flex-none text-muted"
                  ><path d={r.open ? 'M1.5 3.5L5 7l3.5-3.5' : 'M3.5 1.5L7 5 3.5 8.5'} fill="none" stroke="currentColor" stroke-width="1.4" /></svg
                >
              {:else}
                <span class="w-2.5 flex-none" aria-hidden="true"></span>
              {/if}
              <img class="flex-none" src={folder ? folderIcon(r.open) : fileIcon(e.name, app.dark)} alt="" width="16" height="16" draggable="false" />
              <span class="min-w-0 truncate {e.deleted ? 'text-muted line-through decoration-git-deleted' : 'text-ink'}"
                >{#each parts(e.name, r.positions) as p, pi (pi)}<span class={p.hit ? 'font-semibold underline underline-offset-2' : ''}>{p.text}</span>{/each}</span
              >
              {#if e.kind === 'link'}<span class="flex-none font-mono text-[11px] text-faint" aria-label="symbolic link">-&gt;</span>{/if}
              {#if e.kind === 'module'}<span class="flex-none text-[11px] text-faint">repo</span>{/if}
              {#if mark}
                {#if folder}
                  <span class="ml-auto size-1.5 flex-none {DOT_TONE[mark.letter]}" aria-label={mark.words}></span>
                {:else}
                  <span class="ml-auto w-3 flex-none text-center font-mono text-[12px] font-bold {MARK_TONE[mark.letter]}" aria-label={mark.words}>{mark.letter}</span>
                {/if}
              {/if}
            </div>
          {:else if r.kind === 'more'}
            <div
              role="treeitem"
              aria-level={r.depth + 1}
              aria-selected={cur}
              tabindex={cur ? 0 : -1}
              data-index={i}
              class="absolute inset-x-0 flex cursor-default items-center pr-2 text-[12px] text-muted outline-none hover:bg-surface hover:text-ink focus-visible:outline-2 focus-visible:-outline-offset-2 focus-visible:outline-rule-strong
                {cur ? 'bg-raised' : ''}"
              style="top: {i * ROW_H}px; height: {ROW_H}px; padding-left: {8 + r.depth * INDENT + 22}px"
              title="This folder has more rows than the tree lists at once: Enter or a click reads the next ones"
              onclick={() => (void moveTo(i), activate(r))}
              onkeydown={onTreeKey}
            >
              {r.count === null ? 'More' : `${r.count.toLocaleString()} more`}&hellip;
            </div>
          {:else}
            <div
              role="treeitem"
              aria-level={r.depth + 1}
              aria-selected={cur}
              aria-busy="true"
              tabindex={cur ? 0 : -1}
              data-index={i}
              class="absolute inset-x-0 flex items-center text-[12px] text-faint outline-none"
              style="top: {i * ROW_H}px; height: {ROW_H}px; padding-left: {8 + r.depth * INDENT + 22}px"
            >
              Reading&hellip;
            </div>
          {/if}
        {/each}
      </div>
    </div>
    {#if filtering && tree.all}
      <p class="m-0 flex-none border-t border-rule px-3 py-1 text-[12px] text-muted" role="status">
        {shown.matches === 1 ? '1 file matches' : `${(shown.matches ?? 0).toLocaleString()} files match`}{tree.allMore
          ? ` in the first ${tree.all.length.toLocaleString()} rows (${tree.allMore.toLocaleString()} more not searched)`
          : ''}
      </p>
    {/if}
  {/if}
</div>
