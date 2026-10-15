<script lang="ts">
  // Add account: a harness and a name (checked as the CLI checks it), and
  // whether to adopt the CLI's current login instead of a fresh home. The
  // profile is made at once, then its sign-in runs in a terminal and this
  // sheet folds into a small panel that watches the account until its
  // credentials appear, so the terminal stays usable for the login.
  import { tick } from 'svelte';
  import { api, type AccountRow } from '../api';
  import { app, refreshLaunchables, toast } from '../store.svelte';
  import { addSheet, closeAddAccount, watchSignIn } from './add.svelte';
  import { runLogin } from './actions';
  import { loginTitle } from './usage';
  import { nameProblem, signedIn, suggestName } from './view';

  let harnesses = $state<string[]>([]);
  let harness = $state('claude');
  let name = $state('');
  let adopt = $state(false);
  let makeDefault = $state(false);
  let touched = $state(false);
  let busy = $state(false);
  let error = $state<string | null>(null);
  let box = $state<HTMLElement>();

  const taken = $derived(app.accounts.filter((r) => r.profile).map((r) => r.name));
  const problem = $derived(nameProblem(name, taken));
  const hasDefault = $derived(app.accounts.some((r) => r.profile && r.harness === harness && r.default));

  // Each opening starts the form afresh, on the harness asked for.
  let opened = false;
  $effect(() => {
    const open = addSheet.phase === 'form';
    if (open && !opened) {
      opened = true;
      error = null;
      touched = false;
      busy = false;
      adopt = addSheet.adopt;
      makeDefault = false;
      api
        .profileHarnesses()
        .then((h) => {
          harnesses = h;
          if (!h.includes(harness)) harness = h[0] ?? 'claude';
        })
        .catch(() => {});
      harness = addSheet.harness || app.launch.default_harness || 'claude';
      name = suggestName(harness, taken);
      tick().then(() => box?.querySelector<HTMLInputElement>('input[name="account-name"]')?.select());
    } else if (!open) opened = false;
  });
  // A harness switched before the name was typed takes its own suggestion.
  function pickHarness(h: string) {
    harness = h;
    if (!touched) name = suggestName(h, taken);
  }

  async function create() {
    touched = true;
    if (problem || busy) return;
    busy = true;
    error = null;
    try {
      const out = await api.profileAdd({ name: name.trim(), harness, default: makeDefault, useDefault: adopt, env: '' });
      refreshLaunchables();
      const rows = await api.accounts();
      app.accounts = rows;
      const row = rows.find((r) => r.profile && r.name === name.trim());
      if (row && row.login !== 'logged_in' && row.loginHint) {
        // Signing in is the point of adding one: start it now.
        closeAddAccount();
        if (!(await runLogin(row))) watchSignIn(row.name);
      } else {
        closeAddAccount();
        toast(
          row?.login === 'logged_in'
            ? `Added ${name.trim()}: already signed in.`
            : `Added ${name.trim()}${out.home ? `, home ${out.home}` : ''}. kete knows no sign-in for ${harness}: sign in from a session under it.`,
        );
      }
    } catch (e) {
      error = String(e);
    } finally {
      busy = false;
    }
  }

  function onKey(e: KeyboardEvent) {
    e.stopPropagation();
    if (e.key === 'Escape') (e.preventDefault(), closeAddAccount());
    else if (e.key === 'Enter' && (e.target as HTMLElement).tagName !== 'BUTTON') (e.preventDefault(), create());
  }

  // The signing panel asks for the list every 2s while it is up: the row
  // turns signed in once the CLI writes its credentials.
  const watched = $derived<AccountRow | undefined>(app.accounts.find((r) => r.profile && r.name === addSheet.name));
  const done = $derived(addSheet.phase === 'signing' && signedIn(app.accounts, addSheet.name));
  $effect(() => {
    if (addSheet.phase !== 'signing') return;
    const tickOnce = () =>
      api
        .accounts()
        .then((r) => (app.accounts = r))
        .catch(() => {});
    tickOnce();
    const t = setInterval(tickOnce, 2000);
    return () => clearInterval(t);
  });
  // Once signed in, the panel says so, then goes.
  $effect(() => {
    if (!done) return;
    refreshLaunchables();
    const t = setTimeout(closeAddAccount, 6000);
    return () => clearTimeout(t);
  });

  async function again() {
    if (watched) await runLogin(watched);
  }

  const field = 'h-8 w-full border border-rule bg-bg px-2 text-[13px] text-ink outline-none hover:border-rule-strong focus:border-rule-strong';
  const label = 'text-[12px] font-medium text-muted';
</script>

{#if addSheet.phase === 'form'}
  <!-- svelte-ignore a11y_no_static_element_interactions, a11y_click_events_have_key_events -->
  <div class="fixed inset-0 z-40 bg-[rgb(28_25_23/0.18)]" onmousedown={closeAddAccount}></div>
  <div
    bind:this={box}
    class="fixed top-[14%] left-1/2 z-50 w-[min(520px,calc(100vw-24px))] -translate-x-1/2 border border-rule-strong bg-bg text-ink"
    role="dialog"
    aria-modal="true"
    aria-labelledby="add-account-title"
    tabindex="-1"
    onkeydown={onKey}
  >
    <h2 id="add-account-title" class="m-0 border-b border-rule px-4 py-2.5 text-[15px] font-semibold">Add an account</h2>
    <div class="flex flex-col gap-3 px-4 py-3">
      <label class="flex flex-col gap-1">
        <span class={label}>Harness</span>
        <select class="{field} font-mono text-[12px]" value={harness} onchange={(e) => pickHarness((e.currentTarget as HTMLSelectElement).value)} aria-label="Harness">
          {#each harnesses.length ? harnesses : [harness] as h (h)}<option value={h}>{h}</option>{/each}
        </select>
      </label>
      <label class="flex flex-col gap-1">
        <span class={label}>Name</span>
        <input
          name="account-name"
          class="{field} font-mono text-[12px] {touched && problem ? 'border-fail' : ''}"
          bind:value={name}
          oninput={() => (touched = true)}
          spellcheck="false"
          autocomplete="off"
          aria-invalid={!!(touched && problem)}
          aria-describedby="add-account-name-hint"
        />
        <span id="add-account-name-hint" class="text-[12px] {touched && problem ? 'text-fail' : 'text-muted'}"
          >{touched && problem ? problem : 'Letters, digits, - and _. It names the account\u2019s own home, so it cannot change later.'}</span
        >
      </label>
      <label class="flex items-start gap-2 text-[13px] text-ink">
        <input type="checkbox" class="mt-0.5 size-3.5 accent-[var(--color-ink)]" bind:checked={adopt} />
        <span
          >Adopt my current {harness} login
          <span class="block text-[12px] text-muted"
            >Uses the CLI's own default config instead of a fresh home, so the login you already have becomes one of the accounts.</span
          ></span
        >
      </label>
      <label class="flex items-start gap-2 text-[13px] text-ink">
        <input type="checkbox" class="mt-0.5 size-3.5 accent-[var(--color-ink)]" bind:checked={makeDefault} />
        <span
          >Make it the {harness} default
          {#if hasDefault}<span class="block text-[12px] text-muted">Takes the star from the current default.</span>{/if}</span
        >
      </label>
      {#if error}<p class="m-0 text-[13px] text-fail" role="alert">{error}</p>{/if}
    </div>
    <div class="flex items-center justify-between gap-2 border-t border-rule px-4 py-2.5">
      <span class="text-[12px] text-muted">Then the sign-in runs in a terminal.</span>
      <span class="flex gap-2">
        <button class="h-7 border border-rule bg-transparent px-3 text-[13px] text-ink hover:border-rule-strong" onclick={closeAddAccount} title="Cancel (Esc)"
          >Cancel</button
        >
        <button
          class="h-7 border-0 bg-ink px-3 text-[13px] font-medium text-bg disabled:bg-transparent disabled:text-faint"
          disabled={busy || (touched && !!problem)}
          onclick={create}
          title="Add the account and sign it in (Enter)">{busy ? 'Adding\u2026' : 'Add and sign in'}</button
        >
      </span>
    </div>
  </div>
{:else if addSheet.phase === 'signing'}
  <!-- Not modal: the login runs in the terminal beside it. Top right, clear of the toasts. -->
  <div
    class="fixed top-12 right-4 z-30 w-[min(360px,calc(100vw-32px))] border border-rule-strong bg-bg text-ink"
    role="status"
    aria-live="polite"
    aria-label="Signing in {addSheet.name}"
  >
    <div class="flex items-center gap-2 border-b border-rule px-3 py-1.5">
      <span class="size-1.5 flex-none {done ? 'bg-done' : 'bg-ember'}" aria-hidden="true"></span>
      <span class="min-w-0 flex-1 truncate text-[13px] font-semibold">{done ? `${addSheet.name} is signed in` : `Signing ${addSheet.name} in`}</span>
      <button
        class="grid size-6 flex-none place-items-center border-0 bg-transparent p-0 text-muted hover:text-ink"
        onclick={closeAddAccount}
        title="Close (the login keeps running in its terminal)"
        aria-label="Close"
      >
        <svg width="10" height="10" viewBox="0 0 12 12" aria-hidden="true"><path d="M2 2l8 8M10 2l-8 8" stroke="currentColor" stroke-width="1.4" /></svg>
      </button>
    </div>
    <div class="px-3 py-2 text-[12px] text-muted">
      {#if done}
        Its credentials are in place: sessions can run on it now.
      {:else if watched}
        <p class="m-0">Finish the login in the terminal (a browser may open). This watches for the credentials.</p>
        <p class="m-0 mt-1 font-mono text-[11px]">{loginTitle(watched)}</p>
        {#if watched.loginHint}
          <button class="mt-2 h-6 border border-rule bg-transparent px-2 text-[12px] text-ink hover:border-rule-strong" onclick={again} title="Run the login again in a new terminal"
            >Run it again</button
          >
        {/if}
      {:else}
        Looking for {addSheet.name}&hellip;
      {/if}
    </div>
  </div>
{/if}
