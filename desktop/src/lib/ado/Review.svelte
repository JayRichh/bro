<script lang="ts">
  // One pull request: the server's policy checklist, reviewers and votes,
  // Publish for a draft, comment threads with reply, and the diff pinned
  // to the SHAs it opened on.
  import { app, toast } from '../store.svelte';
  import { api } from '../api';
  import { age } from '../status';
  import Chip from './Chip.svelte';
  import Mark from './Mark.svelte';
  import DiffPane from '../DiffPane.svelte';
  import type { DiffMode } from '../diff-api';
  import { policyMark, prMark, prs, VOTES, voteSquare, type PrDetail, type Thread } from './prs-api';
  import { prsState } from './prs-state.svelte';
  import { errKind, errText, type ErrKind } from './errors';
  import { signin } from './signin.svelte';
  import SignInAgain from './SignInAgain.svelte';

  let {
    worktree,
    repo,
    id,
    onchanged,
  }: { worktree: string; repo: string; id: number; onchanged?: () => void } = $props();

  let detail = $state<PrDetail | null>(null);
  let error = $state<string | null>(null);
  let errorKind = $state<ErrKind>('other');
  let loading = $state(false);
  let tab = $state<'overview' | 'changes'>('overview');
  /** The Changes tab opens inline, as the PR detail's column is narrow. */
  let diffMode = $state<DiffMode>('inline');
  let publishing = $state(false);
  let voting = $state<number | null>(null);
  let actionError = $state<string | null>(null);
  let actionKind = $state<ErrKind>('other');
  /** Reply drafts per thread id. */
  let replies = $state<Record<number, string>>({});
  let replying = $state<number | null>(null);

  const note = $derived(prsState.created[repo]?.id === id ? prsState.created[repo].note : null);

  let asked = 0;
  async function load(quiet = false) {
    const mine = ++asked;
    if (!quiet) loading = true;
    error = null;
    try {
      const d = await prs.detail(worktree, id);
      if (mine === asked) detail = d;
    } catch (e) {
      if (mine === asked) {
        error = errText(e);
        errorKind = errKind(e);
      }
    } finally {
      if (mine === asked) loading = false;
    }
  }

  let loadedFor = '';
  $effect(() => {
    const key = `${worktree}\u0000${id}`;
    if (key !== loadedFor) {
      loadedFor = key;
      detail = null;
      tab = 'overview';
      actionError = null;
      load();
    }
  });

  async function publish() {
    publishing = true;
    actionError = null;
    try {
      await prs.publish(worktree, id);
      await load(true);
      onchanged?.();
    } catch (e) {
      actionError = errText(e);
      actionKind = errKind(e);
    } finally {
      publishing = false;
    }
  }

  async function vote(v: number) {
    voting = v;
    actionError = null;
    try {
      await prs.vote(worktree, id, v);
      await load(true);
      onchanged?.();
    } catch (e) {
      actionError = errText(e);
      actionKind = errKind(e);
    } finally {
      voting = null;
    }
  }

  async function reply(t: Thread) {
    const text = (replies[t.id] ?? '').trim();
    if (!text) return;
    replying = t.id;
    try {
      const c = await prs.reply(worktree, id, t.id, text);
      t.comments.push(c);
      replies[t.id] = '';
    } catch (e) {
      toast(errText(e));
    } finally {
      replying = null;
    }
  }

  function when(at: string): string {
    const ms = Date.parse(at);
    return Number.isFinite(ms) ? age(ms, app.now) : '';
  }

  const label = 'm-0 mb-2 text-[12px] font-medium text-muted';
  const btn =
    'inline-flex h-7 items-center border border-rule-strong bg-transparent px-2.5 text-[13px] text-ink enabled:hover:bg-surface disabled:border-rule disabled:text-faint';
</script>

{#if !detail}
  <div class="px-6 py-6 text-[13px]">
    {#if error}
      <p class="m-0 text-fail select-text">{error}</p>
      <div class="mt-3 flex flex-wrap items-center gap-3">
        <button class={btn} onclick={() => load()}>Try again</button>
        {#if errorKind === 'auth'}<SignInAgain {worktree} onsignedin={() => load()} />{/if}
      </div>
    {:else if loading}
      <p class="m-0 font-mono text-[12px] text-muted" role="status">{signin.slow ? 'Signing in\u2026' : `Loading PR ${id}`}</p>
    {/if}
  </div>
{:else}
  {@const pr = detail.pr}
  {@const state = prMark(pr)}
  <div class="flex h-full min-h-0 flex-col">
    <header class="flex-none border-b border-rule px-6 pt-5">
      <div class="flex min-w-0 items-baseline gap-3">
        <h2 class="m-0 min-w-0 text-[18px] leading-snug font-semibold text-balance text-ink">{pr.title}</h2>
        <span class="ml-auto flex-none font-mono text-[12px] text-muted">PR {pr.id}</span>
      </div>
      <div class="mt-2 flex min-w-0 flex-wrap items-center gap-x-4 gap-y-1.5 text-[13px]">
        <Mark square={state.square} word={state.word} strong />
        <span class="min-w-0 truncate font-mono text-[12px] text-ink">{pr.source} <span class="text-muted">&rarr;</span> {pr.target}</span>
        <span class="text-muted">by {pr.createdBy}</span>
        <button
          class="border-0 bg-transparent p-0 text-[13px] text-muted hover:text-ink hover:underline hover:underline-offset-4"
          onclick={() => api.openUrl(pr.url).catch((e) => toast(errText(e)))}>Open in Azure DevOps</button
        >
        {#if loading}<span class="font-mono text-[12px] text-muted" role="status">Refreshing</span>{/if}
      </div>
      <div class="mt-4 flex gap-5" role="tablist" aria-label="PR detail">
        {#each [{ id: 'overview', label: 'Overview' }, { id: 'changes', label: 'Changes' }] as t (t.id)}
          {@const on = tab === t.id}
          <button
            role="tab"
            aria-selected={on}
            class="-mb-px border-0 border-b-2 bg-transparent px-0 pb-2 text-[13px] {on
              ? 'border-rule-strong font-medium text-ink'
              : 'border-bg text-muted hover:text-ink'}"
            onclick={() => (tab = t.id as 'overview' | 'changes')}>{t.label}</button
          >
        {/each}
      </div>
    </header>

    {#if tab === 'changes'}
      <div class="min-h-0 flex-1">
        <DiffPane {worktree} range={{ base: pr.target, tip: `origin/${pr.source}` }} bind:mode={diffMode} />
      </div>
    {:else}
      <div class="min-h-0 flex-1 overflow-y-auto px-6 pt-5 pb-10">
        {#if note}
          <p class="m-0 mb-4 text-[13px] text-muted">{note}</p>
        {/if}

        {#if pr.isDraft}
          <div class="mb-6 flex items-center gap-4 border-y border-rule py-3">
            <p class="m-0 min-w-0 flex-1 text-[13px] text-ink">
              Draft. The server does not evaluate policies and nobody can vote until it is published.
            </p>
            <button
              class="inline-flex h-8 flex-none items-center border-0 bg-ink px-3 text-[13px] font-medium text-bg enabled:hover:bg-muted disabled:bg-faint"
              onclick={publish}
              disabled={publishing}>{publishing ? 'Publishing' : 'Publish'}</button
            >
          </div>
        {/if}
        {#if actionError}
          <p class="m-0 mb-4 text-[13px] text-fail select-text">{actionError}</p>
          {#if actionKind === 'auth'}<div class="mb-4"><SignInAgain {worktree} onsignedin={() => load(true)} /></div>{/if}
        {/if}

        <section class="mb-7">
          <h3 class={label}>Task</h3>
          {#if pr.workItems.length}
            <div class="flex flex-wrap gap-2">
              {#each pr.workItems as w (w)}
                <Chip kind="task" label={`#${w}`} place={{ view: 'tasks', repo, item: String(w) }} />
              {/each}
            </div>
          {:else}
            <p class="m-0 text-[13px] text-muted">No task linked.</p>
          {/if}
        </section>

        <section class="mb-7">
          <h3 class={label}>Policy</h3>
          {#if detail.policies.length}
            <ul class="m-0 list-none border-t border-rule p-0">
              {#each detail.policies as p, i (i)}
                {@const m = policyMark(p.state, pr.isDraft)}
                <li class="grid grid-cols-[minmax(0,1fr)_200px_72px] items-center gap-3 border-b border-rule py-2 text-[13px]">
                  <span class="min-w-0 truncate text-ink">{p.name}</span>
                  <Mark square={m.square} word={m.word} strong={p.state === 'rejected'} />
                  <span class="text-right text-[12px] text-muted">{p.blocking ? 'required' : 'optional'}</span>
                </li>
              {/each}
            </ul>
          {:else}
            <p class="m-0 text-[13px] text-muted">No branch policies apply to {pr.target}.</p>
          {/if}
        </section>

        <section class="mb-7">
          <h3 class={label}>Reviewers</h3>
          {#if pr.reviewers.length}
            <ul class="m-0 list-none border-t border-rule p-0">
              {#each pr.reviewers as r (r.id)}
                <li class="grid grid-cols-[minmax(0,1fr)_200px_72px] items-center gap-3 border-b border-rule py-2 text-[13px]">
                  <span class="min-w-0 truncate text-ink">{r.name}</span>
                  <Mark square={voteSquare(r.vote)} word={r.label} strong={r.vote !== 0} />
                  <span class="text-right text-[12px] text-muted">{r.required ? 'required' : 'optional'}</span>
                </li>
              {/each}
            </ul>
          {:else}
            <p class="m-0 text-[13px] text-muted">No reviewers yet.</p>
          {/if}
          <div class="mt-3 flex flex-wrap items-center gap-2" role="group" aria-label="Your vote">
            {#each VOTES as v (v.vote)}
              <button class={btn} onclick={() => vote(v.vote)} disabled={pr.isDraft || voting !== null}>
                <span class="mr-1.5 size-1.5 flex-none {voteSquare(v.vote)}" aria-hidden="true"></span>{voting === v.vote
                  ? 'Voting'
                  : v.label}
              </button>
            {/each}
          </div>
          {#if pr.isDraft}
            <p class="m-0 mt-2 text-[12px] text-muted">Voting opens once the PR is published.</p>
          {/if}
        </section>

        <section>
          <h3 class={label}>{detail.threads.length} {detail.threads.length === 1 ? 'thread' : 'threads'}</h3>
          {#each detail.threads as t (t.id)}
            <article class="mb-4 border-t border-rule pt-3">
              <div class="mb-2 flex min-w-0 items-center gap-3 text-[12px]">
                {#if t.file}
                  <span class="min-w-0 truncate font-mono text-ink" title={t.file}
                    >{t.file.replace(/^\//, '')}{t.line ? `:${t.line}` : ''}</span
                  >
                {:else}
                  <span class="text-muted">On the PR</span>
                {/if}
                <span class="ml-auto flex-none">
                  <Mark
                    square={t.status === 'active' || t.status === 'pending' ? 'border border-running' : 'bg-faint'}
                    word={t.status}
                  />
                </span>
              </div>
              <ol class="m-0 list-none p-0">
                {#each t.comments as c, ci (ci)}
                  <li class="mb-2.5">
                    <div class="flex items-baseline gap-2">
                      <span class="text-[13px] font-medium text-ink">{c.author}</span>
                      <span class="font-mono text-[12px] text-muted">{when(c.at)}</span>
                    </div>
                    <p class="m-0 mt-0.5 max-w-[72ch] text-[13px] whitespace-pre-wrap text-ink select-text">{c.text}</p>
                  </li>
                {/each}
              </ol>
              <form
                class="flex items-end gap-2"
                onsubmit={(e) => {
                  e.preventDefault();
                  reply(t);
                }}
              >
                <textarea
                  class="min-h-8 flex-1 resize-y border border-rule bg-bg px-2 py-1.5 text-[13px] text-ink outline-none focus:border-rule-strong"
                  rows="1"
                  placeholder="Reply"
                  aria-label="Reply to this thread"
                  bind:value={replies[t.id]}
                  onkeydown={(e) => {
                    if (e.key === 'Enter' && (e.ctrlKey || e.metaKey)) {
                      e.preventDefault();
                      reply(t);
                    }
                  }}
                ></textarea>
                <button class={btn} type="submit" disabled={replying === t.id || !(replies[t.id] ?? '').trim()}
                  >{replying === t.id ? 'Sending' : 'Reply'}</button
                >
              </form>
            </article>
          {:else}
            <p class="m-0 text-[13px] text-muted">No comments yet.</p>
          {/each}
        </section>
      </div>
    {/if}
  </div>
{/if}
