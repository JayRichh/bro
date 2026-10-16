<script lang="ts">
  // The Whakaoho TIMELINE: one line per Claude account on a shared 24-hour
  // strip (two hours back, the rest ahead), so the windows of many accounts
  // read against each other and against the clock. Layered per line: the
  // activity heat (very faint), the live 5-hour window (solid), the windows
  // a wake-up at the chosen hour would start (hatched, as a stale reading
  // is), the scheduled wake-ups (ticks), the weekly reset (a flag), the
  // usual hours (an underline) and the now line. Beside each line sit the
  // capacity bars (the Accounts view's UsageBar). The line follows the
  // card's draft, so moving a slider redraws it before anything is saved.
  import type { UsageWindow } from '../api';
  import { app } from '../store.svelte';
  import UsageBar from '../accounts/UsageBar.svelte';
  import { shownAt, staleAt, usageTitle, windowLabel } from '../accounts/usage';
  import { scheduleOf, wakeEdit } from './state.svelte';
  import { WINDOW, accountLabel, daily, heat, hhmm, hoursText, liveWindow, pickSession, previewAt, slotsOf, strip, tickAt, ticks, when } from './timeline';
  import type { WakeAccount, WakeOverview, WakeSessionWindow } from './types';

  let { overview, onpick }: { overview: WakeOverview; onpick: (account: string) => void } = $props();

  const s = $derived(strip(overview.now, overview.offset));
  const axis = $derived(ticks(s));
  const rows = $derived(overview.accounts.filter((a) => a.known));

  /** The kept reading as a bar's window (`WakeSessionWindow.kind` is any string). */
  const asUsage = (w: WakeSessionWindow | null): UsageWindow | null => (w ? { ...w, kind: 'session' } : null);

  /** The caption of the model-weekly column: the first reading that has one. */
  const modelHead = $derived.by(() => {
    for (const a of rows) {
      const w = app.model.accounts?.[a.account]?.windows.find((x) => x.kind === 'model_weekly');
      if (w) return windowLabel(w, 1);
    }
    return null;
  });

  /** Everything one line draws, from the overview, the live reading and the draft. */
  function lane(a: WakeAccount) {
    const sched = scheduleOf(a);
    const u = app.model.accounts?.[a.account] ?? null;
    const find = (kind: UsageWindow['kind']) => u?.windows.find((w) => w.kind === kind) ?? null;
    const session = find('session');
    const weekly = find('weekly');
    const live = pickSession(session, a.session, overview.now);
    const reset = live?.resets_at && live.resets_at > overview.now ? live.resets_at : null;
    const slots = slotsOf(a, sched);
    // A switch that is off draws nothing it would do, unless its slider is
    // being worked: then the windows it would make show, to choose by.
    const show = sched.enabled || wakeEdit.active === a.account;
    const parts = [accountLabel(a.account)];
    if (reset) parts.push(`Live 5-hour window: ${when(reset - WINDOW * 60, overview.offset)} to ${when(reset, overview.offset)}`);
    if (weekly?.resets_at) parts.push(`Weekly limit resets ${when(weekly.resets_at, overview.offset)}`);
    parts.push(sched.enabled ? `Wake-up at ${slots.map(hhmm).join(', ')}` : 'Wake-up is off');
    const usual = hoursText(a.usual);
    if (usual) parts.push(`Usual hours ${usual}`);
    return {
      sched,
      u,
      title: parts.join('\n'),
      heat: heat(s, a.bins),
      usual: a.usual ? daily(s, a.usual.start, a.usual.start + a.usual.len) : [],
      live: liveWindow(s, live, overview.now),
      weekly: weekly?.resets_at ? tickAt(s, weekly.resets_at) : null,
      est: show ? previewAt(a, sched.anchor_min).windows.flatMap((w) => daily(s, w.start, w.end)) : [],
      slots: sched.enabled ? slots.flatMap((m) => daily(s, m, m + 1)) : [],
      session: session ?? asUsage(a.session),
      weeklyBar: weekly,
      model: find('model_weekly'),
      needs: sched.enabled && !a.logged_in,
    };
  }

  const HATCH = 'background-image: repeating-linear-gradient(90deg, var(--color-faint) 0 1px, transparent 1px 3px);';
  const rowTitle = (a: WakeAccount, u: ReturnType<typeof lane>['u']) =>
    usageTitle({ name: a.account, harness: 'claude', profile: a.account !== '@claude' }, u, app.now);
  /** A tick label stays on its strip: left-aligned at the start, right-aligned at the end. */
  const shift = (pct: number) => (pct < 4 ? '0' : pct > 96 ? '-100%' : '-50%');
</script>

{#if rows.length}
  <section class="flex flex-col gap-2" aria-label="Wake-up timeline">
    <div class="flex flex-wrap items-baseline gap-x-3 gap-y-0.5">
      <h2 class="m-0 text-[13px] font-semibold text-ink">Next 24 hours</h2>
      <span class="font-mono text-[11px] text-muted">local time, now at the line</span>
    </div>

    <div class="grid grid-cols-[minmax(48px,84px)_minmax(0,1fr)_auto] items-center gap-x-3 gap-y-1 @min-[640px]:grid-cols-[minmax(64px,140px)_minmax(0,1fr)_auto]">
      <!-- The axis: an hour tick each hour, a label every third (every sixth when the strip is narrow). -->
      <span aria-hidden="true"></span>
      <div class="@container relative h-[16px]" aria-hidden="true">
        {#each axis as t (t.pct)}
          <span class="absolute bottom-0 w-px bg-rule {t.label ? 'h-[5px]' : 'h-[3px]'}" style="left: {t.pct}%"></span>
          {#if t.label}
            <span
              class="absolute top-0 font-mono text-[10px] leading-none text-faint {t.major ? '' : '@max-[340px]:hidden'}"
              style="left: {t.pct}%; transform: translateX({shift(t.pct)})">{t.label}</span
            >
          {/if}
        {/each}
        <span class="absolute bottom-0 w-px bg-ink" style="left: {s.nowPct}%; top: 0" aria-hidden="true"></span>
      </div>
      <div class="flex items-end gap-3 font-mono text-[10px] leading-none text-faint" aria-hidden="true">
        <span class="w-[80px] flex-none">5h</span>
        <span class="w-[80px] flex-none">wk</span>
        {#if modelHead}<span class="hidden w-[80px] flex-none @min-[760px]:block" title="The weekly limit of one model">{modelHead}</span>{/if}
      </div>

      {#each rows as a (a.account)}
        {@const L = lane(a)}
        <button
          class="flex min-w-0 items-center gap-1.5 border-0 bg-transparent p-0 text-left text-[12px] hover:underline hover:underline-offset-4 {L.needs ? 'text-ember-ink' : 'text-ink'} {a.disabled ? 'opacity-60' : ''}"
          onclick={() => onpick(a.account)}
          title="{accountLabel(a.account)}: wake-up {L.needs ? 'on, but not logged in' : L.sched.enabled ? 'on' : 'off'}. Open its card"
        >
          <span class="size-[6px] flex-none {L.needs ? 'bg-ember' : L.sched.enabled ? 'bg-ink' : 'border border-faint'}" aria-hidden="true"></span>
          <span class="min-w-0 truncate">{accountLabel(a.account)}</span>
        </button>

        <div class="relative h-[20px] bg-surface" role="img" aria-label={L.title} title={L.title}>
          {#each axis as t (t.pct)}
            {#if t.label}<span class="absolute inset-y-0 w-px bg-rule" style="left: {t.pct}%"></span>{/if}
          {/each}
          {#each L.heat as h, i (i)}
            <span class="absolute inset-y-0 bg-ink" style="left: {h.seg.left}%; width: {h.seg.width}%; opacity: {0.05 + h.level * 0.15}"></span>
          {/each}
          {#each L.usual as g, i (i)}
            <span class="absolute bottom-0 h-[2px] bg-faint" style="left: {g.left}%; width: {g.width}%"></span>
          {/each}
          {#each L.est as g, i (i)}
            <span class="absolute top-[11px] h-[5px] border-x border-faint" style="left: {g.left}%; width: {g.width}%; {HATCH}"></span>
          {/each}
          {#if L.live}<span class="absolute top-[3px] h-[6px] bg-muted" style="left: {L.live.left}%; width: {L.live.width}%"></span>{/if}
          {#each L.slots as g, i (i)}
            <span class="absolute inset-y-[2px] w-[2px] bg-ink" style="left: {g.left}%"></span>
          {/each}
          {#if L.weekly != null}
            <span class="absolute inset-y-0 w-px bg-muted" style="left: {L.weekly}%"></span>
            <span class="absolute top-0 size-[5px] border border-muted bg-bg" style="left: calc({L.weekly}% - 2px)"></span>
          {/if}
          <span class="absolute w-px bg-ink" style="left: {s.nowPct}%; top: -2px; bottom: -2px"></span>
        </div>

        <div class="flex items-center gap-3" title={rowTitle(a, L.u)}>
          <span class="w-[80px] flex-none"
            ><UsageBar width={44} percent={L.session ? shownAt(L.session, app.now) : null} stale={!!L.session && staleAt(L.session, app.now)} /></span
          >
          <span class="w-[80px] flex-none"
            ><UsageBar width={44} percent={L.weeklyBar ? shownAt(L.weeklyBar, app.now) : null} stale={!!L.weeklyBar && staleAt(L.weeklyBar, app.now)} /></span
          >
          {#if modelHead}
            <span class="hidden w-[80px] flex-none @min-[760px]:block">
              {#if L.model}<UsageBar width={44} percent={shownAt(L.model, app.now)} stale={staleAt(L.model, app.now)} />{/if}
            </span>
          {/if}
        </div>
      {/each}
    </div>

    <ul class="m-0 mt-1 flex list-none flex-wrap gap-x-4 gap-y-1 p-0 font-mono text-[10px] text-muted" aria-label="Legend">
      <li class="inline-flex items-center gap-1.5"><span class="h-[10px] w-px bg-ink" aria-hidden="true"></span>now</li>
      <li class="inline-flex items-center gap-1.5"><span class="h-[6px] w-[14px] bg-muted" aria-hidden="true"></span>live 5h window</li>
      <li class="inline-flex items-center gap-1.5"><span class="h-[10px] w-[2px] bg-ink" aria-hidden="true"></span>wake-up</li>
      <li class="inline-flex items-center gap-1.5">
        <span class="h-[5px] w-[14px] border-x border-faint" style={HATCH} aria-hidden="true"></span>windows it would start
      </li>
      <li class="inline-flex items-center gap-1.5"><span class="size-[5px] border border-muted bg-bg" aria-hidden="true"></span>weekly reset</li>
      <li class="inline-flex items-center gap-1.5"><span class="h-[2px] w-[14px] bg-faint" aria-hidden="true"></span>usual hours</li>
      <li class="inline-flex items-center gap-1.5"><span class="h-[8px] w-[14px] bg-ink opacity-15" aria-hidden="true"></span>activity</li>
    </ul>
  </section>
{/if}
