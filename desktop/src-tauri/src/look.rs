//! The window's look, Settings > Desktop > Appearance: VS Code's theme
//! followed live, a VS Code theme imported (an installed one, or a file),
//! VS Code's fonts taken, the window's zoom, and Kete's own look back.
//!
//! The reading is `nebula_config::vscode`'s and the mapping onto the
//! window's colour tokens `nebula_config::look`'s, shared with the TUI;
//! this only runs them off the async runtime and writes the result into
//! the `desktop` preferences. An imported theme is stored as the palette
//! it maps to, never as the file or the path it came from.

use crate::Res;
use nebula_config::desktop::{self, DesktopPrefs};
use nebula_config::look::{self, Palette};
use nebula_config::vscode::{self, Rgba};
use serde::Serialize;
use std::time::UNIX_EPOCH;
use tauri::Manager;
use tauri_plugin_dialog::DialogExt;

/// What the `vscode` theme wears now, for the window to paint.
#[derive(Debug, Serialize)]
pub struct VsCodeLook {
    /// VS Code's settings.json's modified time, in ms: the window asks
    /// again with it, and gets `changed: false` while it holds.
    pub stamp: Option<u64>,
    /// Something moved since the stamp the window held.
    pub changed: bool,
    /// The palette, when VS Code and its theme were found.
    pub palette: Option<Palette>,
    /// Why there is none: VS Code isn't installed, or its theme isn't.
    pub problem: Option<String>,
}

fn stamp_of(install: &vscode::Install) -> Option<u64> {
    let modified = std::fs::metadata(&install.settings).ok()?.modified().ok()?;
    Some(modified.duration_since(UNIX_EPOCH).ok()?.as_millis() as u64)
}

/// The palette VS Code's current theme maps to, unless `known` is the
/// stamp the window already painted from. `os_light` is what the OS says,
/// for VS Code's own auto-detect.
#[tauri::command]
pub async fn look_vscode(os_light: bool, known: Option<u64>) -> VsCodeLook {
    tauri::async_runtime::spawn_blocking(move || vscode_look(os_light, known))
        .await
        .unwrap_or(VsCodeLook {
            stamp: None,
            changed: false,
            palette: None,
            problem: Some("the read did not finish".into()),
        })
}

fn vscode_look(os_light: bool, known: Option<u64>) -> VsCodeLook {
    let Some(install) = vscode::install() else {
        return VsCodeLook {
            stamp: None,
            changed: known.is_some(),
            palette: None,
            problem: Some("VS Code's settings weren't found: One Dark until they are".into()),
        };
    };
    let stamp = stamp_of(&install);
    if known.is_some() && known == stamp {
        return VsCodeLook {
            stamp,
            changed: false,
            palette: None,
            problem: None,
        };
    }
    let settings = vscode::read_settings(&install).unwrap_or(serde_json::Value::Null);
    let wanted = vscode::chosen_theme(&settings, Some(os_light));
    match vscode::current_in(&install, &settings, Some(os_light)) {
        Some(shown) => VsCodeLook {
            stamp,
            changed: true,
            palette: Some(look::from_vscode(
                &shown.data,
                shown.file.light,
                &shown.file.label,
            )),
            problem: None,
        },
        None => VsCodeLook {
            stamp,
            changed: true,
            palette: None,
            problem: Some(format!(
                "{} is set to {wanted}, which isn't installed where Kete looks: One Dark until it is",
                install.name
            )),
        },
    }
}

/// An installed theme, for the import list.
#[derive(Debug, Serialize)]
pub struct InstalledTheme {
    /// What settings and the import call it by.
    pub name: String,
    pub label: String,
    pub light: bool,
    pub extension: String,
    pub builtin: bool,
    /// Its canvas, text and four syntax colours, for the swatches.
    pub swatch: Vec<String>,
}

/// Which VS Code the look reads, by name; None when none is found. The
/// import list and "Use VS Code's fonts" say so rather than fail.
#[tauri::command]
pub fn look_vscode_install() -> Option<String> {
    vscode::install().map(|i| i.name.to_string())
}

/// Every installed VS Code theme with its swatches, the user's first.
#[tauri::command]
pub async fn look_installed_themes() -> Vec<InstalledTheme> {
    tauri::async_runtime::spawn_blocking(installed_themes)
        .await
        .unwrap_or_default()
}

fn installed_themes() -> Vec<InstalledTheme> {
    vscode::installed_themes()
        .into_iter()
        .filter_map(|file| {
            let data = vscode::load_theme(&file.path)?;
            let palette = look::from_vscode(&data, file.light, &file.label);
            let swatch = [
                "color-bg",
                "color-ink",
                "color-syn-keyword",
                "color-syn-string",
                "color-syn-function",
                "color-syn-number",
            ]
            .iter()
            .filter_map(|k| palette.tokens.get(*k).cloned())
            .collect();
            Some(InstalledTheme {
                name: file.settings_id().to_string(),
                label: file.label,
                light: file.light,
                extension: file.extension,
                builtin: file.builtin,
                swatch,
            })
        })
        .collect()
}

/// Keep `palette` as an imported theme and wear it.
fn keep(palette: Palette) -> Res<DesktopPrefs> {
    DesktopPrefs::update(|p| {
        let picked = p.import_theme(palette)?;
        p.set("theme", &picked)
    })
}

/// Import the installed theme `name` (an id or a label), as VS Code
/// would show it, and wear it.
#[tauri::command]
pub async fn look_import_installed(name: String) -> Res<DesktopPrefs> {
    tauri::async_runtime::spawn_blocking(move || {
        let shown = vscode::named(&name).ok_or_else(|| format!("{name} isn't installed"))?;
        keep(look::from_vscode(
            &shown.data,
            shown.file.light,
            &shown.file.label,
        ))
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Pick a theme file in the open dialog, import it and wear it; None
/// when the dialog was cancelled.
#[tauri::command]
pub async fn look_import_file(app: tauri::AppHandle) -> Res<Option<DesktopPrefs>> {
    tauri::async_runtime::spawn_blocking(move || {
        let Some(picked) = app
            .dialog()
            .file()
            .set_title("Import a VS Code theme")
            .add_filter("VS Code theme", &["json", "jsonc"])
            .blocking_pick_file()
        else {
            return Ok(None);
        };
        let path = picked.into_path().map_err(|e| e.to_string())?;
        keep(palette_of_file(&path)?).map(Some)
    })
    .await
    .map_err(|e| e.to_string())?
}

/// A theme file picked by hand, as a palette: its `include` chain
/// followed, its lightness from its `type` (else its canvas), its name
/// from its `name` (else the file's).
fn palette_of_file(path: &std::path::Path) -> Res<Palette> {
    let data = vscode::load_theme(path).ok_or("that file isn't a VS Code theme (JSON with comments, at most 4 MiB)")?;
    if data.colors.is_empty() && data.tokens.is_empty() {
        return Err("that file has no colors or tokenColors: not a VS Code theme".into());
    }
    let light = data.kind_light().unwrap_or_else(|| {
        data.color("editor.background")
            .is_some_and(|bg| look::luminance(bg.over(Rgba::rgb(0, 0, 0))) > 0.4)
    });
    let name = nebula_config::jsonc::read(path)
        .and_then(|v| v.get("name").and_then(|n| n.as_str()).map(str::to_string))
        .filter(|n| !n.trim().is_empty())
        .unwrap_or_else(|| {
            path.file_stem()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_else(|| "Imported theme".into())
        });
    Ok(look::from_vscode(&data, light, &name))
}

/// Forget the imported theme `id`.
#[tauri::command]
pub fn look_remove_theme(id: String) -> Res<DesktopPrefs> {
    DesktopPrefs::update(|p| p.remove_theme(&id))
}

/// Kete's own look back (the imported themes stay).
#[tauri::command]
pub fn look_reset() -> Res<DesktopPrefs> {
    DesktopPrefs::update(|p| {
        p.reset_look();
        Ok(())
    })
}

/// What "Use VS Code's fonts" did.
#[derive(Debug, Serialize)]
pub struct FontsTaken {
    pub prefs: DesktopPrefs,
    /// One line: what was taken, for the toast.
    pub said: String,
}

/// Take VS Code's fonts: the editor's family for code in the window, the
/// terminal's (the editor's where unset) for the terminals and the diff,
/// their sizes to the nearest the window offers.
#[tauri::command]
pub fn look_use_vscode_fonts() -> Res<FontsTaken> {
    let install = vscode::install().ok_or("VS Code's settings weren't found")?;
    let settings = vscode::read_settings(&install).ok_or("VS Code's settings.json can't be read")?;
    let fonts = vscode::fonts(&settings);
    let mut said = Vec::new();
    let prefs = DesktopPrefs::update(|p| take_fonts(p, &fonts, &mut said))?;
    let said = if said.is_empty() {
        format!("{} sets no fonts of its own: nothing changed", install.name)
    } else {
        format!("From {}: {}", install.name, said.join(", "))
    };
    Ok(FontsTaken { prefs, said })
}

/// Lay `fonts` onto `p`, saying what each took.
fn take_fonts(p: &mut DesktopPrefs, fonts: &vscode::Fonts, said: &mut Vec<String>) -> Res<()> {
    said.clear();
    let usable = |f: &Option<String>| f.clone().filter(|f| look::font_family_ok(f));
    if let Some(family) = usable(&fonts.editor_family) {
        p.set("font_mono", &family)?;
        said.push(format!("code font {family}"));
    }
    if let Some(family) = usable(&fonts.terminal_family) {
        p.set("font_term", &family)?;
        said.push(format!("terminal font {family}"));
    }
    if let Some(size) = fonts.terminal_size {
        let near = nearest(desktop::TERM_SIZES, size);
        p.set("term_size", near)?;
        said.push(format!("terminal size {near}"));
    }
    if let Some(size) = fonts.editor_size {
        let near = nearest(desktop::EDITOR_SIZES, size);
        p.set("editor_size", near)?;
        said.push(format!("diff size {near}"));
    }
    Ok(())
}

/// The size on `list` nearest `want`.
fn nearest(list: &[&'static str], want: f64) -> &'static str {
    list.iter()
        .copied()
        .min_by(|a, b| {
            let d = |s: &str| (s.parse::<f64>().unwrap_or(0.0) - want).abs();
            d(a).total_cmp(&d(b))
        })
        .unwrap_or(list[0])
}

/// Scale the window: the Interface size over the window's own 14.
#[tauri::command]
pub fn look_zoom(app: tauri::AppHandle, scale: f64) -> Res<()> {
    let scale = if scale.is_finite() {
        scale.clamp(0.5, 2.0)
    } else {
        1.0
    };
    let window = app
        .get_webview_window("main")
        .ok_or("the main window isn't there")?;
    window.set_zoom(scale).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_size_rounds_to_the_nearest_offered() {
        assert_eq!(nearest(desktop::TERM_SIZES, 15.0), "15");
        assert_eq!(nearest(desktop::TERM_SIZES, 30.0), "20");
        assert_eq!(nearest(desktop::EDITOR_SIZES, 12.4), "12.5");
        assert_eq!(nearest(desktop::EDITOR_SIZES, 6.0), "10");
    }

    /// Jay's VS Code: JetBrains Mono 15 for the editor and, unset, the
    /// terminal too.
    #[test]
    fn vs_code_s_fonts_land_on_the_code_and_terminal_rows() {
        let fonts = vscode::fonts(&serde_json::json!({
            "editor.fontFamily": "'JetBrains Mono', 'monospace', monospace",
            "editor.fontSize": 15,
        }));
        let mut p = DesktopPrefs::default();
        let mut said = Vec::new();
        take_fonts(&mut p, &fonts, &mut said).unwrap();
        assert_eq!(p.font_mono, "JetBrains Mono");
        assert_eq!(p.font_term, "JetBrains Mono");
        assert_eq!((p.term_size.as_str(), p.editor_size.as_str()), ("15", "15"));
        assert_eq!(said.len(), 4);
    }

    /// A theme file picked by hand: its `name`, its `type`, its include.
    #[test]
    fn a_theme_file_becomes_a_palette_by_its_own_name_and_type() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("base.json"),
            r##"{ "colors": { "editor.foreground": "#222222" } }"##,
        )
        .unwrap();
        let file = dir.path().join("paper.json");
        std::fs::write(
            &file,
            r##"{ "name": "Paper", "type": "light", "include": "./base.json",
                 "colors": { "editor.background": "#fdfdf8" } }"##,
        )
        .unwrap();
        let p = palette_of_file(&file).unwrap();
        assert_eq!(p.label, "Paper");
        assert!(p.light);
        assert_eq!(p.tokens["color-ink"], "#222222");
        let junk = dir.path().join("junk.json");
        std::fs::write(&junk, "{}").unwrap();
        assert!(palette_of_file(&junk).is_err());
    }
}
