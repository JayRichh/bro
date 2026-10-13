<script lang="ts">
  import { app, openSession, projectOf, sessionWorktree, toast } from './store.svelte';
  import { api } from './api';
  import { nav } from './shell/history.svelte';

  // The quick prompt: one line pinned under the main column. Pick where and
  // with what, type the task, Enter starts it. `P` focuses it from anywhere.
  const TERMINAL = '__terminal';
  let project = $state('');
  let worktree = $state('');
  let harness = $state('claude');
  let profile = $state('');
  let prompt = $state('');
  let busy = $state(false);
  let box: HTMLInputElement;

  const harnesses = $derived(app.launch.harnesses.length ? app.launch.harnesses : [{ id: 'claude', label: 'Claude' }]);
  const worktrees = $derived(
    app.model.worktrees
      .filter((w) => w.project_id === project)
      .sort((a, b) => Number(b.is_main) - Number(a.is_main) || a.sort_order - b.sort_order),
  );
  const profiles = $derived(app.launch.profiles.filter((p) => p.harness === harness));
  const ready = $derived(app.model.connected && !!worktree && !busy);

  // Follow the window: the repo in view, else the open session's.
  $effect(() => {
    const repo = nav.current.repo;
    if (repo) project = repo;
  });
  $effect(() => {
    if (app.model.projects.find((p) => p.id === project)) return;
    project = projectOf(sessionWorktree(app.session)) ?? app.model.projects[0]?.id ?? '';
  });
  $effect(() => {
    if (!worktrees.find((w) => w.id === worktree)) worktree = worktrees[0]?.id ?? '';
  });
  $effect(() => {
    const def = profiles.find((p) => p.default);
    if (!profiles.find((p) => p.name === profile)) profile = def?.name ?? '';
  });

  // A focus request, maybe with a preset from the rail or a repo row.
  let seen = 0;
  $effect(() => {
    const n = app.quickFocus;
    if (n === seen) return;
    seen = n;
    const p = app.quickPreset;
    app.quickPreset = null;
    if (p?.project) project = p.project;
    if (p?.harness) harness = p.harness;
    if (p?.profile) queueMicrotask(() => (profile = p.profile!));
    box?.focus();
  });

  async function submit() {
    if (!ready) return;
    busy = true;
    try {
      if (harness === TERMINAL) {
        const id = await api.createTerminal(worktree);
        if (id) openSession({ Terminal: id });
      } else {
        const id = await api.createAgent({ worktree, kind: harness, profile: profile || null, prompt });
        if (id) openSession({ Agent: id });
      }
      prompt = '';
      box?.blur();
    } catch (e) {
      toast(String(e));
    } finally {
      busy = false;
    }
  }

  function onKey(e: KeyboardEvent) {
    if (e.key === 'Escape') {
      e.preventDefault();
      box?.blur();
    } else if (e.key === 'Enter') {
      e.preventDefault();
      submit();
    }
  }

  const pick =
    'h-full min-w-0 appearance-none border-0 border-r border-rule bg-transparent px-3 text-[13px] text-ink outline-none hover:bg-surface focus-visible:-outline-offset-2 disabled:text-faint';
</script>

<form
  class="flex h-11 flex-none items-stretch border-t border-rule-strong bg-bg"
  aria-label="Start a session"
  onsubmit={(e) => {
    e.preventDefault();
    submit();
  }}
>
  <select class="{pick} max-w-40" bind:value={project} aria-label="Repo">
    {#each app.model.projects as p (p.id)}<option value={p.id}>{p.name}</option>{/each}
  </select>
  <select class="{pick} max-w-44 font-mono text-[12px]" bind:value={worktree} aria-label="Worktree">
    {#each worktrees as w (w.id)}<option value={w.id}>{w.branch}</option>{/each}
  </select>
  <select class="{pick} max-w-32" bind:value={harness} aria-label="Harness">
    {#each harnesses as h (h.id)}<option value={h.id}>{h.label}</option>{/each}
    <option value={TERMINAL}>Terminal</option>
  </select>
  <select class="{pick} max-w-36" bind:value={profile} disabled={harness === TERMINAL || !profiles.length} aria-label="Profile">
    <option value="">{profiles.length ? 'No profile' : 'Default login'}</option>
    {#each profiles as p (p.name)}<option value={p.name}>{p.name}</option>{/each}
  </select>
  <input
    class="min-w-0 flex-1 border-0 bg-transparent px-3 text-[14px] text-ink outline-none disabled:text-faint"
    bind:this={box}
    bind:value={prompt}
    onkeydown={onKey}
    disabled={harness === TERMINAL}
    aria-label="Prompt"
    placeholder={harness === TERMINAL
      ? 'Opens a shell in this worktree. Enter to start.'
      : 'What should it do? Empty opens the CLI. P to focus, Enter to start.'}
  />
  <button
    type="submit"
    class="flex-none border-0 border-l border-rule-strong bg-ink px-4 text-[13px] font-medium text-bg enabled:hover:bg-surface enabled:hover:text-ink disabled:bg-transparent disabled:text-faint"
    disabled={!ready}>{busy ? 'Starting\u2026' : 'Start'}</button
  >
</form>
