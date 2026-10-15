<script lang="ts">
  // The ship sheet, over the board: the PR composer (a card dropped on
  // Ship, a review's Open PR) and the merge confirm (a Ship card dropped on
  // Done, a review's Merge). Square and ruled like the dialog; Esc closes,
  // Ctrl+Enter sends, Enter in the title too. A failed send keeps what was
  // typed and says why, with its button: Sign in, How to install gh, Open
  // in ADO.
  import { tick } from 'svelte';
  import { api, type MergeMethod } from '../api';
  import { toast } from '../store.svelte';
  import { firstFocus } from '../ui/focus';
  import { shipSheet as s } from './state.svelte';
  import { closeShip, createPr, mergePr, runErrorAction, sheetPlace } from './sheet.svelte';
  import { createBlocked, errorAction, forgeName, isOpen } from './ship';
  import { signin } from '../ado/signin.svelte';

  let box = $state<HTMLDivElement>();

  const st = $derived(s.status);
  const pr = $derived(st?.pr ?? null);
  const blocked = $derived(s.mode === 'compose' ? createBlocked(s.form, st) : null);
  const action = $derived(s.error ? errorAction(s.error) : null);
  const mergeable = $derived(!!pr && isOpen(pr) && !pr.draft);
  const METHODS: { id: MergeMethod; label: string; hint: string }[] = [
    { id: 'squash', label: 'Squash', hint: 'one commit on the base' },
    { id: 'merge', label: 'Merge commit', hint: 'every commit, and a merge' },
    { id: 'rebase', label: 'Rebase', hint: 'every commit, replayed' },
  ];

  // Focus the title (compose) or the primary button (merge) as it opens,
  // and again once the status lands and the form fills.
  let opened = '';
  $effect(() => {
    const key = s.mode ? `${s.mode}\u0000${s.worktree}\u0000${s.loading}` : '';
    if (!key || key === opened) return;
    opened = key;
    tick().then(() => {
      if (box?.contains(document.activeElement) && document.activeElement !== box) return;
      const first = box ? firstFocus(box) : null;
      (first ?? box)?.focus();
    });
  });

  function send() {
    if (s.mode === 'compose') void createPr();
    else if (s.mode === 'merge' && mergeable) void mergePr();
  }

  function onKey(e: KeyboardEvent) {
    e.stopPropagation();
    if (e.key === 'Escape') return (e.preventDefault(), closeShip());
    if (e.key === 'Tab' && box) {
      const all = [...box.querySelectorAll<HTMLElement>('input, textarea, button:not([disabled])')];
      const i = all.indexOf(document.activeElement as HTMLElement);
      if (e.shiftKey && i <= 0) (e.preventDefault(), all[all.length - 1]?.focus());
      else if (!e.shiftKey && i === all.length - 1) (e.preventDefault(), all[0]?.focus());
      return;
    }
    const t = e.target as HTMLElement;
    if (e.key === 'Enter' && (e.ctrlKey || (t.tagName === 'INPUT' && (t as HTMLInputElement).type === 'text'))) {
      e.preventDefault();
      send();
    }
  }

  function openPr() {
    if (pr) api.openUrl(pr.url).catch((e) => toast(String(e)));
  }

  const btn = 'h-7 flex-none border px-3 text-[13px]';
  const field =
    'w-full border border-rule bg-bg px-2 py-1 text-[13px] text-ink outline-none placeholder:text-faint hover:border-rule-strong focus:border-rule-strong';
  const label = 'text-[12px] font-medium text-muted';
</script>

{#if s.mode}
  <!-- svelte-ignore a11y_no_static_element_interactions, a11y_click_events_have_key_events -->
  <div class="fixed inset-0 z-40 bg-[rgb(28_25_23/0.18)]" onmousedown={closeShip}></div>
  <div
    bind:this={box}
    class="fixed top-[10%] left-1/2 z-50 flex max-h-[80vh] w-[min(620px,94vw)] -translate-x-1/2 flex-col border border-rule-strong bg-bg text-ink"
    role="dialog"
    aria-modal="true"
    aria-labelledby="ship-title"
    tabindex="-1"
    onkeydown={onKey}
  >
    <header class="flex flex-none items-baseline gap-3 border-b border-rule px-4 py-2.5">
      <h2 id="ship-title" class="m-0 text-[15px] font-semibold">{s.mode === 'compose' ? 'Open a pull request' : 'Merge the pull request'}</h2>
      <span class="min-w-0 flex-1 truncate font-mono text-[12px] text-muted" title={sheetPlace()}>{sheetPlace()}</span>
      {#if st?.forge}<span class="flex-none font-mono text-[11px] text-faint">{forgeName(st.forge)}</span>{/if}
    </header>

    <div class="flex min-h-0 flex-col gap-3 overflow-y-auto px-4 py-3">
      <!-- Where the branch stands: head into base, what no remote has yet. -->
      {#if s.loading && !st}
        <p class="m-0 text-[13px] text-muted">Reading the branch&hellip;</p>
      {:else if st}
        <p class="m-0 font-mono text-[12px] text-muted">
          {st.branch ?? 'detached HEAD'} &rarr; {pr?.base ?? st.base ?? '?'}{#if st.unpushed}<span class="ml-1.5">&middot; {st.unpushed} commit{st.unpushed === 1 ? '' : 's'} to push</span>{/if}
        </p>
      {/if}

      {#if s.error}
        <div class="flex flex-wrap items-center gap-3 border border-fail px-3 py-2 text-[13px]" role="alert">
          <span class="min-w-0 flex-1 select-text">{s.error.message}</span>
          {#if action}
            <button
              class="{btn} border-rule-strong bg-transparent text-ink hover:bg-surface disabled:text-faint"
              disabled={action.kind === 'signIn' && signin.busy}
              title={action.kind === 'signIn' ? `Sign in to ${forgeName(action.forge ?? st?.forge)}` : action.url}
              onclick={() => runErrorAction(action)}>{action.kind === 'signIn' && signin.busy ? 'Signing in\u2026' : action.label}</button
            >
          {/if}
        </div>
      {/if}

      {#if s.mode === 'compose'}
        <label class="flex flex-col gap-1">
          <span class={label}>Title</span>
          <input class={field} spellcheck="false" bind:value={s.form.title} placeholder={st?.branch ?? ''} />
        </label>
        <label class="flex flex-col gap-1">
          <span class={label}>Description</span>
          <textarea class="{field} min-h-32 resize-y" spellcheck="false" bind:value={s.form.body}></textarea>
        </label>
        <div class="flex flex-wrap items-end gap-4">
          <label class="flex min-w-[180px] flex-1 flex-col gap-1">
            <span class={label}>Into</span>
            <input class="{field} font-mono text-[12px]" spellcheck="false" bind:value={s.form.base} placeholder={st?.base ?? 'main'} />
          </label>
          <label class="flex h-7 items-center gap-2 text-[13px]">
            <input type="checkbox" class="size-3.5 accent-[var(--color-ink)]" bind:checked={s.form.draft} />
            Draft
          </label>
        </div>
      {:else if st && !pr}
        <p class="m-0 text-[13px]">This branch has no pull request open.</p>
      {:else if pr}
        <button
          class="flex min-w-0 items-baseline gap-2 border border-rule bg-transparent px-3 py-2 text-left hover:border-rule-strong"
          title="Open {pr.url}"
          onclick={openPr}
        >
          <span class="flex-none font-mono text-[12px] text-muted">#{pr.number}</span>
          <span class="min-w-0 flex-1 truncate text-[14px]">{pr.title}</span>
          {#if pr.draft}<span class="flex-none font-mono text-[11px] text-muted">draft</span>{/if}
        </button>
        <p class="m-0 -mt-2 truncate font-mono text-[11px] text-muted select-text" title={pr.url}>{pr.url}</p>
        {#if pr.draft}
          <p class="m-0 text-[13px] text-muted">A draft can't merge: mark it ready for review on {forgeName(pr.forge)} first.</p>
        {/if}
        <fieldset class="m-0 flex flex-col border border-rule p-0">
          <legend class="sr-only">Merge method</legend>
          {#each METHODS as m (m.id)}
            <label class="flex cursor-default items-baseline gap-2 border-b border-rule px-3 py-1.5 text-[13px] last:border-b-0 hover:bg-surface">
              <input type="radio" name="merge-method" class="accent-[var(--color-ink)]" value={m.id} bind:group={s.merge.method} />
              <span>{m.label}</span>
              <span class="text-[12px] text-muted">{m.hint}</span>
            </label>
          {/each}
        </fieldset>
        <label class="flex items-center gap-2 text-[13px]">
          <input type="checkbox" class="size-3.5 accent-[var(--color-ink)]" bind:checked={s.merge.deleteBranch} />
          Delete the remote branch
        </label>
        <p class="m-0 text-[12px] text-muted">The local branch and the worktree stay; the worktree's delete removes them.</p>
      {/if}
    </div>

    <footer class="flex flex-none flex-wrap items-center justify-end gap-2 border-t border-rule px-4 py-2.5">
      <span class="mr-auto min-w-0 text-[12px] text-muted">
        {#if s.mode === 'compose' && blocked && !s.loading}{blocked}{:else}<span class="font-mono text-[11px]">Ctrl+Enter to send</span>{/if}
      </span>
      <button class="{btn} border-rule bg-transparent text-ink hover:border-rule-strong" title="Close (Esc)" onclick={closeShip}>Cancel</button>
      {#if s.mode === 'compose'}
        <button
          data-autofocus
          class="{btn} border-transparent bg-ink font-medium text-bg disabled:bg-surface disabled:text-faint"
          disabled={!!blocked || s.busy}
          title={blocked ?? 'Push the branch and open the pull request (Ctrl+Enter)'}
          onclick={send}>{s.busy ? 'Opening\u2026' : s.form.draft ? 'Open draft PR' : 'Open PR'}</button
        >
      {:else}
        <button
          data-autofocus
          class="{btn} border-transparent bg-ink font-medium text-bg disabled:bg-surface disabled:text-faint"
          disabled={!mergeable || s.busy}
          title={mergeable ? `Merge it, ${s.merge.method} (Ctrl+Enter)` : 'Nothing open to merge'}
          onclick={send}>{s.busy ? 'Merging\u2026' : 'Merge'}</button
        >
      {/if}
    </footer>
  </div>
{/if}
