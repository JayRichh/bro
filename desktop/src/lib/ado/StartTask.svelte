<script lang="ts">
  import { onMount } from 'svelte';
  import { app, openSession, toast } from '../store.svelte';
  import { branchProblem, suggestBranch, tasksApi, type StartedTask, type WorkItem } from './tasks-api';

  let {
    worktree,
    repo,
    item,
    initials,
    initialsUnknown = false,
    taken,
    onclose,
    onstarted,
  }: {
    worktree: string;
    repo: string;
    item: WorkItem;
    initials: string;
    /** The branch list failed, so `initials` is empty for want of it. */
    initialsUnknown?: boolean;
    taken: string[];
    onclose: () => void;
    onstarted: () => void;
  } = $props();

  // The form starts from the task once and is the user's from then on.
  // svelte-ignore state_referenced_locally
  let branch = $state(suggestBranch(initials, item.title));
  let base = $state('');
  let assignToMe = $state(true);
  let activate = $state(true);
  let spawn = $state(true);
  const harnesses = $derived(app.launch.harnesses);
  const preferred = app.launch.profiles.find((p) => p.default);
  let harness = $state(preferred?.harness ?? app.launch.harnesses[0]?.id ?? 'claude');
  let profile = $state(preferred?.name ?? '');
  const profiles = $derived(app.launch.profiles.filter((p) => p.harness === harness));
  let prompt = $state('');
  let promptState = $state<'loading' | 'ready' | 'failed'>('loading');
  let promptError = $state('');
  let busy = $state(false);
  let error = $state<string | null>(null);
  let outcome = $state<StartedTask | null>(null);
  let box: HTMLInputElement | undefined = $state();

  // Initials arrive with the branch list; until the user edits, follow them.
  let edited = false;
  $effect(() => {
    const suggestion = suggestBranch(initials, item.title);
    if (!edited) branch = suggestion;
  });

  const problem = $derived(branchProblem(branch, taken));

  async function loadPrompt() {
    promptState = 'loading';
    try {
      prompt = await tasksApi.prompt(worktree, item.id);
      promptState = 'ready';
    } catch (e) {
      promptError = String(e);
      promptState = 'failed';
    }
  }

  onMount(() => {
    loadPrompt();
    if (!box) return;
    box.focus();
    // Select the slug, caret at its start, so the initials stay in view.
    const cut = branch.indexOf('/') + 1;
    box.setSelectionRange(cut, branch.length, 'backward');
    box.scrollLeft = 0;
  });

  /** The user changed something here, so leaving asks first. */
  let touched = $state(false);
  let confirmDiscard = $state(false);

  /** The harness's default profile, else none. */
  function pickHarness(h: string) {
    profile = app.launch.profiles.find((p) => p.harness === h && p.default)?.name ?? '';
  }

  async function submit(e: SubmitEvent) {
    e.preventDefault();
    if (busy || problem) return;
    busy = true;
    error = null;
    try {
      const done = await tasksApi.start(worktree, {
        id: item.id,
        branch: branch.trim(),
        base: base.trim() || null,
        assignToMe,
        activate,
        spawn: spawn ? { worktree: '', kind: harness, profile: profile || null, prompt: prompt.trim() || null } : null,
      });
      onstarted();
      if (done.problems.length) {
        outcome = done;
        return;
      }
      onclose();
      if (done.agentId) openSession({ Agent: done.agentId });
      else toast(`Started ${branch.trim()} in a new worktree.`);
    } catch (err) {
      error = String(err);
    } finally {
      busy = false;
    }
  }

  function close() {
    if (busy) return;
    if (touched && !outcome && !confirmDiscard) {
      confirmDiscard = true;
      return;
    }
    onclose();
  }

  function onEscape(e: KeyboardEvent) {
    if (e.key !== 'Escape') return;
    e.stopPropagation();
    // A second Escape on the question keeps editing.
    if (confirmDiscard) confirmDiscard = false;
    else close();
  }

  const STEP: Record<string, string> = {
    link: 'Link the branch to the task',
    read: 'Read the task',
    assign: 'Assign it to you',
    activate: 'Set it to Active',
    spawn: 'Start the agent',
  };

  const field = 'h-8 w-full min-w-0 border border-rule bg-bg px-2.5 text-[13px] text-ink outline-none focus:border-rule-strong';
  const pick =
    'h-8 w-full min-w-0 appearance-none border border-rule bg-bg pr-7 pl-2.5 text-[13px] text-ink outline-none focus:border-rule-strong disabled:text-faint';
  const label = 'text-[12px] font-medium text-muted';
</script>

{#snippet chevron()}
  <svg class="pointer-events-none absolute top-1/2 right-2.5 -translate-y-1/2 text-muted" width="10" height="10" viewBox="0 0 10 10" aria-hidden="true"
    ><path d="M1.5 3.5L5 7l3.5-3.5" fill="none" stroke="currentColor" stroke-width="1.4" /></svg
  >
{/snippet}

{#snippet toggle(on: boolean, set: (v: boolean) => void, text: string, hint: string)}
  <label class="flex cursor-default items-start gap-2.5 py-1">
    <input type="checkbox" class="peer sr-only" checked={on} onchange={(e) => set((e.currentTarget as HTMLInputElement).checked)} disabled={busy} />
    <span
      class="mt-0.5 grid size-3.5 flex-none place-items-center border border-rule-strong peer-focus-visible:outline-2 peer-focus-visible:outline-offset-2 peer-focus-visible:outline-rule-strong"
      aria-hidden="true"
    >
      {#if on}<span class="size-2 bg-ink"></span>{/if}
    </span>
    <span class="min-w-0">
      <span class="block text-[13px] text-ink">{text}</span>
      <span class="block text-[12px] text-muted">{hint}</span>
    </span>
  </label>
{/snippet}

<svelte:window onkeydown={onEscape} />

<!-- Takes the detail pane's place, so the list stays in view while you fill it in. -->
<div class="flex min-h-0 flex-1 flex-col">
  <div class="flex min-h-0 flex-1 flex-col" role="dialog" aria-labelledby="start-title">
    <header class="flex-none border-b border-rule px-5 pt-4 pb-3">
      <p class="m-0 flex items-baseline gap-3 font-mono text-[12px] text-muted">
        <span>{item.kind} {item.id}</span><span class="flex-1"></span><span>Esc cancels</span>
      </p>
      <h2 id="start-title" class="m-0 mt-0.5 text-[18px] leading-snug font-semibold text-ink">Start {item.title}</h2>
    </header>

    {#if outcome}
      <div class="px-5 py-4">
        <p class="m-0 flex items-center gap-2 text-[14px] text-ink">
          <span class="size-1.5 flex-none bg-done" aria-hidden="true"></span>
          <span>The worktree for <span class="font-mono text-[13px]">{branch}</span> is ready. Some steps did not happen:</span>
        </p>
        <ul class="m-0 mt-3 list-none border-t border-rule p-0">
          {#each outcome.problems as p (p.step)}
            <li class="flex items-baseline gap-3 border-b border-rule py-2 text-[13px]">
              <span class="inline-flex flex-none items-center gap-1.5"
                ><span class="size-1.5 bg-fail" aria-hidden="true"></span><span class="font-medium text-fail">Failed</span></span
              >
              <span class="flex-none text-ink">{STEP[p.step] ?? p.step}</span>
              <span class="min-w-0 text-muted select-text">{p.message}</span>
            </li>
          {/each}
        </ul>
      </div>
      <footer class="flex flex-none items-center justify-end gap-2 border-t border-rule px-5 py-3">
        {#if outcome.agentId}
          {@const id = outcome.agentId}
          <button
            class="h-8 border border-rule bg-transparent px-3 text-[13px] text-ink hover:border-rule-strong"
            onclick={() => (onclose(), openSession({ Agent: id }))}>Open the session</button
          >
        {/if}
        <button class="h-8 border-0 bg-ink px-3 text-[13px] font-medium text-bg hover:bg-muted" onclick={onclose}>Done</button>
      </footer>
    {:else}
      <form class="flex min-h-0 flex-1 flex-col" onsubmit={submit} oninput={() => (touched = true)} onchange={() => (touched = true)}>
        <div class="min-h-0 flex-1 overflow-y-auto px-5 py-4">
          <div class="grid grid-cols-[minmax(0,1fr)_140px] gap-3">
            <label class="flex min-w-0 flex-col gap-1">
              <span class={label}>Branch</span>
              <input
                bind:this={box}
                class="{field} font-mono {problem ? 'border-fail focus:border-fail' : ''}"
                bind:value={branch}
                oninput={() => (edited = true)}
                spellcheck="false"
                autocomplete="off"
                aria-invalid={!!problem}
                aria-describedby="branch-hint"
                disabled={busy}
              />
            </label>
            <label class="flex min-w-0 flex-col gap-1">
              <span class={label}>Base</span>
              <input class="{field} font-mono" bind:value={base} placeholder="default branch" spellcheck="false" autocomplete="off" disabled={busy} />
            </label>
          </div>
          <p id="branch-hint" class="m-0 mt-1.5 flex min-h-[18px] items-center gap-1.5 text-[12px] {problem ? 'text-fail' : 'text-muted'}" aria-live="polite">
            {#if problem}
              <span class="size-1.5 flex-none bg-fail" aria-hidden="true"></span>{problem}
            {:else if initialsUnknown && !initials}
              Your initials are not known (the branch list did not load), so the name has no prefix. Add one if your team uses it.
            {:else}
              A new worktree on this branch, cut from {base.trim() || 'the default branch'}.
            {/if}
          </p>

          <div class="mt-3 grid grid-cols-2 gap-x-4 border-t border-rule pt-2">
            {@render toggle(assignToMe, (v) => (assignToMe = v), 'Assign to me', 'Sets Assigned To on the task.')}
            {@render toggle(activate, (v) => (activate = v), 'Set to Active', 'Moves the task out of New.')}
          </div>

          <div class="mt-2 border-t border-rule pt-2">
            {@render toggle(spawn, (v) => (spawn = v), 'Start an agent', 'Runs in the new worktree with the prompt below.')}
            {#if spawn}
              <div class="mt-2 grid grid-cols-2 gap-3 pl-6">
                <label class="flex min-w-0 flex-col gap-1">
                  <span class={label}>Harness</span>
                  <span class="relative flex">
                    <select class={pick} bind:value={harness} onchange={() => pickHarness(harness)} disabled={busy || !harnesses.length}>
                      {#each harnesses as h (h.id)}<option value={h.id}>{h.label}</option>{/each}
                      {#if !harnesses.length}<option value={harness}>{harness}</option>{/if}
                    </select>
                    {@render chevron()}
                  </span>
                </label>
                <label class="flex min-w-0 flex-col gap-1">
                  <span class={label}>Profile</span>
                  <span class="relative flex">
                    <select class={pick} bind:value={profile} disabled={busy || !profiles.length}>
                      <option value="">{profiles.length ? 'No profile' : 'Default login'}</option>
                      {#each profiles as p (p.name)}<option value={p.name}>{p.name}</option>{/each}
                    </select>
                    {@render chevron()}
                  </span>
                </label>
              </div>
              <label class="mt-3 flex flex-col gap-1 pl-6">
                <span class="{label} flex items-baseline gap-2">
                  Prompt
                  {#if promptState === 'loading'}<span class="font-mono font-normal">loading from the task</span>{/if}
                </span>
                <textarea
                  class="h-44 w-full resize-y border border-rule bg-bg px-2.5 py-2 font-mono text-[12px] leading-[18px] text-ink outline-none focus:border-rule-strong"
                  bind:value={prompt}
                  spellcheck="false"
                  disabled={busy || promptState === 'loading'}
                  placeholder={promptState === 'failed' ? 'Write the prompt, or leave it empty to use the task' : ''}
                ></textarea>
                {#if promptState === 'failed'}
                  <span class="flex items-center gap-1.5 text-[12px] text-muted">
                    Could not read the prompt: {promptError}
                    <button type="button" class="border-0 bg-transparent p-0 text-[12px] text-ink underline underline-offset-4 hover:no-underline" onclick={loadPrompt}
                      >Try again</button
                    >
                  </span>
                {/if}
              </label>
            {/if}
          </div>
        </div>

        {#if error}
          <p class="m-0 flex flex-none items-start gap-2 border-t border-rule px-5 py-2.5 text-[13px] text-ink select-text" role="alert">
            <span class="mt-1.5 size-1.5 flex-none bg-fail" aria-hidden="true"></span>
            <span><span class="font-medium text-fail">Not started.</span> {error}</span>
          </p>
        {/if}
        {#if confirmDiscard}
          <div class="flex flex-none items-center gap-3 border-t border-rule px-5 py-2.5 text-[13px] text-ink" role="alertdialog" aria-label="Discard changes">
            <span class="min-w-0 flex-1">Discard what you changed in this form?</span>
            <button type="button" class="h-7 border-0 bg-ink px-2.5 text-[13px] font-medium text-bg hover:bg-muted" onclick={onclose}>Discard</button>
            <button type="button" class="h-7 border border-rule bg-transparent px-2.5 text-[13px] text-ink hover:border-rule-strong" onclick={() => (confirmDiscard = false)}
              >Keep editing</button
            >
          </div>
        {/if}
        <footer class="flex flex-none items-center gap-2 border-t border-rule px-5 py-3">
          <span class="min-w-0 flex-1 truncate font-mono text-[12px] text-muted">{repo ? (app.model.projects.find((p) => p.id === repo)?.name ?? '') : ''}</span>
          <button type="button" class="h-8 border border-rule bg-transparent px-3 text-[13px] text-ink hover:border-rule-strong disabled:text-faint" onclick={close} disabled={busy}
            >Cancel</button
          >
          <button
            type="submit"
            class="h-8 min-w-24 border-0 bg-ink px-3 text-[13px] font-medium text-bg enabled:hover:bg-muted disabled:bg-raised disabled:text-faint"
            disabled={busy || !!problem}>{busy ? 'Starting' : 'Start task'}</button
          >
        </footer>
      </form>
    {/if}
  </div>
</div>
