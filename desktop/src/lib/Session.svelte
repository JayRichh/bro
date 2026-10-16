<script lang="ts">
  import { app, goHome, openDiff, projectOf, sessionWorktree } from './store.svelte';
  import { toneOf, terminalTone, harnessLabel, placeOf, age, lastPrompt } from './status';
  import Status from './Status.svelte';
  import StopBadge from './StopBadge.svelte';
  import Terminal from './Terminal.svelte';
  import OrchBadge from './orchestrator/OrchBadge.svelte';
  import ContextBar from './accounts/ContextBar.svelte';
  import { agentMenu, followUp, newTerminal, openEditor, restartAgent, terminalMenu, unarchiveAgent } from './actions';
  import { openMenu } from './ui/ui.svelte';
  import { toggleMaximize } from './commands';
  import { withKey } from './shortcuts';
  import IconButton from './ui/IconButton.svelte';
  import { accountMenu } from './accounts/actions';
  import { accountLabel, canSwitch } from './accounts/view';

  /** The account chip: where this session runs, and the move menu. */
  function accounts(at: HTMLElement) {
    if (agent) openMenu(accountMenu(agent.id), at, 'Account');
  }

  function actions(at: HTMLElement | { x: number; y: number }) {
    const items = agent ? agentMenu(agent) : terminal ? terminalMenu(terminal) : [];
    openMenu(items, at, agent?.name ?? terminal?.name ?? 'Session');
  }

  const s = $derived(app.session);
  const agent = $derived(s && 'Agent' in s ? app.model.agents.find((a) => a.id === s.Agent) : undefined);
  const terminal = $derived(
    s && 'Terminal' in s ? app.model.terminals.find((t) => t.id === s.Terminal) : undefined,
  );
  const wt = $derived(sessionWorktree(s));
  const where = $derived(wt ? placeOf(app.model, wt) : null);
  const tone = $derived(agent ? toneOf(agent) : terminal ? terminalTone(terminal) : 'idle');
  const prompt = $derived(agent ? lastPrompt(agent) : null);
  const target = $derived(agent ? `agent:${agent.id}` : terminal ? `terminal:${terminal.id}` : undefined);
</script>

{#if s && (agent || terminal)}
  <div class="flex h-full min-h-0 flex-col">
    <!-- The header is the session's row: with one of its controls focused,
         the row keys (Space, T, Shift+O, A, Del) act on this session. -->
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div
      class="@container flex min-w-0 flex-none flex-col gap-1 border-b border-rule px-4 py-2"
      data-target={target}
      oncontextmenu={(e) => (e.preventDefault(), actions({ x: e.clientX, y: e.clientY }))}
    >
      <div class="flex min-w-0 items-center gap-3">
        <span class="min-w-0 truncate text-[15px] font-semibold {tone === 'needs' ? 'text-ember-ink' : 'text-ink'}"
          >{agent?.name ?? terminal?.name}</span
        >
        {#if agent?.role === 'orchestrator'}<OrchBadge />{/if}
        <Status {tone} />
        <!-- A usage limit it waits out, or why it stopped: "running, resumes 3:45pm". -->
        <StopBadge note={agent ? app.model.stops?.[agent.id] : null} class="self-center" />
        <span class="flex-1"></span>
        <div class="flex flex-none items-center gap-1" role="toolbar" aria-label="Session">
          {#if agent?.archived}
            <IconButton icon="unarchive" label="Unarchive" word="Unarchive" title={withKey('Unarchive', 'unarchive')} onclick={() => unarchiveAgent(agent.id)} />
          {:else if agent}
            <IconButton
              icon="reply"
              label="Follow-up prompt"
              word="Follow-up"
              title={withKey(agent.alive ? 'Follow-up prompt' : 'Follow-up prompt (not running: restart it first)', 'follow_up')}
              onclick={() => followUp(agent.id)}
            />
            <IconButton icon="restart" label="Restart the agent" title={withKey('Restart the agent', 'restart')} onclick={() => restartAgent(agent.id)} />
          {/if}
          <!-- Secondary verbs give way in a narrow pane; Actions keeps them. -->
          <span class="hidden items-center gap-1 @min-[560px]:flex">
            <IconButton icon="terminal" label="New terminal in this worktree" title={withKey('New terminal in this worktree', 'new_terminal')} onclick={() => newTerminal(wt ?? undefined)} />
            <IconButton icon="editor" label="Open the worktree in the editor" title={withKey('Open the worktree in the editor', 'open_editor')} onclick={() => openEditor(wt ?? undefined)} />
          </span>
          <IconButton icon="diff" label="Diff this worktree" word="Diff" title={withKey('Diff this worktree', ['diff', 'open_diff'])} onclick={() => openDiff(wt)} />
          <IconButton
            icon={app.paneMax ? 'restore' : 'maximize'}
            label={app.paneMax ? 'Bring the pane back down' : 'Full pane'}
            title={withKey(app.paneMax ? 'Bring the pane back down' : 'Full pane: give the session the window', ['maximize_z', 'maximize'])}
            pressed={app.paneMax}
            onclick={toggleMaximize}
          />
          <IconButton
            icon="dots"
            label="Session actions"
            menu
            title={withKey('Session actions: rename, move, duplicate, archive, delete (or right-click the header)', 'row_menu')}
            onclick={(e) => actions(e.currentTarget as HTMLElement)}
          />
          <IconButton icon="close" label="Close the split" title="Close the split (Esc)" onclick={() => goHome(projectOf(wt))} />
        </div>
      </div>
      <div class="flex min-w-0 items-baseline gap-3 text-[13px] text-muted">
        {#if agent}
          <span class="flex-none">{harnessLabel(agent)}</span>
          <button
            class="inline-flex h-5 flex-none items-center gap-1 self-center border border-rule bg-transparent px-1.5 font-mono text-[11px] text-ink hover:border-rule-strong"
            aria-haspopup="menu"
            title={canSwitch(app.model)
              ? `Runs on ${agent.profile?.trim() || "its harness's own login"}. Move it to another account (restarts fresh in this worktree)`
              : `Runs on ${agent.profile?.trim() || "its harness's own login"}. Moving accounts needs a newer daemon`}
            onclick={(e) => accounts(e.currentTarget as HTMLElement)}
            >{accountLabel(agent)}<svg width="7" height="7" viewBox="0 0 8 8" aria-hidden="true"><path d="M1 2.5l3 3 3-3" fill="none" stroke="currentColor" stroke-width="1.2" /></svg
            ></button
          >
        {/if}
        {#if where}
          <span class="flex-none font-mono text-[12px]" title={where.path}>{where.project} / {where.branch}</span>
        {/if}
        {#if agent}<ContextBar context={app.model.contexts?.[agent.id]} wide />{/if}
        {#if prompt}<span class="min-w-0 truncate" title={prompt}>{prompt}</span>{/if}
        {#if agent}<span class="ml-auto flex-none font-mono text-[12px]">{age(agent.status_changed_at, app.now)}</span>{/if}
      </div>
    </div>
    <div class="min-h-0 flex-1">
      <Terminal session={s} dark={app.dark} />
    </div>
  </div>
{:else}
  <div class="grid h-full place-items-center text-[14px] text-muted">
    <div class="flex flex-col items-center gap-3">
      <p class="m-0">That session is gone. Pick another in the overview.</p>
      <button
        class="h-7 border border-rule bg-transparent px-3 text-[13px] text-ink hover:border-rule-strong"
        title="Back to the overview (Esc)"
        onclick={() => goHome(projectOf(wt))}>Back to the overview</button
      >
    </div>
  </div>
{/if}
