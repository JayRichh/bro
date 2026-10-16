//! The Win32 half of `editors`: the registry and vswhere reads the scan
//! takes, the open-window check, Dock, and the exe's icon. On
//! `windows-sys`, as the rest of nebula-core is.
//!
//! Nothing here sends input or activates a window: Dock moves the two
//! windows with `SWP_NOACTIVATE` and un-maximises with
//! `SW_SHOWNOACTIVATE`, and the editor is put just behind Kete in the
//! z-order, so the keyboard stays where the user left it.

use super::detect::UninstallEntry;
use super::dock::{self, Insets, Rect};
use super::{Editor, EditorWindow, Family, Solution};
use std::ffi::OsString;
use std::os::windows::ffi::{OsStrExt, OsStringExt};
use std::path::{Path, PathBuf};

use windows_sys::Win32::Foundation::{
    CloseHandle, ERROR_ACCESS_DENIED, HANDLE, HWND, LPARAM, RECT,
};

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

fn wide_path(p: &Path) -> Vec<u16> {
    p.as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect()
}

// ---- the registry ----

mod reg {
    use super::wide;
    use windows_sys::Win32::Foundation::{ERROR_SUCCESS, FILETIME};
    use windows_sys::Win32::System::Registry::{
        RegCloseKey, RegEnumKeyExW, RegGetValueW, RegOpenKeyExW, HKEY, KEY_READ,
        RRF_RT_REG_EXPAND_SZ, RRF_RT_REG_SZ,
    };

    /// An open key, closed on drop.
    pub struct Key(HKEY);

    impl Key {
        pub fn open(root: HKEY, path: &str, view: u32) -> Option<Self> {
            let mut key: HKEY = std::ptr::null_mut();
            let path = wide(path);
            // SAFETY: `path` is NUL-terminated; `key` receives the handle.
            let rc = unsafe { RegOpenKeyExW(root, path.as_ptr(), 0, KEY_READ | view, &mut key) };
            (rc == ERROR_SUCCESS).then_some(Self(key))
        }

        /// The names of its subkeys.
        pub fn subkeys(&self) -> Vec<String> {
            let mut out = Vec::new();
            let mut buf = [0u16; 512];
            for i in 0.. {
                let mut len = buf.len() as u32;
                let mut written = FILETIME {
                    dwLowDateTime: 0,
                    dwHighDateTime: 0,
                };
                // SAFETY: `buf` holds `len` u16s; the rest are optional.
                let rc = unsafe {
                    RegEnumKeyExW(
                        self.0,
                        i,
                        buf.as_mut_ptr(),
                        &mut len,
                        std::ptr::null(),
                        std::ptr::null_mut(),
                        std::ptr::null_mut(),
                        &mut written,
                    )
                };
                if rc != ERROR_SUCCESS {
                    break;
                }
                out.push(String::from_utf16_lossy(&buf[..len as usize]));
            }
            out
        }

        /// A string value of the subkey `sub` (`""` for the key itself),
        /// environment variables expanded; `name` None is the default
        /// value.
        pub fn string(&self, sub: &str, name: Option<&str>) -> Option<String> {
            let sub = wide(sub);
            let name = name.map(wide);
            let name_ptr = name.as_ref().map_or(std::ptr::null(), |n| n.as_ptr());
            let mut size = 0u32;
            let flags = RRF_RT_REG_SZ | RRF_RT_REG_EXPAND_SZ;
            // SAFETY: a size query: no data pointer.
            let rc = unsafe {
                RegGetValueW(
                    self.0,
                    sub.as_ptr(),
                    name_ptr,
                    flags,
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                    &mut size,
                )
            };
            if rc != ERROR_SUCCESS || size == 0 {
                return None;
            }
            let mut buf = vec![0u16; (size as usize).div_ceil(2) + 1];
            let mut bytes = (buf.len() * 2) as u32;
            // SAFETY: `buf` holds `bytes` bytes.
            let rc = unsafe {
                RegGetValueW(
                    self.0,
                    sub.as_ptr(),
                    name_ptr,
                    flags,
                    std::ptr::null_mut(),
                    buf.as_mut_ptr().cast(),
                    &mut bytes,
                )
            };
            if rc != ERROR_SUCCESS {
                return None;
            }
            let len = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
            Some(String::from_utf16_lossy(&buf[..len]))
        }
    }

    impl Drop for Key {
        fn drop(&mut self) {
            // SAFETY: the key came from RegOpenKeyExW and is closed once.
            unsafe { RegCloseKey(self.0) };
        }
    }
}

const UNINSTALL: &str = r"Software\Microsoft\Windows\CurrentVersion\Uninstall";
const APP_PATHS: &str = r"Software\Microsoft\Windows\CurrentVersion\App Paths";

/// Every uninstall key: the user's, then the machine's 64-bit and 32-bit
/// views.
pub fn uninstall_entries() -> Vec<UninstallEntry> {
    use windows_sys::Win32::System::Registry::{
        HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, KEY_WOW64_32KEY, KEY_WOW64_64KEY,
    };
    let mut out = Vec::new();
    let roots = [
        (HKEY_CURRENT_USER, 0, false),
        (HKEY_LOCAL_MACHINE, KEY_WOW64_64KEY, true),
        (HKEY_LOCAL_MACHINE, KEY_WOW64_32KEY, true),
    ];
    for (root, view, machine) in roots {
        let Some(key) = reg::Key::open(root, UNINSTALL, view) else {
            continue;
        };
        for sub in key.subkeys() {
            let get = |name: &str| key.string(&sub, Some(name)).unwrap_or_default();
            let entry = UninstallEntry {
                key: sub.clone(),
                machine,
                display_version: get("DisplayVersion"),
                install_location: get("InstallLocation"),
                display_icon: get("DisplayIcon"),
            };
            if !entry.install_location.is_empty() || !entry.display_icon.is_empty() {
                out.push(entry);
            }
        }
    }
    out
}

/// `App Paths\<exe>`'s default value, the user's then the machine's.
pub fn app_path(exe: &str) -> Option<String> {
    use windows_sys::Win32::System::Registry::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE};
    [HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE]
        .into_iter()
        .find_map(|root| {
            reg::Key::open(root, APP_PATHS, 0)?
                .string(exe, None)
                .filter(|v| !v.trim().is_empty())
        })
}

/// vswhere's JSON for every instance (`-all -prerelease -products *`),
/// run with no console and given ten seconds; None without vswhere.
pub fn vswhere() -> Option<String> {
    use std::os::windows::process::CommandExt;
    use std::time::{Duration, Instant};
    let base =
        std::env::var_os("ProgramFiles(x86)").or_else(|| std::env::var_os("ProgramFiles"))?;
    let exe = Path::new(&base)
        .join("Microsoft Visual Studio")
        .join("Installer")
        .join("vswhere.exe");
    if !exe.is_file() {
        return None;
    }
    let mut child = std::process::Command::new(exe)
        .args([
            "-all",
            "-prerelease",
            "-products",
            "*",
            "-format",
            "json",
            "-utf8",
            "-sort",
        ])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .creation_flags(crate::platform::NO_WINDOW_FLAGS)
        .spawn()
        .ok()?;
    let mut stdout = child.stdout.take()?;
    let reader = std::thread::spawn(move || {
        let mut out = String::new();
        let _ = std::io::Read::read_to_string(&mut stdout, &mut out);
        out
    });
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(20)),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
        }
    }
    reader.join().ok()
}

// ---- processes ----

/// A process handle, closed on drop.
struct Process(HANDLE);

impl Process {
    fn open(pid: u32) -> Option<Self> {
        use windows_sys::Win32::System::Threading::{
            OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION,
        };
        // SAFETY: plain FFI call; null is the failure value.
        let h = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
        (!h.is_null()).then_some(Self(h))
    }

    fn exe(&self) -> Option<PathBuf> {
        use windows_sys::Win32::System::Threading::{
            QueryFullProcessImageNameW, PROCESS_NAME_WIN32,
        };
        let mut buf = vec![0u16; 1024];
        let mut len = buf.len() as u32;
        // SAFETY: `buf` holds `len` u16s.
        let ok = unsafe {
            QueryFullProcessImageNameW(self.0, PROCESS_NAME_WIN32, buf.as_mut_ptr(), &mut len)
        } != 0;
        ok.then(|| PathBuf::from(OsString::from_wide(&buf[..len as usize])))
    }
}

impl Drop for Process {
    fn drop(&mut self) {
        // SAFETY: the handle came from OpenProcess and is closed once.
        unsafe { CloseHandle(self.0) };
    }
}

/// Whether a process runs elevated. Some(true) too when its token can't
/// even be read for that (a medium process looking at an elevated one);
/// None when the process can't be opened.
fn elevated(process: HANDLE) -> Option<bool> {
    use windows_sys::Win32::Foundation::GetLastError;
    use windows_sys::Win32::Security::{
        GetTokenInformation, TokenElevation, TOKEN_ELEVATION, TOKEN_QUERY,
    };
    use windows_sys::Win32::System::Threading::OpenProcessToken;
    let mut token: HANDLE = std::ptr::null_mut();
    // SAFETY: `process` is a live handle; `token` receives one.
    if unsafe { OpenProcessToken(process, TOKEN_QUERY, &mut token) } == 0 {
        // SAFETY: plain FFI call.
        return (unsafe { GetLastError() } == ERROR_ACCESS_DENIED).then_some(true);
    }
    let mut info = TOKEN_ELEVATION { TokenIsElevated: 0 };
    let mut len = 0u32;
    // SAFETY: `info` is sized for the class; the token is live.
    let ok = unsafe {
        GetTokenInformation(
            token,
            TokenElevation,
            (&mut info as *mut TOKEN_ELEVATION).cast(),
            std::mem::size_of::<TOKEN_ELEVATION>() as u32,
            &mut len,
        )
    } != 0;
    // SAFETY: the token came from OpenProcessToken and is closed once.
    unsafe { CloseHandle(token) };
    ok.then_some(info.TokenIsElevated != 0)
}

fn self_elevated() -> bool {
    use windows_sys::Win32::System::Threading::GetCurrentProcess;
    // SAFETY: the pseudo-handle needs no closing.
    elevated(unsafe { GetCurrentProcess() }).unwrap_or(false)
}

/// Let `pid` take the foreground this process has.
pub fn allow_foreground(pid: u32) {
    use windows_sys::Win32::UI::WindowsAndMessaging::AllowSetForegroundWindow;
    // SAFETY: plain FFI call; failure (not foreground now) is harmless.
    unsafe { AllowSetForegroundWindow(pid) };
}

// ---- windows ----

fn hwnd(h: isize) -> HWND {
    h as HWND
}

/// Whether DWM hides the window (another virtual desktop, a suspended
/// UWP frame): it is there but not on screen.
fn cloaked(h: HWND) -> bool {
    use windows_sys::Win32::Graphics::Dwm::{DwmGetWindowAttribute, DWMWA_CLOAKED};
    let mut v = 0u32;
    // SAFETY: `v` is the u32 the attribute fills.
    let hr =
        unsafe { DwmGetWindowAttribute(h, DWMWA_CLOAKED as u32, (&mut v as *mut u32).cast(), 4) };
    hr == 0 && v != 0
}

fn title(h: HWND) -> String {
    use windows_sys::Win32::UI::WindowsAndMessaging::{GetWindowTextLengthW, GetWindowTextW};
    // SAFETY: plain FFI calls; `buf` holds `len + 1` u16s.
    let len = unsafe { GetWindowTextLengthW(h) };
    if len <= 0 {
        return String::new();
    }
    let mut buf = vec![0u16; len as usize + 1];
    let n = unsafe { GetWindowTextW(h, buf.as_mut_ptr(), buf.len() as i32) };
    String::from_utf16_lossy(&buf[..n.max(0) as usize])
}

/// Every visible, unowned, uncloaked top-level window, topmost first,
/// with its process.
fn top_windows() -> Vec<(HWND, u32)> {
    use windows_sys::core::BOOL;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        EnumWindows, GetWindow, GetWindowThreadProcessId, IsWindowVisible, GW_OWNER,
    };
    unsafe extern "system" fn each(h: HWND, data: LPARAM) -> BOOL {
        // SAFETY: `data` is the Vec `top_windows` passed, alive for the
        // whole EnumWindows call.
        let out = unsafe { &mut *(data as *mut Vec<(HWND, u32)>) };
        // SAFETY: plain FFI calls on the window being enumerated.
        unsafe {
            if IsWindowVisible(h) != 0 && GetWindow(h, GW_OWNER).is_null() && !cloaked(h) {
                let mut pid = 0u32;
                GetWindowThreadProcessId(h, &mut pid);
                out.push((h, pid));
            }
        }
        1
    }
    let mut out: Vec<(HWND, u32)> = Vec::new();
    // SAFETY: the callback only touches `out` through the pointer.
    unsafe { EnumWindows(Some(each), &mut out as *mut _ as LPARAM) };
    out
}

fn same_file(a: &Path, b: &Path) -> bool {
    let norm = |p: &Path| p.to_string_lossy().replace('/', "\\").to_ascii_lowercase();
    norm(a) == norm(b)
}

/// The top-level windows `editors`' exes own, topmost first.
pub fn editor_windows(editors: &[Editor]) -> Vec<EditorWindow> {
    if editors.is_empty() {
        return Vec::new();
    }
    let mut exes: std::collections::HashMap<u32, Option<String>> = Default::default();
    let mut out = Vec::new();
    for (h, pid) in top_windows() {
        let editor = exes
            .entry(pid)
            .or_insert_with(|| {
                let exe = Process::open(pid)?.exe()?;
                editors
                    .iter()
                    .find(|e| same_file(&e.exe, &exe))
                    .map(|e| e.id.clone())
            })
            .clone();
        if let Some(editor) = editor {
            out.push(EditorWindow {
                hwnd: h as isize,
                pid,
                editor,
                title: title(h),
            });
        }
    }
    out
}

// ---- Dock ----

fn rect_of(r: RECT) -> Rect {
    Rect::new(r.left, r.top, r.right, r.bottom)
}

fn outer(h: HWND) -> Option<Rect> {
    use windows_sys::Win32::UI::WindowsAndMessaging::GetWindowRect;
    let mut r = RECT {
        left: 0,
        top: 0,
        right: 0,
        bottom: 0,
    };
    // SAFETY: `r` is the RECT the call fills.
    (unsafe { GetWindowRect(h, &mut r) } != 0).then(|| rect_of(r))
}

/// The drawn frame (without the invisible resize borders); the outer
/// rectangle where DWM can't say.
fn frame(h: HWND) -> Option<Rect> {
    use windows_sys::Win32::Graphics::Dwm::{DwmGetWindowAttribute, DWMWA_EXTENDED_FRAME_BOUNDS};
    let mut r = RECT {
        left: 0,
        top: 0,
        right: 0,
        bottom: 0,
    };
    // SAFETY: `r` is the RECT the attribute fills.
    let hr = unsafe {
        DwmGetWindowAttribute(
            h,
            DWMWA_EXTENDED_FRAME_BOUNDS as u32,
            (&mut r as *mut RECT).cast(),
            std::mem::size_of::<RECT>() as u32,
        )
    };
    if hr == 0 {
        Some(rect_of(r))
    } else {
        outer(h)
    }
}

/// Un-maximise or un-minimise `h` without activating it.
fn unsnap(h: HWND) {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        IsIconic, IsZoomed, ShowWindow, SW_SHOWNOACTIVATE,
    };
    // SAFETY: plain FFI calls.
    unsafe {
        if IsZoomed(h) != 0 || IsIconic(h) != 0 {
            ShowWindow(h, SW_SHOWNOACTIVATE);
        }
    }
}

/// Runs `f` with this thread per-monitor DPI aware, so every rectangle
/// read and set is in physical pixels whatever the process's own
/// awareness (the TUI's console process is unaware).
fn per_monitor<T>(f: impl FnOnce() -> T) -> T {
    use windows_sys::Win32::UI::HiDpi::{
        SetThreadDpiAwarenessContext, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
    };
    // SAFETY: plain FFI calls; the old context is put back.
    let old = unsafe { SetThreadDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2) };
    let out = f();
    if !old.is_null() {
        unsafe { SetThreadDpiAwarenessContext(old) };
    }
    out
}

/// See [`super::dock`].
pub fn dock(
    editors: &[Editor],
    folder: &Path,
    solutions: &[Solution],
    kete: isize,
) -> Result<(), String> {
    use windows_sys::Win32::Graphics::Gdi::{
        GetMonitorInfoW, MonitorFromWindow, MONITORINFO, MONITOR_DEFAULTTONEAREST,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        SetWindowPos, SWP_NOACTIVATE, SWP_NOOWNERZORDER,
    };
    let kete = hwnd(kete);
    if kete.is_null() {
        return Err("Kete couldn't find its own window to dock beside".into());
    }
    let all = editor_windows(editors);
    let stems: Vec<String> = solutions.iter().map(|s| s.stem()).collect();
    let family_of = |w: &EditorWindow| {
        editors
            .iter()
            .find(|e| e.id == w.editor)
            .map(|e| e.family)
            .unwrap_or(Family::Code)
    };
    // The window whose title names the checkout, else the topmost window
    // of the editor a folder opens in by default.
    let target = all
        .iter()
        .find(|w| super::titles::matches(&w.title, family_of(w), folder, &stems))
        .or_else(|| {
            let first = editors.first()?;
            all.iter().find(|w| w.editor == first.id)
        })
        .ok_or_else(|| super::DOCK_NO_WINDOW.to_string())?;
    let name = editors
        .iter()
        .find(|e| e.id == target.editor)
        .map_or("The editor", |e| match e.family {
            Family::Code => e.short.as_str(),
            Family::VisualStudio => "Visual Studio",
        });
    if !self_elevated() {
        let up = Process::open(target.pid).and_then(|p| elevated(p.0));
        if up == Some(true) {
            return Err(format!(
                "{name} is running as administrator; Kete can't move it"
            ));
        }
    }
    let editor = hwnd(target.hwnd);
    per_monitor(|| {
        unsnap(editor);
        unsnap(kete);
        // SAFETY: plain FFI calls; `info` is sized.
        let monitor = unsafe { MonitorFromWindow(kete, MONITOR_DEFAULTTONEAREST) };
        let mut info = MONITORINFO {
            cbSize: std::mem::size_of::<MONITORINFO>() as u32,
            rcMonitor: RECT {
                left: 0,
                top: 0,
                right: 0,
                bottom: 0,
            },
            rcWork: RECT {
                left: 0,
                top: 0,
                right: 0,
                bottom: 0,
            },
            dwFlags: 0,
        };
        if unsafe { GetMonitorInfoW(monitor, &mut info) } == 0 {
            return Err("Kete couldn't read its monitor's size".to_string());
        }
        let insets = |h: HWND| match (outer(h), frame(h)) {
            (Some(o), Some(f)) => Insets::between(o, f),
            _ => Insets::default(),
        };
        let kete_frame = frame(kete).ok_or("Kete couldn't read its own window")?;
        let plan = dock::plan(
            rect_of(info.rcWork),
            kete_frame,
            insets(kete),
            insets(editor),
        );
        let flags = SWP_NOACTIVATE | SWP_NOOWNERZORDER;
        let place = |h: HWND, after: HWND, r: Rect, z: bool| {
            let flags = if z {
                flags
            } else {
                flags | windows_sys::Win32::UI::WindowsAndMessaging::SWP_NOZORDER
            };
            // SAFETY: plain FFI call on a window handle.
            unsafe { SetWindowPos(h, after, r.left, r.top, r.width(), r.height(), flags) != 0 }
        };
        // The editor just behind Kete, so both show once side by side.
        if !place(editor, kete, plan.editor, true) {
            use windows_sys::Win32::Foundation::GetLastError;
            // SAFETY: plain FFI call.
            return Err(if unsafe { GetLastError() } == ERROR_ACCESS_DENIED {
                format!("{name} is running as administrator; Kete can't move it")
            } else {
                format!("Kete couldn't move the {name} window")
            });
        }
        place(kete, std::ptr::null_mut(), plan.kete, false);
        Ok(())
    })
}

/// The terminal window the TUI runs in: the console's own window when it
/// shows (conhost), else the window that owns the pseudo-console
/// (Windows Terminal).
pub fn console_window() -> Option<isize> {
    use windows_sys::Win32::System::Console::GetConsoleWindow;
    use windows_sys::Win32::UI::WindowsAndMessaging::{GetAncestor, IsWindowVisible, GA_ROOTOWNER};
    // SAFETY: plain FFI calls.
    unsafe {
        let console = GetConsoleWindow();
        if console.is_null() {
            return None;
        }
        if IsWindowVisible(console) != 0 {
            return Some(console as isize);
        }
        let owner = GetAncestor(console, GA_ROOTOWNER);
        (!owner.is_null() && owner != console && IsWindowVisible(owner) != 0)
            .then_some(owner as isize)
    }
}

// ---- icons ----

/// RGBA pixels of `size`×`size` from the icon of `exe`.
fn icon_rgba(exe: &Path, size: u32) -> Option<Vec<u8>> {
    use windows_sys::Win32::Graphics::Gdi::{
        CreateCompatibleDC, DeleteDC, DeleteObject, GetDIBits, BITMAPINFO, BITMAPINFOHEADER,
        BI_RGB, DIB_RGB_COLORS, HBITMAP,
    };
    use windows_sys::Win32::UI::Shell::SHDefExtractIconW;
    use windows_sys::Win32::UI::WindowsAndMessaging::{DestroyIcon, GetIconInfo, HICON, ICONINFO};
    let path = wide_path(exe);
    let mut icon: HICON = std::ptr::null_mut();
    // SAFETY: `path` is NUL-terminated; only the large icon is asked for,
    // at `size` px.
    let hr =
        unsafe { SHDefExtractIconW(path.as_ptr(), 0, 0, &mut icon, std::ptr::null_mut(), size) };
    if hr != 0 || icon.is_null() {
        return None;
    }
    let mut info = ICONINFO::default();
    // SAFETY: `icon` is live; `info` is the struct the call fills.
    let got = unsafe { GetIconInfo(icon, &mut info) } != 0;
    let pixels = got.then(|| {
        // SAFETY: a memory DC for GetDIBits, deleted below.
        let dc = unsafe { CreateCompatibleDC(std::ptr::null_mut()) };
        let read = |bitmap: HBITMAP| -> Option<Vec<u8>> {
            if bitmap.is_null() {
                return None;
            }
            let mut bmi = BITMAPINFO {
                bmiHeader: BITMAPINFOHEADER {
                    biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                    biWidth: size as i32,
                    // Top-down rows.
                    biHeight: -(size as i32),
                    biPlanes: 1,
                    biBitCount: 32,
                    biCompression: BI_RGB,
                    ..Default::default()
                },
                ..Default::default()
            };
            let mut buf = vec![0u8; (size * size * 4) as usize];
            // SAFETY: `buf` holds size×size 32-bit pixels, as `bmi` asks.
            let lines = unsafe {
                GetDIBits(
                    dc,
                    bitmap,
                    0,
                    size,
                    buf.as_mut_ptr().cast(),
                    &mut bmi,
                    DIB_RGB_COLORS,
                )
            };
            (lines == size as i32).then_some(buf)
        };
        let color = read(info.hbmColor);
        let mask = read(info.hbmMask);
        // SAFETY: the DC and both bitmaps are ours to free, once.
        unsafe {
            DeleteDC(dc);
            if !info.hbmColor.is_null() {
                DeleteObject(info.hbmColor);
            }
            if !info.hbmMask.is_null() {
                DeleteObject(info.hbmMask);
            }
        }
        let mut px = color?;
        // BGRA to RGBA; an icon with no alpha at all takes its mask's.
        let has_alpha = px.chunks(4).any(|p| p[3] != 0);
        for (i, p) in px.chunks_mut(4).enumerate() {
            p.swap(0, 2);
            if !has_alpha {
                let opaque = mask.as_ref().is_none_or(|m| m[i * 4] == 0);
                p[3] = if opaque { 255 } else { 0 };
            }
        }
        Some(px)
    });
    // SAFETY: the icon came from SHDefExtractIconW and is freed once.
    unsafe { DestroyIcon(icon) };
    pixels.flatten()
}

/// The icon of `exe` at `size` px as PNG bytes.
pub fn icon_png(exe: &Path, size: u32) -> Option<Vec<u8>> {
    let rgba = icon_rgba(exe, size)?;
    encode_png(&rgba, size, size)
}

/// RGBA pixels as a PNG.
pub fn encode_png(rgba: &[u8], width: u32, height: u32) -> Option<Vec<u8>> {
    let mut out = Vec::new();
    {
        let mut enc = png::Encoder::new(&mut out, width, height);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        let mut w = enc.write_header().ok()?;
        w.write_image_data(rgba).ok()?;
    }
    Some(out)
}

/// The icon's cache file: the exe's path, its modification time and the
/// size, hashed, so an update to the editor extracts afresh.
fn icon_file(exe: &Path, size: u32, cache_dir: &Path) -> Option<PathBuf> {
    use std::hash::{Hash, Hasher};
    let modified = std::fs::metadata(exe).and_then(|m| m.modified()).ok()?;
    let mut h = std::collections::hash_map::DefaultHasher::new();
    exe.to_string_lossy().to_ascii_lowercase().hash(&mut h);
    modified.hash(&mut h);
    size.hash(&mut h);
    Some(cache_dir.join(format!("{:016x}-{size}.png", h.finish())))
}

/// See [`super::icon`].
pub fn cached_icon(exe: &Path, size: u32, cache_dir: &Path) -> Option<PathBuf> {
    let file = icon_file(exe, size, cache_dir)?;
    if file.is_file() {
        return Some(file);
    }
    let png = icon_png(exe, size)?;
    std::fs::create_dir_all(cache_dir).ok()?;
    let tmp = file.with_extension("tmp");
    std::fs::write(&tmp, &png).ok()?;
    std::fs::rename(&tmp, &file).ok()?;
    Some(file)
}
