// The generated cast reads the way README.md says: every frame decodes to
// w*h pixel values inside its palette, every pose has frames, the names
// carry their macrons, and the RNG check is the stream overlay's.
import { describe, expect, it } from "vitest";
import { decodeFrame, GLYPHS, MACRONS, POSES, RNG_CHECK, SPECIES } from "./cast.gen";

function hashStr(s: string): number {
  let h = 1779033703 ^ s.length;
  for (let i = 0; i < s.length; i++) {
    h = Math.imul(h ^ s.charCodeAt(i), 3432918353);
    h = (h << 13) | (h >>> 19);
  }
  return h >>> 0;
}

function mulberry32(seed: number): () => number {
  let a = seed;
  return () => {
    a |= 0;
    a = (a + 0x6d2b79f5) | 0;
    let t = Math.imul(a ^ (a >>> 15), 1 | a);
    t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t;
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

describe("cast.gen", () => {
  it("decodes every frame to w*h values inside the palette", () => {
    for (const s of SPECIES) {
      const { w, h, palette, frames } = s.sprite;
      for (const pose of POSES) {
        expect(frames[pose].length, `${s.id} ${pose}`).toBeGreaterThan(0);
        for (const f of frames[pose]) {
          const px = decodeFrame(f);
          expect(px.length, `${s.id} ${pose}`).toBe(w * h);
          for (const v of px) expect(v).toBeLessThanOrEqual(palette.length);
        }
      }
    }
  });

  it("keeps the macrons and draws them", () => {
    const pukeko = SPECIES.find((s) => s.id === "pukeko");
    expect(pukeko?.name).toBe("P\u016bkeko");
    for (const s of SPECIES) {
      for (const ch of s.name.toUpperCase()) {
        expect(GLYPHS[ch] ?? MACRONS[ch], `${s.name}: ${ch}`).toBeTruthy();
      }
    }
  });

  it("matches the stream overlay's RNG", () => {
    expect(hashStr(RNG_CHECK.seed)).toBe(RNG_CHECK.hash);
    const next = mulberry32(RNG_CHECK.hash);
    expect([next(), next(), next(), next()]).toEqual([...RNG_CHECK.first]);
  });
});
