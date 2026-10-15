<script lang="ts">
  // ACCOUNTS: every account kete can run a session on, in one place. Each
  // profile (switched off and broken ones too) and each harness's own login
  // in use, with its login state, plan, 5-hour and weekly use with their
  // resets, the default star, the enabled switch, the sessions on it and
  // the projects that start on it; and every verb: sign in again, edit,
  // duplicate, remove, make default, make it a project's account. The
  // account spread sits on top, said in words. Shift+U opens it, as the
  // terminal app's accounts overlay.
  import { onMount } from 'svelte';
  import { api, type AccountRow, type ProfileForm, type UsageWindow } from '../api';
  import { app, openSession, refreshAccounts, refreshLaunchables, toast } from '../store.svelte';
  import { ask, openMenu, type MenuEntry } from '../ui/ui.svelte';
  import UsageBar from './UsageBar.svelte';
  import ProfileEditor from './ProfileEditor.svelte';
  import {
    bars,
    liveUsage,
    loginTitle,
    mergeRows,
    ownLoginSessionHarnesses,
    resetsIn,
    sessionsOn,
    shownAt,
    staleAt,
    starTitle,
    usageTitle,
    windowLabel,
  } from './usage';
  import { removeWarning } from './pick';
  import { nameProblem, projectAccount, projectsDefaultingTo, renameBlocker, SPREADS, suggestName, THRESHOLDS, viewRows } from './view';
  import { runLogin, setProjectAccount } from './actions';
  import { openAddAccount } from './add.svelte';

  const rows = $derived(viewRows(mergeRows(app.accounts, app.model.accounts ?? {}, ownLoginSessionHarnesses(app.model.agents))));
  const spread = $derived(app.launch.spread ?? 'manual');
  const threshold = $derived(app.launch.spread_threshold ?? '90');
  const spreadInfo = $derived(SPREADS.find((s) => s.id === spread) ?? SPREADS[0]);
  let harnesses = $state<string[]>([]);
  let editing = $state<{ key: string; form: ProfileForm; envKeys: string[] } | null>(null);
  let sessionsOpen = $state<string | null>(null);

  const GLYPH: Record<string, string> = { claude: 'C', codex: 'X', copilot: 'G', cursor: 'U', pi: 'P', opencode: 'O' };

  onMount(() => {
    refreshLaunchables();
    api
      .profileHarnesses()
      .then((h) => (harnesses = h))
      .catch(() => {});
    // Usage and logins move: read the list again while the view is up.
    const t = setInterval(() => {
      if (document.visibilityState === 'visible' && app.model.connected) refreshAccounts();
    }, 10_000);
    return () => clearInterval(t);
  });

  async function setSpread(id: string) {
    if (id === spread) return;
    try {
      await api.settingsSet({ scope: 'global', key: 'account_spread' }, id);
      refreshLaunchables();
    } catch (e) {
      toast(`Could not save the spread: ${e}`);
    }
  }

  async function setThreshold(v: string) {
    try {
      await api.settingsSet({ scope: 'global', key: 'account_spread_threshold' }, v);
      refreshLaunchables();
    } catch (e) {
      toast(`Could not save the threshold: ${e}`);
    }
  }

  async function setDefault(r: AccountRow) {
    if (r.default) return;
    try {
      await api.profileSetDefault(r.name);
      refreshLaunchables();
    } catch (e) {
      toast(String(e));
    }
  }

  async function setEnabled(r: AccountRow) {
    try {
      await api.profileSetEnabled(r.name, r.disabled);
      toast(r.disabled ? `${r.name} is on.` : `${r.name} is off: launches that pick no account skip it.`);
      refreshLaunchables();
    } catch (e) {
      toast(String(e));
    }
  }

  function edit(r: AccountRow) {
    if (editing?.key === r.key) return void (editing = null);
    editing = {
      key: r.key,
      form: { name: r.name, harness: r.harness, default: !!r.flaggedDefault || r.default, useDefault: r.useDefault, env: '' },
      envKeys: r.envKeys ?? [],
    };
  }

  async function duplicate(r: AccountRow) {
    const taken = app.accounts.filter((x) => x.profile).map((x) => x.name);
    const v = await ask.prompt({
      title: `Duplicate ${r.name}`,
      note: `A new ${r.harness} account with the same variables and kind of login, in a home of its own: it signs in separately.`,
      fields: [{ key: 'name', label: 'Name', value: suggestName(r.name, taken), mono: true }],
      confirm: 'Duplicate',
    });
    if (!v) return;
    const why = nameProblem(v.name, taken);
    if (why) return toast(why);
    try {
      const out = await api.profileDuplicate(r.name, v.name.trim());
      refreshLaunchables();
      if (out.login) {
        const go = await ask.confirm({
          title: `Sign ${v.name.trim()} in`,
          message: `${v.name.trim()} is added. Sign it in now? The login runs in a new terminal.`,
          detail: [out.login],
          confirm: 'Sign in',
        });
        if (go.ok) {
          const rows = await api.accounts();
          app.accounts = rows;
          const row = rows.find((x) => x.profile && x.name === v.name.trim());
          if (row) await runLogin(row);
        }
      } else toast(`Added ${v.name.trim()}.`);
    } catch (e) {
      toast(String(e));
    }
  }

  async function remove(r: AccountRow) {
    const projects = projectsDefaultingTo(app.launch, app.model.projects, r.harness, r.name);
    const detail = [
      ...(r.home ? [`Its home is ${r.home}: it stays unless you tick the box below.`] : []),
      ...(projects.length ? [`Default account of ${projects.map((p) => p.name).join(', ')}: those fall back to the spread.`] : []),
    ];
    const ok = await ask.confirm({
      title: 'Remove account',
      message: removeWarning(r.name, app.model.agents),
      detail: detail.length ? detail : undefined,
      confirm: 'Remove',
      danger: true,
      // The purge `kete profile remove --purge` does: the login goes too.
      check: r.home ? `Also delete ${r.home} and the login in it` : undefined,
    });
    if (!ok.ok) return;
    try {
      const out = await api.profileRemove(r.name, !!r.home && ok.checked);
      toast(
        out.purged ? `Removed ${r.name} and deleted ${out.home}.` : out.home ? `Removed ${r.name}. Its home stays at ${out.home}.` : `Removed ${r.name}.`,
      );
      if (editing?.key === r.key) editing = null;
      refreshLaunchables();
    } catch (e) {
      toast(String(e));
    }
  }

  const RENAME_OFFLINE = 'Not connected to the kete daemon: a rename checks the sessions on the account first.';

  /** Rename: refused while a session names it; the home, the repos it is
   *  the default of and its reading move with the name. */
  async function rename(r: AccountRow) {
    const blocked = app.model.connected ? renameBlocker(r.name, app.model.agents) : RENAME_OFFLINE;
    if (blocked) return toast(blocked);
    const taken = app.accounts.filter((x) => x.profile && x.key !== r.key).map((x) => x.name);
    const v = await ask.prompt({
      title: `Rename ${r.name}`,
      note: r.home
        ? `Its home moves from ${r.home} to the new name; repos that start on it follow.`
        : `Repos that start on it follow the new name.`,
      fields: [{ key: 'name', label: 'New name', value: r.name, mono: true }],
      confirm: 'Rename',
    });
    if (!v || v.name.trim() === r.name) return;
    const why = nameProblem(v.name, taken);
    if (why) return toast(why);
    try {
      const out = await api.profileRename(r.name, v.name.trim());
      const repos = out.projects.length ? `, the default of ${out.projects.length} ${out.projects.length === 1 ? 'repo' : 'repos'}` : '';
      const archived = out.archived.length ? ` ${out.archived.length} archived ${out.archived.length === 1 ? 'session still names' : 'sessions still name'} ${r.name}.` : '';
      toast(`Renamed ${r.name} to ${out.name}${repos}.${archived}`);
      if (editing?.key === r.key) editing = null;
      refreshLaunchables();
      refreshAccounts();
    } catch (e) {
      toast(String(e));
    }
  }

  /** A row's keys while focus is in it: R renames, Delete removes. */
  function rowKey(e: KeyboardEvent, r: AccountRow) {
    if (!r.profile || e.ctrlKey || e.metaKey || e.altKey) return;
    const t = e.target as HTMLElement;
    if (t.closest('input, textarea, select, [contenteditable="true"]')) return;
    // The window's own r and Delete act on the open session: not here.
    if (e.key === 'r' || e.key === 'R') (e.preventDefault(), e.stopPropagation(), rename(r));
    else if (e.key === 'Delete') (e.preventDefault(), e.stopPropagation(), remove(r));
  }

  async function signIn(r: AccountRow) {
    await runLogin(r);
  }

  /** "Use for...": each repo, ticked where this account is its default for
   *  the harness; picking a ticked one clears it. */
  function useFor(r: AccountRow, at: HTMLElement) {
    const projects = app.model.projects;
    const items: MenuEntry[] = [{ heading: `Default ${r.harness} account of` }];
    if (!projects.length) items.push({ label: 'No repos yet', run: () => {}, disabled: true });
    for (const p of projects) {
      const now = projectAccount(app.launch, p.repo_path, r.harness);
      const mine = now === r.name;
      items.push({
        label: mine ? `\u2713 ${p.name} (clear)` : now ? `${p.name} (now ${now})` : p.name,
        run: () => setProjectAccount(p.id, r.harness, mine ? null : r.name),
      });
    }
    openMenu(items, at, `Use ${r.name} for`);
  }

  function resetLine(w: UsageWindow | null): string {
    if (!w || !w.resets_at || w.resets_at * 1000 <= app.now) return '';
    return `resets ${resetsIn(w.resets_at, app.now)}`;
  }

  function stateWord(r: AccountRow): { text: string; tone: string } {
    if (r.problem) return { text: 'broken', tone: 'text-fail' };
    if (r.login === 'logged_in') return { text: 'signed in', tone: 'text-done' };
    if (r.login === 'needs_login') return { text: r.expired ? 'login expired' : 'not signed in', tone: 'text-ember-ink' };
    return { text: 'unknown', tone: 'text-faint' };
  }

  const btn = 'h-6 border border-rule bg-transparent px-2 text-[12px] text-ink hover:border-rule-strong disabled:text-faint';
  const head = 'font-mono text-[11px] text-faint';
</script>

<section class="flex min-h-0 min-w-0 flex-1 flex-col overflow-y-auto" aria-label="Accounts">
  <div class="mx-auto w-full max-w-[1180px] px-6 pt-5 pb-10 max-sm:px-3">
    <div class="flex flex-wrap items-baseline gap-x-3 gap-y-2 border-b border-rule-strong pb-2">
      <h1 class="m-0 text-[22px] font-semibold tracking-tight text-ink">Accounts</h1>
      <span class="font-mono text-[12px] text-muted">every login sessions can run on</span>
      <span class="flex-1"></span>
      <button class="h-7 border-0 bg-ink px-3 text-[13px] font-medium text-bg" onclick={() => openAddAccount()} title="Add an account: a harness, a name, then its sign-in"
        >Add account</button
      >
    </div>

    <!-- The account spread: what a launch naming no account runs on. -->
    <div class="mt-4 border border-rule">
      <div class="flex flex-wrap items-center gap-x-4 gap-y-2 border-b border-rule px-3 py-2">
        <span class="text-[13px] font-semibold text-ink">Account spread</span>
        <div class="flex h-7 border border-rule" role="radiogroup" aria-label="Account spread">
          {#each SPREADS as s, i (s.id)}
            {@const on = spread === s.id}
            <button
              role="radio"
              aria-checked={on}
              class="border-0 px-2.5 text-[12px] {i ? 'border-l border-rule' : ''} {on ? 'bg-ink text-bg' : 'bg-transparent text-muted hover:text-ink'}"
              title="{s.label}: {s.text}"
              onclick={() => setSpread(s.id)}>{s.label}</button
            >
          {/each}
        </div>
        {#if spread === 'failover'}
          <label class="flex items-center gap-2 text-[12px] text-muted">
            moves on at
            <select
              class="h-7 border border-rule bg-bg px-1.5 font-mono text-[12px] text-ink outline-none hover:border-rule-strong"
              value={threshold}
              onchange={(e) => setThreshold((e.currentTarget as HTMLSelectElement).value)}
              aria-label="Failover threshold"
            >
              {#each THRESHOLDS as t (t)}<option value={t}>{t}%</option>{/each}
            </select>
          </label>
        {/if}
      </div>
      <p class="m-0 px-3 py-2 text-[13px] text-muted">
        <span class="text-ink">{spreadInfo.label}.</span>
        {spreadInfo.text} A launch that names an account always runs on it; a repo's own default account (Use for&hellip;) comes before the spread.
      </p>
    </div>

    <div class="mt-5 hidden grid-cols-[minmax(200px,1.3fr)_minmax(150px,1fr)_minmax(220px,1.3fr)_minmax(250px,auto)] gap-x-5 border-b border-rule pb-1.5 lg:grid">
      <span class={head}>account</span><span class={head}>runs</span><span class={head}>use</span><span class={head}>actions</span>
    </div>

    <ul class="m-0 list-none p-0 max-lg:mt-4 max-lg:border-t max-lg:border-rule" aria-label="Every account">
      {#each rows as r (r.key)}
        {@const u = liveUsage(r, app.model.accounts ?? {})}
        {@const [top, bottom] = bars(u)}
        {@const on = sessionsOn(r, app.model.agents)}
        {@const st = stateWord(r)}
        {@const projects = r.profile ? projectsDefaultingTo(app.launch, app.model.projects, r.harness, r.name) : []}
        {@const dimStar = !r.default && !!r.flaggedDefault && r.disabled}
        <li class="border-b border-rule py-3">
          <!-- svelte-ignore a11y_no_static_element_interactions -->
          <div
            onkeydown={(e) => rowKey(e, r)}
            class="grid items-start gap-x-5 gap-y-2.5 lg:grid-cols-[minmax(200px,1.3fr)_minmax(150px,1fr)_minmax(220px,1.3fr)_minmax(250px,auto)] {r.disabled &&
            !r.problem
              ? 'opacity-60'
              : ''}"
          >
            <!-- Who: the account, its harness, its login, its plan. -->
            <div class="flex min-w-0 flex-col gap-1">
              <div class="flex min-w-0 items-center gap-2">
                <span
                  class="grid size-[16px] flex-none place-items-center border font-mono text-[10px] leading-none font-semibold {r.problem
                    ? 'border-fail text-fail'
                    : 'border-rule-strong text-ink'}"
                  aria-hidden="true">{r.problem ? '!' : (GLYPH[r.harness] ?? r.harness.slice(0, 1).toUpperCase())}</span
                >
                <span class="min-w-0 truncate text-[14px] font-medium text-ink" title={r.name}>{r.name}</span>
                {#if r.profile && !r.problem}
                  <button
                    class="grid size-[18px] flex-none place-items-center border-0 bg-transparent p-0 {r.default
                      ? 'text-ember'
                      : dimStar
                        ? 'text-ember opacity-45'
                        : 'text-faint hover:text-ink'}"
                    onclick={() => setDefault(r)}
                    title={starTitle(r)}
                    aria-label={r.default ? `${r.name} is the default` : `Make ${r.name} the default`}
                    aria-pressed={r.default}
                  >
                    <svg width="12" height="12" viewBox="0 0 12 12" aria-hidden="true"
                      ><path
                        d="M6 .8l1.55 3.3 3.6.45-2.65 2.5.68 3.57L6 8.85l-3.18 1.77.68-3.57L.85 4.55l3.6-.45z"
                        fill={r.default || dimStar ? 'currentColor' : 'none'}
                        stroke="currentColor"
                        stroke-width="1"
                        stroke-linejoin="miter"
                      /></svg
                    >
                  </button>
                {/if}
              </div>
              <div class="flex flex-wrap items-center gap-x-2 gap-y-0.5 pl-6 font-mono text-[11px] text-muted">
                <span>{r.harness}</span>
                <span aria-hidden="true">&middot;</span>
                <span>{r.profile ? (r.useDefault ? "CLI's own config" : 'own home') : 'own login, no profile'}</span>
                {#if u?.plan}<span aria-hidden="true">&middot;</span><span title="Plan">{u.plan}</span>{/if}
              </div>
              <div class="pl-6 text-[12px] {st.tone}" title={r.problem ?? loginTitle(r)}>
                {st.text}{#if r.disabled && !r.problem}<span class="text-muted"> &middot; switched off</span>{/if}
              </div>
              {#if r.problem}<p class="m-0 pl-6 text-[12px] text-fail">{r.problem}</p>{/if}
            </div>

            <!-- What runs on it: sessions, and the repos that start on it. -->
            <div class="flex min-w-0 flex-col gap-1 text-[12px] max-lg:pl-6">
              {#if on.length}
                <button
                  class="w-fit border-0 bg-transparent p-0 text-left text-[12px] text-ink underline-offset-4 hover:underline"
                  onclick={() => (sessionsOpen = sessionsOpen === r.key ? null : r.key)}
                  aria-expanded={sessionsOpen === r.key}
                  title="Show the sessions on {r.name}">{on.length} {on.length === 1 ? 'session' : 'sessions'}</button
                >
              {:else}
                <span class="text-muted">no sessions</span>
              {/if}
              {#if projects.length}
                <span class="text-muted" title="These repos start {r.harness} sessions on {r.name} when none is picked"
                  >default for {projects.map((p) => p.name).join(', ')}</span
                >
              {/if}
              {#if sessionsOpen === r.key && on.length}
                <ul class="m-0 mt-1 list-none border border-rule p-0" aria-label="Sessions on {r.name}">
                  {#each on as a (a.id)}
                    <li>
                      <button
                        class="block w-full truncate border-0 bg-transparent px-1.5 py-0.5 text-left text-[12px] text-ink hover:bg-surface"
                        onclick={() => openSession({ Agent: a.id })}
                        title="Open {a.name}">{a.name}</button
                      >
                    </li>
                  {/each}
                </ul>
              {/if}
            </div>

            <!-- Use: 5-hour and weekly (Copilot: its monthly allowances), with resets. -->
            <div class="flex min-w-0 flex-col gap-1.5 max-lg:pl-6" title={usageTitle(r, u, app.now)}>
              {#each [top, bottom] as w, i (i)}
                <div class="flex min-w-0 items-center gap-2">
                  <div class="min-w-0 flex-1">
                    <UsageBar label={windowLabel(w, i as 0 | 1)} percent={w ? shownAt(w, app.now) : null} stale={!!w && staleAt(w, app.now)} />
                  </div>
                  <span class="w-[92px] flex-none truncate font-mono text-[10px] text-faint">{resetLine(w)}</span>
                </div>
              {/each}
            </div>

            <!-- Verbs. -->
            <div class="flex flex-wrap items-center gap-1.5 max-lg:pl-6">
              {#if r.profile && !r.problem}
                <button
                  class="relative mr-1 h-[14px] w-[26px] flex-none border p-0 {r.disabled ? 'border-rule bg-transparent' : 'border-rule-strong bg-ink'}"
                  role="switch"
                  aria-checked={!r.disabled}
                  onclick={() => setEnabled(r)}
                  title={r.disabled ? `Off: launches that pick no account skip ${r.name}. Switch on` : `On. Switch ${r.name} off`}
                  aria-label="{r.name} enabled"
                >
                  <span class="absolute top-[2px] size-[8px] {r.disabled ? 'left-[2px] bg-faint' : 'right-[2px] bg-bg'}" aria-hidden="true"></span>
                </button>
              {/if}
              {#if r.loginHint && !r.problem}
                <button
                  class="{btn} {r.login === 'needs_login' ? 'border-ember text-ember-ink' : ''}"
                  onclick={() => signIn(r)}
                  title="Run {r.loginHint} in a new terminal{r.profile ? ', and watch for its credentials' : ''}"
                  >{r.login === 'logged_in' ? 'Re-sign in' : 'Sign in'}</button
                >
              {/if}
              {#if r.profile}
                <button class={btn} onclick={() => edit(r)} aria-expanded={editing?.key === r.key} disabled={!!r.problem} title={r.problem ? 'Fix this entry in config.json, or rename it (R)' : `Edit ${r.name}`}
                  >Edit</button
                >
                {#if !r.problem}
                  <button class={btn} onclick={() => duplicate(r)} title="A new {r.harness} account with {r.name}'s variables, signed in separately">Duplicate</button>
                  <button class={btn} onclick={(e) => useFor(r, e.currentTarget as HTMLElement)} aria-haspopup="menu" title="Make {r.name} the {r.harness} account a repo starts sessions on"
                    >Use for&hellip;</button
                  >
                {/if}
                {#if r.name !== '(no name)'}
                  {@const blocked = app.model.connected ? renameBlocker(r.name, app.model.agents) : RENAME_OFFLINE}
                  <button
                    class="{btn} {blocked ? 'text-faint' : ''}"
                    onclick={() => rename(r)}
                    aria-disabled={!!blocked}
                    title={blocked ?? `Rename ${r.name}: its home and repo defaults follow (R)`}>Rename</button
                  >
                {/if}
                <button class="{btn} text-fail hover:border-fail" onclick={() => remove(r)} title="Remove {r.name}, its home kept or deleted as you choose (Delete)">Remove</button>
              {:else}
                <button
                  class={btn}
                  onclick={() => openAddAccount({ harness: r.harness, adopt: true })}
                  title="Add this login as a named account, so it can be starred, switched off or used for a repo">Add as account</button
                >
              {/if}
            </div>
          </div>
          {#if editing?.key === r.key}
            {#key editing}
              <div class="mt-3 max-w-[760px]">
                <ProfileEditor
                  isNew={false}
                  initial={editing.form}
                  envKeys={editing.envKeys}
                  harnesses={harnesses.length ? harnesses : [r.harness]}
                  onsaved={() => {
                    editing = null;
                    toast(`Saved ${r.name}.`);
                    refreshLaunchables();
                  }}
                  oncancel={() => (editing = null)}
                />
              </div>
            {/key}
          {/if}
        </li>
      {:else}
        <li class="border-b border-rule py-6 text-[13px] text-muted">
          No accounts yet: sessions run on each CLI's own login.
          <button class="ml-1 border-0 bg-transparent p-0 text-[13px] text-ink underline underline-offset-4" onclick={() => openAddAccount()}>Add one</button>
          to run a second subscription beside it.
        </li>
      {/each}
    </ul>
  </div>
</section>
