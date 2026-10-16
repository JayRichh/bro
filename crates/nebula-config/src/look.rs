//! THE WINDOW'S LOOK FROM A VS CODE THEME: the colour contract VS Code
//! themes are written to (`editor.background`, `foreground`,
//! `focusBorder`, `list.activeSelectionBackground`, `terminal.ansi*`, the
//! token colours…) mapped onto the desktop window's CSS tokens
//! (`desktop/src/theme.css`), then held to the rules Kete's own palettes
//! keep. The window paints the result as inline variables on `<html>`;
//! an imported theme is stored as this result, never as the theme file.
//!
//! Where a theme leaves a token unsaid the value is made from what it did
//! say — surfaces and rules from its background and foreground, statuses
//! from its git and terminal colours, else its syntax colours — and the
//! terminal's ANSI colours fall back to VS Code's own defaults, as VS Code
//! itself does for a theme with none (Atom One Dark has none).
//!
//! Then the repair ([`repair`]): text that would miss WCAG AA on the
//! canvas is lifted until it reads (4.5:1 for body text, 3:1 for status
//! squares, syntax and the faint tier), and the rules every Kete palette
//! keeps put the Workshop colour back where a theme's breaks one: merged
//! is purple, unread and done are a hue apart from it and each other, and
//! ember — "needs you" — never sits on fail's hue. Ember comes from the
//! theme only when its accent reads as an attention colour (an orange);
//! a blue focus ring stays a focus ring.

use crate::vscode::{Rgba, ThemeData};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Every token a palette sets, as the CSS custom property it is minus the
/// `--`. What the window clears when it goes back to its own palette.
pub const TOKENS: &[&str] = &[
    "color-bg",
    "color-surface",
    "color-raised",
    "color-rule",
    "color-rule-strong",
    "color-ink",
    "color-muted",
    "color-faint",
    "color-ember",
    "color-ember-ink",
    "color-on-ember",
    "color-unread",
    "color-done",
    "color-fail",
    "color-merged",
    "color-running",
    "color-selection",
    "color-git-modified",
    "color-git-added",
    "color-git-deleted",
    "color-git-renamed",
    "color-git-conflict",
    "color-term-bg",
    "color-term-fg",
    "color-term-cursor",
    "color-term-selection",
    "color-term-black",
    "color-term-red",
    "color-term-green",
    "color-term-yellow",
    "color-term-blue",
    "color-term-magenta",
    "color-term-cyan",
    "color-term-white",
    "color-term-bright-black",
    "color-term-bright-red",
    "color-term-bright-green",
    "color-term-bright-yellow",
    "color-term-bright-blue",
    "color-term-bright-magenta",
    "color-term-bright-cyan",
    "color-term-bright-white",
    "color-syn-comment",
    "color-syn-keyword",
    "color-syn-string",
    "color-syn-number",
    "color-syn-type",
    "color-syn-tag",
    "color-syn-attr",
    "color-syn-function",
    "color-syn-regexp",
    "color-editor-gutter",
    "color-editor-line",
    "diff-add-line",
    "diff-add-word",
    "diff-del-line",
    "diff-del-word",
];

/// A palette for the window: what an imported theme is stored as, and
/// what the live VS Code theme is sent as.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Palette {
    /// The theme's name, as the Theme row lists it.
    pub label: String,
    /// A light theme: the window's light chrome (scrollbars, the title
    /// bar) and the diff editor's light base.
    pub light: bool,
    /// Every one of [`TOKENS`], each `#rrggbb` or `#rrggbbaa`.
    pub tokens: BTreeMap<String, String>,
}

impl Palette {
    /// Every token present, known and a colour, and the label short and
    /// printable: what a stored palette must be to be painted.
    pub fn valid(&self) -> bool {
        let label_ok = !self.label.trim().is_empty()
            && self.label.chars().count() <= 80
            && !self.label.chars().any(char::is_control);
        label_ok
            && self.tokens.len() == TOKENS.len()
            && TOKENS.iter().all(|t| {
                self.tokens
                    .get(*t)
                    .is_some_and(|v| is_hex_colour(v) && Rgba::parse(v).is_some())
            })
    }
}

/// `#rrggbb` or `#rrggbbaa`, the two spellings a palette stores.
fn is_hex_colour(v: &str) -> bool {
    let Some(hex) = v.strip_prefix('#') else {
        return false;
    };
    matches!(hex.len(), 6 | 8) && hex.chars().all(|c| c.is_ascii_hexdigit())
}

/// The Workshop colours a repair puts back, for a dark or a light canvas.
struct Workshop {
    ember: Rgba,
    merged: Rgba,
    unread: Rgba,
    done: Rgba,
}

const WORKSHOP_DARK: Workshop = Workshop {
    ember: Rgba::rgb(0xE0, 0x8A, 0x35),
    merged: Rgba::rgb(0xB4, 0x8E, 0xC2),
    unread: Rgba::rgb(0x5F, 0xA8, 0xA0),
    done: Rgba::rgb(0x9B, 0xAA, 0x5E),
};

const WORKSHOP_LIGHT: Workshop = Workshop {
    ember: Rgba::rgb(0xD9, 0x82, 0x2B),
    merged: Rgba::rgb(0x6B, 0x4A, 0x7A),
    unread: Rgba::rgb(0x2F, 0x6F, 0x6A),
    done: Rgba::rgb(0x5B, 0x6B, 0x2E),
};

/// VS Code's own terminal colours (its terminalColorRegistry), dark then
/// light, in ANSI order: what a theme with no `terminal.ansi*` gets.
const ANSI_DARK: [&str; 16] = [
    "#000000", "#cd3131", "#0dbc79", "#e5e510", "#2472c8", "#bc3fbc", "#11a8cd", "#e5e5e5",
    "#666666", "#f14c4c", "#23d18b", "#f5f543", "#3b8eea", "#d670d6", "#29b8db", "#e5e5e5",
];
const ANSI_LIGHT: [&str; 16] = [
    "#000000", "#cd3131", "#107c10", "#949800", "#0451a5", "#bc05bc", "#0598bc", "#555555",
    "#666666", "#cd3131", "#14ce14", "#b5ba00", "#0451a5", "#bc05bc", "#0598bc", "#a5a5a5",
];
/// The `terminal.ansi*` keys and Kete's `term-*` names, in ANSI order.
const ANSI: [(&str, &str); 16] = [
    ("terminal.ansiBlack", "black"),
    ("terminal.ansiRed", "red"),
    ("terminal.ansiGreen", "green"),
    ("terminal.ansiYellow", "yellow"),
    ("terminal.ansiBlue", "blue"),
    ("terminal.ansiMagenta", "magenta"),
    ("terminal.ansiCyan", "cyan"),
    ("terminal.ansiWhite", "white"),
    ("terminal.ansiBrightBlack", "bright-black"),
    ("terminal.ansiBrightRed", "bright-red"),
    ("terminal.ansiBrightGreen", "bright-green"),
    ("terminal.ansiBrightYellow", "bright-yellow"),
    ("terminal.ansiBrightBlue", "bright-blue"),
    ("terminal.ansiBrightMagenta", "bright-magenta"),
    ("terminal.ansiBrightCyan", "bright-cyan"),
    ("terminal.ansiBrightWhite", "bright-white"),
];

/// `a` taken `t` of the way toward `b`, opaque.
fn mix(a: Rgba, b: Rgba, t: f64) -> Rgba {
    let ch = |x: u8, y: u8| {
        (f64::from(x) + (f64::from(y) - f64::from(x)) * t)
            .round()
            .clamp(0.0, 255.0) as u8
    };
    Rgba::rgb(ch(a.r, b.r), ch(a.g, b.g), ch(a.b, b.b))
}

/// WCAG relative luminance.
pub fn luminance(c: Rgba) -> f64 {
    let lin = |v: u8| {
        let v = f64::from(v) / 255.0;
        if v <= 0.04045 {
            v / 12.92
        } else {
            ((v + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * lin(c.r) + 0.7152 * lin(c.g) + 0.0722 * lin(c.b)
}

/// WCAG contrast ratio between two opaque colours, 1 to 21.
pub fn contrast(a: Rgba, b: Rgba) -> f64 {
    let (la, lb) = (luminance(a), luminance(b));
    let (hi, lo) = if la > lb { (la, lb) } else { (lb, la) };
    (hi + 0.05) / (lo + 0.05)
}

/// Hue in degrees, None for a grey; and the HSL saturation.
fn hue_sat(c: Rgba) -> (Option<f64>, f64) {
    let (r, g, b) = (
        f64::from(c.r) / 255.0,
        f64::from(c.g) / 255.0,
        f64::from(c.b) / 255.0,
    );
    let (hi, lo) = (r.max(g).max(b), r.min(g).min(b));
    let d = hi - lo;
    if d < 1.0 / 255.0 {
        return (None, 0.0);
    }
    let l = (hi + lo) / 2.0;
    let s = d / (1.0 - (2.0 * l - 1.0).abs()).max(1e-9);
    let h = if hi == r {
        ((g - b) / d).rem_euclid(6.0)
    } else if hi == g {
        (b - r) / d + 2.0
    } else {
        (r - g) / d + 4.0
    };
    (Some(h * 60.0), s)
}

/// How far apart two hues are, 0 to 180; 0 when either is a grey.
fn hue_gap(a: Rgba, b: Rgba) -> f64 {
    match (hue_sat(a).0, hue_sat(b).0) {
        (Some(a), Some(b)) => {
            let d = (a - b).abs();
            d.min(360.0 - d)
        }
        _ => 0.0,
    }
}

/// `c` moved toward white on a dark canvas (black on a light one) until
/// it reaches `target` contrast on every one of `on`; as is when it does.
fn lift(c: Rgba, on: &[Rgba], light: bool, target: f64) -> Rgba {
    let toward = if light {
        Rgba::rgb(0, 0, 0)
    } else {
        Rgba::rgb(255, 255, 255)
    };
    let reads = |x: Rgba| on.iter().all(|&o| contrast(x, o) >= target);
    let mut t: f64 = 0.0;
    let mut out = c;
    while !reads(out) && t < 1.0 {
        t += 0.04;
        out = mix(c, toward, t.min(1.0));
    }
    out
}

/// The window's palette for a VS Code theme: its colours mapped onto
/// [`TOKENS`] and repaired. `light` is the theme's own say (`uiTheme`, or
/// its `type`).
pub fn from_vscode(data: &ThemeData, light: bool, label: &str) -> Palette {
    let black = Rgba::rgb(0, 0, 0);
    let bg = data
        .color("editor.background")
        .map(|c| c.over(black))
        .unwrap_or(if light {
            Rgba::rgb(0xff, 0xff, 0xff)
        } else {
            Rgba::rgb(0x1e, 0x1e, 0x1e)
        });
    // The first key the theme sets, composited over `under`.
    let key = |keys: &[&str], under: Rgba| {
        keys.iter()
            .find_map(|k| data.color(k))
            .map(|c| c.over(under))
    };
    let token = |scopes: &[&str]| data.token(scopes).map(|c| c.over(bg));
    let ansi_defaults = if light { ANSI_LIGHT } else { ANSI_DARK };
    let ansi = |i: usize| {
        key(&[ANSI[i].0], bg).unwrap_or_else(|| Rgba::parse(ansi_defaults[i]).expect("a colour"))
    };
    let has_ansi = |i: usize| data.color(ANSI[i].0).is_some();

    let ink = key(&["editor.foreground", "foreground"], bg).unwrap_or(if light {
        Rgba::rgb(0x33, 0x33, 0x33)
    } else {
        Rgba::rgb(0xd4, 0xd4, 0xd4)
    });
    // A surface the theme gives that is the canvas itself is no surface.
    let apart = |c: Rgba, from: Rgba, min: f64| (contrast(c, from) >= min).then_some(c);
    let surface = key(
        &[
            "sideBar.background",
            "panel.background",
            "editorWidget.background",
        ],
        bg,
    )
    .and_then(|c| apart(c, bg, 1.04))
    .unwrap_or_else(|| mix(bg, ink, 0.04));
    let raised = key(
        &["list.activeSelectionBackground", "list.hoverBackground"],
        bg,
    )
    .and_then(|c| apart(c, bg, 1.06))
    .filter(|c| contrast(*c, surface) >= 1.04)
    .unwrap_or_else(|| mix(bg, ink, 0.09));
    let rule = key(
        &[
            "panel.border",
            "sideBar.border",
            "editorGroup.border",
            "widget.border",
        ],
        bg,
    )
    .and_then(|c| apart(c, bg, 1.25))
    .unwrap_or_else(|| mix(bg, ink, 0.18));
    let muted = key(&["descriptionForeground"], bg).unwrap_or_else(|| mix(bg, ink, 0.72));
    let faint = key(&["editorLineNumber.foreground"], bg).unwrap_or_else(|| mix(bg, ink, 0.48));

    // Statuses: the theme's git and terminal colours where it has them,
    // else its syntax's, else VS Code's terminal defaults.
    let pick = |explicit: Option<Rgba>, ansi_i: usize, scopes: &[&str]| {
        explicit
            .or_else(|| has_ansi(ansi_i).then(|| ansi(ansi_i)))
            .or_else(|| token(scopes))
            .unwrap_or_else(|| ansi(ansi_i))
    };
    let done = pick(
        key(&["gitDecoration.addedResourceForeground"], bg),
        2,
        &["string"],
    );
    let fail = pick(
        key(
            &[
                "errorForeground",
                "editorError.foreground",
                "gitDecoration.deletedResourceForeground",
            ],
            bg,
        ),
        1,
        &["entity.name.tag", "invalid"],
    );
    let merged = pick(None, 5, &["keyword", "storage.type", "storage"]);
    let unread = pick(None, 6, &["support", "support.function", "constant.other"]);
    let modified = pick(
        key(&["gitDecoration.modifiedResourceForeground"], bg),
        3,
        &["entity.name.type", "support.type", "entity.name.class"],
    );

    // Ember from the theme only when its accent is an attention colour.
    let workshop = if light {
        &WORKSHOP_LIGHT
    } else {
        &WORKSHOP_DARK
    };
    let accent = key(
        &[
            "focusBorder",
            "button.background",
            "activityBarBadge.background",
        ],
        bg,
    );
    let ember = accent.filter(|&c| attention(c)).unwrap_or(workshop.ember);

    let term_bg = key(
        &[
            "terminal.background",
            "panel.background",
            "editor.background",
        ],
        bg,
    )
    .unwrap_or(bg);
    let term_fg = key(&["terminal.foreground"], term_bg).unwrap_or(ink);
    let wash = |c: Rgba, a: f64| {
        Rgba {
            a: (a * 255.0).round() as u8,
            ..c
        }
        .hex()
    };
    // A diff key as the theme writes it, alpha kept: the editor lays the
    // word's wash over the line's.
    let raw = |k: &str| data.color(k).map(Rgba::hex);

    let mut t: BTreeMap<String, String> = BTreeMap::new();
    let mut set = |k: &str, v: String| {
        t.insert(k.to_string(), v);
    };
    set("color-bg", bg.hex());
    set("color-surface", surface.hex());
    set("color-raised", raised.hex());
    set("color-rule", rule.hex());
    set("color-rule-strong", ink.hex());
    set("color-ink", ink.hex());
    set("color-muted", muted.hex());
    set("color-faint", faint.hex());
    set("color-ember", ember.hex());
    set("color-ember-ink", ember.hex());
    set("color-on-ember", ink.hex());
    set("color-unread", unread.hex());
    set("color-done", done.hex());
    set("color-fail", fail.hex());
    set("color-merged", merged.hex());
    set("color-running", ink.hex());
    set(
        "color-selection",
        data.color("editor.selectionBackground")
            .map(Rgba::hex)
            .unwrap_or_else(|| wash(ink, if light { 0.16 } else { 0.2 })),
    );
    set("color-git-modified", modified.hex());
    set(
        "color-git-added",
        key(&["gitDecoration.addedResourceForeground"], bg)
            .unwrap_or(done)
            .hex(),
    );
    set(
        "color-git-deleted",
        key(&["gitDecoration.deletedResourceForeground"], bg)
            .unwrap_or(fail)
            .hex(),
    );
    set(
        "color-git-renamed",
        key(&["gitDecoration.renamedResourceForeground"], bg)
            .unwrap_or(unread)
            .hex(),
    );
    set(
        "color-git-conflict",
        key(&["gitDecoration.conflictingResourceForeground"], bg)
            .unwrap_or(merged)
            .hex(),
    );
    set("color-term-bg", term_bg.hex());
    set("color-term-fg", term_fg.hex());
    set(
        "color-term-cursor",
        key(
            &["terminalCursor.foreground", "editorCursor.foreground"],
            term_bg,
        )
        .unwrap_or(term_fg)
        .hex(),
    );
    set(
        "color-term-selection",
        key(
            &["terminal.selectionBackground", "editor.selectionBackground"],
            term_bg,
        )
        .unwrap_or_else(|| mix(term_bg, term_fg, 0.22))
        .hex(),
    );
    for (i, (_, name)) in ANSI.iter().enumerate() {
        set(&format!("color-term-{name}"), ansi(i).hex());
    }
    let syn = |scopes: &[&str], or: Rgba| token(scopes).unwrap_or(or).hex();
    set("color-syn-comment", syn(&["comment"], faint));
    set(
        "color-syn-keyword",
        syn(&["keyword", "storage.type", "storage"], merged),
    );
    set("color-syn-string", syn(&["string"], done));
    set(
        "color-syn-number",
        syn(&["constant.numeric", "constant"], modified),
    );
    set(
        "color-syn-type",
        syn(
            &["entity.name.type", "support.type", "storage.type"],
            unread,
        ),
    );
    set("color-syn-tag", syn(&["entity.name.tag"], fail));
    set(
        "color-syn-attr",
        syn(&["entity.other.attribute-name"], modified),
    );
    set(
        "color-syn-function",
        syn(&["entity.name.function", "support.function"], ansi(4)),
    );
    set(
        "color-syn-regexp",
        syn(&["string.regexp"], token(&["string"]).unwrap_or(unread)),
    );
    set(
        "color-editor-gutter",
        key(&["editorLineNumber.foreground"], bg)
            .unwrap_or(faint)
            .hex(),
    );
    set(
        "color-editor-line",
        key(&["editor.lineHighlightBackground"], bg)
            .unwrap_or(surface)
            .hex(),
    );
    let (line_a, word_a) = if light { (0.11, 0.26) } else { (0.10, 0.24) };
    set(
        "diff-add-line",
        raw("diffEditor.insertedLineBackground").unwrap_or_else(|| wash(done, line_a)),
    );
    set(
        "diff-add-word",
        raw("diffEditor.insertedTextBackground").unwrap_or_else(|| wash(done, word_a)),
    );
    set(
        "diff-del-line",
        raw("diffEditor.removedLineBackground").unwrap_or_else(|| wash(fail, line_a)),
    );
    set(
        "diff-del-word",
        raw("diffEditor.removedTextBackground").unwrap_or_else(|| wash(fail, word_a)),
    );
    let mut palette = Palette {
        label: label.trim().chars().take(80).collect(),
        light,
        tokens: t,
    };
    repair(&mut palette);
    palette
}

/// Does `c` read as "needs you": an orange or amber with colour in it, the
/// family Kete's ember is from?
fn attention(c: Rgba) -> bool {
    match hue_sat(c) {
        (Some(h), s) => (18.0..=48.0).contains(&h) && s >= 0.45,
        _ => false,
    }
}

/// Hold a palette to the rules Kete's palettes keep (see the module
/// docs): Workshop's colour back where a theme's breaks a rule, then
/// every text token lifted until it reads on the canvas.
pub fn repair(p: &mut Palette) {
    let light = p.light;
    let workshop = if light {
        &WORKSHOP_LIGHT
    } else {
        &WORKSHOP_DARK
    };
    let get = |p: &Palette, k: &str| {
        p.tokens
            .get(k)
            .and_then(|v| Rgba::parse(v))
            .unwrap_or(Rgba::rgb(0x80, 0x80, 0x80))
    };
    let put = |p: &mut Palette, k: &str, c: Rgba| {
        p.tokens.insert(k.to_string(), c.hex());
    };
    let bg = get(p, "color-bg");
    let surface = get(p, "color-surface");

    // The rules: which status is which, by hue.
    let merged = get(p, "color-merged");
    if !hue_sat(merged)
        .0
        .is_some_and(|h| (255.0..=320.0).contains(&h))
    {
        put(p, "color-merged", workshop.merged);
    }
    let (merged, done, fail) = (
        get(p, "color-merged"),
        get(p, "color-done"),
        get(p, "color-fail"),
    );
    if hue_gap(done, fail) < 30.0 || hue_gap(done, merged) < 30.0 {
        put(p, "color-done", workshop.done);
    }
    let done = get(p, "color-done");
    let unread = get(p, "color-unread");
    if hue_gap(unread, merged) < 25.0 || hue_gap(unread, done) < 25.0 {
        put(p, "color-unread", workshop.unread);
    }
    if hue_gap(get(p, "color-ember"), fail) < 20.0 {
        put(p, "color-ember", workshop.ember);
    }

    // Three tiers of text, in order, each reading on the canvas.
    let ink = lift(get(p, "color-ink"), &[bg, surface], light, 4.5);
    put(p, "color-ink", ink);
    put(p, "color-rule-strong", ink);
    put(p, "color-running", ink);
    let mut muted = get(p, "color-muted");
    if contrast(muted, bg) >= contrast(ink, bg) || muted == ink {
        muted = mix(bg, ink, 0.72);
    }
    put(p, "color-muted", lift(muted, &[bg], light, 4.5));
    let mut faint = get(p, "color-faint");
    if contrast(faint, bg) >= contrast(get(p, "color-muted"), bg) {
        faint = mix(bg, ink, 0.48);
    }
    put(p, "color-faint", lift(faint, &[bg], light, 3.0));

    // Ember's text tone and what is written on an ember fill.
    let ember = get(p, "color-ember");
    let ember_ink = if light {
        mix(ember, Rgba::rgb(0, 0, 0), 0.3)
    } else {
        mix(ember, Rgba::rgb(255, 255, 255), 0.25)
    };
    put(p, "color-ember-ink", lift(ember_ink, &[bg], light, 4.5));
    let on = if contrast(ink, ember) >= contrast(bg, ember) {
        ink
    } else {
        bg
    };
    put(p, "color-on-ember", on);

    // Statuses, git letters and syntax: 3:1 on the canvas.
    for k in [
        "color-unread",
        "color-done",
        "color-fail",
        "color-merged",
        "color-git-modified",
        "color-git-added",
        "color-git-deleted",
        "color-git-renamed",
        "color-git-conflict",
        "color-syn-comment",
        "color-syn-keyword",
        "color-syn-string",
        "color-syn-number",
        "color-syn-type",
        "color-syn-tag",
        "color-syn-attr",
        "color-syn-function",
        "color-syn-regexp",
        "color-editor-gutter",
    ] {
        let c = lift(get(p, k), &[bg], light, 3.0);
        put(p, k, c);
    }
    let term_bg = get(p, "color-term-bg");
    let term_fg = lift(get(p, "color-term-fg"), &[term_bg], light, 4.5);
    put(p, "color-term-fg", term_fg);
}

/// A family name the window may paint with: what a font preference holds.
/// Short, printable, and free of what could end the CSS value it goes
/// into (quotes, `;`, braces, angle brackets, backslashes).
pub fn font_family_ok(name: &str) -> bool {
    let name = name.trim();
    !name.is_empty()
        && name.chars().count() <= 64
        && !name
            .chars()
            .any(|c| c.is_control() || matches!(c, '"' | '\'' | ';' | '{' | '}' | '<' | '>' | '\\'))
}

/// A short id for an imported theme, from its label: lower case, words
/// joined by `-`, at most 40 characters.
pub fn slug(label: &str) -> String {
    let mut out = String::new();
    for c in label.trim().chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
        } else if !out.ends_with('-') && !out.is_empty() {
            out.push('-');
        }
    }
    let out: String = out.trim_end_matches('-').chars().take(40).collect();
    let out = out.trim_end_matches('-').to_string();
    if out.is_empty() {
        "theme".into()
    } else {
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn data(colors: &[(&str, &str)], tokens: &[(&str, &str)]) -> ThemeData {
        ThemeData {
            colors: colors
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
            tokens: tokens
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
            kind: None,
        }
    }

    fn tok(p: &Palette, k: &str) -> Rgba {
        Rgba::parse(&p.tokens[k]).unwrap()
    }

    /// Atom One Dark as akamud ships it: no terminal colours, a blue focus
    /// ring, its syntax palette.
    fn one_dark() -> ThemeData {
        data(
            &[
                ("editor.background", "#282C34"),
                ("editor.foreground", "#ABB2BF"),
                ("focusBorder", "#528BFF"),
                ("button.background", "#4D78CC"),
                ("sideBar.background", "#21252B"),
                ("list.activeSelectionBackground", "#2C313A"),
                ("editorLineNumber.foreground", "#636D83"),
                ("editor.selectionBackground", "#3E4451"),
                ("editorCursor.foreground", "#528BFF"),
                ("diffEditor.insertedTextBackground", "#00809B33"),
            ],
            &[
                ("comment", "#5C6370"),
                ("keyword", "#C678DD"),
                ("string", "#98C379"),
                ("constant.numeric", "#D19A66"),
                ("entity.name.function", "#61AFEF"),
                ("entity.name.type", "#E5C07B"),
                ("entity.name.tag", "#E06C75"),
                ("support", "#56B6C2"),
            ],
        )
    }

    /// Every token comes out, each a colour, and the result is a palette
    /// the store would keep.
    #[test]
    fn a_theme_maps_onto_every_token() {
        let p = from_vscode(&one_dark(), false, "Atom One Dark");
        assert_eq!(p.tokens.len(), TOKENS.len());
        assert!(p.valid(), "{p:?}");
        let p = from_vscode(&ThemeData::default(), true, "Bare");
        assert!(p.valid(), "an empty theme still gives a whole palette");
        assert_eq!(tok(&p, "color-bg"), Rgba::rgb(255, 255, 255));
    }

    /// Atom One Dark: its canvas, ink and surfaces; its syntax for the
    /// statuses (it has no git or terminal colours); VS Code's terminal
    /// defaults for the ANSI colours; Workshop's ember kept, since its
    /// accent is a blue focus ring, not an attention colour.
    #[test]
    fn atom_one_dark_keeps_workshop_ember_and_its_own_syntax() {
        let p = from_vscode(&one_dark(), false, "Atom One Dark");
        assert_eq!(p.tokens["color-bg"], "#282c34");
        assert_eq!(p.tokens["color-ink"], "#abb2bf");
        assert_eq!(p.tokens["color-surface"], "#21252b");
        assert_eq!(p.tokens["color-raised"], "#2c313a");
        assert_eq!(p.tokens["color-ember"], "#e08a35");
        assert_eq!(p.tokens["color-merged"], "#c678dd");
        assert_eq!(p.tokens["color-done"], "#98c379");
        assert_eq!(p.tokens["color-unread"], "#56b6c2");
        assert_eq!(p.tokens["color-syn-keyword"], "#c678dd");
        assert_eq!(p.tokens["color-syn-function"], "#61afef");
        assert_eq!(p.tokens["color-term-red"], "#cd3131", "VS Code's default");
        assert_eq!(p.tokens["color-term-cursor"], "#528bff");
        assert_eq!(
            p.tokens["diff-add-word"], "#00809b33",
            "a diff wash keeps its alpha"
        );
        assert!(!p.light);
    }

    /// An orange accent is an attention colour, and becomes ember.
    #[test]
    fn an_orange_accent_becomes_ember() {
        let d = data(
            &[
                ("editor.background", "#1d2021"),
                ("editor.foreground", "#ebdbb2"),
                ("focusBorder", "#fe8019"),
            ],
            &[],
        );
        let p = from_vscode(&d, false, "Gruvbox");
        assert_eq!(p.tokens["color-ember"], "#fe8019");
        let ember = tok(&p, "color-ember");
        let on = tok(&p, "color-on-ember");
        assert!(contrast(on, ember) >= 4.5, "what is written on ember reads");
    }

    /// Text that would not read on the canvas is lifted to AA: ink and
    /// muted to 4.5:1, the faint tier and the statuses to 3:1.
    #[test]
    fn low_contrast_text_is_lifted_to_aa() {
        let d = data(
            &[
                ("editor.background", "#202020"),
                ("editor.foreground", "#404040"),
                ("descriptionForeground", "#303030"),
                ("editorLineNumber.foreground", "#252525"),
                ("terminal.ansiGreen", "#203020"),
            ],
            &[("comment", "#282828")],
        );
        let p = from_vscode(&d, false, "Murk");
        let bg = tok(&p, "color-bg");
        assert!(contrast(tok(&p, "color-ink"), bg) >= 4.5);
        assert!(contrast(tok(&p, "color-ink"), tok(&p, "color-surface")) >= 4.5);
        assert!(contrast(tok(&p, "color-muted"), bg) >= 4.5);
        assert!(contrast(tok(&p, "color-faint"), bg) >= 3.0);
        assert!(contrast(tok(&p, "color-done"), bg) >= 3.0);
        assert!(contrast(tok(&p, "color-syn-comment"), bg) >= 3.0);
        assert!(contrast(tok(&p, "color-ember-ink"), bg) >= 4.5);
        assert!(contrast(tok(&p, "color-term-fg"), tok(&p, "color-term-bg")) >= 4.5);
        // The same on a light canvas: lifted toward black.
        let d = data(
            &[
                ("editor.background", "#fafafa"),
                ("editor.foreground", "#c0c0c0"),
            ],
            &[],
        );
        let p = from_vscode(&d, true, "Fog");
        assert!(contrast(tok(&p, "color-ink"), tok(&p, "color-bg")) >= 4.5);
        assert!(luminance(tok(&p, "color-ink")) < luminance(Rgba::rgb(0xc0, 0xc0, 0xc0)));
    }

    /// A magenta that is red is no merge, an unread on the done hue is no
    /// unread, and an ember on fail's hue gives way: Workshop's colours go
    /// back in.
    #[test]
    fn repair_restores_roles_a_theme_would_break() {
        let d = data(
            &[
                ("editor.background", "#000000"),
                ("editor.foreground", "#e0e0e0"),
                ("terminal.ansiMagenta", "#ff2020"),
                ("terminal.ansiRed", "#ff6a00"),
                ("terminal.ansiGreen", "#30c030"),
                ("terminal.ansiCyan", "#40c040"),
                ("focusBorder", "#ff8c1a"),
            ],
            &[],
        );
        let p = from_vscode(&d, false, "Loud");
        assert_eq!(tok(&p, "color-merged"), WORKSHOP_DARK.merged);
        assert_eq!(tok(&p, "color-unread"), WORKSHOP_DARK.unread);
        assert_eq!(
            tok(&p, "color-ember"),
            WORKSHOP_DARK.ember,
            "an orange accent on fail's own hue is no ember"
        );
        let d = data(
            &[
                ("editor.background", "#ffffff"),
                ("editor.foreground", "#202020"),
                ("terminal.ansiMagenta", "#20a020"),
            ],
            &[],
        );
        let light = from_vscode(&d, true, "Loud light");
        assert_eq!(tok(&light, "color-merged"), WORKSHOP_LIGHT.merged);
    }

    #[test]
    fn a_stored_palette_must_be_whole_and_colours_only() {
        let mut p = from_vscode(&one_dark(), false, "Atom One Dark");
        assert!(p.valid());
        p.tokens
            .insert("color-bg".into(), "red; background: url(x)".into());
        assert!(!p.valid());
        let mut p = from_vscode(&one_dark(), false, "Atom One Dark");
        p.tokens.remove("color-ink");
        assert!(!p.valid());
        let mut p = from_vscode(&one_dark(), false, "Atom One Dark");
        p.tokens.insert("color-extra".into(), "#000000".into());
        assert!(!p.valid());
        let mut p = from_vscode(&one_dark(), false, "x");
        p.label = "bad\nlabel".into();
        assert!(!p.valid());
    }

    #[test]
    fn font_names_are_plain_family_names() {
        assert!(font_family_ok("JetBrains Mono"));
        assert!(font_family_ok("Segoe UI Variable Text"));
        assert!(!font_family_ok(""));
        assert!(!font_family_ok("A\"; } body { color: red"));
        assert!(!font_family_ok("x\\y"));
        assert!(!font_family_ok(&"a".repeat(65)));
    }

    #[test]
    fn a_label_slugs_to_a_short_id() {
        assert_eq!(slug("Atom One Dark"), "atom-one-dark");
        assert_eq!(slug("  Dark+ (default) "), "dark-default");
        assert_eq!(slug("!!!"), "theme");
        assert!(slug(&"word ".repeat(30)).len() <= 40);
    }
}
