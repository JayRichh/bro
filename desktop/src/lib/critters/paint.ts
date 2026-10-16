// Painting the world, the way kete-critters' `paint.rs` paints its pixel
// buffer, but with canvas calls: one canvas pixel per sprite pixel, scaled
// up by CSS (image-rendering: pixelated). The canvas is exactly as tall as
// the floor line, so sprite rows at or below the floor are clipped for
// free. Frames, icons and words are built once into small canvases and
// drawn with drawImage.

import { ICONS, INK, SPECIES, type IconName, type Pose } from './cast.gen';
import { textHeight, textPixels, textWidth } from './font';
import { framePixels, head, particleAlpha, rround, type Particle, type SpriteDraw, type World } from './world';

type Canvasish = HTMLCanvasElement | OffscreenCanvas;
type Ctx = CanvasRenderingContext2D | OffscreenCanvasRenderingContext2D;

function makeCanvas(w: number, h: number): Canvasish {
  if (typeof OffscreenCanvas === 'function') return new OffscreenCanvas(w, h);
  const c = document.createElement('canvas');
  c.width = w;
  c.height = h;
  return c;
}

const rgbCache = new Map<string, [number, number, number]>();
function rgb(hex: string): [number, number, number] {
  let v = rgbCache.get(hex);
  if (!v) {
    const n = parseInt(hex.slice(1), 16);
    v = [(n >> 16) & 255, (n >> 8) & 255, n & 255];
    rgbCache.set(hex, v);
  }
  return v;
}

/** A small bitmap from a pixel function: `at(x, y)` gives a colour or null. */
function bitmap(w: number, h: number, at: (x: number, y: number) => string | null): Canvasish {
  const c = makeCanvas(Math.max(1, w), Math.max(1, h));
  const g = c.getContext('2d') as Ctx;
  const img = g.createImageData(Math.max(1, w), Math.max(1, h));
  for (let y = 0; y < h; y++) {
    for (let x = 0; x < w; x++) {
      const col = at(x, y);
      if (!col) continue;
      const [r, gr, b] = rgb(col);
      const i = (y * w + x) * 4;
      img.data[i] = r;
      img.data[i + 1] = gr;
      img.data[i + 2] = b;
      img.data[i + 3] = 255;
    }
  }
  g.putImageData(img, 0, 0);
  return c;
}

const frames = new Map<string, Canvasish>();
/** One baked frame: eyes closed swap the blink values, a flash draws every
 *  opaque non-outline pixel in INK.flash, `flip` mirrors it. */
export function frameBitmap(species: number, pose: Pose, f: number, closed: boolean, flash: boolean, flip: boolean): Canvasish {
  const key = `${species}/${pose}/${f}/${+closed}${+flash}${+flip}`;
  let c = frames.get(key);
  if (!c) {
    const { w, h, palette, blink } = SPECIES[species].sprite;
    const px = framePixels(species, pose, f);
    c = bitmap(w, h, (x, y) => {
      const v = px[y * w + (flip ? w - 1 - x : x)];
      if (v === 0) return null;
      if (flash && v > 1) return INK.flash;
      const shut = closed ? blink.find(([from]) => from === v) : undefined;
      return palette[(shut ? shut[1] : v) - 1] ?? null;
    });
    frames.set(key, c);
  }
  return c;
}

const icons = new Map<IconName, Canvasish>();
/** An icon with a soot ring, one pixel of margin all round. */
function iconBitmap(name: IconName): Canvasish {
  let c = icons.get(name);
  if (!c) {
    const { color, rows } = ICONS[name];
    const w = rows[0].length;
    const h = rows.length;
    const on = (i: number, j: number) => i >= 0 && j >= 0 && i < w && j < h && rows[j][i] === 'X';
    c = bitmap(w + 2, h + 2, (x, y) => {
      const [i, j] = [x - 1, y - 1];
      if (on(i, j)) return color;
      if (on(i - 1, j) || on(i + 1, j) || on(i, j - 1) || on(i, j + 1)) return INK.soot;
      return null;
    });
    icons.set(name, c);
  }
  return c;
}

const words = new Map<string, Canvasish>();
/** Words in the pixel font, with an optional ring, one pixel of margin. */
function textBitmap(s: string, color: string, outline: string | null): Canvasish {
  const key = `${s}\u0000${color}\u0000${outline ?? ''}`;
  let c = words.get(key);
  if (!c) {
    if (words.size > 256) words.clear();
    const w = textWidth(s);
    const h = textHeight(s);
    const on = new Set(textPixels(s).map(([x, y]) => y * 1024 + x));
    const ink = (x: number, y: number) => x >= 0 && y >= 0 && on.has(y * 1024 + x);
    c = bitmap(w + 2, h + 2, (x, y) => {
      const [i, j] = [x - 1, y - 1];
      if (ink(i, j)) return color;
      if (outline && (ink(i - 1, j) || ink(i + 1, j) || ink(i, j - 1) || ink(i, j + 1))) return outline;
      return null;
    });
    words.set(key, c);
  }
  return c;
}

function fill(g: Ctx, x: number, y: number, w: number, h: number, color: string, alpha: number) {
  if (alpha <= 0) return;
  g.globalAlpha = Math.min(alpha, 1);
  g.fillStyle = color;
  g.fillRect(x, y, w, h);
}

function blit(g: Ctx, c: Canvasish, x: number, y: number, alpha: number) {
  if (alpha <= 0) return;
  g.globalAlpha = Math.min(alpha, 1);
  g.drawImage(c, x, y);
}

/** An icon with its bottom-centre at (x, y). */
function icon(g: Ctx, name: IconName, x: number, y: number, alpha: number) {
  const rows = ICONS[name].rows;
  const left = x - Math.floor(rows[0].length / 2);
  const top = y - rows.length;
  blit(g, iconBitmap(name), left - 1, top - 1, alpha);
}

function text(g: Ctx, x: number, y: number, s: string, color: string, outline: string | null, alpha: number) {
  blit(g, textBitmap(s, color, outline), x - 1, y - 1, alpha);
}

/** The icon an item on the floor shows. */
export function itemIcon(name: string): IconName {
  return name === 'cowhide' ? 'hide' : name === 'feather' ? 'feather' : 'star';
}

/** A speech bubble's box for a tail at (hx, top): kept inside the view. */
export function bubbleBox(hx: number, top: number, s: string, viewW: number) {
  const bw = textWidth(s) + 4;
  const bh = textHeight(s) + 4;
  const bx = Math.min(Math.max(hx - Math.trunc(bw / 2), 1), Math.max(viewW - bw - 1, 1));
  const by = top - bh - 2;
  return { bx, by, bw, bh };
}

function bubble(g: Ctx, hx: number, top: number, s: string, viewW: number, alpha: number) {
  const { bx, by, bw, bh } = bubbleBox(hx, top, s, viewW);
  // Border: a rect one larger all round, corners left out.
  fill(g, bx - 1, by, bw + 2, bh, INK.soot, alpha);
  fill(g, bx, by - 1, bw, bh + 2, INK.soot, alpha);
  fill(g, bx, by, bw, bh, INK.bubble, alpha);
  // The tail.
  const tx = Math.min(Math.max(hx, bx + 1), bx + bw - 2);
  fill(g, tx, by + bh, 1, 2, INK.soot, alpha);
  text(g, bx + 2, by + 2, s, INK.text, null, alpha);
}

function beam(g: Ctx, p: Particle, x: number, y1: number, a: number) {
  if (y1 > 0) {
    const [r, gr, b] = rgb(p.color);
    for (const [dx, k] of [
      [-1, 0.18],
      [0, 0.45],
      [1, 0.18],
    ] as const) {
      const grad = g.createLinearGradient(0, 0, 0, y1);
      grad.addColorStop(0, `rgba(${r},${gr},${b},0)`);
      grad.addColorStop(1, `rgba(${r},${gr},${b},${k * a})`);
      g.globalAlpha = 1;
      g.fillStyle = grad;
      g.fillRect(x + dx, 0, 1, y1);
    }
  }
  icon(g, 'spark', x, y1, a);
}

/** Paints the whole world into `g`, whose canvas is the view's width and
 *  the floor's height, in sprite pixels. */
export function paint(g: Ctx, world: World) {
  const floor = world.floor;
  const ground = (rise: number) => rround(floor - rise);
  g.globalAlpha = 1;
  g.clearRect(0, 0, g.canvas.width, g.canvas.height);
  // On the floor, under the critters: trails, items, the loot beam.
  for (const p of world.particles) {
    const a = particleAlpha(p);
    const x = rround(p.x);
    if (p.kind === 'trail') fill(g, x, ground(0) - 1, 2, 1, p.color, 0.35 * a);
    else if (p.kind === 'beam') beam(g, p, x, ground(0), a);
    else if (p.kind === 'item' && p.carriedBy === null) icon(g, itemIcon(p.text), x, ground(p.rise), a);
  }
  for (const d of world.sprites()) drawSprite(g, d);
  for (const p of world.particles) {
    const a = particleAlpha(p);
    const x = rround(p.x);
    const y = ground(p.rise);
    switch (p.kind) {
      case 'dust':
        fill(g, x, y - 1, 1, 1, p.color, 0.6 * a);
        break;
      case 'sparkle':
        fill(g, x, y, 1, 1, p.color, a);
        if (p.life > p.max * 0.5) {
          for (const [dx, dy] of [
            [-1, 0],
            [1, 0],
            [0, -1],
            [0, 1],
          ])
            fill(g, x + dx, y + dy, 1, 1, p.color, 0.5 * a);
        }
        break;
      case 'crumb':
      case 'confetti':
        fill(g, x, y - 1, 1, 1, p.color, a);
        break;
      case 'smoke':
        fill(g, x, y - 2, 2, 2, p.color, 0.7 * a);
        break;
      case 'dart':
        fill(g, x, y - 1, 2, 1, p.color, a);
        break;
      case 'icon':
        if (p.icon) icon(g, p.icon, x, y, a);
        break;
      case 'item':
        if (p.carriedBy !== null) icon(g, itemIcon(p.text), x, y, a);
        break;
      case 'text': {
        const w = textWidth(p.text);
        const h = textHeight(p.text);
        text(g, x - Math.trunc(w / 2), y - h, p.text, p.color, INK.soot, a);
        break;
      }
    }
  }
  const viewW = rround(world.viewW);
  for (const c of world.critters) {
    if (!c.bubble) continue;
    const [hx] = head(c);
    const top = rround(floor - c.lift) - SPECIES[c.species].sprite.h;
    bubble(g, rround(hx), top, c.bubble.text, viewW, Math.min(c.bubble.t * 3, 1) * Math.max(c.alpha, 0.4));
  }
  const hov = world.hover === null ? undefined : world.get(world.hover);
  if (hov && !hov.bubble) {
    const w = textWidth(hov.name);
    const h = textHeight(hov.name);
    const top = rround(floor - hov.lift) - SPECIES[hov.species].sprite.h;
    const x = Math.min(Math.max(rround(hov.x) - Math.trunc(w / 2), 1), Math.max(viewW - w - 1, 1));
    text(g, x, top - h - 2, hov.name, INK.bone, INK.soot, 1);
  }
  g.globalAlpha = 1;
}

function drawSprite(g: Ctx, d: SpriteDraw) {
  blit(g, frameBitmap(d.species, d.pose, d.frame, d.closed, d.flash, d.flip), d.left, d.top, d.alpha);
}
