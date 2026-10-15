<script lang="ts">
  // One checkout's header in the overview, as the terminal app's grid
  // draws a band's rule: the root (\u2302) or a linked worktree (\u21b3), its
  // branch, where it stands in git, its pull request, its run and ports,
  // and what is in it. Its verbs show on hover or focus, and on a right
  // click. Dropped on its header, a session moves there and a worktree
  // lands above or below it (by the pointer's half; the root stays
  // first). Its sessions and terminals follow it, folded away with it.
  import { app, toast } from '../store.svelte';
  import { api } from '../api';
  import { openMenu } from '../ui/ui.svelte';
  import { age } from '../status';
  import RunButton from './RunButton.svelte';
  import SessionTree from './SessionTree.svelte';
  import { type WorktreeGroup, isEmpty, prLabel, worktreeKey } from '../worktrees/groups';
  import { isCollapsed, rows, toggleCollapsed, setCollapsed } from '../worktrees/rows.svelte';
  import { verbTitle, verbsFor, worktreeMenu } from '../worktrees/verbs';
  import { drag, dropWorktree, endDrag, leftFor, moveOnto, offsetIn, startDrag, worktreeDropOk } from '../reorder.svelte';
  import { dropZone, type Zone } from '../order';
  import { withKey } from '../shortcuts';

  let { repo, group }: { repo: string; group: WorktreeGroup } = $props();

  const w = $derived(group.worktree);
  const key = $derived(worktreeKey(w.id));
  const folded = $derived(isCollapsed(key));
  const git = $derived(rows.git[w.id]);
  const verbs = $derived(verbsFor(w));
  const changes = $derived(git ? git.changed + git.untracked : null);
  const empty = $derived(isEmpty(group));
  /** Where a drag over the header would land: onto it (a session), or
   *  above or below it (a worktree). */
  let over = $state<Zone | null>(null);

  const plural = (n: number, one: string) => `${n} ${one}${n === 1 ? '' : 's'}`;
  const contents = $derived(
    empty
      ? 'empty'
      : [group.sessions && plural(group.sessions, 'session'), group.terminals.length && plural(group.terminals.length, 'terminal')]
          .filter(Boolean)
          .join(' \u00b7 '),
  );

  function menu(e: MouseEvent) {
    e.preventDefault();
    openMenu(worktreeMenu(w), { x: e.clientX, y: e.clientY }, w.branch);
  }

  /** The row's own keys; the shortcut table's row keys (t, Shift+O, c)
   *  reach it through `data-target`. */
  function onKey(e: KeyboardEvent) {
    if (e.target !== e.currentTarget || e.ctrlKey || e.altKey || e.metaKey) return;
    const run = (id: string) => verbs.find((v) => v.id === id)?.run();
    if (e.key === 'Enter' || e.key === ' ') toggleCollapsed(key);
    else if (e.key === 'ArrowLeft' && !folded) setCollapsed(key, true);
    else if (e.key === 'ArrowRight' && folded) setCollapsed(key, false);
    else if (e.key === 's' && !e.shiftKey) run('session');
    else if (e.key === 'd' && !e.shiftKey) run('diff');
    else if (e.key === 'Delete' && !w.is_main) run('delete');
    else return;
    e.preventDefault();
    e.stopPropagation();
  }

  function dragOver(e: DragEvent) {
    const d = drag.item;
    let zone: Zone | null = null;
    if (d?.kind === 'agent') zone = d.worktree === w.id ? null : 'onto';
    else if (worktreeDropOk(d, w.project_id, w.id)) {
      const [y, h] = offsetIn(e);
      // Nothing goes above the root.
      zone = w.is_main ? 'after' : dropZone(y, h, { insert: true, onto: false });
    }
    over = zone;
    if (!zone || !e.dataTransfer) return;
    e.preventDefault();
    e.dataTransfer.dropEffect = 'move';
  }

  async function drop(e: DragEvent) {
    const d = drag.item;
    const zone = over;
    over = null;
    if (!d || !zone) return;
    e.preventDefault();
    endDrag();
    if (d.kind === 'agent' && zone === 'onto') await moveOnto(d.id, w.id);
    else if (d.kind === 'worktree' && zone !== 'onto') await dropWorktree(w.project_id, d.id, w.id, zone);
  }

  const meta = 'font-mono text-[12px] whitespace-nowrap';
</script>

<div class="@container/wt" data-worktree={w.id}>
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div
    class="group/wt relative flex min-h-8 min-w-0 items-center gap-3 border-b border-rule bg-bg pr-4 pl-6 {over === 'onto' && drag.item
      ? 'outline-2 -outline-offset-2 outline-ember'
      : ''}"
    oncontextmenu={menu}
    ondragenter={dragOver}
    ondragover={dragOver}
    ondragleave={(e) => leftFor(e) && (over = null)}
    ondrop={drop}
  >
    {#if (over === 'before' || over === 'after') && drag.item}
      <span class="pointer-events-none absolute inset-x-0 z-10 h-px bg-ember {over === 'before' ? '-top-px' : '-bottom-px'}" aria-hidden="true"></span>
    {/if}
    <div
      role="button"
      tabindex="0"
      data-row
      data-target="worktree:{w.id}"
      aria-expanded={!folded}
      aria-label="{w.is_main ? 'Root checkout' : 'Worktree'} {w.branch}, {contents}. Enter folds; s new session, t terminal, d diff{w.is_main ? '' : ', Delete deletes'}"
      title="{w.path}. Click or Enter to {folded ? 'unfold' : 'fold'}; right-click for its actions{w.is_main ? '' : `; ${withKey('drag to reorder', ['move_up', 'move_down'])}`}."
      draggable={!w.is_main}
      ondragstart={(e) => startDrag(e, { kind: 'worktree', id: w.id, project: w.project_id })}
      ondragend={() => ((over = null), endDrag())}
      class="flex min-w-0 flex-1 cursor-default items-center gap-2 self-stretch overflow-hidden outline-none focus-visible:outline-2 focus-visible:-outline-offset-2 focus-visible:outline-rule-strong"
      onclick={() => toggleCollapsed(key)}
      onkeydown={onKey}
    >
      <svg class="flex-none text-muted" width="10" height="10" viewBox="0 0 10 10" aria-hidden="true"
        ><path d={folded ? 'M3.5 1.5L7 5 3.5 8.5' : 'M1.5 3.5L5 7l3.5-3.5'} fill="none" stroke="currentColor" stroke-width="1.4" /></svg
      >
      <span class="w-3 flex-none text-center font-mono text-[13px] text-muted" aria-hidden="true">{w.is_main ? '\u2302' : '\u21b3'}</span>
      <!-- The branch keeps its name (up to half the row); the counts after it clip. -->
      <span class="max-w-[50%] min-w-0 flex-none truncate font-mono text-[13px] font-medium text-ink">{w.branch}</span>
      {#if git}
        {#if git.ahead || git.behind}
          <span class="{meta} text-muted" title="Against {git.upstream}: {git.ahead ?? 0} ahead, {git.behind ?? 0} behind"
            >{git.ahead ? `\u2191${git.ahead}` : ''}{git.ahead && git.behind ? ' ' : ''}{git.behind ? `\u2193${git.behind}` : ''}</span
          >
        {:else if !git.upstream && !w.is_main}
          <span class="{meta} text-faint @max-[620px]/wt:hidden" title="This branch has no upstream yet">no upstream</span>
        {/if}
        <span
          class="{meta} {changes ? 'text-git-modified' : 'text-faint'}"
          title={changes ? `${git.changed} changed, ${git.untracked} untracked` : 'No uncommitted changes'}
          >{changes ? `${changes} changed` : 'clean'}</span
        >
      {/if}
      <span class="{meta} text-muted @max-[520px]/wt:hidden">{contents}</span>
      {#if group.activity}<span class="{meta} text-faint @max-[620px]/wt:hidden" title="Last session activity">{age(group.activity, app.now)}</span>{/if}
    </div>

    {#if group.pr}
      {@const pr = group.pr}
      <button
        class="{meta} flex-none border border-rule bg-transparent px-1 leading-4 hover:border-rule-strong {pr.state === 'merged'
          ? 'text-merged'
          : pr.state === 'closed'
            ? 'text-muted line-through'
            : 'text-ink'}"
        title="{pr.title} ({prLabel(pr)}). Open it."
        onclick={() => api.openUrl(pr.url).catch((e) => toast(String(e)))}>{prLabel(pr)}</button
      >
    {/if}
    {#each group.ports as port (port)}
      <button
        class="{meta} flex-none border-0 bg-transparent p-0 text-muted hover:text-ink hover:underline hover:underline-offset-4 @max-[620px]/wt:hidden"
        title="Open http://localhost:{port}"
        onclick={() => api.openUrl(`http://localhost:${port}`).catch((e) => toast(String(e)))}>:{port}</button
      >
    {/each}
    <RunButton worktree={w} label="" />

    <!-- The verbs: on hover or focus, the menu button always. -->
    <span
      class="flex flex-none items-center gap-0.5 opacity-0 group-focus-within/wt:opacity-100 group-hover/wt:opacity-100 @max-[960px]/wt:hidden"
    >
      {#each verbs.filter((v) => v.id !== 'run') as v (v.id)}
        <button
          class="h-6 border border-transparent bg-transparent px-1.5 text-[12px] hover:border-rule {v.danger
            ? 'text-fail'
            : 'text-muted hover:text-ink'}"
          title={verbTitle(v)}
          onclick={() => v.run()}>{v.short}</button
        >
      {/each}
    </span>
    <button
      class="grid size-6 flex-none place-items-center border border-transparent bg-transparent p-0 text-muted hover:border-rule hover:text-ink"
      title="Worktree actions: new session, terminal, diff, editor, folder, copy path, run, delete (or right-click; {withKey('on the row', 'row_menu')})"
      aria-label="{w.branch} actions"
      aria-haspopup="menu"
      onclick={(e) => openMenu(worktreeMenu(w), e.currentTarget as HTMLElement, w.branch)}
    >
      <svg width="12" height="12" viewBox="0 0 12 12" aria-hidden="true"
        ><rect x="1" y="5" width="2" height="2" fill="currentColor" /><rect x="5" y="5" width="2" height="2" fill="currentColor" /><rect
          x="9"
          y="5"
          width="2"
          height="2"
          fill="currentColor"
        /></svg
      >
    </button>
  </div>
  {#if !folded && !empty}
    <SessionTree {repo} forest={group.forest} terminals={group.terminals} />
  {/if}
</div>
