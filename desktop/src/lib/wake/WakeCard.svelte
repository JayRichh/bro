<script lang="ts">
  // One account's WAKE-UP CARD: the switch, the hour slider (15-minute
  // steps) with the windows it would make and the suggested hour, the
  // days, the options (keep rolling, skip if idle, wake the PC, mode), the
  // last decision and its history, and Wake now. Every change shows at
  // once from the draft in state.svelte.ts (the timeline follows it) and
  // saves after a quiet spell; nothing here talks to the Rust side but
  // through that file.
  import { openAccounts } from '../accounts/actions';
  import { copyText } from '../menus/clipboard';
  import { ownMenu } from '../menus/router';
  import { nav } from '../shell/history.svelte';
  import { wakeCardItems } from './card-menu';
  import { editWake, scheduleOf, wake, wakeEdit, wakeNowFor } from './state.svelte';
  import {
    DAY_LABELS,
    MAX_ANCHOR,
    STEP,
    WINDOW,
    accountLabel,
    domId,
    freshText,
    hhmm,
    hoursText,
    modeChoice,
    onDay,
    previewAt,
    sliderFrac,
    snap,
    toggleDay,
    when,
    windowsText,
  } from './timeline';
  import type { WakeAccount, WakeModeChoice, WakeOverview } from './types';

  let { account: a, overview }: { account: WakeAccount; overview: WakeOverview } = $props();

  const key = $derived(a.account);
  const dom = $derived(domId(a.account));
  const name = $derived(accountLabel(a.account));
  const sched = $derived(scheduleOf(a));
  const preview = $derived(previewAt(a, sched.anchor_min));
  const fresh = $derived(freshText(preview));
  const usual = $derived(hoursText(a.usual));
  /** Switched on, but nothing can go out until it is logged in again: the one needs-you case. */
  const needs = $derived(sched.enabled && !a.logged_in);
  /** An edit not yet landed: the saved `next` no longer describes it. */
  const pending = $derived(wakeEdit.draft[a.account] != null);
  const next = $derived(a.next != null && sched.enabled && !pending ? when(a.next, overview.offset) : null);
  const waking = $derived(!!wake.waking[a.account]);
  let historyOpen = $state(false);

  /** The thumb's width in the slider's CSS: the tick under it allows for it. */
  const THUMB = 10;
  const at = (min: number) => `calc(${THUMB / 2}px + (100% - ${THUMB}px) * ${sliderFrac(min)})`;
  /** The chip stays on the card: tucked under its tick at the slider's ends. */
  const chipShift = (min: number) => {
    const f = sliderFrac(min);
    return f < 0.18 ? '-10px' : f > 0.82 ? 'calc(-100% + 10px)' : '-50%';
  };

  const MODES: { id: WakeModeChoice; label: string; title: string }[] = [
    { id: 'auto', label: 'Auto', title: 'Interactive (a hidden Claude session), or print mode when the daemon cannot run one' },
    { id: 'interactive', label: 'Interactive', title: 'Always a hidden interactive Claude session: plain subscription use. Needs the daemon' },
    { id: 'print', label: 'Print', title: 'claude -p, a plain headless prompt. On some plans it may not start the 5-hour window' },
  ];
  const SKIP = [1, 2, 3, 5, 7];

  /** A right-click anywhere on the card (menu matrix 5.21): its own rows,
   *  never the account row's keys. */
  const cardMenu = () => ({
    label: name,
    items: wakeCardItems(
      { a, sched, waking },
      {
        wakeNow: () => void wakeNowFor(key),
        setEnabled: (on) => editWake(a, { enabled: on }, 0),
        setHour: (min) => editWake(a, { anchor_min: min }, 0),
        copyLogin: (hint) => void copyText(hint, `${hint}. Run it in a terminal to sign in.`),
        details: () => nav.go({ view: 'accounts', item: key }),
      },
    ),
  });

  const btn = 'h-6 border border-rule bg-transparent px-2 text-[12px] text-ink hover:border-rule-strong disabled:text-faint disabled:hover:border-rule';
  const check = 'inline-flex items-center gap-2 text-[12px] text-ink';
</script>

<section id="wake-card-{dom}" class="border border-rule" aria-label="Wake-up for {name}" use:ownMenu={cardMenu}>
  <div class="flex flex-wrap items-center gap-x-3 gap-y-1 border-b border-rule px-3 py-2">
    <h2 class="m-0 min-w-0 truncate text-[14px] font-medium text-ink" title={name}>{name}</h2>
    <span class="min-w-0 flex-1 truncate font-mono text-[11px] text-muted">
      {#if next}next {next}{#if a.days_text}&nbsp;&middot; {a.days_text}{/if}{/if}
    </span>
    {#if a.known}
      <button
        id="wake-switch-{dom}"
        role="switch"
        aria-checked={sched.enabled}
        aria-label="Wake-up for {name}"
        class="inline-flex h-6 items-center gap-2 border-0 bg-transparent p-0 text-ink"
        onclick={() => editWake(a, { enabled: !sched.enabled }, 0)}
        title={sched.enabled
          ? `On: Kete sends one tiny prompt to ${overview.model} at ${hhmm(sched.anchor_min)}, with your own subscription. Switch off`
          : 'Off: nothing is sent. Switch on to send one tiny prompt at the hour you pick, with your own subscription'}
      >
        <span class="relative h-[16px] w-[30px] flex-none border {sched.enabled ? 'border-rule-strong bg-ink' : 'border-rule bg-transparent'}" aria-hidden="true">
          <span class="absolute top-[3px] size-[8px] {sched.enabled ? 'right-[3px] bg-bg' : 'left-[3px] bg-faint'}"></span>
        </span>
        <span class="w-[22px] text-left font-mono text-[12px]">{sched.enabled ? 'On' : 'Off'}</span>
      </button>
    {/if}
  </div>

  <div class="flex flex-col gap-3 px-3 py-3">
    {#if !a.known}
      <div class="flex flex-wrap items-center gap-x-3 gap-y-1.5">
        <p class="m-0 text-[13px] text-muted">No such account any more.</p>
        {#if sched.enabled}
          <button class={btn} onclick={() => editWake(a, { enabled: false }, 0)} title="Switch this schedule off: the account is gone, so nothing can go out for it"
            >Switch it off</button
          >
        {/if}
      </div>
    {:else}
      {#if !a.logged_in}
        <p class="m-0 flex items-start gap-2 text-[13px] {needs ? 'text-ember-ink' : 'text-muted'}" title={needs ? 'Switched on, but nothing goes out until this account is logged in again' : undefined}>
          {#if needs}<span class="mt-[7px] size-1.5 flex-none bg-ember" aria-hidden="true"></span>{/if}
          <span>Not logged in{#if a.login_hint}: <code class="font-mono text-[12px]">{a.login_hint}</code>{/if}</span>
        </p>
      {/if}
      {#if a.disabled}
        <p class="m-0 flex flex-wrap items-center gap-x-3 gap-y-1 text-[13px] text-muted">
          <span>Switched off in Accounts: nothing goes out until it's on</span>
          <button
            class="border-0 bg-transparent p-0 text-[13px] text-ink underline underline-offset-4"
            onclick={openAccounts}
            title="Open Accounts, where each account's own switch is">Open Accounts</button
          >
        </p>
      {/if}

      <!-- The hour: a slider in 15-minute steps, what it would make, the hour that fits best. -->
      <div class="flex flex-col gap-1">
        <div class="flex items-center gap-3">
          <label for="wake-hour-{dom}" class="w-[32px] flex-none text-[12px] text-muted">Hour</label>
          <div class="min-w-0 flex-1">
            <input
              id="wake-hour-{dom}"
              class="wake-range block w-full"
              type="range"
              min="0"
              max={MAX_ANCHOR}
              step={STEP}
              value={sched.anchor_min}
              aria-valuetext={hhmm(sched.anchor_min)}
              title="The hour the first wake-up goes out. Arrow keys move it by 15 minutes"
              oninput={(e) => editWake(a, { anchor_min: Number(e.currentTarget.value) })}
              onfocus={() => (wakeEdit.active = key)}
              onblur={() => {
                if (wakeEdit.active === key) wakeEdit.active = null;
              }}
            />
            {#if a.suggested != null}
              {@const sug = a.suggested}
              <div class="relative mt-0.5 h-[22px]">
                <span class="pointer-events-none absolute top-0 h-[5px] w-px bg-ink" style="left: {at(sug)}" aria-hidden="true"></span>
                <button
                  class="absolute top-[5px] h-[17px] border px-1.5 font-mono text-[11px] leading-none whitespace-nowrap {sched.anchor_min === snap(sug)
                    ? 'border-rule-strong bg-transparent text-ink'
                    : 'border-rule bg-bg text-muted hover:border-rule-strong hover:text-ink'}"
                  style="left: {at(sug)}; transform: translateX({chipShift(sug)})"
                  onclick={() => editWake(a, { anchor_min: snap(sug) }, 0)}
                  title={usual
                    ? `Suggested: the most fresh windows inside your usual hours (${usual})`
                    : 'Suggested: the most fresh windows inside your usual hours'}>Suggested {hhmm(sug)}</button
                >
              </div>
            {/if}
          </div>
          <output for="wake-hour-{dom}" class="w-[48px] flex-none text-right font-mono text-[14px] text-ink">{hhmm(sched.anchor_min)}</output>
        </div>
        <p class="m-0 pl-[44px] text-[12px] text-muted max-sm:pl-0" title={windowsText(preview)}>
          {#if fresh}{fresh}{:else}Usual hours unknown yet: the window runs {hoursText({ start: sched.anchor_min, len: WINDOW })}{/if}
        </p>
        <p class="m-0 min-h-[16px] pl-[44px] font-mono text-[11px] text-muted max-sm:pl-0" aria-live="polite">
          {#if wakeEdit.saving[key]}Saving&hellip;{:else if wakeEdit.line[key]}{wakeEdit.line[key]}{/if}
        </p>
      </div>

      <!-- Days. -->
      <div class="flex flex-wrap items-center gap-x-3 gap-y-1.5">
        <span class="w-[32px] flex-none text-[12px] text-muted">Days</span>
        <div class="flex flex-wrap gap-1" role="group" aria-label="Days of the week">
          {#each DAY_LABELS as d, i (d)}
            {@const on = onDay(sched.days, i)}
            <button
              class="h-6 min-w-[40px] border px-2 font-mono text-[11px] {on ? 'border-rule-strong bg-ink text-bg' : 'border-rule bg-transparent text-muted hover:border-rule-strong hover:text-ink'}"
              aria-pressed={on}
              onclick={() => editWake(a, { days: toggleDay(sched.days, i) })}
              title={on ? `Wakes on ${d}. Click to skip ${d} (at least one day stays on)` : `Skips ${d}. Click to wake on ${d}`}>{d}</button
            >
          {/each}
        </div>
      </div>

      <!-- Options. -->
      <div class="flex flex-wrap items-center gap-x-5 gap-y-2">
        <label class={check} title="Also start a window at each 5-hour step inside your usual hours">
          <input
            type="checkbox"
            class="size-[14px] accent-ink"
            checked={sched.keep_rolling}
            onchange={(e) => editWake(a, { keep_rolling: e.currentTarget.checked })}
          />
          Keep rolling
        </label>
        <label class="inline-flex items-center gap-2 text-[12px] text-ink" title="Don't wake an account that saw no use in the last few days">
          Skip if idle
          <select
            class="h-7 border border-rule bg-bg px-1.5 font-mono text-[12px] text-ink outline-none hover:border-rule-strong"
            value={String(sched.skip_idle_days ?? '')}
            onchange={(e) => editWake(a, { skip_idle_days: e.currentTarget.value ? Number(e.currentTarget.value) : null })}
          >
            <option value="">Off</option>
            {#if sched.skip_idle_days && !SKIP.includes(sched.skip_idle_days)}<option value={String(sched.skip_idle_days)}>{sched.skip_idle_days} days</option>{/if}
            {#each SKIP as n (n)}<option value={String(n)}>{n} {n === 1 ? 'day' : 'days'}</option>{/each}
          </select>
        </label>
        {#if overview.windows}
          <label class={check} title="Let Windows wake the PC from sleep for the wake-up (the scheduled task's wake to run)">
            <input type="checkbox" class="size-[14px] accent-ink" checked={sched.wake_pc} onchange={(e) => editWake(a, { wake_pc: e.currentTarget.checked })} />
            Wake the PC
          </label>
        {/if}
        <div class="inline-flex items-center gap-2 text-[12px] text-ink">
          Mode
          <div class="flex h-7 border border-rule" role="radiogroup" aria-label="Mode">
            {#each MODES as m, i (m.id)}
              {@const on = modeChoice(sched) === m.id}
              <button
                role="radio"
                aria-checked={on}
                class="border-0 px-2.5 text-[12px] {i ? 'border-l border-rule' : ''} {on ? 'bg-ink text-bg' : 'bg-transparent text-muted hover:text-ink'}"
                title="{m.label}: {m.title}"
                onclick={() => editWake(a, { mode: m.id === 'auto' ? null : m.id })}>{m.label}</button
              >
            {/each}
          </div>
        </div>
      </div>
    {/if}

    <!-- The last decision, and the ones before it. -->
    <div class="flex flex-col gap-1">
      <p class="m-0 font-mono text-[11px] {a.last_summary ? 'text-muted' : 'text-faint'}">
        {#if a.last_summary}<span class="text-faint">last </span>{a.last_summary}{:else}No wake-up has run yet.{/if}
      </p>
      {#if a.recent.length}
        <button
          class="w-fit border-0 bg-transparent p-0 text-left text-[12px] text-ink underline-offset-4 hover:underline"
          aria-expanded={historyOpen}
          onclick={() => (historyOpen = !historyOpen)}
          title="The last decisions: fired, skipped and why">History ({a.recent.length})</button
        >
        {#if historyOpen}
          <ul class="m-0 flex list-none flex-col gap-0.5 border-l border-rule p-0 pl-2" aria-label="Recent wake-up decisions for {name}">
            {#each a.recent as r, i (i)}
              <li class="font-mono text-[11px] text-muted">{r.summary}</li>
            {/each}
          </ul>
        {/if}
      {/if}
    </div>

    {#if a.known}
      <div class="flex flex-wrap items-center gap-x-3 gap-y-1.5">
        <button
          class={btn}
          disabled={waking}
          onclick={() => wakeNowFor(key)}
          title="Send the tiny prompt to {overview.model} now, whatever the schedule. A hidden Claude session: up to 2 minutes"
          >{#if waking}Waking&hellip; (up to 2 min){:else}Wake now{/if}</button
        >
        {#if wake.result[key]}<span class="min-w-0 font-mono text-[11px] text-muted" aria-live="polite">{wake.result[key]}</span>{/if}
      </div>
    {/if}
  </div>
</section>

<style>
  /* The hour slider: a hairline track and a square ink thumb, the
     Workshop's own (no rounded thumb, no shadow). WebView2 and WebKit draw
     these pseudo-elements. */
  .wake-range {
    appearance: none;
    -webkit-appearance: none;
    height: 18px;
    margin: 0;
    padding: 0;
    background: transparent;
    cursor: default;
  }
  .wake-range::-webkit-slider-runnable-track {
    height: 2px;
    background: var(--color-faint);
  }
  .wake-range::-webkit-slider-thumb {
    -webkit-appearance: none;
    appearance: none;
    width: 10px;
    height: 18px;
    margin-top: -8px;
    border: 0;
    border-radius: 0;
    background: var(--color-ink);
  }
  .wake-range:hover::-webkit-slider-thumb {
    background: var(--color-muted);
  }
  .wake-range:focus-visible {
    outline: 2px solid var(--color-rule-strong);
    outline-offset: 1px;
  }
</style>
