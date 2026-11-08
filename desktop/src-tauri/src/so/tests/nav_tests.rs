// SPDX-License-Identifier: FSL-1.1-MIT
//! T23 (panel navigation rule) and T26 (main window navigation rule).
use crate::so::{panel, webview};

fn u(s: &str) -> tauri::Url {
    s.parse().unwrap()
}
fn own(host_scheme: &str) -> tauri::Url {
    u(&format!("{host_scheme}{}", panel::PAGE_PATH))
}

#[test]
fn t23_panel_own_page_only() {
    assert!(panel::nav_allowed(&own("tauri://localhost"), false));
    assert!(panel::nav_allowed(&own("http://tauri.localhost"), false));
    assert!(panel::nav_allowed(&own("https://tauri.localhost"), false));
    assert!(!panel::nav_allowed(&u("https://example.com/"), false));
    assert!(!panel::nav_allowed(&own("https://example.com"), false));
    assert!(!panel::nav_allowed(&u("tauri://localhost/index.html"), false));
    assert!(!panel::nav_allowed(&u("tauri://localhost/src/overlay/index.html"), false));
    assert!(!panel::nav_allowed(&u("file:///etc/passwd"), false));
    assert!(!panel::nav_allowed(&u("data:text/plain,blocked"), false));
    assert!(!panel::nav_allowed(&u("javascript:alert(1)"), false));
    assert!(!panel::nav_allowed(&u("about:blank"), false));
}

#[test]
fn t23_panel_dev_url_only_in_debug() {
    let dev = u(&format!("http://localhost:1420{}", panel::PAGE_PATH));
    assert!(panel::nav_allowed(&dev, true));
    assert!(!panel::nav_allowed(&dev, false));
    // Even in debug only the exact dev origin + panel page.
    assert!(!panel::nav_allowed(&u("http://localhost:1420/index.html"), true));
    assert!(!panel::nav_allowed(&u(&format!("http://localhost:9999{}", panel::PAGE_PATH)), true));
    assert!(!panel::nav_allowed(&u(&format!("http://evil.localhost:1420{}", panel::PAGE_PATH)), true));
}

#[test]
fn t26_main_app_origin_only() {
    assert!(webview::main_nav_allowed(&u("tauri://localhost/index.html")));
    assert!(webview::main_nav_allowed(&u("tauri://localhost/")));
    assert!(webview::main_nav_allowed(&u("http://tauri.localhost/index.html")));
    assert!(webview::main_nav_allowed(&u("https://tauri.localhost/")));
    assert!(!webview::main_nav_allowed(&u("https://example.com/")));
    assert!(!webview::main_nav_allowed(&u("https://tauri.localhost.evil.com/")));
    assert!(!webview::main_nav_allowed(&u("http://tauri.evil.com/")));
    assert!(!webview::main_nav_allowed(&u("file:///etc/passwd")));
    assert!(!webview::main_nav_allowed(&u("data:text/html,<script>1</script>")));
    assert!(!webview::main_nav_allowed(&u("javascript:alert(1)")));
    assert!(!webview::main_nav_allowed(&u("about:blank")));
    assert!(!webview::main_nav_allowed(&u("ftp://tauri.localhost/")));
}

#[test]
fn t26_main_dev_server_only_in_debug_builds() {
    assert_eq!(webview::main_nav_allowed(&u("http://localhost:1420/")), cfg!(debug_assertions));
    assert!(!webview::main_nav_allowed(&u("http://localhost:9999/")));
}
