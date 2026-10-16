// The port replays the Rust world: cast.gen's GOLDEN is a seeded
// five-minute run of `crates/kete-critters`, and this one must land on the
// same cast at every checkpoint, to two decimals.
import { describe, expect, it } from 'vitest';
import { GOLDEN, RNG_CHECK } from './cast.gen';
import { hashStr, Rng } from './rng';
import { defaultInput, rround, speciesOf, steps, STEP, World } from './world';

/** Two decimals the way the generator rounds them; `+ 0` turns -0 into 0. */
const r2 = (v: number) => rround(v * 100) / 100 + 0;

describe('the port', () => {
  it('draws the stream overlay RNG', () => {
    expect(hashStr(RNG_CHECK.seed)).toBe(RNG_CHECK.hash);
    const r = Rng.fromSeed(RNG_CHECK.seed);
    expect([r.float(), r.float(), r.float(), r.float()]).toEqual([...RNG_CHECK.first]);
  });

  it('replays the golden seeded run', () => {
    const input = { ...defaultInput(), viewW: GOLDEN.viewW, viewH: GOLDEN.viewH, floor: GOLDEN.floor, hour: GOLDEN.hour };
    const w = new World(GOLDEN.seed);
    w.often = 'busy';
    for (const cp of GOLDEN.checkpoints) {
      for (let i = 0; i < steps(10); i++) w.tick(STEP, input);
      const cast = w.critters.map((c) => [speciesOf(c).id, r2(c.x), r2(c.lift), c.pose]);
      expect(cast, `t=${cp.t}`).toEqual(cp.cast.map((row) => [...row]));
    }
  });
});
