//! The installed font families, for the font pickers of Settings > Desktop >
//! Appearance. The window's CSP keeps `font-src` at `'self' data:`, so a page
//! can only use a family that is installed on this machine or one the app
//! bundles; this lists the installed ones, so a picker offers names that
//! render, and marks the fixed-pitch ones, so a picker for a terminal or code
//! font can offer those first.
//!
//! Windows asks DirectWrite for its system font collection: the family
//! grouping GDI and WebView2 match in CSS, where "Segoe UI" and "Segoe UI
//! Light" are two families. A family is `mono` there when its regular face
//! reports itself monospaced. Elsewhere `fc-list` is read (`parse_fc_list`),
//! and a machine without it, macOS for one, lists none.
//!
//! The list is read once per process (`system_families`); fonts installed
//! while Kete runs show after a restart. Nothing here fails loudly: a read
//! that goes wrong lists what it had gathered, or nothing.

#[cfg(windows)]
use windows::Win32::Graphics::DirectWrite::IDWriteFontFamily;

use std::sync::OnceLock;

/// One installed font family.
#[derive(Debug, Clone, serde::Serialize, PartialEq, Eq)]
pub struct FontFamily {
    /// The family name CSS can match (`font-family: "<name>"`).
    pub name: String,
    /// Every face of it is fixed-pitch (a terminal or code font).
    pub mono: bool,
}

static FAMILIES: OnceLock<Vec<FontFamily>> = OnceLock::new();

/// The installed font families, sorted case-insensitively by name and
/// deduplicated, read once per process: later calls return the cached list.
pub fn system_families() -> Vec<FontFamily> {
    FAMILIES.get_or_init(read).clone()
}

/// `system_fonts`: what the font pickers list. Runs the read off the async
/// runtime.
#[tauri::command]
pub async fn system_fonts() -> Vec<FontFamily> {
    tauri::async_runtime::spawn_blocking(system_families)
        .await
        .unwrap_or_default()
}

/// Sorted case-insensitively (exact name breaks a tie), empty names dropped,
/// one entry per exact name: a family listed several times is mono only if
/// every listing says so.
fn tidy(mut families: Vec<FontFamily>) -> Vec<FontFamily> {
    families.retain(|f| !f.name.is_empty());
    families.sort_by_cached_key(|f| (f.name.to_lowercase(), f.name.clone()));
    families.dedup_by(|later, kept| {
        if later.name == kept.name {
            kept.mono &= later.mono;
            true
        } else {
            false
        }
    });
    families
}

/// `fc-list --format '%{family[0]}\t%{spacing}\n'` output as families: one
/// face per line, the family before the tab, fontconfig's spacing after it
/// (100 mono and 90 dual are fixed-pitch; 0, nothing, or no tab is not).
#[cfg_attr(windows, allow(dead_code))]
pub fn parse_fc_list(text: &str) -> Vec<FontFamily> {
    let faces = text
        .lines()
        .filter_map(|line| {
            let (name, spacing) = line.split_once('\t').unwrap_or((line, ""));
            let name = name.trim();
            if name.is_empty() {
                return None;
            }
            // `100`, or `100(i)(s)` on a fontconfig that prints its constants.
            let digits: String = spacing
                .trim()
                .chars()
                .take_while(char::is_ascii_digit)
                .collect();
            Some(FontFamily {
                name: name.to_string(),
                mono: matches!(digits.as_str(), "100" | "90"),
            })
        })
        .collect();
    tidy(faces)
}

#[cfg(not(windows))]
fn read() -> Vec<FontFamily> {
    std::process::Command::new("fc-list")
        .args(["--format", r"%{family[0]}\t%{spacing}\n"])
        .stderr(std::process::Stdio::null())
        .output()
        .ok()
        .filter(|out| out.status.success())
        .map(|out| parse_fc_list(&String::from_utf8_lossy(&out.stdout)))
        .unwrap_or_default()
}

#[cfg(windows)]
fn read() -> Vec<FontFamily> {
    let mut found = Vec::new();
    // A failure partway keeps what was gathered.
    let _ = gather(&mut found);
    tidy(found)
}

/// Every family of DirectWrite's system collection, appended to `found`. A
/// family whose name can't be read is skipped; a failure of the factory or
/// the collection ends the read.
#[cfg(windows)]
fn gather(found: &mut Vec<FontFamily>) -> windows::core::Result<()> {
    use windows::Win32::Graphics::DirectWrite::{
        DWriteCreateFactory, IDWriteFactory, IDWriteFontCollection, DWRITE_FACTORY_TYPE_SHARED,
    };

    // SAFETY: a factory type in, an owned interface out; DirectWrite needs no COM setup.
    let factory: IDWriteFactory = unsafe { DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED)? };
    let mut collection: Option<IDWriteFontCollection> = None;
    // SAFETY: `collection` is a valid out-pointer for the whole call.
    unsafe { factory.GetSystemFontCollection(&mut collection, false)? };
    let Some(collection) = collection else {
        return Ok(());
    };
    // SAFETY: a plain getter on a live collection.
    let count = unsafe { collection.GetFontFamilyCount() };
    for index in 0..count {
        // SAFETY: `index` is below the count read above.
        let Ok(family) = (unsafe { collection.GetFontFamily(index) }) else {
            continue;
        };
        let Some(name) = family_name(&family) else {
            continue;
        };
        found.push(FontFamily {
            mono: is_mono(&family),
            name,
        });
    }
    Ok(())
}

/// The family's "en-us" name, else its first; `None` when it has none or an
/// empty one.
#[cfg(windows)]
fn family_name(family: &IDWriteFontFamily) -> Option<String> {
    use windows::core::{w, BOOL};

    // SAFETY: a plain getter on a live family.
    let names = unsafe { family.GetFamilyNames() }.ok()?;
    let mut index = 0u32;
    let mut exists = BOOL(0);
    // SAFETY: both out-pointers are valid for the call and the locale is a NUL-terminated literal.
    let found = unsafe { names.FindLocaleName(w!("en-us"), &mut index, &mut exists) };
    if found.is_err() || !exists.as_bool() {
        // A miss leaves `index` unset (DirectWrite's UINT_MAX): take the first name.
        index = 0;
    }
    // SAFETY: a plain getter; an index outside the list is an error, not a read.
    let len = unsafe { names.GetStringLength(index) }.ok()? as usize;
    // The buffer size DirectWrite checks counts the terminating NUL.
    let mut buf = vec![0u16; len + 1];
    // SAFETY: `buf` is writable for the `len + 1` units it reports to DirectWrite.
    unsafe { names.GetString(index, &mut buf) }.ok()?;
    let name = String::from_utf16_lossy(&buf[..len]);
    (!name.is_empty()).then_some(name)
}

/// Whether the family's regular face reports itself monospaced; false when
/// it has no such face or doesn't answer.
#[cfg(windows)]
fn is_mono(family: &IDWriteFontFamily) -> bool {
    use windows::core::Interface;
    use windows::Win32::Graphics::DirectWrite::{
        IDWriteFont1, DWRITE_FONT_STRETCH_NORMAL, DWRITE_FONT_STYLE_NORMAL,
        DWRITE_FONT_WEIGHT_NORMAL,
    };

    // SAFETY: a plain getter on a live family; it returns an owned font.
    let Ok(font) = (unsafe {
        family.GetFirstMatchingFont(
            DWRITE_FONT_WEIGHT_NORMAL,
            DWRITE_FONT_STRETCH_NORMAL,
            DWRITE_FONT_STYLE_NORMAL,
        )
    }) else {
        return false;
    };
    let Ok(font) = font.cast::<IDWriteFont1>() else {
        return false;
    };
    // SAFETY: a plain getter on a live font.
    unsafe { font.IsMonospacedFont() }.as_bool()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn family(name: &str, mono: bool) -> FontFamily {
        FontFamily {
            name: name.to_string(),
            mono,
        }
    }

    #[test]
    fn spacing_100_and_90_are_mono_and_the_rest_are_not() {
        let list = parse_fc_list("Mono\t100\nDual\t90\nSans\t0\nCell\t110\n");
        assert_eq!(
            list,
            vec![
                family("Cell", false),
                family("Dual", true),
                family("Mono", true),
                family("Sans", false),
            ]
        );
    }

    #[test]
    fn a_family_is_mono_only_when_every_face_is() {
        let list = parse_fc_list("Odd\t100\nOdd\t0\nSteady\t100\nSteady\t100\nOdd\t100\n");
        assert_eq!(list, vec![family("Odd", false), family("Steady", true)]);
    }

    #[test]
    fn a_line_without_a_tab_or_a_spacing_is_not_mono() {
        let list = parse_fc_list("No Tab\nNo Spacing\t\nSpaced Out\t 100 \n");
        assert_eq!(
            list,
            vec![
                family("No Spacing", false),
                family("No Tab", false),
                family("Spaced Out", true),
            ]
        );
    }

    #[test]
    fn a_spacing_printed_with_its_constant_still_counts() {
        let list = parse_fc_list("Mono\t100(i)(s)\nSans\t0(i)(s)\n");
        assert_eq!(list, vec![family("Mono", true), family("Sans", false)]);
    }

    #[test]
    fn blank_lines_and_empty_names_are_skipped() {
        let list = parse_fc_list("\n   \n\t100\n  \t0\nReal\t100\n\n");
        assert_eq!(list, vec![family("Real", true)]);
    }

    #[test]
    fn a_family_listed_once_per_face_is_one_family() {
        let text = "Inter\t0\nInter\t0\nInter\t0\nFira Code\t100\nFira Code\t100\n";
        let list = parse_fc_list(text);
        assert_eq!(
            list,
            vec![family("Fira Code", true), family("Inter", false)]
        );
    }

    #[test]
    fn families_sort_case_insensitively() {
        let list = parse_fc_list("zeta\t0\nAlpha\t0\nbeta\t0\nGamma\t0\n");
        let names: Vec<&str> = list.iter().map(|f| f.name.as_str()).collect();
        assert_eq!(names, ["Alpha", "beta", "Gamma", "zeta"]);
    }

    #[test]
    fn names_are_trimmed_and_windows_line_endings_are_read() {
        let list = parse_fc_list("  Spaced  \t100\r\nSans\t0\r\n");
        assert_eq!(list, vec![family("Sans", false), family("Spaced", true)]);
    }

    #[test]
    fn an_empty_listing_is_no_families() {
        assert!(parse_fc_list("").is_empty());
    }

    #[cfg(windows)]
    #[test]
    fn the_windows_list_is_sorted_and_unique_and_knows_consolas_from_segoe_ui() {
        let list = system_families();
        assert!(!list.is_empty());
        assert!(list
            .windows(2)
            .all(|pair| pair[0].name.to_lowercase() <= pair[1].name.to_lowercase()));
        let unique: std::collections::HashSet<&str> =
            list.iter().map(|f| f.name.as_str()).collect();
        assert_eq!(unique.len(), list.len());
        let mono_of = |name: &str| list.iter().find(|f| f.name == name).map(|f| f.mono);
        assert_eq!(mono_of("Consolas"), Some(true));
        assert_eq!(mono_of("Segoe UI"), Some(false));
        assert_eq!(system_families(), list);
    }
}
