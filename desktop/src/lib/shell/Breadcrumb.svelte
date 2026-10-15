<script lang="ts">
  import { app, goHome, sessionFromKey } from '../store.svelte';
  import { nav } from './history.svelte';
  import { placeOf } from '../status';
  import { parseItem } from '../ado/prs-api';
  import { parseDiffItem } from '../diff-api';

  type Crumb = { label: string; mono?: boolean; go?: () => void };

  const VIEW_LABEL: Record<string, string> = {
    tasks: 'Tasks',
    branches: 'Branches',
    prs: 'Pull requests',
    settings: 'Settings',
    orchestrator: 'Orchestrator',
    accounts: 'Accounts',
  };

  const crumbs = $derived.by((): Crumb[] => {
    const p = nav.current;
    const m = app.model;
    const out: Crumb[] = [{ label: app.home === 'board' ? 'Board' : 'Overview', go: () => goHome() }];
    const project = p.repo ? m.projects.find((x) => x.id === p.repo) : undefined;
    if (project) out.push({ label: project.name, go: () => goHome(project.id) });
    if (VIEW_LABEL[p.view]) out.push({ label: VIEW_LABEL[p.view], go: () => nav.go({ view: p.view, repo: p.repo }) });
    if (p.view === 'branches' && p.item) {
      // `repo / Branches / jari / gst-rounding`: the person, then the rest.
      const cut = p.item.indexOf('/');
      if (cut > 0) out.push({ label: p.item.slice(0, cut), mono: true }, { label: p.item.slice(cut + 1), mono: true });
      else out.push({ label: p.item, mono: true });
    }
    if (p.view === 'prs' && p.item) {
      // `repo / Pull requests / PR 3417`, or `/ New / <branch>` in the composer.
      const it = parseItem(p.item);
      if (it.kind === 'pr') out.push({ label: `PR ${it.id}`, mono: true });
      else if (it.kind === 'compose') out.push({ label: 'New' }, { label: it.branch, mono: true });
    }
    if (p.view === 'session') {
      const s = sessionFromKey(p.item);
      const name =
        s && 'Agent' in s
          ? m.agents.find((a) => a.id === s.Agent)?.name
          : s
            ? m.terminals.find((t) => t.id === s.Terminal)?.name
            : undefined;
      if (name) out.push({ label: name });
    } else if (p.view === 'diff') {
      out.push({ label: 'Diff' });
      const d = parseDiffItem(p.item);
      // A range reads commits, so it names its two ends, not the worktree
      // it happens to be read in.
      const ref = (r: string) => r.replace(/^refs\/(heads|remotes)\//, '');
      if (d.range) out.push({ label: `${ref(d.range.base)}...${ref(d.range.tip)}`, mono: true });
      else if (d.worktree) out.push({ label: placeOf(m, d.worktree).branch, mono: true });
    }
    return out;
  });

  const btn =
    'grid size-7 place-items-center border-0 bg-transparent text-ink enabled:hover:bg-surface disabled:text-faint';
</script>

<nav class="flex min-w-0 items-center gap-1" aria-label="Where you are">
  <button class={btn} onclick={() => nav.back()} disabled={!nav.canBack} title="Back (Alt+Left)" aria-label="Back">
    <svg width="14" height="14" viewBox="0 0 16 16" aria-hidden="true"
      ><path d="M13 8H3.5M7.5 3.5L3 8l4.5 4.5" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="square" /></svg
    >
  </button>
  <button class={btn} onclick={() => nav.forward()} disabled={!nav.canForward} title="Forward (Alt+Right)" aria-label="Forward">
    <svg width="14" height="14" viewBox="0 0 16 16" aria-hidden="true"
      ><path d="M3 8h9.5M8.5 3.5L13 8l-4.5 4.5" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="square" /></svg
    >
  </button>
  <ol class="m-0 ml-2 flex min-w-0 list-none items-center gap-2 p-0">
    {#each crumbs as c, i (i)}
      {@const last = i === crumbs.length - 1}
      {#if i}<li class="flex-none text-faint" aria-hidden="true">/</li>{/if}
      <li class="min-w-0 truncate">
        {#if c.go && !last}
          <button
            class="border-0 bg-transparent p-0 text-[14px] text-muted hover:text-ink hover:underline hover:underline-offset-4 {c.mono ? 'font-mono text-[13px]' : ''}"
            onclick={c.go}>{c.label}</button
          >
        {:else}
          <span class="text-[14px] font-semibold text-ink {c.mono ? 'font-mono text-[13px] font-medium' : ''}" aria-current={last ? 'page' : undefined}
            >{c.label}</span
          >
        {/if}
      </li>
    {/each}
  </ol>
</nav>
