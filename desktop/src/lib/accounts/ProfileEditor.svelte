<script lang="ts">
  // A profile's add or edit form: name (fixed once made: it names the
  // home), harness, which login, default, environment. Settings, Profiles
  // and the Accounts view both draw it; the save goes through the same
  // commands `kete profile` uses.
  import { api, type ProfileForm, type ProfileSaved } from '../api';
  import { nameProblem } from './view';

  let {
    isNew,
    initial,
    envKeys = [],
    harnesses,
    taken = [],
    onsaved,
    oncancel,
  }: {
    isNew: boolean;
    initial: ProfileForm;
    envKeys?: string[];
    harnesses: string[];
    /** Every profile name, for the new name's uniqueness check. */
    taken?: string[];
    onsaved: (out: ProfileSaved, form: ProfileForm) => void;
    oncancel: () => void;
  } = $props();

  // The form edits its own copy: the caller's object stays as it was.
  // svelte-ignore state_referenced_locally
  let f = $state<ProfileForm>({ ...initial });
  // svelte-ignore state_referenced_locally
  let replaceEnv = $state(isNew);
  let error = $state<string | null>(null);
  let saving = $state(false);
  const nameError = $derived(isNew && f.name ? nameProblem(f.name, taken) : null);

  async function save() {
    if (isNew) {
      const why = nameProblem(f.name, taken);
      if (why) return void (error = why);
    }
    saving = true;
    try {
      const out = isNew ? await api.profileAdd(f) : await api.profileUpdate(f, !replaceEnv);
      error = null;
      onsaved(out, { ...f });
    } catch (err) {
      error = String(err);
    } finally {
      saving = false;
    }
  }

  function onKey(e: KeyboardEvent) {
    if (e.key === 'Escape') (e.stopPropagation(), oncancel());
  }

  const field =
    'h-7 border border-rule bg-bg px-2 text-[13px] text-ink outline-none placeholder:text-faint hover:border-rule-strong focus:border-rule-strong disabled:text-faint';
  const label = 'font-mono text-[12px] text-muted';
  const btn = 'h-7 border border-rule bg-transparent px-2.5 text-[13px] text-ink hover:border-rule-strong disabled:text-faint';
</script>

<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
<section class="border border-rule-strong bg-bg" aria-label={isNew ? 'Add profile' : `Edit ${f.name}`} onkeydown={onKey}>
  <h3 class="m-0 border-b border-rule px-4 py-2 text-[14px] font-semibold text-ink">{isNew ? 'Add profile' : `Edit ${f.name}`}</h3>
  <div class="grid grid-cols-[120px_minmax(0,1fr)] items-center gap-x-4 gap-y-2.5 px-4 py-3 max-sm:grid-cols-1">
    <span class={label}>name</span>
    <div class="flex flex-wrap items-center gap-x-3 gap-y-1">
      <input class="{field} w-64 max-w-full font-mono text-[12px]" bind:value={f.name} disabled={!isNew} placeholder="work" spellcheck="false" aria-label="Name" />
      <span class="text-[12px] {nameError ? 'text-fail' : 'text-muted'}"
        >{nameError ?? (isNew ? 'letters, digits, - and _' : 'the name names its home, so it stays')}</span
      >
    </div>
    <span class={label}>harness</span>
    <select class="{field} w-64 max-w-full font-mono text-[12px]" bind:value={f.harness} aria-label="Harness">
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
      {#if !isNew && !replaceEnv}
        <div class="flex items-center gap-3 text-[13px] text-muted">
          {envKeys.length ? `Keeps ${envKeys.join(', ')}` : 'None set.'}
          <button class="border-0 bg-transparent p-0 text-[13px] text-ink underline underline-offset-4" onclick={() => (replaceEnv = true)}
            >{envKeys.length ? 'Replace' : 'Add variables'}</button
          >
        </div>
      {:else}
        <textarea class="{field} h-20 resize-y py-1 font-mono text-[12px]" bind:value={f.env} placeholder="HTTPS_PROXY=http://proxy:8080" spellcheck="false" aria-label="Environment"
        ></textarea>
        <span class="text-[12px] text-muted">KEY=VALUE, one per line. Values are stored in config.json as typed.</span>
      {/if}
    </div>
  </div>
  {#if error}<p class="m-0 px-4 pb-2 text-[13px] text-fail" role="alert">{error}</p>{/if}
  <div class="flex justify-end gap-2 border-t border-rule px-4 py-2.5">
    <button class={btn} onclick={oncancel} title="Cancel (Esc)">Cancel</button>
    <button class="h-7 border-0 bg-ink px-3 text-[13px] font-medium text-bg disabled:bg-faint" disabled={saving || !!nameError} onclick={save}
      >{isNew ? 'Add profile' : 'Save'}</button
    >
  </div>
</section>
