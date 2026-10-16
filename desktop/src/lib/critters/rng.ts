// The seeded random source, bit for bit kete-critters' `rng.rs` (and the
// stream overlay's): `hashStr` turns a seed string into a u32 and
// mulberry32 walks from it. The world's draws must come in the same order
// as the Rust's for a seeded run to replay (README.md, "Matching Rust").

/** The overlay's `hashStr` over UTF-16 code units. */
export function hashStr(s: string): number {
  let h = 1779033703 ^ s.length;
  for (let i = 0; i < s.length; i++) {
    h = Math.imul(h ^ s.charCodeAt(i), 3432918353);
    h = (h << 13) | (h >>> 19);
  }
  return h >>> 0;
}

/** mulberry32: a 32-bit state, one number in [0, 1) per call. */
export class Rng {
  state: number;

  constructor(seed: number) {
    this.state = seed >>> 0;
  }

  static fromSeed(seed: string): Rng {
    return new Rng(hashStr(seed));
  }

  clone(): Rng {
    return new Rng(this.state);
  }

  /** The next value in [0, 1). */
  float(): number {
    this.state = (this.state + 0x6d2b79f5) >>> 0;
    const a = this.state;
    let t = Math.imul(a ^ (a >>> 15), 1 | a);
    t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t;
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  }

  /** Uniform in [a, b). */
  range(a: number, b: number): number {
    return a + (b - a) * this.float();
  }

  /** True with probability `p`. */
  chance(p: number): boolean {
    return this.float() < p;
  }

  /** An index into a list of length `n` (n > 0). */
  index(n: number): number {
    return Math.min(Math.floor(this.float() * n), Math.max(0, n - 1));
  }

  /** -1 or 1, evenly. */
  sign(): number {
    return this.chance(0.5) ? 1 : -1;
  }

  /** The overlay's `wpick`: zero weights are never picked unless every
   *  weight is zero (then index 0). */
  weighted(weights: readonly number[]): number {
    let total = 0;
    for (const w of weights) total += w;
    if (total <= 0) return 0;
    let r = this.float() * total;
    for (let i = 0; i < weights.length; i++) {
      const w = weights[i];
      if (w <= 0) continue;
      r -= w;
      if (r <= 0) return i;
    }
    for (let i = weights.length - 1; i >= 0; i--) if (weights[i] > 0) return i;
    return 0;
  }
}
