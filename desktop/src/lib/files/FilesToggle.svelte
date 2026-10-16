<script lang="ts">
  // The header's Files, beside Diff: the checkout in context as a tree
  // (the focused row's, the session's, the repo's), or, pressed, back to
  // where the files were opened.
  import { nav } from '../shell/history.svelte';
  import { toggleFiles } from '../commands';
  import { withKey } from '../shortcuts';
  import Icon from '../ui/Icon.svelte';

  const on = $derived(nav.current.view === 'files');
</script>

<button
  class="inline-flex h-6 flex-none items-center gap-1.5 border px-2 text-[12px] {on ? 'border-ink bg-ink text-bg' : 'border-rule bg-transparent text-muted hover:text-ink'}"
  aria-pressed={on}
  title={withKey(on ? 'Close the files: back to where they were opened' : 'Files of the checkout in context: the focused row, the session, the repo', ['files', 'open_files'])}
  onclick={toggleFiles}><Icon name="files" size={12} /><span class="hidden @min-[700px]:inline">Files</span></button
>
