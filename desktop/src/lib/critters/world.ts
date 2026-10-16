// The critters' world: a function-for-function port of kete-critters'
// `world.rs` (Rust), run against the generated cast. The TUI runs the Rust
// world; this one must replay a seeded Rust run exactly (cast.gen's
// GOLDEN), so the order of RNG draws, rounding and evaluation follow the
// Rust line for line. README.md has the rules ("Matching Rust exactly").
//
// Units are sprite pixels and seconds. `x` is a critter's centre; `lift`
// is its feet's height above the floor (negative while sinking into it).

import {
  CLOCK,
  FLAGS,
  ICONS,
  INK,
  MOODS,
  NAMES,
  NEXT,
  NEXT_TO,
  OFTEN,
  PERSONALITIES,
  PHYSICS,
  PREFIXES,
  RELATIONS,
  SPECIES,
  STATES,
  decodeFrame,
  type IconName,
  type Personality,
  type Pose,
  type Relation,
  type SpeciesRow,
  type State,
} from './cast.gen';
import { Rng } from './rng';

export const STEP = PHYSICS.STEP;
export const STEP_US = PHYSICS.STEP_US;
export const MAX_TICK_US = PHYSICS.MAX_TICK_US;
const GRAVITY = PHYSICS.GRAVITY;
const BOUNCE = PHYSICS.BOUNCE;
const FRICTION = PHYSICS.FRICTION;
const WALL_BOUNCE = PHYSICS.WALL_BOUNCE;
const SETTLE_SPEED = PHYSICS.SETTLE_SPEED;
const MAX_FLING = PHYSICS.MAX_FLING;
export const SQUASH_TIME = PHYSICS.SQUASH_TIME;
const BLINK_TIME = PHYSICS.BLINK_TIME;
const MAX_VISITORS = PHYSICS.MAX_VISITORS;
const ITEM_LIFE = PHYSICS.ITEM_LIFE;
const FLIGHT_ARC = PHYSICS.FLIGHT_ARC;
const LATE_SLEEP = PHYSICS.LATE_SLEEP;

// ---------- Rust's arithmetic ----------

/** Rust's `f64::round`: halves away from zero (`Math.round(-2.5)` is -2). */
export function rround(v: number): number {
  return Math.sign(v) * Math.round(Math.abs(v));
}

/** Rust's `fract`: `x - trunc(x)`. */
function fract(x: number): number {
  return x - Math.trunc(x);
}

/** Rust's `f64::clamp` (min <= max at every call site). */
function clamp(x: number, lo: number, hi: number): number {
  if (x < lo) return lo;
  if (x > hi) return hi;
  return x;
}

/** Seconds to whole steps (at least one). */
export function steps(secs: number): number {
  return Math.max(1, rround(secs / STEP));
}

/** An arch over u in [0, 1]: 0 at both ends, 1 in the middle. */
export function arch(u: number): number {
  return 4 * u * (1 - u);
}

/** A triangle wave of period 1 in [-1, 1]. */
export function wave(t: number): number {
  return 1 - 4 * Math.abs(Math.abs(fract(t)) - 0.5);
}

/** Six directions a puff of smoke goes, every 60 degrees from east. */
const RING: readonly (readonly [number, number])[] = [
  [1.0, 0.0],
  [0.5, 0.8660254037844386],
  [-0.5, 0.8660254037844386],
  [-1.0, 0.0],
  [-0.5, -0.8660254037844386],
  [0.5, -0.8660254037844386],
];

const between = (h: number, [from, until]: readonly [number, number]) => h >= from && h < until;
/** Night: from 20:00 until 06:00. */
export function isNight(hour: number): boolean {
  return !between(hour, [CLOCK.night[1], CLOCK.night[0]]);
}
/** Late: from 22:00 until 05:00. */
export function isLate(hour: number): boolean {
  return !between(hour, [CLOCK.late[1], CLOCK.late[0]]);
}

export type Often = keyof typeof OFTEN;
export function parseOften(s: string): Often {
  return s === 'rare' || s === 'busy' ? s : 'normal';
}

// ---------- the cast ----------

export function byId(id: string): number | undefined {
  const i = SPECIES.findIndex((s) => s.id === id);
  return i < 0 ? undefined : i;
}

function has(s: SpeciesRow, f: number): boolean {
  return (s.flags & f) !== 0;
}

/** The relation `a` has towards `b`, if any. */
export function relation(a: string, b: string): Relation | undefined {
  return RELATIONS.find(([x, , y]) => x === a && y === b)?.[1];
}

const spriteW = (species: number) => SPECIES[species].sprite.w;
const spriteH = (species: number) => SPECIES[species].sprite.h;

/** Decoded frames, built once per (species, pose, frame) for hit tests. */
const decoded = new Map<string, Uint8Array>();
export function framePixels(species: number, pose: Pose, f: number): Uint8Array {
  const frames = SPECIES[species].sprite.frames[pose];
  const i = f % frames.length;
  const key = `${species}/${pose}/${i}`;
  let px = decoded.get(key);
  if (!px) {
    px = decodeFrame(frames[i]);
    decoded.set(key, px);
  }
  return px;
}

// ---------- the world's types ----------

export interface Rect {
  x: number;
  y: number;
  w: number;
  h: number;
}
export interface Pointer {
  x: number;
  y: number;
}

/** Something that happened in the app, edge-triggered. */
export type AppEvent = { kind: 'needsYou'; x: number | null } | { kind: 'turnDone' } | { kind: 'prMerged'; x: number | null };

/** What the world needs from its host each tick, in sprite pixels. */
export interface Input {
  viewW: number;
  viewH: number;
  /** The floor line: critters' feet stand on it. */
  floor: number;
  /** Surfaces fliers may perch on: the top edge of each rect. */
  perches: Rect[];
  /** The pointer, when it's over the window. */
  pointer: Pointer | null;
  /** The local hour, 0-23. */
  hour: number;
  /** Hold poses, no particles, no walking on. */
  reducedMotion: boolean;
  /** Nothing is running. */
  allIdle: boolean;
  events: AppEvent[];
}

export function defaultInput(): Input {
  return { viewW: 160, viewH: 60, floor: 56, perches: [], pointer: null, hour: 12, reducedMotion: false, allIdle: false, events: [] };
}

/** What the host might want to remember (the collection log). */
export type Note =
  | { kind: 'seen'; species: string }
  | { kind: 'petted'; species: string }
  | { kind: 'fed'; species: string }
  | { kind: 'named'; species: string; name: string }
  | { kind: 'lamp' };

export interface Bubble {
  text: string;
  /** Seconds left. */
  t: number;
}

export type Mode = 'brain' | 'held' | 'air';

interface Hold {
  dx: number;
  dy: number;
  vx: number;
  vy: number;
  last: [number, number] | null;
}

type Mark = 'entered' | 'gone' | 'meetingOver' | 'exiting';

/** One step of a critter's plan. Each runs until it says it's done. Every
 *  act is a fresh object: two queue slots never share one. */
type Act =
  | { k: 'pose'; pose: Pose; t: number }
  | { k: 'turn' }
  | { k: 'face'; d: number }
  | { k: 'faceX'; x: number }
  | { k: 'fade'; to: number; t: number; from: number | null; e: number }
  | { k: 'walkTo'; x: number; pace: number }
  | { k: 'stairs'; from: number; to: number; fadeIn: boolean; dist: number }
  | { k: 'flyTo'; x: number; lift: number; perch: number | null; from: [number, number] | null; e: number; total: number }
  | { k: 'eat'; t: number }
  | { k: 'sleep'; t: number; zt: number }
  | { k: 'emote'; icon: IconName }
  | { k: 'say'; text: string }
  | { k: 'hops'; n: number; e: number; h: number }
  | { k: 'pushups'; n: number; e: number }
  | { k: 'wait'; t: number }
  | { k: 'pop'; text: string; color: string }
  | { k: 'dropItem' }
  | { k: 'puff' }
  | { k: 'spores' }
  | { k: 'sink'; to: number }
  | { k: 'jump'; x: number }
  | { k: 'flash'; t: number }
  | { k: 'burst' }
  | { k: 'pinch' }
  | { k: 'carry'; item: number | null; to: number }
  | { k: 'throw'; target: number }
  | { k: 'popSelf' }
  | { k: 'merge'; with: number }
  | { k: 'offer'; t: number }
  | { k: 'breathe' }
  | { k: 'mark'; m: Mark };

// Act builders (fresh objects each call).
const pose = (p: Pose, t: number): Act => ({ k: 'pose', pose: p, t });
const idle = (t: number): Act => ({ k: 'pose', pose: 'idle', t });
const wait = (t: number): Act => ({ k: 'wait', t });
const emoteAct = (icon: IconName): Act => ({ k: 'emote', icon });
const say = (text: string): Act => ({ k: 'say', text });
const walkTo = (x: number, pace: number): Act => ({ k: 'walkTo', x, pace });
const flyTo = (x: number, lift: number, perch: number | null): Act => ({ k: 'flyTo', x, lift, perch, from: null, e: 0, total: 0 });
const fade = (to: number, t: number): Act => ({ k: 'fade', to, t, from: null, e: 0 });
const hops = (n: number, h: number): Act => ({ k: 'hops', n, e: 0, h });
const mark = (m: Mark): Act => ({ k: 'mark', m });

/** A visitor. */
export interface Critter {
  id: number;
  species: number;
  name: string;
  personality: Personality;
  x: number;
  lift: number;
  /** 1 faces right, -1 left. */
  dir: number;
  pose: Pose;
  frame: number;
  alpha: number;
  blink: boolean;
  flash: boolean;
  bubble: Bubble | null;
  mode: Mode;
  entered: boolean;
  exiting: boolean;
  gone: boolean;
  vx: number;
  vlift: number;
  /** The perch top (screen y) it's standing on, if perched. */
  perch: number | null;
  hold: Hold | null;
  anim: number;
  age: number;
  budget: number;
  blinkT: number;
  squashT: number;
  meeting: boolean;
  prev: State;
  acts: Act[];
  cur: Act | null;
  trailD: number;
  /** Lamp offered and not yet taken. */
  offering: boolean;
}

export function speciesOf(c: Critter): SpeciesRow {
  return SPECIES[c.species];
}
const sp = speciesOf;
const halfW = (c: Critter) => spriteW(c.species) / 2;
const height = (c: Critter) => spriteH(c.species);
function grounded(c: Critter): boolean {
  return c.mode === 'brain' && c.perch === null && Math.abs(c.lift) < 0.5;
}
/** The head's position (x, rise above the floor). */
export function head(c: Critter): [number, number] {
  const h = height(c);
  return [c.x + c.dir * spriteW(c.species) * 0.2, c.lift + h - 1];
}

export type Kind = 'dust' | 'trail' | 'sparkle' | 'crumb' | 'confetti' | 'smoke' | 'icon' | 'text' | 'item' | 'beam' | 'dart';

export interface Particle {
  kind: Kind;
  /** The icon, for kind `icon`. */
  icon: IconName | null;
  x: number;
  rise: number;
  vx: number;
  vrise: number;
  gravity: number;
  life: number;
  max: number;
  color: string;
  text: string;
  carriedBy: number | null;
}

function particle(kind: Kind, x: number, rise: number, life: number, color: string, icon: IconName | null = null): Particle {
  return { kind, icon, x, rise, vx: 0, vrise: 0, gravity: 0, life, max: life, color, text: '', carriedBy: null };
}

/** Opacity from the remaining life: full until the last third. */
export function particleAlpha(p: Particle): number {
  return clamp((p.life / p.max) * 3, 0, 1);
}

type Effect = { k: 'plan'; id: number; acts: Act[] } | { k: 'merge'; a: number; b: number } | { k: 'split'; id: number } | { k: 'monkey'; id: number };

interface Ctx {
  dt: number;
  viewW: number;
  viewH: number;
  floor: number;
  perches: Rect[];
  pointer: Pointer | null;
  reduced: boolean;
  /** (id, species, x, lift) of everyone, before this step. */
  others: [number, number, number, number][];
}

/** One sprite to draw (README.md, Art). */
export interface SpriteDraw {
  id: number;
  species: number;
  pose: Pose;
  frame: number;
  closed: boolean;
  flip: boolean;
  left: number;
  top: number;
  alpha: number;
  flash: boolean;
  clip: number;
}

const iconColor = (i: IconName) => ICONS[i].color;

// ---------- the world ----------

export class World {
  critters: Critter[] = [];
  particles: Particle[] = [];
  rng: Rng;
  /** Steps simulated so far. */
  steps = 0;
  often: Often = 'normal';
  /** Visitors come on their own; off after "hide critters until tomorrow". */
  spawning = true;
  /** The critter under the pointer (its name shows). */
  hover: number | null = null;
  viewW = defaultInput().viewW;
  viewH = defaultInput().viewH;
  floor = defaultInput().floor;
  /** Wall time not yet stepped, in microseconds. */
  accUs = 0;
  /** Steps until the next visitor, counted while nobody is on screen. */
  spawnIn: number;
  secondIn: number | null = null;
  /** Steps until two visitors may meet. */
  meetIn = steps(10);
  pending: AppEvent[] = [];
  nextId = 1;
  lastSpecies: number | null = null;
  /** Names given by the user, by species id. */
  names = new Map<string, string>();
  notes: Note[] = [];
  heldId: number | null = null;
  reduced = false;

  constructor(seed: number) {
    this.rng = new Rng(seed);
    // The first visitor comes soon after the critters are turned on.
    this.spawnIn = steps(this.rng.range(3, 8));
  }

  /** Anything to animate: a critter or a particle on screen. */
  active(): boolean {
    return this.critters.length > 0 || this.particles.length > 0;
  }

  /** Seconds until the world next needs a tick when nothing is on screen,
   *  or null while it's active or idle for good (spawning off). */
  nextWake(): number | null {
    if (this.active() || !this.spawning) return null;
    const due = this.spawnIn * STEP - this.accUs / 1e6;
    return Math.max(due, 0);
  }

  drainNotes(): Note[] {
    const n = this.notes;
    this.notes = [];
    return n;
  }

  get(id: number): Critter | undefined {
    return this.critters.find((c) => c.id === id);
  }

  /** Simulated seconds. */
  clock(): number {
    return this.steps * STEP;
  }

  /** Advances the world by `dt` seconds of wall time. */
  tick(dt: number, input: Input) {
    this.viewW = Math.max(input.viewW, 1);
    this.viewH = Math.max(input.viewH, 1);
    this.floor = input.floor;
    this.reduced = input.reducedMotion;
    this.pending.push(...input.events);
    this.accUs += rround(Math.max(dt, 0) * 1e6);
    // Nobody here: the steps up to the next visitor change nothing but the
    // clock, so take them in one go.
    if (!this.active() && this.pending.length === 0) {
      const have = Math.floor(this.accUs / STEP_US);
      const skip = this.spawning ? Math.min(have, Math.max(0, this.spawnIn - 1)) : have;
      this.steps += skip;
      if (this.spawning) this.spawnIn -= skip;
      this.accUs -= skip * STEP_US;
    }
    this.accUs = Math.min(this.accUs, MAX_TICK_US);
    while (this.accUs >= STEP_US) {
      this.accUs -= STEP_US;
      this.step(input);
    }
    this.hover = input.pointer ? this.hit(input.pointer.x, input.pointer.y) : null;
  }

  private step(input: Input) {
    const dt = STEP;
    this.steps += 1;
    const pending = this.pending;
    this.pending = [];
    for (const ev of pending) this.appEvent(ev, input);
    this.stage(input);

    const others: [number, number, number, number][] = this.critters.map((c) => [c.id, c.species, c.x, c.lift]);
    const ctx: Ctx = {
      dt,
      viewW: this.viewW,
      viewH: this.viewH,
      floor: this.floor,
      perches: input.perches,
      pointer: input.pointer,
      reduced: input.reducedMotion,
      others,
    };
    const effects: Effect[] = [];
    const late = input.allIdle && isLate(input.hour);
    for (const c of this.critters) update(c, ctx, late, this.rng, this.particles, effects);
    for (const e of effects) this.apply(e);
    if (this.heldId !== null) {
      const h = this.get(this.heldId);
      if (!h || h.mode !== 'held') this.heldId = null;
    }
    this.critters = this.critters.filter((c) => !c.gone);
    this.particles = tickParticles(this.particles, this.critters, dt, input.reducedMotion);
  }

  private apply(e: Effect) {
    switch (e.k) {
      case 'plan': {
        const c = this.get(e.id);
        if (c && c.mode === 'brain') {
          c.cur = null;
          c.acts = e.acts;
        }
        return;
      }
      case 'merge': {
        const ca = this.get(e.a);
        const cb = this.get(e.b);
        if (!ca || !cb) return;
        if (ca.gone || cb.gone) return;
        ca.gone = true;
        cb.gone = true;
        const slime = byId('slime');
        if (slime !== undefined) {
          const x = (ca.x + cb.x) / 2;
          const id = this.add(slime, x, ca.dir);
          const c = this.get(id)!;
          c.entered = true;
          c.squashT = SQUASH_TIME * 2;
          c.acts = [emoteAct('heart'), pose('idle', 1.0)];
          this.burst(x, 4, 6, false);
        }
        return;
      }
      case 'split': {
        const c = this.get(e.id);
        if (!c) return;
        c.gone = true;
        const mini = byId('slime_mini');
        if (mini !== undefined) {
          for (const side of [-1, 1]) {
            const nid = this.add(mini, c.x + side * 5, side);
            const m = this.get(nid)!;
            m.entered = true;
            m.mode = 'air';
            m.vx = side * 40;
            m.vlift = 70;
            m.budget = Math.max(c.budget, 30);
          }
        }
        this.burst(c.x, 4, 5, false);
        return;
      }
      case 'monkey':
        this.monkey(e.id);
        return;
    }
  }

  /** The made-in-NZ nod: a dart monkey runs in under the balloon, throws,
   *  the balloon pops, the monkey leaves pleased with itself. */
  private monkey(balloon: number) {
    const b = this.get(balloon);
    const m = byId('dart_monkey');
    if (!b || m === undefined) return;
    const w = this.viewW;
    const side = b.x < w / 2 ? -1 : 1;
    const hw = spriteW(m) / 2;
    const id = this.add(m, side < 0 ? -hw : w + hw, -side);
    const stand = clamp(b.x + side * 10, hw, w - hw);
    const acts: Act[] = [
      walkTo(stand, 1.0),
      { k: 'faceX', x: b.x },
      wait(0.3),
      { k: 'throw', target: balloon },
      wait(0.7),
      say('pop!'),
      emoteAct('spark'),
      pose('idle', 0.8),
      mark('entered'),
    ];
    const c = this.get(id)!;
    acts.push(...exitPlan({ ...c, x: stand }, w, this.rng, 1.2, false));
    c.exiting = true;
    c.acts = acts;
  }

  private burst(x: number, rise: number, n: number, confetti: boolean) {
    for (let i = 0; i < n; i++) {
      const p = particle(confetti ? 'confetti' : 'dust', x, rise, confetti ? 1.6 : 0.5, confetti ? INK.confetti[i % INK.confetti.length] : INK.dust);
      p.vx = this.rng.range(-40, 40);
      p.vrise = this.rng.range(20, confetti ? 90 : 40);
      p.gravity = confetti ? 120 : 80;
      this.particles.push(p);
    }
  }

  // ---------- spawning and meetings ----------

  /** Night/morning-aware weights for picking a visitor. */
  spawnWeights(hour: number, exclude: readonly number[]): number[] {
    const night = isNight(hour);
    const morning = between(hour, CLOCK.morning);
    return SPECIES.map((s, i) => {
      if (exclude.includes(i) || has(s, FLAGS.noSpawn)) return 0;
      if (has(s, FLAGS.nocturnal) && !night) return 0;
      let w = s.weight;
      if (has(s, FLAGS.nightBias) && night) w *= 2;
      if (has(s, FLAGS.morning)) w *= morning ? 3 : 0.6;
      return w;
    });
  }

  private pickSpecies(hour: number): number {
    const exclude = this.critters.map((c) => c.species);
    if (this.lastSpecies !== null) exclude.push(this.lastSpecies);
    return this.rng.weighted(this.spawnWeights(hour, exclude));
  }

  private visitors(): number {
    return this.critters.filter((c) => !has(sp(c), FLAGS.noSpawn)).length;
  }

  private stage(input: Input) {
    if (this.spawning) {
      if (this.critters.length === 0) {
        this.spawnIn = Math.max(0, this.spawnIn - 1);
        if (this.spawnIn === 0) {
          const s = this.pickSpecies(input.hour);
          this.visit(s, null, input);
          this.secondIn = this.rng.chance(OFTEN[this.often].second) ? steps(this.rng.range(5, 25)) : null;
          const [a, b] = OFTEN[this.often].gap;
          this.spawnIn = steps(this.rng.range(a, b));
        }
      } else if (this.secondIn !== null) {
        const t = this.secondIn;
        const v = this.visitors();
        if (v >= 1 && v < MAX_VISITORS) {
          if (t <= 1) {
            this.secondIn = null;
            const first = this.critters[0];
            if (!first.exiting) {
              const side = first.x < this.viewW / 2 ? 1 : -1;
              const s = this.pickSpecies(input.hour);
              this.visit(s, side, input);
            }
          } else {
            this.secondIn = t - 1;
          }
        }
      }
    }
    if (this.critters.length >= 2) {
      this.meetIn = Math.max(0, this.meetIn - 1);
      if (this.meetIn === 0) this.meet(input);
    }
  }

  /** Adds a critter at `x` on the floor with no plan. Returns its id. */
  private add(species: number, x: number, dir: number): number {
    const id = this.nextId;
    this.nextId += 1;
    const s = SPECIES[species];
    let name = this.names.get(s.id);
    if (name === undefined) {
      const n = NAMES[this.rng.index(NAMES.length)];
      name = this.rng.chance(0.25) ? `${PREFIXES[this.rng.index(PREFIXES.length)]} ${n}` : n;
    }
    const personality = PERSONALITIES[this.rng.weighted(s.bias)];
    const budget = this.rng.range(40, 110) * s.stay;
    const blinkT = this.rng.range(2, 5);
    this.critters.push({
      id,
      species,
      name,
      personality,
      x,
      lift: 0,
      dir,
      pose: 'idle',
      frame: 0,
      alpha: 1,
      blink: false,
      flash: false,
      bubble: null,
      mode: 'brain',
      entered: false,
      exiting: false,
      gone: false,
      vx: 0,
      vlift: 0,
      perch: null,
      hold: null,
      anim: 0,
      age: 0,
      budget,
      blinkT,
      squashT: 0,
      meeting: false,
      prev: 'start',
      acts: [],
      cur: null,
      trailD: 0,
      offering: false,
    });
    return id;
  }

  /** A visitor arrives: from `side` (-1 left, 1 right) or a random one, by
   *  the species' way of arriving. */
  private visit(species: number, sideIn: number | null, input: Input): number {
    const side = sideIn ?? this.rng.sign();
    const s = SPECIES[species];
    const w = this.viewW;
    const hw = spriteW(species) / 2;
    const dir = -side;
    const id = this.add(species, 0, dir);
    this.lastSpecies = species;
    this.notes.push({ kind: 'seen', species: s.id });
    const edgeIn = side < 0 ? hw + 2 : w - hw - 2;
    const off = side < 0 ? -hw : w + hw;
    const into = (lo: number, hi: number) => off + dir * this.rng.range(lo, hi) * w;
    const acts: Act[] = [];
    let x: number, lift: number, alpha: number, mode: Mode;
    if (input.reducedMotion) {
      // No walking on: appear somewhere and hold still.
      x = this.rng.range(0.2, 0.8) * w;
      [lift, alpha, mode] = [0, 1, 'brain'];
    } else if (has(s, FLAGS.ghostly)) {
      x = this.rng.range(0.15, 0.85) * w;
      [lift, alpha, mode] = [0, 0, 'brain'];
      acts.push(fade(1.0, 1.5));
    } else if (has(s, FLAGS.dropsIn)) {
      x = this.rng.range(0.2, 0.8) * w;
      [lift, alpha, mode] = [Math.max(input.floor - 2, 10), 1, 'air'];
    } else if (s.special === 'dig') {
      x = this.rng.range(0.15, 0.85) * w;
      [lift, alpha, mode] = [-spriteH(species), 1, 'brain'];
      acts.push({ k: 'sink', to: 0 });
    } else if (s.special === 'teleport' && s.gait !== 'float') {
      x = this.rng.range(0.15, 0.85) * w;
      [lift, alpha, mode] = [0, 1, 'brain'];
      acts.push({ k: 'puff' });
    } else if (has(s, FLAGS.flier) && (s.gait === 'fly' || this.rng.chance(0.34))) {
      x = off;
      const alt = this.rng.range(0.5, 0.9) * Math.max(input.floor, 8);
      [lift, alpha, mode] = [alt, 1, 'brain'];
      const tx = into(0.1, 0.3);
      acts.push(flyTo(tx, s.hover, null));
    } else if (s.gait === 'float' || this.rng.chance(0.5)) {
      x = off;
      [lift, alpha, mode] = [0, 1, 'brain'];
      const tx = into(0.08, 0.3);
      acts.push(walkTo(tx, 1.0));
    } else {
      // Up the stairs: rises through the floor line at the edge.
      x = edgeIn;
      const h = spriteH(species);
      [lift, alpha, mode] = [-h, 1, 'brain'];
      acts.push({ k: 'stairs', from: -h, to: 0, fadeIn: true, dist: 0 });
    }
    acts.push(mark('entered'));
    const c = this.get(id)!;
    c.x = x;
    c.lift = lift;
    c.alpha = alpha;
    c.mode = mode;
    c.acts = acts;
    return id;
  }

  /** Two visitors on screen may meet: their relation decides how, and
   *  strangers meet in the middle and emote. */
  private meet(input: Input) {
    const free: number[] = [];
    this.critters.forEach((c, i) => {
      if (c.entered && !c.exiting && !c.meeting && c.mode === 'brain' && c.perch === null && !has(sp(c), FLAGS.inanimate)) free.push(i);
    });
    if (free.length < 2 || input.reducedMotion) {
      this.meetIn = steps(2);
      return;
    }
    const [ia, ib] = [free[0], free[1]];
    const a = this.critters[ia];
    const b = this.critters[ib];
    const social = a.personality === 'social' || b.personality === 'social';
    this.meetIn = steps(this.rng.range(20, 40) * (social ? 0.6 : 1));
    const sa = sp(a).id;
    const sb = sp(b).id;
    const w = this.viewW;
    const [l, r] = a.x < b.x ? [ia, ib] : [ib, ia];
    const lc = this.critters[l];
    const rc = this.critters[r];
    const mid = clamp((lc.x + rc.x) / 2, 0.15 * w, 0.85 * w);
    const gap = (halfW(lc) + halfW(rc)) * 0.7 + 3;
    const lid = lc.id;
    const rid = rc.id;
    const rel = relation;
    const done = () => mark('meetingOver');
    // Who chases whom (either way round).
    const chase: [number, number, string, string] | null =
      rel(sa, sb) === 'chases' ? [a.id, b.id, sb, sa] : rel(sb, sa) === 'chases' ? [b.id, a.id, sa, sb] : null;
    let lActs: Act[], rActs: Act[];
    if (chase) {
      const [hunter, prey, preySp, hunterSp] = chase;
      const preyC = this.get(prey)!;
      const fleeX = preyC.x < w / 2 ? -halfW(preyC) * 2 : w + halfW(preyC) * 2;
      const afraid = rel(preySp, hunterSp) === 'fears';
      const hunt = [emoteAct('bang'), walkTo(preyC.x, 1.8), emoteAct('note'), pose('idle', 1.0), done()];
      const preyActs = afraid
        ? [wait(0.3), emoteAct('bang'), walkTo(fleeX, 2.2), mark('gone')]
        : [emoteAct('what'), hops(1, 4), done()];
      if (hunter === lid) [lActs, rActs] = [hunt, preyActs];
      else [lActs, rActs] = [preyActs, hunt];
      if (afraid) {
        const c = this.get(prey);
        if (c) c.exiting = true;
      }
    } else if (rel(sa, sb) === 'fears' || rel(sb, sa) === 'fears') {
      const scared = rel(sa, sb) === 'fears' ? a.id : b.id;
      const c = this.get(scared)!;
      const fleeX = c.x < w / 2 ? -halfW(c) * 2 : w + halfW(c) * 2;
      const run = [emoteAct('bang'), walkTo(fleeX, 2.2), mark('gone')];
      const calm = [emoteAct('what'), done()];
      if (scared === lid) [lActs, rActs] = [run, calm];
      else [lActs, rActs] = [calm, run];
      c.exiting = true;
    } else if (rel(sa, sb) === 'merges' || rel(sb, sa) === 'merges') {
      lActs = [walkTo(mid - 2, 1.2), { k: 'merge', with: rid }];
      rActs = [walkTo(mid + 2, 1.2), wait(5.0), done()];
    } else if (rel(sa, sb) === 'bristles' || rel(sb, sa) === 'bristles') {
      const back = (x: number, d: number) => clamp(x + d * 0.2 * w, 0.06 * w, 0.94 * w);
      lActs = [walkTo(mid - gap - 4, 1.0), { k: 'face', d: 1 }, emoteAct('anger'), pose('idle', 1.5), walkTo(back(mid, -1), 1.3), done()];
      rActs = [walkTo(mid + gap + 4, 1.0), { k: 'face', d: -1 }, wait(0.6), emoteAct('anger'), pose('idle', 0.9), walkTo(back(mid, 1), 1.3), done()];
    } else {
      const greets = rel(sa, sb) === 'greets' || rel(sb, sa) === 'greets';
      if (!greets && !this.rng.chance(social ? 0.9 : 0.7)) return;
      const e1: IconName = greets ? 'heart' : (['heart', 'note', 'spark', 'bang'] as const)[this.rng.index(4)];
      const e2: IconName = greets ? 'heart' : (['heart', 'note', 'what'] as const)[this.rng.index(3)];
      const sit1: Pose = this.rng.chance(0.5) ? 'sit' : 'idle';
      const sit2: Pose = this.rng.chance(0.5) ? 'sit' : 'idle';
      const t1 = this.rng.range(3, 7);
      const t2 = this.rng.range(3, 7);
      const rs = sp(this.critters[r]).lines;
      const rSays = rs.length === 0 ? emoteAct('note') : say(rs[this.rng.index(rs.length)]);
      lActs = [walkTo(mid - gap, 1.0), { k: 'face', d: 1 }, wait(0.4), emoteAct(e1), pose('idle', 2.2), emoteAct('heart'), pose(sit1, t1), done()];
      rActs = [walkTo(mid + gap, 1.0), { k: 'face', d: -1 }, wait(1.2), emoteAct(e2), pose('idle', 1.6), rSays, pose(sit2, t2), done()];
    }
    for (const [i, acts] of [
      [l, lActs],
      [r, rActs],
    ] as const) {
      const c = this.critters[i];
      c.meeting = true;
      c.cur = null;
      c.acts = acts;
    }
  }

  private appEvent(ev: AppEvent, input: Input) {
    switch (ev.kind) {
      case 'needsYou': {
        const at = ev.x ?? this.viewW / 2;
        let near: Critter | undefined;
        for (const c of this.critters) {
          if (!(c.entered && !c.gone && !has(sp(c), FLAGS.inanimate))) continue;
          // min_by keeps the first of equal minimums.
          if (!near || Math.abs(c.x - at) < Math.abs(near.x - at)) near = c;
        }
        if (near) {
          const c = near;
          if (grounded(c) && !c.meeting) {
            c.cur = null;
            c.acts.unshift(pose('idle', 1.2));
            c.acts.unshift(emoteAct('bang'));
            c.acts.unshift({ k: 'faceX', x: at });
          } else {
            const [hx, hr] = head(c);
            emote(this.particles, hx, hr, 'bang', input.reducedMotion);
          }
        }
        return;
      }
      case 'turnDone': {
        const adv = byId('adventurer');
        const a = this.critters.find((c) => c.species === adv && c.entered);
        if (a) {
          a.bubble = { text: 'gz', t: 2.6 };
        } else if (this.rng.chance(0.5)) {
          const c = this.critters.find((c) => c.entered && !has(sp(c), FLAGS.inanimate));
          if (c) {
            const [hx, hr] = head(c);
            emote(this.particles, hx, hr, 'note', input.reducedMotion);
          }
        }
        return;
      }
      case 'prMerged': {
        const x = ev.x ?? this.rng.range(0.2, 0.8) * this.viewW;
        if (!input.reducedMotion) {
          const beam = particle('beam', x, 0, 6, INK.beam);
          beam.text = 'loot';
          this.particles.push(beam);
        }
        const adv = byId('adventurer');
        const w = this.viewW;
        for (const c of this.critters) {
          if (!(c.entered && grounded(c))) continue;
          const [hx, hr] = head(c);
          emote(this.particles, hx, hr, 'bang', input.reducedMotion);
          if (c.species === adv && !c.meeting) {
            c.cur = null;
            c.acts = [walkTo(clamp(x - 6, 0.06 * w, 0.94 * w), 1.6), { k: 'faceX', x }, say('loot!'), pose('eat', 1.2), emoteAct('spark')];
          }
        }
        return;
      }
    }
  }

  // ---------- the host's commands (the critter menu, the mouse) ----------

  /** Brings a species in now (the debug hook, tests). */
  summon(speciesId: string, input: Input): number | null {
    const s = byId(speciesId);
    if (s === undefined) return null;
    this.viewW = Math.max(input.viewW, 1);
    this.viewH = Math.max(input.viewH, 1);
    this.floor = input.floor;
    return this.visit(s, null, input);
  }

  /** Desktop only (the TUI has no kete mark): brings a species in at `x`,
   *  `lift` above the floor, hopping out and falling (the kiwi climbing
   *  out of the basket). The arrival is `summon`'s, then it's moved. */
  summonAt(speciesId: string, x: number, lift: number, input: Input): number | null {
    const id = this.summon(speciesId, input);
    if (id === null) return null;
    const c = this.get(id)!;
    c.x = x;
    c.lift = lift;
    c.alpha = 1;
    c.cur = null;
    c.acts = [];
    c.mode = 'air';
    c.vx = 30;
    c.vlift = 60;
    return id;
  }

  /** Click: a heart and the species' reaction. */
  pet(id: number): boolean {
    const c = this.get(id);
    if (!c) return false;
    if (c.gone || c.mode !== 'brain') return false;
    const s = sp(c);
    const [hx, hr] = head(c);
    this.notes.push({ kind: 'petted', species: s.id });
    emote(this.particles, hx, hr, 'heart', false);
    let plan: Act[] | null = null;
    if (s.special === 'fuse') {
      plan = [say('sss...'), { k: 'flash', t: 1.4 }, { k: 'burst' }];
    } else if (s.special === 'lamp' && c.offering) {
      this.notes.push({ kind: 'lamp' });
      const p = particle('text', hx, hr + 6, 2.4, INK.gold);
      p.text = '+xp lamp';
      p.vrise = 14;
      this.particles.push(p);
      plan = [say('enjoy'), pose('idle', 1.0), fade(0.0, 1.2), mark('gone')];
    } else if (s.special === 'teleport') {
      const w = this.viewW;
      const to = this.rng.range(0.15, 0.85) * w;
      plan = [{ k: 'puff' }, { k: 'jump', x: to }, { k: 'puff' }, say('hehe')];
    } else if (s.special === 'dig') {
      const to = this.rng.range(0.15, 0.85) * this.viewW;
      const h = spriteH(c.species);
      plan = [{ k: 'sink', to: -h }, { k: 'jump', x: to }, { k: 'sink', to: 0 }];
    } else if (s.special === 'hide') {
      plan = [pose('sit', 3.0), pose('idle', 0.5)];
    }
    if (s.id === 'slime') {
      this.apply({ k: 'split', id });
      return true;
    }
    if (s.id === 'balloon') plan = [{ k: 'popSelf' }];
    if (!plan) {
      const p: Act[] = [pose('idle', 0.4)];
      if (s.lines.length > 0 && this.rng.chance(0.6)) p.push(say(s.lines[this.rng.index(s.lines.length)]));
      else if (s.gait === 'hop') p.push(hops(1, 4));
      p.push(pose('idle', 0.8));
      plan = p;
    }
    if (c.exiting || c.meeting) return true;
    c.cur = null;
    c.acts = plan;
    return true;
  }

  /** Double-click or Feed: crumbs fall in front of it and it eats. */
  feed(id: number): boolean {
    const c = this.get(id);
    if (!c) return false;
    if (c.gone || c.mode !== 'brain' || c.exiting) return false;
    const s = sp(c);
    this.notes.push({ kind: 'fed', species: s.id });
    const x = c.x + c.dir * (halfW(c) + 1);
    const top = height(c) + 6;
    for (let i = 0; i < 6; i++) {
      const p = particle('crumb', x + (i % 3) - 1, top + i, 2.5, INK.crumb);
      p.gravity = 60;
      p.vx = this.rng.range(-4, 4);
      this.particles.push(p);
    }
    const greedy = c.personality === 'greedy';
    const plan: Act[] = [wait(0.5), { k: 'eat', t: greedy ? 5 : 3 }, emoteAct('heart')];
    if (greedy) plan.push(say('nom'));
    plan.push(pose('idle', 0.8));
    c.cur = null;
    c.meeting = false;
    c.acts = plan;
    c.perch = null;
    return true;
  }

  /** Shoo: it leaves now. */
  shoo(id: number): boolean {
    const w = this.viewW;
    const c = this.get(id);
    if (!c) return false;
    if (c.gone || c.mode !== 'brain') return false;
    const acts: Act[] = [emoteAct('what')];
    acts.push(...exitPlan(c, w, this.rng, 1.8, this.reduced));
    c.cur = null;
    c.meeting = false;
    c.entered = true;
    c.acts = acts;
    c.exiting = true;
    return true;
  }

  /** Names a visitor; the species keeps the name for next time. */
  rename(id: number, nameIn: string): boolean {
    const name = Array.from(nameIn.trim()).slice(0, 16).join('');
    const c = this.get(id);
    if (!c) return false;
    if (!name) return false;
    c.name = name;
    const species = sp(c).id;
    this.names.set(species, name);
    this.notes.push({ kind: 'named', species, name });
    return true;
  }

  /** Everyone leaves (Hide critters until tomorrow); no new visitors until
   *  `spawning` is set again. */
  scatter() {
    this.spawning = false;
    for (const id of this.critters.map((c) => c.id)) {
      if (!this.shoo(id)) {
        const c = this.get(id);
        if (c) c.gone = true;
      }
    }
  }

  /** Takes everything off screen at once. */
  clear() {
    this.critters = [];
    this.particles = [];
    this.heldId = null;
  }

  /** Restarts the visit timer (spawning back on, the next visitor in a few
   *  seconds). */
  resume() {
    this.spawning = true;
    this.spawnIn = steps(this.rng.range(3, 8));
    this.accUs = 0;
  }

  /** Picks a critter up at the pointer. */
  grab(id: number, px: number, py: number): boolean {
    const floor = this.floor;
    const c = this.get(id);
    if (!c) return false;
    if (c.gone || c.offering) return false;
    const feetY = floor - c.lift;
    c.hold = { dx: c.x - px, dy: feetY - py, vx: 0, vy: 0, last: [px, py] };
    c.mode = 'held';
    c.perch = null;
    c.cur = null;
    c.meeting = false;
    c.acts = [];
    c.pose = 'held';
    c.bubble = null;
    this.heldId = id;
    return true;
  }

  /** The critter being held, if any. */
  held(): number | null {
    return this.heldId;
  }

  /** Lets go: it falls with the pointer's velocity (fliers fly off). */
  release() {
    const id = this.heldId;
    this.heldId = null;
    if (id === null) return;
    const w = this.viewW;
    const c = this.get(id);
    if (!c) return;
    const hold = c.hold ?? { dx: 0, dy: 0, vx: 0, vy: 0, last: null };
    c.hold = null;
    const vx = clamp(hold.vx, -MAX_FLING, MAX_FLING);
    const vy = clamp(hold.vy, -MAX_FLING, MAX_FLING);
    const s = sp(c);
    if (has(s, FLAGS.flier) || s.gait === 'float' || s.gait === 'fly') {
      c.mode = 'brain';
      const tx = clamp(c.x + vx * 0.4 + this.rng.sign() * this.rng.range(0.05, 0.2) * w, 0.08 * w, 0.92 * w);
      c.acts = [flyTo(tx, s.hover, null), emoteAct('note')];
    } else {
      c.mode = 'air';
      c.vx = vx;
      c.vlift = -vy;
    }
  }

  // ---------- queries ----------

  /** The draw list, back to front (the held critter last). */
  sprites(): SpriteDraw[] {
    const floor = this.floor;
    const out = this.critters.map((c): SpriteDraw => {
      const s = SPECIES[c.species];
      const ghost = has(s, FLAGS.ghostly) ? 0.8 : 1;
      const squash = c.squashT > 0;
      return {
        id: c.id,
        species: c.species,
        pose: squash ? 'squash' : c.pose,
        frame: c.frame,
        closed: c.blink || c.pose === 'sleep',
        flip: c.dir < 0,
        left: rround(c.x - s.sprite.w / 2),
        top: rround(floor - c.lift) - s.sprite.h,
        alpha: clamp(c.alpha * ghost, 0, 1),
        flash: c.flash,
        clip: rround(floor),
      };
    });
    // Held critters draw on top (a stable sort, as Rust's sort_by_key).
    const held = this.heldId;
    return out.sort((a, b) => Number(a.id === held) - Number(b.id === held));
  }

  /** The critter whose drawn pixels cover (x, y), topmost first. */
  hit(x: number, y: number): number | null {
    const px = Math.floor(x);
    const py = Math.floor(y);
    const list = this.sprites();
    for (let i = list.length - 1; i >= 0; i--) {
      const d = list[i];
      const { w, h } = SPECIES[d.species].sprite;
      let lx = px - d.left;
      const ly = py - d.top;
      if (lx < 0 || ly < 0 || lx >= w || ly >= h || py >= d.clip || d.alpha < 0.3) continue;
      if (d.flip) lx = w - 1 - lx;
      if (framePixels(d.species, d.pose, d.frame)[ly * w + lx] !== 0) return d.id;
    }
    return null;
  }

  /** A critter's drawn box (left, top, w, h): the desktop's hit box. */
  bounds(id: number): [number, number, number, number] | null {
    const d = this.sprites().find((d) => d.id === id);
    if (!d) return null;
    const { w, h } = SPECIES[d.species].sprite;
    return [d.left, d.top, w, h];
  }
}

// ---------- the brain ----------

function emote(particles: Particle[], x: number, rise: number, icon: IconName, reduced: boolean) {
  if (reduced) return;
  const p = particle('icon', x, rise + 4, 1.8, iconColor(icon), icon);
  p.vrise = 10;
  particles.push(p);
}

function dust(particles: Particle[], rng: Rng, x: number, rise: number, n: number, reduced: boolean) {
  if (reduced) return;
  for (let i = 0; i < n; i++) {
    const p = particle('dust', x, rise + 1, 0.5, INK.dust);
    p.vx = rng.range(-20, 20);
    p.vrise = rng.range(5, 20);
    p.gravity = 40;
    particles.push(p);
  }
}

const col = (st: State) => NEXT_TO.indexOf(st);

/** The weighted pick of what to do next, from the state just left. */
export function nextWeights(s: SpeciesRow, prev: State, mood: Personality, late: boolean): number[] {
  const r = STATES.indexOf(prev);
  const row = r < 0 ? 1 : r;
  const m = PERSONALITIES.indexOf(mood);
  const moodRow = MOODS[m < 0 ? 0 : m];
  let w = [...NEXT[row]];
  for (let i = 0; i < moodRow.length; i++) w[i] *= moodRow[i];
  if (s.lines.length === 0) w[col('say')] = 0;
  if (s.gait === 'slide') {
    w[col('hop')] = 0;
    w[col('walk')] += 2;
  }
  if (s.special === 'nothing') {
    // The pet rock: sits about. Beloved.
    w = [0, 3, 0, 0, 2, 1, 0, 0, 0, 0];
  }
  if (has(s, FLAGS.inanimate)) {
    // The balloon drifts and hangs about, and that's all.
    return [3, 2, 0, 0, 0, 0, 0, 0, 0, 0];
  }
  if (late) {
    w[col('sit')] *= LATE_SLEEP;
    w[col('sleep')] *= LATE_SLEEP;
    w[col('walk')] /= 3;
    // Sitting leads to sleep: give idle a way into it too.
    if (w[col('sit')] === 0) w[col('sit')] = 1;
  }
  return w;
}

/** The plan for the state picked next. */
function nextActs(c: Critter, ctx: Ctx, late: boolean, rng: Rng): Act[] {
  const s = sp(c);
  const w = ctx.viewW;
  if (ctx.reduced) {
    // Reduced motion: hold poses in place.
    const p = (['idle', 'sit', 'sleep'] as const)[rng.index(3)];
    return [{ k: 'pose', pose: p, t: rng.range(4, 10) }];
  }
  const weights = nextWeights(s, c.prev, c.personality, late);
  const k = NEXT_TO[rng.weighted(weights)];
  c.prev = k;
  const lo = 0.06 * w + halfW(c);
  const hi = Math.max(0.94 * w - halfW(c), lo);
  const walkTarget = (c: Critter, rng: Rng): number => {
    const p = ctx.pointer;
    if (p) {
      const near = Math.abs(p.x - c.x) < 40;
      if (c.personality === 'curious' && rng.chance(0.6)) return clamp(p.x - c.dir * 4, lo, hi);
      if (c.personality === 'shy' && near) {
        const away = p.x < c.x ? 1 : -1;
        return clamp(c.x + away * rng.range(0.15, 0.3) * w, lo, hi);
      }
    }
    if (c.personality === 'social' && rng.chance(0.5)) {
      const o = ctx.others.find((o) => o[0] !== c.id);
      if (o) {
        // Alongside, not on top of it.
        const gap = halfW(c) + spriteW(o[1]) / 2 + 2;
        const side = c.x < o[2] ? -1 : 1;
        return clamp(o[2] + side * gap, lo, hi);
      }
    }
    const max = (s.gait === 'slide' ? 0.12 : 0.35) * w;
    return clamp(c.x + rng.sign() * rng.range(0.05 * w, Math.max(max, 0.06 * w)), lo, hi);
  };
  switch (k) {
    case 'walk': {
      const pace = c.personality === 'shy' && ctx.pointer && Math.abs(ctx.pointer.x - c.x) < 40 ? 1.8 : 1.0;
      return [{ k: 'walkTo', x: walkTarget(c, rng), pace }];
    }
    case 'idle':
      return [{ k: 'pose', pose: 'idle', t: rng.range(2, 6) }];
    case 'look':
      return [
        { k: 'pose', pose: 'idle', t: rng.range(0.8, 1.5) },
        { k: 'turn' },
        { k: 'pose', pose: 'idle', t: rng.range(0.8, 1.6) },
        { k: 'turn' },
        pose('idle', 0.6),
      ];
    case 'eat':
      return [{ k: 'eat', t: rng.range(2.5, 6) }];
    case 'sit':
      return [{ k: 'pose', pose: 'sit', t: rng.range(3, 8) }];
    case 'sleep': {
      const sleepy = c.personality === 'sleepy' ? 1.5 : 1;
      const long = late ? 3 : 1;
      return [pose('sit', 0.4), { k: 'sleep', t: rng.range(6, 16) * sleepy * long, zt: 0 }, pose('idle', 0.8)];
    }
    case 'emote': {
      const icons: IconName[] = ['heart', 'note', 'note', 'what', 'spark'];
      return [emoteAct(icons[rng.index(icons.length)]), pose('idle', 1.0)];
    }
    case 'say':
      return [say(s.lines[rng.index(Math.max(s.lines.length, 1))]), pose('idle', 1.0)];
    case 'hop':
      return [{ k: 'hops', n: Math.ceil(rng.range(1, 3)), e: 0, h: hopHeight(c) }];
    case 'special':
      return special(c, ctx, rng);
    case 'start':
      return [pose('idle', 2.0)];
  }
}

function hopHeight(c: Critter): number {
  return clamp(rround((height(c) - 2) / 3), 2, 5);
}

/** A flight somewhere: a perch when there is one (60%), else the floor. */
function flight(c: Critter, ctx: Ctx, rng: Rng): Act {
  const w = ctx.viewW;
  const hw = halfW(c);
  const perches = ctx.perches.filter((r) => r.w >= hw * 2 && r.y > height(c) + 2 && r.y < ctx.floor - 4);
  if (perches.length > 0 && rng.chance(0.6)) {
    const r = perches[rng.index(perches.length)];
    const x = rng.range(r.x + hw, r.x + r.w - hw);
    return flyTo(x, ctx.floor - r.y, r.y);
  }
  const x = clamp(c.x + rng.sign() * rng.range(0.1, 0.25) * w, 0.1 * w, 0.9 * w);
  return flyTo(x, sp(c).hover, null);
}

function special(c: Critter, ctx: Ctx, rng: Rng): Act[] {
  const s = sp(c);
  const w = ctx.viewW;
  const clampx = (x: number) => clamp(x, 0.06 * w + halfW(c), Math.max(0.94 * w - halfW(c), 0.06 * w + halfW(c)));
  switch (s.special) {
    case 'emote': {
      const icons: IconName[] = ['note', 'spark', 'heart'];
      return [emoteAct(icons[rng.index(3)]), idle(1.0)];
    }
    case 'graze':
      return [{ k: 'eat', t: rng.range(5, 9) }, emoteAct('note'), idle(1.0)];
    case 'flight':
      if (c.perch !== null) {
        // Back down to the floor.
        const x = clampx(c.x + rng.sign() * rng.range(0.05, 0.2) * w);
        return [flyTo(x, s.hover, null)];
      }
      return [flight(c, ctx, rng), { k: 'pose', pose: 'sit', t: rng.range(5, 12) }];
    case 'pinch': {
      const p = ctx.pointer;
      if (p) {
        return [
          walkTo(clampx(p.x), 1.5),
          emoteAct('bang'),
          { k: 'pinch' },
          say('mine!'),
          idle(0.5),
          // Off with it.
          mark('exiting'),
          walkTo(p.x < w / 2 ? w + halfW(c) * 2 : -halfW(c) * 2, 2.2),
          mark('gone'),
        ];
      }
      return [{ k: 'eat', t: rng.range(3, 5) }, idle(1.0)];
    }
    case 'boom':
      return [
        pose('sit', 0.6),
        { k: 'pop', text: 'boom', color: '#9fc87a' },
        pose('sit', 1.4),
        { k: 'pop', text: 'boom', color: '#9fc87a' },
        pose('sit', 1.4),
        { k: 'pop', text: 'boom', color: '#9fc87a' },
        idle(1.0),
      ];
    case 'flit': {
      const v: Act[] = [];
      for (let i = 0; i < 3; i++) {
        const x = clampx(c.x + rng.sign() * rng.range(6, 20));
        v.push(flyTo(x, 0, null));
        v.push({ k: 'turn' });
        v.push(idle(0.3));
      }
      v.push(emoteAct('note'));
      return v;
    }
    case 'slide':
      return [pose('sit', 0.3), walkTo(clampx(c.x + c.dir * 0.15 * w), 3.0), idle(1.0)];
    case 'pushups':
      return [{ k: 'pushups', n: 3, e: 0 }, idle(1.0)];
    case 'hops':
      return [hops(3, hopHeight(c))];
    case 'loaf':
      return [{ k: 'pose', pose: 'sit', t: rng.range(8, 16) }, emoteAct('note')];
    case 'drop':
      return [{ k: 'eat', t: rng.range(2, 4) }, { k: 'dropItem' }, idle(1.2)];
    case 'spin':
      return [{ k: 'turn' }, wait(0.15), { k: 'turn' }, wait(0.15), { k: 'turn' }, wait(0.15), { k: 'turn' }, emoteAct('heart'), idle(0.8)];
    case 'hide':
      return [{ k: 'pose', pose: 'sit', t: rng.range(3, 6) }, idle(1.0)];
    case 'pounce':
      return [pose('sit', 0.8), hops(1, 7), say('heh'), idle(1.0)];
    case 'snip':
      return [{ k: 'eat', t: 1.5 }, emoteAct('spark'), idle(0.8)];
    case 'spores':
      return [{ k: 'spores' }, idle(1.5)];
    case 'teleport': {
      const to = clampx(rng.range(0.15, 0.85) * w);
      return [{ k: 'puff' }, { k: 'jump', x: to }, { k: 'puff' }, idle(1.0)];
    }
    case 'mine':
      return [{ k: 'eat', t: 3 }, emoteAct('spark'), { k: 'pop', text: '+25 xp', color: INK.gold }, idle(1.5)];
    case 'dig': {
      const to = clampx(rng.range(0.15, 0.85) * w);
      const h = height(c);
      return [{ k: 'sink', to: -h }, { k: 'jump', x: to }, { k: 'sink', to: 0 }, idle(1.0)];
    }
    case 'nothing':
      return [idle(rng.range(4, 8))];
    case 'lamp':
      return [say('lamp for you'), { k: 'offer', t: 20 }];
    case 'fuse':
      return [say('sss...'), { k: 'flash', t: 1.6 }, { k: 'burst' }];
    case 'buzz': {
      const x = clampx(c.x + rng.sign() * rng.range(10, 30));
      return [walkTo(x, 1.6), emoteAct('note'), { k: 'turn' }, idle(0.5)];
    }
    case 'fetch':
      return [{ k: 'carry', item: null, to: ctx.pointer ? ctx.pointer.x : w / 2 }];
    case 'bounce':
      return [hops(2, 8), idle(0.6)];
    case 'smoke':
      return [{ k: 'breathe' }, idle(0.4), { k: 'breathe' }, say('*puff*'), idle(1.0)];
  }
}

/** How a critter leaves, by kind: fade, fly off, dig, puff, float up, or
 *  walk off a side (sometimes down the stairs at the edge). */
function exitPlan(c: Critter, w: number, rng: Rng, pace: number, reduced: boolean): Act[] {
  const s = sp(c);
  if (reduced) return [fade(0.0, 1.0), mark('gone')];
  const hw = halfW(c);
  const side = c.x < w / 2 ? -1 : 1;
  const off = side < 0 ? -hw * 2 : w + hw * 2;
  if (has(s, FLAGS.ghostly)) return [fade(0.0, 1.5), mark('gone')];
  if (s.special === 'dig') return [{ k: 'sink', to: -height(c) - 1 }, mark('gone')];
  if (s.special === 'teleport') return [{ k: 'puff' }, fade(0.0, 0.2), mark('gone')];
  if (s.id === 'balloon') return [flyTo(c.x + side * 10, 400, null), mark('gone')];
  if ((has(s, FLAGS.flier) && rng.chance(0.5)) || s.gait === 'fly') {
    return [flyTo(off, rng.range(20, 60), null), mark('gone')];
  }
  if (c.perch === null && rng.chance(0.35) && s.gait !== 'float') {
    const edge = side < 0 ? hw + 2 : w - hw - 2;
    const h = height(c);
    return [walkTo(edge, pace), { k: 'face', d: side }, { k: 'stairs', from: 0, to: -h - 1, fadeIn: false, dist: 0 }, mark('gone')];
  }
  return [walkTo(off, pace), mark('gone')];
}

/** Moves toward `tx` at `speed`; true on arrival. */
function moveToward(c: Critter, tx: number, dt: number, speed: number, particles: Particle[], reduced: boolean): boolean {
  const d = tx - c.x;
  if (Math.abs(d) < 0.5) return true;
  if (sp(c).gait !== 'sideways') c.dir = Math.sign(d);
  const step = Math.min(speed * dt, Math.abs(d)) * Math.sign(d);
  c.x += step;
  if (has(sp(c), FLAGS.trail) && !reduced) {
    c.trailD += Math.abs(step);
    if (c.trailD > 1.5) {
      c.trailD = 0;
      const x = c.x - c.dir * halfW(c) * 0.5;
      particles.push(particle('trail', x, 0, 25, INK.trail));
    }
  }
  return false;
}

/** One step of one critter. */
function update(c: Critter, ctx: Ctx, late: boolean, rng: Rng, particles: Particle[], fx: Effect[]) {
  const dt = ctx.dt;
  c.age += dt;
  c.anim += dt;
  c.flash = false;
  c.blinkT -= dt;
  if (c.blinkT < 0) {
    c.blink = !c.blink;
    c.blinkT = c.blink ? BLINK_TIME : rng.range(2.5, 6);
  }
  if (c.bubble) {
    c.bubble.t -= dt;
    if (c.bubble.t <= 0) c.bubble = null;
  }
  if (c.squashT > 0) c.squashT -= dt;
  if (c.mode === 'held') return held(c, ctx);
  if (c.mode === 'air') return air(c, ctx, rng, particles);
  // A perch that went away (the layout changed): fall.
  if (c.perch !== null) {
    const py = c.perch;
    const still = ctx.perches.some((r) => Math.abs(r.y - py) < 0.5 && c.x >= r.x && c.x <= r.x + r.w);
    if (!still) {
      c.perch = null;
      c.mode = 'air';
      c.vlift = 0;
      c.vx = 0;
      c.cur = null;
      c.acts = [];
      return;
    }
    c.lift = ctx.floor - py;
  }
  if (c.cur === null) {
    if (c.acts.length === 0) {
      if (c.entered && !c.exiting && c.age > c.budget && !c.meeting) {
        c.exiting = true;
        if (sp(c).id === 'balloon' && !ctx.reduced && rng.chance(0.6)) {
          // Hang about for the monkey (its dart re-plans this).
          fx.push({ k: 'monkey', id: c.id });
          c.acts = [wait(12.0)];
        } else {
          c.acts = exitPlan(c, ctx.viewW, rng, 1.0, ctx.reduced);
        }
      } else if (!c.exiting) {
        c.acts = nextActs(c, ctx, late, rng);
      } else {
        // Exiting with nothing left to do: re-plan the way out.
        c.acts = exitPlan(c, ctx.viewW, rng, 1.0, ctx.reduced);
      }
    }
    c.cur = c.acts.shift() ?? null;
  }
  const act = c.cur;
  if (act) {
    c.cur = null;
    if (!run(act, c, ctx, rng, particles, fx)) c.cur = act;
  }
  const s = sp(c);
  if (has(s, FLAGS.sparkle) && !ctx.reduced && rng.chance(dt * 1.2)) {
    const x = c.x + rng.range(-1, 1) * halfW(c) * 0.8;
    const p = particle('sparkle', x, c.lift + rng.range(0.3, 1) * height(c), 1.2, '#ffe38a');
    p.vrise = 8;
    particles.push(p);
  }
  // Hover gaits: float bobs, fly hangs at its height unless sitting.
  const k = c.cur?.k;
  if (c.perch === null && !(k === 'flyTo' || k === 'sink' || k === 'stairs')) {
    if (s.gait === 'float') {
      const target = s.hover + rround(wave(c.age / 3) * 1.5);
      c.lift += clamp(target - c.lift, -10 * dt, 10 * dt);
    } else if (s.gait === 'fly') {
      const landed = c.pose === 'sit' || c.pose === 'sleep' || c.pose === 'eat';
      const target = landed ? 0 : s.hover + rround(wave(c.age / 2));
      c.lift += clamp(target - c.lift, -12 * dt, 12 * dt);
      if (c.lift > 0.5 && (c.pose === 'idle' || c.pose === 'walk')) {
        c.pose = 'fly';
        c.frame = Math.floor(c.anim * 12) % 2;
      }
    }
  }
}

/** Runs one action for a step; true when it's finished. */
function run(act: Act, c: Critter, ctx: Ctx, rng: Rng, particles: Particle[], fx: Effect[]): boolean {
  const dt = ctx.dt;
  const s = sp(c);
  const reduced = ctx.reduced;
  switch (act.k) {
    case 'mark':
      if (act.m === 'entered') c.entered = true;
      else if (act.m === 'gone') c.gone = true;
      else if (act.m === 'meetingOver') c.meeting = false;
      else c.exiting = true;
      return true;
    case 'pose':
      c.pose = act.pose;
      c.frame = 0;
      act.t -= dt;
      return act.t <= 0;
    case 'turn':
      if (s.gait !== 'sideways') c.dir = -c.dir;
      return true;
    case 'face':
      if (s.gait !== 'sideways') c.dir = act.d;
      return true;
    case 'faceX':
      if (s.gait !== 'sideways' && Math.abs(act.x - c.x) > 0.5) c.dir = Math.sign(act.x - c.x);
      return true;
    case 'wait':
      act.t -= dt;
      return act.t <= 0;
    case 'fade': {
      if (act.from === null) act.from = c.alpha;
      const f = act.from;
      act.e += dt;
      c.pose = 'idle';
      c.alpha = f + (act.to - f) * Math.min(act.e / act.t, 1);
      return act.e >= act.t;
    }
    case 'walkTo':
      return walk(c, act.x, act.pace, dt, particles, reduced);
    case 'stairs': {
      const STEP_W = 5;
      const STEP_H = 3;
      const n = Math.ceil(Math.abs(act.to - act.from) / STEP_H);
      if (act.dist === 0) {
        c.lift = act.from;
        if (act.fadeIn) c.alpha = 0;
      }
      c.pose = 'walk';
      c.frame = Math.floor(c.anim * 5) % 2;
      const v = s.speed * 0.8 * dt;
      act.dist += v;
      c.x += c.dir * v;
      const k = Math.min(Math.floor(act.dist / STEP_W), n);
      const prev = c.lift;
      c.lift = act.from + Math.sign(act.to - act.from) * Math.min(Math.abs(act.to - act.from), k * STEP_H);
      if (c.lift !== prev) dust(particles, rng, c.x - c.dir * halfW(c) * 0.6, Math.max(c.lift, 0), 3, reduced);
      if (act.fadeIn) c.alpha = Math.min(act.dist / STEP_W, 1);
      return k >= n;
    }
    case 'flyTo': {
      if (act.from === null) act.from = [c.x, c.lift];
      const [sx, sl] = act.from;
      if (act.total === 0) act.total = Math.max(Math.abs(act.x - sx) / (s.speed * 2.2), 1.2);
      act.e += dt;
      const u = Math.min(act.e / act.total, 1);
      c.pose = 'fly';
      c.frame = Math.floor(c.anim * 8) % 2;
      if (Math.abs(act.x - sx) > 0.5) c.dir = Math.sign(act.x - sx);
      c.x = sx + (act.x - sx) * u;
      const arc = Math.min(FLIGHT_ARC, ctx.viewH * 0.3);
      c.lift = sl + (act.lift - sl) * (1 - (1 - u) * (1 - u)) + arch(u) * arc;
      if (u >= 1) {
        c.lift = act.lift;
        c.perch = act.perch;
        c.pose = 'idle';
        return true;
      }
      return false;
    }
    case 'eat':
      act.t -= dt;
      c.pose = Math.floor(c.anim * 2.5) % 3 !== 0 ? 'eat' : 'idle';
      c.frame = 0;
      if (act.t <= 0) {
        c.pose = 'idle';
        return true;
      }
      return false;
    case 'sleep':
      c.pose = 'sleep';
      c.frame = 0;
      act.t -= dt;
      act.zt -= dt;
      if (act.zt <= 0 && !reduced) {
        act.zt = 1.3;
        const [hx, hr] = head(c);
        const p = particle('icon', hx, hr, 2.4, iconColor('z'), 'z');
        p.vx = 6 * c.dir;
        p.vrise = 14;
        particles.push(p);
      }
      return act.t <= 0;
    case 'emote': {
      const [hx, hr] = head(c);
      emote(particles, hx, hr, act.icon, reduced);
      return true;
    }
    case 'say':
      c.bubble = { text: act.text, t: 2.6 };
      return true;
    case 'hops': {
      act.e += dt;
      c.pose = 'walk';
      const ph = act.e * 2.4;
      c.frame = fract(ph) < 0.8 ? 1 : 0;
      c.lift = rround(arch(fract(ph)) * act.h);
      if (ph >= act.n) {
        c.lift = 0;
        c.pose = 'idle';
        c.frame = 0;
        dust(particles, rng, c.x, 0, 4, reduced);
        return true;
      }
      return false;
    }
    case 'pushups':
      act.e += dt;
      c.pose = 'idle';
      c.lift = Math.floor(act.e * 3) % 2;
      if (act.e >= act.n / 1.5) {
        c.lift = 0;
        return true;
      }
      return false;
    case 'pop':
      if (!reduced) {
        const [hx, hr] = head(c);
        const p = particle('text', hx, hr + 6, 2.2, act.color);
        p.text = act.text;
        p.vrise = 14;
        particles.push(p);
      }
      return true;
    case 'dropItem':
      if (s.drop) {
        const x = c.x - c.dir * (halfW(c) + 2);
        const item = particle('item', x, 0, ITEM_LIFE, INK.bone);
        item.text = s.drop;
        particles.push(item);
        if (!reduced) {
          const p = particle('text', x, 6, 2.2, INK.bone);
          p.text = s.drop;
          p.vrise = 12;
          particles.push(p);
        }
      }
      return true;
    case 'puff':
      if (!reduced) {
        for (const [dx, dy] of RING) {
          const p = particle('smoke', c.x, c.lift + height(c) / 2, 0.7, INK.smoke);
          p.vx = dx * 18;
          p.vrise = dy * 12;
          particles.push(p);
        }
      }
      c.alpha = c.alpha > 0.5 ? 0 : 1;
      return true;
    case 'spores':
      if (!reduced) {
        for (let i = 0; i < 6; i++) {
          const p = particle('sparkle', c.x + rng.range(-4, 4), height(c), 1.6, '#e8d8b0');
          p.vrise = rng.range(4, 10);
          p.vx = rng.range(-5, 5);
          particles.push(p);
        }
      }
      c.pose = 'eat';
      return true;
    case 'sink': {
      c.pose = 'idle';
      const d = act.to - c.lift;
      const step = Math.min(8 * dt, Math.abs(d)) * Math.sign(d);
      c.lift += step;
      if (rng.chance(dt * 6)) dust(particles, rng, c.x, 0, 2, reduced);
      return Math.abs(act.to - c.lift) < 0.01;
    }
    case 'jump':
      c.x = act.x;
      return true;
    case 'flash': {
      act.t -= dt;
      c.pose = 'idle';
      // Faster as the fuse burns down.
      const rate = act.t < 0.6 ? 12 : 5;
      c.flash = Math.floor(c.anim * rate) % 2 === 1;
      return act.t <= 0;
    }
    case 'burst':
      if (!reduced) {
        for (let i = 0; i < 14; i++) {
          const p = particle('confetti', c.x, height(c) / 2, 1.8, INK.confetti[i % INK.confetti.length]);
          p.vx = rng.range(-50, 50);
          p.vrise = rng.range(30, 100);
          p.gravity = 120;
          particles.push(p);
        }
        const p = particle('text', c.x, height(c), 1.6, INK.gold);
        p.text = 'pop!';
        p.vrise = 16;
        particles.push(p);
      }
      c.gone = true;
      return true;
    case 'pinch':
      if (!reduced) {
        const [hx, hr] = head(c);
        const p = particle('text', hx, hr + 6, 2.2, INK.gold);
        p.text = '-1 shiny';
        p.vrise = 14;
        particles.push(p);
      }
      return true;
    case 'carry': {
      // Find the nearest free item the first time; no item, a twirl.
      if (act.item === null) {
        // min_by keeps the first of equal minimums.
        let near: number | null = null;
        for (let i = 0; i < particles.length; i++) {
          const p = particles[i];
          if (p.kind !== 'item' || p.carriedBy !== null) continue;
          if (near === null || Math.abs(p.x - c.x) < Math.abs(particles[near].x - c.x)) near = i;
        }
        if (near === null) {
          const [hx, hr] = head(c);
          emote(particles, hx, hr, 'note', reduced);
          return true;
        }
        act.item = near;
      }
      const i = act.item;
      const p = particles[i];
      if (!p) return true;
      if (p.kind !== 'item') return true;
      if (p.carriedBy !== c.id) {
        const px = p.x;
        if (moveToward(c, px, dt, s.speed, particles, reduced)) {
          particles[i].carriedBy = c.id;
          particles[i].life = ITEM_LIFE;
        }
        c.pose = 'walk';
        c.frame = Math.floor(c.anim * 5) % 2;
        return false;
      }
      const lo = 0.06 * ctx.viewW;
      const hi = 0.94 * ctx.viewW;
      const dest = clamp(act.to, lo, hi);
      c.pose = 'walk';
      c.frame = Math.floor(c.anim * 5) % 2;
      if (moveToward(c, dest, dt, s.speed, particles, reduced)) {
        particles[i].carriedBy = null;
        particles[i].rise = 0;
        c.bubble = { text: 'for you!', t: 2.6 };
        const [hx, hr] = head(c);
        emote(particles, hx, hr, 'heart', reduced);
        c.pose = 'idle';
        return true;
      }
      return false;
    }
    case 'throw': {
      const [hx, hr] = head(c);
      const o = ctx.others.find((o) => o[0] === act.target);
      if (o) {
        const [, , bx, bl] = o;
        const p = particle('dart', hx, hr, 0.5, '#d8d0c0');
        p.vx = (bx - hx) / 0.5;
        p.vrise = (bl + 6 - hr) / 0.5;
        particles.push(p);
        fx.push({ k: 'plan', id: act.target, acts: [wait(0.5), { k: 'popSelf' }] });
      }
      c.pose = 'eat';
      return true;
    }
    case 'popSelf':
      if (!reduced) {
        for (let i = 0; i < 10; i++) {
          const p = particle('confetti', c.x, c.lift + height(c) / 2, 1.4, INK.confetti[i % INK.confetti.length]);
          p.vx = rng.range(-40, 40);
          p.vrise = rng.range(10, 60);
          p.gravity = 120;
          particles.push(p);
        }
        const p = particle('text', c.x, c.lift + height(c), 1.4, INK.gold);
        p.text = 'pop!';
        p.vrise = 16;
        particles.push(p);
      }
      c.gone = true;
      return true;
    case 'merge':
      fx.push({ k: 'merge', a: c.id, b: act.with });
      return true;
    case 'offer':
      c.offering = true;
      c.pose = 'idle';
      act.t -= dt;
      if (act.t <= 0) {
        c.offering = false;
        c.bubble = { text: '...fine', t: 2.0 };
        c.budget = 0;
        return true;
      }
      return false;
    case 'breathe':
      if (!reduced) {
        const [hx, hr] = head(c);
        for (let i = 0; i < 4; i++) {
          const p = particle('smoke', hx + c.dir * 3, hr - 2, 0.9 + i * 0.1, INK.smoke);
          p.vx = c.dir * rng.range(10, 22);
          p.vrise = rng.range(2, 8);
          particles.push(p);
        }
      }
      c.pose = 'eat';
      return true;
  }
}

/** Walking by gait (the overlay's walkTo); true on arrival. */
function walk(c: Critter, tx: number, pace: number, dt: number, particles: Particle[], reduced: boolean): boolean {
  const s = sp(c);
  const speed = s.speed * pace;
  if (s.gait === 'hop') {
    const ph = fract(c.anim * 2.2);
    const h = hopHeight(c);
    c.lift = rround(arch(ph) * h);
    c.pose = c.lift > 0 ? 'walk' : 'idle';
    c.frame = 1;
    const moving = ph < 0.85 ? 1.3 : 0;
    const done = moveToward(c, tx, dt, speed * moving, particles, reduced);
    if (done) {
      c.lift = 0;
      c.pose = 'idle';
      c.frame = 0;
    }
    return done;
  }
  c.pose = 'walk';
  const rate = s.gait === 'slide' ? 1.2 : s.gait === 'dart' ? 9 : s.gait === 'waddle' ? 4 : 5;
  c.frame = Math.floor(c.anim * rate) % 2;
  if (s.gait === 'waddle') c.lift = c.frame;
  let v = speed;
  if (s.gait === 'dart') {
    const ph = fract(c.anim * 0.9);
    if (ph >= 0.55) {
      v = 0;
      c.pose = 'idle';
    } else {
      v *= 2.2;
    }
  }
  const done = moveToward(c, tx, dt, v, particles, reduced);
  if (done) {
    if (s.gait === 'waddle') c.lift = 0;
    c.pose = 'idle';
    c.frame = 0;
  }
  return done;
}

function held(c: Critter, ctx: Ctx) {
  const dt = ctx.dt;
  c.pose = 'held';
  c.frame = Math.floor(c.anim * 6) % 2;
  const h = c.hold;
  if (!h) return;
  const p = ctx.pointer;
  if (!p) return;
  if (h.last) {
    const [lx, ly] = h.last;
    const vx = (p.x - lx) / dt;
    const vy = (p.y - ly) / dt;
    h.vx = h.vx * 0.5 + vx * 0.5;
    h.vy = h.vy * 0.5 + vy * 0.5;
  }
  h.last = [p.x, p.y];
  const hw = spriteW(c.species) / 2;
  c.x = clamp(p.x + h.dx, hw, Math.max(ctx.viewW - hw, hw));
  const feet = p.y + h.dy;
  c.lift = clamp(ctx.floor - feet, 0, Math.max(ctx.floor - height(c), 0));
}

function air(c: Critter, ctx: Ctx, rng: Rng, particles: Particle[]) {
  const dt = ctx.dt;
  const hw = halfW(c);
  c.pose = 'held';
  c.frame = 0;
  const prevY = ctx.floor - c.lift;
  c.vlift -= GRAVITY * dt;
  c.x += c.vx * dt;
  c.lift += c.vlift * dt;
  // Walls and ceiling.
  const lo = hw;
  const hi = Math.max(ctx.viewW - hw, hw);
  if (c.x < lo || c.x > hi) {
    c.x = clamp(c.x, lo, hi);
    c.vx = -c.vx * WALL_BOUNCE;
  }
  const ceiling = Math.max(ctx.floor - height(c), 0);
  if (c.lift > ceiling) {
    c.lift = ceiling;
    c.vlift = Math.min(c.vlift, 0);
  }
  // Perches catch a falling critter whose feet cross their top.
  const y = ctx.floor - c.lift;
  let ground = 0;
  let perch: number | null = null;
  if (c.vlift < 0) {
    for (const r of ctx.perches) {
      if (prevY <= r.y && y >= r.y && c.x >= r.x && c.x <= r.x + r.w && r.y < ctx.floor) {
        ground = ctx.floor - r.y;
        perch = r.y;
        break;
      }
    }
  }
  if (c.lift <= ground) {
    c.lift = ground;
    if (-c.vlift > SETTLE_SPEED) {
      c.vlift = -c.vlift * BOUNCE;
      c.vx *= FRICTION;
      c.squashT = SQUASH_TIME;
      dust(particles, rng, c.x, ground, 3, ctx.reduced);
    } else {
      // Settled.
      c.vlift = 0;
      c.vx = 0;
      c.mode = 'brain';
      c.perch = perch;
      c.squashT = SQUASH_TIME;
      c.pose = 'idle';
      c.cur = null;
      c.meeting = false;
      const s = sp(c);
      const acts: Act[] = [pose('idle', 0.3), emoteAct('what')];
      if (s.lines.length > 0 && rng.chance(0.4)) acts.push(say(s.lines[rng.index(s.lines.length)]));
      acts.push(pose('idle', 0.6));
      if (!c.entered) acts.push(mark('entered'));
      c.acts = acts;
    }
  }
}

function tickParticles(particles: Particle[], critters: Critter[], dt: number, reduced: boolean): Particle[] {
  // Only lying items survive reduced motion.
  const ps = reduced ? particles.filter((p) => p.kind === 'item') : particles;
  for (const p of ps) {
    p.life -= dt;
    if (p.carriedBy !== null) {
      const c = critters.find((c) => c.id === p.carriedBy);
      if (c) {
        p.x = c.x + c.dir * (halfW(c) - 1);
        p.rise = c.lift + 2;
        continue;
      }
      p.carriedBy = null;
    }
    p.x += p.vx * dt;
    p.rise += p.vrise * dt;
    p.vrise -= p.gravity * dt;
    if (p.gravity > 0 && p.rise < 0) {
      p.rise = 0;
      p.vrise = 0;
      p.vx *= 0.5;
    }
  }
  return ps.filter((p) => p.life > 0);
}
