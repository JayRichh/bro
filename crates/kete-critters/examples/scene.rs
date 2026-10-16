//! A filmstrip of the world running: one frame every `--every` seconds,
//! painted as the TUI and the desktop would see it, for eyeballing the
//! brain (walks, meetings, bubbles, particles).
//!
//!     cargo run -p kete-critters --example scene -- out.png [--seed 7] [--every 1] [--frames 24]
//!         [--summon kiwi,cat] [--hour 14] [--scale 2]

use kete_critters::paint::{paint, Canvas, Opts};
use kete_critters::world::{Input, Often, Pointer, Rect, World, STEP};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    // The first argument that isn't a flag or a flag's value.
    let out = (0..args.len())
        .find(|i| !args[*i].starts_with("--") && (*i == 0 || !args[i - 1].starts_with("--")))
        .map(|i| args[i].clone())
        .unwrap_or_else(|| "scene.png".into());
    let opt = |k: &str| {
        args.iter()
            .position(|a| a == k)
            .and_then(|i| args.get(i + 1))
            .cloned()
    };
    let seed: u32 = opt("--seed").and_then(|s| s.parse().ok()).unwrap_or(7);
    let every: f64 = opt("--every").and_then(|s| s.parse().ok()).unwrap_or(1.0);
    let frames: usize = opt("--frames").and_then(|s| s.parse().ok()).unwrap_or(24);
    let scale: i32 = opt("--scale").and_then(|s| s.parse().ok()).unwrap_or(2);
    let hour: u8 = opt("--hour").and_then(|s| s.parse().ok()).unwrap_or(14);
    let input = Input {
        view_w: 160.0,
        view_h: 60.0,
        floor: 56.0,
        hour,
        perches: vec![Rect {
            x: 90.0,
            y: 24.0,
            w: 50.0,
            h: 3.0,
        }],
        pointer: opt("--pointer")
            .and_then(|s| s.parse().ok())
            .map(|x| Pointer { x, y: 40.0 }),
        ..Input::default()
    };
    let mut w = World::new(seed);
    w.often = Often::Busy;
    if let Some(list) = opt("--summon") {
        w.spawning = false;
        for id in list.split(',') {
            w.summon(id, &input);
        }
    }
    let (vw, vh) = (input.view_w as i32, input.view_h as i32);
    let cols = 3;
    let rows = frames.div_ceil(cols) as i32;
    let tile_w = vw * scale + 6;
    let tile_h = vh * scale + 6;
    let mut sheet = Canvas::new((tile_w * cols as i32) as usize, (tile_h * rows) as usize);
    sheet.fill(
        0,
        0,
        sheet.w as i32,
        sheet.h as i32,
        [0x14, 0x12, 0x18],
        1.0,
    );
    for k in 0..frames {
        let mut t = 0.0;
        while t < every {
            w.tick(STEP, &input);
            t += STEP;
        }
        let mut c = Canvas::new(vw as usize, vh as usize);
        // A terminal-ish backdrop: dark panel, the footer row under the floor.
        c.fill(0, 0, vw, vh, [0x22, 0x26, 0x30], 1.0);
        c.fill(0, input.floor as i32, vw, vh, [0x3a, 0x34, 0x40], 1.0);
        for r in &input.perches {
            c.fill(
                r.x as i32,
                r.y as i32,
                r.w as i32,
                r.h as i32,
                [0x3a, 0x34, 0x40],
                1.0,
            );
        }
        paint(&w, &mut c, Opts::default());
        let ox = (k % cols) as i32 * tile_w + 3;
        let oy = (k / cols) as i32 * tile_h + 3;
        for y in 0..c.h {
            for x in 0..c.w {
                let p = c.get(x, y);
                sheet.fill(
                    ox + x as i32 * scale,
                    oy + y as i32 * scale,
                    scale,
                    scale,
                    [p[0], p[1], p[2]],
                    1.0,
                );
            }
        }
    }
    let file = std::fs::File::create(&out).expect("create the png");
    let mut enc = png::Encoder::new(
        std::io::BufWriter::new(file),
        sheet.w as u32,
        sheet.h as u32,
    );
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    let mut wr = enc.write_header().expect("png header");
    let data: Vec<u8> = sheet.px.iter().flatten().copied().collect();
    wr.write_image_data(&data).expect("png data");
    println!(
        "wrote {out} ({}x{}), {} critters at the end",
        sheet.w,
        sheet.h,
        w.critters.len()
    );
}
