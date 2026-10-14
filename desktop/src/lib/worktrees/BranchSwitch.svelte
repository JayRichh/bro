<script lang="ts">
  // The branch switcher (c on a row, the repo menu, the palette): the
  // repo's root checkout moved onto another branch. A fuzzy list of local
  // branches and remote ones with no local twin; Enter switches, or with
  // a name no branch has, creates it. Uncommitted changes stop and ask how
  // they travel: stash, bring along, or discard (pressed twice).
  import { tick } from 'svelte';
  import { app } from '../store.svelte';
  import { ui } from '../ui/ui.svelte';
  import { age } from '../status';
  import { CHOICES, press, refusal, rows, type Choice, type Row } from './branches';
  import { closeBranchSwitch, runSwitch, sw } from './switcher.svelte';

  let query = $state('');
  let at = $state(0);
  let choice = $state(0);
  let input = $state<HTMLInputElement>();
  let box = $state<HTMLDivElement>();
  let list = $state<HTMLUListElement>();

  const root = $derived(app.model.worktrees.find((w) => w.id === sw.worktree));
  const repo = $derived(app.model.projects.find((p) => p.id === sw.project)?.name ?? 'repo');
  const shown = $derived(ui.branchSwitch ? rows(sw.branches, query).slice(0, 200) : []);
  const live = $derived(app.model.agents.filter((a) => a.worktree_id === sw.worktree && !a.archived && a.alive).length);
  const stage = $derived(sw.stage);

  $effect(() => {
    if (!ui.branchSwitch) return;
    query = '';
    at = 0;
    tick().then(() => input?.focus());
  });
  $effect(() => {
    void query;
    at = 0;
  });
  $effect(() => {
    // The question takes the keys: focus its box, Stash first.
    if (stage.kind === 'dirty') {
      choice = 0;
      tick().then(() => box?.focus());
    } else if (stage.kind === 'pick') tick().then(() => input?.focus());
  });

  function go(r: Row | undefined) {
    if (!r || sw.stage.kind !== 'pick') return;
    if (r.kind === 'create') return runSwitch({ name: r.name, remote: false }, { kind: 'create' });
    const why = refusal(r.branch);
    if (why) {
      sw.status = { text: why, error: true };
      return;
    }
    runSwitch({ name: r.branch.name, remote: r.branch.remote }, { kind: 'ask' });
  }

  function pick(c: Choice) {
    const s = sw.stage;
    if (s.kind !== 'dirty') return;
    choice = CHOICES.findIndex((x) => x.id === c);
    const p = press(s.armed, c);
    if (!p.run) {
      sw.stage = { ...s, armed: p.armed };
      sw.status = { text: 'Press d (or Enter) again to throw the changes away.', error: true };
      return;
    }
    sw.stage = { ...s, armed: false };
    runSwitch(s.target, c === 'discard' ? { kind: 'discard', keys: s.keys } : { kind: c });
  }

  function onKey(e: KeyboardEvent) {
    e.stopPropagation();
    const s = sw.stage;
    if (e.key === 'Escape') {
      e.preventDefault();
      if (s.kind === 'dirty') {
        sw.stage = { kind: 'pick' };
        sw.status = null;
      } else closeBranchSwitch();
      return;
    }
    if (s.kind === 'working') return e.preventDefault();
    const down = e.key === 'ArrowDown' || (e.ctrlKey && e.key === 'n');
    const up = e.key === 'ArrowUp' || (e.ctrlKey && e.key === 'p');
    if (s.kind === 'dirty') {
      const key = CHOICES.find((c) => c.key === e.key.toLowerCase() && !e.ctrlKey && !e.altKey);
      if (key) return (e.preventDefault(), pick(key.id));
      if (down || up) {
        e.preventDefault();
        choice = Math.max(0, Math.min(CHOICES.length - 1, choice + (down ? 1 : -1)));
        if (s.armed) sw.stage = { ...s, armed: false };
      } else if (e.key === 'Enter') {
        e.preventDefault();
        pick(CHOICES[choice].id);
      }
      return;
    }
    if (down || up) {
      e.preventDefault();
      at = Math.max(0, Math.min(shown.length - 1, at + (down ? 1 : -1)));
      tick().then(() => list?.querySelector('[aria-selected="true"]')?.scrollIntoView({ block: 'nearest' }));
    } else if (e.key === 'Enter') {
      e.preventDefault();
      go(shown[at]);
    }
  }

  const tag = 'flex-none font-mono text-[11px]';
</script>

{#if ui.branchSwitch}
  <!-- svelte-ignore a11y_no_static_element_interactions, a11y_click_events_have_key_events -->
  <div class="fixed inset-0 z-40 bg-[rgb(28_25_23/0.18)]" onmousedown={closeBranchSwitch}></div>
  <div
    bind:this={box}
    class="fixed top-[12%] left-1/2 z-50 flex max-h-[72vh] w-[min(720px,94vw)] -translate-x-1/2 flex-col border border-rule-strong bg-bg text-ink outline-none"
    role="dialog"
    aria-modal="true"
    aria-label="Switch branch"
    tabindex="-1"
    onkeydown={onKey}
  >
    <header class="flex flex-none items-baseline gap-3 border-b border-rule px-4 py-2">
      <h2 class="m-0 text-[15px] font-semibold">Switch branch</h2>
      <span class="min-w-0 flex-1 truncate font-mono text-[12px] text-muted" title={root?.path}>{repo} root checkout, on {root?.branch ?? '?'}</span>
      {#if sw.fetching}<span class="{tag} text-muted">fetching</span>{/if}
    </header>

    {#if stage.kind === 'dirty'}
      {@const to = stage.target.remote ? stage.target.name.slice(stage.target.name.indexOf('/') + 1) : stage.target.name}
      <div class="flex min-h-0 flex-col gap-3 overflow-y-auto px-4 py-3">
        <p class="m-0 text-[14px]">
          {stage.files.length} uncommitted change{stage.files.length === 1 ? '' : 's'} in the checkout. How should they travel to <span class="font-mono text-[13px]">{to}</span>?
        </p>
        <ul class="m-0 max-h-36 list-none overflow-y-auto border border-rule p-0">
          {#each stage.files as f (f.path)}
            <li class="flex gap-3 border-b border-rule px-2 py-1 font-mono text-[12px] last:border-b-0">
              <span class="w-5 flex-none text-muted">{f.xy.trim() || f.xy}</span><span class="min-w-0 truncate">{f.path}</span>
            </li>
          {/each}
        </ul>
        <ul class="m-0 list-none border border-rule p-0" role="listbox" aria-label="What happens to the changes">
          {#each CHOICES as c, i (c.id)}
            {@const sel = i === choice}
            {@const armed = c.id === 'discard' && stage.armed}
            <li role="option" aria-selected={sel}>
              <button
                class="flex w-full items-baseline gap-3 border-0 border-b border-rule px-2 py-1.5 text-left last:border-b-0 {sel
                  ? armed
                    ? 'bg-fail text-bg'
                    : 'bg-ink text-bg'
                  : 'bg-transparent text-ink hover:bg-surface'}"
                onmouseenter={() => (choice = i)}
                onclick={() => pick(c.id)}
              >
                <kbd class="{tag} w-4 {sel ? 'text-bg' : 'text-muted'}">{c.key}</kbd>
                <span class="flex-none text-[13px] font-medium">{armed ? 'Discard & switch: press again' : c.label}</span>
                <span class="min-w-0 truncate text-[12px] {sel ? 'text-bg' : 'text-muted'}">{c.detail(to)}</span>
              </button>
            </li>
          {/each}
        </ul>
      </div>
    {:else}
      <label class="flex h-11 flex-none items-center gap-3 border-b border-rule-strong px-4">
        <span class="font-mono text-[12px] text-muted" aria-hidden="true">&gt;</span>
        <span class="sr-only">Branch</span>
        <input
          bind:this={input}
          bind:value={query}
          disabled={stage.kind === 'working'}
          class="h-full min-w-0 flex-1 border-0 bg-transparent text-[15px] text-ink outline-none placeholder:text-faint"
          placeholder="Filter branches, or type a new name to create it"
          spellcheck="false"
          role="combobox"
          aria-expanded="true"
          aria-controls="branch-list"
        />
        <kbd class="border border-rule px-1.5 font-mono text-[11px] leading-5 text-muted">Esc</kbd>
      </label>
      <ul bind:this={list} id="branch-list" class="m-0 min-h-0 flex-1 list-none overflow-y-auto p-0" role="listbox" aria-label="Branches">
        {#each shown as r, i (r.kind === 'create' ? `+${r.name}` : r.branch.name)}
          {@const sel = i === at}
          <li role="option" aria-selected={sel}>
            {#if r.kind === 'create'}
              <button
                class="flex w-full items-baseline gap-3 border-0 border-b border-rule px-4 py-1.5 text-left {sel ? 'bg-ink text-bg' : 'bg-transparent hover:bg-surface'}"
                onmouseenter={() => (at = i)}
                onclick={() => go(r)}
              >
                <span class="min-w-0 flex-1 truncate text-[14px]">Create <span class="font-mono text-[13px]">{r.name}</span> from {root?.branch ?? 'HEAD'}</span>
                <span class="{tag} {sel ? 'text-bg' : 'text-muted'}">new branch</span>
              </button>
            {:else}
              {@const b = r.branch}
              {@const held = !!b.checked_out_at}
              <button
                class="flex w-full items-baseline gap-3 border-0 border-b border-rule px-4 py-1.5 text-left {sel ? 'bg-ink text-bg' : 'bg-transparent hover:bg-surface'}"
                title={b.checked_out_at ? `Checked out in ${b.checked_out_at}` : b.subject}
                onmouseenter={() => (at = i)}
                onclick={() => go(r)}
              >
                <span class="min-w-0 flex-[0_1_auto] truncate font-mono text-[13px] {held && !sel ? 'text-faint' : ''}"
                  >{#each [...b.name] as ch, ci (ci)}<span class={r.positions.includes(ci) ? 'font-semibold underline underline-offset-2' : ''}>{ch}</span>{/each}</span
                >
                {#if b.current}<span class="{tag} {sel ? 'text-bg' : 'text-ink'}">current</span>{/if}
                {#if held}<span class="{tag} {sel ? 'text-bg' : 'text-muted'}">in a worktree</span>{/if}
                {#if b.remote}<span class="{tag} {sel ? 'text-bg' : 'text-muted'}">remote</span>{/if}
                <span class="min-w-0 flex-1 truncate text-[12px] {sel ? 'text-bg' : 'text-muted'}">{b.subject}</span>
                <span class="{tag} {sel ? 'text-bg' : 'text-muted'}">{age(b.committed * 1000, app.now)}</span>
              </button>
            {/if}
          </li>
        {:else}
          <li class="px-4 py-3 text-[13px] text-muted">{sw.listed ? 'Nothing matches. Type a name to create a branch.' : 'Listing branches\u2026'}</li>
        {/each}
      </ul>
    {/if}

    {#if sw.status || stage.kind === 'working'}
      <p class="m-0 flex-none border-t border-rule px-4 py-1.5 text-[13px] {sw.status?.error ? 'text-fail' : 'text-muted'}" role="status">
        {stage.kind === 'working' ? stage.what : sw.status?.text}
      </p>
    {/if}
    <div class="flex flex-none items-center gap-4 border-t border-rule px-4 py-1.5 font-mono text-[11px] text-muted">
      {#if stage.kind === 'dirty'}
        <span>s b d choose</span><span>Enter pick</span><span>Esc back</span>
      {:else}
        <span>&uarr;&darr; move</span><span>Enter switch</span><span>Esc close</span>
      {/if}
      <span class="ml-auto">
        {sw.changes === null ? '' : sw.changes ? `${sw.changes} uncommitted` : 'clean'}{live ? ` \u00b7 ${live} session${live === 1 ? '' : 's'} running here` : ''}
      </span>
    </div>
  </div>
{/if}
