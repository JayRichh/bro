<script lang="ts">
  // Profiles: several accounts per harness, the same `profiles` list
  // `kete profile` edits. The window never signs one in: a profile that
  // needs a login shows the command to run in a terminal.
  import { onMount } from 'svelte';
  import { api, type ProfileForm, type ProfileRow, type ProfileSaved } from '../api';
  import { app, refreshLaunchables, toast } from '../store.svelte';
  import { removeWarning } from '../accounts/pick';
  import { ask } from '../ui/ui.svelte';
  import ProfileEditor from '../accounts/ProfileEditor.svelte';
  import { openAccounts } from '../accounts/actions';

  let rows = $state<ProfileRow[]>([]);
  let harnesses = $state<string[]>([]);
  let editing = $state<{ isNew: boolean; form: ProfileForm; envKeys: string[] } | null>(null);
  let error = $state<string | null>(null);
  let notice = $state<{ text: string; command?: string } | null>(null);

  async function load() {
    try {
      [rows, harnesses] = await Promise.all([api.profiles(), api.profileHarnesses()]);
      error = null;
    } catch (e) {
      error = String(e);
    }
  }

  function edit(r: ProfileRow | null) {
    error = null;
    editing = r
      ? { isNew: false, form: { name: r.name, harness: r.harness, default: r.default, useDefault: r.useDefault, env: '' }, envKeys: r.envKeys }
      : { isNew: true, form: { name: '', harness: harnesses[0] ?? 'claude', default: false, useDefault: false, env: '' }, envKeys: [] };
  }

  function saved(out: ProfileSaved, form: ProfileForm) {
    const isNew = editing?.isNew ?? false;
    rows = out.rows;
    notice = out.login
      ? { text: `${isNew ? 'Added' : 'Saved'} ${form.name}. Sign it in from a terminal:`, command: out.login }
      : { text: `${isNew ? 'Added' : 'Saved'} ${form.name}${out.home ? `, home ${out.home}` : ", on the CLI's default login"}.` };
    editing = null;
    error = null;
    refreshLaunchables();
  }

  async function remove(r: ProfileRow) {
    const ok = await ask.confirm({
      title: 'Remove profile',
      message: removeWarning(r.name, app.model.agents),
      detail: r.home ? [`Its home stays at ${r.home} (kete profile remove --purge ${r.name} deletes it)`] : undefined,
      confirm: 'Remove',
      danger: true,
    });
    if (!ok.ok) return;
    try {
      const out = await api.profileRemove(r.name);
      rows = out.rows;
      notice = { text: out.home ? `Removed ${r.name}. Its home stays at ${out.home}.` : `Removed ${r.name}.` };
      refreshLaunchables();
    } catch (e) {
      toast(String(e));
    }
  }

  async function makeDefault(r: ProfileRow) {
    try {
      rows = await api.profileSetDefault(r.name);
      refreshLaunchables();
    } catch (e) {
      toast(String(e));
    }
  }

  function copy(text: string) {
    navigator.clipboard?.writeText(text).then(
      () => toast('Copied.'),
      () => toast('Could not copy.'),
    );
  }

  onMount(load);

  const label = 'font-mono text-[12px] text-muted';
  const btn = 'h-7 border border-rule bg-transparent px-2.5 text-[13px] text-ink hover:border-rule-strong disabled:text-faint';
</script>

{#snippet command(cmd: string)}
  <span class="inline-flex items-center border border-rule-strong">
    <code class="px-2 py-0.5 font-mono text-[12px] text-ink select-all">{cmd}</code>
    <button class="border-0 border-l border-rule-strong bg-transparent px-2 py-0.5 text-[12px] text-muted hover:text-ink" onclick={() => copy(cmd)}>Copy</button>
  </span>
{/snippet}

<div class="max-w-[920px] px-6 pt-5">
  <div class="flex items-baseline gap-3 border-b border-rule-strong pb-1.5">
    <h2 class="m-0 text-[15px] font-semibold text-ink">Profiles</h2>
    <span class={label}>the same list as kete profile</span>
    <span class="flex-1"></span>
    <button class={btn} onclick={openAccounts} title="Every account with its use, login and sessions (Shift+U)">Accounts view</button>
    <button class={btn} onclick={() => edit(null)}>Add profile</button>
  </div>
  <p class="m-0 mt-2 max-w-[68ch] text-[13px] text-muted">
    A profile is another account for a harness: its own login in its own home, or the CLI's default login. The quick prompt picks one per launch; on
    auto, the flagged default is used, or the account spread chooses (Settings, Sessions).
  </p>

  {#if notice}
    <div class="mt-3 flex flex-wrap items-center gap-3 border border-rule-strong bg-surface px-3 py-2 text-[13px] text-ink" role="status">
      <span>{notice.text}</span>
      {#if notice.command}{@render command(notice.command)}{/if}
      <button class="ml-auto border-0 bg-transparent p-0 text-[12px] text-muted hover:text-ink" onclick={() => (notice = null)}>Dismiss</button>
    </div>
  {/if}
  {#if error && !editing}<p class="m-0 mt-3 text-[13px] text-fail">{error}</p>{/if}

  <ul class="m-0 mt-3 list-none border-t border-rule p-0">
    {#each rows as r (r.name)}
      <li class="grid grid-cols-[minmax(0,1fr)_auto] items-center gap-x-6 border-b border-rule py-2.5">
        <div class="min-w-0">
          <div class="flex items-baseline gap-2">
            <span class="text-[14px] font-medium text-ink">{r.name}</span>
            <span class="font-mono text-[12px] text-muted">{r.harness}</span>
            {#if r.default}<span class="border border-ink px-1 font-mono text-[11px] leading-4 text-ink">default</span>{/if}
            {#if r.usage}<span class="font-mono text-[11px] text-muted">{r.usage}</span>{/if}
          </div>
          <p class="m-0 mt-0.5 truncate font-mono text-[12px] text-muted" title={r.home ?? ''}>
            {r.useDefault ? "the CLI's default login" : r.home}{r.envKeys.length ? ` \u00b7 env ${r.envKeys.join(', ')}` : ''}
          </p>
          {#if r.problem}<p class="m-0 mt-0.5 text-[12px] text-fail">{r.problem}</p>{/if}
          {#if r.login}
            <p class="m-0 mt-1 flex items-center gap-2 text-[12px] text-muted">Sign in: {@render command(r.login)}</p>
          {/if}
        </div>
        <div class="flex gap-2">
          {#if !r.default}<button class={btn} onclick={() => makeDefault(r)}>Make default</button>{/if}
          <button class={btn} onclick={() => edit(r)}>Edit</button>
          <button class="{btn} text-fail hover:border-fail" onclick={() => remove(r)}>Remove</button>
        </div>
      </li>
    {:else}
      <li class="border-b border-rule py-3 text-[13px] text-muted">No profiles. Sessions run on each CLI's default login.</li>
    {/each}
  </ul>

  {#if editing}
    {#key editing}
      <div class="mt-5">
        <ProfileEditor
          isNew={editing.isNew}
          initial={editing.form}
          envKeys={editing.envKeys}
          {harnesses}
          taken={rows.map((r) => r.name)}
          onsaved={saved}
          oncancel={() => (editing = null)}
        />
      </div>
    {/key}
  {/if}
</div>
