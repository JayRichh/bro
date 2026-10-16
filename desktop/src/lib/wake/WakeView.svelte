<script lang="ts">
  // WHAKAOHO (wake-up): at an hour the user picks, Kete sends one tiny
  // prompt to the cheapest model under a Claude account, so its 5-hour
  // window starts then and resets earlier in their day. This is the view:
  // what it does in plain words, the scheduled task's line (or the crontab
  // lines to add by hand), the timeline of every account's windows on top,
  // and a card per account below with its switch, hour, days and options.
  // It reads the overview when it opens, every minute while shown, and
  // after each save (state.svelte.ts). A place that names an account
  // scrolls to its card and focuses its switch.
  import { onMount, tick } from 'svelte';
  import { openAccounts } from '../accounts/actions';
  import { nav } from '../shell/history.svelte';
  import { toast } from '../store.svelte';
  import { editWake, refreshWake, scheduleOf, wake } from './state.svelte';
  import { accountLabel, domId, hhmm, stagger } from './timeline';
  import WakeCard from './WakeCard.svelte';
  import WakeTimeline from './WakeTimeline.svelte';

  const overview = $derived(wake.overview);
  const accounts = $derived(overview?.accounts ?? []);
  /** Accounts switched on: with two or more, a stagger makes sense. */
  const on = $derived(accounts.filter((a) => a.known && scheduleOf(a).enabled));

  onMount(() => {
    void refreshWake();
    const again = () => {
      if (document.visibilityState === 'visible') void refreshWake();
    };
    const t = setInterval(again, 60_000);
    window.addEventListener('focus', again);
    return () => {
      clearInterval(t);
      window.removeEventListener('focus', again);
    };
  });

  /** Bring an account's card into view and focus its switch. By id, never
   *  a selector: an account key can hold characters a selector can't. */
  function showCard(account: string) {
    const id = domId(account);
    document.getElementById(`wake-card-${id}`)?.scrollIntoView({ block: 'start' });
    document.getElementById(`wake-switch-${id}`)?.focus({ preventScroll: true });
  }

  // Opened on an account (the Accounts view's link, the account menu):
  // once, when the overview has it.
  let shown = '';
  $effect(() => {
    const item = nav.current.view === 'whakaoho' ? nav.current.item : undefined;
    if (!item) {
      shown = '';
      return;
    }
    if (shown === item || !accounts.some((a) => a.account === item)) return;
    shown = item;
    void tick().then(() => showCard(item));
  });

  /** The first switched-on account keeps its hour, each next one sits
   *  2h30 after the one before; each is saved in turn. */
  function staggerNow() {
    const out = stagger(on.map((a) => ({ account: a.account, anchor: scheduleOf(a).anchor_min })));
    for (const a of on) if (out[a.account] !== scheduleOf(a).anchor_min) editWake(a, { anchor_min: out[a.account] }, 0);
    toast(`Staggered: ${on.map((a) => `${accountLabel(a.account)} ${hhmm(out[a.account])}`).join(', ')}`);
  }

  const btn = 'h-6 border border-rule bg-transparent px-2 text-[12px] text-ink hover:border-rule-strong disabled:text-faint disabled:hover:border-rule';
  const banner = 'm-0 border px-3 py-2 text-[13px]';
</script>

<section class="flex min-h-0 min-w-0 flex-1 flex-col overflow-y-auto" aria-label="Whakaoho, wake-up">
  <div class="@container mx-auto w-full max-w-[1180px] px-6 pt-5 pb-10 max-sm:px-3">
    <div class="flex flex-wrap items-baseline gap-x-3 gap-y-2 border-b border-rule-strong pb-2">
      <h1 class="m-0 text-[22px] font-semibold tracking-tight text-ink">Whakaoho <span class="font-normal text-muted">&middot; Wake-up</span></h1>
      <span class="flex-1"></span>
      <button class={btn} onclick={() => refreshWake()} disabled={wake.loading} title="Read the schedules, windows and usual hours again">Refresh</button>
    </div>

    <p class="m-0 mt-3 max-w-[72ch] text-[13px] text-muted">
      At the hour you pick, Kete sends one tiny prompt to {overview?.model ?? 'haiku'} with your own subscription, so the account's 5-hour window starts then and
      resets earlier in your day. Off by default, per account.
    </p>

    {#if overview}
      <p class="m-0 mt-2 font-mono text-[12px] text-ink" title={overview.log ? `Every decision is logged in ${overview.log}` : undefined}>{overview.task_line}</p>
      {#if !overview.windows && overview.crontab.length}
        <div class="mt-2">
          <p class="m-0 mb-1 text-[12px] text-muted">Kete installs nothing on this system. Add these lines with <code class="font-mono">crontab -e</code>:</p>
          <pre class="m-0 border border-rule bg-surface px-3 py-2 font-mono text-[12px] break-all whitespace-pre-wrap text-ink select-text">{#each overview.crontab as line, i (i)}<span class="block">{line}</span>{/each}</pre>
        </div>
      {/if}
    {/if}

    {#if overview?.daemon === false}
      <p class="{banner} mt-3 border-rule bg-surface text-ink" role="status">
        The running daemon is older than this app: wake-ups fall back to claude -p (print mode) until it restarts. Run <code class="font-mono">kete kill</code>; Kete
        restarts it.
      </p>
    {:else if overview?.daemon === null}
      <p class="{banner} mt-3 border-rule bg-surface text-ink" role="status">Kete's daemon isn't running, so usual hours are unknown.</p>
    {/if}
    {#if wake.error}
      <p class="{banner} mt-3 border-fail text-fail" role="alert">{wake.error}</p>
    {/if}

    {#if !overview}
      {#if wake.loading}
        <p class="m-0 mt-5 text-[13px] text-muted" role="status">Reading the wake-up schedules&hellip;</p>
      {:else if wake.error}
        <button class="{btn} mt-3" onclick={() => refreshWake()}>Try again</button>
      {/if}
    {:else if !accounts.length}
      <p class="m-0 mt-5 text-[13px] text-muted">
        No Claude accounts yet.
        <button class="ml-1 border-0 bg-transparent p-0 text-[13px] text-ink underline underline-offset-4" onclick={openAccounts}>Open Accounts</button>
        to add one.
      </p>
    {:else}
      <div class="mt-5">
        <WakeTimeline {overview} onpick={showCard} />
      </div>

      <div class="mt-6 flex flex-wrap items-center gap-x-3 gap-y-1.5 border-b border-rule pb-1.5">
        <h2 class="m-0 text-[13px] font-semibold text-ink">Accounts</h2>
        <span class="flex-1"></span>
        {#if on.length >= 2}
          <button
            class={btn}
            onclick={staggerNow}
            title="Space the switched-on accounts 2h30 apart, the first keeping its hour: resets interleave, so account spread always has a fresh window to move to"
            >Stagger</button
          >
        {/if}
      </div>
      <div class="mt-3 grid grid-cols-1 items-start gap-3 @min-[860px]:grid-cols-2">
        {#each accounts as a (a.account)}
          <WakeCard account={a} {overview} />
        {/each}
      </div>
    {/if}
  </div>
</section>
