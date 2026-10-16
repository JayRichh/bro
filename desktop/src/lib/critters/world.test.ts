// The world's own tests, mirrored from kete-critters' tests.rs: it is
// deterministic from a seed, settles, stays on screen, honours the clock
// and reacts to clicks and app events. Same seeds, durations and inputs as
// the Rust, so a drift between the two shows up here and in golden.test.ts.
import { describe, expect, it } from 'vitest';
import { FLAGS, SPECIES } from './cast.gen';
import { byId, defaultInput, speciesOf, STEP, World, type Input, type Note, type Often } from './world';

function input(): Input {
  return { ...defaultInput(), viewW: 160, viewH: 60, floor: 56 };
}

/** Ticks `world` for `secs` at `fps`, calling `check` after every tick. */
function run(world: World, inp: Input, secs: number, fps: number, check: (w: World) => void = () => {}) {
  const dt = 1 / fps;
  const n = Math.round(secs * fps);
  for (let i = 0; i < n; i++) {
    world.tick(dt, inp);
    check(world);
  }
}

/** Everything that should match between two runs, as one string. */
function fingerprint(w: World): string {
  let s = `t=${w.steps} n=${w.critters.length} p=${w.particles.length}`;
  for (const c of w.critters) {
    s += ` [${c.id} ${speciesOf(c).id} ${c.x.toFixed(4)} ${c.lift.toFixed(4)} ${c.pose} ${c.name}]`;
  }
  return s;
}

const has = (species: number, flag: number) => (SPECIES[species].flags & flag) !== 0;

interface Entry {
  seen: number;
  pets: number;
  fed: number;
  name?: string;
  firstSeen?: string;
}

/** Folds notes into per-species counts, as the collection log does. */
function fold(notes: Note[], today: string): Map<string, Entry> {
  const log = new Map<string, Entry>();
  const entry = (species: string) => {
    let e = log.get(species);
    if (!e) log.set(species, (e = { seen: 0, pets: 0, fed: 0 }));
    return e;
  };
  for (const n of notes) {
    if (n.kind === 'seen') {
      const e = entry(n.species);
      e.seen++;
      e.firstSeen ??= today;
    } else if (n.kind === 'petted') entry(n.species).pets++;
    else if (n.kind === 'fed') entry(n.species).fed++;
    else if (n.kind === 'named') entry(n.species).name = n.name;
  }
  return log;
}

describe('critters world', () => {
  it('the world is deterministic from a seed', () => {
    const inp = input();
    const a = new World(42);
    const b = new World(42);
    a.often = 'busy';
    b.often = 'busy';
    run(a, inp, 240, 8);
    run(b, inp, 240, 8);
    expect(fingerprint(a)).toBe(fingerprint(b));
    expect(a.clock()).toBeGreaterThan(239);
    // A different seed goes a different way.
    const c = new World(43);
    c.often = 'busy';
    run(c, inp, 240, 8);
    expect(fingerprint(a)).not.toBe(fingerprint(c));
  });

  it('frame rate does not change the simulation', () => {
    // 8 fps and 32 fps tick the same fixed steps.
    const inp = input();
    const a = new World(9);
    const b = new World(9);
    run(a, inp, 60, 8);
    run(b, inp, 60, 1 / STEP);
    expect(fingerprint(a)).toBe(fingerprint(b));
  });

  it('a flung critter bounces and settles', () => {
    const inp = input();
    const w = new World(1);
    w.spawning = false;
    const id = w.summon('cow', inp)!;
    run(w, inp, 8, 30);
    const cx = w.get(id)!.x;
    const top = inp.floor - 6;
    expect(w.grab(id, cx, top)).toBe(true);
    // Drag it up and to the right fast, then let go.
    const held: Input = { ...inp };
    for (let k = 0; k < 8; k++) {
      held.pointer = { x: cx + k * 6, y: top - k * 4 };
      w.tick(STEP, held);
      expect(w.get(id)!.mode).toBe('held');
      expect(w.get(id)!.pose).toBe('held');
    }
    w.release();
    expect(w.get(id)!.mode).toBe('air');
    let bounced = false;
    let last = Number.MAX_VALUE;
    run(w, inp, 6, 30, (w) => {
      const c = w.get(id);
      if (c) {
        if (c.lift > last + 0.01 && c.mode === 'air') bounced = true;
        last = c.lift;
        const hw = SPECIES[c.species].sprite.w / 2;
        // Inside the walls.
        expect(c.x).toBeGreaterThanOrEqual(hw - 0.01);
        expect(c.x).toBeLessThanOrEqual(inp.viewW - hw + 0.01);
      }
    });
    const c = w.get(id)!;
    expect(bounced, 'it bounced').toBe(true);
    expect(c.mode, 'it settled').toBe('brain');
    expect(Math.abs(c.lift)).toBeLessThan(0.01);
  });

  it('fliers take off when dropped', () => {
    const inp = input();
    const w = new World(2);
    w.spawning = false;
    const id = w.summon('tui', inp)!;
    run(w, inp, 8, 30);
    const x = w.get(id)!.x;
    const y = inp.floor - 4;
    w.grab(id, x, y);
    const held: Input = { ...inp, pointer: { x, y: y - 20 } };
    w.tick(0.2, held);
    w.release();
    expect(w.get(id)!.mode).toBe('brain');
    w.tick(STEP * 2, inp);
    expect(w.get(id)!.pose).toBe('fly');
  });

  it('critters stay on screen unless coming or going', { timeout: 30_000 }, () => {
    const inp: Input = { ...input(), perches: [{ x: 20, y: 20, w: 60, h: 4 }] };
    for (let seed = 0; seed < 4; seed++) {
      const w = new World(seed);
      w.often = 'busy';
      let onScreenSteps = 0;
      run(w, inp, 900, 8, (w) => {
        for (const c of w.critters) {
          if (!c.entered || c.exiting || c.mode === 'held') continue;
          onScreenSteps++;
          const s = speciesOf(c);
          const hw = s.sprite.w / 2;
          const where = `seed ${seed}: ${s.id} at x ${c.x} (view ${inp.viewW})`;
          expect(c.x, where).toBeGreaterThanOrEqual(hw * 0.5);
          expect(c.x, where).toBeLessThanOrEqual(inp.viewW - hw * 0.5);
          const digger = s.special === 'dig' || s.special === 'teleport';
          expect(c.lift > -0.5 || digger, `seed ${seed}: ${s.id} sank to ${c.lift}`).toBe(true);
          expect(c.lift <= inp.floor, `seed ${seed}: ${s.id} above the top`).toBe(true);
        }
      });
      expect(onScreenSteps, `seed ${seed}: somebody visited`).toBeGreaterThan(0);
    }
  });

  it('busier settings bring more visitors', { timeout: 30_000 }, () => {
    const inp = input();
    const seen = (often: Often) => {
      const w = new World(5);
      w.often = often;
      let n = 0;
      run(w, inp, 3600, 4);
      for (const note of w.drainNotes()) {
        if (note.kind === 'seen') n++;
      }
      return n;
    };
    const [rare, normal, busy] = [seen('rare'), seen('normal'), seen('busy')];
    expect(rare < normal && normal < busy, `rare ${rare}, normal ${normal}, busy ${busy}`).toBe(true);
    expect(rare, `rare still visits: ${rare}`).toBeGreaterThanOrEqual(5);
  });

  it('nocturnal species only come at night', { timeout: 30_000 }, () => {
    const w = new World(0);
    const day = w.spawnWeights(13, []);
    const night = w.spawnWeights(23, []);
    SPECIES.forEach((s, i) => {
      if (has(i, FLAGS.nocturnal)) {
        expect(day[i], `${s.id} by day`).toBe(0);
        expect(night[i], `${s.id} at night`).toBeGreaterThan(0);
      }
      if (has(i, FLAGS.noSpawn)) {
        expect(day[i] + night[i], `${s.id} never spawns`).toBe(0);
      }
    });
    const tui = byId('tui')!;
    expect(w.spawnWeights(7, [])[tui], 'tui like mornings').toBeGreaterThan(w.spawnWeights(15, [])[tui]);
    // And a long day brings no night birds.
    const w2 = new World(11);
    w2.often = 'busy';
    const inp: Input = { ...input(), hour: 12 };
    run(w2, inp, 3600, 4);
    for (const note of w2.drainNotes()) {
      if (note.kind === 'seen') {
        expect(has(byId(note.species)!, FLAGS.nocturnal), `${note.species} came by day`).toBe(false);
      }
    }
  });

  it('clicks land on drawn pixels only', () => {
    const inp = input();
    const w = new World(3);
    w.spawning = false;
    const id = w.summon('cow', inp)!;
    run(w, inp, 10, 30);
    const c = w.get(id)!;
    const s = SPECIES[c.species].sprite;
    // The middle of the body.
    const x = c.x;
    const y = inp.floor - c.lift - s.h / 2;
    expect(w.hit(x, y)).toBe(id);
    // Well above it, and the far corner of its box.
    expect(w.hit(x, y - 30)).toBeNull();
    const [left, top] = w.bounds(id)!;
    expect(w.hit(left, top), 'the box corner is transparent').toBeNull();
  });

  it('petting, feeding and naming', () => {
    const inp = input();
    const w = new World(4);
    w.spawning = false;
    const id = w.summon('kiwi', inp)!;
    run(w, inp, 10, 30);
    expect(w.pet(id)).toBe(true);
    expect(w.particles.some((p) => p.kind === 'icon' && p.icon === 'heart')).toBe(true);
    run(w, inp, 3, 30);
    expect(w.feed(id)).toBe(true);
    let ate = false;
    run(w, inp, 4, 30, (w) => {
      ate ||= w.get(id)?.pose === 'eat';
    });
    expect(ate).toBe(true);
    expect(w.rename(id, 'Bob')).toBe(true);
    expect(w.get(id)!.name).toBe('Bob');
    const log = fold(w.drainNotes(), '2026-10-16');
    const e = log.get('kiwi')!;
    expect([e.seen, e.pets, e.fed]).toEqual([1, 1, 1]);
    expect(e.name).toBe('Bob');
    expect(e.firstSeen).toBe('2026-10-16');
    // The next kiwi is Bob too.
    w.names = new Map([...log].flatMap(([species, l]) => (l.name === undefined ? [] : [[species, l.name] as [string, string]])));
    w.clear();
    const next = w.summon('kiwi', inp)!;
    expect(w.get(next)!.name).toBe('Bob');
  });

  it('shoo sends it off screen', () => {
    const inp = input();
    const w = new World(6);
    w.spawning = false;
    const id = w.summon('pig', inp)!;
    run(w, inp, 6, 30);
    expect(w.shoo(id)).toBe(true);
    run(w, inp, 40, 30);
    expect(w.get(id), 'gone').toBeUndefined();
  });

  it('the cat chases the mouse off', () => {
    const inp = input();
    const w = new World(8);
    w.spawning = false;
    const cat = w.summon('cat', inp)!;
    const mouse = w.summon('mouse', inp)!;
    run(w, inp, 10, 30);
    w.meetIn = 1;
    w.tick(STEP, inp);
    run(w, inp, 30, 30);
    expect(w.get(mouse), 'the mouse ran off').toBeUndefined();
    expect(w.get(cat), 'the cat stayed').toBeDefined();
  });

  it('a clicked slime splits and merges back', () => {
    const inp = input();
    const w = new World(10);
    w.spawning = false;
    const id = w.summon('slime', inp)!;
    run(w, inp, 8, 30);
    expect(w.pet(id)).toBe(true);
    const minis = w.critters.filter((c) => speciesOf(c).id === 'slime_mini').map((c) => c.id);
    expect(minis.length).toBe(2);
    run(w, inp, 3, 30);
    w.meetIn = 1;
    run(w, inp, 20, 30);
    expect(
      w.critters.some((c) => speciesOf(c).id === 'slime'),
      'merged back',
    ).toBe(true);
    expect(w.critters.some((c) => speciesOf(c).id === 'slime_mini')).toBe(false);
  });

  it('app events reach the critters', () => {
    const inp = input();
    const w = new World(12);
    w.spawning = false;
    const adv = w.summon('adventurer', inp)!;
    run(w, inp, 8, 30);
    const ev: Input = { ...inp };
    ev.events = [{ kind: 'turnDone' }];
    w.tick(STEP, ev);
    expect(w.get(adv)!.bubble?.text).toBe('gz');
    ev.events = [{ kind: 'prMerged', x: 80 }];
    w.tick(STEP, ev);
    expect(w.particles.some((p) => p.kind === 'beam')).toBe(true);
    ev.events = [{ kind: 'needsYou', x: 10 }];
    w.tick(STEP * 3, ev);
    expect(w.particles.some((p) => p.kind === 'icon' && p.icon === 'bang')).toBe(true);
  });

  it('late and idle means sleep', () => {
    const inp: Input = { ...input(), hour: 23, allIdle: true };
    const w = new World(13);
    w.spawning = false;
    w.summon('dog', inp);
    let asleep = 0;
    run(w, inp, 120, 8, (w) => {
      if (w.critters.some((c) => c.pose === 'sleep')) asleep++;
    });
    expect(asleep, `slept for ${asleep} of 960 ticks`).toBeGreaterThan(120);
  });

  it('reduced motion makes no particles', () => {
    const inp: Input = { ...input(), reducedMotion: true };
    const w = new World(14);
    w.often = 'busy';
    run(w, inp, 600, 8, (w) => {
      expect(
        w.particles.every((p) => p.kind === 'item'),
        'no particles',
      ).toBe(true);
      for (const c of w.critters) expect(c.pose, `${speciesOf(c).id} walked`).not.toBe('walk');
    });
  });

  it('nothing to animate between visits', () => {
    const inp = input();
    const w = new World(15);
    expect(w.active()).toBe(false);
    const wake = w.nextWake();
    expect(wake, 'a spawn timer').not.toBeNull();
    expect(wake!).toBeGreaterThan(0);
    expect(wake!).toBeLessThan(10);
    w.tick(wake! + STEP * 2, inp);
    expect(w.active(), 'the visitor arrived on time').toBe(true);
    expect(w.nextWake(), 'animating while someone is here').toBeNull();
    w.spawning = false;
    w.clear();
    expect(w.nextWake(), 'spawning off: no timer at all').toBeNull();
  });

  it('the balloon only drifts', () => {
    const inp = input();
    const w = new World(17);
    w.spawning = false;
    const id = w.summon('balloon', inp)!;
    run(w, inp, 60, 8, (w) => {
      const c = w.get(id);
      if (c) expect(['sleep', 'eat', 'sit'], `a balloon in ${c.pose}`).not.toContain(c.pose);
      expect(
        w.particles.some((p) => p.kind === 'icon'),
        'nobody emotes here',
      ).toBe(false);
    });
  });
});
