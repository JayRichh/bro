<script lang="ts">
  import type { Snippet } from 'svelte';

  // A 6px square plus a word: a PR state, a vote or a policy row never
  // rests on colour alone. `square` is the square's token classes.
  let {
    square,
    word,
    strong = false,
    class: extra = '',
    children,
  }: { square: string; word: string; strong?: boolean; class?: string; children?: Snippet } = $props();
</script>

<span class="inline-flex min-w-0 items-center gap-1.5 {extra}">
  <span class="size-1.5 flex-none {square}" aria-hidden="true"></span>
  <span class="text-[12px] leading-none whitespace-nowrap {strong ? 'font-medium text-ink' : 'text-muted'}">{word}</span>
  {#if children}{@render children()}{/if}
</span>
