// The overlay's runtime: one World, ticked by requestAnimationFrame only
// while something is on screen and by one setTimeout between visits, the
// canvas it paints, and a transparent hit box per critter (the only part
// of the overlay that takes the mouse). It pauses while the window is
// hidden or unfocused. Svelte-free: Critters.svelte owns the elements and
// the app state, this owns the clock.

import { FLAGS, SPECIES } from './cast.gen';
import { paint } from './paint';
import { World, type AppEvent, type Input, type Note, type Pointer, type Rect } from './world';

/** CSS px per sprite px, before devicePixelRatio. */
export const SIZES = { small: 2, medium: 3, large: 4 } as const;
export type Size = keyof typeof SIZES;

/** The floor when no bar is marked `data-critter-floor`: the bottom bar's
 *  height above the window's bottom edge. */
const FLOOR_FALLBACK = 40;
/** Movement (CSS px) that turns a press into a pick-up. */
const GRAB_SLOP = 4;
/** Frames between layout reads (the floor, the perches) while animating. */
const MEASURE_EVERY = 30;

export interface Hooks {
  /** Visits, pets, feeds and names, for the collection log. */
  notes(notes: Note[]): void;
}

/** Device pixels per sprite pixel: a whole number, so pixels stay square. */
export function devicePerSprite(size: Size, dpr: number): number {
  return Math.max(1, Math.round(SIZES[size] * dpr));
}

export class Engine {
  world: World;
  size: Size = 'medium';
  reduced = false;
  allIdle = false;
  /** Tests (the e2e window may never have the OS focus). */
  ignoreFocus = false;
  /** Tests: the clock stopped, the world moved only by `step`. */
  frozen = false;
  /** Frames painted, timer wakes and ticks: the idle-cost check reads them. */
  readonly stats = { frames: 0, wakes: 0, ticks: 0 };

  private g: CanvasRenderingContext2D;
  private boxes = new Map<number, { el: HTMLDivElement; style: string }>();
  private raf: number | null = null;
  private timer: ReturnType<typeof setTimeout> | null = null;
  private last = 0;
  private started = false;
  private running = false;
  private tracking = false;
  private pointer: Pointer | null = null;
  private events: AppEvent[] = [];
  private css = 3;
  private floorCss = 0;
  private view = { w: 160, h: 60, floor: 56, perches: [] as Rect[] };
  private dirty = true;
  private drag: { id: number; pointer: number; x: number; y: number; grabbed: boolean } | null = null;
  /** The pointer (client px) while any hit box is up: a box takes events
   *  only over its critter's drawn pixels, never its transparent corners. */
  private hoverAt: { x: number; y: number } | null = null;
  private sensing = false;
  private dragged = false;

  constructor(
    private canvas: HTMLCanvasElement,
    private layer: HTMLElement,
    private hooks: Hooks,
  ) {
    this.g = canvas.getContext('2d')!;
    this.world = new World((Date.now() ^ (Math.random() * 0x100000000)) >>> 0);
  }

  // ---------- life ----------

  start() {
    if (this.started) return;
    this.started = true;
    window.addEventListener('resize', this.onResize);
    window.addEventListener('focus', this.onFocus);
    window.addEventListener('blur', this.onFocus);
    document.addEventListener('visibilitychange', this.onFocus);
    this.dirty = true;
    // Turned on: the first visitor comes in a few seconds, not after
    // whatever gap was left from before.
    if (!this.world.active() && this.world.spawning) this.world.resume();
    this.update();
  }

  /** Everything off: no listeners, no timers, nothing drawn. */
  stop() {
    if (!this.started) return;
    this.started = false;
    window.removeEventListener('resize', this.onResize);
    window.removeEventListener('focus', this.onFocus);
    window.removeEventListener('blur', this.onFocus);
    document.removeEventListener('visibilitychange', this.onFocus);
    this.update();
    this.world.clear();
    this.events = [];
    this.draw();
  }

  /** Runs while started and in view (or forced by a test). */
  update() {
    const want = this.started && !this.frozen && (this.ignoreFocus || (document.visibilityState === 'visible' && document.hasFocus()));
    if (want === this.running) return;
    this.running = want;
    if (want) {
      this.last = performance.now();
      this.dirty = true;
      this.schedule();
    } else {
      this.cancel();
      this.track(false);
      this.letGo();
    }
  }

  setSize(size: Size) {
    if (size === this.size) return;
    this.size = size;
    this.dirty = true;
    if (this.running) this.frameNow();
  }

  /** Something happened in the app. Dropped while paused: nobody saw it. */
  event(ev: AppEvent) {
    if (!this.running) return;
    this.events.push(ev);
    this.schedule();
  }

  // ---------- the host's commands ----------

  /** The view as the world sees it, measured now, with the app events
   *  waiting for the next tick. */
  input(): Input {
    return { ...this.view0(), events: this.events.splice(0) };
  }

  /** The view alone (no events): what a summon reads. */
  private view0(): Input {
    if (this.dirty) this.measure();
    return {
      viewW: this.view.w,
      viewH: this.view.h,
      floor: this.view.floor,
      perches: this.view.perches,
      pointer: this.pointer,
      hour: new Date().getHours(),
      reducedMotion: this.reduced,
      allIdle: this.allIdle,
      events: [],
    };
  }

  /** Brings a species in now (discovery, the test hook). */
  summon(id: string): number | null {
    return this.world.summon(id, this.view0());
  }

  /** Brings a species in at a point on screen (CSS px), hopping out of it
   *  and falling: the kiwi climbing out of the kete mark's basket. */
  summonAt(id: string, x: number, y: number): number | null {
    const inp = this.view0();
    const p = this.toSprite(x, y);
    return this.world.summonAt(id, p.x, Math.max(0, inp.floor - p.y), inp);
  }

  /** CSS px to sprite px. */
  toSprite(x: number, y: number): Pointer {
    return { x: x / this.css, y: y / this.css };
  }

  /** Something done to the world from outside (a command, a summon): draw
   *  it and get the clock going. */
  poke() {
    this.draw();
    this.schedule();
  }

  // ---------- the clock ----------

  private cancel() {
    if (this.raf !== null) cancelAnimationFrame(this.raf);
    if (this.timer !== null) clearTimeout(this.timer);
    this.raf = null;
    this.timer = null;
  }

  private schedule() {
    this.cancel();
    if (!this.running) return;
    if (this.world.active() || this.events.length) {
      this.track(true);
      this.raf = requestAnimationFrame(this.frame);
      return;
    }
    // Between visits: one timer to the next visitor, nothing else.
    this.track(false);
    const wake = this.world.nextWake();
    if (wake !== null) this.timer = setTimeout(this.wake, Math.ceil(wake * 1000) + 20);
  }

  private frame = () => {
    this.raf = null;
    this.stats.frames++;
    if (this.stats.frames % MEASURE_EVERY === 0) this.dirty = true;
    this.tick();
    this.schedule();
  };

  private wake = () => {
    this.timer = null;
    this.stats.wakes++;
    this.dirty = true;
    this.tick();
    this.schedule();
  };

  /** A tick now (a resize, a size change): no waiting for the next frame. */
  private frameNow() {
    this.tick();
    this.schedule();
  }

  private tick() {
    const now = performance.now();
    // Wall time since the last tick, uncapped: a wake after a long gap
    // must reach the visitor (the world caps time while anyone is here).
    const dt = Math.max((now - this.last) / 1000, 0);
    this.last = now;
    this.stats.ticks++;
    const steps = this.world.steps;
    const hover = this.world.hover;
    this.world.tick(dt, this.input());
    const notes = this.world.drainNotes();
    if (notes.length) this.hooks.notes(notes);
    // A frame between two steps changes nothing on screen.
    if (this.world.steps !== steps || this.world.hover !== hover || this.dirty) this.draw();
  }

  /** Steps the world by `secs` at once (tests). */
  step(secs: number) {
    this.world.tick(secs, this.input());
    const notes = this.world.drainNotes();
    if (notes.length) this.hooks.notes(notes);
    this.last = performance.now();
    this.poke();
  }

  private draw() {
    if (this.dirty) this.measure();
    paint(this.g, this.world);
    this.syncBoxes();
  }

  // ---------- layout ----------

  private onResize = () => {
    this.dirty = true;
    if (this.running && this.world.active()) this.frameNow();
  };

  private onFocus = () => this.update();

  /** The floor (the top of the bar marked `data-critter-floor`), the view
   *  in sprite px, and the perches (`data-perch` elements in view). */
  private measure() {
    this.dirty = false;
    const dpr = window.devicePixelRatio || 1;
    this.css = devicePerSprite(this.size, dpr) / dpr;
    const vw = window.innerWidth;
    const vh = window.innerHeight;
    // The highest marked bar in the bottom of the window: the verb bar
    // when the list shows one, else the quick prompt (there is no status
    // bar), else a bar's height above the bottom.
    let floorCss = Infinity;
    for (const el of document.querySelectorAll<HTMLElement>('[data-critter-floor]')) {
      const r = el.getBoundingClientRect();
      if (r.width > 0 && r.height > 0 && r.top > vh * 0.3) floorCss = Math.min(floorCss, r.top);
    }
    if (!Number.isFinite(floorCss)) floorCss = vh - FLOOR_FALLBACK;
    this.floorCss = floorCss;
    const w = Math.ceil(vw / this.css);
    const h = Math.ceil(vh / this.css);
    const floor = Math.max(16, Math.min(h, Math.floor(floorCss / this.css)));
    const perches: Rect[] = [];
    for (const el of document.querySelectorAll<HTMLElement>('[data-perch]')) {
      const r = el.getBoundingClientRect();
      // The top edge, or the bottom one for `data-perch="bottom"` (a rule
      // under a header that sits too near the top of the window).
      const edge = el.dataset.perch === 'bottom' ? r.bottom : r.top;
      if (r.width < 24 || edge <= 0 || edge >= floorCss - 8) continue;
      // In view and on top: not scrolled away under another pane.
      const hit = document.elementFromPoint(r.left + r.width / 2, el.dataset.perch === 'bottom' ? edge - 1 : edge + 1);
      if (!hit || !(el.contains(hit) || hit.closest('[data-critter]'))) continue;
      perches.push({ x: r.left / this.css, y: Math.round(edge / this.css), w: r.width / this.css, h: r.height / this.css });
    }
    this.view = { w, h, floor, perches };
    if (this.canvas.width !== w || this.canvas.height !== floor) {
      this.canvas.width = w;
      this.canvas.height = floor;
      this.g.imageSmoothingEnabled = false;
    }
    const cw = `${w * this.css}px`;
    const ch = `${floor * this.css}px`;
    if (this.canvas.style.width !== cw) this.canvas.style.width = cw;
    if (this.canvas.style.height !== ch) this.canvas.style.height = ch;
  }

  // ---------- the mouse ----------

  /** The pointer over the window, for the brain (the curious come over, the
   *  shy keep off). Only while a critter is on screen. */
  private track(on: boolean) {
    if (on === this.tracking) return;
    this.tracking = on;
    const root = document.documentElement;
    if (on) {
      window.addEventListener('pointermove', this.onMove, { passive: true, capture: true });
      root.addEventListener('pointerleave', this.onLeave);
    } else {
      window.removeEventListener('pointermove', this.onMove, { capture: true });
      root.removeEventListener('pointerleave', this.onLeave);
      this.pointer = null;
    }
  }

  private onMove = (e: PointerEvent) => {
    this.pointer = this.toSprite(e.clientX, e.clientY);
  };

  private onLeave = () => {
    if (!this.drag) this.pointer = null;
  };

  /** A held critter is dropped (the window lost focus, the press ended
   *  somewhere else). */
  private letGo() {
    if (this.drag?.grabbed || this.world.held() !== null) this.world.release();
    this.drag = null;
  }

  private syncBoxes() {
    const seen = new Set<number>();
    const floor = this.view.floor;
    const css = this.css;
    for (const d of this.world.sprites()) {
      const { w, h } = SPECIES[d.species].sprite;
      const bottom = Math.min(d.top + h, floor);
      if (bottom <= d.top || d.alpha < 0.3) continue;
      seen.add(d.id);
      let box = this.boxes.get(d.id);
      if (!box) {
        box = { el: this.makeBox(d.id), style: '' };
        this.boxes.set(d.id, box);
        this.layer.append(box.el);
      }
      const style = `transform:translate(${d.left * css}px,${d.top * css}px);width:${w * css}px;height:${(bottom - d.top) * css}px`;
      if (style !== box.style) {
        box.style = style;
        box.el.style.transform = `translate(${d.left * css}px, ${d.top * css}px)`;
        box.el.style.width = `${w * css}px`;
        box.el.style.height = `${(bottom - d.top) * css}px`;
      }
      box.el.style.cursor = this.world.held() === d.id ? 'grabbing' : 'grab';
    }
    for (const [id, box] of this.boxes) {
      if (seen.has(id)) continue;
      box.el.remove();
      this.boxes.delete(id);
    }
    this.sense(this.boxes.size > 0);
    this.aim();
  }

  /** Follows the pointer while any hit box is up (whatever the clock is
   *  doing), so each box knows whether its critter is under it. */
  private sense(on: boolean) {
    if (on === this.sensing) return;
    this.sensing = on;
    const root = document.documentElement;
    if (on) {
      window.addEventListener('pointermove', this.onSense, { passive: true, capture: true });
      root.addEventListener('pointerleave', this.onSenseLeave);
    } else {
      window.removeEventListener('pointermove', this.onSense, { capture: true });
      root.removeEventListener('pointerleave', this.onSenseLeave);
      this.hoverAt = null;
    }
  }

  private onSense = (e: PointerEvent) => {
    this.hoverAt = { x: e.clientX, y: e.clientY };
    this.aim();
  };

  private onSenseLeave = () => {
    this.hoverAt = null;
    this.aim();
  };

  /** Only the box whose critter's drawn pixel is under the pointer (or the
   *  one being dragged) takes events: a transparent corner over a button
   *  leaves the button its click. */
  private aim() {
    const p = this.hoverAt ? this.toSprite(this.hoverAt.x, this.hoverAt.y) : null;
    const under = p ? this.world.hit(p.x, p.y) : null;
    for (const [id, box] of this.boxes) {
      const v = id === under || this.drag?.id === id ? 'auto' : 'none';
      if (box.el.style.pointerEvents !== v) box.el.style.pointerEvents = v;
    }
  }

  private makeBox(id: number): HTMLDivElement {
    const el = document.createElement('div');
    el.className = 'critter-box';
    // Over the canvas, which takes no events: only this box catches the mouse.
    el.style.cssText = 'position:absolute;left:0;top:0;pointer-events:none;touch-action:none;user-select:none;cursor:grab';
    el.dataset.critter = String(id);
    el.dataset.target = `critter:${id}`;
    el.setAttribute('aria-hidden', 'true');
    // Never the focus: the press is the critter's, the keys stay where they were.
    el.addEventListener('mousedown', (e) => e.preventDefault());
    el.addEventListener('pointerdown', (e) => this.down(e, id, el));
    el.addEventListener('pointermove', (e) => this.moveBox(e));
    el.addEventListener('pointerup', (e) => this.up(e));
    el.addEventListener('pointercancel', () => this.cancelDrag());
    el.addEventListener('lostpointercapture', () => this.cancelDrag());
    el.addEventListener('click', (e) => this.click(e, id));
    // A right-click is the window's menu router's (`critter:` target,
    // menus/builtin.ts), which lets go of any drag first (`cancelDrag`).
    return el;
  }

  private down(e: PointerEvent, id: number, el: HTMLElement) {
    e.preventDefault();
    if (e.button !== 0) return;
    this.dragged = false;
    this.drag = { id, pointer: e.pointerId, x: e.clientX, y: e.clientY, grabbed: false };
    this.pointer = this.toSprite(e.clientX, e.clientY);
    try {
      el.setPointerCapture(e.pointerId);
    } catch {
      // A synthetic pointer (tests) may have no capture to take.
    }
  }

  private moveBox(e: PointerEvent) {
    const d = this.drag;
    if (!d || e.pointerId !== d.pointer) return;
    this.pointer = this.toSprite(e.clientX, e.clientY);
    if (d.grabbed || Math.hypot(e.clientX - d.x, e.clientY - d.y) < GRAB_SLOP) return;
    const at = this.toSprite(d.x, d.y);
    d.grabbed = this.world.grab(d.id, at.x, at.y);
    if (d.grabbed) this.poke();
  }

  private up(e: PointerEvent) {
    const d = this.drag;
    if (!d || e.pointerId !== d.pointer) return;
    this.drag = null;
    if (d.grabbed) {
      this.pointer = this.toSprite(e.clientX, e.clientY);
      this.world.release();
      this.dragged = true;
      this.poke();
    }
  }

  cancelDrag() {
    if (!this.drag) return;
    const grabbed = this.drag.grabbed;
    this.drag = null;
    if (grabbed) {
      this.world.release();
      this.dragged = true;
      this.poke();
    }
  }

  /** A click pets; the second click of a double-click feeds instead of
   *  petting twice. A press that picked it up is no click. */
  private click(e: MouseEvent, id: number) {
    e.preventDefault();
    if (this.dragged) {
      this.dragged = false;
      return;
    }
    const c = this.world.get(id);
    if (!c) return;
    // A balloon isn't fed: its second click would stop it popping.
    if (e.detail === 2 && !(SPECIES[c.species].flags & FLAGS.inanimate)) this.world.feed(id);
    else if (e.detail <= 1) this.world.pet(id);
    this.poke();
  }

  /** The floor line in CSS px (where the kete-mark kiwi lands). */
  floorY(): number {
    if (this.dirty) this.measure();
    return this.floorCss;
  }

  /** The perches as last measured, in sprite px (tests). */
  perches(): Rect[] {
    if (this.dirty) this.measure();
    return this.view.perches;
  }

  /** How many hit boxes are up (tests). */
  boxCount(): number {
    return this.boxes.size;
  }

  destroy() {
    this.stop();
    this.sense(false);
    for (const b of this.boxes.values()) b.el.remove();
    this.boxes.clear();
  }
}
