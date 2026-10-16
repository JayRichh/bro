//! Kete's critters: a small cast of pixel-art visitors that wander along the
//! bottom of the window, an easter egg after the stream overlay Jay wrote
//! for his Twitch scenes (`critters.html`), rewritten small and chunky so
//! the terminal and the desktop window show the same pixels.
//!
//! - [`art`]: the ASCII-map art format and the baker (outline, derived
//!   poses, blink).
//! - [`species`]: the cast table, relationships and names.
//! - [`cast`]: the baked sprites, loaded once.
//! - [`world`]: the deterministic world and the brain.
//! - [`paint`]: drawing the world into an RGBA buffer.
//! - [`font`]: the 3×5 pixel font and emote icons.
//! - [`log`]: the collection log both apps share.
//! - [`gen`] writes `desktop/src/lib/critters/cast.gen.ts`.

pub mod art;
pub mod cast;
pub mod font;
pub mod gen;
pub mod log;
pub mod paint;
pub mod rng;
pub mod species;
pub mod world;

#[cfg(test)]
mod tests;

pub use art::{Pose, Rgb};
pub use log::CritterLog;
pub use rng::Rng;
pub use species::{Species, SPECIES};
pub use world::{AppEvent, Input, Note, Often, Pointer, Rect, World};
