<!--
  One diff, embeddable anywhere: the Diff view, a branch's range, the PR
  composer's preview. Props:
  - `worktree`: the worktree id every command reads in.
  - `range`: base and tip (refs or SHAs) for a range diff; absent, the
    working tree against its base or HEAD (`against`).
  - `against`: the working tree against the merge-base with the base
    branch (`base`: an agent's commits stay in view) or HEAD (`head`, with
    per-hunk Stage, Unstage and Discard); null, the base when there is
    one. Bindable: the header's toggle sets it.
  - `shown`: what the last load read against, and the base there is;
    bindable, for the palette's entry.
  - `mode`: side, inline or replacement; bindable.
  - `keys`: answer the view's keys (j/k, F7) from anywhere in the window,
    not only when focus is inside the pane.
  - `lead`: a snippet drawn at the left of the toolbar.
  - `trail`: a snippet drawn at the right end of the toolbar (a close button).
  - `pick`: drawn in place of the diff while there is no worktree (a picker).
  - `session`: the session a review is of. With more than one session's
    snapshots in the worktree, the header offers "This session / All in
    this worktree" (L); all by default.
-->
<script lang="ts">
  import { onDestroy, onMount, tick, type Snippet } from 'svelte';
  import { app, toast } from './store.svelte';
  import { diffApi, type Blob, type DiffFile, type DiffMode, type DiffRange, type DiffSide, type Hunk, type HunkAction } from './diff-api';
  import {
    againstLabel,
    againstTitle,
    canStage,
    sessionOffered,
    sessionReadable,
    sessionTitle,
    type Against,
    type ReviewBase,
    type ReviewCommit,
    type SessionDiff,
  } from './diff-base';
  import { fileIcon, languageOf } from './icons';
  import { blockLine, replacementModel, type ReplacementModel } from './replacement';
  import { defineMonacoTheme, termFamily, token } from './theme';
  import { joinPath, registerViewer, type ViewerSurface } from './menus/surfaces';
  import type * as Monaco from 'monaco-esm/editor/editor.api.js';

  let {
    worktree,
    range = null,
    mode = $bindable('side'),
    against = $bindable(null),
    shown: shownOut = $bindable(null),
    keys = false,
    lead,
    trail,
    pick,
    session = null,
  }: {
    worktree: string | null;
    range?: DiffRange | null;
    mode?: DiffMode;
    against?: Against | null;
    shown?: { against: Against; base: ReviewBase | null } | null;
    keys?: boolean;
    lead?: Snippet;
    trail?: Snippet;
    pick?: Snippet;
    session?: string | null;
  } = $props();

  /** `absent`: the status says this side has no file, so why. */
  type Side = { label: string; blob: Blob; absent?: string; which: 'old' | 'new' };
  /** A side that is not text, or text that should be there and is not. */
  type Problem = { path: string; sides: Side[] };

  let monaco: typeof Monaco | null = null;
  let diffHost: HTMLDivElement;
  let repHost: HTMLDivElement;
  let diffEditor: Monaco.editor.IStandaloneDiffEditor | null = null;
  let repEditor: Monaco.editor.IStandaloneCodeEditor | null = null;
  let repDecorations: Monaco.editor.IEditorDecorationsCollection | null = null;
  let ready = $state(false);

  let files = $state<DiffFile[]>([]);
  let current = $state<DiffFile | null>(null);
  let loading = $state(false);
  let error = $state<string | null>(null);
  let offline = $state(false);
  /** The SHAs a range diff is pinned to. */
  let pinned = $state<{ base: string; tip: string } | null>(null);
  let problem = $state<Problem | null>(null);
  let fileError = $state<string | null>(null);
  let showFiles = $state(true);
  /** What the working tree was read against, its base, and the commits
   *  made since that base (read-only, newest first). */
  let readAgainst = $state<Against>('head');
  let base = $state<ReviewBase | null>(null);
  let commits = $state<ReviewCommit[]>([]);
  let showCommits = $state(true);
  /** The reviewed session's snapshots, and whether only its changes show. */
  let sessionDiff = $state<SessionDiff | null>(null);
  let onlySession = $state(false);
  const offerSession = $derived(!range && !!session && sessionOffered(sessionDiff));

  /** Change stops in the shown file, as modified-side lines. */
  let stops = $state<number[]>([]);
  let stopAt = $state(-1);
  let rep: ReplacementModel | null = null;
  /** Where to land once the next file's changes are known. */
  let landOn: 'first' | 'last' | null = null;

  let hunks = $state<{ unstaged: Hunk[]; staged: Hunk[]; note: string | null }>({ unstaged: [], staged: [], note: null });
  let confirming = $state<string | null>(null);
  let busy = $state(false);

  /** Hunks stage and discard: a working tree read against HEAD. */
  const working = $derived(canStage(!!range, readAgainst) && !onlySession);
  /** The working tree against the merge-base: the old side is that commit. */
  const sinceBase = $derived(!range && readAgainst === 'base' && !!base);
  const totals = $derived(
    files.reduce((t, f) => ({ add: t.add + (f.added ?? 0), del: t.del + (f.removed ?? 0) }), { add: 0, del: 0 }),
  );
  const short = (sha: string) => (/^[0-9a-f]{40,64}$/.test(sha) ? sha.slice(0, 7) : sha);

  // ---- loading ------------------------------------------------------------

  /** Bumped per load and per pick: an answer for an older one is dropped. */
  let loadSeq = 0;
  let pickSeq = 0;

  export async function refresh() {
    await load(true);
  }

  async function load(keep: boolean) {
    const wt = worktree;
    const r = range;
    const asked = against;
    const mine = ++loadSeq;
    if (!wt) {
      files = [];
      base = null;
      commits = [];
      await show(null);
      return;
    }
    loading = true;
    error = null;
    try {
      let list: DiffFile[];
      if (r) {
        const res = await diffApi.range(wt, r.base, r.tip, r.pr);
        if (mine !== loadSeq) return;
        pinned = { base: res.base, tip: res.tip };
        offline = res.offline;
        list = res.files;
        base = null;
        commits = [];
      } else {
        const who = session;
        const [res, sess] = await Promise.all([
          diffApi.review(wt, asked),
          who ? diffApi.session(wt, who).catch(() => null) : Promise.resolve(null),
        ]);
        if (mine !== loadSeq) return;
        sessionDiff = sess;
        offline = false;
        readAgainst = res.against;
        base = res.base;
        commits = res.commits;
        shownOut = { against: res.against, base: res.base };
        if (onlySession && sess && sessionReadable(sess)) {
          // The session's own pair: its start snapshot to its last.
          pinned = { base: sess.start!, tip: sess.last! };
          list = sess.files;
        } else {
          if (onlySession) onlySession = false;
          pinned = null;
          list = res.files;
        }
      }
      files = list;
      const again = keep && current ? list.find((f) => f.path === current!.path) : undefined;
      await show(again ?? list[0] ?? null, !!again);
    } catch (e) {
      if (mine !== loadSeq) return;
      error = String(e);
      files = [];
      pinned = null;
      commits = [];
      await show(null);
    } finally {
      if (mine === loadSeq) loading = false;
    }
  }

  function sidesOf(f: DiffFile): { old: DiffSide | null; new: DiffSide | null; oldLabel: string; newLabel: string } {
    if (pinned) {
      return {
        old: f.status === 'A' ? null : { kind: 'commit', rev: pinned.base },
        new: f.status === 'D' ? null : { kind: 'commit', rev: pinned.tip },
        oldLabel: onlySession ? `Session start ${short(pinned.base)}` : `Base ${short(pinned.base)}`,
        newLabel: onlySession ? `Last turn ${short(pinned.tip)}` : `Tip ${short(pinned.tip)}`,
      };
    }
    // Against the base the old side is the merge-base commit, the new one
    // the disk: what was committed since and what is not yet, together.
    const from = sinceBase && base ? base.sha : 'HEAD';
    return {
      old: f.status === 'A' || f.status === 'U' ? null : { kind: 'commit', rev: from },
      new: f.status === 'D' ? null : { kind: 'worktree' },
      oldLabel: sinceBase && base ? `${base.branch} ${base.short}` : 'HEAD',
      newLabel: 'Working tree',
    };
  }

  /** Missing where the status says nothing is there is an empty side, as
   *  `git::file_pair` reads it; anywhere else it is a problem to show. */
  function expectedMissing(f: DiffFile, which: 'old' | 'new'): boolean {
    if (which === 'old') return f.status === 'C';
    return !pinned && (f.status === 'A' || f.status === 'R' || f.status === 'C');
  }

  async function show(f: DiffFile | null, keepView = false) {
    const mine = ++pickSeq;
    if (!keepView) hunks = { unstaged: [], staged: [], note: null };
    current = f;
    problem = null;
    fileError = null;
    confirming = null;
    stops = [];
    stopAt = -1;
    if (!f || !worktree) {
      landOn = null;
      setText(null);
      hunks = { unstaged: [], staged: [], note: null };
      return;
    }
    const wt = worktree;
    const s = sidesOf(f);
    const read = (side: DiffSide | null, path: string): Promise<Blob> =>
      side ? diffApi.blob(wt, side, path) : Promise.resolve({ state: 'text', value: '' });
    try {
      const [oldBlob, newBlob] = await Promise.all([read(s.old, f.from ?? f.path), read(s.new, f.path)]);
      if (mine !== pickSeq) return;
      const o: Blob = oldBlob.state === 'missing' && expectedMissing(f, 'old') ? { state: 'text', value: '' } : oldBlob;
      const n: Blob = newBlob.state === 'missing' && expectedMissing(f, 'new') ? { state: 'text', value: '' } : newBlob;
      if (o.state === 'text' && n.state === 'text') {
        setText({ old: o.value, new: n.value, path: f.path }, keepView);
      } else {
        problem = {
          path: f.path,
          sides: [
            { label: s.oldLabel, blob: o, absent: s.old ? undefined : 'Added here, so there is no earlier copy.', which: 'old' },
            { label: s.newLabel, blob: n, absent: s.new ? undefined : 'Deleted here.', which: 'new' },
          ],
        };
        // No text, so no changes to land on.
        landOn = null;
        setText(null);
      }
    } catch (e) {
      if (mine !== pickSeq) return;
      fileError = String(e);
      landOn = null;
      setText(null);
    }
    if (working) loadHunks(f, mine);
    else hunks = { unstaged: [], staged: [], note: null };
  }

  async function loadHunks(f: DiffFile, mine: number) {
    // Too large or binary: git has no hunks to offer, so offer none.
    if (problem?.sides.some((s) => s.blob.state === 'tooLarge' || s.blob.state === 'binary')) {
      hunks = { unstaged: [], staged: [], note: 'No hunks: this file has no text to split.' };
      return;
    }
    if (f.status === 'U') {
      hunks = { unstaged: [], staged: [], note: 'Untracked, so there are no hunks yet. Add the file whole first.' };
      return;
    }
    try {
      const [unstaged, staged] = await Promise.all([
        diffApi.hunks(worktree!, f.path, false),
        diffApi.hunks(worktree!, f.path, true),
      ]);
      if (mine !== pickSeq) return;
      const none = !unstaged.length && !staged.length;
      hunks = { unstaged, staged, note: none && problem ? 'No hunks: this file has no text to split.' : null };
    } catch (e) {
      if (mine !== pickSeq) return;
      hunks = { unstaged: [], staged: [], note: String(e) };
    }
  }

  // ---- editors ------------------------------------------------------------

  let shown: { old: string; new: string; path: string } | null = null;

  function setText(t: { old: string; new: string; path: string } | null, keepView = false) {
    shown = t;
    if (!monaco || !diffEditor) return;
    const lang = t ? languageOf(t.path) : 'plaintext';
    // Diff editor (side by side, inline).
    const oldPair = diffEditor.getModel();
    const view = keepView ? diffEditor.getModifiedEditor().saveViewState() : null;
    diffEditor.setModel(
      t ? { original: monaco.editor.createModel(t.old, lang), modified: monaco.editor.createModel(t.new, lang) } : null,
    );
    if (view) diffEditor.getModifiedEditor().restoreViewState(view);
    oldPair?.original.dispose();
    oldPair?.modified.dispose();
    // Replacement: the new text alone.
    const oldRep = repEditor?.getModel();
    rep = t ? replacementModel(t.old, t.new) : null;
    if (repEditor) {
      const repView = keepView ? repEditor.saveViewState() : null;
      repEditor.setModel(t ? monaco.editor.createModel(t.new, lang) : null);
      decorateReplacement();
      if (repView) repEditor.restoreViewState(repView);
    }
    oldRep?.dispose();
    if (mode === 'replacement') syncStops();
  }

  function decorateReplacement() {
    if (!monaco || !repEditor || !repDecorations) return;
    const model = repEditor.getModel();
    if (!model || !rep) {
      repDecorations.clear();
      return;
    }
    const M = monaco;
    const ruler = (name: string) => ({ color: token(name), position: M.editor.OverviewRulerLane.Full });
    const fence = (lines: string[]) => ({ value: '```\n' + lines.join('\n') + '\n```' });
    const d: Monaco.editor.IModelDeltaDecoration[] = [];
    for (const b of rep.blocks) {
      if (b.end < b.start) continue;
      d.push({
        range: new M.Range(b.start, 1, b.end, 1),
        options: {
          isWholeLine: true,
          className: 'kete-rep-line',
          linesDecorationsClassName: 'kete-rep-gutter',
          overviewRuler: ruler('diff-add-word'),
          hoverMessage: b.old.length ? [{ value: `Was ${b.old.length === 1 ? '1 line' : `${b.old.length} lines`}:` }, fence(b.old)] : { value: 'New lines' },
        },
      });
    }
    for (const s of rep.spans) {
      d.push({ range: new M.Range(s.line, s.startColumn, s.line, s.endColumn), options: { inlineClassName: 'kete-rep-word' } });
    }
    const last = model.getLineCount();
    for (const r of rep.removed) {
      const n = r.lines.length;
      const word = `${n === 1 ? '1 line' : `${n} lines`} removed`;
      const top = r.afterLine === 0;
      const line = top ? 1 : Math.min(r.afterLine, last);
      const col = top ? 1 : model.getLineMaxColumn(line);
      d.push({
        range: new M.Range(line, col, line, col),
        options: {
          // An empty range draws nothing unless asked to.
          showIfCollapsed: true,
          [top ? 'before' : 'after']: { content: word, inlineClassName: top ? 'kete-rep-removed kete-rep-removed-top' : 'kete-rep-removed' },
          overviewRuler: ruler('diff-del-word'),
          hoverMessage: [{ value: `Removed ${n === 1 ? '1 line' : `${n} lines`}:` }, fence(r.lines)],
        },
      });
    }
    repDecorations.set(d);
  }

  /** Change stops for the mode on screen, as modified-side lines. */
  function syncStops() {
    if (mode === 'replacement') {
      const count = repEditor?.getModel()?.getLineCount() ?? 1;
      stops = rep ? rep.blocks.map((b) => blockLine(b, count)) : [];
    } else {
      const changes = diffEditor?.getLineChanges();
      // Null: the diff is not computed yet, so keep where to land for when it is.
      if (!changes) {
        stops = [];
        stopAt = -1;
        return;
      }
      stops = changes.map((c) => Math.max(1, c.modifiedEndLineNumber === 0 ? c.modifiedStartLineNumber + 1 : c.modifiedStartLineNumber));
    }
    stopAt = -1;
    if (!landOn) return;
    const where = landOn;
    // Spent either way: a file with no stops has nowhere to land.
    landOn = null;
    if (stops.length) goStop(where === 'first' ? 0 : stops.length - 1);
  }

  function goStop(i: number) {
    const line = stops[i];
    if (line === undefined) return;
    stopAt = i;
    reveal(line);
  }

  function reveal(line: number) {
    const ed = mode === 'replacement' ? repEditor : diffEditor?.getModifiedEditor();
    if (!ed) return;
    ed.revealLineInCenter(line);
    ed.setPosition({ lineNumber: line, column: 1 });
  }

  /** Next or previous change; past either end, the next or previous file. */
  export function step(d: 1 | -1) {
    if (!current) return;
    const pos = (mode === 'replacement' ? repEditor : diffEditor?.getModifiedEditor())?.getPosition()?.lineNumber ?? 0;
    let i: number;
    if (stopAt >= 0 && stops[stopAt] === pos) i = stopAt + d;
    else if (d > 0) i = stops.findIndex((l) => l > pos);
    else i = stops.reduce((at, l, j) => (l < pos ? j : at), -1);
    if (i >= 0 && i < stops.length) return goStop(i);
    const fi = files.findIndex((f) => f.path === current!.path) + d;
    if (fi < 0 || fi >= files.length) return;
    landOn = d > 0 ? 'first' : 'last';
    show(files[fi]);
    scrollFile();
  }

  export function moveFile(d: number) {
    if (!files.length) return;
    const i = current ? files.findIndex((f) => f.path === current!.path) : -1;
    const next = files[Math.max(0, Math.min(files.length - 1, i + d))];
    if (next.path !== current?.path) show(next);
    scrollFile();
  }

  function scrollFile() {
    queueMicrotask(() => list?.querySelector('[data-file-cur]')?.scrollIntoView({ block: 'nearest' }));
  }

  // ---- hunks --------------------------------------------------------------

  const hunkKey = (staged: boolean, h: Hunk) => `${staged ? 's' : 'u'}${h.header}`;

/** The first added or removed line of an unstaged hunk, on the working-tree side. */
  function firstChange(h: Hunk): number {
    const body = h.patch.slice(h.patch.indexOf('\n@@') + 1).split('\n').slice(1);
    let line = h.newStart;
    for (const l of body) {
      if (l.startsWith('+') || l.startsWith('-')) return Math.max(1, line);
      if (l.startsWith(' ')) line++;
    }
    return Math.max(1, h.newStart);
  }

  function hunkLabel(h: Hunk) {
    const m = /^@@ -(\S+) \+(\S+) @@ ?(.*)$/.exec(h.header);
    return m ? { range: `\u2212${m[1]} +${m[2]}`, context: m[3] } : { range: h.header, context: '' };
  }

  /** Open or close a hunk's Discard question. Open, focus sits on Keep, the safe answer. */
  function askDiscard(k: string) {
    confirming = confirming === k ? null : k;
    if (confirming) tick().then(() => list?.querySelector<HTMLButtonElement>('[data-keep]')?.focus());
  }

  /** Escape answers Keep, and focus goes back to the hunk's Discard. */
  function onConfirmKey(e: KeyboardEvent, k: string) {
    if (e.key !== 'Escape') return;
    // Ahead of the window's Escape, which closes the split.
    e.preventDefault();
    e.stopPropagation();
    keep(k);
  }

  function keep(k: string) {
    confirming = null;
    tick().then(() => list?.querySelector<HTMLButtonElement>(`[data-discard="${CSS.escape(k)}"]`)?.focus());
  }

  /** The header's toggle and the palette's entry: the base, or HEAD. */
  export function toggleAgainst() {
    if (!base || range) return;
    onlySession = false;
    against = readAgainst === 'base' ? 'head' : 'base';
  }

  /** The header's session filter (L): this session's changes, or all. */
  export function setSessionOnly(only: boolean) {
    if (!offerSession || only === onlySession) return;
    if (only && !sessionReadable(sessionDiff)) {
      toast(sessionDiff?.note ?? 'No snapshot of this session yet.');
      return;
    }
    onlySession = only;
  }

  async function act(h: Hunk, action: HunkAction) {
    if (!worktree || busy) return;
    busy = true;
    confirming = null;
    try {
      await diffApi.applyHunk(worktree, h, action);
      await load(true);
    } catch (e) {
      toast(String(e));
    } finally {
      busy = false;
    }
  }

  // ---- keys ---------------------------------------------------------------

  /** F7 belongs to the pane, ahead of Monaco's own binding. */
  function onPaneKey(e: KeyboardEvent) {
    if (e.key !== 'F7' || e.ctrlKey || e.altKey || e.metaKey) return;
    e.preventDefault();
    e.stopPropagation();
    step(e.shiftKey ? -1 : 1);
  }

  function onWindowKey(e: KeyboardEvent) {
    if (!keys || e.defaultPrevented) return;
    // Typing, a terminal or another editor keeps its keys, F7 included.
    // F7 inside this pane's own editors never gets here (onPaneKey).
    const t = e.target as HTMLElement;
    if (t.closest('input, textarea, select, .monaco-editor, .xterm')) return;
    // The board beside a review keeps its own arrows and j/k.
    if (t.closest('[data-board]')) return;
    if (e.key === 'F7' && !e.ctrlKey && !e.altKey && !e.metaKey) {
      e.preventDefault();
      return step(e.shiftKey ? -1 : 1);
    }
    if (e.ctrlKey || e.altKey || e.metaKey) return;
    if (e.key === 'ArrowDown' || e.key === 'j') (e.preventDefault(), moveFile(1));
    else if (e.key === 'ArrowUp' || e.key === 'k') (e.preventDefault(), moveFile(-1));
    else if (e.key === 'f') showFiles = !showFiles;
    else if (e.key === 'l' && offerSession) (e.preventDefault(), setSessionOnly(!onlySession));
  }

  // ---- lifecycle ----------------------------------------------------------

  let list: HTMLUListElement | undefined = $state();

  onMount(async () => {
    await document.fonts.load('12px ' + termFamily()).catch(() => {});
    const m = (await import('./monaco')) as typeof import('./monaco');
    monaco = m.monaco;
    defineMonacoTheme(monaco, app.dark);
    const base = m.readingOptions(termFamily());
    diffEditor = monaco.editor.createDiffEditor(diffHost, {
      ...base,
      originalEditable: false,
      renderSideBySide: mode !== 'inline',
      useInlineViewWhenSpaceIsLimited: false,
      renderOverviewRuler: true,
      hideUnchangedRegions: { enabled: true, contextLineCount: 3, minimumLineCount: 4 },
      ignoreTrimWhitespace: false,
      renderIndicators: true,
      renderMarginRevertIcon: false,
    });
    diffEditor.onDidUpdateDiff(() => {
      if (mode !== 'replacement') syncStops();
    });
    repEditor = monaco.editor.create(repHost, { ...base, glyphMargin: false, folding: false, lineDecorationsWidth: 8 });
    repDecorations = repEditor.createDecorationsCollection();
    // The viewers' right-click menu (menus/builtin.ts): which editor, and
    // the shown file on disk.
    const viewer: ViewerSurface = {
      editorAt: (el) => {
        const sides: { editor: Monaco.editor.ICodeEditor; old: boolean }[] = diffEditor
          ? [
              { editor: diffEditor.getOriginalEditor(), old: true },
              { editor: diffEditor.getModifiedEditor(), old: false },
            ]
          : [];
        if (repEditor) sides.push({ editor: repEditor, old: false });
        return sides.find((s) => s.editor.getDomNode()?.contains(el)) ?? null;
      },
      path: () => {
        const root = app.model.worktrees.find((w) => w.id === worktree)?.path;
        return root && shown ? joinPath(root, shown.path) : null;
      },
    };
    unregisterViewers = [registerViewer(diffHost, viewer), registerViewer(repHost, viewer)];
    ready = true;
  });
  let unregisterViewers: (() => void)[] = [];

  let loadedFor = '';
  // Another session (or none): the whole worktree again.
  let sessionFor: string | null = null;
  $effect(() => {
    const s = session;
    if (s !== sessionFor) {
      sessionFor = s;
      onlySession = false;
      sessionDiff = null;
    }
  });
  $effect(() => {
    const k = `${worktree ?? ''}\u0000${range?.base ?? ''}\u0000${range?.tip ?? ''}\u0000${range ? '' : (against ?? '')}\u0000${session ?? ''}\u0000${onlySession}`;
    if (ready && k !== loadedFor) {
      loadedFor = k;
      current = null;
      load(false);
    }
  });
  $effect(() => {
    const m = mode;
    if (!ready) return;
    if (m !== 'replacement') diffEditor?.updateOptions({ renderSideBySide: m === 'side' });
    // The hidden editor has no size; lay out the shown one once visible.
    requestAnimationFrame(() => {
      if (m === 'replacement') repEditor?.layout();
      else diffEditor?.layout();
      syncStops();
    });
  });
  $effect(() => {
    const dark = app.dark;
    requestAnimationFrame(() => {
      if (!monaco) return;
      defineMonacoTheme(monaco, dark);
      decorateReplacement();
    });
  });

  onDestroy(() => {
    for (const off of unregisterViewers) off();
    const pair = diffEditor?.getModel();
    const one = repEditor?.getModel();
    diffEditor?.dispose();
    repEditor?.dispose();
    pair?.original.dispose();
    pair?.modified.dispose();
    one?.dispose();
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
  const MODES: { id: DiffMode; label: string; key: string; icon: string }[] = [
    { id: 'side', label: 'Side by side', key: 'S', icon: 'M8 2.5v11' },
    { id: 'inline', label: 'Inline', key: 'I', icon: 'M4 6.5h8M4 9.5h8' },
    { id: 'replacement', label: 'Replacement', key: 'E', icon: 'M4 5.5h8M4 8h5M4 10.5h8' },
  ];

  function blobWord(b: Blob, which: 'old' | 'new'): { word: string; detail: string; tone: string } {
    switch (b.state) {
      case 'missing':
        return {
          word: 'Missing',
          // A range reads commits, so a missing blob is a ref not fetched yet.
          detail: pinned
            ? `The file is missing on the ${which === 'old' ? 'base' : 'source'} side. Fetch and reopen.`
            : 'The file is not on this side.',
          tone: 'bg-fail',
        };
      case 'tooLarge':
        return { word: 'Too large', detail: `${(b.value / (1024 * 1024)).toFixed(1)} MB, over the 2 MB limit.`, tone: 'bg-faint' };
      case 'binary':
        return { word: 'Binary', detail: 'Not text, so there is nothing to compare line by line.', tone: 'bg-faint' };
      default: {
        // A trailing newline ends the last line; it does not start another.
        const n = b.value === '' ? 0 : b.value.split('\n').length - (b.value.endsWith('\n') ? 1 : 0);
        return { word: 'Text', detail: `${n === 1 ? '1 line' : `${n} lines`}.`, tone: 'bg-done' };
      }
    }
  }
</script>

<svelte:window onkeydown={onWindowKey} />

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div class="@container flex h-full min-h-0 flex-col" onkeydowncapture={onPaneKey}>
  <!-- Too narrow for one row (the split's narrowest pane), the tools wrap to a second. -->
  <div class="flex min-h-10 min-w-0 flex-none flex-wrap items-center gap-x-2 gap-y-1.5 border-b border-rule py-1.5 pr-2 pl-2 text-[13px] @min-[720px]:gap-x-3">
    <button
      class="grid size-7 flex-none place-items-center border-0 bg-transparent text-muted hover:bg-surface hover:text-ink {showFiles ? '' : 'text-ink'}"
      onclick={() => (showFiles = !showFiles)}
      title="{showFiles ? 'Hide' : 'Show'} the file list (F)"
      aria-label="{showFiles ? 'Hide' : 'Show'} the file list"
      aria-pressed={showFiles}
    >
      <svg width="14" height="14" viewBox="0 0 16 16" aria-hidden="true"
        ><rect x="1.5" y="2.5" width="13" height="11" fill="none" stroke="currentColor" stroke-width="1.3" /><path
          d="M6 2.5v11"
          stroke="currentColor"
          stroke-width="1.3"
        />{#if showFiles}<path d="M2 3h4v10H2z" fill="currentColor" />{/if}</svg
      >
    </button>
    {@render lead?.()}
    {#if !range && worktree}
      <!-- Against the base or HEAD: one button, the word says which is read. -->
      <button
        class="h-7 max-w-[260px] min-w-[64px] shrink truncate border border-rule bg-transparent px-2 font-mono text-[12px] whitespace-nowrap
          {base ? 'text-ink enabled:hover:border-rule-strong' : 'cursor-default text-muted'}"
        disabled={loading}
        aria-disabled={!base}
        onclick={toggleAgainst}
        title="{againstTitle(readAgainst, base)}{base ? ` (also in the command palette)` : ''}"
        aria-label="{againstLabel(readAgainst, base)}. {base ? 'Switch the comparison' : 'No base branch'}"
        aria-pressed={readAgainst === 'base'}
        >{againstLabel(readAgainst, base)}</button
      >
    {/if}
    {#if offerSession}
      <!-- A review of one session in a worktree several worked in: its own
           changes (start to last turn), or everyone's. -->
      <div class="flex h-7 flex-none border border-rule" role="radiogroup" aria-label="Whose changes">
        {#each [{ id: 'all', long: 'All in this worktree', word: 'All' }, { id: 'session', long: 'This session', word: 'Session' }] as const as c, i (c.id)}
          {@const on = (c.id === 'session') === onlySession}
          <button
            role="radio"
            aria-checked={on}
            class="inline-flex items-center border-0 px-2 text-[12px] whitespace-nowrap
              {i ? 'border-l border-rule' : ''} {on ? 'bg-ink text-bg' : 'bg-transparent text-muted hover:text-ink'}"
            disabled={loading}
            onclick={() => setSessionOnly(c.id === 'session')}
            title={sessionTitle(c.id, sessionDiff)}
            aria-label={c.long}
            ><span class="@min-[720px]:hidden">{c.word}</span><span class="hidden @min-[720px]:inline">{c.long}</span></button
          >
        {/each}
      </div>
    {/if}
    <!-- The range and totals give way first when the pane is narrow. -->
    <span class="flex min-w-0 flex-auto items-center gap-3 overflow-hidden">
      {#if pinned}
        <span
          class="min-w-0 truncate font-mono text-[12px] whitespace-nowrap text-ink"
          title={onlySession ? `Session start ${pinned.base}, last turn ${pinned.tip}` : `Merge-base ${pinned.base}, tip ${pinned.tip}`}
          >{short(pinned.base)}<span class="text-faint">...</span>{short(pinned.tip)}</span
        >
      {/if}
      {#if files.length}
        <span class="min-w-0 truncate font-mono text-[12px] whitespace-nowrap text-muted"
          >{files.length} {files.length === 1 ? 'file' : 'files'}
          <span class="text-git-added">+{totals.add}</span> <span class="text-git-deleted">&minus;{totals.del}</span></span
        >
      {/if}
    </span>
    {#if loading}<span class="font-mono text-[12px] text-muted" role="status">Loading</span>{/if}
    {#if !showFiles && working && hunks.unstaged.length + hunks.staged.length}
      <!-- Stage, Unstage and Discard live under the file in the list: with
           the list hidden, say they are there and bring it back. -->
      <button
        class="h-7 flex-none border border-rule bg-transparent px-2 font-mono text-[12px] whitespace-nowrap text-ink hover:border-rule-strong"
        title="Stage, unstage or discard this file's hunks: shows the file list (F)"
        onclick={() => (showFiles = true)}
        >{hunks.unstaged.length + hunks.staged.length} {hunks.unstaged.length + hunks.staged.length === 1 ? 'hunk' : 'hunks'}</button
      >
    {/if}
    <div class="flex h-7 flex-none items-center border border-rule" aria-label="Changes">
      <button
        class="grid h-full w-7 place-items-center border-0 bg-transparent text-muted enabled:hover:bg-surface enabled:hover:text-ink disabled:text-faint"
        onclick={() => step(-1)}
        disabled={!current}
        title="Previous change (Shift+F7)"
        aria-label="Previous change"
      >
        <svg width="12" height="12" viewBox="0 0 16 16" aria-hidden="true"
          ><path d="M3.5 10L8 5.5l4.5 4.5" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="square" /></svg
        >
      </button>
      <span class="hidden min-w-[74px] border-x border-rule px-2 text-center font-mono text-[12px] leading-[26px] text-muted @min-[620px]:block" aria-live="polite"
        >{#if !current}&nbsp;{:else if !stops.length}no changes{:else if stopAt >= 0}{stopAt + 1} of {stops.length}{:else}{stops.length}
          {stops.length === 1 ? 'change' : 'changes'}{/if}</span
      >
      <button
        class="grid h-full w-7 place-items-center border-0 border-l border-rule bg-transparent @min-[620px]:border-l-0 text-muted enabled:hover:bg-surface enabled:hover:text-ink disabled:text-faint"
        onclick={() => step(1)}
        disabled={!current}
        title="Next change (F7)"
        aria-label="Next change"
      >
        <svg width="12" height="12" viewBox="0 0 16 16" aria-hidden="true"
          ><path d="M3.5 6L8 10.5 12.5 6" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="square" /></svg
        >
      </button>
    </div>
    <div class="flex h-7 flex-none border border-rule" role="radiogroup" aria-label="Diff layout">
      {#each MODES as m, i (m.id)}
        {@const on = mode === m.id}
        <button
          role="radio"
          aria-checked={on}
          class="inline-flex items-center gap-1.5 border-0 px-2 text-[12px] whitespace-nowrap
            {i ? 'border-l border-rule' : ''} {on ? 'bg-ink text-bg' : 'bg-transparent text-muted hover:text-ink'}"
          onclick={() => (mode = m.id)}
          title="{m.label} ({m.key})"
          aria-label={m.label}
        >
          <svg width="13" height="13" viewBox="0 0 16 16" aria-hidden="true"
            ><rect x="1.5" y="2.5" width="13" height="11" fill="none" stroke="currentColor" stroke-width="1.3" /><path
              d={m.icon}
              stroke="currentColor"
              stroke-width="1.3"
            /></svg
          >
          <!-- In a narrow pane only the mode in use keeps its word, and in a very
               narrow one none does, so the close button stays in view. -->
          <span class={on ? 'hidden @min-[720px]:inline' : 'hidden @min-[960px]:inline'}>{m.label}</span>
        </button>
      {/each}
    </div>
    <button
      class="grid size-7 flex-none place-items-center border-0 bg-transparent text-muted hover:bg-surface hover:text-ink"
      onclick={() => load(true)}
      title="Refresh (R)"
      aria-label="Refresh"
    >
      <svg width="13" height="13" viewBox="0 0 16 16" aria-hidden="true"
        ><path d="M13.5 8A5.5 5.5 0 1 1 11.8 4M13.5 2v3h-3" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round" /></svg
      >
    </button>
    {@render trail?.()}
  </div>
  {#if offline}
    <div class="flex h-7 flex-none items-center gap-2 border-b border-rule px-3 font-mono text-[12px] text-muted" role="status">
      <span class="size-1.5 flex-none bg-faint" aria-hidden="true"></span>offline, using cached refs
    </div>
  {/if}
  <div class="grid min-h-0 flex-1 {showFiles ? 'grid-cols-[minmax(160px,min(248px,40%))_1fr]' : 'grid-cols-[1fr]'}">
    {#if showFiles}
      <div class="flex min-h-0 flex-col border-r border-rule bg-bg">
      {#if base && !range}
        <!-- What was committed since the base: read-only, so an agent's
             commits are in view beside the files they changed. -->
        <section class="flex max-h-[40%] min-h-0 flex-none flex-col border-b border-rule" aria-label="Commits since {base.branch}">
          <button
            class="flex h-7 w-full flex-none items-center gap-1.5 border-0 bg-transparent px-3 text-left text-[12px] text-muted hover:bg-surface hover:text-ink"
            onclick={() => (showCommits = !showCommits)}
            aria-expanded={showCommits}
            title="{showCommits ? 'Fold' : 'Show'} the commits on this branch since it forked from {base.branch} at {base.short}"
          >
            <svg width="9" height="9" viewBox="0 0 10 10" aria-hidden="true" class={showCommits ? 'rotate-90' : ''}
              ><path d="M3 1.5L7 5 3 8.5" fill="none" stroke="currentColor" stroke-width="1.4" /></svg
            >
            <span class="truncate"
              >{commits.length
                ? `${commits.length}${commits.length >= 200 ? '+' : ''} ${commits.length === 1 ? 'commit' : 'commits'} since ${base.branch}`
                : `No commits since ${base.branch}`}</span
            >
          </button>
          {#if showCommits && commits.length}
            <ol class="m-0 min-h-0 list-none overflow-y-auto p-0 pb-1">
              {#each commits as c (c.sha)}
                <li class="flex h-6 items-center gap-2 px-3 text-[12px]" title={`${c.sha}\n${c.subject}`}>
                  <span class="flex-none font-mono text-faint">{c.short}</span>
                  <span class="min-w-0 truncate text-ink">{c.subject}</span>
                </li>
              {/each}
            </ol>
          {/if}
        </section>
      {/if}
      <ul bind:this={list} class="m-0 min-h-0 flex-1 list-none overflow-y-auto p-0" aria-label="Changed files">
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
              aria-current={cur || undefined}
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
            {#if cur && sinceBase && f.status !== 'U'}
              <!-- Staging reads the index against HEAD; against the base the
                   hunks would not line up, so the controls wait there. -->
              <div class="flex min-h-7 items-center gap-2 border-b border-rule bg-surface py-1 pr-2 pl-8 text-[12px] text-muted">
                <span class="min-w-0 flex-1">Stage and discard work against HEAD.</span>
                <button
                  class="flex-none border border-rule bg-bg px-1.5 leading-5 text-ink hover:bg-raised"
                  title="Compare with HEAD to stage, unstage or discard this file's hunks"
                  onclick={toggleAgainst}>vs HEAD</button
                >
              </div>
            {/if}
            {#if cur && working}
              <div class="border-b border-rule bg-surface pb-1.5 text-[12px]">
                {#if hunks.note}
                  <p class="m-0 px-3 pt-1.5 pl-8 text-muted">{hunks.note}</p>
                {/if}
                {#each [{ staged: false, title: 'Unstaged', list: hunks.unstaged }, { staged: true, title: 'Staged', list: hunks.staged }] as g (g.title)}
                  {#if g.list.length}
                    <div class="mt-1.5 ml-[19px] border-l border-rule pl-2">
                      <div class="flex h-5 items-center gap-1.5 pr-3 text-muted">
                        {g.title}<span class="font-mono text-faint">{g.list.length}</span>
                      </div>
                      {#each g.list as h (hunkKey(g.staged, h))}
                        {@const lab = hunkLabel(h)}
                        {@const k = hunkKey(g.staged, h)}
                        <div class="flex min-h-6 items-center gap-1 pr-2">
                          {#if !g.staged}
                            <button
                              class="min-w-0 flex-1 truncate border-0 bg-transparent p-0 text-left font-mono text-ink hover:underline hover:underline-offset-4"
                              onclick={() => reveal(firstChange(h))}
                              title="Show this hunk: {h.header}">{lab.range}</button
                            >
                            <button
                              class="flex-none border border-rule bg-bg px-1.5 leading-5 text-ink enabled:hover:bg-raised disabled:text-faint"
                              disabled={busy}
                              title="Stage this hunk: add it to the next commit"
                              onclick={() => act(h, 'stage')}>Stage</button
                            >
                            <button
                              class="flex-none border border-rule bg-bg px-1.5 leading-5 text-ink enabled:hover:bg-raised disabled:text-faint {confirming === k ? 'border-fail' : ''}"
                              disabled={busy}
                              data-discard={k}
                              aria-expanded={confirming === k}
                              title="Discard this hunk from disk (asks first)"
                              onclick={() => askDiscard(k)}>Discard</button
                            >
                          {:else}
                            <span class="min-w-0 flex-1 truncate font-mono text-ink" title={h.header}>{lab.range}</span>
                            <button
                              class="flex-none border border-rule bg-bg px-1.5 leading-5 text-ink enabled:hover:bg-raised disabled:text-faint"
                              disabled={busy}
                              title="Unstage this hunk: keep it on disk, out of the next commit"
                              onclick={() => act(h, 'unstage')}>Unstage</button
                            >
                          {/if}
                        </div>
                        {#if lab.context}
                          <div class="-mt-0.5 mb-1 truncate pr-2 font-mono text-[11.5px] text-muted" title={lab.context}>{lab.context}</div>
                        {/if}
                        {#if confirming === k}
                          <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
                          <div
                            class="mr-2 mb-1.5 border border-fail bg-bg px-2 py-1.5"
                            role="group"
                            aria-label="Discard this hunk"
                            onkeydown={(e) => onConfirmKey(e, k)}
                          >
                            <p class="m-0 text-ink">Discard this hunk from disk? It cannot be undone.</p>
                            <div class="mt-1.5 flex justify-end gap-1">
                              <button data-keep class="border border-rule bg-bg px-1.5 leading-5 text-ink hover:bg-raised" onclick={() => keep(k)}
                                >Keep</button
                              >
                              <button class="border-0 bg-fail px-1.5 leading-5 font-medium text-bg" disabled={busy} onclick={() => act(h, 'discard')}
                                >Discard hunk</button
                              >
                            </div>
                          </div>
                        {/if}
                      {/each}
                    </div>
                  {/if}
                {/each}
              </div>
            {/if}
          </li>
        {/each}
        {#if !files.length && !loading}
          <li class="px-3 py-3 text-[13px] text-muted">
            {#if error}<span class="text-fail">{error}</span>{:else if range}No changes between these two commits.{:else if sinceBase && base}No changes since {base.branch}.{:else if worktree}Working tree clean. Nothing to diff against HEAD.{:else}No worktree to diff.{/if}
          </li>
        {/if}
      </ul>
      </div>
    {/if}
    <div class="relative min-h-0 min-w-0">
      <div class="absolute inset-0 {mode === 'replacement' ? 'hidden' : ''}" bind:this={diffHost}></div>
      <div class="absolute inset-0 {mode === 'replacement' ? '' : 'hidden'}" bind:this={repHost}></div>
      {#if problem}
        <div class="absolute inset-0 overflow-y-auto bg-bg px-6 pt-6">
          <h2 class="m-0 truncate font-mono text-[13px] font-normal text-muted" title={problem.path}>{problem.path}</h2>
          <p class="mt-1 mb-4 text-[15px] font-semibold text-ink">No text diff for this file</p>
          <dl class="m-0 grid max-w-[560px] grid-cols-[minmax(110px,auto)_auto_1fr] items-baseline gap-x-3 gap-y-2 border-t border-rule pt-3 text-[13px]">
            {#each problem.sides as s (s.label)}
              {@const w = s.absent ? { word: 'None', detail: s.absent, tone: 'bg-faint' } : blobWord(s.blob, s.which)}
              <dt class="font-mono text-[12px] text-muted">{s.label}</dt>
              <dd class="m-0 flex items-center gap-1.5 font-medium text-ink"><span class="size-1.5 {w.tone}" aria-hidden="true"></span>{w.word}</dd>
              <dd class="m-0 text-muted">{w.detail}</dd>
            {/each}
          </dl>
        </div>
      {:else if fileError}
        <div class="absolute inset-0 bg-bg px-6 pt-6 text-[13px]">
          <p class="m-0 flex items-center gap-1.5 font-medium text-ink"><span class="size-1.5 bg-fail" aria-hidden="true"></span>Could not read this file</p>
          <p class="mt-1 max-w-[65ch] text-muted">{fileError}</p>
        </div>
      {:else if !worktree && pick}
        <div class="absolute inset-0 overflow-y-auto bg-bg">{@render pick()}</div>
      {:else if !current && !loading}
        <div class="absolute inset-0 bg-bg"></div>
      {/if}
    </div>
  </div>
</div>

<style>
  /* Monaco draws outside this component's scope: these reach its DOM. */
  :global(.kete-rep-line) {
    background: var(--diff-add-line);
  }
  :global(.kete-rep-word) {
    background: var(--diff-add-word);
  }
  :global(.kete-rep-gutter) {
    background: var(--diff-add-word);
    width: 3px !important;
    margin-left: 3px;
  }
  :global(.kete-rep-removed) {
    margin-left: 2ch;
    padding: 0 0.6ch;
    background: var(--diff-del-line);
    color: var(--color-fail);
    font-family: var(--font-ui);
    font-size: 11.5px;
  }
  :global(.kete-rep-removed-top) {
    margin-left: 0;
    margin-right: 2ch;
  }
  :global(.kete-rep-removed::before) {
    content: '';
    display: inline-block;
    width: 6px;
    height: 6px;
    margin-right: 0.6ch;
    vertical-align: 1px;
    background: var(--color-fail);
  }
</style>
