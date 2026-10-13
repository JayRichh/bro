<script lang="ts">
  import { onMount } from 'svelte';
  import { app, openSession, sessionWorktree, toast } from './store.svelte';
  import { api, type Launchables } from './api';

  const TERMINAL = '__terminal';
  let launch = $state<Launchables>({ harnesses: [], profiles: [] });
  let project = $state('');
  let worktree = $state('');
  let harness = $state('claude');
  let profile = $state('');
  let prompt = $state('');
  let busy = $state(false);
  let box: HTMLTextAreaElement;

  const worktrees = $derived(
    app.model.worktrees
      .filter((w) => w.project_id === project)
      .sort((a, b) => Number(b.is_main) - Number(a.is_main) || a.sort_order - b.sort_order),
  );
  const profiles = $derived(launch.profiles.filter((p) => p.harness === harness));

  onMount(async () => {
    const wt = sessionWorktree(app.session);
    const w = app.model.worktrees.find((x) => x.id === wt);
    project = w?.project_id ?? app.model.projects[0]?.id ?? '';
    worktree = w?.id ?? '';
    try {
      launch = await api.launchables();
    } catch {
      /* the built-in fallback below still works */
    }
    if (!launch.harnesses.length) launch.harnesses = [{ id: 'claude', label: 'Claude' }];
    box?.focus();
  });

  $effect(() => {
    if (!worktrees.find((w) => w.id === worktree)) worktree = worktrees[0]?.id ?? '';
  });
  $effect(() => {
    const def = profiles.find((p) => p.default);
    if (!profiles.find((p) => p.name === profile)) profile = def?.name ?? '';
  });

  function close() {
    app.quickPrompt = false;
  }

  async function submit() {
    if (!worktree || busy) return;
    busy = true;
    try {
      if (harness === TERMINAL) {
        const id = await api.createTerminal(worktree);
        if (id) openSession({ Terminal: id });
      } else {
        const id = await api.createAgent({ worktree, kind: harness, profile: profile || null, prompt });
        if (id) openSession({ Agent: id });
      }
      close();
    } catch (e) {
      toast(String(e));
    } finally {
      busy = false;
    }
  }

  function onKey(e: KeyboardEvent) {
    if (e.key === 'Escape') {
      e.preventDefault();
      close();
    } else if (e.key === 'Enter' && !e.shiftKey) {
      e.preventDefault();
      submit();
    }
  }

  const field = 'flex min-w-0 flex-col gap-[3px]';
  const label = 'pl-0.5 text-[10.5px] font-semibold text-muted';
  const control =
    'min-w-0 rounded-md border border-line bg-bg outline-none focus:border-focus focus:ring-1 focus:ring-focus';
</script>

<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
<div class="fixed inset-0 z-20 bg-scrim" onclick={close}></div>
<div
  class="fixed top-[12vh] left-1/2 z-21 flex w-[min(760px,calc(100vw-48px))] -translate-x-1/2 flex-col gap-2.5 rounded-[10px] border border-line bg-raised p-3 shadow-pop"
  role="dialog"
  aria-label="New session"
  tabindex="-1"
  onkeydown={onKey}
>
  <div class="grid grid-cols-[1.1fr_1.1fr_1fr_1fr] gap-2">
    <label class={field}>
      <span class={label}>Project</span>
      <select class="{control} px-1.5 py-1 text-[12.5px]" bind:value={project}>
        {#each app.model.projects as p (p.id)}<option value={p.id}>{p.name}</option>{/each}
      </select>
    </label>
    <label class={field}>
      <span class={label}>Worktree</span>
      <select class="{control} px-1.5 py-1 font-mono text-[11.5px]" bind:value={worktree}>
        {#each worktrees as w (w.id)}<option value={w.id}>{w.branch}</option>{/each}
      </select>
    </label>
    <label class={field}>
      <span class={label}>Harness</span>
      <select class="{control} px-1.5 py-1 text-[12.5px]" bind:value={harness}>
        {#each launch.harnesses as h (h.id)}<option value={h.id}>{h.label}</option>{/each}
        <option value={TERMINAL}>Terminal (shell)</option>
      </select>
    </label>
    <label class={field}>
      <span class={label}>Profile</span>
      <select class="{control} px-1.5 py-1 text-[12.5px]" bind:value={profile} disabled={harness === TERMINAL || !profiles.length}>
        <option value="">{profiles.length ? 'No profile' : 'Default login'}</option>
        {#each profiles as p (p.name)}<option value={p.name}>{p.name}{p.default ? ' (default)' : ''}</option>{/each}
      </select>
    </label>
  </div>
  <textarea
    class="{control} resize-none px-2.5 py-[9px] text-[13.5px] leading-[1.45] disabled:opacity-60"
    bind:this={box}
    bind:value={prompt}
    rows="4"
    disabled={harness === TERMINAL}
    placeholder={harness === TERMINAL ? 'Opens a shell in this worktree.' : 'What should it do? Leave empty to open the CLI and type there.'}
  ></textarea>
  <div class="flex items-center gap-2">
    <span class="flex items-center gap-1 text-[11px] text-muted">
      <kbd>Enter</kbd> start <kbd>Shift</kbd><kbd>Enter</kbd> new line <kbd>Esc</kbd> close
    </span>
    <button
      class="ml-auto rounded-md border-0 bg-focus px-3.5 py-1.5 font-semibold text-on-focus enabled:hover:brightness-110 disabled:opacity-50"
      onclick={submit}
      disabled={!worktree || busy}>{#if busy}Starting&hellip;{:else}Start session{/if}</button
    >
  </div>
</div>
