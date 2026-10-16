<script lang="ts">
  // The critters overlay: a full-window canvas that never takes the mouse,
  // and a transparent hit box per critter (engine.ts) that does, so only a
  // critter catches a click. Mounted once in App.svelte, above the window
  // and the toast, under the scrims. Off (the default), with Animations
  // off, or between visits it costs nothing: no frame loop, no listeners
  // but the Konami key check, at most one timer.
  import { onMount, untrack } from 'svelte';
  import { app } from '../store.svelte';
  import { layerOpen } from '../ui/ui.svelte';
  import { Engine } from './engine';
  import { allIdle, changes, snap, type Change, type Snap } from './events';
  import { konami, typingTarget } from './konami';
  import { today } from './log';
  import { attachEngine, critterMenuAt, critters, discover, flushQueued, loadCritters, logNotes, sizeOf } from './state.svelte';
  import { parseOften, type AppEvent } from './world';

  let canvas: HTMLCanvasElement;
  let layer: HTMLDivElement;
  let engine = $state<Engine | null>(null);

  const enabled = $derived(critters.prefs.on && critters.prefs.animations);
  const motion = window.matchMedia('(prefers-reduced-motion: reduce)');
  let reduced = $state(motion.matches);
  /** Today, moved on at midnight while the critters are hidden for the day. */
  let day = $state(today());

  onMount(() => {
    const e = new Engine(canvas, layer, { notes: logNotes, menu: critterMenuAt });
    engine = e;
    attachEngine(e);
    loadCritters();
    const onMotion = (ev: MediaQueryListEvent) => (reduced = ev.matches);
    motion.addEventListener('change', onMotion);
    // Back in the window: the terminal app may have changed the setting or
    // hidden the critters for the day.
    const onFocus = () => {
      day = today();
      loadCritters();
    };
    window.addEventListener('focus', onFocus);
    return () => {
      motion.removeEventListener('change', onMotion);
      window.removeEventListener('focus', onFocus);
      attachEngine(null);
      e.destroy();
    };
  });

  $effect(() => {
    const e = engine;
    if (!e) return;
    if (enabled) {
      untrack(() => {
        e.start();
        flushQueued(e);
      });
    } else e.stop();
  });

  $effect(() => {
    const e = engine;
    if (!e) return;
    e.setSize(sizeOf(critters.prefs.size));
    e.world.often = parseOften(critters.prefs.often);
    e.reduced = reduced;
  });

  // Hidden for the day: everyone leaves and nobody comes until midnight.
  const hidden = $derived(critters.hiddenOn === day);
  $effect(() => {
    const e = engine;
    const h = hidden;
    if (!e || !enabled) return;
    untrack(() => {
      if (h && e.world.spawning) e.world.scatter();
      else if (!h && !e.world.spawning) e.world.resume();
      e.poke();
    });
  });
  $effect(() => {
    if (!hidden) return;
    const now = new Date();
    const midnight = new Date(now.getFullYear(), now.getMonth(), now.getDate() + 1, 0, 0, 5);
    const t = setTimeout(() => (day = today()), midnight.getTime() - now.getTime());
    return () => clearTimeout(t);
  });

  // App events: what changed between two daemon models, while on.
  let prev: Snap | null = null;
  $effect(() => {
    const m = app.model;
    const on = enabled;
    untrack(() => {
      if (!on || !engine) return void (prev = null);
      const next = snap(m);
      engine.allIdle = allIdle(m);
      for (const c of changes(prev, next)) engine.event(toEvent(c));
      prev = next;
    });
  });

  /** Where on screen a change happened (its card or band), in sprite px. */
  function toEvent(c: Change): AppEvent {
    const css = c.kind === 'prMerged' ? `[data-target="worktree:${CSS.escape(c.key)}"]` : `[data-target="agent:${CSS.escape(c.agent)}"]`;
    let x: number | null = null;
    for (const el of document.querySelectorAll(css)) {
      const r = el.getBoundingClientRect();
      if (r.width > 0 && r.height > 0) {
        x = engine!.toSprite(r.left + r.width / 2, 0).x;
        break;
      }
    }
    if (c.kind === 'prMerged') return { kind: 'prMerged', x };
    if (c.kind === 'needsYou') return { kind: 'needsYou', x };
    return { kind: 'turnDone' };
  }

  // The Konami code, anywhere but a text field: the key that completes it
  // is swallowed, so a row's `a` doesn't fire too.
  const code = konami();
  function onKey(e: KeyboardEvent) {
    if (e.repeat || layerOpen() || typingTarget(e.target)) return;
    if (!code(e.key)) return;
    e.preventDefault();
    e.stopPropagation();
    discover('konami');
  }

  // The e2e suite's hook, in debug builds only.
  $effect(() => {
    const e = engine;
    if (!e || !critters.prefs.debug) return;
    const w = window as unknown as Record<string, unknown>;
    w.__kete_critters = hook(e);
    return () => delete w.__kete_critters;
  });

  function hook(e: Engine) {
    return {
      /** Bring `species` in now; its id. */
      spawn: (species: string) => {
        const id = e.summon(species);
        e.poke();
        return id;
      },
      /** Tick on the window's own clock even without the OS focus. */
      ignoreFocus: (on = true) => {
        e.ignoreFocus = on;
        e.update();
      },
      /** Stop the clock (or start it again): the world moves only by `step`. */
      freeze: (on = true) => {
        e.frozen = on;
        e.update();
      },
      /** Everyone off screen at once, the next visitor in a few seconds. */
      clear: () => {
        e.world.clear();
        e.world.resume();
        e.poke();
      },
      /** Step the world `secs` at once. */
      step: (secs: number) => e.step(secs),
      list: () =>
        e.world.critters.map((c) => ({ id: c.id, species: c.species, name: c.name, entered: c.entered, mode: c.mode, pose: c.pose, x: c.x, lift: c.lift })),
      particles: () => e.world.particles.map((p) => (p.kind === 'icon' ? `icon:${p.icon}` : p.kind)),
      /** A point (client px) on a drawn pixel of critter `id`. */
      point: (id: number) => {
        const b = e.world.bounds(id);
        if (!b) return null;
        const [left, top, w, h] = b;
        const one = e.toSprite(1, 0).x;
        for (let y = top + Math.floor(h / 2); y < top + h; y++) {
          for (let dx = 0; dx < w; dx++) {
            const x = left + ((Math.floor(w / 2) + dx) % w);
            if (e.world.hit(x + 0.5, y + 0.5) === id) return { x: (x + 0.5) / one, y: (y + 0.5) / one };
          }
        }
        return null;
      },
      stats: () => ({ ...e.stats, active: e.world.active(), wake: e.world.nextWake(), boxes: e.boxCount(), held: e.world.held() }),
    };
  }
</script>

<svelte:window onkeydowncapture={onKey} />

<div class="pointer-events-none fixed inset-0 z-[35] overflow-hidden" class:hidden={!enabled} aria-hidden="true" data-critters>
  <canvas bind:this={canvas} class="absolute top-0 left-0 [image-rendering:pixelated]" width="1" height="1"></canvas>
  <div bind:this={layer} class="absolute inset-0"></div>
</div>
