//! Defense-in-depth navigation guard.
//!
//! The frontend already intercepts clicks inside `.tome-article` and routes
//! them to either the in-app router or the system browser via the shell
//! plugin. This module is the second line: the Tauri webview itself refuses
//! to navigate to anything that isn't our own app shell, and hands external
//! URLs to the OS browser instead. If anything ever slips past the JS click
//! handler — a meta refresh, a `target=_top` link, a scripted
//! `window.location.assign`, an outright bug — the user can never get
//! "stuck" on wikipedia.org inside the app.
//!
//! Wired up as `WebviewWindowBuilder::on_navigation(...)` at window
//! construction time (which is why we build the main window in Rust setup
//! rather than declaring it in `tauri.conf.json` — the navigation hook is
//! only available on the builder).

use tauri::{AppHandle, Url};
use tauri_plugin_shell::ShellExt;

/// Whether the URL should be allowed to load inside the Tauri webview.
///
/// Returns `true` for internal URLs that belong to our app shell, dev
/// server, or our own custom protocols. Everything else is rejected and
/// (when possible) re-routed to the system browser by [`open_external`].
pub fn is_internal_url(url: &Url) -> bool {
    match url.scheme() {
        // Empty / data / blob URLs can't navigate the user away.
        "about" | "data" | "blob" => true,

        // Production app shell.
        // - macOS / Linux: tauri://localhost/
        // - Windows: https://tauri.localhost/
        "tauri" => url.host_str() == Some("localhost"),
        "https" => url.host_str() == Some("tauri.localhost"),

        // Vite dev server (and its HMR websocket).
        "http" | "ws" => matches!(url.host_str(), Some("localhost") | Some("127.0.0.1")),

        // Our own custom URI scheme for offline pmtiles.
        "tome-pmtiles" => true,

        // The pmtiles JS protocol wrapper resolves to a different inner URL
        // before fetching, so MapLibre never actually navigates the webview
        // to a pmtiles:// URL — but allow it just in case.
        "pmtiles" => true,

        _ => false,
    }
}

/// Hand `url` off to the user's system browser. Errors are swallowed because
/// the meaningful action — refusing the in-webview navigation — happens
/// regardless. Logs a warning so we'd notice in dev.
pub fn open_external(app: &AppHandle, url: &Url) {
    let url_str = url.to_string();
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        // shell::open is deprecated in favor of tauri-plugin-opener, but
        // migrating both Rust and JS sides is bigger churn than this guard
        // warrants. The deprecation is a warning, not removal.
        #[allow(deprecated)]
        if let Err(e) = app.shell().open(&url_str, None) {
            tracing::warn!("nav guard: failed to open {url_str:?} externally: {e}");
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn url(s: &str) -> Url {
        Url::parse(s).unwrap()
    }

    #[test]
    fn dev_server_is_internal() {
        assert!(is_internal_url(&url("http://localhost:1420/")));
        assert!(is_internal_url(&url("http://localhost:1420/index.html")));
    }

    #[test]
    fn production_shell_is_internal() {
        assert!(is_internal_url(&url("tauri://localhost/")));
        assert!(is_internal_url(&url("https://tauri.localhost/")));
    }

    #[test]
    fn pmtiles_protocols_are_internal() {
        assert!(is_internal_url(&url(
            "tome-pmtiles://localhost/world.pmtiles"
        )));
        assert!(is_internal_url(&url("pmtiles://example/")));
    }

    #[test]
    fn data_blob_about_are_internal() {
        assert!(is_internal_url(&url("about:blank")));
        assert!(is_internal_url(&url("data:text/plain,hello")));
        assert!(is_internal_url(&url("blob:http://localhost/abc")));
    }

    #[test]
    fn dev_hmr_websocket_is_internal() {
        assert!(is_internal_url(&url("ws://localhost:1420/")));
    }

    #[test]
    fn wikipedia_is_external() {
        assert!(!is_internal_url(&url(
            "https://en.wikipedia.org/wiki/Photon"
        )));
        assert!(!is_internal_url(&url(
            "http://en.wikipedia.org/wiki/Photon"
        )));
    }

    #[test]
    fn random_https_is_external() {
        assert!(!is_internal_url(&url("https://google.com/")));
        assert!(!is_internal_url(&url("https://attacker.example/")));
    }

    #[test]
    fn impostor_tauri_localhost_subdomain_is_external() {
        // A page that tried to navigate to tauri.localhost.attacker.com
        // would be parsed with that host string; we only accept exact match.
        assert!(!is_internal_url(&url(
            "https://tauri.localhost.attacker.com/"
        )));
    }

    #[test]
    fn custom_schemes_are_external() {
        // file:// reads can leak local paths into the page; reject.
        assert!(!is_internal_url(&url("file:///etc/passwd")));
        assert!(!is_internal_url(&url("ftp://example.org/")));
    }

    #[test]
    fn javascript_url_is_external() {
        // Won't actually parse via Url cleanly in all cases but if it does,
        // refuse it.
        if let Ok(u) = Url::parse("javascript:alert(1)") {
            assert!(!is_internal_url(&u));
        }
    }
}
