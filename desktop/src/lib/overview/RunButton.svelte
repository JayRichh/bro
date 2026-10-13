<script lang="ts">
  // Run or stop a worktree's run command (the project's Run command
  // setting, else its .kete.json "run"), as the TUI's menu Run does.
  import { app, toast } from '../store.svelte';
  import { api, type Worktree } from '../api';

  let { worktree, label }: { worktree: Worktree; label: string } = $props();

  const command = $derived(app.runCommands[worktree.id]);
  const live = $derived(app.model.terminals.some((t) => t.worktree_id === worktree.id && t.run_command && t.alive));
  let busy = $state(false);

  async function press(e: MouseEvent) {
    e.stopPropagation();
    busy = true;
    try {
      if (live) await api.stopRun(worktree.id);
      else await api.startRun(worktree.id);
    } catch (err) {
      toast(`${live ? 'Could not stop' : 'Could not start'} the run: ${err}`);
    } finally {
      busy = false;
    }
  }
</script>

{#if command || live}
  <button
    class="inline-flex flex-none items-center gap-1.5 border-0 bg-transparent p-0 font-mono text-[12px] whitespace-nowrap enabled:hover:underline enabled:hover:underline-offset-4 disabled:text-faint {live
      ? 'text-ink'
      : 'text-muted enabled:hover:text-ink'}"
    title="{live ? 'Stop' : 'Run'} {command ?? 'the run'} in {worktree.branch}"
    aria-label="{live ? 'Stop' : 'Run'} the run command in {worktree.branch}"
    disabled={busy || !app.model.connected}
    onclick={press}
  >
    <svg width="9" height="9" viewBox="0 0 10 10" aria-hidden="true">
      {#if live}<rect x="1" y="1" width="8" height="8" fill="currentColor" />{:else}<path d="M2 1l7 4-7 4z" fill="currentColor" />{/if}
    </svg>
    {live ? 'stop' : 'run'}{label ? ` ${label}` : ''}
  </button>
{/if}
