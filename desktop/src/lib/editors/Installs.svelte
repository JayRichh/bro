<script lang="ts">
  // Settings, Desktop, Editors: the installs Kete found (VS Code and its
  // forks, each Visual Studio instance), with a Re-scan for after one is
  // installed or updated. The first of each kind is what its Open in row
  // opens; Open folders in / Open files in above pick between kinds.
  import { toast } from '../store.svelte';
  import { editors, rescanEditors } from './store.svelte';

  let scanning = $state(false);

  async function rescan() {
    scanning = true;
    try {
      const s = await rescanEditors();
      toast(s.editors.length ? `Found ${s.editors.length} ${s.editors.length === 1 ? 'editor' : 'editors'}` : 'No VS Code or Visual Studio found');
    } catch (e) {
      toast(`Could not scan: ${e}`);
    } finally {
      scanning = false;
    }
  }

  const where: Record<string, string> = {
    'user-install': 'installed for you',
    'system-install': 'installed for everyone',
    'app-paths': 'registered',
    'known-folder': 'found in its folder',
    path: 'on PATH',
    vswhere: 'Visual Studio Installer',
  };
</script>

<div class="border-b border-rule px-6 py-3">
  <div class="flex items-baseline gap-3">
    <span class="text-[14px] font-medium text-ink">Installed</span>
    <span class="flex-1"></span>
    <button
      class="inline-flex h-7 flex-none items-center border border-rule bg-transparent px-2 text-[13px] text-ink enabled:hover:border-rule-strong disabled:text-faint"
      title="Look for VS Code, its forks and Visual Studio again (after installing or updating one)"
      disabled={scanning}
      onclick={rescan}>{scanning ? 'Scanning\u2026' : 'Re-scan'}</button
    >
  </div>
  {#if !editors.loaded}
    <p class="m-0 mt-1 text-[13px] text-muted">Looking for editors&hellip;</p>
  {:else if !editors.list.length}
    <p class="m-0 mt-1 max-w-[68ch] text-[13px] leading-snug text-muted">
      No VS Code (or a fork such as Cursor or VSCodium) and no Visual Studio found. Install one, then Re-scan; until then Open in editor runs the
      repo's Open command.
    </p>
  {:else}
    <ul class="m-0 mt-2 list-none border-t border-rule p-0" aria-label="Installed editors">
      {#each editors.list as e, i (e.id)}
        {@const first = editors.list.findIndex((x) => x.family === e.family) === i}
        <li class="grid grid-cols-[16px_minmax(0,1fr)] items-start gap-x-2.5 border-b border-rule py-2">
          {#if e.icon}<img src={e.icon} alt="" width="16" height="16" class="mt-0.5 block" draggable="false" />{:else}<span
              class="mt-0.5 font-mono text-[10px] text-muted">{e.family === 'code' ? 'VSC' : 'VS'}</span
            >{/if}
          <div class="min-w-0">
            <div class="flex min-w-0 flex-wrap items-baseline gap-x-2">
              <span class="text-[13px] text-ink">{e.name}</span>
              {#if e.version}<span class="font-mono text-[12px] text-muted">{e.version}</span>{/if}
              {#if e.edition && e.family === 'visual-studio'}<span class="text-[12px] text-muted">{e.edition}</span>{/if}
              {#if e.prerelease}<span class="border border-rule px-1 font-mono text-[11px] leading-4 text-muted">preview</span>{/if}
              {#if first}<span class="border border-rule px-1 font-mono text-[11px] leading-4 text-muted" title="What Open in {e.family === 'code' ? 'VS Code' : 'Visual Studio'} opens">default</span>{/if}
            </div>
            <p class="m-0 truncate font-mono text-[11px] text-faint" title={e.exe}>{e.exe} &middot; {where[e.source] ?? e.source}</p>
          </div>
        </li>
      {/each}
    </ul>
  {/if}
</div>
