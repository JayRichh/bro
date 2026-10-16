<script lang="ts">
  // A checkout's editor buttons, for a row's hover verbs and the session
  // header: the default editor's own icon (VS Code, Cursor; the drawn editor
  // icon when the Open command is the default), and Visual Studio's beside
  // it where the checkout has a solution or project. A small square marks
  // an editor that looks open on the checkout: asked when the pointer or the
  // focus reaches a button, never polled. Clicks stop at the button.
  import type { Worktree } from '../api';
  import Icon from '../ui/Icon.svelte';
  import { withKey } from '../shortcuts';
  import { openEditor } from '../actions';
  import { probeWindows } from './store.svelte';
  import { factsFor, folderDefault, openDefault, openSolution } from './verbs';

  let {
    worktree,
    btn,
    size = 14,
  }: {
    worktree: Worktree | undefined;
    /** The surface's own button classes, so the icons sit with its verbs. */
    btn: string;
    size?: number;
  } = $props();

  const facts = $derived(worktree ? factsFor(worktree.id) : null);
  const def = $derived(facts ? folderDefault(facts) : null);
  const several = $derived((facts?.solutions?.length ?? 0) > 1 ? '\u2026' : '');
  /** Visual Studio beside the default, when the default isn't it. */
  const vs = $derived(
    facts && facts.solutions?.length && facts.vs.length && !(def?.kind === 'editor' && def.editor.family === 'visual-studio') ? facts.vs[0] : null,
  );
  const open = (id: string | undefined) => !!id && !!facts?.windows?.includes(id);

  function probe() {
    if (worktree) void probeWindows([worktree.id]);
  }
</script>

{#if worktree && def}
  {#if def.kind === 'editor'}
    {@const e = def.editor}
    {@const name = `${e.short}${e.family === 'visual-studio' ? several : ''}`}
    <button
      class="relative {btn}"
      title="Open in {name}{open(e.id) ? ' (a window is open on it)' : ''}"
      aria-label="Open {worktree.branch} in {name}"
      onmouseenter={probe}
      onfocus={probe}
      onclick={(ev) => {
        ev.stopPropagation();
        openDefault(worktree, () => openEditor(worktree.id));
      }}
    >
      {#if e.icon}<img src={e.icon} alt="" width={size} height={size} class="block flex-none" draggable="false" />{:else}<Icon name="editor" size={size - 2} />{/if}
      {#if open(e.id)}<span class="absolute top-0.5 right-0.5 size-[5px] bg-ink outline outline-1 outline-bg" aria-hidden="true"></span>{/if}
    </button>
  {:else}
    <button
      class={btn}
      title={withKey('Run the Open command', 'open_editor')}
      aria-label="Run the Open command for {worktree.branch}"
      onclick={(ev) => {
        ev.stopPropagation();
        openEditor(worktree.id);
      }}><Icon name="editor" size={size - 2} /></button
    >
  {/if}
  {#if vs}
    <button
      class="relative {btn}"
      title="Open in {vs.short}{several}{open(vs.id) ? ' (a window is open on it)' : ''}"
      aria-label="Open {worktree.branch} in {vs.short}"
      onmouseenter={probe}
      onfocus={probe}
      onclick={(ev) => {
        ev.stopPropagation();
        void openSolution(worktree.id, vs);
      }}
    >
      {#if vs.icon}<img src={vs.icon} alt="" width={size} height={size} class="block flex-none" draggable="false" />{:else}<span class="font-mono text-[10px] leading-none">VS</span>{/if}
      {#if open(vs.id)}<span class="absolute top-0.5 right-0.5 size-[5px] bg-ink outline outline-1 outline-bg" aria-hidden="true"></span>{/if}
    </button>
  {/if}
{/if}
