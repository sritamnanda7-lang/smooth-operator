// SPDX-License-Identifier: FSL-1.1-MIT
//! Gate 3: the floating panel. A small window that never takes focus, shows
//! text only, and cannot navigate anywhere.
//! - macOS: a non-activating NSPanel (same technique as Handy's overlay), floats over full-screen apps.
//! - Windows: a window with `focusable(false)`, always on top.
//! - Text arrives only through `eval` of a fixed call, JSON-escaped, and the page writes it with
//!   `textContent` (see public/spike/panel.js). The page has no IPC access at all.
//! - `on_navigation` allows only the panel's own page; `on_new_window` denies every `window.open`.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::Mutex;
use tauri::{AppHandle, Manager, WebviewUrl};

#[cfg(target_os = "macos")]
use tauri_nspanel::{tauri_panel, CollectionBehavior, PanelBuilder, PanelLevel, StyleMask};

pub const LABEL: &str = "spike_panel";
const WIDTH: f64 = 440.0;
const HEIGHT: f64 = 150.0;
const MAX_LINES: usize = 6;
const HIDE_AFTER_MS: u64 = 6000;

/// Counters the self-test button reads.
pub static NAV_BLOCKED: AtomicUsize = AtomicUsize::new(0);
pub static NEW_WINDOW_BLOCKED: AtomicUsize = AtomicUsize::new(0);

static LINES: Mutex<VecDeque<String>> = Mutex::new(VecDeque::new());
/// Bumped on every update so an older "hide later" timer does nothing.
static GENERATION: AtomicU64 = AtomicU64::new(0);

#[cfg(target_os = "macos")]
tauri_panel! {
    panel!(SpikePanel {
        config: {
            can_become_key_window: false,
            is_floating_panel: true
        }
    })
}

/// Only the panel's own page may load. Everything else (any `location=`) is refused.
fn allowed(url: &tauri::Url) -> bool {
    let local_host = matches!(url.host_str(), Some("tauri.localhost") | Some("localhost"));
    let local_scheme = url.scheme() == "tauri" || (local_host && matches!(url.scheme(), "http" | "https"));
    local_scheme && url.path() == "/spike/panel.html"
}

pub fn create(app: &AppHandle) {
    let url = WebviewUrl::App("spike/panel.html".into());
    let nav = |u: &tauri::Url| {
        let ok = allowed(u);
        if !ok {
            NAV_BLOCKED.fetch_add(1, Ordering::SeqCst);
            super::harness::event("g3", "navigation_blocked");
        }
        ok
    };
    let new_window = |_u: tauri::Url, _f: tauri::webview::NewWindowFeatures| {
        NEW_WINDOW_BLOCKED.fetch_add(1, Ordering::SeqCst);
        super::harness::event("g3", "window_open_blocked");
        tauri::webview::NewWindowResponse::Deny
    };

    #[cfg(target_os = "macos")]
    {
        let built = PanelBuilder::<_, SpikePanel>::new(app, LABEL)
            .url(url)
            .title("Smooth Operator panel")
            .level(PanelLevel::Status)
            .size(tauri::Size::Logical(tauri::LogicalSize { width: WIDTH, height: HEIGHT }))
            .has_shadow(false)
            .transparent(true)
            .no_activate(true)
            .corner_radius(0.0)
            .style_mask(StyleMask::empty().borderless().nonactivating_panel())
            .with_window(move |w| {
                w.decorations(false)
                    .transparent(true)
                    .focusable(false)
                    .on_navigation(nav)
                    .on_new_window(new_window)
            })
            .collection_behavior(CollectionBehavior::new().can_join_all_spaces().full_screen_auxiliary())
            .build();
        match built {
            Ok(panel) => panel.hide(),
            Err(_) => super::harness::event("g3", "panel_create_failed"),
        }
    }

    #[cfg(not(target_os = "macos"))]
    {
        let built = tauri::WebviewWindowBuilder::new(app, LABEL, url)
            .title("Smooth Operator panel")
            .resizable(false)
            .inner_size(WIDTH, HEIGHT)
            .shadow(false)
            .maximizable(false)
            .minimizable(false)
            .closable(false)
            .decorations(false)
            .always_on_top(true)
            .skip_taskbar(true)
            .transparent(true)
            .focusable(false)
            .focused(false)
            .visible(false)
            .on_navigation(nav)
            .on_new_window(new_window)
            .build();
        if built.is_err() {
            super::harness::event("g3", "panel_create_failed");
        }
    }
}

/// Add a line of text to the panel and show it for a few seconds. Safe from any thread.
pub fn say(app: &AppHandle, text: &str) {
    let joined = {
        let mut l = LINES.lock().unwrap_or_else(|e| e.into_inner());
        l.push_back(text.to_string());
        while l.len() > MAX_LINES {
            l.pop_front();
        }
        l.iter().cloned().collect::<Vec<_>>().join("\n")
    };
    let gen = GENERATION.fetch_add(1, Ordering::SeqCst) + 1;
    let handle = app.clone();
    let _ = app.run_on_main_thread(move || {
        let Some(win) = handle.get_webview_window(LABEL) else { return };
        // serde_json::to_string makes a safe JS string literal (quotes, newlines escaped).
        let js = format!("window.soPanel&&window.soPanel.set({});", serde_json::to_string(&joined).unwrap_or_default());
        let _ = win.eval(&js);
        if let Ok(Some(m)) = handle.primary_monitor() {
            let s = m.scale_factor();
            let x = m.position().x as f64 / s + (m.size().width as f64 / s - WIDTH) / 2.0;
            let y = m.position().y as f64 / s + 40.0;
            let _ = win.set_position(tauri::Position::Logical(tauri::LogicalPosition { x, y }));
        }
        let _ = win.show();
    });
    let handle = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(HIDE_AFTER_MS));
        if GENERATION.load(Ordering::SeqCst) == gen {
            let h2 = handle.clone();
            let _ = handle.run_on_main_thread(move || {
                if let Some(win) = h2.get_webview_window(LABEL) {
                    let _ = win.hide();
                }
            });
        }
    });
}

/// Gate 3 self-test: make the panel page try `location=` and `window.open`.
/// Returns after a short wait with how many of each were blocked.
pub fn navigation_selftest(app: &AppHandle) -> (usize, usize) {
    let before = (NAV_BLOCKED.load(Ordering::SeqCst), NEW_WINDOW_BLOCKED.load(Ordering::SeqCst));
    if let Some(win) = app.get_webview_window(LABEL) {
        let _ = win.eval("try{window.open('https://example.com/')}catch(e){}");
        let _ = win.eval("try{location='https://example.com/'}catch(e){}");
    }
    std::thread::sleep(std::time::Duration::from_millis(1500));
    (
        NAV_BLOCKED.load(Ordering::SeqCst) - before.0,
        NEW_WINDOW_BLOCKED.load(Ordering::SeqCst) - before.1,
    )
}

#[cfg(test)]
mod tests {
    use super::allowed;

    fn u(s: &str) -> tauri::Url {
        s.parse().unwrap()
    }

    #[test]
    fn own_page_only() {
        assert!(allowed(&u("tauri://localhost/spike/panel.html")));
        assert!(allowed(&u("http://tauri.localhost/spike/panel.html")));
        assert!(!allowed(&u("https://example.com/")));
        assert!(!allowed(&u("https://example.com/spike/panel.html")));
        assert!(!allowed(&u("tauri://localhost/index.html")));
        assert!(!allowed(&u("file:///etc/passwd")));
    }
}
