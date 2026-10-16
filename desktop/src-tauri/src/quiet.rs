//! The window's WebView2, quieted: no browser right-click menu (Back,
//! Refresh, Save as, Print, Inspect) and, in release builds, no browser
//! keys (reload, print, find, zoom, devtools, back and forward). Kete
//! draws its own menus instead (desktop/src/lib/menus): Copy and Paste for
//! fields, the terminal and the diff viewer, and every row's verbs.
//!
//! WebView2 keeps the editing keys (Ctrl+C, V, X, A, Z) with the browser
//! keys off, and every key it no longer takes reaches the page: Alt+Left
//! and Alt+Right are Kete's back and forward, Ctrl+R, F and P go to the
//! shell in a terminal. Zoom keys were already off (Tauri's
//! `zoomHotkeysEnabled` defaults to false).
//!
//! Paste reads the clipboard with `navigator.clipboard.readText()`, which
//! WebView2 asks permission for. The grant here is the one Tauri's
//! `enable_clipboard_access` makes for a window it builds, held to this
//! app's own pages.
//!
//! Tauri doesn't pass these settings to wry, so they are set on the live
//! webview. WebView2 applies most settings at the next navigation, and the
//! first one has started by the time the window exists: one reload, before
//! the page has drawn, makes them hold.

/// Whether WebView2's browser keys stay on: in debug builds (F12 opens the
/// devtools there, F5 reloads), unless `KETE_BROWSER_KEYS=off` asks for the
/// release behaviour (the e2e keyboard flow).
pub fn browser_keys(debug: bool, env: Option<&str>) -> bool {
    debug && !matches!(env, Some(v) if v.eq_ignore_ascii_case("off"))
}

/// A page of this app: the bundled one (`http(s)://tauri.localhost`) or the
/// dev server's (`http://localhost:<port>`). Nothing else gets the clipboard.
pub fn own_page(uri: &str) -> bool {
    let Some((scheme, rest)) = uri.split_once("://") else {
        return false;
    };
    if scheme != "http" && scheme != "https" && scheme != "tauri" {
        return false;
    }
    let host_port = rest.split(['/', '?', '#']).next().unwrap_or("");
    let host = match host_port.rsplit_once(':') {
        Some((h, port)) if port.chars().all(|c| c.is_ascii_digit()) => h,
        _ => host_port,
    };
    matches!(host, "tauri.localhost" | "localhost" | "127.0.0.1")
}

pub fn quiet(w: &tauri::WebviewWindow) {
    use webview2_com::Microsoft::Web::WebView2::Win32::{
        ICoreWebView2Settings3, COREWEBVIEW2_PERMISSION_KIND,
        COREWEBVIEW2_PERMISSION_KIND_CLIPBOARD_READ, COREWEBVIEW2_PERMISSION_STATE_ALLOW,
    };
    use webview2_com::PermissionRequestedEventHandler;
    use windows::core::{Interface, PWSTR};

    let keys = browser_keys(
        cfg!(debug_assertions),
        std::env::var("KETE_BROWSER_KEYS").ok().as_deref(),
    );
    let _ = w.with_webview(move |pw| unsafe {
        let Ok(core) = pw.controller().CoreWebView2() else {
            return;
        };
        let Ok(settings) = core.Settings() else {
            return;
        };
        let _ = settings.SetAreDefaultContextMenusEnabled(false);
        if !keys {
            if let Ok(s3) = settings.cast::<ICoreWebView2Settings3>() {
                let _ = s3.SetAreBrowserAcceleratorKeysEnabled(false);
            }
        }
        let grant = PermissionRequestedEventHandler::create(Box::new(|_, args| {
            let Some(args) = args else {
                return Ok(());
            };
            let mut kind = COREWEBVIEW2_PERMISSION_KIND::default();
            args.PermissionKind(&mut kind)?;
            if kind != COREWEBVIEW2_PERMISSION_KIND_CLIPBOARD_READ {
                return Ok(());
            }
            let mut uri = PWSTR::null();
            args.Uri(&mut uri)?;
            if own_page(&webview2_com::take_pwstr(uri)) {
                args.SetState(COREWEBVIEW2_PERMISSION_STATE_ALLOW)?;
            }
            Ok(())
        }));
        let mut token = 0i64;
        let _ = core.add_PermissionRequested(&grant, &mut token);
        let _ = core.Reload();
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn browser_keys_only_in_debug_and_off_on_request() {
        assert!(browser_keys(true, None));
        assert!(browser_keys(true, Some("on")));
        assert!(!browser_keys(true, Some("off")));
        assert!(!browser_keys(true, Some("OFF")));
        assert!(!browser_keys(false, None));
        assert!(!browser_keys(false, Some("on")));
    }

    #[test]
    fn own_page_is_the_bundle_or_the_dev_server() {
        assert!(own_page("http://tauri.localhost/"));
        assert!(own_page("https://tauri.localhost/index.html"));
        assert!(own_page("http://localhost:1420/"));
        assert!(own_page("http://127.0.0.1:1420"));
        assert!(own_page("tauri://localhost"));
        assert!(!own_page("https://example.com/"));
        assert!(!own_page("https://tauri.localhost.example.com/"));
        assert!(!own_page("http://localhost.evil:80/"));
        assert!(!own_page("file:///C:/x.html"));
        assert!(!own_page("about:blank"));
        assert!(!own_page(""));
    }
}
