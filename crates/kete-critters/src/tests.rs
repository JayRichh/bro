//! Cast and world tests: the art is well formed and inside its limits, the
//! font covers every word the critters say, and the world is
//! deterministic, settles, stays on screen and honours the clock.

use std::collections::BTreeSet;

use crate::art::{self, Pose, MAX_H, MAX_W, MIN_H, MIN_W};
use crate::cast::{authored, cast, sprite};
use crate::font;
use crate::log::CritterLog;
use crate::paint::{paint, Canvas, Opts};
use crate::species::{
    by_id, flag, relation, Gait, Relation, Special, NAMES, PREFIXES, RELATIONS, SPECIES,
};
use crate::world::{self, AppEvent, Input, Mode, Note, Often, Pointer, World, STEP};

fn input() -> Input {
    Input {
        view_w: 160.0,
        view_h: 60.0,
        floor: 56.0,
        ..Input::default()
    }
}

/// Ticks `world` for `secs` at `fps`, calling `check` after every tick.
fn run(world: &mut World, input: &Input, secs: f64, fps: f64, mut check: impl FnMut(&World)) {
    let dt = 1.0 / fps;
    let n = (secs * fps).round() as usize;
    for _ in 0..n {
        world.tick(dt, input);
        check(world);
    }
}

// ---------- the cast ----------

#[test]
fn every_art_file_parses() {
    if let Err(e) = authored() {
        panic!("{e}");
    }
}

#[test]
fn every_species_has_art_and_every_drawing_a_species() {
    assert!(cast().missing.is_empty(), "no art for {:?}", cast().missing);
    let ids: BTreeSet<&str> = SPECIES.iter().map(|s| s.id).collect();
    assert_eq!(ids.len(), SPECIES.len(), "duplicate species id");
    for (file, a) in authored().unwrap() {
        assert!(
            ids.contains(a.id.as_str()),
            "art/{file}.txt draws {} which isn't in the cast",
            a.id
        );
    }
}

#[test]
fn authored_sizes_are_within_the_limits() {
    for (file, a) in authored().unwrap() {
        assert!(
            (MIN_W..=MAX_W).contains(&a.w) && (MIN_H..=MAX_H).contains(&a.h),
            "art/{file}.txt {} is {}x{}, limits {MIN_W}..={MAX_W} x {MIN_H}..={MAX_H}",
            a.id,
            a.w,
            a.h
        );
    }
}

#[test]
fn every_pixel_value_has_a_colour() {
    for (i, sp) in SPECIES.iter().enumerate() {
        let s = sprite(i);
        assert_eq!(s.poses.len(), Pose::ALL.len());
        for (p, frames) in s.poses.iter().enumerate() {
            assert!(
                !frames.is_empty(),
                "{} has no {:?} frame",
                sp.id,
                Pose::ALL[p]
            );
            for f in frames {
                assert_eq!(f.len(), s.w * s.h, "{} frame size", sp.id);
                for v in f {
                    assert!(
                        (*v as usize) <= s.palette.len(),
                        "{}: value {v} past the palette",
                        sp.id
                    );
                }
            }
        }
        for (from, to) in &s.blink {
            assert!((*from as usize) <= s.palette.len() && (*to as usize) <= s.palette.len());
        }
    }
}

#[test]
fn every_species_has_the_poses_its_gait_needs() {
    let blocks = authored().unwrap();
    for sp in SPECIES {
        let Some((file, a)) = blocks.iter().find(|(_, a)| a.id == sp.id) else {
            continue; // reported by the art test
        };
        let has = |p: Pose| !a.authored(p).is_empty();
        for p in [Pose::Idle, Pose::Sit, Pose::Eat, Pose::Held] {
            assert!(has(p), "art/{file}.txt {} has no {} frame", sp.id, p.name());
        }
        if sp.gait != Gait::Float {
            assert!(
                has(Pose::Walk),
                "art/{file}.txt {} walks but has no walk frame",
                sp.id
            );
        }
        if sp.has(flag::FLIER) || sp.gait == Gait::Fly {
            assert_eq!(
                a.authored(Pose::Fly).len(),
                2,
                "art/{file}.txt {} flies: two fly frames",
                sp.id
            );
        } else {
            assert!(
                !has(Pose::Fly),
                "art/{file}.txt {} has fly frames but isn't a flier",
                sp.id
            );
        }
        if a.has_eyes() {
            assert!(
                a.lid.is_some(),
                "art/{file}.txt {} has eyes and no lid",
                sp.id
            );
        }
        // Every frame stands on the bottom row (fly and held hang free).
        for (pose, rows) in &a.frames {
            if matches!(pose, Pose::Fly | Pose::Held)
                || sp.gait == Gait::Float
                || sp.gait == Gait::Fly
            {
                continue;
            }
            let bottom = rows.last().unwrap();
            assert!(
                bottom.iter().any(|c| *c != '.'),
                "art/{file}.txt {} {}: the bottom row is empty (feet go on the bottom row)",
                sp.id,
                pose.name()
            );
        }
    }
}

#[test]
fn bodies_are_mostly_inside_the_twitch_scale() {
    // The typical body is 10-16 wide and 6-12 tall; only a few big or
    // long ones go past it.
    let big: Vec<String> = authored()
        .unwrap()
        .into_iter()
        .filter(|(_, a)| a.w > 17 || a.h > 12)
        .map(|(_, a)| a.id)
        .collect();
    assert!(big.len() <= 8, "too many oversized critters: {big:?}");
}

#[test]
fn the_font_covers_everything_the_critters_say() {
    let mut words: Vec<String> = Vec::new();
    for s in SPECIES {
        words.push(s.name.into());
        words.extend(s.lines.iter().map(|l| l.to_string()));
        words.push(s.drop.into());
    }
    words.extend(NAMES.iter().map(|n| n.to_string()));
    words.extend(PREFIXES.iter().map(|n| n.to_string()));
    for w in world::WORDS {
        words.push(w.to_string());
    }
    for w in words {
        assert!(font::covers(&w), "the font can't draw {w:?}");
    }
}

#[test]
fn relations_name_real_species() {
    for (a, _, b) in RELATIONS {
        assert!(by_id(a).is_some() && by_id(b).is_some(), "{a} / {b}");
    }
    assert_eq!(relation("cat", "mouse"), Some(Relation::Chases));
    assert_eq!(relation("mouse", "cat"), Some(Relation::Fears));
}

#[test]
fn names_carry_their_macrons() {
    let name = |id: &str| SPECIES[by_id(id).unwrap()].name;
    assert_eq!(name("pukeko"), "Pūkeko");
    assert_eq!(name("tui"), "Tūī");
    assert_eq!(name("kakapo"), "Kākāpō");
    assert_eq!(name("takahe"), "Takahē");
    assert_eq!(name("korora"), "Kororā");
    assert_eq!(name("weta"), "Wētā");
    assert_eq!(name("piwakawaka"), "Pīwakawaka");
}

// ---------- the world ----------

fn fingerprint(w: &World) -> String {
    let mut s = format!(
        "t={} n={} p={}",
        w.steps,
        w.critters.len(),
        w.particles.len()
    );
    for c in &w.critters {
        s.push_str(&format!(
            " [{} {} {:.4} {:.4} {:?} {}]",
            c.id,
            c.species().id,
            c.x,
            c.lift,
            c.pose,
            c.name
        ));
    }
    s
}

#[test]
fn the_world_is_deterministic_from_a_seed() {
    let inp = input();
    let mut a = World::new(42);
    let mut b = World::new(42);
    a.often = Often::Busy;
    b.often = Often::Busy;
    run(&mut a, &inp, 240.0, 8.0, |_| {});
    run(&mut b, &inp, 240.0, 8.0, |_| {});
    assert_eq!(fingerprint(&a), fingerprint(&b));
    assert!(a.clock() > 239.0);
    // A different seed goes a different way.
    let mut c = World::new(43);
    c.often = Often::Busy;
    run(&mut c, &inp, 240.0, 8.0, |_| {});
    assert_ne!(fingerprint(&a), fingerprint(&c));
}

#[test]
fn frame_rate_does_not_change_the_simulation() {
    // 8 fps and 30 fps tick the same fixed steps.
    let inp = input();
    let mut a = World::new(9);
    let mut b = World::new(9);
    run(&mut a, &inp, 60.0, 8.0, |_| {});
    run(&mut b, &inp, 60.0, 1.0 / STEP, |_| {});
    assert_eq!(fingerprint(&a), fingerprint(&b));
}

#[test]
fn a_flung_critter_bounces_and_settles() {
    let inp = input();
    let mut w = World::new(1);
    w.spawning = false;
    let id = w.summon("cow", &inp).unwrap();
    run(&mut w, &inp, 8.0, 30.0, |_| {});
    let c = w.get(id).unwrap();
    let (cx, top) = (c.x, inp.floor - 6.0);
    assert!(w.grab(id, cx, top));
    // Drag it up and to the right fast, then let go.
    let mut held = inp.clone();
    for k in 0..8 {
        held.pointer = Some(Pointer {
            x: cx + f64::from(k) * 6.0,
            y: top - f64::from(k) * 4.0,
        });
        w.tick(STEP, &held);
        assert_eq!(w.get(id).unwrap().mode, Mode::Held);
        assert_eq!(w.get(id).unwrap().pose, Pose::Held);
    }
    w.release();
    assert_eq!(w.get(id).unwrap().mode, Mode::Air);
    let mut bounced = false;
    let mut last = f64::MAX;
    run(&mut w, &inp, 6.0, 30.0, |w| {
        if let Some(c) = w.get(id) {
            if c.lift > last + 0.01 && c.mode == Mode::Air {
                bounced = true;
            }
            last = c.lift;
            let hw = sprite(c.species).w as f64 / 2.0;
            assert!(
                c.x >= hw - 0.01 && c.x <= inp.view_w - hw + 0.01,
                "inside the walls"
            );
        }
    });
    let c = w.get(id).unwrap();
    assert!(bounced, "it bounced");
    assert_eq!(c.mode, Mode::Brain, "it settled");
    assert!(c.lift.abs() < 0.01);
}

#[test]
fn fliers_take_off_when_dropped() {
    let inp = input();
    let mut w = World::new(2);
    w.spawning = false;
    let id = w.summon("tui", &inp).unwrap();
    run(&mut w, &inp, 8.0, 30.0, |_| {});
    let c = w.get(id).unwrap();
    let (x, y) = (c.x, inp.floor - 4.0);
    w.grab(id, x, y);
    let mut held = inp.clone();
    held.pointer = Some(Pointer { x, y: y - 20.0 });
    w.tick(0.2, &held);
    w.release();
    assert_eq!(w.get(id).unwrap().mode, Mode::Brain);
    w.tick(STEP * 2.0, &inp);
    assert_eq!(w.get(id).unwrap().pose, Pose::Fly);
}

#[test]
fn critters_stay_on_screen_unless_coming_or_going() {
    let mut inp = input();
    inp.perches = vec![world::Rect {
        x: 20.0,
        y: 20.0,
        w: 60.0,
        h: 4.0,
    }];
    for seed in 0..4u32 {
        let mut w = World::new(seed);
        w.often = Often::Busy;
        let mut on_screen_steps = 0;
        run(&mut w, &inp, 900.0, 8.0, |w| {
            for c in &w.critters {
                if !c.entered || c.exiting || c.mode == Mode::Held {
                    continue;
                }
                on_screen_steps += 1;
                let hw = sprite(c.species).w as f64 / 2.0;
                assert!(
                    c.x >= hw * 0.5 && c.x <= inp.view_w - hw * 0.5,
                    "seed {seed}: {} at x {} (view {})",
                    c.species().id,
                    c.x,
                    inp.view_w
                );
                let digger =
                    c.species().special == Special::Dig || c.species().special == Special::Teleport;
                assert!(
                    c.lift > -0.5 || digger,
                    "seed {seed}: {} sank to {}",
                    c.species().id,
                    c.lift
                );
                assert!(
                    c.lift <= inp.floor,
                    "seed {seed}: {} above the top",
                    c.species().id
                );
            }
        });
        assert!(on_screen_steps > 0, "seed {seed}: somebody visited");
    }
}

#[test]
fn busier_settings_bring_more_visitors() {
    let inp = input();
    let seen = |often: Often| {
        let mut w = World::new(5);
        w.often = often;
        let mut n = 0;
        run(&mut w, &inp, 3600.0, 4.0, |_| {});
        for note in w.drain_notes() {
            if matches!(note, Note::Seen { .. }) {
                n += 1;
            }
        }
        n
    };
    let (rare, normal, busy) = (seen(Often::Rare), seen(Often::Normal), seen(Often::Busy));
    assert!(
        rare < normal && normal < busy,
        "rare {rare}, normal {normal}, busy {busy}"
    );
    assert!(rare >= 5, "rare still visits: {rare}");
}

#[test]
fn nocturnal_species_only_come_at_night() {
    let w = World::new(0);
    let day = w.spawn_weights(13, &[]);
    let night = w.spawn_weights(23, &[]);
    for (i, s) in SPECIES.iter().enumerate() {
        if s.has(flag::NOCTURNAL) {
            assert_eq!(day[i], 0.0, "{} by day", s.id);
            assert!(night[i] > 0.0, "{} at night", s.id);
        }
        if s.has(flag::NO_SPAWN) {
            assert_eq!(day[i] + night[i], 0.0, "{} never spawns", s.id);
        }
    }
    let tui = by_id("tui").unwrap();
    assert!(
        w.spawn_weights(7, &[])[tui] > w.spawn_weights(15, &[])[tui],
        "tūī like mornings"
    );
    // And a long day brings no night birds.
    let mut w = World::new(11);
    w.often = Often::Busy;
    let mut inp = input();
    inp.hour = 12;
    run(&mut w, &inp, 3600.0, 4.0, |_| {});
    for note in w.drain_notes() {
        if let Note::Seen { species } = note {
            let s = &SPECIES[by_id(species).unwrap()];
            assert!(!s.has(flag::NOCTURNAL), "{species} came by day");
        }
    }
}

#[test]
fn clicks_land_on_drawn_pixels_only() {
    let inp = input();
    let mut w = World::new(3);
    w.spawning = false;
    let id = w.summon("cow", &inp).unwrap();
    run(&mut w, &inp, 10.0, 30.0, |_| {});
    let c = w.get(id).unwrap();
    let s = sprite(c.species);
    // The middle of the body.
    let (x, y) = (c.x, inp.floor - c.lift - s.h as f64 / 2.0);
    assert_eq!(w.hit(x, y), Some(id));
    // Well above it, and the far corner of its box.
    assert_eq!(w.hit(x, y - 30.0), None);
    let (left, top, _, _) = w.bounds(id).unwrap();
    assert_eq!(
        w.hit(f64::from(left), f64::from(top)),
        None,
        "the box corner is transparent"
    );
}

#[test]
fn petting_feeding_and_naming() {
    let inp = input();
    let mut w = World::new(4);
    w.spawning = false;
    let id = w.summon("kiwi", &inp).unwrap();
    run(&mut w, &inp, 10.0, 30.0, |_| {});
    assert!(w.pet(id));
    assert!(w
        .particles
        .iter()
        .any(|p| p.kind == world::Kind::Icon(font::Icon::Heart)));
    run(&mut w, &inp, 3.0, 30.0, |_| {});
    assert!(w.feed(id));
    let mut ate = false;
    run(&mut w, &inp, 4.0, 30.0, |w| {
        ate |= w.get(id).is_some_and(|c| c.pose == Pose::Eat)
    });
    assert!(ate);
    assert!(w.rename(id, "Bob"));
    assert_eq!(w.get(id).unwrap().name, "Bob");
    let notes = w.drain_notes();
    let mut log = CritterLog::default();
    for n in &notes {
        log.record(n, "2026-10-16");
    }
    let e = &log.species["kiwi"];
    assert_eq!((e.seen, e.pets, e.fed), (1, 1, 1));
    assert_eq!(e.name.as_deref(), Some("Bob"));
    assert_eq!(e.first_seen.as_deref(), Some("2026-10-16"));
    // The next kiwi is Bob too.
    w.names = log.names();
    w.clear();
    let id = w.summon("kiwi", &inp).unwrap();
    assert_eq!(w.get(id).unwrap().name, "Bob");
}

#[test]
fn shoo_sends_it_off_screen() {
    let inp = input();
    let mut w = World::new(6);
    w.spawning = false;
    let id = w.summon("pig", &inp).unwrap();
    run(&mut w, &inp, 6.0, 30.0, |_| {});
    assert!(w.shoo(id));
    run(&mut w, &inp, 40.0, 30.0, |_| {});
    assert!(w.get(id).is_none(), "gone");
}

#[test]
fn the_cat_chases_the_mouse_off() {
    let inp = input();
    let mut w = World::new(8);
    w.spawning = false;
    let cat = w.summon("cat", &inp).unwrap();
    let mouse = w.summon("mouse", &inp).unwrap();
    run(&mut w, &inp, 10.0, 30.0, |_| {});
    w.meet_in = 1;
    w.tick(STEP, &inp);
    run(&mut w, &inp, 30.0, 30.0, |_| {});
    assert!(w.get(mouse).is_none(), "the mouse ran off");
    assert!(w.get(cat).is_some(), "the cat stayed");
}

#[test]
fn a_clicked_slime_splits_and_merges_back() {
    let inp = input();
    let mut w = World::new(10);
    w.spawning = false;
    let id = w.summon("slime", &inp).unwrap();
    run(&mut w, &inp, 8.0, 30.0, |_| {});
    assert!(w.pet(id));
    let minis: Vec<u32> = w
        .critters
        .iter()
        .filter(|c| c.species().id == "slime_mini")
        .map(|c| c.id)
        .collect();
    assert_eq!(minis.len(), 2);
    run(&mut w, &inp, 3.0, 30.0, |_| {});
    w.meet_in = 1;
    run(&mut w, &inp, 20.0, 30.0, |_| {});
    assert!(
        w.critters.iter().any(|c| c.species().id == "slime"),
        "merged back"
    );
    assert!(!w.critters.iter().any(|c| c.species().id == "slime_mini"));
}

#[test]
fn app_events_reach_the_critters() {
    let inp = input();
    let mut w = World::new(12);
    w.spawning = false;
    let adv = w.summon("adventurer", &inp).unwrap();
    run(&mut w, &inp, 8.0, 30.0, |_| {});
    let mut ev = inp.clone();
    ev.events = vec![AppEvent::TurnDone];
    w.tick(STEP, &ev);
    assert_eq!(
        w.get(adv).unwrap().bubble.as_ref().map(|b| b.text.as_str()),
        Some("gz")
    );
    ev.events = vec![AppEvent::PrMerged { x: Some(80.0) }];
    w.tick(STEP, &ev);
    assert!(w.particles.iter().any(|p| p.kind == world::Kind::Beam));
    ev.events = vec![AppEvent::NeedsYou { x: Some(10.0) }];
    w.tick(STEP * 3.0, &ev);
    assert!(w
        .particles
        .iter()
        .any(|p| p.kind == world::Kind::Icon(font::Icon::Bang)));
}

#[test]
fn late_and_idle_means_sleep() {
    let mut inp = input();
    inp.hour = 23;
    inp.all_idle = true;
    let mut w = World::new(13);
    w.spawning = false;
    w.summon("dog", &inp).unwrap();
    let mut asleep = 0;
    run(&mut w, &inp, 120.0, 8.0, |w| {
        if w.critters.iter().any(|c| c.pose == Pose::Sleep) {
            asleep += 1;
        }
    });
    assert!(asleep > 120, "slept for {} of 960 ticks", asleep);
}

#[test]
fn reduced_motion_makes_no_particles() {
    let mut inp = input();
    inp.reduced_motion = true;
    let mut w = World::new(14);
    w.often = Often::Busy;
    run(&mut w, &inp, 600.0, 8.0, |w| {
        assert!(
            w.particles.iter().all(|p| p.kind == world::Kind::Item),
            "no particles"
        );
        for c in &w.critters {
            assert!(c.pose != Pose::Walk, "{} walked", c.species().id);
        }
    });
}

#[test]
fn nothing_to_animate_between_visits() {
    let inp = input();
    let mut w = World::new(15);
    assert!(!w.active());
    let wake = w.next_wake().expect("a spawn timer");
    assert!(wake > 0.0 && wake < 10.0);
    w.tick(wake + STEP * 2.0, &inp);
    assert!(w.active(), "the visitor arrived on time");
    assert_eq!(w.next_wake(), None, "animating while someone's here");
    w.spawning = false;
    w.clear();
    assert_eq!(w.next_wake(), None, "spawning off: no timer at all");
}

#[test]
fn painting_puts_pixels_where_the_critters_are() {
    let inp = input();
    let mut w = World::new(16);
    w.spawning = false;
    let id = w.summon("cow", &inp).unwrap();
    run(&mut w, &inp, 8.0, 30.0, |_| {});
    let mut c = Canvas::new(inp.view_w as usize, inp.view_h as usize);
    paint(&w, &mut c, Opts::default());
    let (left, top, bw, bh) = w.bounds(id).unwrap();
    let mut inside = 0;
    for y in 0..c.h {
        for x in 0..c.w {
            if c.get(x, y)[3] > 0 {
                let ins = (x as i32) >= left
                    && (x as i32) < left + bw as i32
                    && (y as i32) >= top
                    && (y as i32) < top + bh as i32;
                if ins {
                    inside += 1;
                }
            }
        }
    }
    assert!(inside > 40, "the cow is drawn ({inside} px)");
    // Nothing is drawn on or below the floor line.
    for y in inp.floor as usize..c.h {
        for x in 0..c.w {
            assert_eq!(c.get(x, y)[3], 0, "pixel at {x},{y} is below the floor");
        }
    }
}

#[test]
fn art_errors_name_the_file_and_line() {
    let bad = "== x\nsize 5 4\npal b #808080\n-- idle\n.....\n.bbb.\n.bxb.\n.b.b.\n";
    let e = art::parse(bad).unwrap_err();
    assert_eq!(e.line, 7);
}

#[test]
fn the_balloon_only_drifts() {
    let inp = input();
    let mut w = World::new(17);
    w.spawning = false;
    let id = w.summon("balloon", &inp).unwrap();
    run(&mut w, &inp, 60.0, 8.0, |w| {
        if let Some(c) = w.get(id) {
            assert!(
                !matches!(c.pose, Pose::Sleep | Pose::Eat | Pose::Sit),
                "a balloon in {:?}",
                c.pose
            );
        }
        assert!(
            !w.particles
                .iter()
                .any(|p| matches!(p.kind, world::Kind::Icon(_))),
            "nobody emotes here"
        );
    });
}
