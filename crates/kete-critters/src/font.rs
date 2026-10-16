//! The built-in pixel type: a 3×5 capital font for speech bubbles, name
//! tags and text pops, and the 5×5 emote icons. No external font, because
//! the desktop's CSP blocks web fonts (the stream overlay used Silkscreen).
//!
//! Text is drawn in capitals; lower case maps up. Macron vowels (ā ē ī ō ū)
//! are the plain capital with a bar two rows above the cap height, so a
//! line holding one is 7 px tall instead of 5.

/// One glyph: `w` columns, five rows of `#`/`.`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Glyph {
    pub ch: char,
    pub w: usize,
    pub rows: [&'static str; 5],
}

/// Cap height. A macron sits at row -2 (one clear row above the cap).
pub const CAP: usize = 5;
pub const MACRON_RISE: usize = 2;
/// Blank columns between glyphs.
pub const SPACING: usize = 1;

macro_rules! g {
    ($ch:expr, $w:expr, [$a:expr, $b:expr, $c:expr, $d:expr, $e:expr]) => {
        Glyph {
            ch: $ch,
            w: $w,
            rows: [$a, $b, $c, $d, $e],
        }
    };
}

/// The font, in a fixed order (the generated table keeps it).
pub const GLYPHS: &[Glyph] = &[
    g!('A', 3, [".#.", "#.#", "###", "#.#", "#.#"]),
    g!('B', 3, ["##.", "#.#", "##.", "#.#", "##."]),
    g!('C', 3, [".##", "#..", "#..", "#..", ".##"]),
    g!('D', 3, ["##.", "#.#", "#.#", "#.#", "##."]),
    g!('E', 3, ["###", "#..", "##.", "#..", "###"]),
    g!('F', 3, ["###", "#..", "##.", "#..", "#.."]),
    g!('G', 3, [".##", "#..", "#.#", "#.#", ".##"]),
    g!('H', 3, ["#.#", "#.#", "###", "#.#", "#.#"]),
    g!('I', 3, ["###", ".#.", ".#.", ".#.", "###"]),
    g!('J', 3, ["..#", "..#", "..#", "#.#", ".#."]),
    g!('K', 3, ["#.#", "#.#", "##.", "#.#", "#.#"]),
    g!('L', 3, ["#..", "#..", "#..", "#..", "###"]),
    g!('M', 5, ["#...#", "##.##", "#.#.#", "#...#", "#...#"]),
    g!('N', 4, ["#..#", "##.#", "#.##", "#..#", "#..#"]),
    g!('O', 3, [".#.", "#.#", "#.#", "#.#", ".#."]),
    g!('P', 3, ["##.", "#.#", "##.", "#..", "#.."]),
    g!('Q', 3, [".#.", "#.#", "#.#", "##.", ".##"]),
    g!('R', 3, ["##.", "#.#", "##.", "#.#", "#.#"]),
    g!('S', 3, [".##", "#..", ".#.", "..#", "##."]),
    g!('T', 3, ["###", ".#.", ".#.", ".#.", ".#."]),
    g!('U', 3, ["#.#", "#.#", "#.#", "#.#", "###"]),
    g!('V', 3, ["#.#", "#.#", "#.#", "#.#", ".#."]),
    g!('W', 5, ["#...#", "#...#", "#.#.#", "##.##", "#...#"]),
    g!('X', 3, ["#.#", "#.#", ".#.", "#.#", "#.#"]),
    g!('Y', 3, ["#.#", "#.#", ".#.", ".#.", ".#."]),
    g!('Z', 3, ["###", "..#", ".#.", "#..", "###"]),
    g!('0', 3, ["###", "#.#", "#.#", "#.#", "###"]),
    g!('1', 3, [".#.", "##.", ".#.", ".#.", "###"]),
    g!('2', 3, ["##.", "..#", ".#.", "#..", "###"]),
    g!('3', 3, ["##.", "..#", ".#.", "..#", "##."]),
    g!('4', 3, ["#.#", "#.#", "###", "..#", "..#"]),
    g!('5', 3, ["###", "#..", "##.", "..#", "##."]),
    g!('6', 3, [".##", "#..", "###", "#.#", "###"]),
    g!('7', 3, ["###", "..#", ".#.", ".#.", ".#."]),
    g!('8', 3, ["###", "#.#", "###", "#.#", "###"]),
    g!('9', 3, ["###", "#.#", "###", "..#", "##."]),
    g!(' ', 2, ["..", "..", "..", "..", ".."]),
    g!('!', 1, ["#", "#", "#", ".", "#"]),
    g!('?', 3, ["##.", "..#", ".#.", "...", ".#."]),
    g!('.', 1, [".", ".", ".", ".", "#"]),
    g!(',', 2, ["..", "..", "..", ".#", "#."]),
    g!('\'', 1, ["#", "#", ".", ".", "."]),
    g!('"', 3, ["#.#", "#.#", "...", "...", "..."]),
    g!('-', 3, ["...", "...", "###", "...", "..."]),
    g!('+', 3, ["...", ".#.", "###", ".#.", "..."]),
    g!('*', 3, ["...", "#.#", ".#.", "#.#", "..."]),
    g!(':', 1, [".", "#", ".", "#", "."]),
    g!('/', 3, ["..#", "..#", ".#.", "#..", "#.."]),
    g!('(', 2, [".#", "#.", "#.", "#.", ".#"]),
    g!(')', 2, ["#.", ".#", ".#", ".#", "#."]),
    g!('~', 4, ["....", ".#.#", "#.#.", "....", "...."]),
    g!('=', 3, ["...", "###", "...", "###", "..."]),
    g!('<', 3, ["..#", ".#.", "#..", ".#.", "..#"]),
    g!('>', 3, ["#..", ".#.", "..#", ".#.", "#.."]),
    g!('%', 3, ["#.#", "..#", ".#.", "#..", "#.#"]),
    g!('_', 3, ["...", "...", "...", "...", "###"]),
];

/// A char's glyph and whether it carries a macron. Lower case maps to
/// capitals; anything the font lacks is `None` (the coverage test keeps
/// every line and name in the cast inside the font).
pub fn glyph(c: char) -> Option<(&'static Glyph, bool)> {
    let (base, macron) = match c {
        'ā' | 'Ā' => ('A', true),
        'ē' | 'Ē' => ('E', true),
        'ī' | 'Ī' => ('I', true),
        'ō' | 'Ō' => ('O', true),
        'ū' | 'Ū' => ('U', true),
        c => (c.to_ascii_uppercase(), false),
    };
    GLYPHS.iter().find(|g| g.ch == base).map(|g| (g, macron))
}

/// Whether the font can draw every char of `s`.
pub fn covers(s: &str) -> bool {
    s.chars().all(|c| glyph(c).is_some())
}

/// Width in pixels of `s` set in the font (unknown chars count as `?`).
pub fn text_width(s: &str) -> usize {
    let mut w = 0;
    for (i, c) in s.chars().enumerate() {
        if i > 0 {
            w += SPACING;
        }
        w += glyph(c).or_else(|| glyph('?')).map_or(3, |(g, _)| g.w);
    }
    w
}

/// Height of a line of `s`: the cap height, plus the macron rise when a
/// macron vowel is in it.
pub fn text_height(s: &str) -> usize {
    if s.chars().any(|c| glyph(c).is_some_and(|(_, m)| m)) {
        CAP + MACRON_RISE
    } else {
        CAP
    }
}

/// The pixels of `s`, as (x, y) offsets from the top-left of its line box
/// (`text_height(s)` tall).
pub fn text_pixels(s: &str) -> Vec<(usize, usize)> {
    let rise = text_height(s) - CAP;
    let mut out = Vec::new();
    let mut x0 = 0;
    for c in s.chars() {
        let (g, macron) = glyph(c).or_else(|| glyph('?')).expect("? is in the font");
        for (y, row) in g.rows.iter().enumerate() {
            for (x, b) in row.chars().enumerate() {
                if b == '#' {
                    out.push((x0 + x, rise + y));
                }
            }
        }
        if macron {
            out.extend((0..g.w).map(|x| (x0 + x, 0)));
        }
        x0 += g.w + SPACING;
    }
    out
}

/// The emote icons: the particles that float up from a critter's head.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Icon {
    Heart,
    Note,
    Bang,
    What,
    Zed,
    Spark,
    Drop,
    Star,
    Lamp,
    Puff,
    Anger,
    /// Items lying on the floor.
    Hide,
    Feather,
}

impl Icon {
    pub const ALL: [Icon; 13] = [
        Icon::Heart,
        Icon::Note,
        Icon::Bang,
        Icon::What,
        Icon::Zed,
        Icon::Spark,
        Icon::Drop,
        Icon::Star,
        Icon::Lamp,
        Icon::Puff,
        Icon::Anger,
        Icon::Hide,
        Icon::Feather,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Icon::Heart => "heart",
            Icon::Note => "note",
            Icon::Bang => "bang",
            Icon::What => "what",
            Icon::Zed => "z",
            Icon::Spark => "spark",
            Icon::Drop => "drop",
            Icon::Star => "star",
            Icon::Lamp => "lamp",
            Icon::Puff => "puff",
            Icon::Anger => "anger",
            Icon::Hide => "hide",
            Icon::Feather => "feather",
        }
    }

    /// The icon's rows: `X` is its colour, `.` transparent. Drawn with a
    /// soot outline around it, like the sprites.
    pub fn rows(self) -> &'static [&'static str] {
        match self {
            Icon::Heart => &[".X.X.", "XXXXX", "XXXXX", ".XXX.", "..X.."],
            Icon::Note => &["..XX.", "..X.X", "..X..", "XXX..", "XX..."],
            Icon::Bang => &["X", "X", "X", ".", "X"],
            Icon::What => &["XX.", "..X", ".X.", "...", ".X."],
            Icon::Zed => &["XXX", "..X", ".X.", "XXX"],
            Icon::Spark => &["..X..", ".XXX.", "XX.XX", ".XXX.", "..X.."],
            Icon::Drop => &[".X.", "XXX", "XXX", ".X."],
            Icon::Star => &["..X..", "XXXXX", ".XXX.", "XX.XX"],
            Icon::Lamp => &["...X...", ".XXXX.X", "XXXXXX.", "..XX..."],
            Icon::Puff => &[".XX.", "XXXX", ".XX."],
            Icon::Anger => &["X.X", ".X.", "X.X"],
            Icon::Hide => &["XXXX.", "XXXXX", ".XXXX"],
            Icon::Feather => &["...X", "..XX", ".XX.", "X..."],
        }
    }

    pub fn color(self) -> [u8; 3] {
        match self {
            Icon::Heart => [0xf0, 0x74, 0x8a],
            Icon::Note => [0x9a, 0xd0, 0xff],
            Icon::Bang | Icon::What => [0xff, 0xd1, 0x66],
            Icon::Zed => [0xcf, 0xd8, 0xff],
            Icon::Spark | Icon::Star => [0xff, 0xe3, 0x8a],
            Icon::Drop => [0x9a, 0xd0, 0xff],
            Icon::Lamp => [0xe8, 0xb3, 0x3a],
            Icon::Puff => [0xc8, 0xc4, 0xcc],
            Icon::Anger => [0xe8, 0x50, 0x48],
            Icon::Hide => [0xc8, 0xa0, 0x78],
            Icon::Feather => [0xf2, 0xed, 0xe4],
        }
    }

    pub fn size(self) -> (usize, usize) {
        let r = self.rows();
        (r[0].len(), r.len())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn glyph_rows_match_their_width() {
        for g in GLYPHS {
            for r in g.rows {
                assert_eq!(r.len(), g.w, "{:?}", g.ch);
                assert!(r.chars().all(|c| c == '#' || c == '.'));
            }
        }
        for i in Icon::ALL {
            let (w, _) = i.size();
            assert!(i.rows().iter().all(|r| r.len() == w), "{}", i.name());
        }
    }

    #[test]
    fn macrons_rise_above_the_cap() {
        assert_eq!(text_height("kia ora"), 5);
        assert_eq!(text_height("Pūkeko"), 7);
        assert!(covers("Kākāpō, kia ora! +25 xp"));
        assert_eq!(text_width("A"), 3);
        assert_eq!(text_width("AA"), 7);
        let px = text_pixels("ū");
        assert!(px.contains(&(0, 0)) && px.contains(&(2, 0)));
        assert!(
            !px.iter().any(|(_, y)| *y == 1),
            "a clear row under the bar"
        );
    }
}
