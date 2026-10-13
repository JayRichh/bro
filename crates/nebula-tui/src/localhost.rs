//! LOCALHOST PREVIEWS: a worktree's dev server opened in a browser of your
//! choosing (`w`), or shot headless and shown in the image modal (`W`).
//!
//! Which port is the DAEMON's word (`nebula_core::ports`): the first one
//! the worktree's sessions listen on inside its PORT BLOCK, else any they
//! listen on, else the block's first port ([`crate::app::Tree::browse_port`]).
//!
//! The chooser lists the browsers this machine has ([`detect`]): Chrome,
//! Edge and Firefox where they are installed (the registry's `App Paths`
//! and then the usual install folders on Windows, `/Applications` and
//! `~/Applications` on macOS, the PATH on Linux), and the system default. A
//! browser can open on one of its own profiles — `browsers` in
//! `config.json`, `{"chrome": {"profile": "Profile 1"}}`, passed as
//! `--profile-directory` to Chrome and Edge and `-P` to Firefox. The pick
//! is remembered per project (`projects.<repo>.browser`), and the chooser
//! opens on it next time.
//!
//! The screenshot runs Edge or Chrome headless — `--headless=new
//! --screenshot=<file> --window-size=1280,800` — with a throwaway
//! `--user-data-dir`, so it never hands the job to a browser already
//! running, off the key handler (`view_jobs`). The picture opens in the
//! file modal's image view, which paints it with half-blocks: no terminal
//! graphics protocol is needed, so it shows in Windows Terminal, VS Code's
//! terminal and over ssh alike. With inline graphics switched off
//! (`inline_graphics: "off"`) it opens in the OS image viewer instead.

use std::path::{Path, PathBuf};

use crate::app::{App, ContextMenu, MenuAction, MenuItem, Overlay};

/// One browser the chooser can offer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Browser {
    /// `chrome`, `edge`, `firefox`, or `default`.
    pub id: &'static str,
    pub label: &'static str,
    /// The executable; None for the system default, which opens through
    /// the OS's own handler.
    pub program: Option<PathBuf>,
}

/// The system default browser: always offered, always first.
pub const DEFAULT: &str = "default";

/// The browsers installed on this machine, the system default first.
pub fn detect() -> Vec<Browser> {
    let mut out = vec![Browser {
        id: DEFAULT,
        label: "System default",
        program: None,
    }];
    for (id, label) in [
        ("chrome", "Google Chrome"),
        ("edge", "Microsoft Edge"),
        ("firefox", "Firefox"),
    ] {
        if let Some(program) = candidates(id).into_iter().find(|p| p.is_file()) {
            out.push(Browser {
                id,
                label,
                program: Some(program),
            });
        }
    }
    out
}

/// Where a browser installs itself: first wherever its installer said, in
/// the `App Paths` registry key (`HKCU`, then `HKLM`) — the same lookup
/// the Run dialog's `msedge` makes, and right for a browser installed
/// somewhere unusual — then the usual install folders under Program Files
/// and the per-user `%LOCALAPPDATA%` one Chrome uses without admin rights.
#[cfg(windows)]
fn candidates(id: &str) -> Vec<PathBuf> {
    let (exe, rel): (&str, &[&str]) = match id {
        "chrome" => ("chrome.exe", &[r"Google\Chrome\Application\chrome.exe"]),
        "edge" => ("msedge.exe", &[r"Microsoft\Edge\Application\msedge.exe"]),
        "firefox" => ("firefox.exe", &[r"Mozilla Firefox\firefox.exe"]),
        _ => return Vec::new(),
    };
    let roots: Vec<PathBuf> = ["ProgramFiles", "ProgramFiles(x86)", "LOCALAPPDATA"]
        .iter()
        .filter_map(|var| std::env::var_os(var).map(PathBuf::from))
        .collect();
    app_paths(exe)
        .into_iter()
        .chain(
            roots
                .iter()
                .flat_map(|root| rel.iter().map(move |r| root.join(r))),
        )
        .collect()
}

/// `exe`'s registered location under `Software\Microsoft\Windows\
/// CurrentVersion\App Paths\<exe>` (its default value), per user first,
/// then machine-wide. Quotes an installer left around the path are
/// dropped.
#[cfg(windows)]
fn app_paths(exe: &str) -> Vec<PathBuf> {
    use windows_sys::Win32::System::Registry::{
        RegGetValueW, HKEY, HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, RRF_RT_REG_SZ,
    };
    let wide = |s: &str| s.encode_utf16().chain(Some(0)).collect::<Vec<u16>>();
    let key = wide(&format!(
        r"Software\Microsoft\Windows\CurrentVersion\App Paths\{exe}"
    ));
    let read = |root: HKEY| -> Option<PathBuf> {
        let mut buf = vec![0u16; 1024];
        let mut size = (buf.len() * 2) as u32;
        // SAFETY: the key name is NUL-terminated UTF-16 that outlives the
        // call, a null value name reads the key's default value, and
        // `buf`/`size` describe a writable buffer of `size` bytes, which
        // the call never writes past.
        let status = unsafe {
            RegGetValueW(
                root,
                key.as_ptr(),
                std::ptr::null(),
                RRF_RT_REG_SZ,
                std::ptr::null_mut(),
                buf.as_mut_ptr().cast(),
                &mut size,
            )
        };
        if status != 0 {
            return None;
        }
        let len = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
        let path = String::from_utf16_lossy(&buf[..len]);
        let path = path.trim().trim_matches('"');
        (!path.is_empty()).then(|| PathBuf::from(path))
    };
    [HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE]
        .into_iter()
        .filter_map(read)
        .collect()
}

#[cfg(target_os = "macos")]
fn candidates(id: &str) -> Vec<PathBuf> {
    let app = match id {
        "chrome" => "Google Chrome.app/Contents/MacOS/Google Chrome",
        "edge" => "Microsoft Edge.app/Contents/MacOS/Microsoft Edge",
        "firefox" => "Firefox.app/Contents/MacOS/firefox",
        _ => return Vec::new(),
    };
    // The machine's Applications folder, then the user's own.
    let mut out = vec![PathBuf::from("/Applications").join(app)];
    if let Some(home) = std::env::var_os("HOME") {
        out.push(PathBuf::from(home).join("Applications").join(app));
    }
    out
}

#[cfg(all(unix, not(target_os = "macos")))]
fn candidates(id: &str) -> Vec<PathBuf> {
    let names: &[&str] = match id {
        "chrome" => &["google-chrome", "google-chrome-stable", "chromium"],
        "edge" => &["microsoft-edge", "microsoft-edge-stable"],
        "firefox" => &["firefox"],
        _ => &[],
    };
    let path = std::env::var_os("PATH").unwrap_or_default();
    std::env::split_paths(&path)
        .flat_map(|dir| names.iter().map(move |n| dir.join(n)))
        .collect()
}

/// The argv that opens `url` in `browser` on its `profile`, if one.
pub fn browser_args(browser: &Browser, profile: Option<&str>, url: &str) -> Vec<String> {
    let mut args = Vec::new();
    if let Some(profile) = profile.map(str::trim).filter(|p| !p.is_empty()) {
        match browser.id {
            "chrome" | "edge" => args.push(format!("--profile-directory={profile}")),
            "firefox" => args.extend(["-P".to_string(), profile.to_string()]),
            _ => {}
        }
    }
    args.push(url.to_string());
    args
}

/// Open `url` in the system default browser, detached. Only http(s).
pub fn open_default(url: &str) -> bool {
    if !(url.starts_with("http://") || url.starts_with("https://")) {
        return false;
    }
    if cfg!(test) {
        return true;
    }
    #[cfg(windows)]
    let command = {
        // The URL handler itself, with no shell between: a `&` in a query
        // string is not a command separator here.
        let mut c = std::process::Command::new("rundll32.exe");
        c.args(["url.dll,FileProtocolHandler", url]);
        c
    };
    #[cfg(target_os = "macos")]
    let command = {
        let mut c = std::process::Command::new("open");
        c.arg(url);
        c
    };
    #[cfg(all(unix, not(target_os = "macos")))]
    let command = {
        let mut c = std::process::Command::new("xdg-open");
        c.arg(url);
        c
    };
    crate::event_loop::spawn_and_reap(command, "open url")
}

/// `http://localhost:<port>` for the selected worktree, with the worktree
/// and its project's repo path; a flash saying why not otherwise.
fn target(app: &mut App) -> Option<(nebula_core::Worktree, PathBuf, String)> {
    let Some(worktree) = app.selected_worktree().cloned() else {
        app.chrome.flash = Some("select a worktree first".into());
        return None;
    };
    let Some(port) = app.tree.browse_port(&worktree.id) else {
        app.chrome.flash = Some(format!(
            "nothing is listening in {} yet — start its dev server (PORT is set in its sessions)",
            worktree.branch
        ));
        return None;
    };
    let repo = app
        .tree
        .projects
        .iter()
        .find(|p| p.id == worktree.project_id)
        .map_or_else(|| worktree.path.clone(), |p| p.repo_path.clone());
    Some((worktree, repo, format!("http://localhost:{port}")))
}

/// `w`: the browser chooser for the selected worktree's dev server, on
/// the browser this project used last.
pub(crate) fn open_chooser(app: &mut App) {
    let Some((_, repo, url)) = target(app) else {
        return;
    };
    let cfg = crate::config::Config::load();
    let last = cfg.project(&repo).browser;
    let browsers = detect();
    let items: Vec<MenuItem> = browsers
        .iter()
        .map(|b| {
            let profile = cfg
                .browsers
                .get(b.id)
                .map(|s| s.profile.trim().to_string())
                .filter(|p| !p.is_empty());
            let mut label = b.label.to_string();
            if let Some(profile) = &profile {
                label.push_str(&format!(" ({profile})"));
            }
            if b.id == last {
                label.push_str(" ✓");
            }
            MenuItem::new(
                label,
                MenuAction::OpenInBrowser {
                    browser: b.id.to_string(),
                    url: url.clone(),
                    repo: repo.clone(),
                },
            )
        })
        .collect();
    let hover = browsers.iter().position(|b| b.id == last).unwrap_or(0);
    app.modals.overlay = Some(Overlay::Menu(ContextMenu {
        title: Some(format!("Open {url}")),
        items,
        at: None,
        hover,
        area: ratatui::layout::Rect::default(),
        parent: None,
        filter: None,
    }));
}

/// A chooser row: open `url` in `browser` and remember it for the project.
pub(crate) fn open_in(app: &mut App, browser: &str, url: &str, repo: &Path) {
    let mut cfg = crate::config::Config::load();
    let chosen = detect().into_iter().find(|b| b.id == browser);
    let opened = match &chosen {
        Some(Browser {
            program: Some(program),
            ..
        }) if !cfg!(test) => {
            let profile = cfg.browsers.get(browser).map(|s| s.profile.clone());
            let mut command = std::process::Command::new(program);
            command.args(browser_args(
                chosen.as_ref().expect("matched above"),
                profile.as_deref(),
                url,
            ));
            crate::event_loop::spawn_and_reap(command, "open browser")
        }
        _ => open_default(url),
    };
    app.chrome.flash = Some(if opened {
        format!("↗ {url} in {}", chosen.map_or("the browser", |b| b.label))
    } else {
        format!("couldn't open {url}")
    });
    let mut project = cfg.project(repo);
    if project.browser != browser {
        project.browser = browser.to_string();
        cfg.set_project(repo, project);
        if let Err(e) = cfg.save() {
            tracing::warn!(error = %e, "remembering the browser failed");
        }
    }
}

/// The browser a headless screenshot runs: Edge, then Chrome.
fn headless_browser() -> Option<PathBuf> {
    ["edge", "chrome"]
        .into_iter()
        .find_map(|id| candidates(id).into_iter().find(|p| p.is_file()))
}

/// `W`: shoot the selected worktree's dev server headless, off the key
/// handler, and show the picture when it lands.
pub(crate) fn screenshot(app: &mut App) {
    let Some((worktree, _, url)) = target(app) else {
        return;
    };
    let Some(browser) = headless_browser() else {
        app.chrome.flash = Some("a screenshot needs Edge or Chrome installed".into());
        return;
    };
    let dir = nebula_core::paths::data_dir().join("screenshots");
    let file = dir.join(format!(
        "{}-{}.png",
        worktree.branch.replace(['/', '\\', ':'], "-"),
        nebula_core::clock::now_ms()
    ));
    app.chrome.flash = Some(format!("shooting {url}…"));
    let Some(jobs) = app.jobs.view_jobs.clone() else {
        return;
    };
    jobs.run(move || {
        let result = shoot(&browser, &url, &dir, &file);
        Some(crate::view_jobs::Answer::Screenshot { url, result })
    });
}

/// Run the headless shot and wait for it (30 s at most).
fn shoot(browser: &Path, url: &str, dir: &Path, file: &Path) -> Result<PathBuf, String> {
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    // A throwaway profile: with the user's own, a headless run hands the
    // job to the browser already open and exits with nothing written.
    let profile = std::env::temp_dir().join(format!(
        "bro-shot-{}-{}",
        std::process::id(),
        nebula_core::clock::now_ms()
    ));
    let shot = run_shot(browser, url, file, &profile);
    // Best effort: a renderer the killed parent left behind can still hold
    // a file in it, and a leftover profile in the temp dir harms nothing.
    let _ = std::fs::remove_dir_all(&profile);
    shot
}

fn run_shot(browser: &Path, url: &str, file: &Path, profile: &Path) -> Result<PathBuf, String> {
    let mut command = std::process::Command::new(browser);
    command
        .arg("--headless=new")
        .arg(format!("--screenshot={}", file.display()))
        .arg("--window-size=1280,800")
        .arg("--hide-scrollbars")
        .arg("--disable-gpu")
        .arg(format!("--user-data-dir={}", profile.display()))
        .arg(url)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    #[cfg(windows)]
    nebula_core::platform::no_window(&mut command);
    let mut child = command.spawn().map_err(|e| e.to_string())?;
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if std::time::Instant::now() < deadline => {
                std::thread::sleep(std::time::Duration::from_millis(100));
            }
            _ => {
                // A headless run now and then writes its picture and then
                // hangs on the way out: the picture still counts.
                let _ = child.kill();
                let _ = child.wait();
                if file.is_file() {
                    return Ok(file.to_path_buf());
                }
                return Err("the headless browser timed out".into());
            }
        }
    }
    if file.is_file() {
        Ok(file.to_path_buf())
    } else {
        Err("the headless browser wrote no picture".into())
    }
}

/// A screenshot landed: show it — in the image modal, or the OS viewer
/// when inline graphics are off.
pub(crate) fn land_screenshot(app: &mut App, url: String, result: Result<PathBuf, String>) {
    let file = match result {
        Ok(file) => file,
        Err(e) => {
            app.chrome.flash = Some(format!("screenshot of {url} failed: {e}"));
            return;
        }
    };
    let graphics = crate::config::Config::load().graphics_mode();
    if graphics == crate::graphics::GraphicsMode::Off {
        let opened = open_file(&file);
        app.chrome.flash = Some(if opened {
            format!("↗ {}", file.display())
        } else {
            format!("screenshot saved to {}", file.display())
        });
        return;
    }
    let root = file.parent().map(Path::to_path_buf).unwrap_or_default();
    crate::file_tabs::open(app, root, vec![file]);
    app.chrome.flash = Some(format!("screenshot of {url}"));
}

/// Open a file in the OS's own viewer, detached.
fn open_file(path: &Path) -> bool {
    if cfg!(test) {
        return true;
    }
    #[cfg(windows)]
    let command = {
        let mut c = std::process::Command::new("rundll32.exe");
        c.arg("url.dll,FileProtocolHandler").arg(path);
        c
    };
    #[cfg(target_os = "macos")]
    let command = {
        let mut c = std::process::Command::new("open");
        c.arg(path);
        c
    };
    #[cfg(all(unix, not(target_os = "macos")))]
    let command = {
        let mut c = std::process::Command::new("xdg-open");
        c.arg(path);
        c
    };
    crate::event_loop::spawn_and_reap(command, "open screenshot")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_browser_profile_rides_its_own_flag() {
        let chrome = Browser {
            id: "chrome",
            label: "Google Chrome",
            program: Some(PathBuf::from("chrome")),
        };
        assert_eq!(
            browser_args(&chrome, Some("Profile 1"), "http://localhost:3010"),
            vec!["--profile-directory=Profile 1", "http://localhost:3010"]
        );
        let firefox = Browser {
            id: "firefox",
            ..chrome.clone()
        };
        assert_eq!(
            browser_args(&firefox, Some("dev"), "http://localhost:3010"),
            vec!["-P", "dev", "http://localhost:3010"]
        );
        assert_eq!(
            browser_args(&chrome, None, "http://localhost:3010"),
            vec!["http://localhost:3010"]
        );
        assert_eq!(
            detect()[0].id,
            DEFAULT,
            "the default is always offered first"
        );
    }
}
