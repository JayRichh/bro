<script lang="ts">
  import { nav, type Place } from '../shell/history.svelte';

  let {
    kind,
    label,
    place,
  }: { kind: 'task' | 'branch' | 'pr' | 'session'; label: string; place: Place } = $props();

  const KIND: Record<typeof kind, string> = { task: 'task', branch: 'branch', pr: 'PR', session: 'session' };
</script>

<!-- A link to another view, drawn as a ruled label: kind word, then the name. -->
<button
  type="button"
  class="inline-flex max-w-full min-w-0 items-baseline gap-1.5 border border-rule bg-transparent px-1.5 py-px font-mono text-[12px] leading-[18px] text-ink hover:border-rule-strong hover:bg-surface"
  title="Open {KIND[kind]} {label}"
  onclick={(e) => {
    e.stopPropagation();
    nav.go(place);
  }}
>
  <span class="flex-none text-muted">{KIND[kind]}</span>
  <span class="truncate">{label}</span>
</button>
