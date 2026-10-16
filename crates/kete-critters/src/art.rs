//! The art model. Each species is authored as ASCII pixel maps in
//! `art/<group>.txt` (a format a person can draw in a text editor) and baked
//! once into [`Sprite`]s: palette-indexed frames per [`Pose`], the missing
//! poses derived, a 1 px outline added around every opaque pixel.
//!
//! The file format, one block per species:
//!
//! ```text
//! == kiwi                 the species id
//! size 14 9               authored width and height (before the outline)
//! pal b #8a6a45           a palette char and its colour
//! pal e #18121e           `e` and `g` are reserved: eye and eye glint
//! lid d                   the char the eye (and glint) become when closed
//! outline #1e1826         optional; `outline none` draws no outline
//! -- idle                 a frame: `size` rows of `size` chars
//! ..............          `.` is transparent
//! ```
//!
//! Frames face right; renderers mirror them for a critter walking left.
//! The bottom authored row is where the feet touch the floor.

use std::fmt;

pub type Rgb = [u8; 3];

/// The default outline: soot, warm and a little purple, never pure black.
pub const SOOT: Rgb = [0x1e, 0x18, 0x26];

/// Authored size limits (before the outline). Bodies are meant to be
/// roughly 10–16 px wide and 6–12 px tall; tails, ears and the moa's neck
/// get a little slack.
pub const MAX_W: usize = 20;
pub const MAX_H: usize = 16;
pub const MIN_W: usize = 5;
pub const MIN_H: usize = 4;

/// The pose a frame belongs to. The order is the generated table's.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Pose {
    Idle,
    Walk,
    Sit,
    Sleep,
    Eat,
    Fly,
    Held,
    Squash,
}

impl Pose {
    pub const ALL: [Pose; 8] = [
        Pose::Idle,
        Pose::Walk,
        Pose::Sit,
        Pose::Sleep,
        Pose::Eat,
        Pose::Fly,
        Pose::Held,
        Pose::Squash,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Pose::Idle => "idle",
            Pose::Walk => "walk",
            Pose::Sit => "sit",
            Pose::Sleep => "sleep",
            Pose::Eat => "eat",
            Pose::Fly => "fly",
            Pose::Held => "held",
            Pose::Squash => "squash",
        }
    }

    fn parse(s: &str) -> Option<Pose> {
        Pose::ALL.into_iter().find(|p| p.name() == s)
    }
}

/// One species as written in the art file, before baking.
#[derive(Debug, Clone)]
pub struct Authored {
    pub id: String,
    pub w: usize,
    pub h: usize,
    pub palette: Vec<(char, Rgb)>,
    pub lid: Option<char>,
    pub outline: Option<Rgb>,
    /// Frames in file order, each `h` rows of `w` chars.
    pub frames: Vec<(Pose, Vec<Vec<char>>)>,
}

impl Authored {
    pub fn authored(&self, pose: Pose) -> Vec<&Vec<Vec<char>>> {
        self.frames
            .iter()
            .filter(|(p, _)| *p == pose)
            .map(|(_, f)| f)
            .collect()
    }

    pub fn has_eyes(&self) -> bool {
        self.palette.iter().any(|(c, _)| *c == 'e')
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArtError {
    pub line: usize,
    pub msg: String,
}

impl fmt::Display for ArtError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "line {}: {}", self.line, self.msg)
    }
}

fn err(line: usize, msg: impl Into<String>) -> ArtError {
    ArtError {
        line,
        msg: msg.into(),
    }
}

pub fn parse_hex(s: &str) -> Option<Rgb> {
    let h = s.strip_prefix('#')?;
    if h.len() != 6 || !h.is_ascii() {
        return None;
    }
    let b = |i: usize| u8::from_str_radix(&h[i..i + 2], 16).ok();
    Some([b(0)?, b(2)?, b(4)?])
}

type Open = Option<(Pose, Vec<Vec<char>>, usize)>;

fn close_frame(out: &mut [Authored], frame: &mut Open) -> Result<(), ArtError> {
    if let Some((pose, rows, line)) = frame.take() {
        let sp = out
            .last_mut()
            .expect("a frame is only opened inside a species");
        if rows.len() != sp.h {
            return Err(err(
                line,
                format!(
                    "{} {}: {} rows, size says {}",
                    sp.id,
                    pose.name(),
                    rows.len(),
                    sp.h
                ),
            ));
        }
        sp.frames.push((pose, rows));
    }
    Ok(())
}

/// Parses an art file into its species blocks. Every structural problem
/// is an error with its line number: wrong row width or count, an
/// undefined palette char, a duplicate id, an unknown pose.
pub fn parse(src: &str) -> Result<Vec<Authored>, ArtError> {
    let mut out: Vec<Authored> = Vec::new();
    // The frame being read: (pose, rows, line it started on).
    let mut frame: Open = None;

    for (i, raw) in src.lines().enumerate() {
        let n = i + 1;
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some(id) = line.strip_prefix("==") {
            close_frame(&mut out, &mut frame)?;
            let id = id.trim().to_string();
            if id.is_empty() || out.iter().any(|a| a.id == id) {
                return Err(err(n, format!("missing or duplicate id {id:?}")));
            }
            out.push(Authored {
                id,
                w: 0,
                h: 0,
                palette: Vec::new(),
                lid: None,
                outline: Some(SOOT),
                frames: Vec::new(),
            });
            continue;
        }
        if out.is_empty() {
            return Err(err(n, "content before the first `== id`"));
        }
        if let Some(pose) = line.strip_prefix("--") {
            close_frame(&mut out, &mut frame)?;
            let sp = out.last().expect("checked above");
            if sp.w == 0 {
                return Err(err(n, format!("{}: `size` must come first", sp.id)));
            }
            let pose = Pose::parse(pose.trim())
                .ok_or_else(|| err(n, format!("unknown pose {:?}", pose.trim())))?;
            frame = Some((pose, Vec::new(), n));
            continue;
        }
        let sp = out.last_mut().expect("checked above");
        if let Some((_, rows, _)) = frame.as_mut() {
            let row: Vec<char> = line.chars().collect();
            if row.len() != sp.w {
                return Err(err(
                    n,
                    format!("{}: row is {} wide, size says {}", sp.id, row.len(), sp.w),
                ));
            }
            for c in &row {
                if *c != '.' && !sp.palette.iter().any(|(p, _)| p == c) {
                    return Err(err(n, format!("{}: {c:?} is not in the palette", sp.id)));
                }
            }
            rows.push(row);
            continue;
        }
        let mut words = line.split_whitespace();
        match words.next() {
            Some("size") => {
                let mut num = || words.next().and_then(|w| w.parse::<usize>().ok());
                let (w, h) = (num(), num());
                let (Some(w), Some(h)) = (w, h) else {
                    return Err(err(n, "size wants two numbers"));
                };
                sp.w = w;
                sp.h = h;
            }
            Some("pal") => {
                let c = words.next().and_then(|w| {
                    let mut cs = w.chars();
                    let c = cs.next()?;
                    cs.next().is_none().then_some(c)
                });
                let rgb = words.next().and_then(parse_hex);
                match (c, rgb) {
                    (Some(c), Some(rgb))
                        if c.is_ascii_graphic() && !matches!(c, '.' | '#' | '-' | '=') =>
                    {
                        if sp.palette.iter().any(|(p, _)| *p == c) {
                            return Err(err(n, format!("{}: {c:?} defined twice", sp.id)));
                        }
                        sp.palette.push((c, rgb));
                    }
                    _ => return Err(err(n, "pal wants a char and #rrggbb")),
                }
            }
            Some("lid") => {
                let c = words.next().and_then(|w| w.chars().next());
                match c {
                    Some(c) if sp.palette.iter().any(|(p, _)| *p == c) => sp.lid = Some(c),
                    _ => return Err(err(n, "lid wants a palette char defined above it")),
                }
            }
            Some("outline") => match words.next() {
                Some("none") => sp.outline = None,
                Some(h) => {
                    sp.outline = Some(parse_hex(h).ok_or_else(|| err(n, "bad outline colour"))?)
                }
                None => return Err(err(n, "outline wants #rrggbb or none")),
            },
            _ => return Err(err(n, format!("unknown line {line:?}"))),
        }
    }
    close_frame(&mut out, &mut frame)?;
    Ok(out)
}

/// A baked frame: `w * h` pixel values, row-major. 0 is transparent, 1 is
/// the outline, `v >= 2` is `palette[v - 1]` of its [`Sprite`].
pub type Frame = Vec<u8>;

/// A species' baked art: every pose's frames at one size, so a renderer
/// can swap frames without moving the anchor (bottom centre).
#[derive(Debug, Clone, PartialEq)]
pub struct Sprite {
    /// Baked size: the authored size plus the 1 px outline margin.
    pub w: usize,
    pub h: usize,
    /// `palette[0]` is the outline colour (or unused with no outline);
    /// `palette[i]` for i >= 1 is the authored palette in file order.
    pub palette: Vec<Rgb>,
    /// Pixel-value swaps that close the eyes: (from, to). Blink applies
    /// them for a beat; sleep frames are baked with them already applied.
    pub blink: Vec<(u8, u8)>,
    /// Frames per pose, indexed by `Pose as usize`. Every pose has at
    /// least one frame after baking (missing ones are derived or fall back
    /// to idle); `walk`, `fly` and `held` have two.
    pub poses: Vec<Vec<Frame>>,
}

impl Sprite {
    pub fn frame(&self, pose: Pose, f: usize) -> &Frame {
        let frames = &self.poses[pose as usize];
        &frames[f % frames.len()]
    }

    /// The colour of pixel value `v` with the eyes open or closed.
    pub fn color(&self, v: u8, closed: bool) -> Option<Rgb> {
        if v == 0 {
            return None;
        }
        let mut v = v;
        if closed {
            if let Some((_, to)) = self.blink.iter().find(|(from, _)| *from == v) {
                v = *to;
            }
        }
        self.palette.get(usize::from(v) - 1).copied()
    }

    /// The opaque rows' extent in a frame, top to bottom inclusive.
    pub fn opaque_rows(&self, frame: &Frame) -> Option<(usize, usize)> {
        let opaque = |y: &usize| frame[y * self.w..(y + 1) * self.w].iter().any(|v| *v != 0);
        let top = (0..self.h).find(opaque)?;
        let bottom = (0..self.h).rfind(opaque)?;
        Some((top, bottom))
    }
}

/// An authored grid as pixel values (no outline, no margin yet).
type Grid = Vec<Vec<u8>>;

fn to_grid(a: &Authored, rows: &[Vec<char>]) -> Grid {
    rows.iter()
        .map(|r| {
            r.iter()
                .map(|c| match a.palette.iter().position(|(p, _)| p == c) {
                    Some(i) => (i + 2) as u8,
                    None => 0,
                })
                .collect()
        })
        .collect()
}

/// The lowest row holding an eye pixel, if the species has eyes.
fn eye_row(grid: &Grid, eyes: &[u8]) -> Option<usize> {
    grid.iter()
        .rposition(|r| r.iter().any(|v| eyes.contains(v)))
}

fn close_eyes(grid: &mut Grid, swaps: &[(u8, u8)]) {
    for v in grid.iter_mut().flatten() {
        if let Some((_, to)) = swaps.iter().find(|(from, _)| from == v) {
            *v = *to;
        }
    }
}

/// Sleep from sit: eyes closed and everything from the eye line up sinks
/// one pixel, so the head droops onto the body. The row under the eye line
/// keeps its own pixels where the sinking ones are transparent.
fn derive_sleep(sit: &Grid, eyes: &[u8], swaps: &[(u8, u8)]) -> Grid {
    let h = sit.len();
    let line = eye_row(sit, eyes).unwrap_or(h / 3).min(h.saturating_sub(2));
    let mut g = sit.clone();
    for y in (1..=line + 1).rev() {
        for x in 0..g[y].len() {
            let above = sit[y - 1][x];
            if y == line + 1 {
                if above != 0 {
                    g[y][x] = above;
                }
            } else {
                g[y][x] = above;
            }
        }
    }
    g[0].iter_mut().for_each(|v| *v = 0);
    close_eyes(&mut g, swaps);
    g
}

/// Squash from idle: the middle of the body loses one or two rows (the
/// top comes down) and each row spreads a pixel wider where there's room.
fn derive_squash(idle: &Grid) -> Grid {
    let h = idle.len();
    let w = idle[0].len();
    let rows: Vec<usize> = (0..h)
        .filter(|y| idle[*y].iter().any(|v| *v != 0))
        .collect();
    let (top, bottom) = match (rows.first(), rows.last()) {
        (Some(t), Some(b)) => (*t, *b),
        _ => return idle.clone(),
    };
    let tall = bottom - top + 1;
    let cut = if tall >= 8 { 2 } else { 1 };
    let mid = top + tall / 2;
    // Rows (mid - cut, mid] go; everything above shifts down by `cut`.
    let mut g: Grid = vec![vec![0; w]; h];
    for (y, row) in g.iter_mut().enumerate() {
        if y > mid {
            *row = idle[y].clone();
        } else if y >= cut {
            *row = idle[y - cut].clone();
        }
    }
    // Spread: the left half moves a pixel left and the right half a pixel
    // right where the margin allows; the seam keeps its pixel.
    let half = w / 2;
    for row in g.iter_mut() {
        let src = row.clone();
        if src[0] == 0 {
            row[..half].copy_from_slice(&src[1..=half]);
        }
        if src[w - 1] == 0 {
            row[half + 1..].copy_from_slice(&src[half..w - 1]);
        }
    }
    g
}

/// The second held frame: the bottom two opaque rows (dangling legs)
/// shifted a pixel sideways, so the legs wiggle.
fn derive_wiggle(held: &Grid) -> Grid {
    let mut g = held.clone();
    let rows: Vec<usize> = (0..g.len())
        .filter(|y| g[*y].iter().any(|v| *v != 0))
        .collect();
    for y in rows.iter().rev().take(2) {
        let row = &held[*y];
        let w = row.len();
        if row[w - 1] == 0 {
            g[*y] = std::iter::once(0)
                .chain(row[..w - 1].iter().copied())
                .collect();
        }
    }
    g
}

/// Pads a grid by a pixel on every side and outlines it: a transparent
/// pixel touching an opaque one on any of its four sides becomes 1.
fn outline(grid: &Grid, with: bool) -> Frame {
    let (h, w) = (grid.len() + 2, grid[0].len() + 2);
    let mut px = vec![0u8; w * h];
    for (y, row) in grid.iter().enumerate() {
        for (x, v) in row.iter().enumerate() {
            px[(y + 1) * w + x + 1] = *v;
        }
    }
    if !with {
        return px;
    }
    let src = px.clone();
    for y in 0..h {
        for x in 0..w {
            if src[y * w + x] != 0 {
                continue;
            }
            let near = (x > 0 && src[y * w + x - 1] > 1)
                || (x + 1 < w && src[y * w + x + 1] > 1)
                || (y > 0 && src[(y - 1) * w + x] > 1)
                || (y + 1 < h && src[(y + 1) * w + x] > 1);
            if near {
                px[y * w + x] = 1;
            }
        }
    }
    px
}

/// Bakes one authored species: derives what wasn't drawn, adds the
/// outline.
pub fn bake(a: &Authored) -> Sprite {
    let value = |c: char| {
        a.palette
            .iter()
            .position(|(p, _)| *p == c)
            .map(|i| (i + 2) as u8)
    };
    let eyes: Vec<u8> = ['e', 'g'].into_iter().filter_map(value).collect();
    let swaps: Vec<(u8, u8)> = match a.lid.and_then(value) {
        Some(l) => eyes.iter().map(|e| (*e, l)).collect(),
        None => Vec::new(),
    };
    let grids = |pose: Pose| -> Vec<Grid> {
        a.authored(pose)
            .into_iter()
            .map(|rows| to_grid(a, rows))
            .collect()
    };
    let idle = grids(Pose::Idle)
        .into_iter()
        .next()
        .unwrap_or_else(|| vec![vec![0; a.w]; a.h]);
    let or_idle = |v: Vec<Grid>| if v.is_empty() { vec![idle.clone()] } else { v };

    let walk = match grids(Pose::Walk).as_slice() {
        [] => vec![idle.clone(), idle.clone()],
        [one] => vec![idle.clone(), one.clone()],
        many => many.to_vec(),
    };
    let sit = or_idle(grids(Pose::Sit));
    let sleep = match grids(Pose::Sleep).into_iter().next() {
        Some(mut g) => {
            close_eyes(&mut g, &swaps);
            g
        }
        None => derive_sleep(&sit[0], &eyes, &swaps),
    };
    let eat = or_idle(grids(Pose::Eat));
    let fly = match grids(Pose::Fly).as_slice() {
        // Non-fliers never fly; the walk frames stand in so a lookup
        // never comes back empty.
        [] => walk.clone(),
        [one] => vec![one.clone(), one.clone()],
        many => many.to_vec(),
    };
    let held = match grids(Pose::Held).as_slice() {
        [] => vec![idle.clone(), derive_wiggle(&idle)],
        [one] => vec![one.clone(), derive_wiggle(one)],
        many => many.to_vec(),
    };
    let squash = grids(Pose::Squash)
        .into_iter()
        .next()
        .unwrap_or_else(|| derive_squash(&idle));

    let with = a.outline.is_some();
    let bake_all = |v: &[Grid]| v.iter().map(|g| outline(g, with)).collect::<Vec<_>>();
    let mut palette = vec![a.outline.unwrap_or(SOOT)];
    palette.extend(a.palette.iter().map(|(_, rgb)| *rgb));
    Sprite {
        w: a.w + 2,
        h: a.h + 2,
        palette,
        blink: swaps,
        poses: vec![
            bake_all(std::slice::from_ref(&idle)),
            bake_all(&walk),
            bake_all(&sit),
            bake_all(&[sleep]),
            bake_all(&eat),
            bake_all(&fly),
            bake_all(&held),
            bake_all(&[squash]),
        ],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TINY: &str = "
== dot
size 5 4
pal b #808080
pal e #101010
lid b
-- idle
.....
.bbe.
.bbb.
.b.b.
-- sit
.....
.....
.bbe.
.bbb.
";

    #[test]
    fn parses_and_bakes_a_tiny_species() {
        let a = parse(TINY).unwrap();
        assert_eq!(a.len(), 1);
        let s = bake(&a[0]);
        assert_eq!((s.w, s.h), (7, 6));
        // Outline surrounds the body: the pixel left of the top-left body
        // pixel is outline, the far corner is transparent.
        let idle = s.frame(Pose::Idle, 0);
        assert_eq!(idle[2 * 7 + 1], 1);
        assert_eq!(idle[0], 0);
        // Sleep comes from sit with the eye closed.
        let sleep = s.frame(Pose::Sleep, 0);
        assert!(!sleep.contains(&3), "the eye value is swapped for the lid");
        assert_eq!(s.blink, vec![(3, 2)]);
        // Every pose resolves to a frame.
        for p in Pose::ALL {
            assert_eq!(s.frame(p, 1).len(), 7 * 6);
        }
    }

    #[test]
    fn refuses_bad_rows_and_colours() {
        let wide = TINY.replace(".bbe.\n.bbb.\n.b.b.", ".bbe..\n.bbb.\n.b.b.");
        assert!(parse(&wide).unwrap_err().msg.contains("wide"));
        let unknown = TINY.replace(".b.b.", ".x.b.");
        assert!(parse(&unknown).unwrap_err().msg.contains("palette"));
        let short = TINY.replace(".b.b.\n-- sit", "-- sit");
        assert!(parse(&short).unwrap_err().msg.contains("rows"));
    }
}
