<script lang="ts">
  // The PR composer for one branch: title from the task, the description
  // template (Summary, Changes, Testing, Task), task and reviewer pickers
  // with their "n/a" escapes, Draft on, and the diff beside the form. A
  // failed create keeps everything typed.
  import { app } from '../store.svelte';
  import { nav } from '../shell/history.svelte';
  import Mark from './Mark.svelte';
  import PrDiff from './PrDiff.svelte';
  import { BLOCKING, preflight } from './policy';
  import { prItem, prs, type Reviewer, type WorkItem } from './prs-api';
  import { describe, freshDraft, prsState, type ComposeDraft } from './prs-state.svelte';

  let {
    worktree,
    repo,
    branch,
    task: placeTask = null,
  }: { worktree: string; repo: string; branch: string; task?: number | null } = $props();

  const key = $derived(`${repo}\u0000${branch}`);
  // Made outside the derived so reading a draft never writes state.
  $effect.pre(() => {
    if (!prsState.drafts[key]) prsState.drafts[key] = freshDraft();
  });
  const d = $derived<ComposeDraft>(prsState.drafts[key] ?? freshDraft());

  const check = $derived(preflight({ task: d.task?.id ?? null, noTask: d.noTask, reviewers: d.reviewers, noReviewer: d.noReviewer }));

  // ---- Seed: target, Changes from the commit subjects, the task ----
  let seededFor = '';
  $effect(() => {
    const k = key;
    const draft = prsState.drafts[k];
    if (!draft || draft.seeded || seededFor === k) return;
    seededFor = k;
    seed(draft);
  });

  async function seed(draft: ComposeDraft) {
    try {
      const s = await prs.composeSeed(worktree, branch);
      draft.target ||= s.target;
      if (!draft.changes) draft.changes = s.subjects.map((x) => `- ${x}`).join('\n');
      const id = placeTask ?? s.task;
      if (id && !draft.task && !draft.noTask) {
        const title = s.task === id ? (s.taskTitle ?? '') : (tasks.find((t) => t.id === id)?.title ?? '');
        draft.task = { id, title };
      }
      if (!draft.title) draft.title = draft.task?.title || (s.subjects.length === 1 ? s.subjects[0] : '');
    } catch (e) {
      draft.seedError = String(e);
      draft.target ||= 'main';
    } finally {
      draft.seeded = true;
    }
  }

  /** The diff preview's target: taken when seeded, then on commit of an
   *  edit (Enter or leaving the field), never per keystroke. */
  let diffBase = $state('');
  $effect(() => {
    if (d.seeded && !diffBase) diffBase = d.target.trim();
  });

  // ---- Task picker ----
  let tasks = $state<WorkItem[]>([]);
  let tasksError = $state<string | null>(null);
  let taskQuery = $state('');
  let taskOpen = $state(false);
  $effect(() => {
    const wt = worktree;
    prs
      .tasks(wt, 'myOpen')
      .then((l) => {
        tasks = l.groups.flatMap((g) => g.items);
        // A task picked by id before the list came gets its title now.
        const t = prsState.drafts[key]?.task;
        if (t && !t.title) t.title = tasks.find((x) => x.id === t.id)?.title ?? '';
      })
      .catch((e) => (tasksError = String(e)));
  });
  const taskMatches = $derived.by(() => {
    const q = taskQuery.trim().toLowerCase().replace(/^#/, '');
    const list = q ? tasks.filter((t) => String(t.id).includes(q) || t.title.toLowerCase().includes(q)) : tasks;
    return list.slice(0, 7);
  });
  const typedId = $derived(/^#?\d{1,7}$/.test(taskQuery.trim()) ? Number(taskQuery.trim().replace('#', '')) : null);

  function pickTask(id: number, title: string) {
    d.task = { id, title };
    if (!d.title.trim()) d.title = title;
    taskQuery = '';
    taskOpen = false;
  }

  // ---- Reviewer picker: recent first, then the identity search ----
  let recent = $state<Reviewer[]>([]);
  let found = $state<Reviewer[]>([]);
  let revQuery = $state('');
  let revOpen = $state(false);
  let searching = $state(false);
  let revError = $state<string | null>(null);
  $effect(() => {
    prs
      .recentReviewers(worktree)
      .then((r) => (recent = r))
      .catch(() => {});
  });
  let timer: ReturnType<typeof setTimeout> | undefined;
  $effect(() => {
    const q = revQuery.trim();
    clearTimeout(timer);
    if (!q) {
      found = [];
      searching = false;
      return;
    }
    searching = true;
    timer = setTimeout(() => {
      prs
        .searchIdentities(worktree, q)
        .then((r) => {
          if (revQuery.trim() === q) found = r;
          revError = null;
        })
        .catch((e) => (revError = String(e)))
        .finally(() => revQuery.trim() === q && (searching = false));
    }, 250);
  });
  const revOptions = $derived.by(() => {
    const q = revQuery.trim().toLowerCase();
    const chosen = new Set(d.reviewers.map((r) => r.id));
    const near = recent.filter((r) => !q || r.name.toLowerCase().includes(q) || r.email.toLowerCase().includes(q));
    const out: (Reviewer & { recent: boolean })[] = [];
    for (const r of near) if (!chosen.has(r.id)) out.push({ ...r, recent: true });
    for (const r of found) if (!chosen.has(r.id) && !out.some((o) => o.id === r.id)) out.push({ ...r, recent: false });
    return out.slice(0, 8);
  });
  /** Names that appear more than once show their email to tell them apart. */
  const dupNames = $derived.by(() => {
    const seen = new Map<string, number>();
    for (const r of revOptions) seen.set(r.name, (seen.get(r.name) ?? 0) + 1);
    return seen;
  });

  function addReviewer(r: Reviewer) {
    d.reviewers.push({ id: r.id, name: r.name, email: r.email });
    revQuery = '';
  }

  // ---- The branch's session, for "Ask the session to summarise" ----
  const session = $derived.by(() => {
    const m = app.model;
    const wts = new Set(m.worktrees.filter((w) => w.project_id === repo && w.branch === branch).map((w) => w.id));
    const live = m.agents.filter((a) => wts.has(a.worktree_id) && !a.archived);
    return live.sort((a, b) => b.status_changed_at - a.status_changed_at)[0] ?? null;
  });
  let asking = $state(false);
  let askError = $state<string | null>(null);
  async function askSummary() {
    if (!session) return;
    asking = true;
    askError = null;
    try {
      const answer = await prs.sessionAsk(
        session.id,
        'Summarise the changes on this branch for a pull request description: two to four plain sentences on what changed and why. No headings, no lists.',
      );
      d.summary = answer.trim();
    } catch (e) {
      askError = String(e);
    } finally {
      asking = false;
    }
  }

  // ---- Create ----
  async function create() {
    // A PR exists once there is an outcome: never create twice.
    if (!check.ok || d.busy || !d.title.trim() || d.outcome) return;
    d.busy = true;
    d.error = null;
    d.outcome = null;
    const linkTask = d.noTask ? null : (d.task?.id ?? null);
    d.linkTask = linkTask;
    try {
      const out = await prs.create(worktree, {
        source: branch,
        target: d.target.trim() || 'main',
        title: d.title.trim(),
        description: describe(d),
        draft: d.draft,
        reviewers: d.noReviewer ? [] : d.reviewers,
        workItem: linkTask,
      });
      d.outcome = out;
      if (out.kind === 'created') opened(out.id, out.note);
    } catch (e) {
      d.error = String(e);
    } finally {
      d.busy = false;
    }
  }

  let retrying = $state(false);
  async function retryLink() {
    const o = d.outcome;
    const task = d.linkTask;
    if (!o || task == null) return;
    retrying = true;
    try {
      await prs.linkTask(worktree, o.id, task);
      opened(o.id, o.note);
    } catch (e) {
      o.reason = String(e);
    } finally {
      retrying = false;
    }
  }

  /** Created: show it in PRs, Mine, with its checklist. */
  function opened(id: number, note: string | null) {
    prsState.created = { id, note };
    delete prsState.drafts[key];
    nav.go({ view: 'prs', repo, item: prItem(id) });
  }

  /** A square checkbox: hollow off, filled on. */
  const box = 'size-3.5 flex-none appearance-none border border-rule-strong bg-bg checked:border-ink checked:bg-ink';
  const field = 'w-full border border-rule bg-bg px-2.5 py-1.5 text-[14px] text-ink outline-none focus:border-rule-strong';
  const label = 'mb-1.5 block text-[12px] font-medium text-muted';
  const btn =
    'inline-flex h-7 flex-none items-center border border-rule-strong bg-transparent px-2.5 text-[13px] text-ink enabled:hover:bg-surface disabled:border-rule disabled:text-faint';
  const option = 'flex w-full min-w-0 items-baseline gap-2 border-0 border-b border-rule bg-transparent px-2.5 py-1.5 text-left text-[13px] hover:bg-surface';
</script>

<div class="flex h-full min-h-0 flex-col">
  <header class="flex flex-none items-end gap-4 border-b border-rule px-6 pt-6 pb-4">
    <div class="min-w-0">
      <h1 class="m-0 text-[28px] leading-tight font-semibold tracking-tight text-ink">New pull request</h1>
      <p class="m-0 mt-1.5 flex min-w-0 items-center gap-2 font-mono text-[13px] text-ink">
        <span class="truncate">{branch}</span>
        <span class="text-muted" aria-label="into">&rarr;</span>
        <input
          class="w-[180px] border-0 border-b border-rule bg-transparent px-0.5 py-0 font-mono text-[13px] text-ink outline-none focus:border-rule-strong"
          bind:value={d.target}
          onchange={() => (diffBase = d.target.trim())}
          aria-label="Target branch"
          spellcheck="false"
        />
      </p>
    </div>
    <span class="flex-1"></span>
    <button class="{btn} border-rule" onclick={() => (nav.canBack ? nav.back() : nav.go({ view: 'prs', repo }))}>Cancel</button>
  </header>

  <div class="grid min-h-0 flex-1 grid-cols-[minmax(400px,500px)_minmax(0,1fr)]">
    <form
      class="flex min-h-0 flex-col overflow-y-auto border-r border-rule px-6 pt-5"
      onsubmit={(e) => {
        e.preventDefault();
        create();
      }}
    >
      {#if d.seedError}<p class="m-0 mb-4 text-[12px] text-muted select-text">Could not read the branch: {d.seedError}</p>{/if}

      <label class="mb-5 block">
        <span class={label}>Title</span>
        <input class={field} bind:value={d.title} placeholder="What this changes" />
      </label>

      <div class="mb-5">
        <div class="mb-1.5 flex items-center gap-3">
          <span class="text-[12px] font-medium text-muted" id="summary-label">Summary</span>
          <span class="flex-1"></span>
          {#if session}
            <button
              type="button"
              class="border-0 bg-transparent p-0 text-[12px] text-ink underline decoration-rule underline-offset-4 enabled:hover:decoration-rule-strong disabled:text-faint disabled:no-underline"
              onclick={askSummary}
              disabled={asking}
              title="Asks {session.name} and puts its answer in Summary">{asking ? `Asking ${session.name}` : 'Ask the session to summarise'}</button
            >
          {/if}
        </div>
        <textarea class="{field} min-h-[76px] resize-y text-[13px]" rows="3" bind:value={d.summary} aria-labelledby="summary-label"></textarea>
        {#if askError}<p class="m-0 mt-1 text-[12px] text-fail select-text">{askError}</p>{/if}
      </div>

      <label class="mb-5 block">
        <span class={label}>Changes</span>
        <textarea class="{field} min-h-[96px] resize-y font-mono text-[12px]" rows="4" bind:value={d.changes} spellcheck="false"></textarea>
      </label>

      <label class="mb-6 block">
        <span class={label}>Testing</span>
        <textarea class="{field} min-h-[56px] resize-y text-[13px]" rows="2" bind:value={d.testing} placeholder="How you checked it"></textarea>
      </label>

      <!-- Task -->
      <fieldset class="m-0 mb-6 border-0 p-0">
        <legend class={label}>Task</legend>
        {#if d.task && !d.noTask}
          <div class="flex min-w-0 items-center gap-3 border border-rule px-2.5 py-1.5">
            <span class="flex-none font-mono text-[12px] text-muted">#{d.task.id}</span>
            <span class="min-w-0 flex-1 truncate text-[13px] text-ink">{d.task.title || 'Task'}</span>
            <button type="button" class="border-0 bg-transparent p-0 text-[12px] text-muted hover:text-ink" onclick={() => (d.task = null)}
              >Change</button
            >
          </div>
        {:else}
          <div class="relative">
            <input
              class="{field} text-[13px] disabled:border-rule disabled:text-faint"
              placeholder={tasksError ? 'Type a task id' : 'Search your open tasks by title or id'}
              bind:value={taskQuery}
              disabled={d.noTask}
              onfocus={() => (taskOpen = true)}
              onblur={() => setTimeout(() => (taskOpen = false), 150)}
              aria-label="Find a task"
            />
            {#if taskOpen && !d.noTask && (taskMatches.length || typedId)}
              <ul class="absolute inset-x-0 top-full z-10 m-0 list-none border border-t-0 border-rule-strong bg-bg p-0" role="listbox">
                {#if typedId && !tasks.some((t) => t.id === typedId)}
                  <li>
                    <button type="button" class={option} onmousedown={() => pickTask(typedId, '')}>
                      <span class="font-mono text-[12px] text-muted">#{typedId}</span><span class="text-ink">Link this id</span>
                    </button>
                  </li>
                {/if}
                {#each taskMatches as t (t.id)}
                  <li>
                    <button type="button" class={option} onmousedown={() => pickTask(t.id, t.title)}>
                      <span class="flex-none font-mono text-[12px] text-muted">#{t.id}</span>
                      <span class="min-w-0 flex-1 truncate text-ink">{t.title}</span>
                      <span class="flex-none text-[12px] text-muted">{t.state}</span>
                    </button>
                  </li>
                {/each}
              </ul>
            {/if}
          </div>
        {/if}
        <label class="mt-2 inline-flex items-center gap-2 text-[13px] text-ink">
          <input type="checkbox" class={box} bind:checked={d.noTask} />
          No task (n/a)
        </label>
      </fieldset>

      <!-- Reviewers -->
      <fieldset class="m-0 mb-6 border-0 p-0">
        <legend class={label}>Reviewers</legend>
        {#if d.reviewers.length}
          <ul class="m-0 mb-2 list-none border-t border-rule p-0">
            {#each d.reviewers as r, i (r.id)}
              <li class="flex min-w-0 items-baseline gap-2 border-b border-rule py-1.5 text-[13px]">
                <span class="min-w-0 truncate text-ink">{r.name}</span>
                <span class="min-w-0 truncate font-mono text-[12px] text-muted">{r.email}</span>
                <button
                  type="button"
                  class="ml-auto grid size-5 flex-none place-items-center self-center border-0 bg-transparent text-muted hover:bg-surface hover:text-ink"
                  aria-label="Remove {r.name}"
                  onclick={() => d.reviewers.splice(i, 1)}
                >
                  <svg width="10" height="10" viewBox="0 0 10 10" aria-hidden="true"
                    ><path d="M1.5 1.5l7 7M8.5 1.5l-7 7" stroke="currentColor" stroke-width="1.4" /></svg
                  >
                </button>
              </li>
            {/each}
          </ul>
        {/if}
        <div class="relative">
          <input
            class="{field} text-[13px] disabled:border-rule disabled:text-faint"
            placeholder="Find people by name or email"
            bind:value={revQuery}
            disabled={d.noReviewer}
            onfocus={() => (revOpen = true)}
            onblur={() => setTimeout(() => (revOpen = false), 150)}
            aria-label="Find a reviewer"
          />
          {#if revOpen && !d.noReviewer && (revOptions.length || searching || revError)}
            <ul class="absolute inset-x-0 top-full z-10 m-0 list-none border border-t-0 border-rule-strong bg-bg p-0" role="listbox">
              {#each revOptions as r (r.id)}
                <li>
                  <button type="button" class={option} onmousedown={() => addReviewer(r)}>
                    <span class="min-w-0 truncate text-ink">{r.name}</span>
                    {#if r.email && (dupNames.get(r.name) ?? 0) > 1}<span class="min-w-0 truncate font-mono text-[12px] text-muted">{r.email}</span>{/if}
                    {#if r.recent}<span class="ml-auto flex-none text-[12px] text-muted">recent</span>{/if}
                  </button>
                </li>
              {/each}
              {#if searching}<li class="px-2.5 py-1.5 font-mono text-[12px] text-muted">Searching</li>{/if}
              {#if revError}<li class="px-2.5 py-1.5 text-[12px] text-fail select-text">{revError}</li>{/if}
            </ul>
          {/if}
        </div>
        <label class="mt-2 inline-flex items-center gap-2 text-[13px] text-ink">
          <input type="checkbox" class={box} bind:checked={d.noReviewer} />
          No reviewer
        </label>
      </fieldset>

      <label class="mb-6 inline-flex items-center gap-2 text-[13px] text-ink">
        <input type="checkbox" class={box} bind:checked={d.draft} />
        Draft
        <span class="text-muted">(reviewers are not notified and nobody votes until you publish)</span>
      </label>

      <div class="sticky bottom-0 -mx-6 mt-auto border-t border-rule bg-bg px-6 pt-3 pb-4">
      {#if check.notes.length}
        <ul class="m-0 mb-4 list-none p-0" aria-label="Before you create">
          {#each check.notes as n (n)}
            {@const block = BLOCKING.has(n)}
            <li class="py-0.5">
              <Mark square={block ? 'bg-fail' : 'border border-muted'} word={n} strong={block} />
            </li>
          {/each}
        </ul>
      {/if}

      <div class="flex items-center gap-3">
        <button
          type="submit"
          class="inline-flex h-8 items-center border-0 bg-ink px-3.5 text-[13px] font-medium text-bg enabled:hover:bg-muted disabled:bg-raised disabled:text-faint"
          disabled={!check.ok || d.busy || !d.title.trim() || !!d.outcome}
          >{d.busy ? 'Pushing and creating' : d.draft ? 'Create draft pull request' : 'Create pull request'}</button
        >
        {#if !d.title.trim()}<span class="text-[12px] text-muted">Add a title</span>{/if}
      </div>

      {#if d.error}
        <p class="m-0 mt-3 text-[13px] text-fail select-text">{d.error}</p>
        <p class="m-0 mt-1 text-[12px] text-muted">Nothing you typed was lost. Fix it and create again.</p>
      {/if}

      {#if d.outcome && d.outcome.kind === 'createdTaskNotLinked'}
        {@const o = d.outcome}
        <div class="mt-4 border-y border-rule py-3 text-[13px]">
          <Mark square="border border-fail" word="Created, task not linked" strong />
          {#if o.reason}<p class="m-0 mt-1.5 text-muted select-text">{o.reason}</p>{/if}
          <div class="mt-2.5 flex items-center gap-3">
            <button type="button" class={btn} onclick={retryLink} disabled={retrying || d.linkTask == null}
              >{retrying ? 'Linking' : d.linkTask != null ? `Retry linking #${d.linkTask}` : 'Retry'}</button
            >
            {#if d.linkTask == null}<span class="text-[12px] text-muted">No task id was sent with the create, so there is nothing to link.</span>{/if}
            <button
              type="button"
              class="border-0 bg-transparent p-0 text-[13px] text-ink underline decoration-rule underline-offset-4 hover:decoration-rule-strong"
              onclick={() => nav.go({ view: 'prs', repo, item: prItem(o.id) })}>Open PR {o.id}</button
            >
          </div>
        </div>
      {:else if d.outcome && d.outcome.kind === 'existing'}
        {@const o = d.outcome}
        <div class="mt-4 flex items-center gap-3 border-y border-rule py-3 text-[13px]">
          <Mark square="bg-merged" word="Already open" strong />
          <button
            type="button"
            class="border-0 bg-transparent p-0 text-[13px] text-ink underline decoration-rule underline-offset-4 hover:decoration-rule-strong"
            onclick={() => nav.go({ view: 'prs', repo, item: prItem(o.id) })}>PR {o.id} from {branch}</button
          >
        </div>
      {/if}
      {#if d.outcome?.note}<p class="m-0 mt-2 text-[12px] text-muted">{d.outcome.note}</p>{/if}
      </div>
    </form>

    <div class="min-h-0 min-w-0">
      {#if diffBase}
        <PrDiff {worktree} base={diffBase} tip={branch} />
      {:else}
        <p class="m-0 px-6 py-6 font-mono text-[12px] text-muted" role="status">Reading the branch</p>
      {/if}
    </div>
  </div>
</div>
