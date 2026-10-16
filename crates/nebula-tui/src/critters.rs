//! The CRITTERS: little pixel visitors that wander along the bottom of the
//! screen, an easter egg (Settings → Appearance → Critters, the `critters`
//! verb, or ↑↑↓↓←→←→BA). `kete-critters` is the cast and the world; this
//! is the terminal's half: drawing them in half-block cells over the frame,
//! routing the mouse to them first, ticking only while one is on screen,
//! and the collection log.
//!
//! Cost: with nothing on screen the world asks for one wake at the next
//! visitor's arrival and the loop sleeps until then; while a critter is
//! up it ticks at 8 fps. Nothing runs with `animations` off, with the
//! setting off, or while the terminal window is out of focus.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use kete_critters::log::CritterLog;
use kete_critters::paint::{self, Canvas, Opts};
use kete_critters::species::{flag, Origin, Special, SPECIES};
use kete_critters::world::{self, AppEvent, Input, Often, Pointer, Rect as WRect, World};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};

use crate::app::{App, ContextMenu, HitTarget, MenuAction, MenuItem};

/// The tick while a critter is on screen: 8 fps.
pub const FRAME: Duration = Duration::from_millis(125);

/// How often a day of "Hide critters until tomorrow" looks at the date.
const HIDDEN_CHECK: Duration = Duration::from_secs(600);

/// ↑↑↓↓←→←→BA.
const KONAMI: [KeyCode; 10] = [
    KeyCode::Up,
    KeyCode::Up,
    KeyCode::Down,
    KeyCode::Down,
    KeyCode::Left,
    KeyCode::Right,
    KeyCode::Left,
    KeyCode::Right,
    KeyCode::Char('b'),
    KeyCode::Char('a'),
];

/// `critters_size`: how many sprite pixels a cell holds. A terminal cell
/// is about twice as tall as wide, so half blocks are the smallest square
/// pixel it has: `small` draws as `medium` here (quadrant blocks would
/// make every pixel twice as tall as wide). The desktop has all three.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Size {
    /// Half blocks: 1×2 pixels a cell (`▀` over `▄`).
    #[default]
    Medium,
    /// Two columns a pixel, one row.
    Large,
}

impl Size {
    pub fn parse(s: &str) -> Size {
        match s {
            "large" => Size::Large,
            _ => Size::Medium,
        }
    }

    /// Sprite pixels per cell, across and down.
    fn per_cell(self) -> (f64, f64) {
        match self {
            Size::Medium => (1.0, 2.0),
            Size::Large => (0.5, 1.0),
        }
    }
}

/// A critter menu row's verb (`MenuAction::Critter`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CritterAct {
    Pet(u32),
    Feed(u32),
    Name(u32),
    Shoo(u32),
    HideToday,
    Log,
    Settings,
}

/// The left button went down on a critter: a click pets it, a drag
/// carries it.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Press {
    id: u32,
    at: (u16, u16),
    /// Where it was pressed, in sprite pixels: the grab holds it there.
    from: Pointer,
    grabbed: bool,
}

/// The app state the critters react to, as of the last look.
#[derive(Debug, Clone, Default, PartialEq)]
struct Seen {
    running: Vec<nebula_core::AgentId>,
    needs: Vec<nebula_core::AgentId>,
    merged: Option<Instant>,
}

#[derive(Default)]
pub struct Critters {
    /// Live while the overlay is on.
    pub world: Option<World>,
    /// The `critters` setting.
    pub on: bool,
    pub size: Size,
    pub often: Often,
    /// When the world last ticked.
    last: Option<Instant>,
    /// How far into the Konami code the last keys got.
    konami: usize,
    /// The collection log as last read: a copy for drawing. Every change
    /// re-reads the file first ([`update_log`]); the desktop and other
    /// terminal windows write it too.
    pub log: CritterLog,
    /// Where the overlay was last drawn: the screen, the floor row (the
    /// row critters stand on is the one above it), and the perches.
    screen: Rect,
    floor_row: u16,
    perches: Vec<WRect>,
    pointer: Option<Pointer>,
    press: Option<Press>,
    seen: Option<Seen>,
    events: Vec<AppEvent>,
    all_idle: bool,
    /// The terminal shows 24-bit color (else the 256-color cube).
    truecolor: bool,
}

impl Critters {
    pub fn new() -> Critters {
        Critters {
            truecolor: truecolor(),
            ..Critters::default()
        }
    }

    /// Up and allowed to run: the setting on, animations on.
    pub fn live(&self) -> bool {
        self.on && self.world.is_some()
    }
}

/// Whether the host terminal does 24-bit color. Everything but Apple's
/// Terminal before Tahoe does; it says so by not setting COLORTERM.
fn truecolor() -> bool {
    let colorterm = std::env::var("COLORTERM").unwrap_or_default();
    if colorterm.contains("truecolor") || colorterm.contains("24bit") {
        return true;
    }
    std::env::var("TERM_PROGRAM").as_deref() != Ok("Apple_Terminal")
}

// ---------- the setting ----------

/// Adopts the critters settings (from `apply_config`): turning on makes a
/// world, turning off drops it.
pub fn apply(app: &mut App, on: bool, size: &str, often: &str) {
    let c = &mut app.critters;
    c.size = Size::parse(size);
    c.often = Often::parse(often);
    let on = on && app.chrome.animations;
    if on == c.on && (c.world.is_some() == on) {
        if let Some(w) = c.world.as_mut() {
            w.often = c.often;
        }
        return;
    }
    c.on = on;
    if on {
        load_log(c);
        let seed = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(1, |d| d.as_millis() as u32);
        let mut w = World::new(seed);
        w.often = c.often;
        w.names = c.log.names();
        w.spawning = !c.log.hidden(&local_now().date);
        c.world = Some(w);
        c.last = Some(Instant::now());
        c.seen = None;
    } else {
        c.world = None;
        c.press = None;
    }
    app.chrome.dirty = true;
}

/// The `critters` verb and the Konami code: flip the setting, save it,
/// say how to undo it.
pub fn toggle(app: &mut App) {
    let mut cfg = crate::config::Config::load();
    cfg.critters = !cfg.critters;
    if !crate::event_loop::save_config(app, &cfg) {
        return;
    }
    apply(
        app,
        cfg.critters,
        cfg.critters_size_word(),
        cfg.critters_often_word(),
    );
    app.chrome.flash = Some(if !cfg.critters {
        "critters off".into()
    } else if !app.chrome.animations {
        "critters on, but animations are off (Settings → Appearance)".into()
    } else {
        "critters on: one will wander by soon. Ctrl+K → Critters (or ↑↑↓↓←→←→BA) turns them off"
            .into()
    });
    if cfg.critters && app.chrome.animations {
        // A first visitor right away, so the toggle shows something.
        summon_soon(app);
    }
}

fn summon_soon(app: &mut App) {
    let input = input_of(&app.critters, local_now().hour);
    if let Some(w) = app.critters.world.as_mut() {
        if w.critters.is_empty() && w.spawning {
            w.summon("kiwi", &input);
        }
    }
}

// ---------- time ----------

/// The world wants a wake: the setting on, the window in focus, and either
/// something on screen or a visitor due.
pub fn armed(app: &App) -> bool {
    app.chrome.window_focused
        && app.chrome.animations
        && app.critters.world.as_ref().is_some_and(|w| {
            w.active()
                || w.next_wake().is_some()
                || !app.critters.events.is_empty()
                || app.critters.log.hidden_on.is_some()
        })
}

/// When to wake next: the next frame while something is on screen, else
/// when the next visitor is due.
pub fn deadline(app: &App) -> Instant {
    let c = &app.critters;
    let now = Instant::now();
    let Some(w) = &c.world else {
        return now + Duration::from_secs(3600);
    };
    if !c.events.is_empty() {
        return now;
    }
    let last = c.last.unwrap_or(now);
    if w.active() {
        return last + FRAME;
    }
    match w.next_wake() {
        Some(secs) => last + Duration::from_secs_f64(secs.min(3600.0)),
        // Hidden until tomorrow: look at the date now and then.
        None => last + HIDDEN_CHECK,
    }
}

/// Focus came back: time away doesn't count toward the next visit.
pub fn refocused(app: &mut App) {
    if app.critters.world.is_some() {
        app.critters.last = Some(Instant::now());
    }
}

/// Ticks the world for the time since the last tick.
pub fn wake(app: &mut App) {
    observe(app);
    let now = Instant::now();
    let c = &mut app.critters;
    let Some(last) = c.last.replace(now) else {
        return;
    };
    let clock = local_now();
    let mut input = input_of(c, clock.hour);
    input.events = std::mem::take(&mut c.events);
    // Between visits, a look at the log: "Hide critters until tomorrow"
    // may have been chosen in the other app, or the day has turned.
    if c.world.as_ref().is_some_and(|w| !w.active()) {
        load_log(c);
        let hidden = c.log.hidden(&clock.date);
        if c.log.hidden_on.is_some() && !hidden {
            update_log(c, |log| log.hidden_on = None);
        }
        if let Some(w) = c.world.as_mut() {
            if hidden {
                w.spawning = false;
            } else if !w.spawning {
                w.resume();
            }
        }
    }
    let Some(w) = c.world.as_mut() else { return };
    let was = w.active();
    w.tick(now.saturating_duration_since(last).as_secs_f64(), &input);
    let notes = w.drain_notes();
    let active = w.active();
    if !notes.is_empty() {
        update_log(c, |log| {
            for n in &notes {
                log.record(n, &clock.date);
            }
        });
    }
    if active || was {
        app.chrome.dirty = true;
    }
}

pub(crate) fn input_of(c: &Critters, hour: u8) -> Input {
    let (px, py) = c.size.per_cell();
    let cols = f64::from(c.screen.width.max(20));
    let rows = f64::from(c.screen.height.max(6));
    Input {
        view_w: cols * px,
        view_h: rows * py,
        floor: f64::from(c.floor_row.saturating_sub(c.screen.y)) * py,
        perches: c.perches.clone(),
        pointer: c.pointer,
        hour,
        reduced_motion: false,
        all_idle: c.all_idle,
        events: Vec::new(),
    }
}

/// App events from what changed since the last look: a session that
/// started needing you, a turn that finished, a pull request that merged.
fn observe(app: &mut App) {
    use nebula_core::AgentStatus;
    let now = Seen {
        running: app
            .tree
            .agents
            .iter()
            .filter(|a| !a.archived && a.status == AgentStatus::Running)
            .map(|a| a.id.clone())
            .collect(),
        needs: app
            .tree
            .agents
            .iter()
            .filter(|a| !a.archived && a.status == AgentStatus::NeedsFeedback)
            .map(|a| a.id.clone())
            .collect(),
        merged: app.github.merge_landed.values().max().copied(),
    };
    let c = &mut app.critters;
    c.all_idle = now.running.is_empty() && now.needs.is_empty();
    let Some(before) = c.seen.replace(now.clone()) else {
        return;
    };
    if now.needs.iter().any(|id| !before.needs.contains(id)) {
        c.events.push(AppEvent::NeedsYou { x: None });
    }
    let finished = before.running.iter().any(|id| {
        !now.running.contains(id)
            && app
                .tree
                .agents
                .iter()
                .any(|a| &a.id == id && a.status == AgentStatus::Finished)
    });
    if finished {
        c.events.push(AppEvent::TurnDone);
    }
    if now.merged.is_some() && now.merged != before.merged {
        c.events.push(AppEvent::PrMerged { x: None });
    }
}

// ---------- drawing ----------

/// Draws the critters over the finished frame (last thing drawn): sprites
/// in block characters, their speech and names as text. Cells no critter
/// covers are left alone; a cell half covered keeps its background in the
/// other half.
pub fn draw(buf: &mut Buffer, app: &mut App, screen: Rect) {
    // The layout is kept even with the critters off, so the first visitor
    // after turning them on lands on the real floor.
    app.critters.screen = screen;
    app.critters.floor_row = screen.y + screen.height.saturating_sub(1);
    if app.critters.world.is_none() {
        return;
    }
    observe(app);
    let perches = perches_of(app, screen);
    let theme_bg = app.chrome.theme.bg;
    let c = &mut app.critters;
    c.perches = perches;
    let Some(w) = c.world.as_ref() else { return };
    if !w.active() {
        return;
    }
    let (px, py) = c.size.per_cell();
    let vw = (f64::from(screen.width) * px).round() as usize;
    let vh = (f64::from(screen.height) * py).round() as usize;
    // Painted at the size of this frame; after a resize the world takes
    // the new floor and width on its next tick.
    let mut canvas = Canvas::new(vw, vh);
    paint::paint(w, &mut canvas, Opts { text: false });
    let truecolor = c.truecolor;
    let fallback = theme_bg
        .and_then(crate::theme::color_rgb)
        .unwrap_or((0, 0, 0));
    for row in 0..screen.height {
        for col in 0..screen.width {
            let pos = (screen.x + col, screen.y + row);
            let Some(cell) = buf.cell_mut(pos) else {
                continue;
            };
            let under = crate::theme::color_rgb(cell.bg).unwrap_or(fallback);
            let pick = |x: usize, y: usize| -> Option<(u8, u8, u8)> {
                if x >= canvas.w || y >= canvas.h {
                    return None;
                }
                let p = canvas.get(x, y);
                match p[3] {
                    0..=24 => None,
                    250..=255 => Some((p[0], p[1], p[2])),
                    a => {
                        let a = f32::from(a) / 255.0;
                        let mix = |s: u8, d: u8| {
                            (f32::from(s) * a + f32::from(d) * (1.0 - a)).round() as u8
                        };
                        Some((mix(p[0], under.0), mix(p[1], under.1), mix(p[2], under.2)))
                    }
                }
            };
            let color = |rgb: (u8, u8, u8)| to_color(rgb, truecolor);
            match c.size {
                Size::Medium => {
                    let (x, y) = (usize::from(col), usize::from(row) * 2);
                    match (pick(x, y), pick(x, y + 1)) {
                        (None, None) => {}
                        (Some(t), Some(b)) => {
                            cell.set_char('▀').set_fg(color(t)).set_bg(color(b));
                        }
                        (Some(t), None) => {
                            cell.set_char('▀').set_fg(color(t));
                        }
                        (None, Some(b)) => {
                            cell.set_char('▄').set_fg(color(b));
                        }
                    }
                    cell.modifier = Modifier::empty();
                }
                Size::Large => {
                    if let Some(p) = pick(usize::from(col) / 2, usize::from(row)) {
                        cell.set_char(' ').set_bg(color(p));
                        cell.modifier = Modifier::empty();
                    }
                }
            }
        }
    }
    draw_text(buf, &app.critters, screen);
}

/// An RGB triple.
type Rgb = (u8, u8, u8);

fn to_color(rgb: Rgb, truecolor: bool) -> Color {
    if truecolor {
        return Color::Rgb(rgb.0, rgb.1, rgb.2);
    }
    // The 6×6×6 cube, or the gray ramp when that's nearer.
    let level = |v: u8| -> u8 {
        if v < 48 {
            0
        } else if v < 115 {
            1
        } else {
            ((v - 35) / 40).min(5)
        }
    };
    let (r, g, b) = (level(rgb.0), level(rgb.1), level(rgb.2));
    let cube = |l: u8| if l == 0 { 0 } else { 55 + l * 40 };
    let cube_rgb = (cube(r), cube(g), cube(b));
    let gray = ((u16::from(rgb.0) + u16::from(rgb.1) + u16::from(rgb.2)) / 3) as u8;
    let gi = if gray < 8 {
        0
    } else {
        ((gray - 8) / 10).min(23)
    };
    let gv = 8 + gi * 10;
    let d = |a: (u8, u8, u8), b: (u8, u8, u8)| {
        let f = |x: u8, y: u8| (i32::from(x) - i32::from(y)).pow(2);
        f(a.0, b.0) + f(a.1, b.1) + f(a.2, b.2)
    };
    if d(rgb, (gv, gv, gv)) < d(rgb, cube_rgb) {
        Color::Indexed(232 + gi)
    } else {
        Color::Indexed(16 + 36 * r + 6 * g + b)
    }
}

/// Screen cell of a sprite pixel.
fn cell_of(c: &Critters, x: f64, y: f64) -> (i32, i32) {
    let (px, py) = c.size.per_cell();
    (
        i32::from(c.screen.x) + (x / px).floor() as i32,
        i32::from(c.screen.y) + (y / py).floor() as i32,
    )
}

/// Speech bubbles, the hovered critter's name and the text pops, set in
/// the terminal's own type (crisper than the pixel font at one pixel a
/// half cell).
fn draw_text(buf: &mut Buffer, c: &Critters, screen: Rect) {
    let Some(w) = &c.world else { return };
    let ink = |rgb: kete_critters::Rgb| to_color((rgb[0], rgb[1], rgb[2]), c.truecolor);
    let bubble = Style::default()
        .fg(ink(world::ink::TEXT))
        .bg(ink(world::ink::BUBBLE));
    let mut put = |text: &str, cx: i32, row: i32, style: Style| {
        let width = unicode_width::UnicodeWidthStr::width(text) as i32;
        let max_x = i32::from(screen.x + screen.width) - width;
        let x = (cx - width / 2).clamp(i32::from(screen.x), max_x.max(i32::from(screen.x)));
        if row < i32::from(screen.y) || row >= i32::from(screen.y + screen.height) {
            return;
        }
        buf.set_string(x as u16, row as u16, text, style);
    };
    for cr in &w.critters {
        let s = kete_critters::cast::sprite(cr.species);
        let top = w.floor - cr.lift - s.h as f64;
        let (cx, row) = cell_of(c, cr.head().0, top);
        if let Some(b) = &cr.bubble {
            put(&format!(" {} ", b.text), cx, row - 1, bubble);
        } else if w.hover == Some(cr.id) {
            let tag = format!(" {} ", cr.name);
            put(
                &tag,
                cell_of(c, cr.x, top).0,
                row - 1,
                Style::default()
                    .fg(ink(world::ink::BONE))
                    .bg(ink(world::ink::SOOT)),
            );
        }
    }
    for p in &w.particles {
        if p.kind != world::Kind::Text {
            continue;
        }
        let (cx, row) = cell_of(c, p.x, w.floor - p.rise);
        put(
            &p.text,
            cx,
            row,
            Style::default()
                .fg(ink(p.color))
                .add_modifier(Modifier::BOLD),
        );
    }
}

/// Fliers perch on the top edge of a band's rule or a board lane.
fn perches_of(app: &App, screen: Rect) -> Vec<WRect> {
    let (px, py) = app.critters.size.per_cell();
    app.chrome
        .hits
        .iter()
        .filter(|(_, t)| matches!(t, HitTarget::LauncherBand(_) | HitTarget::KanbanLane(_)))
        .filter(|(r, _)| r.y > screen.y + 3)
        .map(|(r, _)| WRect {
            x: f64::from(r.x.saturating_sub(screen.x)) * px,
            y: f64::from(r.y.saturating_sub(screen.y)) * py,
            w: f64::from(r.width) * px,
            h: py,
        })
        .collect()
}

// ---------- the mouse ----------

/// The sprite pixels a cell covers, centre points.
fn cell_pixels(c: &Critters, col: u16, row: u16) -> Vec<(f64, f64)> {
    let (px, py) = c.size.per_cell();
    let x0 = f64::from(col.saturating_sub(c.screen.x)) * px;
    let y0 = f64::from(row.saturating_sub(c.screen.y)) * py;
    let mut out = Vec::new();
    let (nx, ny) = (px.max(1.0) as usize, py.max(1.0) as usize);
    for j in 0..ny {
        for i in 0..nx {
            out.push((
                x0 + i as f64 + 0.5 * px.min(1.0),
                y0 + j as f64 + 0.5 * py.min(1.0),
            ));
        }
    }
    out
}

/// The critter drawn in a cell, if any.
pub fn critter_at(app: &App, col: u16, row: u16) -> Option<u32> {
    let c = &app.critters;
    let w = c.world.as_ref()?;
    cell_pixels(c, col, row)
        .into_iter()
        .find_map(|(x, y)| w.hit(x, y))
}

/// The mouse goes to the critters first, but only where one is drawn (or
/// while one is held): a click there pets, a drag carries, a right-click
/// opens its menu. True when the event was theirs.
pub fn handle_mouse(app: &mut App, mouse: &MouseEvent) -> bool {
    if app.critters.world.is_none() {
        return false;
    }
    let (px, py) = app.critters.size.per_cell();
    let at = (mouse.column, mouse.row);
    let pointer = Pointer {
        x: f64::from(mouse.column.saturating_sub(app.critters.screen.x)) * px + 0.5 * px.min(1.0),
        y: f64::from(mouse.row.saturating_sub(app.critters.screen.y)) * py + 0.5 * py.min(1.0),
    };
    app.critters.pointer = Some(pointer);
    if let Some(press) = app.critters.press {
        match mouse.kind {
            MouseEventKind::Drag(MouseButton::Left) => {
                if !press.grabbed && at != press.at {
                    if let Some(w) = app.critters.world.as_mut() {
                        w.grab(press.id, press.from.x, press.from.y);
                    }
                    app.critters.press = Some(Press {
                        grabbed: true,
                        ..press
                    });
                }
                app.chrome.dirty = true;
                return true;
            }
            MouseEventKind::Up(MouseButton::Left) => {
                app.critters.press = None;
                if let Some(w) = app.critters.world.as_mut() {
                    if press.grabbed {
                        w.release();
                    } else {
                        w.pet(press.id);
                    }
                }
                app.chrome.dirty = true;
                return true;
            }
            // The release never came (it happened outside the window): a
            // plain move or another press means the button is up. Let go
            // and let the event through.
            MouseEventKind::Moved | MouseEventKind::Down(_) => {
                app.critters.press = None;
                if press.grabbed {
                    if let Some(w) = app.critters.world.as_mut() {
                        w.release();
                    }
                }
            }
            _ => {}
        }
    }
    let hit = critter_at(app, mouse.column, mouse.row);
    match (mouse.kind, hit) {
        (MouseEventKind::Down(MouseButton::Left), Some(id)) => {
            app.critters.press = Some(Press {
                id,
                at,
                from: pointer,
                grabbed: false,
            });
            true
        }
        (MouseEventKind::Down(MouseButton::Right), Some(id)) => {
            open_menu(app, id, at);
            true
        }
        _ => false,
    }
}

// ---------- the keyboard ----------

/// Tracks the Konami code on keys outside text entry. The arrows do what
/// they do; the B and the A that finish the code are taken (an `a` on the
/// sessions panel would otherwise archive). True when the key is taken.
pub fn konami(app: &mut App, key: &KeyEvent) -> bool {
    let plain = key.modifiers.difference(KeyModifiers::SHIFT).is_empty();
    let code = match key.code {
        KeyCode::Char(ch) => KeyCode::Char(ch.to_ascii_lowercase()),
        other => other,
    };
    let c = &mut app.critters;
    if !plain {
        c.konami = 0;
        return false;
    }
    if code == KONAMI[c.konami] {
        c.konami += 1;
    } else {
        c.konami = usize::from(code == KONAMI[0]);
        return false;
    }
    let taken = c.konami > 8;
    if c.konami == KONAMI.len() {
        c.konami = 0;
        toggle(app);
    }
    taken
}

// ---------- the menu ----------

/// The critter menu, at the pointer: rows that make sense for this one.
pub fn open_menu(app: &mut App, id: u32, at: (u16, u16)) {
    let Some(cr) = app.critters.world.as_ref().and_then(|w| w.get(id)) else {
        return;
    };
    let sp = cr.species();
    let asleep = cr.pose == kete_critters::Pose::Sleep;
    let mut items = Vec::new();
    let pet = if sp.has(flag::INANIMATE) {
        "Pop"
    } else if sp.special == Special::Lamp {
        "Take the lamp"
    } else if asleep {
        "Pet (wakes it)"
    } else {
        "Pet"
    };
    let act = |a: CritterAct| MenuAction::Critter(a);
    items.push(MenuItem::new(pet, act(CritterAct::Pet(id))));
    if !cr.exiting && sp.gait != kete_critters::species::Gait::Float {
        items.push(MenuItem::new("Feed", act(CritterAct::Feed(id))));
    }
    items.push(MenuItem::new("Name…", act(CritterAct::Name(id))));
    if !cr.exiting {
        items.push(MenuItem::new("Shoo", act(CritterAct::Shoo(id))));
    }
    items.push(MenuItem::new(
        "Hide critters until tomorrow",
        act(CritterAct::HideToday),
    ));
    items.push(MenuItem::new("Critter log…", act(CritterAct::Log)));
    items.push(MenuItem::new(
        "Critter settings…",
        act(CritterAct::Settings),
    ));
    let title = if cr.name.is_empty() {
        sp.name.to_string()
    } else {
        format!("{} the {}", cr.name, sp.name.to_lowercase())
    };
    app.modals.overlay = Some(crate::app::Overlay::Menu(ContextMenu {
        title: Some(title),
        items,
        at: Some(at),
        hover: 0,
        area: Rect::default(),
        parent: None,
        filter: None,
    }));
    app.chrome.dirty = true;
}

/// A critter menu row, chosen.
pub fn run(app: &mut App, act: CritterAct) {
    let Some(w) = app.critters.world.as_mut() else {
        return;
    };
    match act {
        CritterAct::Pet(id) => {
            w.pet(id);
        }
        CritterAct::Feed(id) => {
            w.feed(id);
        }
        CritterAct::Shoo(id) => {
            w.shoo(id);
        }
        CritterAct::Name(id) => {
            let now = w.get(id).map(|c| c.name.clone()).unwrap_or_default();
            let _ = now;
            crate::event_loop::open_prompt(app, crate::app::PromptKind::CritterName { id });
        }
        CritterAct::HideToday => {
            w.scatter();
            let today = local_now().date;
            update_log(&mut app.critters, |log| log.hidden_on = Some(today));
            app.chrome.flash = Some(
                "critters hidden until tomorrow (Settings → Appearance → Critters turns them off for good)".into(),
            );
        }
        CritterAct::Log => open_log(app),
        CritterAct::Settings => open_settings(app),
    }
    app.chrome.dirty = true;
}

/// One of the three CRITTERS rows of the settings overlay (Shift+L there
/// opens the log).
pub fn is_critters_row(tab: usize, row: usize) -> bool {
    use crate::config::SettingKind;
    crate::config::setting_at(tab, row).is_some_and(|s| {
        matches!(
            s.kind,
            SettingKind::Critters | SettingKind::CrittersSize | SettingKind::CrittersOften
        )
    })
}

/// Settings on the Appearance tab, cursor on the Critters row.
pub fn open_settings(app: &mut App) {
    use crate::config::{SettingKind, TabBody, SETTINGS_TABS};
    let at = SETTINGS_TABS
        .iter()
        .enumerate()
        .find_map(|(t, tab)| match tab.body {
            TabBody::Values(rows) | TabBody::Project(rows) => rows
                .iter()
                .position(|r| r.kind == SettingKind::Critters)
                .map(|i| (t, i)),
            _ => None,
        });
    if let Some((tab, row)) = at {
        app.modals.settings_tab = tab;
        if let Some(slot) = app.modals.settings_selected.get_mut(tab) {
            *slot = row;
        }
        app.modals.settings_on_tabs = false;
    }
    crate::event_loop::reopen_settings(app);
}

/// The Name… prompt's answer.
pub fn rename(app: &mut App, id: u32, name: &str) {
    if let Some(w) = app.critters.world.as_mut() {
        w.rename(id, name);
        let notes = w.drain_notes();
        let today = local_now().date;
        update_log(&mut app.critters, |log| {
            for n in &notes {
                log.record(n, &today);
            }
        });
    }
}

// ---------- the collection log ----------

/// The collection log overlay: every species, by where it's from; one
/// never seen is a silhouette.
#[derive(Debug, Clone, Default)]
pub struct LogView {
    pub scroll: u16,
    pub area: Rect,
    /// Opened from the settings overlay: Esc goes back there.
    pub from_settings: bool,
}

pub fn open_log(app: &mut App) {
    load_log(&mut app.critters);
    let from_settings = matches!(app.modals.overlay, Some(crate::app::Overlay::Settings(_)));
    app.modals.overlay = Some(crate::app::Overlay::CritterLog(LogView {
        from_settings,
        ..LogView::default()
    }));
    app.chrome.dirty = true;
}

/// Draws the log: a grid of tiles, each the species' idle frame in half
/// blocks (a soot silhouette until it has visited) over its name and
/// counts.
pub fn draw_log(buf: &mut Buffer, app: &App, view: &mut LogView, area: Rect) {
    let th = app.chrome.theme;
    let log = &app.critters.log;
    let w = area.width.saturating_sub(4).min(110);
    let h = area.height.saturating_sub(2).min(40);
    let x0 = area.x + (area.width - w) / 2;
    let y0 = area.y + (area.height - h) / 2;
    let rect = Rect::new(x0, y0, w, h);
    view.area = rect;
    ratatui::widgets::Widget::render(ratatui::widgets::Clear, rect, buf);
    let seen_n = SPECIES
        .iter()
        .filter(|s| log.species.contains_key(s.id))
        .count();
    let spawnable = SPECIES.iter().filter(|s| !s.has(flag::NO_SPAWN)).count();
    let title = format!(
        " Critter log  {seen_n}/{spawnable} seen  ·  {} pets ",
        log.species.values().map(|e| e.pets).sum::<u32>()
    );
    let block = ratatui::widgets::Block::bordered()
        .border_type(ratatui::widgets::BorderType::Plain)
        .border_style(Style::default().fg(th.dim))
        .title(title)
        .title_bottom(" ↑↓ scroll · Esc close ");
    let inner = block.inner(rect);
    ratatui::widgets::Widget::render(block, rect, buf);
    const TILE_W: u16 = 22;
    let tile_h: u16 = 10;
    let per_row = (inner.width / TILE_W).max(1);
    let mut lines: Vec<(Option<Origin>, Vec<usize>)> = Vec::new();
    for o in Origin::ALL {
        let ids: Vec<usize> = (0..SPECIES.len())
            .filter(|i| SPECIES[*i].origin == o && !SPECIES[*i].has(flag::NO_SPAWN))
            .collect();
        lines.push((Some(o), Vec::new()));
        for chunk in ids.chunks(usize::from(per_row)) {
            lines.push((None, chunk.to_vec()));
        }
    }
    let mut y = i32::from(inner.y) - i32::from(view.scroll);
    let bottom = i32::from(inner.y + inner.height);
    for (heading, row) in lines {
        let height = if heading.is_some() { 1 } else { tile_h };
        if y + i32::from(height) > i32::from(inner.y) && y < bottom {
            if let Some(o) = heading {
                if y >= i32::from(inner.y) {
                    buf.set_string(
                        inner.x + 1,
                        y as u16,
                        o.label(),
                        Style::default().fg(th.dim).add_modifier(Modifier::BOLD),
                    );
                }
            } else {
                for (k, i) in row.iter().enumerate() {
                    let tx = inner.x + k as u16 * TILE_W;
                    draw_tile(buf, app, *i, tx, y, TILE_W, tile_h, inner);
                }
            }
        }
        y += i32::from(height);
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_tile(buf: &mut Buffer, app: &App, i: usize, x: u16, y: i32, w: u16, h: u16, clip: Rect) {
    let th = app.chrome.theme;
    let sp = &SPECIES[i];
    let entry = app.critters.log.species.get(sp.id);
    let s = kete_critters::cast::sprite(i);
    let frame = s.frame(kete_critters::Pose::Idle, 0);
    let truecolor = app.critters.truecolor;
    let soot = to_color((0x3a, 0x34, 0x40), truecolor);
    // The sprite, bottom-centred in the tile above two text rows.
    let sprite_rows = (s.h as u16).div_ceil(2);
    let left = x + w.saturating_sub(s.w as u16) / 2;
    let top = y + i32::from(h.saturating_sub(2 + sprite_rows));
    for r in 0..sprite_rows {
        let cy = top + i32::from(r);
        if cy < i32::from(clip.y) || cy >= i32::from(clip.y + clip.height) {
            continue;
        }
        for cx in 0..s.w as u16 {
            let px = |yy: usize| -> Option<Color> {
                if yy >= s.h {
                    return None;
                }
                let v = frame[yy * s.w + usize::from(cx)];
                let rgb = s.color(v, false)?;
                Some(if entry.is_some() {
                    to_color((rgb[0], rgb[1], rgb[2]), truecolor)
                } else {
                    soot
                })
            };
            let (t, b) = (px(usize::from(r) * 2), px(usize::from(r) * 2 + 1));
            let pos = (left + cx, cy as u16);
            if pos.0 >= clip.x + clip.width {
                continue;
            }
            let Some(cell) = buf.cell_mut(pos) else {
                continue;
            };
            match (t, b) {
                (None, None) => {}
                (Some(t), Some(b)) => {
                    cell.set_char('▀').set_fg(t).set_bg(b);
                }
                (Some(t), None) => {
                    cell.set_char('▀').set_fg(t);
                }
                (None, Some(b)) => {
                    cell.set_char('▄').set_fg(b);
                }
            }
        }
    }
    let name_row = y + i32::from(h) - 2;
    let text = |row: i32, s: &str, style: Style, buf: &mut Buffer| {
        if row >= i32::from(clip.y) && row < i32::from(clip.y + clip.height) {
            let s: String = s.chars().take(usize::from(w) - 1).collect();
            buf.set_string(x + 1, row as u16, s, style);
        }
    };
    match entry {
        Some(e) => {
            let name = match &e.name {
                Some(n) => format!("{} ({n})", sp.name),
                None => sp.name.to_string(),
            };
            text(name_row, &name, Style::default().fg(th.text), buf);
            let mut counts = format!("×{}", e.seen);
            if e.pets > 0 {
                counts.push_str(&format!(" ♥{}", e.pets));
            }
            if let Some(d) = &e.first_seen {
                counts.push_str(&format!(" · {d}"));
            }
            text(name_row + 1, &counts, Style::default().fg(th.dim), buf);
        }
        None => {
            text(name_row, "???", Style::default().fg(th.dim), buf);
            let hint = if sp.has(flag::NOCTURNAL) {
                "comes out at night"
            } else if sp.weight <= 2.0 {
                "rare"
            } else {
                ""
            };
            text(name_row + 1, hint, Style::default().fg(th.dim), buf);
        }
    }
}

/// Keys over the log: arrows and the wheel scroll, Esc / `q` close.
pub fn log_key(app: &mut App, key: &KeyEvent) {
    let Some(crate::app::Overlay::CritterLog(view)) = app.modals.overlay.as_mut() else {
        return;
    };
    match key.code {
        KeyCode::Esc | KeyCode::Char('q') if view.from_settings => {
            crate::event_loop::reopen_settings(app)
        }
        KeyCode::Esc | KeyCode::Char('q') => app.modals.overlay = None,
        KeyCode::Down | KeyCode::Char('j') => view.scroll = view.scroll.saturating_add(2),
        KeyCode::Up | KeyCode::Char('k') => view.scroll = view.scroll.saturating_sub(2),
        KeyCode::PageDown => view.scroll = view.scroll.saturating_add(10),
        KeyCode::PageUp => view.scroll = view.scroll.saturating_sub(10),
        KeyCode::Home => view.scroll = 0,
        _ => {}
    }
    app.chrome.dirty = true;
}

/// The wheel over the log scrolls it.
pub fn log_mouse(app: &mut App, mouse: &MouseEvent) -> bool {
    let Some(crate::app::Overlay::CritterLog(view)) = app.modals.overlay.as_mut() else {
        return false;
    };
    match mouse.kind {
        MouseEventKind::ScrollDown => view.scroll = view.scroll.saturating_add(2),
        MouseEventKind::ScrollUp => view.scroll = view.scroll.saturating_sub(2),
        _ => return true,
    }
    app.chrome.dirty = true;
    true
}

#[cfg(not(test))]
fn log_path() -> PathBuf {
    nebula_core::paths::data_dir().join(kete_critters::log::LOG_FILE)
}

/// Tests: the override, else a temp file, never the user's data dir.
#[cfg(test)]
fn log_path() -> PathBuf {
    LOG_PATH.with(|p| p.borrow().clone()).unwrap_or_else(|| {
        std::env::temp_dir().join(format!("kete-test-critters-{}.json", std::process::id()))
    })
}

#[cfg(test)]
thread_local! {
    static LOG_PATH: std::cell::RefCell<Option<PathBuf>> = const { std::cell::RefCell::new(None) };
}

#[cfg(test)]
pub fn with_log_path<T>(path: PathBuf, f: impl FnOnce() -> T) -> T {
    LOG_PATH.with(|slot| {
        let prev = slot.replace(Some(path));
        let out = f();
        slot.replace(prev);
        out
    })
}

/// Reads the log (an unreadable file is an empty one: it's a tally).
fn load_log(c: &mut Critters) {
    c.log = std::fs::read_to_string(log_path())
        .ok()
        .and_then(|raw| serde_json::from_str(&raw).ok())
        .unwrap_or_default();
}

/// Changes the log: read the file fresh, apply, write it back, so what
/// the other app wrote meanwhile isn't lost.
fn update_log(c: &mut Critters, change: impl FnOnce(&mut CritterLog)) {
    load_log(c);
    change(&mut c.log);
    let path = log_path();
    if let Err(err) = crate::pr_cache::write_json_atomic(&path, &c.log) {
        tracing::warn!("failed to save {}: {err}", path.display());
    }
}

// ---------- the local clock ----------

pub struct LocalNow {
    pub hour: u8,
    /// `YYYY-MM-DD`.
    pub date: String,
}

/// The local hour and date (nocturnal visitors, "until tomorrow").
pub fn local_now() -> LocalNow {
    let (y, mo, d, h) = local_parts();
    LocalNow {
        hour: h,
        date: format!("{y:04}-{mo:02}-{d:02}"),
    }
}

#[cfg(windows)]
fn local_parts() -> (u16, u16, u16, u8) {
    use windows_sys::Win32::System::SystemInformation::GetLocalTime;
    // SAFETY: GetLocalTime writes the whole SYSTEMTIME it's given.
    let t = unsafe {
        let mut t = std::mem::zeroed();
        GetLocalTime(&mut t);
        t
    };
    (t.wYear, t.wMonth, t.wDay, t.wHour as u8)
}

#[cfg(unix)]
fn local_parts() -> (u16, u16, u16, u8) {
    // `struct tm`'s leading ints are the same on every unix; the tail
    // (gmtoff, zone) varies, so it's padded generously.
    #[repr(C)]
    #[allow(dead_code)] // only the date and hour are read
    struct Tm {
        sec: i32,
        min: i32,
        hour: i32,
        mday: i32,
        mon: i32,
        year: i32,
        wday: i32,
        yday: i32,
        isdst: i32,
        _rest: [u64; 8],
    }
    extern "C" {
        fn localtime_r(t: *const i64, out: *mut Tm) -> *mut Tm;
    }
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64);
    // SAFETY: localtime_r fills `tm` from `now` and touches nothing else.
    let mut tm: Tm = unsafe { std::mem::zeroed() };
    let ok = unsafe { !localtime_r(&now, &mut tm).is_null() };
    if !ok {
        return (1970, 1, 1, 12);
    }
    (
        (tm.year + 1900) as u16,
        (tm.mon + 1) as u16,
        tm.mday as u16,
        tm.hour as u8,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    const R: Rgb = (200, 40, 40);

    #[test]
    fn colors_fall_back_to_the_256_cube() {
        assert_eq!(to_color(R, true), Color::Rgb(200, 40, 40));
        assert!(matches!(to_color(R, false), Color::Indexed(i) if (16..232).contains(&i)));
        assert!(matches!(to_color((128, 128, 128), false), Color::Indexed(i) if i >= 232));
    }
}
