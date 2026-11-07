// SPDX-License-Identifier: FSL-1.1-MIT
//! The spike's control window: plain buttons for gates that need a human click
//! (stall main thread, panel navigation self-test, model preload/unload, RSS).
//! Own page (public/spike/controls.html), strict CSP from tauri.conf.json, no plugin access.

use tauri::{AppHandle, Manager, WebviewUrl};

pub const LABEL: &str = "spike_controls";

pub fn create(app: &AppHandle) {
    let built = tauri::WebviewWindowBuilder::new(app, LABEL, WebviewUrl::App("spike/controls.html".into()))
        .title(format!("Spike harness {}", app.package_info().version))
        .inner_size(480.0, 360.0)
        .on_navigation(|u| matches!(u.scheme(), "tauri") || matches!(u.host_str(), Some("tauri.localhost") | Some("localhost")))
        .on_new_window(|_u, _f| tauri::webview::NewWindowResponse::Deny)
        .build();
    if built.is_err() {
        super::harness::event("boot", "controls_create_failed");
    }
}

/// Show a result line in the controls window (page writes it with textContent).
pub fn set(app: &AppHandle, text: &str) {
    if let Some(w) = app.get_webview_window(LABEL) {
        let js = format!("window.soControls&&window.soControls.set({});", serde_json::to_string(text).unwrap_or_default());
        let _ = w.eval(&js);
    }
}
