//! The flows, one test each.

use super::harness::*;

type Test = fn();

pub const TESTS: &[(&str, Test)] = &[("boots_to_the_empty_grid", boots_to_the_empty_grid)];

fn boots_to_the_empty_grid() {
    let tui = Tui::spawn(Options::default());
    tui.wait_for_text("create your first project");
}
