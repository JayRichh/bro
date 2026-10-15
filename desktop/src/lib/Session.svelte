<script lang="ts">
  import { app, goHome, openDiff, projectOf, sessionWorktree } from './store.svelte';
  import { toneOf, terminalTone, harnessLabel, placeOf, age, lastPrompt } from './status';
  import Status from './Status.svelte';
  import Terminal from './Terminal.svelte';
  import OrchBadge from './orchestrator/OrchBadge.svelte';
  import ContextBar from './accounts/ContextBar.svelte';
  import { agentMenu, terminalMenu } from './actions';
  import { openMenu } from './ui/ui.svelte';
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

  const tool =
    'inline-flex h-7 items-center gap-1.5 border border-rule bg-transparent px-2 text-[13px] text-ink hover:border-rule-strong';
</script>

{#if s && (agent || terminal)}
  <div class="flex h-full min-h-0 flex-col">
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div
      class="flex min-w-0 flex-none flex-col gap-1 border-b border-rule px-4 py-2"
      oncontextmenu={(e) => (e.preventDefault(), actions({ x: e.clientX, y: e.clientY }))}
    >
      <div class="flex min-w-0 items-center gap-3">
        <span class="min-w-0 truncate text-[15px] font-semibold {tone === 'needs' ? 'text-ember-ink' : 'text-ink'}"
          >{agent?.name ?? terminal?.name}</span
        >
        {#if agent?.role === 'orchestrator'}<OrchBadge />{/if}
        <Status {tone} />
        <span class="flex-1"></span>
        <button class={tool} onclick={() => openDiff(wt)} title="Diff this worktree (Ctrl+Shift+D)">Diff</button>
        <button
          class={tool}
          aria-haspopup="menu"
          title="Session actions: rename, restart, move, archive, delete (or right-click the header)"
          onclick={(e) => actions(e.currentTarget as HTMLElement)}>Actions</button
        >
        <button
          class="grid size-7 place-items-center border border-rule bg-transparent text-ink hover:border-rule-strong"
          onclick={() => goHome(projectOf(wt))}
          title="Close the split (Esc)"
          aria-label="Close the split"
        >
          <svg width="12" height="12" viewBox="0 0 12 12" aria-hidden="true"><path d="M2 2l8 8M10 2l-8 8" stroke="currentColor" stroke-width="1.4" /></svg>
        </button>
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
    <p>That session is gone. Pick another in the overview.</p>
  </div>
{/if}
