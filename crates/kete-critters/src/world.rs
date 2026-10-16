//! The world: visitors, their brains, the physics of being picked up and
//! thrown, and the particles. Deterministic from a seed: the same seed and
//! the same inputs give the same run, which is what lets the desktop's
//! TypeScript port be tested against this one.
//!
//! Units are sprite pixels and seconds. `x` is a critter's centre; height
//! is `lift`, the feet's distance above the floor (negative while sinking
//! into it), so a floor that moves (a resized terminal) carries everyone
//! with it. Particles use the same `x`/`rise` pair.
//!
//! [`World::tick`] runs fixed [`STEP`]s off an accumulator of whole
//! microseconds, and every timer counts whole steps, so an 8 fps terminal
//! and a 60 fps window run exactly the same steps for the same time.
//! Between visits (nothing on screen) a tick jumps straight to the next
//! visitor instead of stepping through the gap.

use std::collections::{BTreeMap, VecDeque};

use crate::art::{Pose, Rgb};
use crate::cast::sprite;
use crate::font::Icon;
use crate::rng::Rng;
use crate::species::{
    by_id, flag, relation, Gait, Personality, Relation, Special, Species, NAMES, PREFIXES, SPECIES,
};

/// The fixed simulation step: 1/32 s, a whole number of microseconds.
pub const STEP: f64 = 0.031_25;
pub const STEP_US: u64 = 31_250;
/// The most time one `tick` will simulate while something is on screen
/// (a laptop waking from sleep shouldn't fast-forward a whole visit).
pub const MAX_TICK_US: u64 = 1_000_000;

/// Seconds to whole steps (at least one).
pub fn steps(secs: f64) -> u64 {
    (secs / STEP).round().max(1.0) as u64
}
/// Gravity, px/s².
pub const GRAVITY: f64 = 320.0;
/// Vertical speed kept on a bounce.
pub const BOUNCE: f64 = 0.45;
/// Horizontal speed kept on a bounce.
pub const FRICTION: f64 = 0.7;
/// Speed kept off a side wall.
pub const WALL_BOUNCE: f64 = 0.5;
/// Below this landing speed a critter settles instead of bouncing.
pub const SETTLE_SPEED: f64 = 30.0;
/// Fling speed cap, px/s.
pub const MAX_FLING: f64 = 360.0;
/// How long the squash frame shows on landing.
pub const SQUASH_TIME: f64 = 0.16;
/// How long a blink keeps the eyes closed.
pub const BLINK_TIME: f64 = 0.14;
/// Two visitors at most, plus whatever an event brings (split slimes, the
/// dart monkey).
pub const MAX_VISITORS: usize = 2;
/// How long a dropped item lies on the floor.
pub const ITEM_LIFE: f64 = 20.0;
/// The flight arc's peak above the straight line, capped by the view.
pub const FLIGHT_ARC: f64 = 20.0;

/// Every word the world itself puts in a bubble or a text pop (the font
/// coverage test reads it; species lines and drops are in the cast table).
pub const WORDS: &[&str] = &[
    "gz",
    "loot!",
    "+xp lamp",
    "enjoy",
    "hehe",
    "nom",
    "mine!",
    "boom",
    "heh",
    "+25 xp",
    "lamp for you",
    "sss...",
    "*puff*",
    "pop!",
    "-1 shiny",
    "for you!",
    "...fine",
    "loot",
];

/// The brain uses no transcendental functions (sin, cos, sqrt...), whose
/// last bit differs between C runtimes and JavaScript engines: only
/// arithmetic, so the TypeScript port can replay a run exactly. Arcs are
/// parabolas and bobs triangle waves.
///
/// An arch over `u` in [0, 1]: 0 at both ends, 1 in the middle.
pub fn arch(u: f64) -> f64 {
    4.0 * u * (1.0 - u)
}

/// A triangle wave of period 1 in [-1, 1]: -1 at whole numbers, 1 at
/// halves. `fract` here is `x - trunc(x)`, JavaScript's `x - Math.trunc(x)`.
pub fn wave(t: f64) -> f64 {
    1.0 - 4.0 * (t.fract().abs() - 0.5).abs()
}

/// Six directions a puff of smoke goes, every 60 degrees from east.
pub const RING: [(f64, f64); 6] = [
    (1.0, 0.0),
    (0.5, 0.866_025_403_784_438_6),
    (-0.5, 0.866_025_403_784_438_6),
    (-1.0, 0.0),
    (-0.5, -0.866_025_403_784_438_6),
    (0.5, -0.866_025_403_784_438_6),
];

/// The local hours that matter, as [from, until): night (nocturnal species
/// visit), late (idle critters sleep), morning (tūī come).
pub const NIGHT: (u8, u8) = (20, 6);
pub const LATE: (u8, u8) = (22, 5);
pub const MORNING: (u8, u8) = (5, 10);

/// Night: from 20:00 until 06:00.
pub fn is_night(hour: u8) -> bool {
    !(NIGHT.1..NIGHT.0).contains(&hour)
}

/// Late: from 22:00 until 05:00.
pub fn is_late(hour: u8) -> bool {
    !(LATE.1..LATE.0).contains(&hour)
}

/// The visit rate setting (`critters_often`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Often {
    Rare,
    #[default]
    Normal,
    Busy,
}

impl Often {
    pub fn parse(s: &str) -> Often {
        match s {
            "rare" => Often::Rare,
            "busy" => Often::Busy,
            _ => Often::Normal,
        }
    }

    /// Seconds between visits: (min, max).
    pub fn gap(self) -> (f64, f64) {
        match self {
            Often::Rare => (120.0, 300.0),
            Often::Normal => (45.0, 120.0),
            Often::Busy => (15.0, 45.0),
        }
    }

    /// The chance a second visitor drops in during a visit.
    pub fn second(self) -> f64 {
        match self {
            Often::Rare => 0.1,
            Often::Normal => 0.15,
            Often::Busy => 0.3,
        }
    }
}

/// The states of the brain's weighted machine: where a visitor goes next
/// is a weighted pick from the row of the state it's leaving.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum State {
    Walk,
    Idle,
    Look,
    Eat,
    Sit,
    Sleep,
    Emote,
    Say,
    Hop,
    Special,
    Start,
}

impl State {
    /// The states a visitor can go to, in `NEXT` column order.
    pub const TO: [State; 10] = [
        State::Walk,
        State::Idle,
        State::Look,
        State::Eat,
        State::Sit,
        State::Sleep,
        State::Emote,
        State::Say,
        State::Hop,
        State::Special,
    ];
    /// Every state, in `NEXT` row order.
    pub const ALL: [State; 11] = [
        State::Walk,
        State::Idle,
        State::Look,
        State::Eat,
        State::Sit,
        State::Sleep,
        State::Emote,
        State::Say,
        State::Hop,
        State::Special,
        State::Start,
    ];

    pub fn name(self) -> &'static str {
        match self {
            State::Walk => "walk",
            State::Idle => "idle",
            State::Look => "look",
            State::Eat => "eat",
            State::Sit => "sit",
            State::Sleep => "sleep",
            State::Emote => "emote",
            State::Say => "say",
            State::Hop => "hop",
            State::Special => "special",
            State::Start => "start",
        }
    }
}

/// The overlay's NEXT table: rows are the state left (`State::ALL`
/// order), columns the state entered (`State::TO` order).
#[rustfmt::skip]
pub const NEXT: [[f64; 10]; 11] = [
    //        walk idle look eat  sit  slp  emo  say  hop  spec
    /* walk */ [2.0, 3.0, 2.0, 2.0, 1.0, 0.0, 1.0, 1.0, 1.0, 1.0],
    /* idle */ [4.0, 0.0, 2.0, 1.0, 1.0, 0.0, 1.0, 1.0, 0.0, 1.0],
    /* look */ [3.0, 2.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0],
    /* eat  */ [3.0, 2.0, 0.0, 1.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0],
    /* sit  */ [2.0, 1.0, 0.0, 0.0, 0.0, 3.0, 1.0, 0.0, 0.0, 0.0],
    /* slp  */ [2.0, 2.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0],
    /* emo  */ [3.0, 2.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0],
    /* say  */ [3.0, 2.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0],
    /* hop  */ [2.0, 2.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0],
    /* spec */ [3.0, 2.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0],
    /* strt */ [5.0, 2.0, 2.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0],
];

/// Personality multipliers on the NEXT columns (`Personality::ALL` rows,
/// `State::TO` columns).
#[rustfmt::skip]
pub const MOODS: [[f64; 10]; 7] = [
    //            walk idle look eat  sit  slp  emo  say  hop  spec
    /* curious */ [1.2, 1.0, 2.0, 1.0, 0.8, 0.8, 1.0, 1.0, 1.0, 1.2],
    /* shy     */ [1.0, 1.2, 1.5, 1.0, 0.8, 1.0, 0.6, 0.4, 0.8, 1.0],
    /* sleepy  */ [0.6, 1.0, 0.8, 1.0, 2.5, 2.5, 0.8, 0.8, 0.5, 0.8],
    /* playful */ [1.3, 0.8, 1.0, 1.0, 0.6, 0.6, 1.5, 1.2, 3.0, 1.5],
    /* social  */ [1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.5, 1.5, 1.0, 1.0],
    /* grumpy  */ [1.0, 1.5, 1.0, 1.0, 1.0, 1.0, 0.5, 0.3, 0.6, 0.8],
    /* greedy  */ [1.0, 1.0, 1.0, 3.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0],
];

/// When nothing is running and it's late, sit and sleep weigh this much
/// more (and walking a third as much).
pub const LATE_SLEEP: f64 = 6.0;

/// Colours the world draws with.
pub mod ink {
    use crate::art::Rgb;
    pub const SOOT: Rgb = [0x1e, 0x18, 0x26];
    pub const BUBBLE: Rgb = [0xfb, 0xf6, 0xea];
    pub const TEXT: Rgb = [0x2a, 0x22, 0x30];
    pub const GOLD: Rgb = [0xff, 0xd1, 0x66];
    pub const BONE: Rgb = [0xef, 0xe9, 0xdc];
    pub const DUST: Rgb = [0xdc, 0xd2, 0xc8];
    pub const TRAIL: Rgb = [0xd2, 0xe6, 0xff];
    pub const CRUMB: Rgb = [0xd9, 0xa8, 0x5c];
    pub const SMOKE: Rgb = [0xb8, 0xb4, 0xbe];
    pub const BEAM: Rgb = [0xc0, 0x8c, 0xff];
    pub const FLASH: Rgb = [0xf6, 0xf4, 0xf0];
    pub const CONFETTI: [Rgb; 5] = [
        [0xf0, 0x74, 0x8a],
        [0xff, 0xd1, 0x66],
        [0x7f, 0xc8, 0xa9],
        [0x9a, 0xd0, 0xff],
        [0xc0, 0x8c, 0xff],
    ];
}

#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct Pointer {
    pub x: f64,
    pub y: f64,
}

/// Something that happened in the app, edge-triggered: the host passes
/// it in the one tick after it happens.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum AppEvent {
    /// An agent needs you; `x` is where (the lane's column), if known.
    NeedsYou { x: Option<f64> },
    /// A turn finished.
    TurnDone,
    /// A pull request merged; `x` is where to put the loot beam.
    PrMerged { x: Option<f64> },
}

/// What the world needs from its host each tick. Everything is in sprite
/// pixels from the top-left of the overlay.
#[derive(Clone, Debug, PartialEq)]
pub struct Input {
    pub view_w: f64,
    pub view_h: f64,
    /// The floor line: critters' feet stand on it (sprite rows above it
    /// are drawn, rows at or below it are clipped).
    pub floor: f64,
    /// Surfaces fliers may perch on: the top edge of each rect.
    pub perches: Vec<Rect>,
    /// The pointer, when it's over the overlay.
    pub pointer: Option<Pointer>,
    /// The local hour, 0–23.
    pub hour: u8,
    /// Hold poses, no particles, no walking on.
    pub reduced_motion: bool,
    /// Nothing is running.
    pub all_idle: bool,
    pub events: Vec<AppEvent>,
}

impl Default for Input {
    fn default() -> Self {
        Input {
            view_w: 160.0,
            view_h: 60.0,
            floor: 56.0,
            perches: Vec::new(),
            pointer: None,
            hour: 12,
            reduced_motion: false,
            all_idle: false,
            events: Vec::new(),
        }
    }
}

/// What the host might want to remember (the collection log).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Note {
    Seen {
        species: &'static str,
    },
    Petted {
        species: &'static str,
    },
    Fed {
        species: &'static str,
    },
    Named {
        species: &'static str,
        name: String,
    },
    /// The genie's lamp was taken.
    Lamp,
}

/// A speech bubble over a critter.
#[derive(Clone, Debug, PartialEq)]
pub struct Bubble {
    pub text: String,
    /// Seconds left.
    pub t: f64,
}

/// What's driving a critter's position.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    /// The brain's actions.
    Brain,
    /// Following the pointer.
    Held,
    /// Falling or flung.
    Air,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct Hold {
    /// Pointer-to-critter offset at the grab.
    dx: f64,
    dy: f64,
    /// Smoothed pointer velocity (screen axes, px/s).
    vx: f64,
    vy: f64,
    last: Option<(f64, f64)>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Mark {
    Entered,
    Gone,
    MeetingOver,
    /// On its way out (what follows may leave the screen).
    Exiting,
}

/// One step of a critter's plan. Each runs until it says it's done.
#[derive(Clone, Debug, PartialEq)]
enum Act {
    Pose {
        pose: Pose,
        t: f64,
    },
    Turn,
    Face(f64),
    FaceX(f64),
    Fade {
        to: f64,
        t: f64,
        from: Option<f64>,
        e: f64,
    },
    WalkTo {
        x: f64,
        pace: f64,
    },
    /// Steps up or down through the floor line (lift `from` to `to`).
    Stairs {
        from: f64,
        to: f64,
        fade_in: bool,
        dist: f64,
    },
    /// An arc to `x` at `lift` (absolute: `perch` is the perch's top y).
    FlyTo {
        x: f64,
        lift: f64,
        perch: Option<f64>,
        from: Option<(f64, f64)>,
        e: f64,
        total: f64,
    },
    Eat {
        t: f64,
    },
    Sleep {
        t: f64,
        zt: f64,
    },
    Emote(Icon),
    Say(String),
    Hops {
        n: f64,
        e: f64,
        h: f64,
    },
    Pushups {
        n: f64,
        e: f64,
    },
    Wait(f64),
    Pop {
        text: String,
        color: Rgb,
    },
    DropItem,
    Puff,
    Spores,
    /// Lift moves to `to` at 8 px/s with dust (digging).
    Sink {
        to: f64,
    },
    /// Jumps to `x` (teleport, tunnelling).
    Jump(f64),
    /// The boom-bug's fuse.
    Flash {
        t: f64,
    },
    /// Confetti, then gone.
    Burst,
    /// The weka takes the pointer's shiny.
    Pinch,
    /// The wisp carries the nearest item to `to`.
    Carry {
        item: Option<usize>,
        to: f64,
    },
    /// The dart monkey throws at a balloon.
    Throw {
        target: u32,
    },
    /// Pops this critter (balloon).
    PopSelf,
    /// Two minis become one slime.
    Merge {
        with: u32,
    },
    /// The genie waits to be clicked.
    Offer {
        t: f64,
    },
    /// Smoke from the mouth (whelp).
    Breathe,
    Mark(Mark),
}

/// A visitor.
#[derive(Clone, Debug, PartialEq)]
pub struct Critter {
    pub id: u32,
    pub species: usize,
    pub name: String,
    pub personality: Personality,
    pub x: f64,
    /// Feet height above the floor.
    pub lift: f64,
    /// 1 faces right, -1 left.
    pub dir: f64,
    pub pose: Pose,
    pub frame: usize,
    pub alpha: f64,
    pub blink: bool,
    /// Drawn all-white this tick (the boom-bug's fuse).
    pub flash: bool,
    pub bubble: Option<Bubble>,
    pub mode: Mode,
    pub entered: bool,
    pub exiting: bool,
    pub gone: bool,
    vx: f64,
    vlift: f64,
    /// The perch top (screen y) it's standing on, if perched.
    perch: Option<f64>,
    hold: Option<Hold>,
    anim: f64,
    age: f64,
    budget: f64,
    blink_t: f64,
    squash_t: f64,
    meeting: bool,
    prev: State,
    acts: VecDeque<Act>,
    cur: Option<Act>,
    trail_d: f64,
    /// Lamp offered and not yet taken.
    offering: bool,
}

impl Critter {
    pub fn species(&self) -> &'static Species {
        &SPECIES[self.species]
    }

    fn sp(&self) -> &'static Species {
        &SPECIES[self.species]
    }

    fn half_w(&self) -> f64 {
        sprite(self.species).w as f64 / 2.0
    }

    fn height(&self) -> f64 {
        sprite(self.species).h as f64
    }

    fn grounded(&self) -> bool {
        self.mode == Mode::Brain && self.perch.is_none() && self.lift.abs() < 0.5
    }

    /// The head's position (x, rise above the floor): where emotes,
    /// bubbles and z's come from.
    pub fn head(&self) -> (f64, f64) {
        let h = self.height();
        (
            self.x + self.dir * sprite(self.species).w as f64 * 0.2,
            self.lift + h - 1.0,
        )
    }
}

/// A particle kind. Icons and text float up; dust, crumbs and confetti
/// fall; trails and items lie on the floor.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Kind {
    Dust,
    Trail,
    Sparkle,
    Crumb,
    Confetti,
    Smoke,
    Icon(Icon),
    /// A word floating up (`text`): "+25 xp", "cowhide".
    Text,
    /// A dropped thing lying on the floor; `text` names it.
    Item,
    /// A loot beam: a column of light at `x`.
    Beam,
    /// A thrown dart.
    Dart,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Particle {
    pub kind: Kind,
    pub x: f64,
    /// Height above the floor.
    pub rise: f64,
    pub vx: f64,
    pub vrise: f64,
    /// px/s² pulling `rise` down.
    pub gravity: f64,
    /// Seconds left, and the total (for fades).
    pub life: f64,
    pub max: f64,
    pub color: Rgb,
    pub text: String,
    /// The critter carrying it (items).
    pub carried_by: Option<u32>,
}

impl Particle {
    fn new(kind: Kind, x: f64, rise: f64, life: f64, color: Rgb) -> Particle {
        Particle {
            kind,
            x,
            rise,
            vx: 0.0,
            vrise: 0.0,
            gravity: 0.0,
            life,
            max: life,
            color,
            text: String::new(),
            carried_by: None,
        }
    }

    /// Opacity from the remaining life: full until the last third.
    pub fn alpha(&self) -> f64 {
        (self.life / self.max * 3.0).clamp(0.0, 1.0)
    }
}

/// What one critter's update asks of the world.
#[derive(Debug)]
enum Effect {
    /// Replace a critter's plan.
    Plan(u32, Vec<Act>),
    Merge(u32, u32),
    Split(u32),
    /// A dart monkey comes for this balloon.
    Monkey(u32),
}

/// Read-only view of the world a critter's update gets.
struct Ctx<'a> {
    dt: f64,
    view_w: f64,
    view_h: f64,
    floor: f64,
    perches: &'a [Rect],
    pointer: Option<Pointer>,
    reduced: bool,
    /// (id, species, x, lift) of everyone, before this step.
    others: &'a [(u32, usize, f64, f64)],
}

/// The world.
#[derive(Clone, Debug)]
pub struct World {
    pub critters: Vec<Critter>,
    pub particles: Vec<Particle>,
    pub rng: Rng,
    /// Steps simulated so far.
    pub steps: u64,
    pub often: Often,
    /// Visitors come on their own; off after "hide critters until
    /// tomorrow" (the host turns it back on).
    pub spawning: bool,
    /// The critter under the pointer (its name shows).
    pub hover: Option<u32>,
    /// The view as of the last tick.
    pub view_w: f64,
    pub view_h: f64,
    pub floor: f64,
    /// Wall time not yet stepped, in microseconds.
    acc_us: u64,
    /// Steps until the next visitor, counted while nobody is on screen.
    spawn_in: u64,
    /// Steps until a second visitor joins the first, if one will.
    second_in: Option<u64>,
    /// Steps until two visitors may meet, counted while two are here.
    pub(crate) meet_in: u64,
    /// App events waiting for the next step.
    pending: Vec<AppEvent>,
    next_id: u32,
    last_species: Option<usize>,
    /// Names given by the user, by species id: a returning species keeps
    /// its name.
    pub names: BTreeMap<String, String>,
    notes: Vec<Note>,
    held: Option<u32>,
    /// Reduced motion, as of the last tick.
    reduced: bool,
}

impl World {
    pub fn new(seed: u32) -> World {
        let mut rng = Rng::new(seed);
        // The first visitor comes soon after the critters are turned on.
        let spawn_in = steps(rng.range(3.0, 8.0));
        World {
            critters: Vec::new(),
            particles: Vec::new(),
            rng,
            steps: 0,
            often: Often::Normal,
            spawning: true,
            hover: None,
            view_w: Input::default().view_w,
            view_h: Input::default().view_h,
            floor: Input::default().floor,
            acc_us: 0,
            spawn_in,
            second_in: None,
            meet_in: steps(10.0),
            pending: Vec::new(),
            next_id: 1,
            last_species: None,
            names: BTreeMap::new(),
            notes: Vec::new(),
            held: None,
            reduced: false,
        }
    }

    /// Anything to animate: a critter or a particle on screen.
    pub fn active(&self) -> bool {
        !self.critters.is_empty() || !self.particles.is_empty()
    }

    /// Seconds until the world next needs a tick when nothing is on
    /// screen (the spawn timer), or `None` while it's active (tick every
    /// frame) or idle for good (spawning off).
    pub fn next_wake(&self) -> Option<f64> {
        if self.active() || !self.spawning {
            return None;
        }
        let due = self.spawn_in as f64 * STEP - self.acc_us as f64 / 1e6;
        Some(due.max(0.0))
    }

    /// What happened since the last call (for the collection log).
    pub fn drain_notes(&mut self) -> Vec<Note> {
        std::mem::take(&mut self.notes)
    }

    pub fn get(&self, id: u32) -> Option<&Critter> {
        self.critters.iter().find(|c| c.id == id)
    }

    fn get_mut(&mut self, id: u32) -> Option<&mut Critter> {
        self.critters.iter_mut().find(|c| c.id == id)
    }

    /// Simulated seconds.
    pub fn clock(&self) -> f64 {
        self.steps as f64 * STEP
    }

    /// Advances the world by `dt` seconds of wall time.
    pub fn tick(&mut self, dt: f64, input: &Input) {
        self.view_w = input.view_w.max(1.0);
        self.view_h = input.view_h.max(1.0);
        self.floor = input.floor;
        self.reduced = input.reduced_motion;
        self.pending.extend(input.events.iter().copied());
        self.acc_us += (dt.max(0.0) * 1e6).round() as u64;
        // Nobody here: the steps up to the next visitor change nothing but
        // the clock, so take them in one go.
        if !self.active() && self.pending.is_empty() {
            let have = self.acc_us / STEP_US;
            let skip = if self.spawning {
                have.min(self.spawn_in.saturating_sub(1))
            } else {
                have
            };
            self.steps += skip;
            if self.spawning {
                self.spawn_in -= skip;
            }
            self.acc_us -= skip * STEP_US;
        }
        self.acc_us = self.acc_us.min(MAX_TICK_US);
        while self.acc_us >= STEP_US {
            self.acc_us -= STEP_US;
            self.step(input);
        }
        self.hover = input.pointer.and_then(|p| self.hit(p.x, p.y));
    }

    fn step(&mut self, input: &Input) {
        let dt = STEP;
        self.steps += 1;
        for ev in std::mem::take(&mut self.pending) {
            self.app_event(ev, input);
        }
        self.stage(input);

        let others: Vec<(u32, usize, f64, f64)> = self
            .critters
            .iter()
            .map(|c| (c.id, c.species, c.x, c.lift))
            .collect();
        let ctx = Ctx {
            dt,
            view_w: self.view_w,
            view_h: self.view_h,
            floor: self.floor,
            perches: &input.perches,
            pointer: input.pointer,
            reduced: input.reduced_motion,
            others: &others,
        };
        let mut effects = Vec::new();
        let late = input.all_idle && is_late(input.hour);
        for c in self.critters.iter_mut() {
            update(
                c,
                &ctx,
                late,
                &mut self.rng,
                &mut self.particles,
                &mut effects,
            );
        }
        for e in effects {
            self.apply(e);
        }
        if self
            .held
            .is_some_and(|id| self.get(id).is_none_or(|c| c.mode != Mode::Held))
        {
            self.held = None;
        }
        self.critters.retain(|c| !c.gone);
        tick_particles(
            &mut self.particles,
            &self.critters,
            dt,
            input.reduced_motion,
        );
    }

    fn apply(&mut self, e: Effect) {
        match e {
            Effect::Plan(id, acts) => {
                if let Some(c) = self.get_mut(id) {
                    if c.mode == Mode::Brain {
                        c.cur = None;
                        c.acts = acts.into();
                    }
                }
            }
            Effect::Merge(a, b) => {
                let (Some(ca), Some(cb)) = (self.get(a).cloned(), self.get(b).cloned()) else {
                    return;
                };
                if ca.gone || cb.gone {
                    return;
                }
                for id in [a, b] {
                    if let Some(c) = self.get_mut(id) {
                        c.gone = true;
                    }
                }
                if let Some(slime) = by_id("slime") {
                    let x = (ca.x + cb.x) / 2.0;
                    let id = self.add(slime, x, ca.dir);
                    let c = self.get_mut(id).expect("just added");
                    c.entered = true;
                    c.squash_t = SQUASH_TIME * 2.0;
                    c.acts = vec![
                        Act::Emote(Icon::Heart),
                        Act::Pose {
                            pose: Pose::Idle,
                            t: 1.0,
                        },
                    ]
                    .into();
                    self.burst(x, 4.0, 6, false);
                }
            }
            Effect::Split(id) => {
                let Some(c) = self.get(id).cloned() else {
                    return;
                };
                if let Some(c) = self.get_mut(id) {
                    c.gone = true;
                }
                if let Some(mini) = by_id("slime_mini") {
                    for side in [-1.0, 1.0] {
                        let nid = self.add(mini, c.x + side * 5.0, side);
                        let m = self.get_mut(nid).expect("just added");
                        m.entered = true;
                        m.mode = Mode::Air;
                        m.vx = side * 40.0;
                        m.vlift = 70.0;
                        m.budget = c.budget.max(30.0);
                    }
                }
                self.burst(c.x, 4.0, 5, false);
            }
            Effect::Monkey(bid) => self.monkey(bid),
        }
    }

    /// The made-in-NZ nod: a dart monkey runs in under the balloon, throws,
    /// the balloon pops, the monkey leaves pleased with itself.
    fn monkey(&mut self, balloon: u32) {
        let (Some(b), Some(m)) = (self.get(balloon).cloned(), by_id("dart_monkey")) else {
            return;
        };
        let w = self.view_w;
        let side = if b.x < w / 2.0 { -1.0 } else { 1.0 };
        let hw = sprite(m).w as f64 / 2.0;
        let id = self.add(m, if side < 0.0 { -hw } else { w + hw }, -side);
        let stand = (b.x + side * 10.0).clamp(hw, w - hw);
        let mut acts = vec![
            Act::WalkTo {
                x: stand,
                pace: 1.0,
            },
            Act::FaceX(b.x),
            Act::Wait(0.3),
            Act::Throw { target: balloon },
            Act::Wait(0.7),
            Act::Say("pop!".into()),
            Act::Emote(Icon::Spark),
            Act::Pose {
                pose: Pose::Idle,
                t: 0.8,
            },
            Act::Mark(Mark::Entered),
        ];
        let c = self.get(id).cloned().expect("just added");
        let mut rng = self.rng.clone();
        acts.extend(exit_plan(
            &Critter { x: stand, ..c },
            w,
            &mut rng,
            1.2,
            false,
        ));
        self.rng = rng;
        let c = self.get_mut(id).expect("just added");
        c.exiting = true;
        c.acts = acts.into();
    }

    fn burst(&mut self, x: f64, rise: f64, n: usize, confetti: bool) {
        for i in 0..n {
            let mut p = Particle::new(
                if confetti { Kind::Confetti } else { Kind::Dust },
                x,
                rise,
                if confetti { 1.6 } else { 0.5 },
                if confetti {
                    ink::CONFETTI[i % ink::CONFETTI.len()]
                } else {
                    ink::DUST
                },
            );
            p.vx = self.rng.range(-40.0, 40.0);
            p.vrise = self.rng.range(20.0, if confetti { 90.0 } else { 40.0 });
            p.gravity = if confetti { 120.0 } else { 80.0 };
            self.particles.push(p);
        }
    }

    // ---------- spawning and meetings ----------

    /// Night/morning-aware weights for picking a visitor.
    pub fn spawn_weights(&self, hour: u8, exclude: &[usize]) -> Vec<f64> {
        let night = is_night(hour);
        let morning = (MORNING.0..MORNING.1).contains(&hour);
        SPECIES
            .iter()
            .enumerate()
            .map(|(i, s)| {
                if exclude.contains(&i) || s.has(flag::NO_SPAWN) {
                    return 0.0;
                }
                if s.has(flag::NOCTURNAL) && !night {
                    return 0.0;
                }
                let mut w = s.weight;
                if s.has(flag::NIGHT_BIAS) && night {
                    w *= 2.0;
                }
                if s.has(flag::MORNING) {
                    w *= if morning { 3.0 } else { 0.6 };
                }
                w
            })
            .collect()
    }

    fn pick_species(&mut self, hour: u8) -> usize {
        let mut exclude: Vec<usize> = self.critters.iter().map(|c| c.species).collect();
        exclude.extend(self.last_species);
        let w = self.spawn_weights(hour, &exclude);
        self.rng.weighted(&w)
    }

    fn visitors(&self) -> usize {
        self.critters
            .iter()
            .filter(|c| !c.species().has(flag::NO_SPAWN))
            .count()
    }

    fn stage(&mut self, input: &Input) {
        if self.spawning {
            if self.critters.is_empty() {
                self.spawn_in = self.spawn_in.saturating_sub(1);
                if self.spawn_in == 0 {
                    let sp = self.pick_species(input.hour);
                    self.visit(sp, None, input);
                    self.second_in = self
                        .rng
                        .chance(self.often.second())
                        .then(|| steps(self.rng.range(5.0, 25.0)));
                    let (a, b) = self.often.gap();
                    self.spawn_in = steps(self.rng.range(a, b));
                }
            } else if let Some(t) = self.second_in {
                if (1..MAX_VISITORS).contains(&self.visitors()) {
                    if t <= 1 {
                        self.second_in = None;
                        let first = &self.critters[0];
                        if !first.exiting {
                            let side = if first.x < self.view_w / 2.0 {
                                1.0
                            } else {
                                -1.0
                            };
                            let sp = self.pick_species(input.hour);
                            self.visit(sp, Some(side), input);
                        }
                    } else {
                        self.second_in = Some(t - 1);
                    }
                }
            }
        }
        if self.critters.len() >= 2 {
            self.meet_in = self.meet_in.saturating_sub(1);
            if self.meet_in == 0 {
                self.meet(input);
            }
        }
    }

    /// Adds a critter at `x` on the floor with no plan (the caller gives
    /// it one). Returns its id.
    fn add(&mut self, species: usize, x: f64, dir: f64) -> u32 {
        let id = self.next_id;
        self.next_id += 1;
        let s = &SPECIES[species];
        let name = match self.names.get(s.id) {
            Some(n) => n.clone(),
            None => {
                let n = NAMES[self.rng.index(NAMES.len())];
                if self.rng.chance(0.25) {
                    format!("{} {n}", PREFIXES[self.rng.index(PREFIXES.len())])
                } else {
                    n.to_string()
                }
            }
        };
        let bias: Vec<f64> = s.bias.iter().map(|b| f64::from(*b)).collect();
        let personality = Personality::ALL[self.rng.weighted(&bias)];
        let budget = self.rng.range(40.0, 110.0) * s.stay;
        let blink_t = self.rng.range(2.0, 5.0);
        self.critters.push(Critter {
            id,
            species,
            name,
            personality,
            x,
            lift: 0.0,
            dir,
            pose: Pose::Idle,
            frame: 0,
            alpha: 1.0,
            blink: false,
            flash: false,
            bubble: None,
            mode: Mode::Brain,
            entered: false,
            exiting: false,
            gone: false,
            vx: 0.0,
            vlift: 0.0,
            perch: None,
            hold: None,
            anim: 0.0,
            age: 0.0,
            budget,
            blink_t,
            squash_t: 0.0,
            meeting: false,
            prev: State::Start,
            acts: VecDeque::new(),
            cur: None,
            trail_d: 0.0,
            offering: false,
        });
        id
    }

    /// A visitor arrives: from `side` (-1 left, 1 right) or a random one,
    /// by the species' way of arriving.
    fn visit(&mut self, species: usize, side: Option<f64>, input: &Input) -> u32 {
        let side = side.unwrap_or_else(|| self.rng.sign());
        let s = &SPECIES[species];
        let w = self.view_w;
        let hw = sprite(species).w as f64 / 2.0;
        let dir = -side;
        let id = self.add(species, 0.0, dir);
        self.last_species = Some(species);
        self.notes.push(Note::Seen { species: s.id });
        let edge_in = if side < 0.0 { hw + 2.0 } else { w - hw - 2.0 };
        let off = if side < 0.0 { -hw } else { w + hw };
        let into = |rng: &mut Rng, lo: f64, hi: f64| off + dir * rng.range(lo, hi) * w;
        let mut acts = Vec::new();
        let (x, lift, alpha, mode);
        if input.reduced_motion {
            // No walking on: appear somewhere and hold still.
            x = self.rng.range(0.2, 0.8) * w;
            (lift, alpha, mode) = (0.0, 1.0, Mode::Brain);
        } else if s.has(flag::GHOSTLY) {
            x = self.rng.range(0.15, 0.85) * w;
            (lift, alpha, mode) = (0.0, 0.0, Mode::Brain);
            acts.push(Act::Fade {
                to: 1.0,
                t: 1.5,
                from: None,
                e: 0.0,
            });
        } else if s.has(flag::DROPS_IN) {
            x = self.rng.range(0.2, 0.8) * w;
            (lift, alpha, mode) = ((input.floor - 2.0).max(10.0), 1.0, Mode::Air);
        } else if s.special == Special::Dig {
            x = self.rng.range(0.15, 0.85) * w;
            (lift, alpha, mode) = (-self_height(species), 1.0, Mode::Brain);
            acts.push(Act::Sink { to: 0.0 });
        } else if s.special == Special::Teleport && s.gait != Gait::Float {
            x = self.rng.range(0.15, 0.85) * w;
            (lift, alpha, mode) = (0.0, 1.0, Mode::Brain);
            acts.push(Act::Puff);
        } else if s.has(flag::FLIER) && (s.gait == Gait::Fly || self.rng.chance(0.34)) {
            x = off;
            let alt = self.rng.range(0.5, 0.9) * input.floor.max(8.0);
            (lift, alpha, mode) = (alt, 1.0, Mode::Brain);
            let tx = into(&mut self.rng, 0.1, 0.3);
            acts.push(Act::FlyTo {
                x: tx,
                lift: s.hover,
                perch: None,
                from: None,
                e: 0.0,
                total: 0.0,
            });
        } else if s.gait == Gait::Float || self.rng.chance(0.5) {
            x = off;
            (lift, alpha, mode) = (0.0, 1.0, Mode::Brain);
            let tx = into(&mut self.rng, 0.08, 0.3);
            acts.push(Act::WalkTo { x: tx, pace: 1.0 });
        } else {
            // Up the stairs: rises through the floor line at the edge.
            x = edge_in;
            let h = sprite(species).h as f64;
            (lift, alpha, mode) = (-h, 1.0, Mode::Brain);
            acts.push(Act::Stairs {
                from: -h,
                to: 0.0,
                fade_in: true,
                dist: 0.0,
            });
        }
        acts.push(Act::Mark(Mark::Entered));
        let c = self.get_mut(id).expect("just added");
        c.x = x;
        c.lift = lift;
        c.alpha = alpha;
        c.mode = mode;
        c.acts = acts.into();
        id
    }

    /// Two visitors on screen may meet: their relation decides how, and
    /// strangers meet in the middle and emote (the overlay's meetings).
    fn meet(&mut self, input: &Input) {
        let free: Vec<usize> = (0..self.critters.len())
            .filter(|i| {
                let c = &self.critters[*i];
                c.entered
                    && !c.exiting
                    && !c.meeting
                    && c.mode == Mode::Brain
                    && c.perch.is_none()
                    && !c.sp().has(flag::INANIMATE)
            })
            .collect();
        if free.len() < 2 || input.reduced_motion {
            self.meet_in = steps(2.0);
            return;
        }
        let (ia, ib) = (free[0], free[1]);
        let (a, b) = (&self.critters[ia], &self.critters[ib]);
        let social = a.personality == Personality::Social || b.personality == Personality::Social;
        self.meet_in = steps(self.rng.range(20.0, 40.0) * if social { 0.6 } else { 1.0 });
        let (sa, sb) = (a.sp().id, b.sp().id);
        let w = self.view_w;
        let (l, r) = if a.x < b.x { (ia, ib) } else { (ib, ia) };
        let (lc, rc) = (&self.critters[l], &self.critters[r]);
        let mid = ((lc.x + rc.x) / 2.0).clamp(0.15 * w, 0.85 * w);
        let gap = (lc.half_w() + rc.half_w()) * 0.7 + 3.0;
        let (lid, rid) = (lc.id, rc.id);
        let rel = |x: &str, y: &str| relation(x, y);
        let done = Act::Mark(Mark::MeetingOver);
        // Who chases whom (either way round).
        let chase = if rel(sa, sb) == Some(Relation::Chases) {
            Some((a.id, b.id, sb, sa))
        } else if rel(sb, sa) == Some(Relation::Chases) {
            Some((b.id, a.id, sa, sb))
        } else {
            None
        };
        let (l_acts, r_acts): (Vec<Act>, Vec<Act>);
        if let Some((hunter, prey, prey_sp, hunter_sp)) = chase {
            let prey_c = self.get(prey).expect("on screen");
            let flee_x = if prey_c.x < w / 2.0 {
                -prey_c.half_w() * 2.0
            } else {
                w + prey_c.half_w() * 2.0
            };
            let afraid = rel(prey_sp, hunter_sp) == Some(Relation::Fears);
            let hunt = vec![
                Act::Emote(Icon::Bang),
                Act::WalkTo {
                    x: prey_c.x,
                    pace: 1.8,
                },
                Act::Emote(Icon::Note),
                Act::Pose {
                    pose: Pose::Idle,
                    t: 1.0,
                },
                done.clone(),
            ];
            let prey_acts = if afraid {
                vec![
                    Act::Wait(0.3),
                    Act::Emote(Icon::Bang),
                    Act::WalkTo {
                        x: flee_x,
                        pace: 2.2,
                    },
                    Act::Mark(Mark::Gone),
                ]
            } else {
                vec![
                    Act::Emote(Icon::What),
                    Act::Hops {
                        n: 1.0,
                        e: 0.0,
                        h: 4.0,
                    },
                    done.clone(),
                ]
            };
            if hunter == lid {
                (l_acts, r_acts) = (hunt, prey_acts);
            } else {
                (l_acts, r_acts) = (prey_acts, hunt);
            }
            if afraid {
                if let Some(c) = self.get_mut(prey) {
                    c.exiting = true;
                }
            }
        } else if rel(sa, sb) == Some(Relation::Fears) || rel(sb, sa) == Some(Relation::Fears) {
            let scared = if rel(sa, sb) == Some(Relation::Fears) {
                a.id
            } else {
                b.id
            };
            let c = self.get(scared).expect("on screen");
            let flee_x = if c.x < w / 2.0 {
                -c.half_w() * 2.0
            } else {
                w + c.half_w() * 2.0
            };
            let run = vec![
                Act::Emote(Icon::Bang),
                Act::WalkTo {
                    x: flee_x,
                    pace: 2.2,
                },
                Act::Mark(Mark::Gone),
            ];
            let calm = vec![Act::Emote(Icon::What), done.clone()];
            if scared == lid {
                (l_acts, r_acts) = (run, calm);
            } else {
                (l_acts, r_acts) = (calm, run);
            }
            if let Some(c) = self.get_mut(scared) {
                c.exiting = true;
            }
        } else if rel(sa, sb) == Some(Relation::Merges) || rel(sb, sa) == Some(Relation::Merges) {
            l_acts = vec![
                Act::WalkTo {
                    x: mid - 2.0,
                    pace: 1.2,
                },
                Act::Merge { with: rid },
            ];
            r_acts = vec![
                Act::WalkTo {
                    x: mid + 2.0,
                    pace: 1.2,
                },
                Act::Wait(5.0),
                done.clone(),
            ];
        } else if rel(sa, sb) == Some(Relation::Bristles) || rel(sb, sa) == Some(Relation::Bristles)
        {
            let back = |x: f64, d: f64| (x + d * 0.2 * w).clamp(0.06 * w, 0.94 * w);
            l_acts = vec![
                Act::WalkTo {
                    x: mid - gap - 4.0,
                    pace: 1.0,
                },
                Act::Face(1.0),
                Act::Emote(Icon::Anger),
                Act::Pose {
                    pose: Pose::Idle,
                    t: 1.5,
                },
                Act::WalkTo {
                    x: back(mid, -1.0),
                    pace: 1.3,
                },
                done.clone(),
            ];
            r_acts = vec![
                Act::WalkTo {
                    x: mid + gap + 4.0,
                    pace: 1.0,
                },
                Act::Face(-1.0),
                Act::Wait(0.6),
                Act::Emote(Icon::Anger),
                Act::Pose {
                    pose: Pose::Idle,
                    t: 0.9,
                },
                Act::WalkTo {
                    x: back(mid, 1.0),
                    pace: 1.3,
                },
                done.clone(),
            ];
        } else {
            let greets =
                rel(sa, sb) == Some(Relation::Greets) || rel(sb, sa) == Some(Relation::Greets);
            if !greets && !self.rng.chance(if social { 0.9 } else { 0.7 }) {
                return;
            }
            let e1 = if greets {
                Icon::Heart
            } else {
                [Icon::Heart, Icon::Note, Icon::Spark, Icon::Bang][self.rng.index(4)]
            };
            let e2 = if greets {
                Icon::Heart
            } else {
                [Icon::Heart, Icon::Note, Icon::What][self.rng.index(3)]
            };
            let sit1 = if self.rng.chance(0.5) {
                Pose::Sit
            } else {
                Pose::Idle
            };
            let sit2 = if self.rng.chance(0.5) {
                Pose::Sit
            } else {
                Pose::Idle
            };
            let t1 = self.rng.range(3.0, 7.0);
            let t2 = self.rng.range(3.0, 7.0);
            let rs = &self.critters[r].sp().lines;
            let r_says = if rs.is_empty() {
                Act::Emote(Icon::Note)
            } else {
                Act::Say(rs[self.rng.index(rs.len())].to_string())
            };
            l_acts = vec![
                Act::WalkTo {
                    x: mid - gap,
                    pace: 1.0,
                },
                Act::Face(1.0),
                Act::Wait(0.4),
                Act::Emote(e1),
                Act::Pose {
                    pose: Pose::Idle,
                    t: 2.2,
                },
                Act::Emote(Icon::Heart),
                Act::Pose { pose: sit1, t: t1 },
                done.clone(),
            ];
            r_acts = vec![
                Act::WalkTo {
                    x: mid + gap,
                    pace: 1.0,
                },
                Act::Face(-1.0),
                Act::Wait(1.2),
                Act::Emote(e2),
                Act::Pose {
                    pose: Pose::Idle,
                    t: 1.6,
                },
                r_says,
                Act::Pose { pose: sit2, t: t2 },
                done,
            ];
        }
        for (i, acts) in [(l, l_acts), (r, r_acts)] {
            let c = &mut self.critters[i];
            c.meeting = true;
            c.cur = None;
            c.acts = acts.into();
        }
    }

    fn app_event(&mut self, ev: AppEvent, input: &Input) {
        match ev {
            AppEvent::NeedsYou { x } => {
                let at = x.unwrap_or(self.view_w / 2.0);
                let near = self
                    .critters
                    .iter()
                    .filter(|c| c.entered && !c.gone && !c.sp().has(flag::INANIMATE))
                    .min_by(|a, b| (a.x - at).abs().total_cmp(&(b.x - at).abs()))
                    .map(|c| c.id);
                if let Some(id) = near {
                    let c = self.get_mut(id).expect("found above");
                    if c.grounded() && !c.meeting {
                        c.cur = None;
                        c.acts.push_front(Act::Pose {
                            pose: Pose::Idle,
                            t: 1.2,
                        });
                        c.acts.push_front(Act::Emote(Icon::Bang));
                        c.acts.push_front(Act::FaceX(at));
                    } else {
                        let (hx, hr) = c.head();
                        emote(
                            &mut self.particles,
                            hx,
                            hr,
                            Icon::Bang,
                            input.reduced_motion,
                        );
                    }
                }
            }
            AppEvent::TurnDone => {
                let adv = by_id("adventurer");
                if let Some(c) = self
                    .critters
                    .iter_mut()
                    .find(|c| Some(c.species) == adv && c.entered)
                {
                    c.bubble = Some(Bubble {
                        text: "gz".into(),
                        t: 2.6,
                    });
                } else if self.rng.chance(0.5) {
                    if let Some(c) = self
                        .critters
                        .iter()
                        .find(|c| c.entered && !c.sp().has(flag::INANIMATE))
                    {
                        let (hx, hr) = c.head();
                        emote(
                            &mut self.particles,
                            hx,
                            hr,
                            Icon::Note,
                            input.reduced_motion,
                        );
                    }
                }
            }
            AppEvent::PrMerged { x } => {
                let x = x.unwrap_or_else(|| self.rng.range(0.2, 0.8) * self.view_w);
                if !input.reduced_motion {
                    let mut beam = Particle::new(Kind::Beam, x, 0.0, 6.0, ink::BEAM);
                    beam.text = "loot".into();
                    self.particles.push(beam);
                }
                let adv = by_id("adventurer");
                let w = self.view_w;
                for c in self
                    .critters
                    .iter_mut()
                    .filter(|c| c.entered && c.grounded())
                {
                    let (hx, hr) = c.head();
                    emote(
                        &mut self.particles,
                        hx,
                        hr,
                        Icon::Bang,
                        input.reduced_motion,
                    );
                    if Some(c.species) == adv && !c.meeting {
                        c.cur = None;
                        c.acts = vec![
                            Act::WalkTo {
                                x: (x - 6.0).clamp(0.06 * w, 0.94 * w),
                                pace: 1.6,
                            },
                            Act::FaceX(x),
                            Act::Say("loot!".into()),
                            Act::Pose {
                                pose: Pose::Eat,
                                t: 1.2,
                            },
                            Act::Emote(Icon::Spark),
                        ]
                        .into();
                    }
                }
            }
        }
    }

    // ---------- the host's commands (the critter menu, the mouse) ----------

    /// Brings a species in now (the kete mark's kiwi, tests, a debug hook).
    pub fn summon(&mut self, species_id: &str, input: &Input) -> Option<u32> {
        let sp = by_id(species_id)?;
        self.view_w = input.view_w.max(1.0);
        self.view_h = input.view_h.max(1.0);
        self.floor = input.floor;
        Some(self.visit(sp, None, input))
    }

    /// Click: a heart and the species' reaction.
    pub fn pet(&mut self, id: u32) -> bool {
        let Some(c) = self.get(id).cloned() else {
            return false;
        };
        if c.gone || c.mode != Mode::Brain {
            return false;
        }
        let s = c.sp();
        let (hx, hr) = c.head();
        self.notes.push(Note::Petted { species: s.id });
        emote(&mut self.particles, hx, hr, Icon::Heart, false);
        let mut plan: Option<Vec<Act>> = None;
        match s.special {
            Special::Fuse => {
                plan = Some(vec![
                    Act::Say("sss...".into()),
                    Act::Flash { t: 1.4 },
                    Act::Burst,
                ]);
            }
            Special::Lamp if c.offering => {
                self.notes.push(Note::Lamp);
                let mut p = Particle::new(Kind::Text, hx, hr + 6.0, 2.4, ink::GOLD);
                p.text = "+xp lamp".into();
                p.vrise = 14.0;
                self.particles.push(p);
                plan = Some(vec![
                    Act::Say("enjoy".into()),
                    Act::Pose {
                        pose: Pose::Idle,
                        t: 1.0,
                    },
                    Act::Fade {
                        to: 0.0,
                        t: 1.2,
                        from: None,
                        e: 0.0,
                    },
                    Act::Mark(Mark::Gone),
                ]);
            }
            Special::Teleport => {
                let w = self.view_w;
                let to = self.rng.range(0.15, 0.85) * w;
                plan = Some(vec![
                    Act::Puff,
                    Act::Jump(to),
                    Act::Puff,
                    Act::Say("hehe".into()),
                ]);
            }
            Special::Dig => {
                let to = self.rng.range(0.15, 0.85) * self.view_w;
                let h = self_height(c.species);
                plan = Some(vec![
                    Act::Sink { to: -h },
                    Act::Jump(to),
                    Act::Sink { to: 0.0 },
                ]);
            }
            Special::Hide => {
                plan = Some(vec![
                    Act::Pose {
                        pose: Pose::Sit,
                        t: 3.0,
                    },
                    Act::Pose {
                        pose: Pose::Idle,
                        t: 0.5,
                    },
                ]);
            }
            _ => {}
        }
        if s.id == "slime" {
            self.apply(Effect::Split(id));
            return true;
        }
        if s.id == "balloon" {
            plan = Some(vec![Act::PopSelf]);
        }
        let plan = plan.unwrap_or_else(|| {
            let mut p = vec![Act::Pose {
                pose: Pose::Idle,
                t: 0.4,
            }];
            if !s.lines.is_empty() && self.rng.chance(0.6) {
                p.push(Act::Say(s.lines[self.rng.index(s.lines.len())].to_string()));
            } else if s.gait == Gait::Hop {
                p.push(Act::Hops {
                    n: 1.0,
                    e: 0.0,
                    h: 4.0,
                });
            }
            p.push(Act::Pose {
                pose: Pose::Idle,
                t: 0.8,
            });
            p
        });
        let c = self.get_mut(id).expect("checked above");
        if c.exiting || c.meeting {
            return true;
        }
        c.cur = None;
        c.acts = plan.into();
        true
    }

    /// Double-click or Feed: crumbs fall in front of it and it eats.
    pub fn feed(&mut self, id: u32) -> bool {
        let Some(c) = self.get(id).cloned() else {
            return false;
        };
        if c.gone || c.mode != Mode::Brain || c.exiting {
            return false;
        }
        let s = c.sp();
        self.notes.push(Note::Fed { species: s.id });
        let x = c.x + c.dir * (c.half_w() + 1.0);
        let top = c.height() + 6.0;
        for i in 0..6 {
            let mut p = Particle::new(
                Kind::Crumb,
                x + f64::from(i % 3) - 1.0,
                top + f64::from(i),
                2.5,
                ink::CRUMB,
            );
            p.gravity = 60.0;
            p.vx = self.rng.range(-4.0, 4.0);
            self.particles.push(p);
        }
        let greedy = c.personality == Personality::Greedy;
        let mut plan = vec![
            Act::Wait(0.5),
            Act::Eat {
                t: if greedy { 5.0 } else { 3.0 },
            },
            Act::Emote(Icon::Heart),
        ];
        if greedy {
            plan.push(Act::Say("nom".into()));
        }
        plan.push(Act::Pose {
            pose: Pose::Idle,
            t: 0.8,
        });
        let c = self.get_mut(id).expect("checked above");
        c.cur = None;
        c.meeting = false;
        c.acts = plan.into();
        c.perch = None;
        true
    }

    /// Shoo: it leaves now.
    pub fn shoo(&mut self, id: u32) -> bool {
        let w = self.view_w;
        let Some(c) = self.get(id).cloned() else {
            return false;
        };
        if c.gone || c.mode != Mode::Brain {
            return false;
        }
        let mut acts = vec![Act::Emote(Icon::What)];
        acts.extend(exit_plan(&c, w, &mut self.rng, 1.8, self.reduced));
        let c = self.get_mut(id).expect("checked above");
        c.cur = None;
        c.meeting = false;
        c.entered = true;
        c.acts = acts.into();
        c.exiting = true;
        true
    }

    /// Names a visitor; the species keeps the name for next time.
    pub fn rename(&mut self, id: u32, name: &str) -> bool {
        let name: String = name.trim().chars().take(16).collect();
        let Some(c) = self.get_mut(id) else {
            return false;
        };
        if name.is_empty() {
            return false;
        }
        c.name = name.clone();
        let species = c.sp().id;
        self.names.insert(species.to_string(), name.clone());
        self.notes.push(Note::Named { species, name });
        true
    }

    /// Everyone leaves (Hide critters until tomorrow); no new visitors
    /// until `spawning` is set again.
    pub fn scatter(&mut self) {
        self.spawning = false;
        let ids: Vec<u32> = self.critters.iter().map(|c| c.id).collect();
        for id in ids {
            if !self.shoo(id) {
                if let Some(c) = self.get_mut(id) {
                    c.gone = true;
                }
            }
        }
    }

    /// Takes everything off screen at once.
    pub fn clear(&mut self) {
        self.critters.clear();
        self.particles.clear();
        self.held = None;
    }

    /// Restarts the visit timer (spawning back on, the next visitor in a
    /// few seconds).
    pub fn resume(&mut self) {
        self.spawning = true;
        self.spawn_in = steps(self.rng.range(3.0, 8.0));
        self.acc_us = 0;
    }

    /// Picks a critter up at the pointer.
    pub fn grab(&mut self, id: u32, px: f64, py: f64) -> bool {
        let floor = self.floor;
        let Some(c) = self.get_mut(id) else {
            return false;
        };
        if c.gone || c.offering {
            return false;
        }
        let feet_y = floor - c.lift;
        c.hold = Some(Hold {
            dx: c.x - px,
            dy: feet_y - py,
            vx: 0.0,
            vy: 0.0,
            last: Some((px, py)),
        });
        c.mode = Mode::Held;
        c.perch = None;
        c.cur = None;
        c.meeting = false;
        c.acts.clear();
        c.pose = Pose::Held;
        c.bubble = None;
        self.held = Some(id);
        true
    }

    /// The critter being held, if any.
    pub fn held(&self) -> Option<u32> {
        self.held
    }

    /// Lets go: it falls with the pointer's velocity (fliers fly off).
    pub fn release(&mut self) {
        let Some(id) = self.held.take() else { return };
        let w = self.view_w;
        let floor = self.floor;
        let mut rng = self.rng.clone();
        let perch = None::<f64>;
        let Some(c) = self.get_mut(id) else { return };
        let hold = c.hold.take().unwrap_or(Hold {
            dx: 0.0,
            dy: 0.0,
            vx: 0.0,
            vy: 0.0,
            last: None,
        });
        let vx = hold.vx.clamp(-MAX_FLING, MAX_FLING);
        let vy = hold.vy.clamp(-MAX_FLING, MAX_FLING);
        if c.sp().has(flag::FLIER) || matches!(c.sp().gait, Gait::Float | Gait::Fly) {
            c.mode = Mode::Brain;
            let tx =
                (c.x + vx * 0.4 + rng.sign() * rng.range(0.05, 0.2) * w).clamp(0.08 * w, 0.92 * w);
            let hover = c.sp().hover;
            c.acts = vec![
                Act::FlyTo {
                    x: tx,
                    lift: hover,
                    perch,
                    from: None,
                    e: 0.0,
                    total: 0.0,
                },
                Act::Emote(Icon::Note),
            ]
            .into();
        } else {
            c.mode = Mode::Air;
            c.vx = vx;
            c.vlift = -vy;
        }
        let _ = floor;
        self.rng = rng;
    }

    // ---------- queries ----------

    /// The draw list, back to front.
    pub fn sprites(&self) -> Vec<SpriteDraw> {
        let floor = self.floor;
        let mut out: Vec<SpriteDraw> = self
            .critters
            .iter()
            .map(|c| {
                let s = sprite(c.species);
                let ghost = if c.sp().has(flag::GHOSTLY) { 0.8 } else { 1.0 };
                let squash = c.squash_t > 0.0;
                SpriteDraw {
                    id: c.id,
                    species: c.species,
                    pose: if squash { Pose::Squash } else { c.pose },
                    frame: c.frame,
                    closed: c.blink || c.pose == Pose::Sleep,
                    flip: c.dir < 0.0,
                    left: (c.x - s.w as f64 / 2.0).round() as i32,
                    top: (floor - c.lift).round() as i32 - s.h as i32,
                    alpha: (c.alpha * ghost).clamp(0.0, 1.0),
                    flash: c.flash,
                    clip: floor.round() as i32,
                }
            })
            .collect();
        // Held critters draw on top.
        out.sort_by_key(|d| Some(d.id) == self.held);
        out
    }

    /// The critter whose drawn pixels cover (x, y), topmost first.
    pub fn hit(&self, x: f64, y: f64) -> Option<u32> {
        let (px, py) = (x.floor() as i32, y.floor() as i32);
        self.sprites().iter().rev().find_map(|d| {
            let s = sprite(d.species);
            let (lx, ly) = (px - d.left, py - d.top);
            if lx < 0
                || ly < 0
                || lx >= s.w as i32
                || ly >= s.h as i32
                || py >= d.clip
                || d.alpha < 0.3
            {
                return None;
            }
            let lx = if d.flip { s.w as i32 - 1 - lx } else { lx } as usize;
            let f = s.frame(d.pose, d.frame);
            (f[ly as usize * s.w + lx] != 0).then_some(d.id)
        })
    }

    /// A critter's drawn box (left, top, w, h): the desktop's hit box.
    pub fn bounds(&self, id: u32) -> Option<(i32, i32, usize, usize)> {
        self.sprites().into_iter().find(|d| d.id == id).map(|d| {
            let s = sprite(d.species);
            (d.left, d.top, s.w, s.h)
        })
    }
}

fn self_height(species: usize) -> f64 {
    sprite(species).h as f64
}

/// One sprite to draw: the species' frame for `pose`/`frame`, mirrored when
/// `flip`, its top-left at (`left`, `top`), rows at or below `clip` (the
/// floor) hidden. `closed` applies the species' blink swaps; `flash` draws
/// every opaque pixel in `ink::FLASH`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SpriteDraw {
    pub id: u32,
    pub species: usize,
    pub pose: Pose,
    pub frame: usize,
    pub closed: bool,
    pub flip: bool,
    pub left: i32,
    pub top: i32,
    pub alpha: f64,
    pub flash: bool,
    pub clip: i32,
}

// ---------- the brain ----------

fn emote(particles: &mut Vec<Particle>, x: f64, rise: f64, icon: Icon, reduced: bool) {
    if reduced {
        return;
    }
    let mut p = Particle::new(Kind::Icon(icon), x, rise + 4.0, 1.8, icon.color());
    p.vrise = 10.0;
    particles.push(p);
}

fn dust(particles: &mut Vec<Particle>, rng: &mut Rng, x: f64, rise: f64, n: usize, reduced: bool) {
    if reduced {
        return;
    }
    for _ in 0..n {
        let mut p = Particle::new(Kind::Dust, x, rise + 1.0, 0.5, ink::DUST);
        p.vx = rng.range(-20.0, 20.0);
        p.vrise = rng.range(5.0, 20.0);
        p.gravity = 40.0;
        particles.push(p);
    }
}

/// The weighted pick of what to do next, from the state just left.
pub fn next_weights(s: &Species, prev: State, mood: Personality, late: bool) -> [f64; 10] {
    let row = State::ALL.iter().position(|x| *x == prev).unwrap_or(1);
    let mood = Personality::ALL
        .iter()
        .position(|x| *x == mood)
        .unwrap_or(0);
    let mut w = NEXT[row];
    for (i, m) in MOODS[mood].iter().enumerate() {
        w[i] *= m;
    }
    let col = |st: State| State::TO.iter().position(|x| *x == st).expect("a TO state");
    if s.lines.is_empty() {
        w[col(State::Say)] = 0.0;
    }
    if s.gait == Gait::Slide {
        w[col(State::Hop)] = 0.0;
        w[col(State::Walk)] += 2.0;
    }
    if s.special == Special::Nothing {
        // The pet rock: sits about. Beloved.
        w = [0.0, 3.0, 0.0, 0.0, 2.0, 1.0, 0.0, 0.0, 0.0, 0.0];
    }
    if s.has(flag::INANIMATE) {
        // The balloon drifts and hangs about, and that's all.
        return [3.0, 2.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0];
    }
    if late {
        w[col(State::Sit)] *= LATE_SLEEP;
        w[col(State::Sleep)] *= LATE_SLEEP;
        w[col(State::Walk)] /= 3.0;
        // Sitting leads to sleep: give idle a way into it too.
        if w[col(State::Sit)] == 0.0 {
            w[col(State::Sit)] = 1.0;
        }
    }
    w
}

/// The plan for the state picked next.
fn next_acts(c: &mut Critter, ctx: &Ctx, late: bool, rng: &mut Rng) -> Vec<Act> {
    let s = c.sp();
    let w = ctx.view_w;
    if ctx.reduced {
        // Reduced motion: hold poses in place.
        let pose = [Pose::Idle, Pose::Sit, Pose::Sleep][rng.index(3)];
        return vec![Act::Pose {
            pose,
            t: rng.range(4.0, 10.0),
        }];
    }
    let weights = next_weights(s, c.prev, c.personality, late);
    let k = State::TO[rng.weighted(&weights)];
    c.prev = k;
    let lo = 0.06 * w + c.half_w();
    let hi = (0.94 * w - c.half_w()).max(lo);
    let walk_target = |c: &Critter, rng: &mut Rng| -> f64 {
        if let Some(p) = ctx.pointer {
            let near = (p.x - c.x).abs() < 40.0;
            if c.personality == Personality::Curious && rng.chance(0.6) {
                return (p.x - c.dir * 4.0).clamp(lo, hi);
            }
            if c.personality == Personality::Shy && near {
                let away = if p.x < c.x { 1.0 } else { -1.0 };
                return (c.x + away * rng.range(0.15, 0.3) * w).clamp(lo, hi);
            }
        }
        if c.personality == Personality::Social && rng.chance(0.5) {
            if let Some((_, os, ox, _)) = ctx.others.iter().find(|o| o.0 != c.id) {
                // Alongside, not on top of it.
                let gap = c.half_w() + sprite(*os).w as f64 / 2.0 + 2.0;
                let side = if c.x < *ox { -1.0 } else { 1.0 };
                return (*ox + side * gap).clamp(lo, hi);
            }
        }
        let max = if s.gait == Gait::Slide { 0.12 } else { 0.35 } * w;
        (c.x + rng.sign() * rng.range(0.05 * w, max.max(0.06 * w))).clamp(lo, hi)
    };
    match k {
        State::Walk => {
            let pace = if c.personality == Personality::Shy
                && ctx.pointer.is_some_and(|p| (p.x - c.x).abs() < 40.0)
            {
                1.8
            } else {
                1.0
            };
            vec![Act::WalkTo {
                x: walk_target(c, rng),
                pace,
            }]
        }
        State::Idle => vec![Act::Pose {
            pose: Pose::Idle,
            t: rng.range(2.0, 6.0),
        }],
        State::Look => vec![
            Act::Pose {
                pose: Pose::Idle,
                t: rng.range(0.8, 1.5),
            },
            Act::Turn,
            Act::Pose {
                pose: Pose::Idle,
                t: rng.range(0.8, 1.6),
            },
            Act::Turn,
            Act::Pose {
                pose: Pose::Idle,
                t: 0.6,
            },
        ],
        State::Eat => vec![Act::Eat {
            t: rng.range(2.5, 6.0),
        }],
        State::Sit => vec![Act::Pose {
            pose: Pose::Sit,
            t: rng.range(3.0, 8.0),
        }],
        State::Sleep => {
            let sleepy = if c.personality == Personality::Sleepy {
                1.5
            } else {
                1.0
            };
            let long = if late { 3.0 } else { 1.0 };
            vec![
                Act::Pose {
                    pose: Pose::Sit,
                    t: 0.4,
                },
                Act::Sleep {
                    t: rng.range(6.0, 16.0) * sleepy * long,
                    zt: 0.0,
                },
                Act::Pose {
                    pose: Pose::Idle,
                    t: 0.8,
                },
            ]
        }
        State::Emote => {
            let icons = [Icon::Heart, Icon::Note, Icon::Note, Icon::What, Icon::Spark];
            vec![
                Act::Emote(icons[rng.index(icons.len())]),
                Act::Pose {
                    pose: Pose::Idle,
                    t: 1.0,
                },
            ]
        }
        State::Say => vec![
            Act::Say(s.lines[rng.index(s.lines.len().max(1))].to_string()),
            Act::Pose {
                pose: Pose::Idle,
                t: 1.0,
            },
        ],
        State::Hop => vec![Act::Hops {
            n: (rng.range(1.0, 3.0)).ceil(),
            e: 0.0,
            h: hop_height(c),
        }],
        State::Special => special(c, ctx, rng),
        State::Start => vec![Act::Pose {
            pose: Pose::Idle,
            t: 2.0,
        }],
    }
}

fn hop_height(c: &Critter) -> f64 {
    ((c.height() - 2.0) / 3.0).round().clamp(2.0, 5.0)
}

/// A flight somewhere: a perch when there is one (60%), else the floor.
fn flight(c: &Critter, ctx: &Ctx, rng: &mut Rng) -> Act {
    let w = ctx.view_w;
    let hw = c.half_w();
    let perches: Vec<&Rect> = ctx
        .perches
        .iter()
        .filter(|r| r.w >= hw * 2.0 && r.y > c.height() + 2.0 && r.y < ctx.floor - 4.0)
        .collect();
    if !perches.is_empty() && rng.chance(0.6) {
        let r = perches[rng.index(perches.len())];
        let x = rng.range(r.x + hw, r.x + r.w - hw);
        return Act::FlyTo {
            x,
            lift: ctx.floor - r.y,
            perch: Some(r.y),
            from: None,
            e: 0.0,
            total: 0.0,
        };
    }
    let x = (c.x + rng.sign() * rng.range(0.1, 0.25) * w).clamp(0.1 * w, 0.9 * w);
    Act::FlyTo {
        x,
        lift: c.sp().hover,
        perch: None,
        from: None,
        e: 0.0,
        total: 0.0,
    }
}

fn special(c: &Critter, ctx: &Ctx, rng: &mut Rng) -> Vec<Act> {
    let s = c.sp();
    let w = ctx.view_w;
    let idle = |t: f64| Act::Pose {
        pose: Pose::Idle,
        t,
    };
    let clampx = |x: f64| {
        x.clamp(
            0.06 * w + c.half_w(),
            (0.94 * w - c.half_w()).max(0.06 * w + c.half_w()),
        )
    };
    match s.special {
        Special::Emote => {
            let icons = [Icon::Note, Icon::Spark, Icon::Heart];
            vec![Act::Emote(icons[rng.index(3)]), idle(1.0)]
        }
        Special::Graze => vec![
            Act::Eat {
                t: rng.range(5.0, 9.0),
            },
            Act::Emote(Icon::Note),
            idle(1.0),
        ],
        Special::Flight => {
            if c.perch.is_some() {
                // Back down to the floor.
                let x = clampx(c.x + rng.sign() * rng.range(0.05, 0.2) * w);
                vec![Act::FlyTo {
                    x,
                    lift: s.hover,
                    perch: None,
                    from: None,
                    e: 0.0,
                    total: 0.0,
                }]
            } else {
                vec![
                    flight(c, ctx, rng),
                    Act::Pose {
                        pose: Pose::Sit,
                        t: rng.range(5.0, 12.0),
                    },
                ]
            }
        }
        Special::Pinch => match ctx.pointer {
            Some(p) => vec![
                Act::WalkTo {
                    x: clampx(p.x),
                    pace: 1.5,
                },
                Act::Emote(Icon::Bang),
                Act::Pinch,
                Act::Say("mine!".into()),
                idle(0.5),
                // Off with it.
                Act::Mark(Mark::Exiting),
                Act::WalkTo {
                    x: if p.x < w / 2.0 {
                        w + c.half_w() * 2.0
                    } else {
                        -c.half_w() * 2.0
                    },
                    pace: 2.2,
                },
                Act::Mark(Mark::Gone),
            ],
            None => vec![
                Act::Eat {
                    t: rng.range(3.0, 5.0),
                },
                idle(1.0),
            ],
        },
        Special::Boom => vec![
            Act::Pose {
                pose: Pose::Sit,
                t: 0.6,
            },
            Act::Pop {
                text: "boom".into(),
                color: [0x9f, 0xc8, 0x7a],
            },
            Act::Pose {
                pose: Pose::Sit,
                t: 1.4,
            },
            Act::Pop {
                text: "boom".into(),
                color: [0x9f, 0xc8, 0x7a],
            },
            Act::Pose {
                pose: Pose::Sit,
                t: 1.4,
            },
            Act::Pop {
                text: "boom".into(),
                color: [0x9f, 0xc8, 0x7a],
            },
            idle(1.0),
        ],
        Special::Flit => {
            let mut v = Vec::new();
            for _ in 0..3 {
                let x = clampx(c.x + rng.sign() * rng.range(6.0, 20.0));
                v.push(Act::FlyTo {
                    x,
                    lift: 0.0,
                    perch: None,
                    from: None,
                    e: 0.0,
                    total: 0.0,
                });
                v.push(Act::Turn);
                v.push(idle(0.3));
            }
            v.push(Act::Emote(Icon::Note));
            v
        }
        Special::Slide => vec![
            Act::Pose {
                pose: Pose::Sit,
                t: 0.3,
            },
            Act::WalkTo {
                x: clampx(c.x + c.dir * 0.15 * w),
                pace: 3.0,
            },
            idle(1.0),
        ],
        Special::Pushups => vec![Act::Pushups { n: 3.0, e: 0.0 }, idle(1.0)],
        Special::Hops => vec![Act::Hops {
            n: 3.0,
            e: 0.0,
            h: hop_height(c),
        }],
        Special::Loaf => vec![
            Act::Pose {
                pose: Pose::Sit,
                t: rng.range(8.0, 16.0),
            },
            Act::Emote(Icon::Note),
        ],
        Special::Drop => vec![
            Act::Eat {
                t: rng.range(2.0, 4.0),
            },
            Act::DropItem,
            idle(1.2),
        ],
        Special::Spin => vec![
            Act::Turn,
            Act::Wait(0.15),
            Act::Turn,
            Act::Wait(0.15),
            Act::Turn,
            Act::Wait(0.15),
            Act::Turn,
            Act::Emote(Icon::Heart),
            idle(0.8),
        ],
        Special::Hide => vec![
            Act::Pose {
                pose: Pose::Sit,
                t: rng.range(3.0, 6.0),
            },
            idle(1.0),
        ],
        Special::Pounce => vec![
            Act::Pose {
                pose: Pose::Sit,
                t: 0.8,
            },
            Act::Hops {
                n: 1.0,
                e: 0.0,
                h: 7.0,
            },
            Act::Say("heh".into()),
            idle(1.0),
        ],
        Special::Snip => vec![Act::Eat { t: 1.5 }, Act::Emote(Icon::Spark), idle(0.8)],
        Special::Spores => vec![Act::Spores, idle(1.5)],
        Special::Teleport => {
            let to = clampx(rng.range(0.15, 0.85) * w);
            vec![Act::Puff, Act::Jump(to), Act::Puff, idle(1.0)]
        }
        Special::Mine => vec![
            Act::Eat { t: 3.0 },
            Act::Emote(Icon::Spark),
            Act::Pop {
                text: "+25 xp".into(),
                color: ink::GOLD,
            },
            idle(1.5),
        ],
        Special::Dig => {
            let to = clampx(rng.range(0.15, 0.85) * w);
            let h = c.height();
            vec![
                Act::Sink { to: -h },
                Act::Jump(to),
                Act::Sink { to: 0.0 },
                idle(1.0),
            ]
        }
        Special::Nothing => vec![idle(rng.range(4.0, 8.0))],
        Special::Lamp => vec![Act::Say("lamp for you".into()), Act::Offer { t: 20.0 }],
        Special::Fuse => vec![Act::Say("sss...".into()), Act::Flash { t: 1.6 }, Act::Burst],
        Special::Buzz => {
            let x = clampx(c.x + rng.sign() * rng.range(10.0, 30.0));
            vec![
                Act::WalkTo { x, pace: 1.6 },
                Act::Emote(Icon::Note),
                Act::Turn,
                idle(0.5),
            ]
        }
        Special::Fetch => vec![Act::Carry {
            item: None,
            to: ctx.pointer.map_or(w / 2.0, |p| p.x),
        }],
        Special::Bounce => vec![
            Act::Hops {
                n: 2.0,
                e: 0.0,
                h: 8.0,
            },
            idle(0.6),
        ],
        Special::Smoke => vec![
            Act::Breathe,
            idle(0.4),
            Act::Breathe,
            Act::Say("*puff*".into()),
            idle(1.0),
        ],
    }
}

/// How a critter leaves, by kind: fade, fly off, dig, puff, float up, or
/// walk off a side (sometimes down the stairs at the edge).
fn exit_plan(c: &Critter, w: f64, rng: &mut Rng, pace: f64, reduced: bool) -> Vec<Act> {
    let s = c.sp();
    if reduced {
        return vec![
            Act::Fade {
                to: 0.0,
                t: 1.0,
                from: None,
                e: 0.0,
            },
            Act::Mark(Mark::Gone),
        ];
    }
    let hw = c.half_w();
    let side = if c.x < w / 2.0 { -1.0 } else { 1.0 };
    let off = if side < 0.0 { -hw * 2.0 } else { w + hw * 2.0 };
    if s.has(flag::GHOSTLY) {
        return vec![
            Act::Fade {
                to: 0.0,
                t: 1.5,
                from: None,
                e: 0.0,
            },
            Act::Mark(Mark::Gone),
        ];
    }
    if s.special == Special::Dig {
        return vec![
            Act::Sink {
                to: -c.height() - 1.0,
            },
            Act::Mark(Mark::Gone),
        ];
    }
    if s.special == Special::Teleport {
        return vec![
            Act::Puff,
            Act::Fade {
                to: 0.0,
                t: 0.2,
                from: None,
                e: 0.0,
            },
            Act::Mark(Mark::Gone),
        ];
    }
    if s.id == "balloon" {
        return vec![
            Act::FlyTo {
                x: c.x + side * 10.0,
                lift: 400.0,
                perch: None,
                from: None,
                e: 0.0,
                total: 0.0,
            },
            Act::Mark(Mark::Gone),
        ];
    }
    if (s.has(flag::FLIER) && rng.chance(0.5)) || s.gait == Gait::Fly {
        return vec![
            Act::FlyTo {
                x: off,
                lift: rng.range(20.0, 60.0),
                perch: None,
                from: None,
                e: 0.0,
                total: 0.0,
            },
            Act::Mark(Mark::Gone),
        ];
    }
    if c.perch.is_none() && rng.chance(0.35) && s.gait != Gait::Float {
        let edge = if side < 0.0 { hw + 2.0 } else { w - hw - 2.0 };
        let h = c.height();
        return vec![
            Act::WalkTo { x: edge, pace },
            Act::Face(side),
            Act::Stairs {
                from: 0.0,
                to: -h - 1.0,
                fade_in: false,
                dist: 0.0,
            },
            Act::Mark(Mark::Gone),
        ];
    }
    vec![Act::WalkTo { x: off, pace }, Act::Mark(Mark::Gone)]
}

/// Moves toward `tx` at `speed`; true on arrival.
fn move_toward(
    c: &mut Critter,
    tx: f64,
    dt: f64,
    speed: f64,
    particles: &mut Vec<Particle>,
    reduced: bool,
) -> bool {
    let d = tx - c.x;
    if d.abs() < 0.5 {
        return true;
    }
    if c.sp().gait != Gait::Sideways {
        c.dir = d.signum();
    }
    let step = (speed * dt).min(d.abs()) * d.signum();
    c.x += step;
    if c.sp().has(flag::TRAIL) && !reduced {
        c.trail_d += step.abs();
        if c.trail_d > 1.5 {
            c.trail_d = 0.0;
            let x = c.x - c.dir * c.half_w() * 0.5;
            particles.push(Particle::new(Kind::Trail, x, 0.0, 25.0, ink::TRAIL));
        }
    }
    false
}

/// One step of one critter.
fn update(
    c: &mut Critter,
    ctx: &Ctx,
    late: bool,
    rng: &mut Rng,
    particles: &mut Vec<Particle>,
    fx: &mut Vec<Effect>,
) {
    let dt = ctx.dt;
    c.age += dt;
    c.anim += dt;
    c.flash = false;
    c.blink_t -= dt;
    if c.blink_t < 0.0 {
        c.blink = !c.blink;
        c.blink_t = if c.blink {
            BLINK_TIME
        } else {
            rng.range(2.5, 6.0)
        };
    }
    if let Some(b) = c.bubble.as_mut() {
        b.t -= dt;
        if b.t <= 0.0 {
            c.bubble = None;
        }
    }
    if c.squash_t > 0.0 {
        c.squash_t -= dt;
    }
    match c.mode {
        Mode::Held => {
            held(c, ctx);
            return;
        }
        Mode::Air => {
            air(c, ctx, rng, particles);
            return;
        }
        Mode::Brain => {}
    }
    // A perch that went away (the layout changed): fall.
    if let Some(py) = c.perch {
        let still = ctx
            .perches
            .iter()
            .any(|r| (r.y - py).abs() < 0.5 && c.x >= r.x && c.x <= r.x + r.w);
        if !still {
            c.perch = None;
            c.mode = Mode::Air;
            c.vlift = 0.0;
            c.vx = 0.0;
            c.cur = None;
            c.acts.clear();
            return;
        } else {
            c.lift = ctx.floor - py;
        }
    }
    if c.cur.is_none() {
        if c.acts.is_empty() {
            if c.entered && !c.exiting && c.age > c.budget && !c.meeting {
                c.exiting = true;
                if c.sp().id == "balloon" && !ctx.reduced && rng.chance(0.6) {
                    // Hang about for the monkey (its dart re-plans this).
                    fx.push(Effect::Monkey(c.id));
                    c.acts = vec![Act::Wait(12.0)].into();
                } else {
                    c.acts = exit_plan(c, ctx.view_w, rng, 1.0, ctx.reduced).into();
                }
            } else if !c.exiting {
                let plan = next_acts(c, ctx, late, rng);
                c.acts = plan.into();
            } else {
                // Exiting with nothing left to do: re-plan the way out.
                c.acts = exit_plan(c, ctx.view_w, rng, 1.0, ctx.reduced).into();
            }
        }
        c.cur = c.acts.pop_front();
    }
    if let Some(mut act) = c.cur.take() {
        let done = run(&mut act, c, ctx, rng, particles, fx);
        if !done {
            c.cur = Some(act);
        }
    }
    let s = c.sp();
    if s.has(flag::SPARKLE) && !ctx.reduced && rng.chance(dt * 1.2) {
        let x = c.x + rng.range(-1.0, 1.0) * c.half_w() * 0.8;
        let mut p = Particle::new(
            Kind::Sparkle,
            x,
            c.lift + rng.range(0.3, 1.0) * c.height(),
            1.2,
            [0xff, 0xe3, 0x8a],
        );
        p.vrise = 8.0;
        particles.push(p);
    }
    // Hover gaits: float bobs, fly hangs at its height unless sitting.
    if c.perch.is_none()
        && !matches!(
            c.cur,
            Some(Act::FlyTo { .. }) | Some(Act::Sink { .. }) | Some(Act::Stairs { .. })
        )
    {
        match s.gait {
            Gait::Float => {
                let target = s.hover + (wave(c.age / 3.0) * 1.5).round();
                c.lift += (target - c.lift).clamp(-10.0 * dt, 10.0 * dt);
            }
            Gait::Fly => {
                let landed = matches!(c.pose, Pose::Sit | Pose::Sleep | Pose::Eat);
                let target = if landed {
                    0.0
                } else {
                    s.hover + wave(c.age / 2.0).round()
                };
                c.lift += (target - c.lift).clamp(-12.0 * dt, 12.0 * dt);
                if c.lift > 0.5 && matches!(c.pose, Pose::Idle | Pose::Walk) {
                    c.pose = Pose::Fly;
                    c.frame = (c.anim * 12.0) as usize % 2;
                }
            }
            _ => {}
        }
    }
}

/// Runs one action for a step; true when it's finished.
fn run(
    act: &mut Act,
    c: &mut Critter,
    ctx: &Ctx,
    rng: &mut Rng,
    particles: &mut Vec<Particle>,
    fx: &mut Vec<Effect>,
) -> bool {
    let dt = ctx.dt;
    let s = c.sp();
    let reduced = ctx.reduced;
    match act {
        Act::Mark(m) => {
            match m {
                Mark::Entered => c.entered = true,
                Mark::Gone => c.gone = true,
                Mark::MeetingOver => c.meeting = false,
                Mark::Exiting => c.exiting = true,
            }
            true
        }
        Act::Pose { pose, t } => {
            c.pose = *pose;
            c.frame = 0;
            *t -= dt;
            *t <= 0.0
        }
        Act::Turn => {
            if s.gait != Gait::Sideways {
                c.dir = -c.dir;
            }
            true
        }
        Act::Face(d) => {
            if s.gait != Gait::Sideways {
                c.dir = *d;
            }
            true
        }
        Act::FaceX(x) => {
            if s.gait != Gait::Sideways && (*x - c.x).abs() > 0.5 {
                c.dir = (*x - c.x).signum();
            }
            true
        }
        Act::Wait(t) => {
            *t -= dt;
            *t <= 0.0
        }
        Act::Fade { to, t, from, e } => {
            let f = *from.get_or_insert(c.alpha);
            *e += dt;
            c.pose = Pose::Idle;
            c.alpha = f + (*to - f) * (*e / *t).min(1.0);
            *e >= *t
        }
        Act::WalkTo { x, pace } => walk(c, *x, *pace, dt, particles, reduced),
        Act::Stairs {
            from,
            to,
            fade_in,
            dist,
        } => {
            const STEP_W: f64 = 5.0;
            const STEP_H: f64 = 3.0;
            let n = ((*to - *from).abs() / STEP_H).ceil();
            if *dist == 0.0 {
                c.lift = *from;
                if *fade_in {
                    c.alpha = 0.0;
                }
            }
            c.pose = Pose::Walk;
            c.frame = (c.anim * 5.0) as usize % 2;
            let v = s.speed * 0.8 * dt;
            *dist += v;
            c.x += c.dir * v;
            let k = (*dist / STEP_W).floor().min(n);
            let prev = c.lift;
            c.lift = *from + (*to - *from).signum() * ((*to - *from).abs()).min(k * STEP_H);
            if c.lift != prev {
                dust(
                    particles,
                    rng,
                    c.x - c.dir * c.half_w() * 0.6,
                    c.lift.max(0.0),
                    3,
                    reduced,
                );
            }
            if *fade_in {
                c.alpha = (*dist / STEP_W).min(1.0);
            }
            k >= n
        }
        Act::FlyTo {
            x,
            lift,
            perch,
            from,
            e,
            total,
        } => {
            let (sx, sl) = *from.get_or_insert((c.x, c.lift));
            if *total == 0.0 {
                *total = ((*x - sx).abs() / (s.speed * 2.2)).max(1.2);
            }
            *e += dt;
            let u = (*e / *total).min(1.0);
            c.pose = Pose::Fly;
            c.frame = (c.anim * 8.0) as usize % 2;
            if (*x - sx).abs() > 0.5 {
                c.dir = (*x - sx).signum();
            }
            c.x = sx + (*x - sx) * u;
            let arc = FLIGHT_ARC.min(ctx.view_h * 0.3);
            c.lift = sl + (*lift - sl) * (1.0 - (1.0 - u) * (1.0 - u)) + arch(u) * arc;
            if u >= 1.0 {
                c.lift = *lift;
                c.perch = *perch;
                c.pose = Pose::Idle;
                return true;
            }
            false
        }
        Act::Eat { t } => {
            *t -= dt;
            c.pose = if !((c.anim * 2.5) as usize).is_multiple_of(3) {
                Pose::Eat
            } else {
                Pose::Idle
            };
            c.frame = 0;
            if *t <= 0.0 {
                c.pose = Pose::Idle;
                return true;
            }
            false
        }
        Act::Sleep { t, zt } => {
            c.pose = Pose::Sleep;
            c.frame = 0;
            *t -= dt;
            *zt -= dt;
            if *zt <= 0.0 && !reduced {
                *zt = 1.3;
                let (hx, hr) = c.head();
                let mut p = Particle::new(Kind::Icon(Icon::Zed), hx, hr, 2.4, Icon::Zed.color());
                p.vx = 6.0 * c.dir;
                p.vrise = 14.0;
                particles.push(p);
            }
            *t <= 0.0
        }
        Act::Emote(icon) => {
            let (hx, hr) = c.head();
            emote(particles, hx, hr, *icon, reduced);
            true
        }
        Act::Say(text) => {
            c.bubble = Some(Bubble {
                text: std::mem::take(text),
                t: 2.6,
            });
            true
        }
        Act::Hops { n, e, h } => {
            *e += dt;
            c.pose = Pose::Walk;
            let ph = *e * 2.4;
            c.frame = usize::from(ph.fract() < 0.8);
            c.lift = (arch(ph.fract()) * *h).round();
            if ph >= *n {
                c.lift = 0.0;
                c.pose = Pose::Idle;
                c.frame = 0;
                dust(particles, rng, c.x, 0.0, 4, reduced);
                return true;
            }
            false
        }
        Act::Pushups { n, e } => {
            *e += dt;
            c.pose = Pose::Idle;
            c.lift = ((*e * 3.0) as usize % 2) as f64;
            if *e >= *n / 1.5 {
                c.lift = 0.0;
                return true;
            }
            false
        }
        Act::Pop { text, color } => {
            if !reduced {
                let (hx, hr) = c.head();
                let mut p = Particle::new(Kind::Text, hx, hr + 6.0, 2.2, *color);
                p.text = std::mem::take(text);
                p.vrise = 14.0;
                particles.push(p);
            }
            true
        }
        Act::DropItem => {
            if !s.drop.is_empty() {
                let x = c.x - c.dir * (c.half_w() + 2.0);
                let mut item = Particle::new(Kind::Item, x, 0.0, ITEM_LIFE, ink::BONE);
                item.text = s.drop.to_string();
                particles.push(item);
                if !reduced {
                    let mut p = Particle::new(Kind::Text, x, 6.0, 2.2, ink::BONE);
                    p.text = s.drop.to_string();
                    p.vrise = 12.0;
                    particles.push(p);
                }
            }
            true
        }
        Act::Puff => {
            if !reduced {
                for (dx, dy) in RING {
                    let mut p =
                        Particle::new(Kind::Smoke, c.x, c.lift + c.height() / 2.0, 0.7, ink::SMOKE);
                    p.vx = dx * 18.0;
                    p.vrise = dy * 12.0;
                    particles.push(p);
                }
            }
            c.alpha = if c.alpha > 0.5 { 0.0 } else { 1.0 };
            true
        }
        Act::Spores => {
            if !reduced {
                for _ in 0..6 {
                    let mut p = Particle::new(
                        Kind::Sparkle,
                        c.x + rng.range(-4.0, 4.0),
                        c.height(),
                        1.6,
                        [0xe8, 0xd8, 0xb0],
                    );
                    p.vrise = rng.range(4.0, 10.0);
                    p.vx = rng.range(-5.0, 5.0);
                    particles.push(p);
                }
            }
            c.pose = Pose::Eat;
            true
        }
        Act::Sink { to } => {
            c.pose = Pose::Idle;
            let d = *to - c.lift;
            let step = (8.0 * dt).min(d.abs()) * d.signum();
            c.lift += step;
            if rng.chance(dt * 6.0) {
                dust(particles, rng, c.x, 0.0, 2, reduced);
            }
            (*to - c.lift).abs() < 0.01
        }
        Act::Jump(x) => {
            c.x = *x;
            true
        }
        Act::Flash { t } => {
            *t -= dt;
            c.pose = Pose::Idle;
            // Faster as the fuse burns down.
            let rate = if *t < 0.6 { 12.0 } else { 5.0 };
            c.flash = (c.anim * rate) as usize % 2 == 1;
            *t <= 0.0
        }
        Act::Burst => {
            if !reduced {
                for i in 0..14 {
                    let mut p = Particle::new(
                        Kind::Confetti,
                        c.x,
                        c.height() / 2.0,
                        1.8,
                        ink::CONFETTI[i % ink::CONFETTI.len()],
                    );
                    p.vx = rng.range(-50.0, 50.0);
                    p.vrise = rng.range(30.0, 100.0);
                    p.gravity = 120.0;
                    particles.push(p);
                }
                let mut p = Particle::new(Kind::Text, c.x, c.height(), 1.6, ink::GOLD);
                p.text = "pop!".into();
                p.vrise = 16.0;
                particles.push(p);
            }
            c.gone = true;
            true
        }
        Act::Pinch => {
            if !reduced {
                let (hx, hr) = c.head();
                let mut p = Particle::new(Kind::Text, hx, hr + 6.0, 2.2, ink::GOLD);
                p.text = "-1 shiny".into();
                p.vrise = 14.0;
                particles.push(p);
            }
            true
        }
        Act::Carry { item, to } => {
            // Find the nearest free item the first time; no item, a twirl.
            if item.is_none() {
                let near = particles
                    .iter()
                    .enumerate()
                    .filter(|(_, p)| p.kind == Kind::Item && p.carried_by.is_none())
                    .min_by(|a, b| (a.1.x - c.x).abs().total_cmp(&(b.1.x - c.x).abs()))
                    .map(|(i, _)| i);
                match near {
                    Some(i) => *item = Some(i),
                    None => {
                        let (hx, hr) = c.head();
                        emote(particles, hx, hr, Icon::Note, reduced);
                        return true;
                    }
                }
            }
            let i = item.expect("set above");
            let Some(p) = particles.get(i) else {
                return true;
            };
            if p.kind != Kind::Item {
                return true;
            }
            if p.carried_by != Some(c.id) {
                let px = p.x;
                if move_toward(c, px, dt, s.speed, particles, reduced) {
                    particles[i].carried_by = Some(c.id);
                    particles[i].life = ITEM_LIFE;
                }
                c.pose = Pose::Walk;
                c.frame = (c.anim * 5.0) as usize % 2;
                return false;
            }
            let lo = 0.06 * ctx.view_w;
            let hi = 0.94 * ctx.view_w;
            let dest = to.clamp(lo, hi);
            c.pose = Pose::Walk;
            c.frame = (c.anim * 5.0) as usize % 2;
            if move_toward(c, dest, dt, s.speed, particles, reduced) {
                particles[i].carried_by = None;
                particles[i].rise = 0.0;
                c.bubble = Some(Bubble {
                    text: "for you!".into(),
                    t: 2.6,
                });
                let (hx, hr) = c.head();
                emote(particles, hx, hr, Icon::Heart, reduced);
                c.pose = Pose::Idle;
                return true;
            }
            false
        }
        Act::Throw { target } => {
            let (hx, hr) = c.head();
            if let Some((_, _, bx, bl)) = ctx.others.iter().find(|o| o.0 == *target) {
                let mut p = Particle::new(Kind::Dart, hx, hr, 0.5, [0xd8, 0xd0, 0xc0]);
                p.vx = (*bx - hx) / 0.5;
                p.vrise = (*bl + 6.0 - hr) / 0.5;
                particles.push(p);
                fx.push(Effect::Plan(*target, vec![Act::Wait(0.5), Act::PopSelf]));
            }
            c.pose = Pose::Eat;
            true
        }
        Act::PopSelf => {
            if !reduced {
                for i in 0..10 {
                    let mut p = Particle::new(
                        Kind::Confetti,
                        c.x,
                        c.lift + c.height() / 2.0,
                        1.4,
                        ink::CONFETTI[i % ink::CONFETTI.len()],
                    );
                    p.vx = rng.range(-40.0, 40.0);
                    p.vrise = rng.range(10.0, 60.0);
                    p.gravity = 120.0;
                    particles.push(p);
                }
                let mut p = Particle::new(Kind::Text, c.x, c.lift + c.height(), 1.4, ink::GOLD);
                p.text = "pop!".into();
                p.vrise = 16.0;
                particles.push(p);
            }
            c.gone = true;
            true
        }
        Act::Merge { with } => {
            fx.push(Effect::Merge(c.id, *with));
            true
        }
        Act::Offer { t } => {
            c.offering = true;
            c.pose = Pose::Idle;
            *t -= dt;
            if *t <= 0.0 {
                c.offering = false;
                c.bubble = Some(Bubble {
                    text: "...fine".into(),
                    t: 2.0,
                });
                c.budget = 0.0;
                return true;
            }
            false
        }
        Act::Breathe => {
            if !reduced {
                let (hx, hr) = c.head();
                for i in 0..4 {
                    let mut p = Particle::new(
                        Kind::Smoke,
                        hx + c.dir * 3.0,
                        hr - 2.0,
                        0.9 + f64::from(i) * 0.1,
                        ink::SMOKE,
                    );
                    p.vx = c.dir * rng.range(10.0, 22.0);
                    p.vrise = rng.range(2.0, 8.0);
                    particles.push(p);
                }
            }
            c.pose = Pose::Eat;
            true
        }
    }
}

/// Walking by gait (the overlay's walkTo); true on arrival.
fn walk(
    c: &mut Critter,
    tx: f64,
    pace: f64,
    dt: f64,
    particles: &mut Vec<Particle>,
    reduced: bool,
) -> bool {
    let s = c.sp();
    let speed = s.speed * pace;
    match s.gait {
        Gait::Hop => {
            let ph = (c.anim * 2.2).fract();
            let h = hop_height(c);
            c.lift = (arch(ph) * h).round();
            c.pose = if c.lift > 0.0 { Pose::Walk } else { Pose::Idle };
            c.frame = 1;
            let moving = if ph < 0.85 { 1.3 } else { 0.0 };
            let done = move_toward(c, tx, dt, speed * moving, particles, reduced);
            if done {
                c.lift = 0.0;
                c.pose = Pose::Idle;
                c.frame = 0;
            }
            done
        }
        _ => {
            c.pose = Pose::Walk;
            let rate = match s.gait {
                Gait::Slide => 1.2,
                Gait::Dart => 9.0,
                Gait::Waddle => 4.0,
                _ => 5.0,
            };
            c.frame = (c.anim * rate) as usize % 2;
            if s.gait == Gait::Waddle {
                c.lift = c.frame as f64;
            }
            let mut sp = speed;
            if s.gait == Gait::Dart {
                let ph = (c.anim * 0.9).fract();
                if ph >= 0.55 {
                    sp = 0.0;
                    c.pose = Pose::Idle;
                } else {
                    sp *= 2.2;
                }
            }
            let done = move_toward(c, tx, dt, sp, particles, reduced);
            if done {
                if s.gait == Gait::Waddle {
                    c.lift = 0.0;
                }
                c.pose = Pose::Idle;
                c.frame = 0;
            }
            done
        }
    }
}

fn held(c: &mut Critter, ctx: &Ctx) {
    let dt = ctx.dt;
    c.pose = Pose::Held;
    c.frame = (c.anim * 6.0) as usize % 2;
    let Some(h) = c.hold.as_mut() else { return };
    let Some(p) = ctx.pointer else { return };
    if let Some((lx, ly)) = h.last {
        let (vx, vy) = ((p.x - lx) / dt, (p.y - ly) / dt);
        h.vx = h.vx * 0.5 + vx * 0.5;
        h.vy = h.vy * 0.5 + vy * 0.5;
    }
    h.last = Some((p.x, p.y));
    let hw = sprite(c.species).w as f64 / 2.0;
    c.x = (p.x + h.dx).clamp(hw, (ctx.view_w - hw).max(hw));
    let feet = p.y + h.dy;
    c.lift = (ctx.floor - feet).clamp(0.0, (ctx.floor - c.height()).max(0.0));
}

fn air(c: &mut Critter, ctx: &Ctx, rng: &mut Rng, particles: &mut Vec<Particle>) {
    let dt = ctx.dt;
    let hw = c.half_w();
    c.pose = Pose::Held;
    c.frame = 0;
    let prev_y = ctx.floor - c.lift;
    c.vlift -= GRAVITY * dt;
    c.x += c.vx * dt;
    c.lift += c.vlift * dt;
    // Walls and ceiling.
    let (lo, hi) = (hw, (ctx.view_w - hw).max(hw));
    if c.x < lo || c.x > hi {
        c.x = c.x.clamp(lo, hi);
        c.vx = -c.vx * WALL_BOUNCE;
    }
    let ceiling = (ctx.floor - c.height()).max(0.0);
    if c.lift > ceiling {
        c.lift = ceiling;
        c.vlift = c.vlift.min(0.0);
    }
    // Perches catch a falling critter whose feet cross their top.
    let y = ctx.floor - c.lift;
    let mut ground = 0.0;
    let mut perch = None;
    if c.vlift < 0.0 {
        for r in ctx.perches {
            if prev_y <= r.y && y >= r.y && c.x >= r.x && c.x <= r.x + r.w && r.y < ctx.floor {
                ground = ctx.floor - r.y;
                perch = Some(r.y);
                break;
            }
        }
    }
    if c.lift <= ground {
        c.lift = ground;
        if -c.vlift > SETTLE_SPEED {
            c.vlift = -c.vlift * BOUNCE;
            c.vx *= FRICTION;
            c.squash_t = SQUASH_TIME;
            dust(particles, rng, c.x, ground, 3, ctx.reduced);
        } else {
            // Settled.
            c.vlift = 0.0;
            c.vx = 0.0;
            c.mode = Mode::Brain;
            c.perch = perch;
            c.squash_t = SQUASH_TIME;
            c.pose = Pose::Idle;
            c.cur = None;
            c.meeting = false;
            let s = c.sp();
            let mut acts = vec![
                Act::Pose {
                    pose: Pose::Idle,
                    t: 0.3,
                },
                Act::Emote(Icon::What),
            ];
            if !s.lines.is_empty() && rng.chance(0.4) {
                acts.push(Act::Say(s.lines[rng.index(s.lines.len())].to_string()));
            }
            acts.push(Act::Pose {
                pose: Pose::Idle,
                t: 0.6,
            });
            if !c.entered {
                acts.push(Act::Mark(Mark::Entered));
            }
            c.acts = acts.into();
        }
    }
}

fn tick_particles(particles: &mut Vec<Particle>, critters: &[Critter], dt: f64, reduced: bool) {
    if reduced {
        // Only lying items survive reduced motion.
        particles.retain(|p| p.kind == Kind::Item);
    }
    for p in particles.iter_mut() {
        p.life -= dt;
        if let Some(id) = p.carried_by {
            match critters.iter().find(|c| c.id == id) {
                Some(c) => {
                    p.x = c.x + c.dir * (c.half_w() - 1.0);
                    p.rise = c.lift + 2.0;
                    continue;
                }
                None => p.carried_by = None,
            }
        }
        p.x += p.vx * dt;
        p.rise += p.vrise * dt;
        p.vrise -= p.gravity * dt;
        if p.gravity > 0.0 && p.rise < 0.0 {
            p.rise = 0.0;
            p.vrise = 0.0;
            p.vx *= 0.5;
        }
    }
    particles.retain(|p| p.life > 0.0);
}
