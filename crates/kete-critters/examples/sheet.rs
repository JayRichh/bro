//! The contact sheet: every species in every pose at 4x on a mid-dark
//! background with its name, then the same frames at 1x on dark and on
//! light, so the art can be eyeballed at the size the terminal shows it.
//!
//!     cargo run -p kete-critters --example sheet -- out.png [--origin aotearoa] [--scale 4]

use kete_critters::art::{Pose, Rgb};
use kete_critters::cast::{cast, sprite};
use kete_critters::font;
use kete_critters::paint::{blit, text, Canvas};
use kete_critters::species::{flag, Origin, SPECIES};

const DARK: Rgb = [0x2a, 0x2f, 0x3a];
const LIGHT: Rgb = [0xef, 0xe9, 0xdc];
const LABEL: Rgb = [0xc8, 0xc8, 0xd4];
const DIM: Rgb = [0x8a, 0x8f, 0x9c];

/// The frames shown per species: (label, pose, frame, eyes closed).
fn frames(flier: bool) -> Vec<(&'static str, Pose, usize, bool)> {
    let mut v = vec![
        ("idle", Pose::Idle, 0, false),
        ("blink", Pose::Idle, 0, true),
        ("walk", Pose::Walk, 0, false),
        ("walk", Pose::Walk, 1, false),
        ("sit", Pose::Sit, 0, false),
        ("sleep", Pose::Sleep, 0, true),
        ("eat", Pose::Eat, 0, false),
        ("held", Pose::Held, 0, false),
        ("held", Pose::Held, 1, false),
        ("squash", Pose::Squash, 0, false),
    ];
    if flier {
        v.push(("fly", Pose::Fly, 0, false));
        v.push(("fly", Pose::Fly, 1, false));
    }
    v
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    // The first argument that isn't a flag or a flag's value.
    let out = (0..args.len())
        .find(|i| !args[*i].starts_with("--") && (*i == 0 || !args[i - 1].starts_with("--")))
        .map(|i| args[i].clone())
        .unwrap_or_else(|| "critters-sheet.png".into());
    let opt = |k: &str| {
        args.iter()
            .position(|a| a == k)
            .and_then(|i| args.get(i + 1))
            .cloned()
    };
    let scale: i32 = opt("--scale").and_then(|s| s.parse().ok()).unwrap_or(4);
    let origin = opt("--origin");
    let missing = &cast().missing;
    if !missing.is_empty() {
        eprintln!("no art yet for: {}", missing.join(", "));
    }

    let rows: Vec<usize> = (0..SPECIES.len())
        .filter(|i| {
            origin
                .as_deref()
                .is_none_or(|o| SPECIES[*i].origin.name() == o)
        })
        .collect();
    let cell_w = rows.iter().map(|i| sprite(*i).w).max().unwrap_or(16) as i32 * scale + 6;
    let max_frames = 12;
    let small_w = rows.iter().map(|i| sprite(*i).w).max().unwrap_or(16) as i32 + 3;
    let width = 8 + cell_w * max_frames + 16 + small_w * max_frames * 2 + 24;
    let label_h = 2 * (font::CAP + font::MACRON_RISE) as i32 + 6;
    let mut height = 8;
    let mut last_origin: Option<Origin> = None;
    for i in &rows {
        if last_origin != Some(SPECIES[*i].origin) {
            height += 22;
            last_origin = Some(SPECIES[*i].origin);
        }
        height += label_h + sprite(*i).h as i32 * scale + 10;
    }
    let mut c = Canvas::new(width as usize, height as usize);
    c.fill(0, 0, width, height, DARK, 1.0);

    let mut y = 8;
    last_origin = None;
    for i in rows {
        let sp = &SPECIES[i];
        let s = sprite(i);
        if last_origin != Some(sp.origin) {
            big_text(&mut c, 8, y + 4, &sp.origin.label().to_uppercase(), DIM, 2);
            y += 22;
            last_origin = Some(sp.origin);
        }
        let mut tags = String::new();
        for (bit, name) in flag::ALL {
            if sp.flags & bit != 0 {
                tags.push(' ');
                tags.push_str(name);
            }
        }
        if sp.weight <= 2.0 && sp.weight > 0.0 {
            tags.push_str(" rare");
        }
        let label = format!("{}  {}  {}x{}{}", sp.name, sp.id, s.w - 2, s.h - 2, tags);
        big_text(&mut c, 8, y + 2, &label, LABEL, 2);
        y += label_h;
        let fl = frames(sp.has(flag::FLIER));
        let h = s.h as i32;
        // 4x
        for (k, (_, pose, f, closed)) in fl.iter().enumerate() {
            let x0 = 8 + k as i32 * cell_w;
            let mut tile = Canvas::new(s.w, s.h);
            blit(&mut tile, i, *pose, *f, *closed, false, 0, 0, 1.0, false, h);
            for ty in 0..s.h {
                for tx in 0..s.w {
                    let p = tile.get(tx, ty);
                    if p[3] > 0 {
                        c.fill(
                            x0 + tx as i32 * scale,
                            y + ty as i32 * scale,
                            scale,
                            scale,
                            [p[0], p[1], p[2]],
                            1.0,
                        );
                    }
                }
            }
        }
        // 1x on dark, then on light.
        let sx = 8 + cell_w * max_frames + 16;
        let lx = sx + small_w * max_frames + 8;
        c.fill(
            lx - 4,
            y - 4,
            small_w * fl.len() as i32 + 6,
            h + 8,
            LIGHT,
            1.0,
        );
        for (k, (_, pose, f, closed)) in fl.iter().enumerate() {
            for base in [sx, lx] {
                blit(
                    &mut c,
                    i,
                    *pose,
                    *f,
                    *closed,
                    false,
                    base + k as i32 * small_w,
                    y,
                    1.0,
                    false,
                    y + h,
                );
            }
        }
        y += h * scale + 10;
    }
    write_png(&out, &c);
    println!("wrote {out} ({}x{})", c.w, c.h);
}

/// Pixel-font text at an integer scale.
fn big_text(c: &mut Canvas, x: i32, y: i32, s: &str, rgb: Rgb, k: i32) {
    let mut t = Canvas::new(font::text_width(s) + 2, font::text_height(s) + 2);
    text(&mut t, 0, 0, s, rgb, None, 1.0);
    for ty in 0..t.h {
        for tx in 0..t.w {
            let p = t.get(tx, ty);
            if p[3] > 0 {
                c.fill(x + tx as i32 * k, y + ty as i32 * k, k, k, rgb, 1.0);
            }
        }
    }
}

fn write_png(path: &str, c: &Canvas) {
    let file = std::fs::File::create(path).expect("create the png");
    let mut enc = png::Encoder::new(std::io::BufWriter::new(file), c.w as u32, c.h as u32);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    let mut w = enc.write_header().expect("png header");
    let data: Vec<u8> = c.px.iter().flatten().copied().collect();
    w.write_image_data(&data).expect("png data");
}
