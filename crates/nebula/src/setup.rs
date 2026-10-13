//! `kete setup terminal`: make Windows Terminal ready for Kete — the font
//! the FILE ICONS' Nerd Font glyphs need, and a **Kete** profile that runs
//! `kete` in it, under Kete's icon and the Kete Workshop color scheme. A **Bro**
//! profile an earlier `bro setup terminal` made becomes the Kete one.
//!
//! 1. **The font.** JetBrainsMono Nerd Font (SIL OFL 1.1), from the
//!    latest nerd-fonts GitHub release, installed for this user only: the
//!    `.ttf`s copied into `%LOCALAPPDATA%\Microsoft\Windows\Fonts` and
//!    registered under `HKCU\Software\Microsoft\Windows NT\CurrentVersion\
//!    Fonts`, which needs no administrator. Skipped when it is there
//!    (per user or for the machine), or with `--no-font`.
//! 2. **The icon.** The `kete.ico` beside `kete.exe` when the install put
//!    one there; else Kete's 256px PNG, carried in the binary, written into
//!    Kete's data dir — Windows Terminal wants a file path.
//! 3. **The profile.** Windows Terminal's `settings.json` gets the profile
//!    and the scheme (`nebula_tui::wt_settings::plan`), spliced into the
//!    file so its comments and layout stay, after it is backed up beside
//!    itself. A second run finds them there and writes nothing.
//!
//! `--dry-run` says all of it — the diff of `settings.json` included — and
//! does none of it. The hidden `--wt-settings <PATH>` points the profile
//! step at another file: a copy, to try it on, as the tests do. Off
//! Windows there is no Windows Terminal; it says what to do instead.

use anyhow::{bail, Context, Result};
use nebula_tui::wt_settings;
use std::path::{Path, PathBuf};
use std::process::Command;

/// The font archive: nerd-fonts' latest release, the JetBrainsMono family.
const FONT_URL: &str =
    "https://github.com/ryanoasis/nerd-fonts/releases/latest/download/JetBrainsMono.zip";
/// The font files of the family's main (not Mono, not Propo) variant.
const FONT_PREFIX: &str = "JetBrainsMonoNerdFont-";
/// Where a per-user font install keeps its fonts and their names.
const FONTS_KEY: &str = r"HKCU\Software\Microsoft\Windows NT\CurrentVersion\Fonts";
/// Kete's icon at 256px, for a profile when no `kete.ico` sits beside the
/// binary (a `cargo install`).
const ICON_PNG: &[u8] = include_bytes!("../../../assets/brand/kete/icons/kete-256.png");

pub(crate) struct Options {
    pub dry_run: bool,
    pub no_font: bool,
    pub wt_settings: Option<PathBuf>,
}

pub(crate) fn run_terminal(opts: Options) -> Result<()> {
    if !cfg!(windows) && opts.wt_settings.is_none() {
        println!(
            "kete setup terminal sets up Windows Terminal, which this OS doesn't have. \
             Instead:\n  1. install a Nerd Font (https://www.nerdfonts.com: JetBrainsMono \
             Nerd Font matches Kete's own), from your package manager or by hand;\n  \
             2. set your terminal's font to it (and a dark warm color scheme, if it has \
             one, to match the kete-dark theme);\n  3. in Kete, Settings → Appearance → \
             File icons → nerd."
        );
        return Ok(());
    }
    let say = if opts.dry_run { "would" } else { "will" };
    if opts.dry_run {
        println!("dry run: nothing is downloaded, installed or written.\n");
    }
    if opts.no_font {
        println!("font: skipped (--no-font)");
    } else if !cfg!(windows) {
        println!("font: skipped (not Windows)");
    } else {
        install_font(opts.dry_run, say)?;
    }
    let path = match opts.wt_settings.clone().or_else(wt_settings::path) {
        Some(p) => p,
        None => bail!("couldn't find Windows Terminal's settings.json"),
    };
    let icon = profile_icon(opts.dry_run, say)?;
    let wrote = add_profile(&path, &commandline(), icon.as_deref(), opts.dry_run, say)?;
    if !opts.dry_run && wrote {
        println!(
            "\ndone: restart Windows Terminal and open a tab on the \"{}\" profile; Kete's file \
             icons are glyphs there. If it says the font isn't found, sign out and back in: \
             Windows may only load a newly installed font at the next sign-in.",
            wt_settings::PROFILE_NAME
        );
    }
    Ok(())
}

/// The `kete` the profile runs: this binary, by its full path.
fn commandline() -> String {
    std::env::current_exe()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|_| "kete".into())
}

/// `%LOCALAPPDATA%\Microsoft\Windows\Fonts`.
fn fonts_dir() -> Result<PathBuf> {
    let local = std::env::var_os("LOCALAPPDATA").context("LOCALAPPDATA is not set")?;
    Ok(PathBuf::from(local).join(r"Microsoft\Windows\Fonts"))
}

/// Where the font is installed already, per user or for the machine.
fn installed_font() -> Option<PathBuf> {
    let file = format!("{FONT_PREFIX}Regular.ttf");
    let mut dirs: Vec<PathBuf> = fonts_dir().into_iter().collect();
    if let Some(windir) = std::env::var_os("WINDIR") {
        dirs.push(PathBuf::from(windir).join("Fonts"));
    }
    dirs.into_iter()
        .map(|d| d.join(&file))
        .find(|p| p.is_file())
}

fn install_font(dry_run: bool, say: &str) -> Result<()> {
    if let Some(at) = installed_font() {
        println!(
            "font: {} is installed already ({})",
            wt_settings::FONT_FACE,
            at.display()
        );
        return Ok(());
    }
    let dir = fonts_dir()?;
    println!("font: {say} download {FONT_URL}");
    println!(
        "font: {say} install its {FONT_PREFIX}*.ttf files for this user into {}",
        dir.display()
    );
    println!("font: {say} register them under {FONTS_KEY}");
    if dry_run {
        return Ok(());
    }
    let work = std::env::temp_dir().join(format!("kete-nerd-font-{}", std::process::id()));
    std::fs::create_dir_all(&work)?;
    let result = download_and_install(&work, &dir);
    let _ = std::fs::remove_dir_all(&work);
    let installed = result?;
    println!("font: installed {installed} font files");
    Ok(())
}

fn download_and_install(work: &Path, dir: &Path) -> Result<usize> {
    let zip = work.join("JetBrainsMono.zip");
    let status = Command::new("curl.exe")
        .args(["-fsSL", "-o"])
        .arg(&zip)
        .arg(FONT_URL)
        .status()
        .context("running curl.exe")?;
    if !status.success() {
        bail!("downloading the font failed ({status})");
    }
    let unpacked = work.join("x");
    std::fs::create_dir_all(&unpacked)?;
    // Windows 10 1803+ ships bsdtar, which reads zips.
    let status = Command::new("tar")
        .arg("-xf")
        .arg(&zip)
        .arg("-C")
        .arg(&unpacked)
        .status()
        .context("running tar")?;
    if !status.success() {
        bail!("unpacking the font failed ({status})");
    }
    std::fs::create_dir_all(dir)?;
    let mut installed = 0;
    for entry in std::fs::read_dir(&unpacked)? {
        let path = entry?.path();
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        if !name.starts_with(FONT_PREFIX) || !name.ends_with(".ttf") {
            continue;
        }
        let to = dir.join(name);
        std::fs::copy(&path, &to).with_context(|| format!("copying {name}"))?;
        let status = Command::new("reg")
            .args([
                "add",
                FONTS_KEY,
                "/v",
                &font_value_name(name),
                "/t",
                "REG_SZ",
                "/d",
            ])
            .arg(&to)
            .arg("/f")
            .stdout(std::process::Stdio::null())
            .status()
            .context("running reg")?;
        if !status.success() {
            bail!("registering {name} failed ({status})");
        }
        installed += 1;
    }
    if installed == 0 {
        bail!("the archive had no {FONT_PREFIX}*.ttf files");
    }
    Ok(installed)
}

/// The registry value a font file is registered under, as the Fonts
/// control panel names it: `JetBrainsMonoNerdFont-BoldItalic.ttf` →
/// `JetBrainsMono Nerd Font Bold Italic (TrueType)`.
fn font_value_name(file: &str) -> String {
    let style = file
        .trim_start_matches(FONT_PREFIX)
        .trim_end_matches(".ttf");
    format!("{} {} (TrueType)", wt_settings::FONT_FACE, spaced(style))
}

/// `BoldItalic` → `Bold Italic`.
fn spaced(style: &str) -> String {
    let mut out = String::new();
    for (i, c) in style.chars().enumerate() {
        if i > 0 && c.is_uppercase() {
            out.push(' ');
        }
        out.push(c);
    }
    out
}

/// The profile's icon, an absolute path: the `kete.ico` beside the binary,
/// else the PNG carried in it, written into the data dir (on a dry run,
/// only said). None when neither can be had — the profile then goes
/// without.
fn profile_icon(dry_run: bool, say: &str) -> Result<Option<String>> {
    let beside = std::env::current_exe()
        .ok()
        .and_then(|exe| Some(exe.parent()?.join("kete.ico")))
        .filter(|p| p.is_file());
    if let Some(ico) = beside {
        return Ok(Some(ico.display().to_string()));
    }
    let png = nebula_core::paths::data_dir().join("kete-256.png");
    if std::fs::read(&png).is_ok_and(|have| have == ICON_PNG) {
        return Ok(Some(png.display().to_string()));
    }
    println!("icon: {say} write Kete's icon to {}", png.display());
    if !dry_run {
        if let Some(dir) = png.parent() {
            std::fs::create_dir_all(dir)?;
        }
        std::fs::write(&png, ICON_PNG).with_context(|| format!("writing {}", png.display()))?;
    }
    Ok(Some(png.display().to_string()))
}

/// The profile step on the settings file at `path` (missing: made).
/// Returns whether there was anything to do.
fn add_profile(
    path: &Path,
    commandline: &str,
    icon: Option<&str>,
    dry_run: bool,
    say: &str,
) -> Result<bool> {
    let text = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(e) => return Err(e).with_context(|| format!("reading {}", path.display())),
    };
    let plan = wt_settings::plan(&text, commandline, icon)
        .map_err(|e| anyhow::anyhow!("{}: {e}; left it as it is", path.display()))?;
    if plan.changes.is_empty() {
        println!(
            "profile: {} has the \"{}\" profile already; nothing to change",
            path.display(),
            wt_settings::PROFILE_NAME
        );
        return Ok(false);
    }
    let backup = backup_path(path);
    if path.is_file() {
        println!(
            "profile: {say} back {} up to {}",
            path.display(),
            backup.display()
        );
    }
    for change in &plan.changes {
        println!("profile: {say} {change}");
    }
    if dry_run {
        println!(
            "\n{}",
            wt_settings::unified_diff(&text, &plan.text, &path.display().to_string())
        );
        return Ok(true);
    }
    if path.is_file() {
        std::fs::copy(path, &backup).with_context(|| format!("backing up {}", path.display()))?;
    }
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::write(path, &plan.text).with_context(|| format!("writing {}", path.display()))?;
    println!("profile: written to {}", path.display());
    Ok(true)
}

/// `settings.json` → `settings.json.kete-backup-<unix seconds>`.
fn backup_path(path: &Path) -> PathBuf {
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or_default();
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(format!(".kete-backup-{stamp}"));
    path.with_file_name(name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn font_files_register_under_their_display_names() {
        assert_eq!(spaced("BoldItalic"), "Bold Italic");
        assert_eq!(spaced("Regular"), "Regular");
        assert_eq!(spaced("ExtraLightItalic"), "Extra Light Italic");
        assert_eq!(
            font_value_name("JetBrainsMonoNerdFont-BoldItalic.ttf"),
            "JetBrainsMono Nerd Font Bold Italic (TrueType)"
        );
    }

    /// Against a copy: a dry run writes nothing; the real run backs the
    /// file up and splices the profile in, comments kept; a second run
    /// writes nothing.
    #[test]
    fn the_profile_step_backs_up_and_writes_a_copy() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        let original =
            "{\n    // mine\n    \"profiles\": { \"list\": [ { \"name\": \"pwsh\" }, ] },\n}\n";
        std::fs::write(&path, original).unwrap();
        let kete = r"C:\bin\kete.exe";
        assert!(add_profile(&path, kete, Some(r"C:\bin\kete.ico"), true, "would").unwrap());
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            original,
            "a dry run writes nothing"
        );
        assert!(add_profile(&path, kete, Some(r"C:\bin\kete.ico"), false, "will").unwrap());
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains("// mine"), "comments kept:\n{text}");
        let written = nebula_tui::jsonc::parse(&text).unwrap();
        assert_eq!(
            written["profiles"]["list"][1]["name"],
            wt_settings::PROFILE_NAME
        );
        assert_eq!(written["profiles"]["list"][1]["icon"], r"C:\bin\kete.ico");
        let backups: Vec<_> = std::fs::read_dir(dir.path())
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name().to_string_lossy().contains(".kete-backup-"))
            .collect();
        assert_eq!(backups.len(), 1);
        assert_eq!(
            std::fs::read_to_string(backups[0].path()).unwrap(),
            original
        );
        assert!(!add_profile(&path, kete, Some(r"C:\bin\kete.ico"), false, "will").unwrap());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), text);
    }

    /// A file that is not JSONC is left alone, with an error that says so.
    #[test]
    fn a_broken_settings_file_is_left_alone() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        std::fs::write(&path, "{ broken").unwrap();
        assert!(add_profile(&path, "kete", None, false, "will").is_err());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "{ broken");
    }
}
