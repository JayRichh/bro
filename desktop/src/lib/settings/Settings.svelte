<script lang="ts">
  // Settings: the same config.json the terminal app edits. Tabs and rows
  // come from the Rust side's schema export, so a setting added to the
  // terminal app shows up here without a second copy; each change goes
  // back as one write that touches only its own key.
  import { onMount, tick } from 'svelte';
  import {
    api,
    type HotkeyRow,
    type SettingGroup,
    type SettingRow,
    type SettingsSchema,
    type SettingsTab,
  } from '../api';
  import { app, refreshLaunchables, refreshRunCommands, toast } from '../store.svelte';
  import { setShowTips } from '../learn.svelte';
  import { isDiffMode } from '../diff-api';
  import { nav } from '../shell/history.svelte';
  import { chordLabel, hotkeyGroups, matches, segmented, sequence, targetKey } from './settings';
  import DesktopKeys from './DesktopKeys.svelte';
  import Presets from './Presets.svelte';
  import Profiles from './Profiles.svelte';
  import Appearance from './Appearance.svelte';
  import { exportSettings, importSettings } from './io';
  import { adopt, pickFont } from './appearance';
  import { knownTheme } from '../look';
  import { look, setLookWord } from '../look.svelte';

  /** Tabs the window draws itself, after the schema's: their data is not
   *  settings rows but lists (agent_presets.json, config.json profiles). */
  const LOCAL_TABS = [
    { id: 'presets', title: 'Presets' },
    { id: 'profiles', title: 'Accounts' },
  ];

  let schema = $state<SettingsSchema | null>(null);
  let error = $state<string | null>(null);
  let filter = $state('');
  let confirmReset = $state(false);
  let resetting = $state(false);
  /** Rows with a write in flight, by target key. */
  let pending = $state<Record<string, boolean>>({});

  const tabs = $derived([...(schema?.tabs ?? []), ...LOCAL_TABS.map((t) => ({ ...t, note: null, groups: [], hotkeys: [] }) as SettingsTab)]);
  const local = $derived(LOCAL_TABS.some((t) => t.id === tabId));
  const tabId = $derived(nav.current.item && tabs.some((t) => t.id === nav.current.item) ? nav.current.item : (tabs[0]?.id ?? 'desktop'));
  const tab = $derived<SettingsTab | undefined>(tabs.find((t) => t.id === tabId));

  function shownGroups(t: SettingsTab | undefined): SettingGroup[] {
    if (!t) return [];
    return t.groups
      .map((g) => ({ ...g, rows: g.rows.filter((r) => matches(filter, r.label, r.hint, g.title)) }))
      .filter((g) => g.rows.length);
  }
  const groups = $derived(shownGroups(tab));
  const hotkeys = $derived(
    hotkeyGroups((tab?.hotkeys ?? []).filter((h) => matches(filter, h.label, h.hint, h.group, ...h.bound))),
  );

  const loads = sequence();
  /** The settings files can't be parsed: the rows would show defaults. */
  const blocked = $derived(schema?.problem ?? null);

  async function load() {
    const ticket = loads.next();
    try {
      const next = await api.settingsSchema();
      if (!loads.isLatest(ticket)) return;
      schema = next;
      error = null;
    } catch (e) {
      if (loads.isLatest(ticket)) error = String(e);
    }
  }

  function pickTab(id: string) {
    nav.replace({ view: 'settings', item: id });
    filter = '';
  }

  function tabKey(e: KeyboardEvent) {
    if (e.key !== 'ArrowDown' && e.key !== 'ArrowUp') return;
    e.preventDefault();
    const i = tabs.findIndex((t) => t.id === tabId);
    const next = tabs[(i + (e.key === 'ArrowDown' ? 1 : tabs.length - 1)) % tabs.length];
    if (next) {
      pickTab(next.id);
      tick().then(() => document.getElementById(`settings-tab-${next.id}`)?.focus());
    }
  }

  /** What else follows a write: the window's own prefs apply at once, and
   *  the quick prompt and Run buttons ask again. */
  function after(row: SettingRow, value: boolean | string) {
    const t = row.target;
    if (t.scope === 'desktop' && typeof value === 'string') {
      if (t.key === 'theme' && knownTheme(value, look.prefs.themes)) app.themePref = value;
      setLookWord(t.key, value);
      if (t.key === 'home' && (value === 'overview' || value === 'board')) app.home = value;
      if (t.key === 'diff_mode' && isDiffMode(value)) app.diffMode = value;
    }
    if (t.scope === 'agent' || (t.scope === 'global' && (t.key === 'quick_prompt_kind' || t.key === 'hide_uninstalled_harnesses' || t.key === 'show_all_worktrees')))
      refreshLaunchables();
    if (t.scope === 'project' && t.field === 'run_command') refreshRunCommands();
    if (t.scope === 'global' && t.key === 'show_tips' && typeof value === 'boolean') setShowTips(value);
  }

  async function write(row: SettingRow, value: boolean | string, el?: HTMLInputElement) {
    if (value === row.value) return;
    const key = targetKey(row.target);
    const before = row.value;
    row.value = value;
    pending[key] = true;
    try {
      await api.settingsSet(row.target, value);
      after(row, value);
    } catch (e) {
      row.value = before;
      // A refused text edit must not stay in the box looking saved.
      if (el) el.value = String(before);
      toast(String(e));
    } finally {
      delete pending[key];
      // A write can move other rows (a model change refits its effort).
      await load();
    }
  }

  function commitText(row: SettingRow, el: HTMLInputElement) {
    if (el.value.trim() !== String(row.value).trim()) write(row, el.value, el);
  }

  async function reset() {
    resetting = true;
    try {
      await api.settingsReset();
      const prefs = await api.desktopPrefs().catch(() => null);
      if (prefs) {
        adopt(prefs);
        if (prefs.home === 'overview' || prefs.home === 'board') app.home = prefs.home;
        if (isDiffMode(prefs.diff_mode)) app.diffMode = prefs.diff_mode;
      }
      refreshLaunchables();
      refreshRunCommands();
      toast('Settings are back to their defaults.');
    } catch (e) {
      toast(`Could not reset: ${e}`);
    } finally {
      resetting = false;
      confirmReset = false;
      await load();
    }
  }

  function onKey(e: KeyboardEvent) {
    if (nav.current.view !== 'settings') return;
    if (confirmReset && e.key === 'Escape') {
      e.preventDefault();
      e.stopPropagation();
      confirmReset = false;
    }
  }

  onMount(load);

  const seg = 'border-0 px-2.5 text-[12px] whitespace-nowrap disabled:text-faint';
  const label = 'font-mono text-[12px] text-muted';
</script>

<svelte:window onkeydowncapture={onKey} />

{#snippet control(row: SettingRow)}
  {@const busy = !!pending[targetKey(row.target)]}
  {#if row.control.type === 'toggle'}
    <div class="flex h-7 flex-none border border-rule" role="radiogroup" aria-label={row.label}>
      {#each [false, true] as on, i (i)}
        {@const sel = row.value === on}
        <button
          role="radio"
          aria-checked={sel}
          disabled={busy}
          class="{seg} {i ? 'border-l border-rule' : ''} {sel ? 'bg-ink text-bg' : 'bg-transparent text-muted enabled:hover:text-ink'}"
          onclick={() => write(row, on)}>{on ? 'On' : 'Off'}</button
        >
      {/each}
    </div>
  {:else if row.control.type === 'choice' && !row.control.options.length}
    <span class="{label} flex h-7 items-center">n/a</span>
  {:else if row.control.type === 'choice' && segmented(row)}
    {@const c = row.control}
    <div class="flex h-7 flex-none border border-rule" role="radiogroup" aria-label={row.label}>
      {#each c.options as opt, i (opt)}
        {@const sel = row.value === opt}
        <button
          role="radio"
          aria-checked={sel}
          disabled={busy}
          class="{seg} {i ? 'border-l border-rule' : ''} {sel ? 'bg-ink text-bg' : 'bg-transparent text-muted enabled:hover:text-ink'}"
          onclick={() => write(row, opt)}>{c.labels[i] ?? opt}</button
        >
      {/each}
    </div>
  {:else if row.control.type === 'choice'}
    {@const c = row.control}
    <select
      class="h-7 max-w-[240px] min-w-[140px] border border-rule bg-bg px-2 font-mono text-[12px] text-ink outline-none hover:border-rule-strong focus-visible:border-rule-strong disabled:text-faint"
      aria-label={row.label}
      disabled={busy}
      value={String(row.value)}
      onchange={(e) => write(row, (e.currentTarget as HTMLSelectElement).value)}
    >
      {#each c.options as opt, i (opt)}<option value={opt}>{c.labels[i] ?? opt}</option>{/each}
    </select>
  {:else if row.control.type === 'font'}
    {@const c = row.control}
    {@const family = String(row.value)}
    <button
      class="h-7 max-w-[240px] min-w-[160px] truncate border border-rule bg-bg px-2 text-left text-[13px] text-ink enabled:hover:border-rule-strong disabled:text-faint"
      style:font-family={`"${family || c.placeholder}"`}
      aria-label="{row.label}: {family || `${c.placeholder}, Kete's own`}. Pick another"
      title="Pick any installed font family, or Kete's own ({c.placeholder})"
      disabled={busy}
      onclick={async () => {
        const picked = await pickFont(row);
        if (picked !== null) write(row, picked);
      }}>{family || `${c.placeholder} (Kete's own)`}</button
    >
  {:else if row.control.type === 'text'}
    <input
      class="h-7 w-[260px] border border-rule bg-bg px-2 font-mono text-[12px] text-ink outline-none placeholder:text-faint hover:border-rule-strong focus:border-rule-strong"
      aria-label={row.label}
      placeholder={row.control.placeholder}
      value={String(row.value)}
      spellcheck="false"
      onkeydown={(e) => {
        const el = e.currentTarget as HTMLInputElement;
        if (e.key === 'Enter') (e.preventDefault(), commitText(row, el));
        else if (e.key === 'Escape') (e.stopPropagation(), (el.value = String(row.value)), el.blur());
      }}
      onblur={(e) => commitText(row, e.currentTarget as HTMLInputElement)}
    />
  {/if}
{/snippet}

{#snippet keys(list: string[])}
  {#if list.length}
    {#each list as k, i (i)}<kbd class="border border-rule px-1.5 font-mono text-[11px] leading-5 text-ink">{chordLabel(k)}</kbd>{/each}
  {:else}
    <span class={label}>unbound</span>
  {/if}
{/snippet}

<section class="flex min-h-0 min-w-0 flex-1 flex-col" aria-labelledby="settings-title">
  <header class="flex-none border-b border-rule px-6 pt-6 pb-4">
    <div class="flex flex-wrap items-baseline gap-x-4 gap-y-2">
      <h1 id="settings-title" class="m-0 text-[28px] leading-tight font-semibold tracking-tight text-ink">Settings</h1>
      <span class="{label} whitespace-nowrap">shared with the terminal app</span>
      <span class="flex-1"></span>
      <label class="flex h-7 w-64 min-w-0 shrink items-center border border-rule focus-within:border-rule-strong {local ? 'invisible' : ''}">
        <span class="sr-only">Filter settings</span>
        <input
          class="h-full min-w-0 flex-1 border-0 bg-transparent px-2.5 font-mono text-[12px] text-ink outline-none placeholder:text-faint"
          placeholder="Filter this tab"
          bind:value={filter}
          spellcheck="false"
          onkeydown={(e) => e.key === 'Escape' && filter && (e.stopPropagation(), (filter = ''))}
        />
      </label>
      <button
        class="h-7 flex-none border border-rule bg-transparent px-2.5 text-[13px] whitespace-nowrap text-ink enabled:hover:border-rule-strong disabled:text-faint"
        title="Write config.json, the agent presets and the ssh hosts to one file, as kete config export does"
        disabled={!!blocked}
        onclick={exportSettings}>Export&hellip;</button
      >
      <button
        class="h-7 flex-none border border-rule bg-transparent px-2.5 text-[13px] whitespace-nowrap text-ink enabled:hover:border-rule-strong disabled:text-faint"
        title="Merge a settings file in, as kete config import does: what it changes is shown first"
        disabled={!!blocked}
        onclick={async () => {
          if (!(await importSettings())) return;
          const prefs = await api.desktopPrefs().catch(() => null);
          if (prefs) adopt(prefs);
          await load();
        }}>Import&hellip;</button
      >
      <button
        class="h-7 flex-none border border-rule bg-transparent px-2.5 text-[13px] whitespace-nowrap text-ink enabled:hover:border-rule-strong disabled:text-faint"
        disabled={!schema || resetting || !!blocked}
        onclick={() => (confirmReset = true)}>Reset to defaults</button
      >
    </div>
    {#if blocked}
      <div class="mt-4 flex items-start gap-3 border border-fail bg-surface px-3 py-2 text-[13px] text-ink" role="alert">
        <span class="mt-1.5 size-1.5 flex-none bg-fail" aria-hidden="true"></span>
        <span class="min-w-0 flex-1">
          <span class="font-medium text-fail">The settings file can't be parsed.</span>
          Nothing here is saved until it is fixed, and the rows are hidden because they would show defaults, not your settings.
          <span class="mt-1 block font-mono text-[12px] break-all text-muted">{blocked}</span>
        </span>
        <button class="h-7 flex-none border border-rule bg-transparent px-2.5 text-[13px] text-ink hover:border-rule-strong" onclick={load}>Check again</button>
      </div>
    {/if}
    {#if confirmReset}
      <div class="mt-4 flex items-center gap-3 border border-rule-strong bg-surface px-3 py-2 text-[13px] text-ink" role="alertdialog" aria-label="Reset settings">
        <span class="min-w-0 flex-1">
          Reset every setting on these tabs, each harness's model and effort, and this window's preferences?
          <span class="text-muted">Project commands, profiles, harness definitions and hotkeys stay.</span>
        </span>
        <!-- svelte-ignore a11y_autofocus -->
        <button class="h-7 flex-none border-0 bg-ink px-3 text-[13px] font-medium text-bg disabled:bg-faint" autofocus disabled={resetting} onclick={reset}
          >Reset</button
        >
        <button class="h-7 flex-none border border-rule bg-transparent px-3 text-[13px] text-ink hover:border-rule-strong" onclick={() => (confirmReset = false)}
          >Cancel</button
        >
      </div>
    {/if}
  </header>

  {#if error && !schema}
    <div class="px-6 py-6">
      <p class="m-0 flex items-center gap-2 text-[14px] text-ink">
        <span class="size-1.5 flex-none bg-fail" aria-hidden="true"></span><span class="font-medium text-fail">Failed</span>
        <span class="text-muted">Could not read the settings. {error}</span>
      </p>
      <button class="mt-3 h-7 border border-rule-strong bg-transparent px-2.5 text-[13px] text-ink hover:bg-surface" onclick={load}>Try again</button>
    </div>
  {:else if !schema}
    <p class="m-0 px-6 py-6 font-mono text-[12px] text-muted">Reading settings</p>
  {:else}
    <div class="flex min-h-0 flex-1">
      <nav class="w-[184px] flex-none overflow-y-auto border-r border-rule py-3" aria-label="Settings tabs">
        <ul class="m-0 list-none p-0" role="tablist" aria-orientation="vertical">
          {#each tabs as t (t.id)}
            {@const on = t.id === tabId}
            <li role="presentation">
              <button
                id="settings-tab-{t.id}"
                role="tab"
                aria-selected={on}
                tabindex={on ? 0 : -1}
                class="relative flex w-full items-center border-0 px-6 py-1.5 text-left text-[14px] focus-visible:-outline-offset-2 {on
                  ? 'bg-surface font-semibold text-ink'
                  : 'bg-transparent text-muted hover:text-ink'}"
                onclick={() => pickTab(t.id)}
                onkeydown={tabKey}
              >
                {#if on}<span class="absolute inset-y-0 left-0 w-[3px] bg-ink" aria-hidden="true"></span>{/if}
                {t.title}
              </button>
            </li>
          {/each}
        </ul>
      </nav>

      <div class="min-h-0 min-w-0 flex-1 overflow-y-auto pb-10" role="tabpanel" aria-labelledby="settings-tab-{tabId}">
        {#if tabId === 'presets'}
          <Presets />
        {:else if tabId === 'profiles'}
          <Profiles />
        {:else if blocked}
          <p class="m-0 px-6 pt-6 text-[13px] text-muted">Fix the file named above (or remove it to start from defaults), then Check again.</p>
        {:else}
        <div class="max-w-[920px]">
          {#if tab?.note}
            <p class="m-0 px-6 pt-5 text-[13px] text-muted">{tab.note}</p>
          {/if}

          {#each groups as g, gi (g.title + gi)}
            <section class="pt-5" aria-label={g.title || tab?.title}>
              {#if g.title}
                <h2 class="m-0 flex items-baseline gap-3 border-b border-rule-strong px-6 pb-1.5">
                  <span class="text-[15px] font-semibold text-ink">{g.title}</span>
                  {#if g.terminal}<span class="border border-rule px-1.5 font-mono text-[11px] leading-4 text-muted">terminal</span>{/if}
                  {#if g.subtitle}<span class="min-w-0 truncate font-mono text-[12px] {g.subtitle.startsWith('broken') ? 'text-fail' : 'text-muted'}" title={g.subtitle}
                      >{g.subtitle}</span
                    >{/if}
                </h2>
              {:else}
                <div class="border-b border-rule-strong"></div>
              {/if}
              <ul class="m-0 list-none p-0">
                {#if tabId === 'desktop' && g.rows.some((r) => r.target.scope === 'desktop' && r.target.key === 'theme')}
                  <Appearance onchange={load} />
                {/if}
                {#each g.rows as row (targetKey(row.target))}
                  <li class="grid grid-cols-[minmax(0,1fr)_auto] items-center gap-x-6 border-b border-rule px-6 py-3">
                    <div class="min-w-0">
                      <div class="flex items-baseline gap-2">
                        <span class="text-[14px] font-medium text-ink">{row.label}</span>
                        {#if row.is_new}<span class="border border-rule px-1 font-mono text-[11px] leading-4 text-muted">new</span>{/if}
                      </div>
                      {#if row.hint}<p class="m-0 mt-0.5 max-w-[68ch] text-[13px] leading-snug text-muted">{row.hint}</p>{/if}
                      {#if row.restart}
                        <p class="m-0 mt-1 flex items-center gap-1.5 font-mono text-[11px] text-faint">
                          <span class="size-1.5 flex-none border border-faint" aria-hidden="true"></span>{row.restart}
                        </p>
                      {/if}
                    </div>
                    <div class="flex justify-end">{@render control(row)}</div>
                  </li>
                {/each}
              </ul>
            </section>
          {:else}
            {#if !tab?.hotkeys.length && tabId !== 'hotkeys'}
              <p class="m-0 px-6 pt-6 text-[13px] text-muted">{filter ? 'Nothing on this tab matches.' : 'Nothing to set here.'}</p>
            {/if}
          {/each}

          {#if tabId === 'hotkeys'}
            <h2 class="m-0 mt-6 flex items-baseline gap-3 border-b border-rule-strong px-6 pb-1.5">
              <span class="text-[15px] font-semibold text-ink">This window</span>
              <span class="font-mono text-[12px] text-muted">rebind here</span>
            </h2>
            <DesktopKeys {filter} />
            {#if tab?.hotkeys.length}
              <h2 class="m-0 mt-8 flex items-baseline gap-3 border-b border-rule-strong px-6 pb-1.5">
                <span class="text-[15px] font-semibold text-ink">Terminal app</span>
                <span class="border border-rule px-1.5 font-mono text-[11px] leading-4 text-muted">terminal</span>
                <span class="min-w-0 truncate font-mono text-[12px] text-muted">read-only here: rebind them in its Settings, Hotkeys tab</span>
              </h2>
            {/if}
          {/if}
          {#each hotkeys as hg (hg.group)}
            <section class="pt-5" aria-label="Terminal app: {hg.group}">
              <h2 class="m-0 border-b border-rule-strong px-6 pb-1.5 font-mono text-[12px] font-semibold tracking-wide text-ink">{hg.group}</h2>
              <ul class="m-0 list-none p-0">
                {#each hg.rows as h (h.id)}
                  {@const hk = h as HotkeyRow}
                  <li class="grid grid-cols-[minmax(0,1fr)_auto] items-center gap-x-6 border-b border-rule px-6 py-2" title={hk.hint}>
                    <div class="min-w-0">
                      <span class="text-[14px] text-ink">{hk.label}</span>
                      {#if hk.overridden}<span class="ml-2 font-mono text-[11px] text-muted">changed</span>{/if}
                      <p class="m-0 truncate text-[12px] text-muted">{hk.hint}</p>
                    </div>
                    <div class="flex flex-wrap justify-end gap-1">{@render keys(hk.bound)}</div>
                  </li>
                {/each}
              </ul>
            </section>
          {/each}
          {#if tab?.hotkeys.length && !hotkeys.length}
            <p class="m-0 px-6 pt-6 text-[13px] text-muted">No key matches.</p>
          {/if}
        </div>
        {/if}
      </div>
    </div>
  {/if}
</section>
