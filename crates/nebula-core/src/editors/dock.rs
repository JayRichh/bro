//! Dock's arithmetic: which half of the monitor each window takes, and the
//! outer rectangles that put their visible frames there. Pure, so the
//! tests cover it; the Win32 calls that read and move the windows are in
//! `win.rs`.
//!
//! On Windows 10 and 11 a window's outer rectangle (`GetWindowRect`) is
//! wider than what is drawn by its invisible resize borders; the drawn
//! frame is `DWMWA_EXTENDED_FRAME_BOUNDS`. Each window's borders are added
//! back to its target, so the two frames meet with no gap and no overlap.

use serde::Serialize;

/// A rectangle in screen pixels: right and bottom exclusive.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct Rect {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

impl Rect {
    pub fn new(left: i32, top: i32, right: i32, bottom: i32) -> Self {
        Self {
            left,
            top,
            right,
            bottom,
        }
    }

    pub fn width(&self) -> i32 {
        self.right - self.left
    }

    pub fn height(&self) -> i32 {
        self.bottom - self.top
    }

    fn center_x(&self) -> i32 {
        self.left + self.width() / 2
    }
}

/// How far a window's outer rectangle reaches past its drawn frame on
/// each side (the invisible borders): outer minus frame, never negative.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Insets {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

impl Insets {
    /// From a window's outer rectangle and its drawn frame.
    pub fn between(outer: Rect, frame: Rect) -> Self {
        Self {
            left: (frame.left - outer.left).max(0),
            top: (frame.top - outer.top).max(0),
            right: (outer.right - frame.right).max(0),
            bottom: (outer.bottom - frame.bottom).max(0),
        }
    }

    /// The outer rectangle that draws the frame at `frame`.
    pub fn grow(&self, frame: Rect) -> Rect {
        Rect::new(
            frame.left - self.left,
            frame.top - self.top,
            frame.right + self.right,
            frame.bottom + self.bottom,
        )
    }
}

/// Which half a window takes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum Side {
    Left,
    Right,
}

/// Where Dock puts the two windows: outer rectangles for `SetWindowPos`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Plan {
    /// The half Kete keeps.
    pub kete_side: Side,
    pub kete: Rect,
    pub editor: Rect,
}

/// The two halves of `work` (the monitor's work area, the taskbar left
/// out), left then right; an odd pixel goes to the right.
pub fn halves(work: Rect) -> (Rect, Rect) {
    let mid = work.left + work.width() / 2;
    (
        Rect::new(work.left, work.top, mid, work.bottom),
        Rect::new(mid, work.top, work.right, work.bottom),
    )
}

/// Dock: Kete keeps the half its window is mostly on (`kete_frame`, its
/// drawn frame now), the editor takes the other, each outer rectangle
/// grown by that window's own invisible borders.
pub fn plan(work: Rect, kete_frame: Rect, kete_insets: Insets, editor_insets: Insets) -> Plan {
    let (left, right) = halves(work);
    let kete_side = if kete_frame.center_x() < right.left {
        Side::Left
    } else {
        Side::Right
    };
    let (kete, editor) = match kete_side {
        Side::Left => (left, right),
        Side::Right => (right, left),
    };
    Plan {
        kete_side,
        kete: kete_insets.grow(kete),
        editor: editor_insets.grow(editor),
    }
}
