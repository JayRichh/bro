<script lang="ts">
  // The review sheet: what the orchestrator (or you) put up for review, one
  // session at a time, with tabs: its live Terminal, the Diff of its
  // worktree, its History (the transcript), and its PR (the Azure DevOps
  // review when the repo lives there, else the GitHub link when one is
  // known). Over the window, not in it; Esc closes, [ and ] switch session.
  import { tick } from 'svelte';
  import { app, openSession, projectOf, sessionKey, toast } from '../store.svelte';
  import { api } from '../api';
  import { harnessLabel, placeOf, toneOf } from '../status';
  import Status from '../Status.svelte';
  import Terminal from '../Terminal.svelte';
  import DiffPane from '../DiffPane.svelte';
  import Review from '../ado/Review.svelte';
  import type { DiffMode } from '../diff-api';
  import { ui, type ReviewTab } from '../ui/ui.svelte';
  import { closeReview } from '../orch.svelte';
  import OrchBadge from './OrchBadge.svelte';

  const TABS: { id: ReviewTab; label: string }[] = [
    { id: 'terminal', label: 'Terminal' },
    { id: 'diff', label: 'Diff' },
    { id: 'history', label: 'History' },
    { id: 'pr', label: 'PR' },
  ];

  let box = $state<HTMLElement>();
  let diffMode = $state<DiffMode>('inline');
  let history = $state<{ id: string; text: string | null; error: string | null }>({ id: '', text: null, error: null });

  const r = $derived(ui.review);
  const id = $derived(r ? r.sessions[Math.min(r.at, r.sessions.length - 1)] : '');
  const agent = $derived(app.model.agents.find((a) => a.id === id));
  const where = $derived(agent ? placeOf(app.model, agent.worktree_id) : null);
  const repo = $derived(agent ? (projectOf(agent.worktree_id) ?? '') : '');
  const pr = $derived(agent ? app.model.prs[agent.worktree_id] : undefined);
  const ado = $derived(repo ? app.adoCtx[repo] : null);
  /** The pane beside already shows this session: a second pane on it would steal its output. */
  const beside = $derived(!!agent && sessionKey(app.session) === `a:${agent.id}`);

  $effect(() => {
    if (r) tick().then(() => box?.focus());
  });

  // History: read once per session shown, again on Refresh.
  async function loadHistory(force = false) {
    if (!id || (!force && history.id === id)) return;
    const want = id;
    history = { id: want, text: null, error: null };
    try {
      const text = await api.readSession(want);
      if (history.id === want) history = { id: want, text, error: null };
    } catch (e) {
      if (history.id === want) history = { id: want, text: null, error: String(e) };
    }
  }
  $effect(() => {
    if (r?.tab === 'history') loadHistory();
  });

  function setTab(t: ReviewTab) {
    if (ui.review) ui.review.tab = t;
  }
  function step(d: number) {
    if (!ui.review) return;
    const n = ui.review.sessions.length;
    ui.review.at = (ui.review.at + d + n) % n;
  }
  function openIt() {
    const a = agent;
    closeReview();
    if (a) openSession({ Agent: a.id });
  }

  function onKey(e: KeyboardEvent) {
    const t = e.target as HTMLElement;
    const typing = !!t.closest('input, textarea, select, .xterm, .monaco-editor');
    if (e.key === 'Escape' && !t.closest('.xterm')) {
      e.preventDefault();
      e.stopPropagation();
      closeReview();
    } else if (!typing && (e.key === '[' || e.key === ']')) {
      e.preventDefault();
      step(e.key === ']' ? 1 : -1);
    }
  }

  const tool = 'inline-flex h-7 items-center gap-1.5 border border-rule bg-transparent px-2 text-[13px] text-ink hover:border-rule-strong';
</script>

{#if r}
  <!-- svelte-ignore a11y_no_static_element_interactions, a11y_click_events_have_key_events -->
  <div class="fixed inset-0 z-40 bg-[rgb(28_25_23/0.18)]" onmousedown={closeReview}></div>
  <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
  <div
    bind:this={box}
    class="fixed inset-y-0 right-0 z-50 flex w-[min(1100px,86vw)] flex-col border-l border-rule-strong bg-bg text-ink outline-none"
    role="dialog"
    aria-modal="true"
    aria-label="Review"
    tabindex="-1"
    onkeydown={onKey}
  >
    <header class="flex flex-none flex-col gap-1 border-b border-rule px-4 py-2">
      <div class="flex min-w-0 items-center gap-3">
        <span class="font-mono text-[11px] text-muted">REVIEW</span>
        {#if r.sessions.length > 1}
          <button class="{tool} px-1.5" onclick={() => step(-1)} title="Previous session ([)" aria-label="Previous session">&larr;</button>
          <span class="font-mono text-[12px] text-muted">{r.at + 1} / {r.sessions.length}</span>
          <button class="{tool} px-1.5" onclick={() => step(1)} title="Next session (])" aria-label="Next session">&rarr;</button>
        {/if}
        <span class="min-w-0 truncate text-[15px] font-semibold">{agent?.name ?? 'That session is gone'}</span>
        {#if agent?.role === 'orchestrator'}<OrchBadge />{/if}
        {#if agent}<Status tone={toneOf(agent)} />{/if}
        <span class="flex-1"></span>
        {#if agent}<button class={tool} onclick={openIt} title="Open it in the session pane">Open session</button>{/if}
        <button
          class="grid size-7 place-items-center border border-rule bg-transparent text-ink hover:border-rule-strong"
          onclick={closeReview}
          title="Close (Esc)"
          aria-label="Close the review"
        >
          <svg width="12" height="12" viewBox="0 0 12 12" aria-hidden="true"><path d="M2 2l8 8M10 2l-8 8" stroke="currentColor" stroke-width="1.4" /></svg>
        </button>
      </div>
      {#if agent && where}
        <div class="flex min-w-0 items-baseline gap-3 text-[13px] text-muted">
          <span>{harnessLabel(agent)}{agent.profile ? ` \u00b7 ${agent.profile}` : ''}</span>
          <span class="font-mono text-[12px]" title={where.path}>{where.project} / {where.branch}</span>
        </div>
      {/if}
      <div class="-mb-2 flex items-end gap-0" role="tablist" aria-label="Review tabs">
        {#each TABS as t (t.id)}
          {@const on = r.tab === t.id}
          <button
            role="tab"
            aria-selected={on}
            class="-mb-px border-0 border-b-2 bg-transparent px-3 py-1.5 text-[13px] {on ? 'border-ink font-medium text-ink' : 'border-transparent text-muted hover:text-ink'}"
            onclick={() => setTab(t.id)}>{t.label}</button
          >
        {/each}
      </div>
    </header>

    <div class="relative min-h-0 flex-1" role="tabpanel">
      {#if !agent}
        <p class="p-6 text-[14px] text-muted">That session is gone.</p>
      {:else if r.tab === 'terminal'}
        {#if beside}
          <div class="grid h-full place-items-center p-6 text-center text-[14px] text-muted">
            <p class="m-0 max-w-[48ch]">
              This session is in the pane beside the review. <button class="border-0 bg-transparent p-0 text-ink underline underline-offset-4" onclick={openIt}
                >Show it there</button
              >.
            </p>
          </div>
        {:else}
          {#key agent.id}<Terminal session={{ Agent: agent.id }} dark={app.dark} />{/key}
        {/if}
      {:else if r.tab === 'diff'}
        {#key agent.id}<DiffPane worktree={agent.worktree_id} bind:mode={diffMode} keys={true} />{/key}
      {:else if r.tab === 'history'}
        <div class="flex h-full min-h-0 flex-col">
          <div class="flex flex-none items-center gap-2 border-b border-rule px-4 py-1.5">
            <span class="text-[12px] text-muted">The transcript when kete knows it, else the terminal's recent text.</span>
            <span class="flex-1"></span>
            <button class={tool} onclick={() => loadHistory(true)}>Refresh</button>
          </div>
          {#if history.error}
            <p class="m-4 border border-fail px-3 py-2 text-[13px] text-fail">{history.error}</p>
          {:else if history.text === null}
            <p class="p-4 text-[13px] text-muted">Reading&hellip;</p>
          {:else}
            <pre class="m-0 min-h-0 flex-1 overflow-auto px-4 py-3 font-mono text-[12px] leading-5 whitespace-pre-wrap text-ink select-text">{history.text ||
                'Nothing to show yet.'}</pre>
          {/if}
        </div>
      {:else if r.tab === 'pr'}
        {#if ado && pr}
          <div class="h-full min-h-0 overflow-y-auto">
            {#key `${agent.worktree_id}:${pr.number}`}<Review worktree={agent.worktree_id} {repo} id={pr.number} />{/key}
          </div>
        {:else if pr}
          <div class="flex flex-col items-start gap-3 p-6">
            <p class="m-0 text-[14px] text-ink">
              <span class="font-mono text-[13px]">#{pr.number}</span>
              {pr.title}{pr.draft ? ' (draft)' : ''}
            </p>
            <button class={tool} onclick={() => api.openUrl(pr.url).catch((e) => toast(String(e)))}>Open on GitHub</button>
          </div>
        {:else}
          <p class="p-6 text-[14px] text-muted">
            No pull request for {where?.branch ?? 'this branch'} yet.{ado ? ' Create one from the repo\'s Pull requests view, or ask the orchestrator (kete ado pr create).' : ''}
          </p>
        {/if}
      {/if}
    </div>
  </div>
{/if}
