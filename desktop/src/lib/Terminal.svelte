<script lang="ts">
  import { onDestroy, onMount } from 'svelte';
  import { Terminal } from '@xterm/xterm';
  import { FitAddon } from '@xterm/addon-fit';
  import { WebglAddon } from '@xterm/addon-webgl';
  import { Unicode11Addon } from '@xterm/addon-unicode11';
  import { WebLinksAddon } from '@xterm/addon-web-links';
  import '@xterm/xterm/css/xterm.css';
  import { api, Channel, decodeB64, type PtyEvent, type SessionRef } from './api';
  import { termFamily, xtermTheme } from './theme';
  import { look, termOptions } from './look.svelte';
  import { app, sessionKey, sessionWorktree, toast } from './store.svelte';
  import { registerTerminal } from './menus/surfaces';

  let { session, dark }: { session: SessionRef; dark: boolean } = $props();

  let host: HTMLDivElement;
  let term: Terminal | null = null;
  let fit: FitAddon | null = null;
  let attached: SessionRef | null = null;
  let replaying = false;
  let phase = $state<'booting' | 'live' | 'exited' | 'refused'>('booting');
  let note = $state('');
  let ro: ResizeObserver | null = null;
  let lastSize = { cols: 0, rows: 0 };
  /** Bumped per attach, so a late event from the previous session is dropped. */
  let generation = 0;
  /** The URL under the pointer (the web-links addon's hover). */
  let link: string | null = null;
  let unregister: (() => void) | undefined;

  function send(data: string, binary = false) {
    if (!attached || replaying) return;
    api.input(attached, data, binary).catch(() => {});
  }

  async function attach(s: SessionRef) {
    if (!term || !fit) return;
    if (attached) await api.detach(attached).catch(() => {});
    const gen = ++generation;
    attached = s;
    shownKey = sessionKey(s);
    phase = 'booting';
    note = '';
    term.reset();
    // A hidden pane has no size to fit to; it fits once shown.
    if (host.offsetWidth) fit.fit();
    lastSize = { cols: term.cols, rows: term.rows };
    const ch = new Channel<PtyEvent>();
    ch.onmessage = (ev) => {
      if (gen !== generation || !term) return;
      switch (ev.kind) {
        case 'scrollback':
          // The ring's replay can hold the CLI's old terminal queries; the
          // daemon already answered them, so the pane stays quiet until
          // the replay has been parsed.
          replaying = true;
          term.reset();
          term.write(decodeB64(ev.data), () => {
            replaying = false;
          });
          phase = 'live';
          break;
        case 'output':
          term.write(decodeB64(ev.data));
          phase = 'live';
          break;
        case 'exited':
          phase = 'exited';
          note = ev.code == null ? 'Session ended.' : `Session ended with code ${ev.code}.`;
          break;
        case 'refused':
          phase = 'refused';
          note = ev.message;
          break;
      }
    };
    try {
      await api.attach(s, term.cols, term.rows, ch);
      term.focus();
    } catch (e) {
      phase = 'refused';
      note = String(e);
    }
  }

  // Another client on the same session (the terminal app, the review
  // sheet) can resize its PTY meanwhile. When this pane is shown again it
  // asserts its own size, even when that is unchanged here.
  let hidden = false;
  function refit(force = false) {
    if (!term || !fit || !host.offsetWidth) {
      hidden = true;
      return;
    }
    if (hidden) force = true;
    hidden = false;
    fit.fit();
    if (attached && (force || term.cols !== lastSize.cols || term.rows !== lastSize.rows)) {
      lastSize = { cols: term.cols, rows: term.rows };
      api.resize(attached, term.cols, term.rows).catch(() => {});
    }
  }

  /** Load the terminal font's regular and bold faces at `size`. */
  function loadFont(family: string, size: number) {
    return Promise.all([document.fonts.load(`${size}px ${family}`), document.fonts.load(`bold ${size}px ${family}`)]).catch(() => {});
  }

  onMount(async () => {
    const font = termOptions();
    await loadFont(termFamily(), font.fontSize);
    term = new Terminal({
      fontFamily: termFamily(),
      fontSize: font.fontSize,
      lineHeight: font.lineHeight,
      letterSpacing: 0,
      cursorBlink: true,
      cursorStyle: 'bar',
      scrollback: 10000,
      allowProposedApi: true,
      theme: xtermTheme(),
      drawBoldTextInBrightColors: false,
      minimumContrastRatio: 1,
      macOptionIsMeta: true,
      rightClickSelectsWord: true,
    });
    fit = new FitAddon();
    term.loadAddon(fit);
    term.loadAddon(new Unicode11Addon());
    term.unicode.activeVersion = '11';
    term.loadAddon(
      new WebLinksAddon(
        (_e, uri) => {
          api.openUrl(uri).catch(() => {});
        },
        // The link under the pointer, for the menu's Open link.
        { hover: (_e, uri) => (link = uri), leave: () => (link = null) },
      ),
    );
    // The daemon answers DA1, DSR and the kitty keyboard query for every
    // session; a second answer from here would land in the CLI as input.
    term.parser.registerCsiHandler({ final: 'c' }, (p) => p.length === 0 || p[0] === 0);
    term.parser.registerCsiHandler({ final: 'n' }, (p) => p[0] === 5 || p[0] === 6);
    term.parser.registerCsiHandler({ prefix: '?', final: 'u' }, () => true);

    term.attachCustomKeyEventHandler((e) => {
      if (e.type !== 'keydown') return true;
      // App chords reach the window, not the CLI.
      if (e.ctrlKey && e.shiftKey && /^[BDLMNOPT]$/i.test(e.key)) return false;
      if (e.ctrlKey && !e.shiftKey && e.key === 'c' && term!.hasSelection()) {
        navigator.clipboard.writeText(term!.getSelection()).catch(() => {});
        term!.clearSelection();
        return false;
      }
      if (e.ctrlKey && e.shiftKey && e.key.toLowerCase() === 'c') {
        navigator.clipboard.writeText(term!.getSelection()).catch(() => {});
        return false;
      }
      // Ctrl+V pastes (the browser's paste event; bracketed when the CLI asked).
      if (e.ctrlKey && !e.altKey && e.key.toLowerCase() === 'v') return false;
      return true;
    });
    term.onData((d) => send(d));
    term.onBinary((d) => send(d, true));
    term.open(host);
    // Its right-click menu (menus/builtin.ts). Paste goes through
    // `term.paste`, the same path as Ctrl+V's paste event.
    unregister = registerTerminal(host, {
      live: () => phase === 'live' && !!attached,
      hasSelection: () => !!term?.hasSelection(),
      selection: () => term?.getSelection() ?? '',
      clearSelection: () => term?.clearSelection(),
      paste: (text) => term?.paste(text),
      selectAll: () => term?.selectAll(),
      clear: () => term?.clear(),
      focus: () => term?.focus(),
      folder: () => {
        const w = sessionWorktree(attached ?? session);
        return app.model.worktrees.find((x) => x.id === w)?.path ?? null;
      },
      link: () => link,
      openLink: (url) => void api.openUrl(url).catch((e) => toast(String(e))),
    });
    try {
      const gl = new WebglAddon();
      gl.onContextLoss(() => gl.dispose());
      term.loadAddon(gl);
    } catch {
      /* the DOM renderer is fine */
    }
    let pending: ReturnType<typeof setTimeout> | undefined;
    ro = new ResizeObserver(() => {
      clearTimeout(pending);
      pending = setTimeout(() => refit(), 40);
    });
    ro.observe(host);
    await attach(session);
  });

  // Follow the selected session.
  let shownKey = '';
  $effect(() => {
    const k = sessionKey(session);
    if (term && k !== shownKey) {
      shownKey = k;
      attach(session);
    }
  });

  // Re-read the palette and the font once the look has applied: a new
  // data-theme, an inline palette, a family or a size (`look.seq`).
  $effect(() => {
    void dark;
    void look.seq;
    requestAnimationFrame(async () => {
      if (!term) return;
      term.options.theme = xtermTheme();
      const family = termFamily();
      const font = termOptions();
      const o = term.options;
      if (o.fontFamily === family && o.fontSize === font.fontSize && o.lineHeight === font.lineHeight) return;
      await loadFont(family, font.fontSize);
      if (!term) return;
      term.options.fontFamily = family;
      term.options.fontSize = font.fontSize;
      term.options.lineHeight = font.lineHeight;
      refit(true);
    });
  });

  // Re-attach after the daemon connection comes back.
  let wasConnected = app.model.connected;
  $effect(() => {
    const c = app.model.connected;
    if (c && !wasConnected && term) attach(session);
    wasConnected = c;
  });

  onDestroy(() => {
    unregister?.();
    ro?.disconnect();
    generation++;
    if (attached) api.detach(attached).catch(() => {});
    term?.dispose();
  });
</script>

<div class="relative h-full min-h-0 bg-term-bg">
  <div class="absolute inset-[8px_4px_4px_12px] [&_.xterm]:h-full [&_.xterm-viewport]:bg-transparent!" bind:this={host}></div>
  {#if phase === 'booting'}
    <div class="absolute top-3.5 right-[18px] flex items-center gap-2 font-mono text-[12px] text-muted">
      <span class="size-1.5 bg-running"></span>Attaching&hellip;
    </div>
  {:else if phase === 'exited' || phase === 'refused'}
    <div
      class="absolute bottom-4 left-1/2 max-w-[80%] -translate-x-1/2 border bg-surface px-3 py-1.5 text-[13px]
        {phase === 'refused' ? 'border-fail text-fail' : 'border-rule text-ink'}"
    >
      {note}
    </div>
  {/if}
</div>
