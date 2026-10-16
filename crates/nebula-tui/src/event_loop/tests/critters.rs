//! The CRITTERS overlay in the terminal: drawn over the frame in half
//! blocks without disturbing what isn't covered, the mouse going to a
//! critter only where one is drawn, and no clock at all between visits.

use super::*;
use kete_critters::world::{World, STEP};

/// Critters on, a quiet world (no visitors of its own) laid out for a
/// `w`x`h` screen by one draw.
fn critters_app(w: u16, h: u16) -> (App, Terminal<TestBackend>) {
    let mut app = App::new();
    // The grid, which holds still (the empty app's splash animates).
    seed_tree(&mut app);
    crate::critters::apply(&mut app, true, "medium", "normal");
    let world = app.critters.world.as_mut().expect("critters on");
    *world = World::new(3);
    world.spawning = false;
    let mut terminal = Terminal::new(TestBackend::new(w, h)).unwrap();
    terminal.draw(|f| ui::draw(f, &mut app)).unwrap();
    (app, terminal)
}

/// Brings `species` on and lets it settle somewhere on the floor.
fn visit(app: &mut App, species: &str, secs: f64) -> u32 {
    let input = crate::critters::input_of(&app.critters, 12);
    let world = app.critters.world.as_mut().unwrap();
    let id = world.summon(species, &input).expect("a species");
    let mut t = 0.0;
    while t < secs {
        world.tick(STEP, &input);
        t += STEP;
    }
    // Hold it still where it is for the draw.
    id
}

fn redraw(app: &mut App, terminal: &mut Terminal<TestBackend>) -> ratatui::buffer::Buffer {
    terminal.draw(|f| ui::draw(f, app)).unwrap();
    terminal.backend().buffer().clone()
}

/// The cells a critter's sprite covers (its drawn box, in cells).
fn cells_of(app: &App, id: u32) -> (u16, u16, u16, u16) {
    let w = app.critters.world.as_ref().unwrap();
    let (left, top, bw, bh) = w.bounds(id).unwrap();
    let top = top.max(0) as u16;
    let bottom = (top as i32 + bh as i32 - 1).max(0) as u16;
    (
        left.max(0) as u16,
        top / 2,
        (left + bw as i32 - 1).max(0) as u16,
        bottom / 2,
    )
}

#[test]
fn a_critter_draws_over_the_frame_and_leaves_the_rest_alone() {
    for (w, h) in [(80u16, 24u16), (120, 40)] {
        let (mut app, mut terminal) = critters_app(w, h);
        let before = redraw(&mut app, &mut terminal);
        let id = visit(&mut app, "cow", 12.0);
        let after = redraw(&mut app, &mut terminal);
        let (x0, y0, x1, y1) = cells_of(&app, id);
        let mut blocks = 0;
        for y in 0..h {
            for x in 0..w {
                let inside = x >= x0 && x <= x1 && y >= y0 && y <= y1;
                let (b, a) = (&before[(x, y)], &after[(x, y)]);
                if inside {
                    if matches!(a.symbol(), "▀" | "▄") {
                        blocks += 1;
                    }
                } else if y >= y0 {
                    // Beside and under the critter nothing changed (above
                    // it a bubble or an emote may float).
                    assert_eq!(a, b, "{w}x{h}: cell {x},{y} outside the critter changed");
                }
            }
        }
        assert!(
            blocks > 10,
            "{w}x{h}: the cow is drawn in half blocks ({blocks})"
        );
        // It stands on the row above the status bar: the bar is untouched.
        for x in 0..w {
            assert_eq!(
                after[(x, h - 1)],
                before[(x, h - 1)],
                "{w}x{h}: the bar at {x}"
            );
        }
        assert!(
            y1 <= h - 2,
            "{w}x{h}: feet on the floor row, not in the bar"
        );
    }
}

#[test]
fn a_half_covered_cell_keeps_the_background_in_its_other_half() {
    let (mut app, mut terminal) = critters_app(80, 24);
    let before = redraw(&mut app, &mut terminal);
    let id = visit(&mut app, "kiwi", 12.0);
    let after = redraw(&mut app, &mut terminal);
    let (x0, y0, x1, y1) = cells_of(&app, id);
    let mut halves = 0;
    for y in y0..=y1 {
        for x in x0..=x1 {
            let (b, a) = (&before[(x, y)], &after[(x, y)]);
            match a.symbol() {
                // One half drawn: the cell's own background shows in the
                // other.
                "▀" | "▄" if a.bg == b.bg => halves += 1,
                "▀" | "▄" => {}
                // A cell the sprite's box covers but no pixel of it: as was.
                _ => assert_eq!(a, b, "cell {x},{y}"),
            }
        }
    }
    assert!(halves > 0, "the outline's edge cells are half drawn");
}

#[test]
fn clicks_go_to_a_critter_only_where_it_is_drawn() {
    let (mut app, mut terminal) = critters_app(80, 24);
    let id = visit(&mut app, "cow", 12.0);
    redraw(&mut app, &mut terminal);
    let (x0, y0, x1, y1) = cells_of(&app, id);
    // A cell the cow is drawn in.
    let (cx, cy) = (x0..=x1)
        .flat_map(|x| (y0..=y1).map(move |y| (x, y)))
        .find(|(x, y)| crate::critters::critter_at(&app, *x, *y) == Some(id))
        .expect("a drawn cell");
    let mut out = Vec::new();
    let particles = app.critters.world.as_ref().unwrap().particles.len();
    handle_mouse(
        &mut app,
        mev(MouseEventKind::Down(MouseButton::Left), cx, cy),
        &mut out,
    );
    handle_mouse(
        &mut app,
        mev(MouseEventKind::Up(MouseButton::Left), cx, cy),
        &mut out,
    );
    assert!(out.is_empty(), "the click went nowhere else: {out:?}");
    let world = app.critters.world.as_ref().unwrap();
    assert!(
        world.particles.len() > particles,
        "petted: a heart floats up"
    );
    // Well away from it the critters take nothing.
    let away = if x0 > 20 { 2 } else { 70 };
    assert_eq!(crate::critters::critter_at(&app, away, cy), None);
    assert!(!crate::critters::handle_mouse(
        &mut app,
        &mev(MouseEventKind::Down(MouseButton::Left), away, cy)
    ));
    // Right-click opens its menu, rows for this critter.
    handle_mouse(
        &mut app,
        mev(MouseEventKind::Down(MouseButton::Right), cx, cy),
        &mut out,
    );
    let Some(Overlay::Menu(menu)) = &app.modals.overlay else {
        panic!("no critter menu");
    };
    let labels: Vec<&str> = menu.items.iter().map(|i| i.label.as_str()).collect();
    for want in [
        "Pet",
        "Feed",
        "Name\u{2026}",
        "Shoo",
        "Hide critters until tomorrow",
        "Critter log\u{2026}",
        "Critter settings\u{2026}",
    ] {
        assert!(labels.contains(&want), "{want} in {labels:?}");
    }
}

#[test]
fn a_drag_carries_a_critter_and_drops_it() {
    let (mut app, mut terminal) = critters_app(80, 24);
    let id = visit(&mut app, "pig", 12.0);
    redraw(&mut app, &mut terminal);
    let (x0, y0, x1, y1) = cells_of(&app, id);
    let (cx, cy) = (x0..=x1)
        .flat_map(|x| (y0..=y1).map(move |y| (x, y)))
        .find(|(x, y)| crate::critters::critter_at(&app, *x, *y) == Some(id))
        .unwrap();
    let mut out = Vec::new();
    handle_mouse(
        &mut app,
        mev(MouseEventKind::Down(MouseButton::Left), cx, cy),
        &mut out,
    );
    handle_mouse(
        &mut app,
        mev(MouseEventKind::Drag(MouseButton::Left), cx + 3, cy - 6),
        &mut out,
    );
    let input = crate::critters::input_of(&app.critters, 12);
    app.critters
        .world
        .as_mut()
        .unwrap()
        .tick(STEP * 2.0, &input);
    let held = app.critters.world.as_ref().unwrap().get(id).unwrap();
    assert_eq!(held.mode, kete_critters::world::Mode::Held);
    assert!(held.lift > 4.0, "lifted off the floor");
    handle_mouse(
        &mut app,
        mev(MouseEventKind::Up(MouseButton::Left), cx + 3, cy - 6),
        &mut out,
    );
    let input = crate::critters::input_of(&app.critters, 12);
    let world = app.critters.world.as_mut().unwrap();
    for _ in 0..200 {
        world.tick(STEP, &input);
    }
    let c = world.get(id).unwrap();
    assert_eq!(c.mode, kete_critters::world::Mode::Brain, "landed");
    assert!(c.lift.abs() < 0.01);
    assert!(out.is_empty());
}

#[test]
fn between_visits_there_is_one_timer_and_no_frames() {
    let mut app = App::new();
    assert!(!crate::critters::armed(&app), "off: nothing at all");
    crate::critters::apply(&mut app, true, "medium", "normal");
    let world = app.critters.world.as_ref().unwrap();
    assert!(!world.active());
    assert!(crate::critters::armed(&app), "a wake for the first visitor");
    let wait = crate::critters::deadline(&app).saturating_duration_since(std::time::Instant::now());
    assert!(
        wait > crate::critters::FRAME * 4,
        "not a frame tick: {wait:?}"
    );
    // Out of focus: nothing runs.
    app.chrome.window_focused = false;
    assert!(!crate::critters::armed(&app));
    app.chrome.window_focused = true;
    // Animations off: the critters go.
    app.chrome.animations = false;
    crate::critters::apply(&mut app, true, "medium", "normal");
    assert!(app.critters.world.is_none());
    assert!(!crate::critters::armed(&app));
    // With one on screen, it ticks at the frame rate.
    app.chrome.animations = true;
    let (mut app, _t) = critters_app(80, 24);
    visit(&mut app, "kiwi", 1.0);
    let wait = crate::critters::deadline(&app).saturating_duration_since(std::time::Instant::now());
    assert!(wait <= crate::critters::FRAME);
}

#[test]
fn the_konami_code_turns_them_on_and_off() {
    let dir = tempfile::tempdir().unwrap();
    crate::config::with_config_path(dir.path().join("config.json"), || {
        crate::critters::with_log_path(dir.path().join("critters.json"), || {
            let mut app = App::new();
            let mut out = Vec::new();
            let press = |app: &mut App, code: KeyCode, out: &mut Vec<ClientRequest>| {
                handle_key(app, KeyEvent::new(code, KeyModifiers::NONE), out);
            };
            for code in [
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
            ] {
                press(&mut app, code, &mut out);
            }
            assert!(app.critters.world.is_some(), "on");
            assert!(crate::config::Config::load().critters, "saved");
            let flash = app.chrome.flash.clone().unwrap_or_default();
            assert!(flash.contains("turns them off"), "{flash}");
            // A wrong key part way resets it.
            for code in [KeyCode::Up, KeyCode::Up, KeyCode::Char('x')] {
                press(&mut app, code, &mut out);
            }
            assert!(app.critters.world.is_some(), "still on");
        });
    });
}

#[test]
fn the_log_shows_silhouettes_and_seen_species() {
    let dir = tempfile::tempdir().unwrap();
    crate::critters::with_log_path(dir.path().join("critters.json"), || {
        let mut app = App::new();
        seed_tree(&mut app);
        // What the other app (or an earlier visit) wrote: the log reads
        // the file when it opens.
        let mut log = kete_critters::CritterLog::default();
        log.record(&kete_critters::Note::Seen { species: "kiwi" }, "2026-10-16");
        log.record(
            &kete_critters::Note::Petted { species: "kiwi" },
            "2026-10-16",
        );
        std::fs::write(
            dir.path().join("critters.json"),
            serde_json::to_string(&log).unwrap(),
        )
        .unwrap();
        crate::critters::apply(&mut app, true, "medium", "normal");
        crate::critters::open_log(&mut app);
        for (w, h) in [(80u16, 24u16), (120, 40)] {
            let mut terminal = Terminal::new(TestBackend::new(w, h)).unwrap();
            terminal.draw(|f| ui::draw(f, &mut app)).unwrap();
            let text = buffer_text(&terminal);
            if std::env::var_os("KETE_DUMP").is_some() {
                eprintln!("{text}");
            }
            assert!(text.contains("Critter log"), "{w}x{h}");
            assert!(text.contains("Kiwi"), "the seen kiwi is named at {w}x{h}");
            assert!(
                text.contains("???"),
                "the unseen are silhouettes at {w}x{h}"
            );
        }
        let mut out = Vec::new();
        handle_key(
            &mut app,
            KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE),
            &mut out,
        );
        assert!(app.modals.overlay.is_none());
    });
}

#[test]
fn the_log_keeps_what_the_other_app_wrote() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("critters.json");
    crate::critters::with_log_path(path.clone(), || {
        let (mut app, _t) = critters_app(80, 24);
        let id = visit(&mut app, "kiwi", 2.0);
        // Meanwhile the desktop window logged three kiwi and a cat.
        let mut theirs = kete_critters::CritterLog::default();
        for _ in 0..3 {
            theirs.record(&kete_critters::Note::Seen { species: "kiwi" }, "2026-10-01");
        }
        theirs.record(&kete_critters::Note::Seen { species: "cat" }, "2026-10-02");
        std::fs::write(&path, serde_json::to_string(&theirs).unwrap()).unwrap();
        crate::critters::rename(&mut app, id, "Bob");
        let log: kete_critters::CritterLog =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(log.species["kiwi"].seen, 4, "their three and this visit");
        assert_eq!(log.species["kiwi"].name.as_deref(), Some("Bob"));
        assert!(log.species.contains_key("cat"));
    });
}

/// `KETE_DUMP_CELLS=<dir>` writes each size's frame as JSON cells (symbol,
/// fg, bg) for eyeballing the three critter sizes outside a terminal.
#[test]
fn dump_cells_for_review() {
    let Some(dir) = std::env::var_os("KETE_DUMP_CELLS") else {
        return;
    };
    for size in ["small", "medium", "large"] {
        let (mut app, mut terminal) = critters_app(100, 30);
        crate::critters::apply(&mut app, false, size, "normal");
        crate::critters::apply(&mut app, true, size, "normal");
        let world = app.critters.world.as_mut().unwrap();
        *world = World::new(5);
        world.spawning = false;
        redraw(&mut app, &mut terminal);
        for (i, sp) in ["kiwi", "cow", "pukeko", "cat"].iter().enumerate() {
            let input = crate::critters::input_of(&app.critters, 12);
            let world = app.critters.world.as_mut().unwrap();
            let id = world.summon(sp, &input).unwrap();
            for _ in 0..400 {
                world.tick(STEP, &input);
            }
            let _ = (i, id);
        }
        let buf = redraw(&mut app, &mut terminal);
        let mut cells = Vec::new();
        for y in 0..buf.area.height {
            for x in 0..buf.area.width {
                let c = &buf[(x, y)];
                cells.push(format!(
                    "[{},{},{:?},{:?},{:?}]",
                    x,
                    y,
                    c.symbol(),
                    format!("{:?}", c.fg),
                    format!("{:?}", c.bg)
                ));
            }
        }
        let path = std::path::Path::new(&dir).join(format!("cells-{size}.json"));
        std::fs::write(path, format!("[{}]", cells.join(","))).unwrap();
    }
}
