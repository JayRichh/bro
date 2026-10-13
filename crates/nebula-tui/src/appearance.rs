//! LIGHT/DARK AWARENESS: what the OS says about its app theme, and the
//! LIVE THEME that follows it. The `auto` and `vscode` theme names
//! (`crate::theme::resolve`) are the two that read the machine; this is
//! where the reading happens, so the draw path never does any.
//!
//! - **Windows:** `HKCU\Software\Microsoft\Windows\CurrentVersion\Themes\
//!   Personalize`, value `AppsUseLightTheme` (a DWORD, 1 for light) — the
//!   switch Settings → Personalization → Colors → *Choose your app mode*
//!   flips. One registry read, microseconds.
//! - **macOS:** `defaults read -g AppleInterfaceStyle` prints `Dark` in
//!   dark mode and fails in light mode.
//! - **Linux:** `gsettings get org.gnome.desktop.interface color-scheme`
//!   (`'prefer-dark'` / `'prefer-light'`/`'default'`) where GNOME is there
//!   to ask; elsewhere the answer is unknown and `auto` stays dark.
//!
//! The event loop re-checks on a beat while a live theme is picked, and
//! when the window takes focus again ([`LiveTheme::recheck`]).

use crate::theme::{Machine, Theme};

/// The OS app theme is light: Some(true) light, Some(false) dark, None
/// where the OS cannot be asked.
pub fn os_prefers_light() -> Option<bool> {
    imp::os_prefers_light()
}

#[cfg(windows)]
mod imp {
    use windows_sys::Win32::System::Registry::{RegGetValueW, HKEY_CURRENT_USER, RRF_RT_REG_DWORD};

    pub fn os_prefers_light() -> Option<bool> {
        let wide = |s: &str| s.encode_utf16().chain(Some(0)).collect::<Vec<u16>>();
        let key = wide(r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize");
        let value = wide("AppsUseLightTheme");
        let mut data: u32 = 0;
        let mut size = std::mem::size_of::<u32>() as u32;
        // SAFETY: both names are NUL-terminated UTF-16 that outlive the
        // call, and `data`/`size` describe a DWORD-sized buffer, the one
        // type RRF_RT_REG_DWORD lets through.
        let status = unsafe {
            RegGetValueW(
                HKEY_CURRENT_USER,
                key.as_ptr(),
                value.as_ptr(),
                RRF_RT_REG_DWORD,
                std::ptr::null_mut(),
                (&mut data as *mut u32).cast(),
                &mut size,
            )
        };
        (status == 0).then_some(data != 0)
    }
}

#[cfg(target_os = "macos")]
mod imp {
    pub fn os_prefers_light() -> Option<bool> {
        let out = std::process::Command::new("defaults")
            .args(["read", "-g", "AppleInterfaceStyle"])
            .stderr(std::process::Stdio::null())
            .output()
            .ok()?;
        // The key exists only in dark mode: a failed read is light.
        Some(!(out.status.success() && String::from_utf8_lossy(&out.stdout).contains("Dark")))
    }
}

#[cfg(all(unix, not(target_os = "macos")))]
mod imp {
    pub fn os_prefers_light() -> Option<bool> {
        let out = std::process::Command::new("gsettings")
            .args(["get", "org.gnome.desktop.interface", "color-scheme"])
            .stderr(std::process::Stdio::null())
            .output()
            .ok()?;
        if !out.status.success() {
            return None;
        }
        let said = String::from_utf8_lossy(&out.stdout);
        if said.contains("prefer-light") {
            Some(true)
        } else if said.contains("prefer-dark") {
            Some(false)
        } else {
            None
        }
    }
}

/// What the LIVE THEME was last resolved from, so a beat re-resolves only
/// when something it read has moved.
#[derive(Debug, Clone, Default, PartialEq)]
struct Inputs {
    name: String,
    os_light: Option<bool>,
    vscode_stamp: Option<std::time::SystemTime>,
}

/// The theme the UI draws with, kept in step with the machine for the two
/// names that read it (`auto`, `vscode`): the OS's light or dark, and
/// VS Code's settings file. A preset name resolves once and never looks
/// at the machine.
#[derive(Debug, Default)]
pub struct LiveTheme {
    last: Option<Inputs>,
}

impl LiveTheme {
    /// Resolve `name` now, reading whatever it needs.
    pub fn resolve(&mut self, name: &str) -> Theme {
        let inputs = Self::inputs(name);
        let theme = Self::build(&inputs);
        self.last = Some(inputs);
        theme
    }

    /// Re-read the machine for the theme last resolved, and say what it
    /// resolves to if anything it read has changed since — None while
    /// nothing has, and always for a preset name.
    pub fn recheck(&mut self) -> Option<Theme> {
        let name = self.last.as_ref()?.name.clone();
        if !crate::theme::is_live(&name) {
            return None;
        }
        let inputs = Self::inputs(&name);
        if self.last.as_ref() == Some(&inputs) {
            return None;
        }
        let theme = Self::build(&inputs);
        self.last = Some(inputs);
        Some(theme)
    }

    /// Is the theme last resolved one that follows the machine?
    pub fn is_live(&self) -> bool {
        self.last
            .as_ref()
            .is_some_and(|i| crate::theme::is_live(&i.name))
    }

    fn inputs(name: &str) -> Inputs {
        let live = crate::theme::is_live(name);
        let vscode = name.trim().eq_ignore_ascii_case(crate::theme::VSCODE);
        Inputs {
            name: name.to_string(),
            os_light: if live { os_prefers_light() } else { None },
            vscode_stamp: if vscode {
                crate::vscode_theme::settings_stamp()
            } else {
                None
            },
        }
    }

    fn build(inputs: &Inputs) -> Theme {
        let vscode = inputs
            .name
            .trim()
            .eq_ignore_ascii_case(crate::theme::VSCODE);
        let machine = Machine {
            os_light: inputs.os_light,
            vscode: if vscode {
                crate::vscode_theme::load(inputs.os_light)
            } else {
                None
            },
        };
        crate::theme::resolve(&inputs.name, &machine)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A preset name never looks at the machine again: there is nothing a
    /// re-check could change.
    #[test]
    fn a_preset_is_never_rechecked() {
        let mut live = LiveTheme::default();
        assert_eq!(live.recheck(), None, "nothing resolved yet");
        assert_eq!(live.resolve("forest"), Theme::by_name("forest"));
        assert!(!live.is_live());
        assert_eq!(live.recheck(), None);
    }

    /// `auto` resolves to one of the One presets, and a re-check with the
    /// OS unchanged says nothing.
    #[test]
    fn auto_resolves_to_a_one_preset_and_holds_still() {
        let mut live = LiveTheme::default();
        let th = live.resolve("auto");
        assert!(th == crate::theme::ONE_DARK || th == crate::theme::ONE_LIGHT);
        assert!(live.is_live());
        assert_eq!(live.recheck(), None, "the OS has not moved");
    }
}
