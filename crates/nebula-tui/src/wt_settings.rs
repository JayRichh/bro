//! Windows Terminal's `settings.json` (JSONC, `crate::jsonc`): read for the
//! font the profile Bro runs in draws with — which decides whether the
//! FILE ICONS can be Nerd Font glyphs (`crate::icons::detect`) — and
//! written by `bro setup terminal`, which adds a **Bro** profile: `bro` as
//! its command line, Bro's icon, JetBrainsMono Nerd Font, and a One Dark
//! color scheme to match the `one-dark` theme.
//!
//! The write is the one place Bro edits a file it does not own, so it is
//! careful:
//!
//! - **What** changes is decided on the parsed JSON ([`add_bro_profile`]),
//!   a pure function: the profile has a fixed GUID, so a second run
//!   updates the one the first made rather than adding another; the
//!   scheme goes in only when no scheme of its name is there; a profile or
//!   scheme the user made is never touched.
//! - **How** it is written keeps the file as the user (or Windows
//!   Terminal) wrote it ([`plan`]): the new profile and scheme are spliced
//!   into the text at the places the JSONC OUTLINE finds, so every comment,
//!   every key's order and every other byte stays. The spliced text is
//!   parsed again and must come out equal to what [`add_bro_profile`]
//!   decided, or nothing is written.
//! - The caller backs the file up beside itself before writing it, and
//!   `--dry-run` prints [`unified_diff`] of the two texts instead.

use crate::jsonc::{Node, Shape};
use serde_json::{json, Map, Value};
use std::path::PathBuf;

/// The Bro profile's GUID: fixed, so `bro setup terminal` run twice finds
/// the profile it made the first time.
pub const PROFILE_GUID: &str = "{8b0f3d6e-6a2b-4d8c-9b5e-62726f000001}";
/// The profile's name in Windows Terminal's dropdown.
pub const PROFILE_NAME: &str = "Bro";
/// The color scheme the profile wears. A "One Dark" the user already has
/// is theirs and stays as it is — the profile wears that one.
pub const SCHEME_NAME: &str = "One Dark";
/// The font the profile draws with, the one `bro setup terminal` installs.
pub const FONT_FACE: &str = "JetBrainsMono Nerd Font";

/// Where Windows Terminal keeps its settings: the Store build's
/// `LocalState`, else the Preview's, else the unpackaged build's; the
/// Store path when none exists yet. None off Windows.
pub fn path() -> Option<PathBuf> {
    if !cfg!(windows) {
        return None;
    }
    let local = PathBuf::from(std::env::var_os("LOCALAPPDATA")?);
    let candidates = [
        local.join(r"Packages\Microsoft.WindowsTerminal_8wekyb3d8bbwe\LocalState\settings.json"),
        local.join(
            r"Packages\Microsoft.WindowsTerminalPreview_8wekyb3d8bbwe\LocalState\settings.json",
        ),
        local.join(r"Microsoft\Windows Terminal\settings.json"),
    ];
    candidates
        .iter()
        .find(|p| p.is_file())
        .cloned()
        .or_else(|| candidates.into_iter().next())
}

fn same_guid(a: &str, b: &str) -> bool {
    let norm = |s: &str| s.trim_matches(['{', '}']).to_ascii_lowercase();
    norm(a) == norm(b)
}

fn is_ours(profile: &Value) -> bool {
    profile
        .get("guid")
        .and_then(Value::as_str)
        .is_some_and(|g| same_guid(g, PROFILE_GUID))
}

fn is_our_scheme(scheme: &Value) -> bool {
    scheme.get("name").and_then(Value::as_str) == Some(SCHEME_NAME)
}

/// The profiles list, whichever of the two shapes the file uses: an array
/// (old files) or `{ "defaults": {...}, "list": [...] }`.
fn profile_list(settings: &Value) -> Vec<&Value> {
    match settings.get("profiles") {
        Some(Value::Array(list)) => list.iter().collect(),
        Some(Value::Object(o)) => o
            .get("list")
            .and_then(Value::as_array)
            .map(|l| l.iter().collect())
            .unwrap_or_default(),
        _ => Vec::new(),
    }
}

/// A profile object's font face: `font.face`, or the older `fontFace`.
fn face_of(profile: &Value) -> Option<String> {
    profile
        .pointer("/font/face")
        .or_else(|| profile.get("fontFace"))
        .and_then(Value::as_str)
        .map(str::to_string)
}

/// The font face the profile `guid` draws with — its own, else the
/// profiles' defaults'. With no `guid` (Bro was not started from Windows
/// Terminal, or by a build that does not say), the default profile's.
pub fn font_face(settings: &Value, guid: Option<&str>) -> Option<String> {
    let default_guid = settings.get("defaultProfile").and_then(Value::as_str);
    let want = guid.or(default_guid);
    let profile = want.and_then(|g| {
        profile_list(settings).into_iter().find(|p| {
            p.get("guid")
                .and_then(Value::as_str)
                .is_some_and(|pg| same_guid(pg, g))
        })
    });
    profile
        .and_then(face_of)
        .or_else(|| settings.pointer("/profiles/defaults").and_then(face_of))
}

/// One Dark as a Windows Terminal color scheme: Atom One Dark's palette
/// on its own `#282C34`, the canvas the `one-dark` theme paints.
pub fn one_dark_scheme() -> Value {
    json!({
        "name": SCHEME_NAME,
        "background": "#282C34",
        "foreground": "#ABB2BF",
        "cursorColor": "#528BFF",
        "selectionBackground": "#3E4451",
        "black": "#282C34",
        "red": "#E06C75",
        "green": "#98C379",
        "yellow": "#E5C07B",
        "blue": "#61AFEF",
        "purple": "#C678DD",
        "cyan": "#56B6C2",
        "white": "#ABB2BF",
        "brightBlack": "#5C6370",
        "brightRed": "#E06C75",
        "brightGreen": "#98C379",
        "brightYellow": "#D19A66",
        "brightBlue": "#61AFEF",
        "brightPurple": "#C678DD",
        "brightCyan": "#56B6C2",
        "brightWhite": "#FFFFFF"
    })
}

/// The Bro profile: `commandline` (the `bro` to run, a full path when it
/// is known — quoted when it has a space, as Windows Terminal splits the
/// line), its `icon` (an absolute path) when there is one, the Nerd Font,
/// the One Dark scheme.
pub fn bro_profile(commandline: &str, icon: Option<&str>) -> Value {
    let commandline = if commandline.contains(' ') && !commandline.starts_with('"') {
        format!("\"{commandline}\"")
    } else {
        commandline.to_string()
    };
    let mut profile = json!({
        "guid": PROFILE_GUID,
        "name": PROFILE_NAME,
        "commandline": commandline,
        "font": { "face": FONT_FACE },
        "colorScheme": SCHEME_NAME,
        "hidden": false
    });
    if let Some(icon) = icon {
        profile["icon"] = icon.into();
    }
    profile
}

/// The profile as it should be: ours over `old`, so whatever the user set
/// on it since (an opacity, a starting directory) stays.
fn merged_profile(old: &Value, ours: &Value) -> Value {
    let mut merged = old.clone();
    if let (Some(m), Some(ours)) = (merged.as_object_mut(), ours.as_object()) {
        for (k, v) in ours {
            m.insert(k.clone(), v.clone());
        }
    }
    merged
}

/// Put the Bro profile and its scheme into `settings`: the scheme when
/// none of its name is there, the profile by its GUID (replacing what an
/// earlier run put there, keeping what the user added to it), touching
/// nothing else. Returns what changed, one line each, for the dry run and
/// the summary; empty when it was all there already.
pub fn add_bro_profile(settings: &mut Value, commandline: &str, icon: Option<&str>) -> Vec<String> {
    let mut changes = Vec::new();
    if !settings.is_object() {
        *settings = Value::Object(Map::new());
    }
    let root = settings.as_object_mut().expect("an object now");

    // The scheme, by name; one already there is left as it is.
    let schemes = root
        .entry("schemes")
        .or_insert_with(|| Value::Array(Vec::new()));
    if !schemes.is_array() {
        *schemes = Value::Array(Vec::new());
    }
    let schemes = schemes.as_array_mut().expect("an array now");
    if !schemes.iter().any(is_our_scheme) {
        schemes.push(one_dark_scheme());
        changes.push(format!("add color scheme \"{SCHEME_NAME}\""));
    }

    // The profile, by GUID — in `profiles.list`, or in the bare array an
    // older file keeps its profiles in.
    let profile = bro_profile(commandline, icon);
    let profiles = root
        .entry("profiles")
        .or_insert_with(|| json!({ "defaults": {}, "list": [] }));
    if !profiles.is_array() && !profiles.is_object() {
        *profiles = json!({ "defaults": {}, "list": [] });
    }
    let list = match profiles {
        Value::Array(list) => list,
        Value::Object(o) => {
            let list = o.entry("list").or_insert_with(|| Value::Array(Vec::new()));
            if !list.is_array() {
                *list = Value::Array(Vec::new());
            }
            list.as_array_mut().expect("an array now")
        }
        _ => unreachable!("made an object above"),
    };
    let what = format!(
        "profile \"{PROFILE_NAME}\" ({}, {FONT_FACE}, {SCHEME_NAME})",
        profile["commandline"].as_str().unwrap_or_default()
    );
    match list.iter_mut().find(|p| is_ours(p)) {
        Some(old) => {
            let merged = merged_profile(old, &profile);
            if merged != *old {
                *old = merged;
                changes.push(format!("update {what}"));
            }
        }
        None => {
            list.push(profile);
            changes.push(format!("add {what}"));
        }
    }
    changes
}

/// The order the keys of what Bro writes go in — the profile's as Windows
/// Terminal's own UI writes a profile, the scheme's as its schema lists a
/// scheme — rather than `serde_json`'s alphabetical one, which would put
/// a profile's `colorScheme` above its `name`. Keys not named here follow,
/// alphabetically.
const KEY_ORDER: &[&str] = &[
    "guid",
    "name",
    "commandline",
    "icon",
    "font",
    "face",
    "colorScheme",
    "hidden",
    "background",
    "foreground",
    "cursorColor",
    "selectionBackground",
    "black",
    "red",
    "green",
    "yellow",
    "blue",
    "purple",
    "cyan",
    "white",
    "brightBlack",
    "brightRed",
    "brightGreen",
    "brightYellow",
    "brightBlue",
    "brightPurple",
    "brightCyan",
    "brightWhite",
    "defaults",
    "list",
];

/// `value` as Windows Terminal writes JSON — four-space indents, keys in
/// [`KEY_ORDER`] — for a place indented `indent`: its first line as it
/// is, every line after it indented to match.
fn write(value: &Value, indent: &str) -> String {
    let inner = format!("{indent}    ");
    match value {
        Value::Object(m) if !m.is_empty() => {
            let rank = |k: &str| KEY_ORDER.iter().position(|o| *o == k).unwrap_or(usize::MAX);
            let mut keys: Vec<&String> = m.keys().collect();
            keys.sort_by(|a, b| rank(a).cmp(&rank(b)).then_with(|| a.cmp(b)));
            let body: Vec<String> = keys
                .into_iter()
                .map(|k| {
                    format!(
                        "{inner}{}: {}",
                        Value::from(k.as_str()),
                        write(&m[k], &inner)
                    )
                })
                .collect();
            format!("{{\n{}\n{indent}}}", body.join(",\n"))
        }
        Value::Array(items) if !items.is_empty() => {
            let body: Vec<String> = items
                .iter()
                .map(|v| format!("{inner}{}", write(v, &inner)))
                .collect();
            format!("[\n{}\n{indent}]", body.join(",\n"))
        }
        other => other.to_string(),
    }
}

/// What `bro setup terminal` would do to a settings file: the changes,
/// one line each ([`add_bro_profile`]), and the file's text with them made.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Plan {
    pub changes: Vec<String>,
    pub text: String,
}

/// One splice: the bytes `at` replaced by `with`.
struct Edit {
    at: std::ops::Range<usize>,
    with: String,
}

/// The leading whitespace of the line `pos` is on.
fn line_indent(text: &str, pos: usize) -> &str {
    let line = text[..pos].rfind('\n').map_or(0, |i| i + 1);
    let rest = &text[line..];
    &rest[..rest.len() - rest.trim_start_matches([' ', '\t']).len()]
}

/// The byte ranges of a container's elements: an array's values, an
/// object's members from the quote that opens the key.
fn element_ranges(text: &str, node: &Node) -> Vec<std::ops::Range<usize>> {
    match &node.shape {
        Shape::Array(items) => items.iter().map(|n| n.start..n.end).collect(),
        Shape::Object(members) => members
            .iter()
            .map(|(k, n)| {
                let key = Value::from(k.as_str()).to_string();
                text[..n.start].rfind(&key).unwrap_or(n.start)..n.end
            })
            .collect(),
        Shape::Scalar => Vec::new(),
    }
}

/// The splice that adds `item` (written out by the closure for the indent
/// it is given) to the container `node` — an array, or an object when
/// `item` is a `"key": value` member:
///
/// - after its last element, at that element's indent;
/// - a container all on one line (`"schemes": [ {...} ]`, as hand-written
///   files have them) is opened out instead, one element a line, each
///   element's own text kept — a new element on the end of that line
///   would read badly;
/// - an empty one gets the element on a line of its own, the container's
///   line's indent and four spaces more.
///
/// The opening out splices only the gaps between the elements, so an edit
/// inside one of them (a list inside the root object) still lands.
fn append(text: &str, node: &Node, item: impl Fn(&str) -> String) -> Vec<Edit> {
    let outer = line_indent(text, node.start).to_string();
    let inner = format!("{outer}    ");
    let elements = element_ranges(text, node);
    let span = &text[node.start..node.end];
    let plain = crate::jsonc::strip(span) == span;
    if !elements.is_empty() && !span.contains('\n') && plain {
        let mut edits = vec![Edit {
            at: node.start + 1..elements[0].start,
            with: format!("\n{inner}"),
        }];
        for pair in elements.windows(2) {
            edits.push(Edit {
                at: pair[0].end..pair[1].start,
                with: format!(",\n{inner}"),
            });
        }
        let last = elements.last().expect("not empty");
        edits.push(Edit {
            at: last.end..node.end - 1,
            with: format!(",\n{inner}{}\n{outer}", item(&inner)),
        });
        return edits;
    }
    let edit = match elements.last() {
        Some(last) => {
            let indent = line_indent(text, last.start).to_string();
            Edit {
                at: last.end..last.end,
                with: format!(",\n{indent}{}", item(&indent)),
            }
        }
        None => {
            let inside = node.start + 1..node.end - 1;
            // An empty container's inside is whitespace, or comments to
            // keep: replace the one, write after the other.
            let at = if text[inside.clone()].trim().is_empty() {
                inside
            } else {
                inside.start..inside.start
            };
            Edit {
                at,
                with: format!("\n{inner}{}\n{outer}", item(&inner)),
            }
        }
    };
    vec![edit]
}

/// [`add_bro_profile`] made in `text` itself, every other byte kept (the
/// module docs). Errors when the text is not JSONC, or when a splice
/// would not come out as the parsed change says — then nothing should be
/// written, and the caller says so.
pub fn plan(text: &str, commandline: &str, icon: Option<&str>) -> Result<Plan, String> {
    let text = if text.trim().is_empty() { "{}" } else { text };
    let before = crate::jsonc::parse(text)?;
    let mut want = before.clone();
    let changes = add_bro_profile(&mut want, commandline, icon);
    if changes.is_empty() {
        return Ok(Plan {
            changes,
            text: text.to_string(),
        });
    }
    let root = crate::jsonc::outline(text)?;
    if !matches!(root.shape, Shape::Object(_)) {
        return Err("the settings are not a JSON object".into());
    }
    let profile = bro_profile(commandline, icon);
    let mut edits = Vec::new();
    // Members the root object lacks, added in one splice: two splices at
    // the one place would each think they were the first.
    let mut root_adds: Vec<(&str, Value)> = Vec::new();

    // The scheme.
    if !before
        .get("schemes")
        .and_then(Value::as_array)
        .is_some_and(|s| s.iter().any(is_our_scheme))
    {
        let scheme = one_dark_scheme();
        match root.get("schemes") {
            Some(node) if matches!(node.shape, Shape::Array(_)) => {
                edits.extend(append(text, node, |ind| write(&scheme, ind)));
            }
            Some(node) => edits.push(Edit {
                at: node.start..node.end,
                with: write(&json!([scheme]), line_indent(text, node.start)),
            }),
            None => root_adds.push(("schemes", json!([scheme]))),
        }
    }

    // The profile.
    let list = match root.get("profiles") {
        Some(node) if matches!(node.shape, Shape::Array(_)) => Some(node),
        Some(node) if matches!(node.shape, Shape::Object(_)) => match node.get("list") {
            Some(list) if matches!(list.shape, Shape::Array(_)) => Some(list),
            Some(list) => {
                edits.push(Edit {
                    at: list.start..list.end,
                    with: write(&json!([profile]), line_indent(text, list.start)),
                });
                None
            }
            None => {
                edits.extend(append(text, node, |ind| {
                    format!("\"list\": {}", write(&json!([profile]), ind))
                }));
                None
            }
        },
        Some(node) => {
            edits.push(Edit {
                at: node.start..node.end,
                with: write(
                    &json!({ "defaults": {}, "list": [profile] }),
                    line_indent(text, node.start),
                ),
            });
            None
        }
        None => {
            root_adds.push(("profiles", json!({ "defaults": {}, "list": [profile] })));
            None
        }
    };
    if let Some(list) = list {
        let Shape::Array(items) = &list.shape else {
            unreachable!("matched an array")
        };
        let old = profile_list(&before)
            .into_iter()
            .enumerate()
            .find(|(_, p)| is_ours(p));
        match old {
            Some((i, old)) => {
                let merged = merged_profile(old, &profile);
                if merged != *old {
                    let node = &items[i];
                    edits.push(Edit {
                        at: node.start..node.end,
                        with: write(&merged, line_indent(text, node.start)),
                    });
                }
            }
            None => edits.extend(append(text, list, |ind| write(&profile, ind))),
        }
    }

    if !root_adds.is_empty() {
        edits.extend(append(text, &root, |ind| {
            root_adds
                .iter()
                .map(|(k, v)| format!("\"{k}\": {}", write(v, ind)))
                .collect::<Vec<_>>()
                .join(&format!(",\n{ind}"))
        }));
    }

    // Spliced back to front, so each range is still where it was found.
    edits.sort_by_key(|e| std::cmp::Reverse(e.at.start));
    let mut out = text.to_string();
    // A file whose every line ends `\r\n` gets its new lines the same way.
    let crlf = text.contains("\r\n") && !text.replace("\r\n", "").contains('\n');
    for mut edit in edits {
        if crlf {
            edit.with = edit.with.replace('\n', "\r\n");
        }
        out.replace_range(edit.at, &edit.with);
    }
    if crate::jsonc::parse(&out)? != want {
        return Err("editing the file in place would not have given the intended settings".into());
    }
    Ok(Plan { changes, text: out })
}

/// A unified diff of `old` to `new` (three lines of context, `---`/`+++`
/// headers naming `label`), for `--dry-run` to show what would be
/// written. Empty when they are the same.
pub fn unified_diff(old: &str, new: &str, label: &str) -> String {
    let a: Vec<&str> = old.lines().collect();
    let b: Vec<&str> = new.lines().collect();
    let ops = similar::capture_diff_slices(similar::Algorithm::Myers, &a, &b);
    let groups = similar::group_diff_ops(ops, 3);
    if groups.is_empty() {
        return String::new();
    }
    let mut out = format!("--- {label}\n+++ {label} (after)\n");
    for group in groups {
        let (first, last) = (&group[0], &group[group.len() - 1]);
        let (o, n) = (first.old_range().start, first.new_range().start);
        let (o_len, n_len) = (last.old_range().end - o, last.new_range().end - n);
        out.push_str(&format!("@@ -{},{o_len} +{},{n_len} @@\n", o + 1, n + 1));
        for op in &group {
            let (tag, old_r, new_r) = op.as_tag_tuple();
            match tag {
                similar::DiffTag::Equal => {
                    for line in &a[old_r] {
                        out.push_str(&format!(" {line}\n"));
                    }
                }
                _ => {
                    for line in &a[old_r] {
                        out.push_str(&format!("-{line}\n"));
                    }
                    for line in &b[new_r] {
                        out.push_str(&format!("+{line}\n"));
                    }
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A settings file as Windows Terminal writes it, comments and all.
    const SETTINGS: &str = r##"{
    "$schema": "https://aka.ms/terminal-profiles-schema",
    // the one that opens
    "defaultProfile": "{574e775e-4f2a-5b96-ac1e-a2962a402336}",
    "profiles": {
        "defaults": { "font": { "face": "Cascadia Mono" } },
        "list": [
            { "guid": "{574e775e-4f2a-5b96-ac1e-a2962a402336}", "name": "PowerShell",
              "font": { "face": "CaskaydiaCove NF" } },
            { "guid": "{0caa0dad-35be-5f56-a8ff-afceeeaa6101}", "name": "cmd" },
        ]
    },
    "schemes": [ { "name": "Campbell", "background": "#0C0C0C" } ],
}"##;

    const BRO: &str = r"C:\Users\me\.cargo\bin\bro.exe";

    /// The profile Bro runs in decides: its own face, the defaults' when
    /// it sets none, the default profile's with no GUID to go on.
    #[test]
    fn the_font_face_is_the_profiles_or_the_defaults() {
        let s = crate::jsonc::parse(SETTINGS).unwrap();
        assert_eq!(
            font_face(&s, Some("574e775e-4f2a-5b96-ac1e-a2962a402336")).as_deref(),
            Some("CaskaydiaCove NF")
        );
        assert_eq!(
            font_face(&s, Some("{0CAA0DAD-35BE-5F56-A8FF-AFCEEEAA6101}")).as_deref(),
            Some("Cascadia Mono"),
            "the defaults', GUID case and braces aside"
        );
        assert_eq!(font_face(&s, None).as_deref(), Some("CaskaydiaCove NF"));
    }

    /// The profile and its scheme go in once — a second run changes
    /// nothing — and nothing the user made is touched.
    #[test]
    fn the_bro_profile_goes_in_once_and_leaves_the_rest_alone() {
        let mut s = crate::jsonc::parse(SETTINGS).unwrap();
        let before = s.clone();
        let changes = add_bro_profile(&mut s, BRO, Some(r"C:\bro\bro.ico"));
        assert_eq!(changes.len(), 2, "{changes:?}");
        let list = s.pointer("/profiles/list").unwrap().as_array().unwrap();
        assert_eq!(list.len(), 3);
        let bro = &list[2];
        assert_eq!(bro["guid"], PROFILE_GUID);
        assert_eq!(bro["font"]["face"], FONT_FACE);
        assert_eq!(bro["colorScheme"], SCHEME_NAME);
        assert_eq!(bro["icon"], r"C:\bro\bro.ico");
        assert_eq!(s["schemes"][0], before["schemes"][0], "theirs untouched");
        assert_eq!(s["schemes"][1]["name"], SCHEME_NAME);
        assert_eq!(list[0], before["profiles"]["list"][0]);

        assert!(add_bro_profile(&mut s, BRO, Some(r"C:\bro\bro.ico")).is_empty());
        // A moved binary updates the one profile, keeping what the user
        // set on it since.
        s["profiles"]["list"][2]["opacity"] = json!(90);
        let changes = add_bro_profile(&mut s, r"D:\my tools\bro.exe", None);
        assert_eq!(changes.len(), 1, "{changes:?}");
        let bro = &s["profiles"]["list"][2];
        assert_eq!(bro["commandline"], r#""D:\my tools\bro.exe""#, "quoted");
        assert_eq!(bro["opacity"], 90);
        assert_eq!(s["profiles"]["list"].as_array().unwrap().len(), 3);
    }

    /// A "One Dark" the user already has is theirs: it stays as it is, and
    /// the profile wears it.
    #[test]
    fn a_one_dark_already_there_is_kept() {
        let mut s = json!({ "schemes": [ { "name": "One Dark", "background": "#000000" } ] });
        let changes = add_bro_profile(&mut s, "bro", None);
        assert_eq!(changes.len(), 1, "only the profile: {changes:?}");
        assert_eq!(s["schemes"].as_array().unwrap().len(), 1);
        assert_eq!(s["schemes"][0]["background"], "#000000");
    }

    /// An old file keeps its profiles in a bare array; an empty one has
    /// none at all.
    #[test]
    fn older_and_empty_files_take_the_profile_too() {
        let mut s = json!({ "profiles": [ { "name": "x" } ] });
        add_bro_profile(&mut s, "bro", None);
        assert_eq!(s["profiles"].as_array().unwrap().len(), 2);
        let mut s = json!({});
        add_bro_profile(&mut s, "bro", None);
        assert_eq!(s["profiles"]["list"][0]["name"], PROFILE_NAME);
        assert_eq!(
            font_face(&s, Some(PROFILE_GUID)).as_deref(),
            Some(FONT_FACE)
        );
    }

    /// The plan splices the profile and scheme into the text: every
    /// comment and every other line stays where it was, the result parses
    /// to what [`add_bro_profile`] decided, and planning again on the
    /// result changes nothing.
    #[test]
    fn the_plan_keeps_the_files_comments_and_layout() {
        let plan = plan(SETTINGS, BRO, Some(r"C:\bro\bro.ico")).unwrap();
        assert_eq!(plan.changes.len(), 2);
        assert!(plan.text.contains("    // the one that opens\n"));
        assert!(
            plan.text.starts_with("{\n    \"$schema\""),
            "key order kept"
        );
        let mut want = crate::jsonc::parse(SETTINGS).unwrap();
        add_bro_profile(&mut want, BRO, Some(r"C:\bro\bro.ico"));
        assert_eq!(crate::jsonc::parse(&plan.text).unwrap(), want);
        // Every line of the original is still there but the one-line
        // `schemes`, which is opened out to take the second scheme, its
        // own element kept as written.
        let diff = unified_diff(SETTINGS, &plan.text, "settings.json");
        let removed: Vec<&str> = diff
            .lines()
            .filter(|l| l.starts_with('-') && !l.starts_with("---"))
            .collect();
        assert_eq!(
            removed,
            ["-    \"schemes\": [ { \"name\": \"Campbell\", \"background\": \"#0C0C0C\" } ],"],
            "{diff}"
        );
        assert!(plan
            .text
            .contains("\"schemes\": [\n        { \"name\": \"Campbell\", \"background\": \"#0C0C0C\" },\n        {\n            \"name\": \"One Dark\",\n            \"background\": \"#282C34\","));
        // The new profile sits at the list's indent, its keys in Windows
        // Terminal's order.
        assert!(plan.text.contains(
            "\n            {\n                \"guid\": \"{8b0f3d6e-6a2b-4d8c-9b5e-62726f000001}\",\n                \"name\": \"Bro\",\n                \"commandline\""
        ));

        let again = super::plan(&plan.text, BRO, Some(r"C:\bro\bro.ico")).unwrap();
        assert!(again.changes.is_empty());
        assert_eq!(again.text, plan.text);
        assert!(unified_diff(&plan.text, &again.text, "x").is_empty());
    }

    /// A moved binary replaces the one profile in place; the user's
    /// additions to it ride along.
    #[test]
    fn the_plan_updates_its_own_profile_in_place() {
        let first = plan(SETTINGS, BRO, None).unwrap().text;
        let tweaked = first.replace(
            "\"hidden\": false",
            "\"hidden\": false,\n                \"opacity\": 90",
        );
        let second = plan(&tweaked, r"D:\bro.exe", None).unwrap();
        assert_eq!(second.changes.len(), 1, "{:?}", second.changes);
        let v = crate::jsonc::parse(&second.text).unwrap();
        let list = v["profiles"]["list"].as_array().unwrap();
        assert_eq!(list.len(), 3);
        assert_eq!(list[2]["commandline"], r"D:\bro.exe");
        assert_eq!(list[2]["opacity"], 90);
        assert!(second.text.contains("// the one that opens"));
    }

    /// Files missing the pieces, or with them empty: the pieces are made,
    /// and the result is still what the parsed change says.
    #[test]
    fn the_plan_fills_in_what_is_missing() {
        for text in [
            "",
            "{}",
            "{\n    // nothing yet\n}",
            "{ \"profiles\": [] }",
            "{ \"profiles\": [ { \"name\": \"x\" } ], \"schemes\": [] }",
            "{ \"profiles\": { \"defaults\": {} } }",
            "{ \"profiles\": { \"list\": [ /* none */ ] }, }",
        ] {
            let plan = plan(text, "bro", None).unwrap_or_else(|e| panic!("{text}: {e}"));
            let mut want = crate::jsonc::parse(if text.is_empty() { "{}" } else { text }).unwrap();
            add_bro_profile(&mut want, "bro", None);
            assert_eq!(crate::jsonc::parse(&plan.text).unwrap(), want, "{text}");
            if text.contains("//") || text.contains("/*") {
                assert!(plan.text.contains("nothing yet") || plan.text.contains("/* none */"));
            }
        }
        assert!(plan("{ not json", "bro", None).is_err());

        // CRLF in, CRLF out: no bare newline is spliced in.
        let crlf = SETTINGS.replace('\n', "\r\n");
        let plan = plan(&crlf, "bro", None).unwrap();
        assert_eq!(plan.changes.len(), 2);
        assert!(
            !plan.text.replace("\r\n", "").contains('\n'),
            "{}",
            plan.text
        );
    }
}
