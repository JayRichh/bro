<script lang="ts">
  // Agent presets: the same agent_presets.json the terminal app's `e`
  // lists. Each is a harness, an optional model and effort, text sent
  // before and after the task, and whether to ask for a task at all.
  import { onMount } from 'svelte';
  import { api, type Preset } from '../api';
  import { app, toast } from '../store.svelte';
  import { ask } from '../ui/ui.svelte';

  let list = $state<Preset[]>([]);
  let editing = $state<{ original: string | null; draft: Preset } | null>(null);
  let error = $state<string | null>(null);
  let saving = $state(false);

  const harnesses = $derived(app.launch.harnesses);
  const choices = $derived(harnesses.find((h) => h.id === editing?.draft.harness)?.choices);
  const efforts = $derived(choices ? (choices.efforts[editing?.draft.model ?? 'default'] ?? choices.efforts[choices.model] ?? []) : []);
  /** Which text boxes a new preset opens on (Sessions tab, Preset text). */
  let side = $state<'prefix' | 'postfix' | 'both'>('prefix');

  async function load() {
    try {
      list = await api.presets();
      error = null;
    } catch (e) {
      error = String(e);
    }
  }

  function blank(): Preset {
    return {
      name: '',
      harness: app.launch.default_harness || harnesses[0]?.id || 'claude',
      model: null,
      effort: null,
      prefix: '',
      postfix: '',
      skipTask: false,
    };
  }

  function edit(p: Preset | null) {
    error = null;
    editing = { original: p?.name ?? null, draft: p ? { ...p } : blank() };
    if (p) side = p.prefix.trim() && p.postfix.trim() ? 'both' : p.postfix.trim() ? 'postfix' : 'prefix';
  }

  async function save() {
    if (!editing) return;
    saving = true;
    try {
      const d = editing.draft;
      list = await api.presetSave(editing.original, {
        ...d,
        prefix: side === 'postfix' ? '' : d.prefix,
        postfix: side === 'prefix' ? '' : d.postfix,
      });
      editing = null;
      error = null;
    } catch (e) {
      error = String(e);
    } finally {
      saving = false;
    }
  }

  async function remove(p: Preset) {
    const r = await ask.confirm({ title: 'Delete preset', message: `Delete the preset "${p.name}"?`, confirm: 'Delete', danger: true });
    if (!r.ok) return;
    try {
      list = await api.presetDelete(p.name);
      if (editing?.original === p.name) editing = null;
    } catch (e) {
      toast(String(e));
    }
  }

  onMount(async () => {
    await load();
    // A new preset's text boxes follow the terminal app's Preset text.
    const s = await api.settingsSchema().catch(() => null);
    const row = s?.tabs.flatMap((t) => t.groups.flatMap((g) => g.rows)).find((r) => r.target.scope === 'global' && r.target.key === 'preset_text');
    const v = String(row?.value ?? 'prefix');
    side = v.includes('&') || v === 'both' ? 'both' : v === 'postfix' ? 'postfix' : 'prefix';
  });

  const field =
    'h-7 border border-rule bg-bg px-2 text-[13px] text-ink outline-none placeholder:text-faint hover:border-rule-strong focus:border-rule-strong disabled:text-faint';
  const label = 'font-mono text-[12px] text-muted';
</script>

<div class="max-w-[920px] px-6 pt-5">
  <div class="flex items-baseline gap-3 border-b border-rule-strong pb-1.5">
    <h2 class="m-0 text-[15px] font-semibold text-ink">Agent presets</h2>
    <span class={label}>shared with the terminal app (e)</span>
    <span class="flex-1"></span>
    <button class="h-7 border border-rule bg-transparent px-2.5 text-[13px] text-ink hover:border-rule-strong" onclick={() => edit(null)}>New preset</button>
  </div>
  <p class="m-0 mt-2 max-w-[68ch] text-[13px] text-muted">
    A preset launches a harness with its model and effort, and wraps the task in its own text. Pick one in the quick prompt's options row.
  </p>

  {#if error && !editing}<p class="m-0 mt-3 text-[13px] text-fail">{error}</p>{/if}

  <ul class="m-0 mt-3 list-none border-t border-rule p-0">
    {#each list as p (p.name)}
      <li class="grid grid-cols-[minmax(0,1fr)_auto] items-center gap-x-6 border-b border-rule py-2.5">
        <div class="min-w-0">
          <div class="flex items-baseline gap-2">
            <span class="text-[14px] font-medium text-ink">{p.name}</span>
            <span class="font-mono text-[12px] text-muted">{p.spec}</span>
            {#if p.skipTask}<span class="border border-rule px-1 font-mono text-[11px] leading-4 text-muted">no task</span>{/if}
            {#if p.usable === false}<span class="font-mono text-[11px] text-fail">harness off</span>{/if}
          </div>
          {#if p.prefix || p.postfix}
            <p class="m-0 mt-0.5 truncate text-[13px] text-muted">{[p.prefix, '\u2026task\u2026', p.postfix].filter(Boolean).join('  ')}</p>
          {/if}
        </div>
        <div class="flex gap-2">
          <button class="h-7 border border-rule bg-transparent px-2.5 text-[13px] text-ink hover:border-rule-strong" onclick={() => edit(p)}>Edit</button>
          <button class="h-7 border border-rule bg-transparent px-2.5 text-[13px] text-fail hover:border-fail" onclick={() => remove(p)}>Delete</button>
        </div>
      </li>
    {:else}
      <li class="border-b border-rule py-3 text-[13px] text-muted">No presets yet.</li>
    {/each}
  </ul>

  {#if editing}
    {@const d = editing.draft}
    <section class="mt-5 border border-rule-strong" aria-label={editing.original ? `Edit ${editing.original}` : 'New preset'}>
      <h3 class="m-0 border-b border-rule px-4 py-2 text-[14px] font-semibold text-ink">{editing.original ? `Edit ${editing.original}` : 'New preset'}</h3>
      <div class="grid grid-cols-[120px_minmax(0,1fr)] items-center gap-x-4 gap-y-2.5 px-4 py-3">
        <span class={label}>name</span>
        <input class="{field} w-64" bind:value={d.name} placeholder="reviewer" spellcheck="false" />
        <span class={label}>harness</span>
        <select class="{field} w-64 font-mono text-[12px]" bind:value={d.harness} onchange={() => ((d.model = null), (d.effort = null))}>
          {#each harnesses as h (h.id)}<option value={h.id}>{h.label}</option>{/each}
        </select>
        <span class={label}>model</span>
        <select class="{field} w-64 font-mono text-[12px]" value={d.model ?? 'default'} onchange={(e) => (d.model = (e.currentTarget as HTMLSelectElement).value)}>
          {#each choices?.models ?? ['default'] as m (m)}<option value={m}>{m === 'default' ? 'follow Settings' : m}</option>{/each}
        </select>
        <span class={label}>effort</span>
        <select
          class="{field} w-64 font-mono text-[12px]"
          disabled={!efforts.length}
          value={d.effort ?? 'default'}
          onchange={(e) => (d.effort = (e.currentTarget as HTMLSelectElement).value)}
        >
          {#if !efforts.length}<option value="default">n/a</option>{/if}
          {#each efforts as x (x)}<option value={x}>{x === 'default' ? 'follow Settings' : x}</option>{/each}
        </select>
        <span class={label}>text</span>
        <div class="flex h-7 w-fit border border-rule" role="radiogroup" aria-label="Text">
          {#each [['prefix', 'Before'], ['postfix', 'After'], ['both', 'Both']] as [id, word], i (id)}
            <button
              role="radio"
              aria-checked={side === id}
              class="border-0 px-2.5 text-[12px] {i ? 'border-l border-rule' : ''} {side === id ? 'bg-ink text-bg' : 'bg-transparent text-muted hover:text-ink'}"
              onclick={() => (side = id as typeof side)}>{word}</button
            >
          {/each}
        </div>
        {#if side !== 'postfix'}
          <span class="{label} self-start pt-1.5">before the task</span>
          <textarea class="{field} h-20 resize-y py-1" bind:value={d.prefix} placeholder="Be strict about tests."></textarea>
        {/if}
        {#if side !== 'prefix'}
          <span class="{label} self-start pt-1.5">after the task</span>
          <textarea class="{field} h-20 resize-y py-1" bind:value={d.postfix} placeholder="Commit and push when done."></textarea>
        {/if}
        <span class={label}>task</span>
        <label class="flex items-center gap-2 text-[13px] text-ink">
          <input type="checkbox" class="size-3.5 accent-[var(--color-ink)]" bind:checked={d.skipTask} />
          Launch without asking for a task (the terminal app's skip task)
        </label>
      </div>
      {#if error}<p class="m-0 px-4 pb-2 text-[13px] text-fail">{error}</p>{/if}
      <div class="flex justify-end gap-2 border-t border-rule px-4 py-2.5">
        <button class="h-7 border border-rule bg-transparent px-3 text-[13px] text-ink hover:border-rule-strong" onclick={() => (editing = null)}>Cancel</button>
        <button class="h-7 border-0 bg-ink px-3 text-[13px] font-medium text-bg disabled:bg-faint" disabled={saving} onclick={save}>Save preset</button>
      </div>
    </section>
  {/if}
</div>
