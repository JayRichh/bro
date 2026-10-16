//! Kete's user settings as data: the `config.json` model both front ends
//! read and write ([`Config`]), the settings overlay's tabs and rows
//! ([`SETTINGS_TABS`]), the words its choice rows store ([`choices`]), the
//! harness model catalogues, the rebindable actions ([`actions`]), and the
//! desktop window's export of all of it ([`schema`], [`desktop`]). It also
//! reads VS Code's settings and themes ([`vscode`], through the JSONC
//! reader [`jsonc`]), which both apps can wear, and maps a theme onto the
//! window's colour tokens ([`look`]).
//!
//! Nothing here draws: the TUI keeps its palettes, chord parsing and
//! renderers, and reads the rest from this crate, so the desktop window can
//! depend on it without a terminal backend.

pub mod actions;
pub mod choices;
pub mod claude_catalogue;
mod config;
pub mod cursor_catalogue;
pub mod desktop;
pub mod jsonc;
pub mod learn;
pub mod look;
pub mod presets;
pub mod schema;
pub mod vscode;

pub use config::*;
