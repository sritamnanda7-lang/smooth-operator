// SPDX-License-Identifier: FSL-1.1-MIT
//! Main-window navigation rule. Tests: `so/tests/nav_tests.rs` (T26).

/// True when `url` is on the app's own origin: `tauri://localhost` (macOS) or
/// `http(s)://tauri.localhost` (Windows), or, when `dev_server` is true, the Vite dev
/// server `http://localhost:1420`. Any other scheme, host or port is false.
pub(crate) fn app_origin(url: &tauri::Url, dev_server: bool) -> bool {
    let host = url.host_str();
    match url.scheme() {
        "tauri" => host == Some("localhost") && url.port().is_none(),
        "http" | "https" => {
            (host == Some("tauri.localhost") && url.port().is_none())
                || (dev_server
                    && url.scheme() == "http"
                    && host == Some("localhost")
                    && url.port() == Some(1420))
        }
        _ => false,
    }
}

/// Navigation rule for the settings window: only the app's own origin. The dev server is
/// allowed in debug builds only. Everything else (web sites, `file:`, `data:`, `javascript:`)
/// is refused, so a stray link can never load a remote page into a window that holds our commands.
pub fn main_nav_allowed(url: &tauri::Url) -> bool {
    app_origin(url, cfg!(debug_assertions))
}
