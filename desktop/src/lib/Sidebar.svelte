<script lang="ts">
  import { app, openSession, openDiff, sessionKey, toast } from './store.svelte';
  import { api, type Worktree } from './api';
  import { toneOf, terminalTone } from './status';
  import StatusDot from './StatusDot.svelte';

  const m = $derived(app.model);
  const projects = $derived([...m.projects].sort((a, b) => a.sort_order - b.sort_order));
  let adding = $state(false);
  let addPath = $state('');

  function worktreesOf(pid: string): Worktree[] {
    return m.worktrees
      .filter((w) => w.project_id === pid)
      .sort((a, b) => Number(b.is_main) - Number(a.is_main) || a.sort_order - b.sort_order);
  }
  function portsOf(wid: string) {
    return m.ports.find((p) => p.worktree === wid);
  }
  function agentsOf(wid: string) {
    return m.agents.filter((a) => a.worktree_id === wid && !a.archived);
  }
  function terminalsOf(wid: string) {
    return m.terminals.filter((t) => t.worktree_id === wid);
  }
  async function newTerminal(wid: string) {
    try {
      const id = await api.createTerminal(wid);
      if (id) openSession({ Terminal: id });
    } catch (e) {
      toast(String(e));
    }
  }
  async function addProject(e: SubmitEvent) {
    e.preventDefault();
    if (!addPath.trim()) return;
    try {
      await api.addProject(addPath);
      addPath = '';
      adding = false;
    } catch (err) {
      toast(String(err));
    }
  }
  const current = $derived(sessionKey(app.session));

  const iconBtn =
    'grid size-5 place-items-center rounded border-0 bg-transparent text-muted hover:bg-sel hover:text-fg-strong';
  const sessionRow =
    'flex w-full items-center gap-[7px] rounded-[5px] border-0 py-[3px] pr-1.5 pl-[22px] text-left text-xs focus-visible:-outline-offset-2';
</script>

<aside class="flex min-h-0 flex-col border-r border-line-soft bg-side">
  <div class="flex h-11 flex-none items-center gap-2 pr-3 pl-3.5">
    <img src="/bro-64.png" alt="" width="22" height="22" draggable="false" class="rounded-[5px]" />
    <span class="text-[15px] font-bold tracking-tight text-fg-strong">bro</span>
    <span
      class="ml-auto inline-flex items-center gap-[5px] text-[10.5px] text-muted"
      title={m.connected ? `daemon pid ${m.daemon_pid}` : 'Not connected'}
    >
      <span class="size-1.5 rounded-full {m.connected ? 'bg-green' : 'bg-red'}"></span>{m.connected ? 'daemon' : 'offline'}
    </span>
  </div>

  <nav class="flex-1 overflow-y-auto px-1.5 pt-1 pb-3" aria-label="Projects and worktrees">
    {#each projects as p (p.id)}
      <div class="mb-2.5">
        <div class="flex items-center gap-[7px] px-2 py-[5px] text-muted" title={p.repo_path}>
          <svg width="14" height="14" viewBox="0 0 16 16" aria-hidden="true"
            ><path d="M1.5 4.5a1.5 1.5 0 0 1 1.5-1.5h3l1.5 1.5H13a1.5 1.5 0 0 1 1.5 1.5v6a1.5 1.5 0 0 1-1.5 1.5H3A1.5 1.5 0 0 1 1.5 12z" fill="none" stroke="currentColor" stroke-width="1.3" stroke-linejoin="round" /></svg
          >
          <span class="truncate text-[12.5px] font-semibold text-fg-strong">{p.name}</span>
        </div>
        {#each worktreesOf(p.id) as w (w.id)}
          {@const ports = portsOf(w.id)}
          <div class="mb-1 ml-2.5 border-l border-line-soft pl-2">
            <div class="group flex min-h-6 items-center gap-1.5 rounded-[5px] px-1 py-[3px] hover:bg-hover">
              <svg class="flex-none text-purple" width="12" height="12" viewBox="0 0 16 16" aria-hidden="true"
                ><circle cx="4.5" cy="3.5" r="1.6" fill="none" stroke="currentColor" stroke-width="1.4" /><circle cx="4.5" cy="12.5" r="1.6" fill="none" stroke="currentColor" stroke-width="1.4" /><circle cx="11.5" cy="5.5" r="1.6" fill="none" stroke="currentColor" stroke-width="1.4" /><path d="M4.5 5.1v5.8M11.5 7.1c0 2.4-3 2.6-7 3.8" fill="none" stroke="currentColor" stroke-width="1.4" /></svg
              >
              <span class="truncate font-mono text-[11.5px] text-fg" title={w.path}>{w.branch}</span>
              <span class="ml-auto flex gap-px opacity-0 transition-opacity group-hover:opacity-100 focus-within:opacity-100">
                <button class={iconBtn} title="Diff against HEAD" aria-label="Diff {w.branch}" onclick={() => openDiff(w.id)}>
                  <svg width="13" height="13" viewBox="0 0 16 16"><path d="M5 2v7M1.5 5.5h7M8 13.5h7" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" /></svg>
                </button>
                <button class={iconBtn} title="New terminal here" aria-label="New terminal in {w.branch}" onclick={() => newTerminal(w.id)}>
                  <svg width="13" height="13" viewBox="0 0 16 16"><path d="M8 3v10M3 8h10" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" /></svg>
                </button>
              </span>
            </div>
            {#if ports && (ports.live.length || ports.block)}
              <div class="flex flex-wrap gap-1 pt-px pr-1 pb-[3px] pl-[22px] font-mono">
                {#each ports.live as port (port)}
                  <button
                    class="inline-flex items-center gap-1 rounded border border-line-soft bg-transparent px-[5px] text-[11px] leading-[17px] text-fg hover:border-green hover:text-fg-strong"
                    title="Open http://localhost:{port} in your browser"
                    onclick={() => api.openUrl(`http://localhost:${port}`).catch((e) => toast(String(e)))}
                  >
                    :{port} <span class="text-[9px] text-green" aria-label="listening">&#9679;</span>
                    <svg class="text-muted" width="10" height="10" viewBox="0 0 16 16" aria-hidden="true"><path d="M6 3H3v10h10v-3M9 2h5v5M14 2L7 9" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round" /></svg>
                  </button>
                {/each}
                {#if !ports.live.length && ports.block}
                  <span
                    class="rounded border border-dashed border-line-soft px-[5px] text-[11px] leading-[17px] text-faint"
                    title="Port block for this worktree; nothing listening"
                  >
                    :{ports.block.first}&ndash;{ports.block.first + ports.block.size - 1}
                  </span>
                {/if}
              </div>
            {/if}
            {#each agentsOf(w.id) as a (a.id)}
              {@const cur = current === `a:${a.id}`}
              <button
                class="{sessionRow} {cur ? 'bg-active text-fg-strong' : 'bg-transparent text-fg hover:bg-hover'}"
                onclick={() => openSession({ Agent: a.id })}
              >
                <StatusDot tone={toneOf(a)} size={12} />
                <span class="truncate">{a.name}</span>
              </button>
            {/each}
            {#each terminalsOf(w.id) as t (t.id)}
              {@const cur = current === `t:${t.id}`}
              <button
                class="{sessionRow} {cur ? 'bg-active text-fg-strong' : 'bg-transparent text-fg hover:bg-hover'}"
                onclick={() => openSession({ Terminal: t.id })}
              >
                <StatusDot tone={terminalTone(t)} size={12} />
                <span class="truncate">{t.run_command ? 'run' : t.name}</span>
              </button>
            {/each}
          </div>
        {/each}
      </div>
    {/each}
  </nav>

  <div class="flex-none border-t border-line-soft p-2">
    {#if adding}
      <form onsubmit={addProject}>
        <!-- svelte-ignore a11y_autofocus -->
        <input
          class="w-full rounded-[5px] border border-focus bg-bg px-2 py-[5px] font-mono text-[11.5px] outline-none"
          bind:value={addPath}
          placeholder="C:\path\to\repo"
          autofocus
          onkeydown={(e) => e.key === 'Escape' && (adding = false)}
        />
      </form>
    {:else}
      <button
        class="flex w-full items-center gap-[7px] rounded-[5px] border-0 bg-transparent px-2 py-[5px] text-xs text-muted enabled:hover:bg-hover enabled:hover:text-fg-strong"
        onclick={() => (adding = true)}
        disabled={!m.connected}
      >
        <svg width="12" height="12" viewBox="0 0 16 16" aria-hidden="true"><path d="M8 3v10M3 8h10" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" /></svg>
        Add project
      </button>
    {/if}
  </div>
</aside>
