<script lang="ts" module>
  // The window's verb icons: 16-unit grid, square caps, a 1.4 stroke, so
  // they sit with the hand-drawn ones in the rail and the diff bar.
  export type IconName =
    | 'plus'
    | 'branch'
    | 'switch'
    | 'editor'
    | 'terminal'
    | 'restart'
    | 'reply'
    | 'archive'
    | 'unarchive'
    | 'maximize'
    | 'restore'
    | 'diff'
    | 'dots'
    | 'help'
    | 'palette'
    | 'close';
</script>

<script lang="ts">
  let { name, size = 14 }: { name: IconName; size?: number } = $props();
</script>

<svg width={size} height={size} viewBox="0 0 16 16" aria-hidden="true" class="flex-none">
  {#if name === 'plus'}
    <path d="M8 2.5v11M2.5 8h11" fill="none" stroke="currentColor" stroke-width="1.5" />
  {:else if name === 'branch'}
    <path d="M4.5 2.5v11M4.5 9.5c0-3 7-2 7-5M11.5 4.5v-2" fill="none" stroke="currentColor" stroke-width="1.4" />
    <rect x="3" y="12" width="3" height="3" fill="currentColor" /><rect x="10" y="1" width="3" height="3" fill="currentColor" />
  {:else if name === 'switch'}
    <path d="M2.5 5h10M10 2.5L12.5 5 10 7.5M13.5 11h-10M6 8.5L3.5 11 6 13.5" fill="none" stroke="currentColor" stroke-width="1.4" />
  {:else if name === 'editor'}
    <path d="M2.5 2.5h7l4 4v7h-11z" fill="none" stroke="currentColor" stroke-width="1.3" />
    <path d="M6 8.5L4.5 10 6 11.5M10 8.5l1.5 1.5-1.5 1.5" fill="none" stroke="currentColor" stroke-width="1.3" />
  {:else if name === 'terminal'}
    <rect x="1.5" y="2.5" width="13" height="11" fill="none" stroke="currentColor" stroke-width="1.3" />
    <path d="M4 6l2 2-2 2M8 10.5h4" fill="none" stroke="currentColor" stroke-width="1.3" />
  {:else if name === 'restart'}
    <path d="M13.5 8A5.5 5.5 0 1 1 11.8 4M13.5 2v3h-3" fill="none" stroke="currentColor" stroke-width="1.5" />
  {:else if name === 'reply'}
    <path d="M2.5 3.5h11v7h-6l-3 2.5v-2.5h-2z" fill="none" stroke="currentColor" stroke-width="1.3" />
    <path d="M5 7h6" stroke="currentColor" stroke-width="1.3" />
  {:else if name === 'archive'}
    <path d="M1.5 2.5h13v3h-13zM2.5 5.5v8h11v-8M6 8.5h4" fill="none" stroke="currentColor" stroke-width="1.3" />
  {:else if name === 'unarchive'}
    <path d="M1.5 2.5h13v3h-13zM2.5 5.5v8h11v-8M8 12V7.5M6 9.5l2-2 2 2" fill="none" stroke="currentColor" stroke-width="1.3" />
  {:else if name === 'maximize'}
    <path d="M9.5 2.5h4v4M6.5 13.5h-4v-4M13.5 2.5L9 7M2.5 13.5L7 9" fill="none" stroke="currentColor" stroke-width="1.4" />
  {:else if name === 'restore'}
    <path d="M13.5 6.5h-4v-4M2.5 9.5h4v4M9.5 6.5L14 2M6.5 9.5L2 14" fill="none" stroke="currentColor" stroke-width="1.4" />
  {:else if name === 'diff'}
    <path d="M5 2.5v6M2 5.5h6M2 12.5h6M10 2.5h4v11h-4" fill="none" stroke="currentColor" stroke-width="1.4" />
  {:else if name === 'dots'}
    <rect x="2" y="7" width="2.4" height="2.4" fill="currentColor" /><rect x="6.8" y="7" width="2.4" height="2.4" fill="currentColor" /><rect
      x="11.6"
      y="7"
      width="2.4"
      height="2.4"
      fill="currentColor"
    />
  {:else if name === 'help'}
    <rect x="1.5" y="1.5" width="13" height="13" fill="none" stroke="currentColor" stroke-width="1.3" />
    <path d="M6 6.2a2 2 0 1 1 2.8 1.8c-.6.3-.8.7-.8 1.3v.7" fill="none" stroke="currentColor" stroke-width="1.4" />
    <rect x="7.25" y="11" width="1.5" height="1.5" fill="currentColor" />
  {:else if name === 'palette'}
    <rect x="1.5" y="2.5" width="13" height="11" fill="none" stroke="currentColor" stroke-width="1.3" />
    <path d="M4 6.5l2 1.5-2 1.5" fill="none" stroke="currentColor" stroke-width="1.3" />
  {:else if name === 'close'}
    <path d="M3.5 3.5l9 9M12.5 3.5l-9 9" fill="none" stroke="currentColor" stroke-width="1.4" />
  {/if}
</svg>
