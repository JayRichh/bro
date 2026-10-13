<script lang="ts">
  // "Sign in again", wherever an ADO view shows a sign-in failure: az login
  // for the repo's org (its tenant when Kete knows it), then the view
  // reloads through `onsignedin`.
  import { signin } from './signin.svelte';

  let { worktree, onsignedin }: { worktree: string | null | undefined; onsignedin?: () => void } = $props();

  async function go() {
    if (!worktree) return;
    if (await signin.signIn(worktree)) onsignedin?.();
  }
</script>

<span class="inline-flex min-w-0 flex-wrap items-center gap-3">
  <button
    class="inline-flex h-7 flex-none items-center border border-rule-strong bg-transparent px-2.5 text-[13px] text-ink enabled:hover:bg-surface disabled:border-rule disabled:text-faint"
    onclick={go}
    disabled={!worktree || signin.busy}>{signin.busy ? 'Signing in\u2026' : 'Sign in again'}</button
  >
  {#if signin.error}<span class="min-w-0 text-[12px] text-fail select-text">{signin.error}</span>{/if}
</span>
