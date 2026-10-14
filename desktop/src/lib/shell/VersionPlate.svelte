<script lang="ts">
  // The rail's version plate, as the terminal app's footer: `kete v0.46.0`,
  // with an ember up-arrow and `v0.47.0` once a newer release is out. The cue opens
  // the way to update; nothing installs from the window.
  import { onDestroy, onMount } from 'svelte';
  import { api } from '../api';
  import { toast } from '../store.svelte';
  import { ask } from '../ui/ui.svelte';
  import { plate, updateApi, type UpdateStatus } from '../update';

  let status = $state<UpdateStatus | null>(null);
  const words = $derived(plate(status));

  let unlisten: (() => void) | undefined;
  onMount(() => {
    updateApi
      .status()
      .then((s) => (status = s))
      .catch(() => {});
    updateApi
      .onUpdate((s) => (status = s))
      .then((u) => (unlisten = u))
      .catch(() => {});
  });
  onDestroy(() => unlisten?.());

  async function offer() {
    const s = status;
    if (!s?.latest) return;
    const r = await ask.confirm({
      title: `Kete ${s.latest} is out`,
      message: `This is ${s.version}. To update, run this in a terminal, then reopen the window:`,
      detail: [s.command],
      confirm: 'Read the release notes',
    });
    if (r.ok) api.openUrl(s.url).catch((e) => toast(String(e)));
  }
</script>

{#if words.version}
  <div class="flex h-7 flex-none items-center gap-2 border-t border-rule px-4 font-mono text-[11px] text-muted">
    <span>{words.version}</span>
    {#if words.update}
      <button
        class="border-0 bg-transparent p-0 font-mono text-[11px] font-semibold text-ember-ink hover:underline hover:underline-offset-4"
        onclick={offer}
        title="A newer Kete is published: how to update">&#8673; {words.update}</button
      >
    {/if}
  </div>
{/if}
