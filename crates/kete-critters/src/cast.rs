//! The baked cast: every species' art, parsed from `art/*.txt` and baked
//! once on first use, indexed like [`SPECIES`].

use std::sync::OnceLock;

use crate::art::{self, Authored, Pose, Sprite};
use crate::species::SPECIES;

/// The art files, compiled in. The grouping is by who drew what, not by
/// origin (the species table says where each one is from).
pub const ART_FILES: &[(&str, &str)] = &[
    ("aotearoa", include_str!("../art/aotearoa.txt")),
    ("farm", include_str!("../art/farm.txt")),
    ("wild", include_str!("../art/wild.txt")),
    ("osrs", include_str!("../art/osrs.txt")),
    ("sandbox", include_str!("../art/sandbox.txt")),
    ("maple", include_str!("../art/maple.txt")),
];

pub struct Cast {
    /// `sprites[i]` is `SPECIES[i]`'s art.
    pub sprites: Vec<Sprite>,
    /// Species ids without art (the tests require this to be empty; the
    /// world draws a placeholder for them rather than panicking).
    pub missing: Vec<&'static str>,
}

/// Every authored block in every art file, with the file it came from.
/// Parse errors carry the file name.
pub fn authored() -> Result<Vec<(&'static str, Authored)>, String> {
    let mut out = Vec::new();
    for (file, src) in ART_FILES {
        let blocks = art::parse(src).map_err(|e| format!("art/{file}.txt {e}"))?;
        out.extend(blocks.into_iter().map(|b| (*file, b)));
    }
    Ok(out)
}

fn placeholder() -> Sprite {
    let src = "== missing\nsize 6 6\npal m #e8508f\npal e #18121e\nlid m\n\
               -- idle\nmmmmmm\nm.mm.m\nmmmmem\nmmmmmm\nm.mm.m\nmmmmmm\n";
    let a = art::parse(src).expect("the placeholder parses");
    art::bake(&a[0])
}

pub fn cast() -> &'static Cast {
    static CAST: OnceLock<Cast> = OnceLock::new();
    CAST.get_or_init(|| {
        // A broken art file only happens in development (the tests parse
        // every file); a placeholder keeps the world running meanwhile.
        let blocks = authored().unwrap_or_default();
        let mut missing = Vec::new();
        let sprites = SPECIES
            .iter()
            .map(|s| match blocks.iter().find(|(_, a)| a.id == s.id) {
                Some((_, a)) => art::bake(a),
                None => {
                    missing.push(s.id);
                    placeholder()
                }
            })
            .collect();
        Cast { sprites, missing }
    })
}

/// A species' sprite by index.
pub fn sprite(species: usize) -> &'static Sprite {
    &cast().sprites[species]
}

/// The frame a critter shows.
pub fn frame(species: usize, pose: Pose, f: usize) -> &'static art::Frame {
    sprite(species).frame(pose, f)
}
