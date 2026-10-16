//! The cast: what each species is (art aside). The art lives in
//! `art/*.txt` under the same ids; [`crate::cast`] joins the two.

/// Where a species comes from, for the collection log's grouping.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Origin {
    Aotearoa,
    Farm,
    Osrs,
    Sandbox,
    Maple,
    NzGames,
}

impl Origin {
    pub const ALL: [Origin; 6] = [
        Origin::Aotearoa,
        Origin::Farm,
        Origin::Osrs,
        Origin::Sandbox,
        Origin::Maple,
        Origin::NzGames,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Origin::Aotearoa => "aotearoa",
            Origin::Farm => "farm",
            Origin::Osrs => "osrs",
            Origin::Sandbox => "sandbox",
            Origin::Maple => "maple",
            Origin::NzGames => "nzgames",
        }
    }

    /// The heading the collection log shows over the group.
    pub fn label(self) -> &'static str {
        match self {
            Origin::Aotearoa => "Aotearoa",
            Origin::Farm => "Farm and friends",
            Origin::Osrs => "Old school",
            Origin::Sandbox => "Blocky",
            Origin::Maple => "Mushroom kingdom",
            Origin::NzGames => "Made in NZ",
        }
    }
}

/// How a species gets about. Every gait walks with `walk` frames except
/// as noted.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Gait {
    /// Steady steps.
    Walk,
    /// Hops in arcs; the walk frame is the airborne pose.
    Hop,
    /// Steps with a 1 px side-to-side bob.
    Waddle,
    /// Slow and continuous (snails, slugs); leaves a trail if flagged.
    Slide,
    /// Bursts and stops.
    Dart,
    /// Drifts a little above the floor, bobbing (ghost, balloon, genie).
    Float,
    /// Walks sideways without turning (crab).
    Sideways,
    /// Hovers above the floor with fly frames and only lands to sit,
    /// sleep or eat (bee).
    Fly,
}

impl Gait {
    pub const ALL: [Gait; 8] = [
        Gait::Walk,
        Gait::Hop,
        Gait::Waddle,
        Gait::Slide,
        Gait::Dart,
        Gait::Float,
        Gait::Sideways,
        Gait::Fly,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Gait::Walk => "walk",
            Gait::Hop => "hop",
            Gait::Waddle => "waddle",
            Gait::Slide => "slide",
            Gait::Dart => "dart",
            Gait::Float => "float",
            Gait::Sideways => "sideways",
            Gait::Fly => "fly",
        }
    }
}

/// Species flags, as bits.
pub mod flag {
    /// Takes off, flies to perches, has fly frames.
    pub const FLIER: u32 = 1;
    /// Only visits at night (local 20:00 to 06:00).
    pub const NOCTURNAL: u32 = 1 << 1;
    /// Fades in and out instead of walking on, drawn at 80% alpha.
    pub const GHOSTLY: u32 = 1 << 2;
    /// Leaves a fading slime trail.
    pub const TRAIL: u32 = 1 << 3;
    /// Gives off sparkles.
    pub const SPARKLE: u32 = 1 << 4;
    /// Three times as likely in the morning (05:00 to 10:00).
    pub const MORNING: u32 = 1 << 5;
    /// Twice as likely at night, still visits by day.
    pub const NIGHT_BIAS: u32 = 1 << 6;
    /// Never visits on its own: it only appears from an event or another
    /// critter (the split slimes, the dart monkey).
    pub const NO_SPAWN: u32 = 1 << 7;
    /// Arrives by dropping from the sky (the pet rock).
    pub const DROPS_IN: u32 = 1 << 8;
    /// Not alive (the balloon): drifts about, never eats, sleeps, emotes
    /// or meets anyone.
    pub const INANIMATE: u32 = 1 << 9;

    pub const ALL: [(u32, &str); 10] = [
        (FLIER, "flier"),
        (NOCTURNAL, "nocturnal"),
        (GHOSTLY, "ghostly"),
        (TRAIL, "trail"),
        (SPARKLE, "sparkle"),
        (MORNING, "morning"),
        (NIGHT_BIAS, "nightBias"),
        (NO_SPAWN, "noSpawn"),
        (DROPS_IN, "dropsIn"),
        (INANIMATE, "inanimate"),
    ];
}

/// A visitor's temperament: species bias it, then each visitor rolls one.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Personality {
    /// Walks over to the pointer.
    Curious,
    /// Scurries away from the pointer.
    Shy,
    Sleepy,
    /// Hops and chases.
    Playful,
    /// Seeks out other critters.
    Social,
    /// Says less.
    Grumpy,
    /// Eats more.
    Greedy,
}

impl Personality {
    pub const ALL: [Personality; 7] = [
        Personality::Curious,
        Personality::Shy,
        Personality::Sleepy,
        Personality::Playful,
        Personality::Social,
        Personality::Grumpy,
        Personality::Greedy,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Personality::Curious => "curious",
            Personality::Shy => "shy",
            Personality::Sleepy => "sleepy",
            Personality::Playful => "playful",
            Personality::Social => "social",
            Personality::Grumpy => "grumpy",
            Personality::Greedy => "greedy",
        }
    }
}

/// A species' signature move, picked by the `special` state.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Special {
    /// A random emote.
    Emote,
    /// A long eat.
    Graze,
    /// Flies somewhere else, to a perch when there is one.
    Flight,
    /// Walks to the pointer, takes its "shiny" and runs off (weka).
    Pinch,
    /// Sits and booms (kākāpō).
    Boom,
    /// Short flits back and forth (pīwakawaka).
    Flit,
    /// A belly slide (penguins).
    Slide,
    /// Push-ups (tuatara).
    Pushups,
    /// Three hops.
    Hops,
    /// A long sit with a note.
    Loaf,
    /// Grazes, then drops an item: the species' `drop` text.
    Drop,
    /// Turns in circles, then a heart (dog, ribbon pig).
    Spin,
    /// Hides (sit) a while.
    Hide,
    /// One big pounce.
    Pounce,
    /// Snips its claws (crab).
    Snip,
    /// Puffs spores (mushroom).
    Spores,
    /// Vanishes in a puff and reappears elsewhere (imp, ghost).
    Teleport,
    /// Swings a pickaxe: "+25 xp" (adventurer).
    Mine,
    /// Digs into the floor and pops up elsewhere (mole).
    Dig,
    /// Nothing at all, beloved (pet rock).
    Nothing,
    /// Offers a lamp (genie).
    Lamp,
    /// Hisses, flashes and pops into confetti (boom-bug).
    Fuse,
    /// Buzzes about with a note (bee).
    Buzz,
    /// Fetches a dropped item to the pointer (wisp).
    Fetch,
    /// Big bounces (orange mushroom).
    Bounce,
    /// Puffs smoke (dragon whelp).
    Smoke,
}

impl Special {
    pub fn name(self) -> &'static str {
        match self {
            Special::Emote => "emote",
            Special::Graze => "graze",
            Special::Flight => "flight",
            Special::Pinch => "pinch",
            Special::Boom => "boom",
            Special::Flit => "flit",
            Special::Slide => "slide",
            Special::Pushups => "pushups",
            Special::Hops => "hops",
            Special::Loaf => "loaf",
            Special::Drop => "drop",
            Special::Spin => "spin",
            Special::Hide => "hide",
            Special::Pounce => "pounce",
            Special::Snip => "snip",
            Special::Spores => "spores",
            Special::Teleport => "teleport",
            Special::Mine => "mine",
            Special::Dig => "dig",
            Special::Nothing => "nothing",
            Special::Lamp => "lamp",
            Special::Fuse => "fuse",
            Special::Buzz => "buzz",
            Special::Fetch => "fetch",
            Special::Bounce => "bounce",
            Special::Smoke => "smoke",
        }
    }
}

/// One species.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Species {
    pub id: &'static str,
    /// Display name, with macrons.
    pub name: &'static str,
    pub origin: Origin,
    pub gait: Gait,
    /// Walking speed in sprite px per second.
    pub speed: f64,
    /// `flag::*` bits.
    pub flags: u32,
    /// Spawn weight: 10 is ordinary, 3 uncommon, 1 rare, 0 never.
    pub weight: f64,
    /// Visit length multiplier.
    pub stay: f64,
    /// Speech lines, short and in character.
    pub lines: &'static [&'static str],
    /// Personality weights, in `Personality::ALL` order.
    pub bias: [u8; 7],
    pub special: Special,
    /// What `Special::Drop` leaves behind ("cowhide", "feather").
    pub drop: &'static str,
    /// How high a float or fly gait hangs above the floor, in sprite px.
    pub hover: f64,
}

impl Species {
    pub fn has(&self, f: u32) -> bool {
        self.flags & f != 0
    }
}

/// How two species get on, when they meet. Each row reads "a does this to
/// b"; rows are one-way.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Relation {
    /// a runs at b; b flees if it fears a.
    Chases,
    /// a runs from b on sight.
    Fears,
    /// They meet in the middle with hearts.
    Greets,
    /// They walk together and become one (mini slimes).
    Merges,
    /// They square up, anger marks, and back off.
    Bristles,
}

impl Relation {
    pub fn name(self) -> &'static str {
        match self {
            Relation::Chases => "chases",
            Relation::Fears => "fears",
            Relation::Greets => "greets",
            Relation::Merges => "merges",
            Relation::Bristles => "bristles",
        }
    }
}

pub const RELATIONS: &[(&str, Relation, &str)] = &[
    ("cat", Relation::Chases, "mouse"),
    ("cat", Relation::Chases, "boom_bug"),
    ("mouse", Relation::Fears, "cat"),
    ("boom_bug", Relation::Fears, "cat"),
    ("kiwi", Relation::Greets, "kiwi"),
    ("korora", Relation::Greets, "hoiho"),
    ("pukeko", Relation::Greets, "takahe"),
    ("slime_mini", Relation::Merges, "slime_mini"),
    ("dog", Relation::Bristles, "cat"),
    ("cat", Relation::Bristles, "dog"),
    ("dog", Relation::Chases, "sheep"),
    ("fox", Relation::Chases, "chicken"),
    ("chicken", Relation::Fears, "fox"),
    ("goblin", Relation::Fears, "adventurer"),
    ("adventurer", Relation::Chases, "goblin"),
    ("weka", Relation::Bristles, "kea"),
    ("kea", Relation::Bristles, "weka"),
];

/// The relation `a` has towards `b`, if any.
pub fn relation(a: &str, b: &str) -> Option<Relation> {
    RELATIONS
        .iter()
        .find(|(x, _, y)| *x == a && *y == b)
        .map(|(_, r, _)| *r)
}

use flag::*;
use Gait as G;
use Origin as O;
use Special as S;

/// Bias rows: curious, shy, sleepy, playful, social, grumpy, greedy.
const EVEN: [u8; 7] = [2, 2, 2, 2, 2, 1, 1];

#[allow(clippy::too_many_arguments)]
const fn sp(
    id: &'static str,
    name: &'static str,
    origin: Origin,
    gait: Gait,
    speed: f64,
    flags: u32,
    weight: f64,
    lines: &'static [&'static str],
    bias: [u8; 7],
    special: Special,
) -> Species {
    Species {
        id,
        name,
        origin,
        gait,
        speed,
        flags,
        weight,
        stay: 1.0,
        lines,
        bias,
        special,
        drop: "",
        hover: 0.0,
    }
}

const fn stay(mut s: Species, stay: f64) -> Species {
    s.stay = stay;
    s
}

const fn drops(mut s: Species, what: &'static str) -> Species {
    s.drop = what;
    s
}

const fn hover(mut s: Species, h: f64) -> Species {
    s.hover = h;
    s
}

/// The cast, in collection-log order.
#[rustfmt::skip]
pub const SPECIES: &[Species] = &[
    // Aotearoa
    sp("kiwi", "Kiwi", O::Aotearoa, G::Walk, 12.0, NIGHT_BIAS, 10.0, &["kiwi!", "ki-wi", "kia ora"], [3, 3, 2, 2, 3, 1, 2], S::Graze),
    sp("pukeko", "Pūkeko", O::Aotearoa, G::Walk, 10.0, 0, 10.0, &["kek!", "skreek"], [3, 1, 1, 2, 2, 3, 2], S::Spin),
    sp("tui", "Tūī", O::Aotearoa, G::Hop, 9.0, FLIER | MORNING, 9.0, &["*warble*", "tūī!", "*click*"], [2, 2, 1, 3, 2, 1, 2], S::Flight),
    sp("kea", "Kea", O::Aotearoa, G::Hop, 10.0, FLIER, 8.0, &["keeaa!", "nice wipers", "mine now"], [5, 0, 1, 4, 2, 1, 3], S::Flight),
    stay(sp("kakapo", "Kākāpō", O::Aotearoa, G::Waddle, 5.0, NOCTURNAL, 5.0, &["*boom*", "skraark", "boom..."], [3, 1, 3, 1, 3, 0, 2], S::Boom), 1.3),
    sp("takahe", "Takahē", O::Aotearoa, G::Walk, 7.0, 0, 6.0, &["oomf", "*munch*"], [2, 2, 2, 1, 2, 2, 4], S::Graze),
    sp("ruru", "Ruru", O::Aotearoa, G::Hop, 6.0, FLIER | NOCTURNAL, 8.0, &["more-pork", "ruru"], [3, 2, 3, 1, 1, 2, 1], S::Flight),
    sp("piwakawaka", "Pīwakawaka", O::Aotearoa, G::Hop, 14.0, FLIER, 9.0, &["cheet!", "cheet cheet"], [5, 0, 0, 4, 3, 0, 1], S::Flit),
    sp("weka", "Weka", O::Aotearoa, G::Walk, 11.0, 0, 8.0, &["coo-eet", "mine!", "shiny!"], [5, 0, 1, 2, 1, 2, 4], S::Pinch),
    sp("korora", "Kororā", O::Aotearoa, G::Waddle, 6.0, NIGHT_BIAS, 8.0, &["pip", "brr"], [2, 3, 2, 2, 3, 0, 1], S::Slide),
    sp("hoiho", "Hoiho", O::Aotearoa, G::Waddle, 6.0, 0, 5.0, &["hm", "*squawk*"], [1, 4, 2, 1, 1, 2, 1], S::Slide),
    stay(sp("tuatara", "Tuatara", O::Aotearoa, G::Dart, 6.0, 0, 6.0, &["...", "old as"], [1, 2, 4, 0, 1, 3, 1], S::Pushups), 1.3),
    sp("weta", "Wētā", O::Aotearoa, G::Dart, 7.0, NIGHT_BIAS, 7.0, &["*chirr*", "click"], [2, 3, 1, 2, 1, 2, 1], S::Hops),
    stay(sp("moa", "Moa", O::Aotearoa, G::Walk, 8.0, 0, 1.0, &["...", "not extinct"], [1, 3, 1, 1, 1, 2, 2], S::Graze), 1.2),
    stay(sp("kekeno", "Kekeno", O::Aotearoa, G::Hop, 5.0, 0, 5.0, &["arf", "oof", "*flop*"], [2, 1, 4, 3, 2, 1, 2], S::Loaf), 1.3),
    sp("sheep", "Sheep", O::Aotearoa, G::Walk, 9.0, 0, 10.0, &["baa", "meh", "sweet as"], [1, 3, 2, 1, 4, 1, 3], S::Graze),
    // Farm and classic
    drops(sp("cow", "Cow", O::Farm, G::Walk, 9.0, 0, 10.0, &["moo", "mooo"], [1, 1, 3, 1, 3, 1, 4], S::Drop), "cowhide"),
    sp("pig", "Pig", O::Farm, G::Walk, 9.0, 0, 9.0, &["oink", "snrk"], [2, 1, 3, 2, 2, 1, 5], S::Graze),
    drops(sp("chicken", "Chicken", O::Farm, G::Walk, 10.0, 0, 9.0, &["bok", "bawk"], [3, 2, 1, 2, 2, 1, 3], S::Drop), "feather"),
    sp("duck", "Duck", O::Farm, G::Waddle, 9.0, 0, 9.0, &["quack", "qwak"], [2, 2, 1, 3, 3, 1, 2], S::Emote),
    sp("dog", "Dog", O::Farm, G::Walk, 13.0, 0, 9.0, &["woof", "boof", "go on"], [4, 0, 1, 5, 4, 0, 2], S::Spin),
    sp("cat", "Cat", O::Farm, G::Walk, 11.0, 0, 9.0, &["mrrp", "mew", "prr"], [3, 2, 4, 2, 1, 2, 1], S::Loaf),
    sp("bunny", "Bunny", O::Farm, G::Hop, 14.0, 0, 9.0, &["!", "*thump*"], [2, 4, 1, 3, 2, 0, 2], S::Hops),
    sp("hedgehog", "Hedgehog", O::Farm, G::Walk, 6.0, NIGHT_BIAS, 8.0, &["hff", "snff"], [1, 4, 3, 1, 1, 2, 2], S::Hide),
    sp("frog", "Frog", O::Farm, G::Hop, 12.0, 0, 8.0, &["ribbit", "rbt"], [2, 2, 2, 4, 2, 1, 1], S::Hops),
    stay(sp("capybara", "Capybara", O::Farm, G::Walk, 7.0, 0, 7.0, &["...", "ok", "chill"], [1, 0, 4, 0, 5, 0, 2], S::Loaf), 1.4),
    sp("fox", "Fox", O::Farm, G::Walk, 13.0, 0, 7.0, &["yip", "heh"], [4, 3, 1, 3, 1, 1, 2], S::Pounce),
    stay(sp("snail", "Snail", O::Farm, G::Slide, 2.6, TRAIL, 8.0, &[], [2, 3, 3, 0, 1, 1, 1], S::Hide), 1.6),
    stay(sp("slug", "Slug", O::Farm, G::Slide, 2.4, TRAIL | NIGHT_BIAS, 6.0, &[], [1, 3, 3, 0, 1, 1, 2], S::Hide), 1.6),
    sp("crab", "Crab", O::Farm, G::Sideways, 10.0, 0, 7.0, &["*click*", "snip"], [2, 2, 1, 2, 1, 4, 1], S::Snip),
    sp("mushroom", "Mushroom", O::Farm, G::Walk, 6.0, 0, 6.0, &["...", "spore"], [2, 3, 3, 1, 1, 1, 1], S::Spores),
    hover(sp("ghost", "Ghost", O::Farm, G::Float, 6.0, GHOSTLY | NOCTURNAL, 6.0, &["boo", "hi", "oOo"], [3, 3, 1, 3, 1, 1, 0], S::Teleport), 3.0),
    sp("mouse", "Mouse", O::Farm, G::Dart, 16.0, NIGHT_BIAS, 7.0, &["eek", "squeak"], [2, 5, 1, 2, 1, 0, 3], S::Emote),
    // Old school
    stay(sp("adventurer", "Adventurer", O::Osrs, G::Walk, 10.0, 0, 6.0, &["gz", "nice", "brb", "afk", "ty"], [3, 1, 1, 2, 3, 1, 1], S::Mine), 1.2),
    sp("goblin", "Goblin", O::Osrs, G::Walk, 9.0, 0, 5.0, &["grr", "me smash", "gimme"], [2, 1, 1, 2, 0, 5, 3], S::Emote),
    sp("imp", "Imp", O::Osrs, G::Hop, 12.0, 0, 4.0, &["hehe", "*blink*"], [3, 2, 0, 5, 1, 2, 1], S::Teleport),
    sp("mole", "Mole", O::Osrs, G::Walk, 6.0, 0, 5.0, &["dig dig", "*snff*"], [2, 4, 2, 1, 1, 1, 2], S::Dig),
    stay(sp("pet_rock", "Pet rock", O::Osrs, G::Slide, 1.5, DROPS_IN, 3.0, &[], [0, 0, 5, 0, 0, 0, 0], S::Nothing), 1.5),
    hover(stay(sp("genie", "Genie", O::Osrs, G::Float, 5.0, GHOSTLY, 1.0, &["lamp for you", "a wish?"], [5, 0, 0, 1, 4, 0, 0], S::Lamp), 0.8), 4.0),
    // Blocky
    sp("boom_bug", "Boom-bug", O::Sandbox, G::Walk, 8.0, 0, 4.0, &["sss...", "tsss"], [3, 2, 0, 2, 1, 3, 0], S::Fuse),
    sp("slime", "Cube slime", O::Sandbox, G::Hop, 7.0, 0, 6.0, &["blorp", "boing"], [2, 1, 1, 5, 3, 0, 2], S::Hops),
    sp("slime_mini", "Mini slime", O::Sandbox, G::Hop, 9.0, NO_SPAWN, 0.0, &["blip"], [2, 1, 0, 5, 5, 0, 1], S::Hops),
    sp("axolotl", "Axolotl", O::Sandbox, G::Walk, 6.0, 0, 6.0, &[":)", "blub", "hi!"], [3, 1, 3, 2, 4, 0, 1], S::Emote),
    hover(sp("bee", "Bee", O::Sandbox, G::Fly, 12.0, FLIER | MORNING, 6.0, &["bzz", "bzzt"], [3, 1, 0, 4, 2, 1, 3], S::Buzz), 8.0),
    hover(sp("wisp", "Wisp", O::Sandbox, G::Float, 9.0, SPARKLE, 4.0, &["la la", "for you!", "~"], [5, 0, 1, 3, 4, 0, 0], S::Fetch), 6.0),
    // Mushroom kingdom
    stay(sp("snail_green", "Green snail", O::Maple, G::Slide, 3.0, 0, 5.0, &[], [1, 3, 2, 1, 2, 1, 1], S::Hide), 1.4),
    stay(sp("snail_blue", "Blue snail", O::Maple, G::Slide, 3.0, 0, 4.0, &[], [1, 3, 2, 1, 2, 1, 1], S::Hide), 1.4),
    stay(sp("snail_red", "Red snail", O::Maple, G::Slide, 3.0, 0, 3.0, &[], [1, 3, 2, 1, 2, 2, 1], S::Hide), 1.4),
    sp("orange_mushroom", "Orange mushroom", O::Maple, G::Hop, 8.0, 0, 5.0, &["boing", "hup"], [2, 1, 1, 5, 2, 1, 1], S::Bounce),
    sp("ribbon_pig", "Ribbon pig", O::Maple, G::Walk, 9.0, 0, 4.0, &["oink", "*twirl*"], [2, 1, 2, 4, 3, 0, 3], S::Spin),
    stay(sp("stump", "Stump", O::Maple, G::Walk, 4.0, 0, 4.0, &["...", "creak"], [1, 2, 4, 0, 2, 3, 0], S::Emote), 1.3),
    // Made in NZ
    hover(sp("balloon", "Balloon", O::NzGames, G::Float, 6.0, INANIMATE, 2.0, &[], EVEN, S::Emote), 12.0),
    sp("dart_monkey", "Dart monkey", O::NzGames, G::Walk, 14.0, NO_SPAWN, 0.0, &["pop!", "ook", "got it"], [3, 0, 0, 5, 2, 0, 1], S::Emote),
    sp("whelp", "Dragon whelp", O::NzGames, G::Hop, 10.0, FLIER, 1.0, &["rawr", "*puff*", "squee"], [3, 1, 2, 4, 1, 2, 2], S::Smoke),
];

/// A species by id.
pub fn by_id(id: &str) -> Option<usize> {
    SPECIES.iter().position(|s| s.id == id)
}

/// Names a visitor might be given. Short enough for a name tag; macrons
/// are fine (the font has them).
pub const NAMES: &[&str] = &[
    "Pip",
    "Biscuit",
    "Kūmara",
    "Jandal",
    "Pavlova",
    "Pickle",
    "Feijoa",
    "Gumboot",
    "Pāua",
    "Lamington",
    "Hokey Pokey",
    "Kina",
    "Pipi",
    "Tuatua",
    "Rimu",
    "Kauri",
    "Tōtara",
    "Mānuka",
    "Fern",
    "Moss",
    "Pebble",
    "Scone",
    "Crumpet",
    "Nugget",
    "Toast",
    "Dumpling",
    "Waffles",
    "Noodle",
    "Bean",
    "Muffin",
    "Chippy",
    "Sausage",
    "Possum",
    "Puddle",
    "Sprout",
    "Button",
    "Mo",
    "Bubbles",
    "Jellybean",
    "Kōwhai",
    "Hoki",
    "Flatwhite",
    "Marmalade",
    "Pōhutu",
];

/// Prefixes a name sometimes gets ("Sir Pickle", "Wee Kina").
pub const PREFIXES: &[&str] = &[
    "Sir", "Lady", "Wee", "Big", "Old", "Little", "Captain", "Dr",
];
