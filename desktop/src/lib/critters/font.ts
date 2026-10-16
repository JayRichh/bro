// The cast's 3x5 pixel font, as kete-critters' `font.rs` sets it: capitals
// only (lower case maps up), a macron vowel drawn as its capital with a bar
// two rows above the cap. No web fonts: the CSP blocks them.

import { FONT, GLYPHS, MACRONS } from './cast.gen';

type Glyph = { w: number; rows: readonly string[] };

/** A char's glyph and whether it carries a macron (unknown chars: none). */
function glyph(ch: string): [Glyph, boolean] | null {
  const base = MACRONS[ch];
  if (base) return [GLYPHS[base], true];
  // Rust's to_ascii_uppercase: ASCII letters only.
  const up = ch >= 'a' && ch <= 'z' ? ch.toUpperCase() : ch;
  const g = GLYPHS[up];
  return g ? [g, false] : null;
}

const orQuestion = (ch: string): [Glyph, boolean] => glyph(ch) ?? [GLYPHS['?'], false];

/** Width in pixels of `s` set in the font (unknown chars count as `?`). */
export function textWidth(s: string): number {
  let w = 0;
  let i = 0;
  for (const ch of s) {
    if (i++ > 0) w += FONT.spacing;
    w += orQuestion(ch)[0].w;
  }
  return w;
}

/** 5 rows, or 7 when a macron rises above the cap. */
export function textHeight(s: string): number {
  for (const ch of s) if (glyph(ch)?.[1]) return FONT.cap + FONT.macronRise;
  return FONT.cap;
}

/** The pixels of `s` as [x, y] offsets from its line box's top-left. */
export function textPixels(s: string): [number, number][] {
  const rise = textHeight(s) - FONT.cap;
  const out: [number, number][] = [];
  let x0 = 0;
  for (const ch of s) {
    const [g, macron] = orQuestion(ch);
    g.rows.forEach((row, y) => {
      for (let x = 0; x < row.length; x++) if (row[x] === '#') out.push([x0 + x, rise + y]);
    });
    if (macron) for (let x = 0; x < g.w; x++) out.push([x0 + x, 0]);
    x0 += g.w + FONT.spacing;
  }
  return out;
}
