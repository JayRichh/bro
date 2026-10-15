<script lang="ts">
  // Settings, Accounts: the profiles list lives in the Accounts view (every
  // verb, their use and their logins in one place); this tab says so and
  // opens it. Shift+U opens it from anywhere.
  import { app } from '../store.svelte';
  import { openAccounts } from '../accounts/actions';
  import { keysOf } from '../shortcuts';

  const keys = keysOf('accounts');
  const count = $derived(app.launch.profiles.length);
</script>

<div class="max-w-[920px] px-6 pt-5">
  <div class="flex flex-wrap items-baseline gap-x-3 gap-y-2 border-b border-rule-strong pb-1.5">
    <h2 class="m-0 text-[15px] font-semibold text-ink">Accounts</h2>
    <span class="font-mono text-[12px] text-muted">the same list as kete profile</span>
  </div>
  <p class="m-0 mt-2 max-w-[68ch] text-[13px] text-muted">
    A profile is another account for a harness: its own login in its own home, or the CLI's default login. Adding, editing (variables included), duplicating,
    renaming, switching off and removing them all happen in the Accounts view, beside each one's use, login and sessions.
  </p>
  <div class="mt-4 flex flex-wrap items-center gap-3">
    <button
      class="h-7 border-0 bg-ink px-3 text-[13px] font-medium text-bg"
      onclick={openAccounts}
      title="Open the Accounts view{keys.length ? ` (${keys.join(', ')})` : ''}">Manage accounts</button
    >
    {#if keys.length}<kbd class="border border-rule px-1.5 font-mono text-[11px] text-muted">{keys[0]}</kbd>{/if}
    <span class="font-mono text-[12px] text-muted">{count} {count === 1 ? 'profile' : 'profiles'}</span>
  </div>
</div>
