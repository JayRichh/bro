<script lang="ts">
  // The scope chip in the header: `all > repo > worktree`, then a session
  // or an account when one narrows it, and an x to show everything again.
  // Each segment narrows back to itself; the arrow opens the picker (repos, the
  // scoped repo's worktrees, accounts). A rail click or a digit sets the
  // repo, a worktree's Scope button the worktree. `model.scope` is the
  // truth (the Tauri side holds it per window); the board, the attention
  // strip, the overview, the quick prompt and the diff all follow it.
  import { app, refreshAccounts, setScope } from '../store.svelte';
  import { openMenu, type MenuEntry } from '../ui/ui.svelte';
  import { keyLabel, keysOf, type ShortcutId } from '../shortcuts';
  import { accountScope, isEverything, repoScope, scopeParts, upTo, worktreeScope, type ScopePart } from '../scope';
  import { railOrder, repoRows } from '../overview/repos';

  let { compact = false }: { compact?: boolean } = $props();

  const s = $derived(app.model.scope ?? {});
  const all = $derived(isEverything(s));
  const parts = $derived(scopeParts(app.model, s));
  // Narrow windows keep the innermost segment only.
  const shown = $derived(compact ? parts.slice(-1) : parts);
  const digits = $derived.by(() => {
    const k = keysOf('repo_1')[0];
    return k ? `${keyLabel(k)}\u20139` : '';
  });

  function menu(at: HTMLElement) {
    refreshAccounts();
    const m = app.model;
    const tick = (on: boolean, label: string) => (on ? `\u2713 ${label}` : label);
    const items: MenuEntry[] = [{ label: tick(all, 'Everything'), run: () => setScope({}) }, { heading: 'repo' }];
    railOrder(repoRows(m)).forEach((r, i) =>
      items.push({
        label: tick(s.repo === r.project.id && !s.worktree, r.name),
        run: () => setScope(repoScope(s, r.project.id)),
        keys: i < 9 ? keysOf(`repo_${i + 1}` as ShortcutId) : undefined,
      }),
    );
    const repo = s.repo ?? m.worktrees.find((w) => w.id === s.worktree)?.project_id;
    const wts = repo ? m.worktrees.filter((w) => w.project_id === repo).sort((a, b) => Number(b.is_main) - Number(a.is_main) || a.sort_order - b.sort_order) : [];
    if (wts.length > 1) {
      items.push({ heading: 'worktree' });
      for (const w of wts) items.push({ label: tick(s.worktree === w.id, w.branch), run: () => setScope(worktreeScope(m, s, w.id)) });
    }
    const accounts = app.accounts.filter((a) => !a.disabled);
    if (accounts.length) {
      items.push({ heading: 'account' }, { label: tick(!s.account, 'Any account'), run: () => setScope(accountScope(s, null)) });
      for (const a of accounts) items.push({ label: tick(s.account === a.key, a.name), run: () => setScope(accountScope(s, a.key)) });
    }
    openMenu(items, at, 'Scope');
  }

  function partTitle(p: ScopePart): string {
    if (p.kind === 'repo') return `Repo ${p.label}: show all of it (drops the worktree)`;
    if (p.kind === 'worktree') return `Worktree ${p.label}`;
    if (p.kind === 'session') return `Session ${p.label}`;
    return `Account ${p.label}`;
  }

  const seg = 'min-w-0 truncate border-0 bg-transparent px-1.5 text-[12px] hover:bg-surface hover:text-ink';
</script>

<div
  class="flex h-6 max-w-[36cqw] min-w-0 flex-none items-center border {all ? 'border-rule' : 'border-ink'}"
  role="group"
  aria-label="Scope"
>
  {#if all}
    <button
      class="{seg} h-full text-muted"
      title="Scope: everything. Click to narrow the board, the overview and the counts to a repo, a worktree or an account (a repo in the rail, or {digits || 'the digit keys'}, does it too)"
      aria-haspopup="menu"
      onclick={(e) => menu(e.currentTarget as HTMLElement)}>all &#9662;</button
    >
  {:else}
    {#if !compact}
      <button class="{seg} h-full flex-none text-muted" title="Show everything (the x does it too)" onclick={() => setScope({})}>all</button>
    {/if}
    {#each shown as p (p.kind)}
      {#if p.kind === 'repo' || p.kind === 'worktree' || !compact}
        <span class="flex-none text-[11px] text-faint" aria-hidden="true">{p.kind === 'account' || p.kind === 'session' ? '\u00B7' : '\u25B8'}</span>
      {/if}
      <button
        class="{seg} h-full {p.kind === 'repo' ? '' : 'font-mono'} text-ink"
        title={partTitle(p)}
        onclick={(e) => (p.kind === 'repo' || p.kind === 'worktree' ? setScope(upTo(s, p.kind)) : menu(e.currentTarget as HTMLElement))}>{p.label}</button
      >
    {/each}
    <button
      class="grid h-full w-5 flex-none place-items-center border-0 border-l border-rule bg-transparent p-0 text-[11px] text-muted hover:bg-surface hover:text-ink"
      title="Narrow to another repo, worktree or account"
      aria-label="Change the scope"
      aria-haspopup="menu"
      onclick={(e) => menu(e.currentTarget as HTMLElement)}>&#9662;</button
    >
    <button
      class="grid h-full w-6 flex-none place-items-center border-0 border-l border-rule bg-transparent p-0 text-muted hover:bg-surface hover:text-ink"
      title="Clear the scope: show every repo, worktree and account (no key; {digits || 'the digits'} pick a repo)"
      aria-label="Clear the scope"
      onclick={() => setScope({})}
    >
      <svg width="10" height="10" viewBox="0 0 12 12" aria-hidden="true"><path d="M2 2l8 8M10 2l-8 8" stroke="currentColor" stroke-width="1.4" /></svg>
    </button>
  {/if}
</div>
