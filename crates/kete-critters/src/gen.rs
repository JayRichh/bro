//! `desktop/src/lib/critters/cast.gen.ts`: the cast as the desktop imports
//! it. The art is baked here (outline, derived poses), so the desktop draws
//! exactly these pixels; the tables and constants are the brain's, so its
//! TypeScript port runs on the same numbers. The `cast_gen_ts_is_current`
//! test fails when the committed file and this differ;
//! `KETE_BLESS=1 cargo test -p kete-critters cast_gen` rewrites it.

use std::fmt::Write as _;

use crate::art::{Pose, Rgb};
use crate::cast::sprite;
use crate::font::{Icon, CAP, GLYPHS, MACRON_RISE, SPACING};
use crate::rng::{hash_str, Rng};
use crate::species::{flag, Gait, Origin, Personality, NAMES, PREFIXES, RELATIONS, SPECIES};
use crate::world::{self, ink, Input, Often, State, World, MOODS, NEXT};

/// Where the file lives, from this crate's manifest directory.
pub const TS_PATH: &str = "../../desktop/src/lib/critters/cast.gen.ts";

const TS_HEAD: &str = "\
// Generated from crates/kete-critters: do not edit. The kete-critters test
// cast_gen_ts_is_current fails when this file and the crate differ;
// KETE_BLESS=1 cargo test -p kete-critters cast_gen rewrites it.
// The data contract is in ./README.md.

";

/// A TypeScript string literal, ASCII only (`\\u` escapes otherwise, UTF-16
/// like JavaScript).
pub fn ts_str(s: &str) -> String {
    let mut out = String::from("\"");
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            c if c.is_ascii() && !c.is_ascii_control() => out.push(c),
            c => {
                let mut buf = [0u16; 2];
                for u in c.encode_utf16(&mut buf) {
                    let _ = write!(out, "\\u{:04x}", u);
                }
            }
        }
    }
    out.push('"');
    out
}

fn hex(c: Rgb) -> String {
    format!("\"#{:02x}{:02x}{:02x}\"", c[0], c[1], c[2])
}

/// A number the way JavaScript would print it (no trailing `.0`).
fn num(v: f64) -> String {
    if v.fract() == 0.0 && v.abs() < 1e15 {
        format!("{}", v as i64)
    } else {
        format!("{v}")
    }
}

fn list<T>(items: impl IntoIterator<Item = T>, f: impl Fn(T) -> String) -> String {
    let v: Vec<String> = items.into_iter().map(f).collect();
    format!("[{}]", v.join(", "))
}

/// The frame alphabet: `.` is 0 (transparent), then `A`-`Z`, `a`-`z` for
/// pixel values 1-52 (1 is the outline).
const ALPHABET: &[u8] = b".ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz";

/// Run-length encodes a frame: a value char, preceded by a decimal count
/// when it repeats three or more times ("12.3B" is twelve transparent
/// pixels then three of value 2).
pub fn encode_frame(px: &[u8]) -> String {
    let mut out = String::new();
    let mut i = 0;
    while i < px.len() {
        let v = px[i];
        let mut n = 1;
        while i + n < px.len() && px[i + n] == v {
            n += 1;
        }
        let ch = ALPHABET[v as usize] as char;
        if n >= 3 {
            let _ = write!(out, "{n}{ch}");
        } else {
            for _ in 0..n {
                out.push(ch);
            }
        }
        i += n;
    }
    out
}

/// The inverse of [`encode_frame`] (the desktop's `decodeFrame` mirrors it).
pub fn decode_frame(s: &str) -> Vec<u8> {
    let mut out = Vec::new();
    let mut n = 0usize;
    for b in s.bytes() {
        if b.is_ascii_digit() {
            n = n * 10 + usize::from(b - b'0');
            continue;
        }
        let v = ALPHABET.iter().position(|a| *a == b).unwrap_or(0) as u8;
        for _ in 0..n.max(1) {
            out.push(v);
        }
        n = 0;
    }
    out
}

/// A seeded run the desktop's port checks itself against: the cast on
/// screen every ten seconds of a five-minute busy afternoon, positions to
/// two decimals.
/// One golden checkpoint: the time and the cast as (species, x, lift, pose).
pub type Checkpoint = (f64, Vec<(String, f64, f64, &'static str)>);

pub fn golden() -> Vec<Checkpoint> {
    let input = golden_input();
    let mut w = World::new(GOLDEN_SEED);
    w.often = Often::Busy;
    let mut out = Vec::new();
    for k in 1..=30 {
        for _ in 0..world::steps(10.0) {
            w.tick(world::STEP, &input);
        }
        let row = w
            .critters
            .iter()
            .map(|c| {
                let r2 = |v: f64| (v * 100.0).round() / 100.0;
                (
                    c.species().id.to_string(),
                    r2(c.x),
                    r2(c.lift),
                    c.pose.name(),
                )
            })
            .collect();
        out.push((f64::from(k) * 10.0, row));
    }
    out
}

pub const GOLDEN_SEED: u32 = 7;

pub fn golden_input() -> Input {
    Input {
        view_w: 160.0,
        view_h: 60.0,
        floor: 56.0,
        hour: 14,
        ..Input::default()
    }
}

/// The whole generated file.
pub fn render_ts() -> String {
    let mut o = String::from(TS_HEAD);
    let names = |v: &[&str]| v.iter().map(|s| ts_str(s)).collect::<Vec<_>>();
    let union =
        |name: &str, items: Vec<String>| format!("export type {name} = {};\n", items.join(" | "));

    // ---- enums ----
    let poses = names(&Pose::ALL.map(Pose::name));
    let gaits = names(&Gait::ALL.map(Gait::name));
    let origins = names(&Origin::ALL.map(Origin::name));
    let moods = names(&Personality::ALL.map(Personality::name));
    let states = names(&State::ALL.map(State::name));
    let icons = names(&Icon::ALL.map(Icon::name));
    let mut specials: Vec<&str> = Vec::new();
    for s in SPECIES {
        if !specials.contains(&s.special.name()) {
            specials.push(s.special.name());
        }
    }
    specials.sort_unstable();
    let mut relations: Vec<&str> = RELATIONS.iter().map(|(_, r, _)| r.name()).collect();
    relations.sort_unstable();
    relations.dedup();
    o.push_str(&union("Pose", poses.clone()));
    o.push_str(&union("Gait", gaits.clone()));
    o.push_str(&union("Origin", origins.clone()));
    o.push_str(&union("Personality", moods.clone()));
    o.push_str(&union("State", states.clone()));
    o.push_str(&union("IconName", icons.clone()));
    o.push_str(&union("Special", names(&specials)));
    o.push_str(&union("Relation", names(&relations)));
    o.push('\n');
    let _ = writeln!(
        o,
        "export const POSES: readonly Pose[] = [{}];",
        poses.join(", ")
    );
    let _ = writeln!(
        o,
        "export const GAITS: readonly Gait[] = [{}];",
        gaits.join(", ")
    );
    let _ = writeln!(
        o,
        "export const ORIGINS: readonly Origin[] = [{}];",
        origins.join(", ")
    );
    let _ = writeln!(
        o,
        "export const ORIGIN_LABELS: Readonly<Record<Origin, string>> = {{ {} }};",
        Origin::ALL
            .iter()
            .map(|g| format!("{}: {}", g.name(), ts_str(g.label())))
            .collect::<Vec<_>>()
            .join(", ")
    );
    let _ = writeln!(
        o,
        "export const PERSONALITIES: readonly Personality[] = [{}];",
        moods.join(", ")
    );
    let _ = writeln!(
        o,
        "export const STATES: readonly State[] = [{}];",
        states.join(", ")
    );
    let _ = writeln!(
        o,
        "export const NEXT_TO: readonly State[] = [{}];",
        names(&State::TO.map(State::name)).join(", ")
    );
    let _ = writeln!(
        o,
        "export const FLAGS = {{ {} }} as const;",
        flag::ALL
            .iter()
            .map(|(b, n)| format!("{n}: {b}"))
            .collect::<Vec<_>>()
            .join(", ")
    );
    o.push('\n');

    // ---- types ----
    o.push_str(TS_TYPES);

    // ---- the cast ----
    o.push_str("export const SPECIES: readonly SpeciesRow[] = [\n");
    for (i, s) in SPECIES.iter().enumerate() {
        let sp = sprite(i);
        let _ = writeln!(o, "  {{");
        let _ = writeln!(
            o,
            "    id: {}, name: {}, origin: {}, gait: {}, special: {},",
            ts_str(s.id),
            ts_str(s.name),
            ts_str(s.origin.name()),
            ts_str(s.gait.name()),
            ts_str(s.special.name())
        );
        let _ = writeln!(
            o,
            "    speed: {}, flags: {}, weight: {}, stay: {}, hover: {}, drop: {},",
            num(s.speed),
            s.flags,
            num(s.weight),
            num(s.stay),
            num(s.hover),
            ts_str(s.drop)
        );
        let _ = writeln!(
            o,
            "    lines: {}, bias: {},",
            list(s.lines.iter(), |l| ts_str(l)),
            list(s.bias.iter(), |b| b.to_string())
        );
        let _ = writeln!(
            o,
            "    sprite: {{ w: {}, h: {}, palette: {}, blink: {},",
            sp.w,
            sp.h,
            list(sp.palette.iter(), |c| hex(*c)),
            list(sp.blink.iter(), |(a, b)| format!("[{a}, {b}]"))
        );
        o.push_str("      frames: {\n");
        for (p, frames) in Pose::ALL.iter().zip(&sp.poses) {
            let _ = writeln!(
                o,
                "        {}: {},",
                p.name(),
                list(frames.iter(), |f| ts_str(&encode_frame(f)))
            );
        }
        o.push_str("      } } },\n");
    }
    o.push_str("];\n\n");

    // ---- the brain's tables ----
    let row = |r: &[f64]| list(r.iter(), |v| num(*v));
    let _ = writeln!(
        o,
        "/** NEXT[from][to]: rows in STATES order, columns in NEXT_TO order. */\nexport const NEXT: readonly (readonly number[])[] = [\n{}];",
        NEXT.iter().map(|r| format!("  {},\n", row(r))).collect::<String>()
    );
    let _ = writeln!(
        o,
        "/** MOODS[personality][to]: multipliers on NEXT's columns. */\nexport const MOODS: readonly (readonly number[])[] = [\n{}];",
        MOODS.iter().map(|r| format!("  {},\n", row(r))).collect::<String>()
    );
    let _ = writeln!(
        o,
        "export const RELATIONS: readonly (readonly [string, Relation, string])[] = {};",
        list(RELATIONS.iter(), |(a, r, b)| format!(
            "[{}, {}, {}]",
            ts_str(a),
            ts_str(r.name()),
            ts_str(b)
        ))
    );
    let _ = writeln!(
        o,
        "export const NAMES: readonly string[] = {};",
        list(NAMES.iter(), |n| ts_str(n))
    );
    let _ = writeln!(
        o,
        "export const PREFIXES: readonly string[] = {};",
        list(PREFIXES.iter(), |n| ts_str(n))
    );
    let _ = writeln!(
        o,
        "export const WORDS: readonly string[] = {};",
        list(world::WORDS.iter(), |n| ts_str(n))
    );
    o.push('\n');

    // ---- constants ----
    let _ = writeln!(o, "export const PHYSICS = {{");
    for (k, v) in [
        ("STEP", world::STEP),
        ("STEP_US", world::STEP_US as f64),
        ("MAX_TICK_US", world::MAX_TICK_US as f64),
        ("GRAVITY", world::GRAVITY),
        ("BOUNCE", world::BOUNCE),
        ("FRICTION", world::FRICTION),
        ("WALL_BOUNCE", world::WALL_BOUNCE),
        ("SETTLE_SPEED", world::SETTLE_SPEED),
        ("MAX_FLING", world::MAX_FLING),
        ("SQUASH_TIME", world::SQUASH_TIME),
        ("BLINK_TIME", world::BLINK_TIME),
        ("MAX_VISITORS", world::MAX_VISITORS as f64),
        ("ITEM_LIFE", world::ITEM_LIFE),
        ("FLIGHT_ARC", world::FLIGHT_ARC),
        ("LATE_SLEEP", world::LATE_SLEEP),
    ] {
        let _ = writeln!(o, "  {k}: {},", num(v));
    }
    o.push_str("} as const;\n");
    let _ = writeln!(
        o,
        "/** Local hours as [from, until): night brings the nocturnal, late and\n *  idle means sleep, mornings bring the tui. */\nexport const CLOCK = {{ night: [{}, {}], late: [{}, {}], morning: [{}, {}] }} as const;",
        world::NIGHT.0,
        world::NIGHT.1,
        world::LATE.0,
        world::LATE.1,
        world::MORNING.0,
        world::MORNING.1
    );
    let _ = writeln!(
        o,
        "export const OFTEN = {{ {} }} as const;",
        [
            ("rare", Often::Rare),
            ("normal", Often::Normal),
            ("busy", Often::Busy)
        ]
        .iter()
        .map(|(n, f)| {
            let (a, b) = f.gap();
            format!(
                "{n}: {{ gap: [{}, {}], second: {} }}",
                num(a),
                num(b),
                num(f.second())
            )
        })
        .collect::<Vec<_>>()
        .join(", ")
    );
    let _ = writeln!(
        o,
        "export const INK = {{ soot: {}, bubble: {}, text: {}, gold: {}, bone: {}, dust: {}, trail: {}, crumb: {}, smoke: {}, beam: {}, flash: {}, confetti: {} }} as const;",
        hex(ink::SOOT),
        hex(ink::BUBBLE),
        hex(ink::TEXT),
        hex(ink::GOLD),
        hex(ink::BONE),
        hex(ink::DUST),
        hex(ink::TRAIL),
        hex(ink::CRUMB),
        hex(ink::SMOKE),
        hex(ink::BEAM),
        hex(ink::FLASH),
        list(ink::CONFETTI.iter(), |c| hex(*c))
    );
    o.push('\n');

    // ---- type ----
    let _ = writeln!(
        o,
        "/** The 3x5 capital font: `#` is ink. Lower case maps to capitals; a\n *  vowel in MACRONS draws its capital with a bar MACRON_RISE rows above. */"
    );
    let _ = writeln!(
        o,
        "export const FONT = {{ cap: {CAP}, macronRise: {MACRON_RISE}, spacing: {SPACING} }} as const;"
    );
    o.push_str("export const GLYPHS: Readonly<Record<string, { w: number; rows: readonly string[] }>> = {\n");
    for g in GLYPHS {
        let _ = writeln!(
            o,
            "  {}: {{ w: {}, rows: {} }},",
            ts_str(&g.ch.to_string()),
            g.w,
            list(g.rows.iter(), |r| ts_str(r))
        );
    }
    o.push_str("};\n");
    let macrons = [('ā', 'A'), ('ē', 'E'), ('ī', 'I'), ('ō', 'O'), ('ū', 'U')];
    let _ = writeln!(
        o,
        "export const MACRONS: Readonly<Record<string, string>> = {{ {} }};",
        macrons
            .iter()
            .flat_map(|(l, base)| {
                let up = l.to_uppercase().next().unwrap_or(*l);
                [
                    format!("{}: \"{base}\"", ts_str(&l.to_string())),
                    format!("{}: \"{base}\"", ts_str(&up.to_string())),
                ]
            })
            .collect::<Vec<_>>()
            .join(", ")
    );
    o.push_str("/** Emote icons: `X` is the icon's colour, drawn with a soot outline. */\n");
    o.push_str("export const ICONS: Readonly<Record<IconName, { color: string; rows: readonly string[] }>> = {\n");
    for i in Icon::ALL {
        let _ = writeln!(
            o,
            "  {}: {{ color: {}, rows: {} }},",
            i.name(),
            hex(i.color()),
            list(i.rows().iter(), |r| ts_str(r))
        );
    }
    o.push_str("};\n\n");

    // ---- checks for the port ----
    let mut r = Rng::from_seed("wevd");
    let first: Vec<String> = (0..4).map(|_| format!("{}", r.float())).collect();
    let _ = writeln!(
        o,
        "/** The RNG check: hashStr(\"wevd\") and the first four mulberry32 draws. */\nexport const RNG_CHECK = {{ seed: \"wevd\", hash: {}, first: [{}] }} as const;",
        hash_str("wevd"),
        first.join(", ")
    );
    let gi = golden_input();
    let _ = writeln!(
        o,
        "/** A seeded run to check the port against: World(seed {GOLDEN_SEED}), often busy,\n *  view {}x{} floor {} hour {}, ticked in STEPs; every 10 s the cast on\n *  screen as [species, x, lift, pose], rounded to two decimals. */",
        num(gi.view_w),
        num(gi.view_h),
        num(gi.floor),
        gi.hour
    );
    let _ = writeln!(
        o,
        "export const GOLDEN = {{ seed: {GOLDEN_SEED}, viewW: {}, viewH: {}, floor: {}, hour: {}, often: \"busy\", checkpoints: [",
        num(gi.view_w),
        num(gi.view_h),
        num(gi.floor),
        gi.hour
    );
    for (t, row) in golden() {
        let _ = writeln!(
            o,
            "  {{ t: {}, cast: {} }},",
            num(t),
            list(row.iter(), |(id, x, l, p)| format!(
                "[{}, {}, {}, {}]",
                ts_str(id),
                num(*x),
                num(*l),
                ts_str(p)
            ))
        );
    }
    o.push_str("] } as const;\n\n");
    o.push_str(TS_DECODE);
    o
}

const TS_TYPES: &str = "\
/** Baked art. Pixel value 0 is transparent and v >= 1 is palette[v - 1]
 *  (palette[0] is the outline). Frames are run-length strings over the
 *  alphabet `.A-Za-z` (0, then 1-52), a decimal count before a char that
 *  repeats; decodeFrame turns one into w*h values, row-major. The anchor is
 *  bottom-centre; frames face right (mirror for dir < 0). `blink` is the
 *  [from, to] value swaps that close the eyes. */
export interface SpriteData {
  w: number;
  h: number;
  palette: readonly string[];
  blink: readonly (readonly [number, number])[];
  frames: Readonly<Record<Pose, readonly string[]>>;
}

export interface SpeciesRow {
  id: string;
  name: string;
  origin: Origin;
  gait: Gait;
  special: Special;
  /** Sprite px per second. */
  speed: number;
  /** FLAGS bits. */
  flags: number;
  /** Spawn weight: 10 ordinary, 1 rare, 0 never on its own. */
  weight: number;
  stay: number;
  /** Float/fly gaits hang this many px above the floor. */
  hover: number;
  /** What the `drop` special leaves on the floor ('' for none). */
  drop: string;
  lines: readonly string[];
  /** Personality weights in PERSONALITIES order. */
  bias: readonly number[];
  sprite: SpriteData;
}

";

const TS_DECODE: &str = "\
const ALPHABET = \".ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz\";

/** A frame string to its w*h pixel values (see SpriteData). */
export function decodeFrame(s: string): Uint8Array {
  const out: number[] = [];
  let n = 0;
  for (const ch of s) {
    if (ch >= \"0\" && ch <= \"9\") {
      n = n * 10 + (ch.charCodeAt(0) - 48);
      continue;
    }
    const v = Math.max(0, ALPHABET.indexOf(ch));
    for (let i = 0; i < Math.max(1, n); i++) out.push(v);
    n = 0;
  }
  return Uint8Array.from(out);
}
";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frames_round_trip() {
        for (i, _) in SPECIES.iter().enumerate() {
            for frames in &sprite(i).poses {
                for f in frames {
                    assert_eq!(&decode_frame(&encode_frame(f)), f);
                }
            }
        }
        assert_eq!(encode_frame(&[0, 0, 0, 2, 2, 1]), "3.BBA");
    }

    #[test]
    fn the_generated_file_is_plain_ascii_and_small() {
        let ts = render_ts();
        assert!(ts.is_ascii());
        assert!(ts.len() < 150 * 1024, "cast.gen.ts is {} bytes", ts.len());
        assert_eq!(
            ts_str("P\u{016b}keko \u{1F600}"),
            "\"P\\u016bkeko \\ud83d\\ude00\""
        );
    }

    #[test]
    fn cast_gen_ts_is_current() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(TS_PATH);
        let want = render_ts();
        if std::env::var_os("KETE_BLESS").is_some() {
            if let Some(dir) = path.parent() {
                std::fs::create_dir_all(dir).unwrap();
            }
            std::fs::write(&path, &want).unwrap();
        }
        let have = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("{}: {e} (run with KETE_BLESS=1)", path.display()))
            .replace('\r', "");
        assert!(
            have == want,
            "{} is stale: run KETE_BLESS=1 cargo test -p kete-critters cast_gen",
            path.display()
        );
    }
}
