# Critters: the data contract

`cast.gen.ts` is generated from `crates/kete-critters` (Rust). Don't edit it: change the crate and run
`KETE_BLESS=1 cargo test -p kete-critters cast_gen`. The crate is the one source for the art, the tables and the
behaviour. The TUI runs its `World` directly. The desktop runs a TypeScript port of the same `World` against this file.

To see the cast, run `cargo run -p kete-critters --example sheet -- out.png`. That writes every species in every pose,
at 4x and 1x. `--example scene -- out.png --summon kiwi,cat` writes a filmstrip of the world running.

## Units and coordinates

- Everything is in **sprite pixels** and **seconds**. The overlay's view is `viewW x viewH` sprite px. On the desktop that
  is the window size divided by the scale.
- **Scale.** `critters_size` small/medium/large means 2/3/4 CSS px per sprite px. Multiply by `devicePixelRatio` and round
  to a whole number of device pixels, so the pixels stay crisp. Set `image-rendering: pixelated`.
- **Floor.** `floor` is the y of the floor line: the top edge of the status bar. Critters stand on it.
- **Position.** A critter's position is `x` (its centre) and `lift` (its feet's height above the floor, negative while
  sinking into it). Particles use `x` and `rise` the same way.
  - On screen, the feet are at `y = floor - lift`.
  - A sprite's top-left is `(round(x - w/2), round(floor - lift) - h)`. `w` and `h` are the baked size.
- **Clipping.** Sprite rows at or below `floor` are clipped. That is how stairs and digging go "through" the floor.
  Particles aren't clipped.

## Art (`SPECIES[i].sprite`)

- **Frames.** `frames[pose]` holds 1 or 2 frames for every pose in `POSES`.
  - `walk`, `held` and `fly` have 2.
  - Non-fliers' `fly` holds copies of `walk`. They never fly.
- **Decoding.** `decodeFrame(s)` gives `w*h` pixel values, row-major.
  - `0` is transparent.
  - `v >= 1` is `palette[v - 1]`.
  - `palette[0]` is the outline, which the baker already added: 1 px of soot around every opaque pixel.
- **Facing.** Frames face **right**. Mirror horizontally when a critter faces left (`dir < 0`).
- **Blink.** Apply `blink`'s `[from, to]` value swaps while the eyes are closed:
  - for a 0.14 s blink every 2.5 to 6 s
  - always for the `sleep` pose. Its frames already have the swap applied, so applying it again does nothing.
- **Flash.** Draw every opaque non-outline pixel in `INK.flash` (the boom-bug's fuse).
- **Ghosts.** A `ghostly` species draws at 80% of its alpha.
- **Squash.** For `SQUASH_TIME` after a landing, draw the `squash` pose instead of the current pose.
- **Caching.** Build each frame once into an offscreen canvas (or an `ImageBitmap`) per (species, pose, frame, closed),
  then `drawImage` it at the integer scale.

## The brain: port `crates/kete-critters/src/world.rs`

Port it function for function. The Rust is written to be ported: no hash maps, plain loops, every timer in whole
steps. The pieces:

### Time

- `World.tick(dt, input)`. Add `round(dt * 1e6)` to a microsecond accumulator.
- **Idle skip.** If nothing is on screen and no event is pending, skip whole steps up to one before the next visitor.
- **Cap.** Clamp the accumulator to `MAX_TICK_US`, then run `step()` every `STEP_US` (1/32 s).
- **Events.** App events queue in `pending` and run at the start of the next step. A 60 fps frame shorter than a step
  must not lose them.
- **Hover.** `hover` (the critter under the pointer) is recomputed each tick with `hit()`.

### One step

- `step()` runs `stage()` (spawns, the second visitor, meetings), then `update()` for each critter with a snapshot of
  the others, then applies the effects, removes the gone, and runs `tick_particles()`.
- **Brain.** Each critter runs a queue of acts (`Act` in Rust). `run()` advances the current one and says when it is
  done. With nothing queued:
  - `next_acts()` picks a state with `next_weights()` (`NEXT[prev]`, times `MOODS[personality]`, with the gait, no-lines,
    pet-rock and late-night adjustments) and turns it into acts. `special()` gives the species move.
  - `exit_plan()` takes over once `age > budget`.
- **Physics.** Picked-up critters use `held()` (they follow the pointer and smooth its velocity for the fling). Dropped
  or flung ones use `air()` (gravity, wall bounces, perch catches, the floor bounce and settle). The constants are in
  `PHYSICS`.
- **Commands** (the mouse and the critter menu), with their Rust names:

  | Rust | What it does |
  |---|---|
  | `summon(id)` | the kete-mark kiwi, the debug hook |
  | `pet(id)` | click |
  | `feed(id)` | double-click, or Feed |
  | `shoo(id)` | Shoo |
  | `rename(id, name)` | Name… |
  | `scatter()` | Hide critters until tomorrow |
  | `resume()` | bring them back |
  | `grab(id, x, y)` | pointer down on a critter |
  | `release()` | pointer up |
  | `hit(x, y)` | the critter whose drawn pixel (outline included, mirroring respected) is under the point |
  | `bounds(id)` | the hit-box rect |
  | `sprites()` | the draw list, back to front |
  | `next_wake()` | seconds until the next visitor while idle: one `setTimeout`, no rAF |
  | `active()` | anything on screen: run rAF |
  | `drain_notes()` | events for the collection log |

### Matching Rust exactly

- **RNG.** `hashStr` and `mulberry32` exactly as in the stream overlay. `RNG_CHECK` pins the first draws.
  - `Rng.range(a, b) = a + (b - a) * next()`
  - `chance(p) = next() < p`
  - `index(n) = min(floor(next() * n), n - 1)`
  - `sign() = chance(0.5) ? 1 : -1`
  - `weighted(w)` skips zero weights and falls back to the last positive one.
  - The **order of draws** must match the Rust line for line.
- **Rounding.** Rust's `f64::round` rounds halves away from zero. Use `Math.sign(v) * Math.round(Math.abs(v))`, not
  `Math.round`, which rounds -2.5 to -2.
- **Truncation.** `as usize` on a non-negative float is `Math.floor`. Rust's `x.fract()` is `x - Math.trunc(x)`.
- **No transcendentals.** The brain uses no `sin`, `cos` or `sqrt`, whose last bit differs between C runtimes and
  JavaScript engines. Arcs are `arch(u) = 4u(1 - u)`, bobs are `wave(t)` (a triangle wave), and the smoke puff uses
  the `RING` table. Port those three from `world.rs` as they are, and don't swap in `Math.sin`.
- **Sign.** Rust's `signum(0.0)` is 1 where `Math.sign(0)` is 0. The brain only calls it on non-zero values (each call
  is guarded), so a straight `Math.sign` is safe there.
- **Ties.** `min_by` keeps the first of equal minimums (the critter nearest an event, the item nearest the wisp).
- **`steps(secs)`** is `max(1, round(secs / STEP))`.

## Painting: `crates/kete-critters/src/paint.rs`

Paint in this order:

1. On the floor: trails, lying items and the loot beam.
2. Sprites, back to front, held critter last.
3. Dust, sparkles, crumbs, confetti, smoke, darts, emote icons and text pops.
4. Speech bubbles.
5. The hovered critter's name tag, shown when it has no bubble.

Text uses `GLYPHS`, the 3x5 capital font. Lower case maps to capitals. A macron vowel draws its capital with a bar 2 rows
above the cap, so a line holding one is 7 px tall.

- **Text pops** have a 1 px soot outline.
- **Bubbles**:
  - a bone fill with a soot border
  - corners cut, 2 px of padding
  - a 1x2 tail down to the head
  - kept inside the view.
- **Icons**: `ICONS`, drawn with a soot outline.

No web fonts: the CSP blocks them.

## Collection log: `<data>/critters.json`

It is machine-local and never exported. The TUI and the desktop share the file. The shape is `CritterLog` in
`crates/kete-critters/src/log.rs`:

```json
{ "version": 1,
  "species": { "kiwi": { "seen": 3, "pets": 5, "fed": 1, "name": "Bob", "first_seen": "2026-10-16" } },
  "lamps": 0,
  "hidden_on": "2026-10-16" }
```

- **Defaults.** Every field defaults. An unreadable file is an empty log.
- **Read, apply, write.** Never hold the log as the source of truth: both apps, and every terminal window, write the
  file. Each change reads the file fresh, applies the change and writes it back (the TUI's `update_log`). Read
  `hidden_on` from disk when deciding whether visitors may come, so hiding them in one app hides them in the other.
- **Updating.** `record(note, today)` folds a `Note` in:
  - `Seen` (seen, and first_seen if missing)
  - `Petted`
  - `Fed`
  - `Named` (sets name)
  - `Lamp`
- **Names.** A species with a name gives that name to its next visitor: load `names()` into `World.names`.
- **Hiding.** `hidden_on` is the local date "Hide critters until tomorrow" was chosen. No visitors come while today
  equals it.

## Scenarios to mirror in vitest

The Rust tests in `crates/kete-critters/src/tests.rs`:

- determinism from a seed
- 8 fps and 30 fps agree
- a flung critter bounces and settles
- fliers take off when dropped
- critters stay on screen unless arriving or leaving
- busier settings bring more visitors
- nocturnal gating, and tūī prefer mornings
- hits land only on drawn pixels
- pet, feed and name, and the log
- shoo
- the cat chases the mouse off
- the slime splits and merges back
- app events
- late and idle means sleep
- reduced motion makes no particles
- nothing animates between visits

`GOLDEN` is a seeded five-minute run. Checkpoints every 10 s give the cast as `[species, x, lift, pose]`. Your port
should reproduce it to two decimals.
