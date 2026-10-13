<script lang="ts">
  import { TONE_LABEL, type Tone } from './status';

  let { tone, size = 14 }: { tone: Tone; size?: number } = $props();

  const COLOR: Record<Tone, string> = {
    needs: 'text-st-needs',
    running: 'text-st-running',
    unseen: 'text-st-unseen',
    done: 'text-st-done',
    pr: 'text-st-pr',
    shell: 'text-st-shell',
    dead: 'text-st-idle',
    idle: 'text-st-idle',
  };
</script>

<!-- Each state is a colour and a shape: ! needs input, a turning arc while
     running, a filled dot unread, a tick done, a ring idle, a bar stopped. -->
<svg
  class="block flex-none {COLOR[tone]}"
  width={size}
  height={size}
  viewBox="0 0 16 16"
  role="img"
  aria-label={TONE_LABEL[tone]}
>
  <title>{TONE_LABEL[tone]}</title>
  {#if tone === 'needs'}
    <circle cx="8" cy="8" r="7" fill="currentColor" />
    <path d="M8 4.2v4.6" class="stroke-on-accent" stroke-width="1.9" stroke-linecap="round" />
    <circle cx="8" cy="11.4" r="1.05" class="fill-on-accent" />
  {:else if tone === 'running'}
    <circle cx="8" cy="8" r="6" fill="none" stroke="currentColor" stroke-opacity="0.28" stroke-width="2" />
    <path class="origin-center animate-spin [transform-box:view-box]" d="M8 2a6 6 0 0 1 6 6" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" />
    <circle cx="8" cy="8" r="2" fill="currentColor" />
  {:else if tone === 'unseen'}
    <circle cx="8" cy="8" r="6.5" fill="currentColor" fill-opacity="0.22" />
    <circle cx="8" cy="8" r="3.6" fill="currentColor" />
  {:else if tone === 'done'}
    <circle cx="8" cy="8" r="7" fill="currentColor" />
    <path d="M4.8 8.3l2.2 2.2 4.2-4.6" fill="none" class="stroke-on-accent" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" />
  {:else if tone === 'pr'}
    <circle cx="5" cy="4" r="1.7" fill="none" stroke="currentColor" stroke-width="1.5" />
    <circle cx="5" cy="12" r="1.7" fill="none" stroke="currentColor" stroke-width="1.5" />
    <circle cx="11.5" cy="12" r="1.7" fill="none" stroke="currentColor" stroke-width="1.5" />
    <path d="M5 5.7v4.6M11.5 10.3V7a2 2 0 0 0-2-2H7.5" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" />
  {:else if tone === 'shell'}
    <rect x="1.5" y="2.5" width="13" height="11" rx="2.2" fill="none" stroke="currentColor" stroke-width="1.4" />
    <path d="M4.5 6.2l2 1.8-2 1.8M8.5 10.3h3" fill="none" stroke="currentColor" stroke-width="1.4" stroke-linecap="round" stroke-linejoin="round" />
  {:else if tone === 'dead'}
    <circle cx="8" cy="8" r="6" fill="none" stroke="currentColor" stroke-width="1.6" />
    <path d="M5.3 8h5.4" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" />
  {:else}
    <circle cx="8" cy="8" r="5.6" fill="none" stroke="currentColor" stroke-width="1.6" />
  {/if}
</svg>
