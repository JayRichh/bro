//! Painting the world into an RGBA pixel buffer, one sprite pixel per
//! buffer pixel. The TUI turns the buffer into half-block cells; the sheet
//! example writes it out as a PNG. (The desktop draws with canvas calls of
//! its own, from the same data.)

use crate::art::{Pose, Rgb};
use crate::cast::sprite;
use crate::font::{self, Icon};
use crate::world::{ink, Kind, World};

/// Straight (not premultiplied) RGBA; alpha 0 is transparent.
#[derive(Clone, Debug, PartialEq)]
pub struct Canvas {
    pub w: usize,
    pub h: usize,
    pub px: Vec<[u8; 4]>,
}

impl Canvas {
    pub fn new(w: usize, h: usize) -> Canvas {
        Canvas {
            w,
            h,
            px: vec![[0; 4]; w * h],
        }
    }

    pub fn clear(&mut self) {
        self.px.iter_mut().for_each(|p| *p = [0; 4]);
    }

    pub fn get(&self, x: usize, y: usize) -> [u8; 4] {
        self.px[y * self.w + x]
    }

    /// Source-over: `rgb` at `alpha` (0..=1) onto whatever is there.
    pub fn blend(&mut self, x: i32, y: i32, rgb: Rgb, alpha: f64) {
        if x < 0 || y < 0 || x >= self.w as i32 || y >= self.h as i32 || alpha <= 0.0 {
            return;
        }
        let a = alpha.min(1.0);
        let i = y as usize * self.w + x as usize;
        let d = self.px[i];
        let da = f64::from(d[3]) / 255.0;
        let oa = a + da * (1.0 - a);
        if oa <= 0.0 {
            return;
        }
        let mix = |s: u8, d: u8| {
            ((f64::from(s) * a + f64::from(d) * da * (1.0 - a)) / oa)
                .round()
                .clamp(0.0, 255.0) as u8
        };
        self.px[i] = [
            mix(rgb[0], d[0]),
            mix(rgb[1], d[1]),
            mix(rgb[2], d[2]),
            (oa * 255.0).round() as u8,
        ];
    }

    pub fn fill(&mut self, x: i32, y: i32, w: i32, h: i32, rgb: Rgb, alpha: f64) {
        for j in y..y + h {
            for i in x..x + w {
                self.blend(i, j, rgb, alpha);
            }
        }
    }
}

/// What to paint besides sprites and particles.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Opts {
    /// Speech bubbles and the hovered critter's name tag, in the pixel
    /// font. A renderer that sets real text (the TUI) turns this off and
    /// draws them itself.
    pub text: bool,
}

impl Default for Opts {
    fn default() -> Self {
        Opts { text: true }
    }
}

/// Draws one baked frame with its top-left at (left, top). Rows at or below
/// `clip` are skipped.
#[allow(clippy::too_many_arguments)]
pub fn blit(
    canvas: &mut Canvas,
    species: usize,
    pose: Pose,
    frame: usize,
    closed: bool,
    flip: bool,
    left: i32,
    top: i32,
    alpha: f64,
    flash: bool,
    clip: i32,
) {
    let s = sprite(species);
    let f = s.frame(pose, frame);
    for y in 0..s.h {
        let py = top + y as i32;
        if py >= clip {
            break;
        }
        for x in 0..s.w {
            let v = f[y * s.w + x];
            let Some(mut rgb) = s.color(v, closed) else {
                continue;
            };
            if flash && v > 1 {
                rgb = ink::FLASH;
            }
            let fx = if flip { s.w - 1 - x } else { x };
            canvas.blend(left + fx as i32, py, rgb, alpha);
        }
    }
}

/// Text in the pixel font with its top-left at (x, y), with an optional
/// 1 px outline around the strokes.
pub fn text(
    canvas: &mut Canvas,
    x: i32,
    y: i32,
    s: &str,
    rgb: Rgb,
    outline: Option<Rgb>,
    alpha: f64,
) {
    let px = font::text_pixels(s);
    if let Some(o) = outline {
        let mut ring: Vec<(i32, i32)> = Vec::new();
        for (dx, dy) in &px {
            for (ox, oy) in [(-1, 0), (1, 0), (0, -1), (0, 1)] {
                let p = (*dx as i32 + ox, *dy as i32 + oy);
                if !px.contains(&(p.0.max(0) as usize, p.1.max(0) as usize)) || p.0 < 0 || p.1 < 0 {
                    ring.push(p);
                }
            }
        }
        ring.sort_unstable();
        ring.dedup();
        for (dx, dy) in ring {
            canvas.blend(x + dx, y + dy, o, alpha);
        }
    }
    for (dx, dy) in px {
        canvas.blend(x + dx as i32, y + dy as i32, rgb, alpha);
    }
}

/// An icon with its bottom-centre at (x, y), outlined in soot.
pub fn icon(canvas: &mut Canvas, icon: Icon, x: i32, y: i32, alpha: f64) {
    let rows = icon.rows();
    let (w, h) = icon.size();
    let left = x - w as i32 / 2;
    let top = y - h as i32;
    let on = |i: i32, j: i32| {
        i >= 0
            && j >= 0
            && (i as usize) < w
            && (j as usize) < h
            && rows[j as usize].as_bytes()[i as usize] == b'X'
    };
    for j in -1..=h as i32 {
        for i in -1..=w as i32 {
            if on(i, j) {
                canvas.blend(left + i, top + j, icon.color(), alpha);
            } else if on(i - 1, j) || on(i + 1, j) || on(i, j - 1) || on(i, j + 1) {
                canvas.blend(left + i, top + j, ink::SOOT, alpha);
            }
        }
    }
}

/// The icon an item on the floor shows.
pub fn item_icon(name: &str) -> Icon {
    match name {
        "cowhide" => Icon::Hide,
        "feather" => Icon::Feather,
        _ => Icon::Star,
    }
}

/// A speech bubble whose tail points at (hx, top): bone fill, soot border
/// with clipped corners, ink text.
pub fn bubble(canvas: &mut Canvas, hx: i32, top: i32, s: &str, view_w: i32, alpha: f64) {
    let tw = font::text_width(s) as i32;
    let th = font::text_height(s) as i32;
    let bw = tw + 4;
    let bh = th + 4;
    let bx = (hx - bw / 2).clamp(1, (view_w - bw - 1).max(1));
    let by = top - bh - 2;
    // Border: a rect one larger all round, corners left out.
    canvas.fill(bx - 1, by, bw + 2, bh, ink::SOOT, alpha);
    canvas.fill(bx, by - 1, bw, bh + 2, ink::SOOT, alpha);
    canvas.fill(bx, by, bw, bh, ink::BUBBLE, alpha);
    // The tail.
    let tx = hx.clamp(bx + 1, bx + bw - 2);
    canvas.fill(tx, by + bh, 1, 2, ink::SOOT, alpha);
    text(canvas, bx + 2, by + 2, s, ink::TEXT, None, alpha);
}

/// Paints the whole world (particles, critters, bubbles) into `canvas`,
/// whose size is the world's view.
pub fn paint(world: &World, canvas: &mut Canvas, opts: Opts) {
    let floor = world.floor;
    let ground = |rise: f64| (floor - rise).round() as i32;
    // On the floor, under the critters: trails, items, the loot beam.
    for p in &world.particles {
        let a = p.alpha();
        let x = p.x.round() as i32;
        match p.kind {
            Kind::Trail => canvas.fill(x, ground(0.0) - 1, 2, 1, p.color, 0.35 * a),
            Kind::Beam => {
                let y1 = ground(0.0);
                for y in 0..y1 {
                    let fade = f64::from(y) / f64::from(y1.max(1));
                    canvas.blend(x - 1, y, p.color, 0.18 * a * fade);
                    canvas.blend(x, y, p.color, 0.45 * a * fade);
                    canvas.blend(x + 1, y, p.color, 0.18 * a * fade);
                }
                icon(canvas, Icon::Spark, x, y1, a);
            }
            Kind::Item if p.carried_by.is_none() => {
                icon(canvas, item_icon(&p.text), x, ground(p.rise), a)
            }
            _ => {}
        }
    }
    for d in world.sprites() {
        blit(
            canvas, d.species, d.pose, d.frame, d.closed, d.flip, d.left, d.top, d.alpha, d.flash,
            d.clip,
        );
    }
    for p in &world.particles {
        let a = p.alpha();
        let x = p.x.round() as i32;
        let y = ground(p.rise);
        match p.kind {
            Kind::Dust => canvas.blend(x, y - 1, p.color, 0.6 * a),
            Kind::Sparkle => {
                canvas.blend(x, y, p.color, a);
                if p.life > p.max * 0.5 {
                    for (dx, dy) in [(-1, 0), (1, 0), (0, -1), (0, 1)] {
                        canvas.blend(x + dx, y + dy, p.color, 0.5 * a);
                    }
                }
            }
            Kind::Crumb | Kind::Confetti => canvas.blend(x, y - 1, p.color, a),
            Kind::Smoke => canvas.fill(x, y - 2, 2, 2, p.color, 0.7 * a),
            Kind::Dart => canvas.fill(x, y - 1, 2, 1, p.color, a),
            Kind::Icon(i) => icon(canvas, i, x, y, a),
            Kind::Item if p.carried_by.is_some() => icon(canvas, item_icon(&p.text), x, y, a),
            Kind::Text if opts.text => {
                let w = font::text_width(&p.text) as i32;
                let h = font::text_height(&p.text) as i32;
                text(
                    canvas,
                    x - w / 2,
                    y - h,
                    &p.text,
                    p.color,
                    Some(ink::SOOT),
                    a,
                );
            }
            _ => {}
        }
    }
    if !opts.text {
        return;
    }
    let view_w = world.view_w.round() as i32;
    for c in &world.critters {
        let Some(b) = &c.bubble else { continue };
        let (hx, _) = c.head();
        let top = (floor - c.lift).round() as i32 - sprite(c.species).h as i32;
        bubble(
            canvas,
            hx.round() as i32,
            top,
            &b.text,
            view_w,
            (b.t * 3.0).min(1.0) * c.alpha.max(0.4),
        );
    }
    if let Some(c) = world.hover.and_then(|id| world.get(id)) {
        if c.bubble.is_none() {
            let w = font::text_width(&c.name) as i32;
            let h = font::text_height(&c.name) as i32;
            let top = (floor - c.lift).round() as i32 - sprite(c.species).h as i32;
            let x = (c.x.round() as i32 - w / 2).clamp(1, (view_w - w - 1).max(1));
            text(
                canvas,
                x,
                top - h - 2,
                &c.name,
                ink::BONE,
                Some(ink::SOOT),
                1.0,
            );
        }
    }
}
