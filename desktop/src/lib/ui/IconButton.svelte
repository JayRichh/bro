<script lang="ts">
  // A square verb button: an icon, its tooltip naming the key (build it
  // with `withKey`), and a word beside the icon once the container is wide
  // enough (`wordAt`: a container query width, or never). Clicks stop at
  // the button, so a row or card under it does not open too.
  import Icon, { type IconName } from './Icon.svelte';

  let {
    icon,
    label,
    title,
    word,
    onclick,
    disabled = false,
    pressed,
    menu = false,
    danger = false,
    class: extra = '',
  }: {
    icon: IconName;
    /** The accessible name: the verb, without its key. */
    label: string;
    /** The tooltip: the verb and its key. */
    title?: string;
    /** Shown beside the icon in a wide container. */
    word?: string;
    onclick: (e: MouseEvent) => void;
    disabled?: boolean;
    pressed?: boolean;
    menu?: boolean;
    danger?: boolean;
    class?: string;
  } = $props();
</script>

<button
  class="inline-flex h-7 min-w-7 flex-none items-center justify-center gap-1.5 border border-rule bg-transparent px-1.5 text-[13px] whitespace-nowrap
    enabled:hover:border-rule-strong disabled:text-faint {pressed ? 'border-ink bg-ink text-bg' : danger ? 'text-fail' : 'text-ink'} {extra}"
  title={title ?? label}
  aria-label={label}
  aria-pressed={pressed}
  aria-haspopup={menu ? 'menu' : undefined}
  {disabled}
  onclick={(e) => {
    e.stopPropagation();
    onclick(e);
  }}
>
  <Icon name={icon} />
  {#if word}<span class="hidden @min-[720px]:inline">{word}</span>{/if}
</button>
