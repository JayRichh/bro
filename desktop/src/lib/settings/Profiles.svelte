<script lang="ts">
  // Profiles: several accounts per harness, the same `profiles` list
  // `kete profile` edits. The window never signs one in: a profile that
  // needs a login shows the command to run in a terminal.
  import { onMount } from 'svelte';
  import { api, type ProfileForm, type ProfileRow } from '../api';
  import { refreshLaunchables, toast } from '../store.svelte';
  import { ask } from '../ui/ui.svelte';

  let rows = $state<ProfileRow[]>([]);
  let harnesses = $state<string[]>([]);
  let editing = $state<{ isNew: boolean; form: ProfileForm; envKeys: string[]; replaceEnv: boolean } | null>(null);
  let error = $state<string | null>(null);
  let notice = $state<{ text: string; command?: string } | null>(null);
  let saving = $state(false);

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
      ? { isNew: false, form: { name: r.name, harness: r.harness, default: r.default, useDefault: r.useDefault, env: '' }, envKeys: r.envKeys, replaceEnv: false }
      : { isNew: true, form: { name: '', harness: harnesses[0] ?? 'claude', default: false, useDefault: false, env: '' }, envKeys: [], replaceEnv: true };
  }

  async function save() {
    if (!editing) return;
    saving = true;
    try {
      const e = editing;
      const out = e.isNew ? await api.profileAdd(e.form) : await api.profileUpdate(e.form, !e.replaceEnv);
      rows = out.rows;
      notice = out.login
        ? { text: `${e.isNew ? 'Added' : 'Saved'} ${e.form.name}. Sign it in from a terminal:`, command: out.login }
        : { text: `${e.isNew ? 'Added' : 'Saved'} ${e.form.name}${out.home ? `, home ${out.home}` : ', on the CLI\'s default login'}.` };
      editing = null;
      error = null;
      refreshLaunchables();
    } catch (err) {
      error = String(err);
    } finally {
      saving = false;
    }
  }

  async function remove(r: ProfileRow) {
    const ok = await ask.confirm({
      title: 'Remove profile',
      message: `Remove the profile "${r.name}"? Its sessions keep their name but launch on the default login.`,
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

  const field =
    'h-7 border border-rule bg-bg px-2 text-[13px] text-ink outline-none placeholder:text-faint hover:border-rule-strong focus:border-rule-strong disabled:text-faint';
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
    <button class={btn} onclick={() => edit(null)}>Add profile</button>
  </div>
  <p class="m-0 mt-2 max-w-[68ch] text-[13px] text-muted">
    A profile is another account for a harness: its own login in its own home, or the CLI's default login. The quick prompt picks one per launch; the
    flagged default is used when none is picked.
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
    {@const f = editing.form}
    <section class="mt-5 border border-rule-strong" aria-label={editing.isNew ? 'Add profile' : `Edit ${f.name}`}>
      <h3 class="m-0 border-b border-rule px-4 py-2 text-[14px] font-semibold text-ink">{editing.isNew ? 'Add profile' : `Edit ${f.name}`}</h3>
      <div class="grid grid-cols-[120px_minmax(0,1fr)] items-center gap-x-4 gap-y-2.5 px-4 py-3">
        <span class={label}>name</span>
        <div class="flex items-center gap-3">
          <input class="{field} w-64 font-mono text-[12px]" bind:value={f.name} disabled={!editing.isNew} placeholder="work" spellcheck="false" />
          <span class="text-[12px] text-muted">{editing.isNew ? 'letters, digits, - and _' : 'the name names its home, so it stays'}</span>
        </div>
        <span class={label}>harness</span>
        <select class="{field} w-64 font-mono text-[12px]" bind:value={f.harness}>
          {#each harnesses as h (h)}<option value={h}>{h}</option>{/each}
        </select>
        <span class={label}>login</span>
        <div class="flex h-7 w-fit border border-rule" role="radiogroup" aria-label="Login">
          {#each [[false, 'Its own home'], [true, "The CLI's default login"]] as [v, word], i (word)}
            <button
              role="radio"
              aria-checked={f.useDefault === v}
              class="border-0 px-2.5 text-[12px] {i ? 'border-l border-rule' : ''} {f.useDefault === v ? 'bg-ink text-bg' : 'bg-transparent text-muted hover:text-ink'}"
              onclick={() => (f.useDefault = v as boolean)}>{word}</button
            >
          {/each}
        </div>
        <span class={label}>default</span>
        <label class="flex items-center gap-2 text-[13px] text-ink">
          <input type="checkbox" class="size-3.5 accent-[var(--color-ink)]" bind:checked={f.default} />
          Use this profile when a launch on {f.harness} picks none
        </label>
        <span class="{label} self-start pt-1.5">environment</span>
        <div class="flex flex-col gap-1.5">
          {#if !editing.isNew && !editing.replaceEnv}
            <div class="flex items-center gap-3 text-[13px] text-muted">
              {editing.envKeys.length ? `Keeps ${editing.envKeys.join(', ')}` : 'None set.'}
              <button class="border-0 bg-transparent p-0 text-[13px] text-ink underline underline-offset-4" onclick={() => editing && (editing.replaceEnv = true)}
                >{editing.envKeys.length ? 'Replace' : 'Add variables'}</button
              >
            </div>
          {:else}
            <textarea class="{field} h-20 resize-y py-1 font-mono text-[12px]" bind:value={f.env} placeholder="HTTPS_PROXY=http://proxy:8080" spellcheck="false"
            ></textarea>
            <span class="text-[12px] text-muted">KEY=VALUE, one per line. Values are stored in config.json as typed.</span>
          {/if}
        </div>
      </div>
      {#if error}<p class="m-0 px-4 pb-2 text-[13px] text-fail">{error}</p>{/if}
      <div class="flex justify-end gap-2 border-t border-rule px-4 py-2.5">
        <button class={btn} onclick={() => (editing = null)}>Cancel</button>
        <button class="h-7 border-0 bg-ink px-3 text-[13px] font-medium text-bg disabled:bg-faint" disabled={saving} onclick={save}
          >{editing.isNew ? 'Add profile' : 'Save'}</button
        >
      </div>
    </section>
  {/if}
</div>
