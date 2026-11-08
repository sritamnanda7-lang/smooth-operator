// SPDX-License-Identifier: FSL-1.1-MIT
//! Automated replacement for manual M8: the REAL compiled capabilities + AppManifest (via
//! `generate_context!`) decide which webview may call which command. A mock runtime hosts the webviews;
//! the handler is a stub that answers `null` to anything that gets past the ACL, so "Ok" means "allowed"
//! and an "not allowed" error means "refused by the ACL".
//!
//! Already covered elsewhere (not repeated here): navigation + new-window denial (T23, T26 rules; S12
//! wiring), frozen `window.soPanel` (V1), no `.eval(` beyond one (S13), no `core:default` / floor grants (S2).
use crate::so::panel::LABEL as PANEL_LABEL;
use tauri::test::{get_ipc_response, mock_builder, INVOKE_KEY};
use tauri::webview::InvokeRequest;
use tauri::{ipc::CallbackFn, ipc::InvokeBody, WebviewWindow, WebviewWindowBuilder};

fn request(cmd: &str) -> InvokeRequest {
    InvokeRequest {
        cmd: cmd.into(),
        callback: CallbackFn(0),
        error: CallbackFn(1),
        url: if cfg!(any(windows, target_os = "android")) { "http://tauri.localhost" } else { "tauri://localhost" }
            .parse()
            .unwrap(),
        body: InvokeBody::default(),
        headers: Default::default(),
        invoke_key: INVOKE_KEY.to_string(),
    }
}

/// Some(error text) when refused, None when the call got through.
fn call(w: &WebviewWindow<tauri::test::MockRuntime>, cmd: &str) -> Result<(), String> {
    match get_ipc_response(w, request(cmd)) {
        Ok(_) => Ok(()),
        Err(v) => Err(v.to_string()),
    }
}

fn assert_refused(w: &WebviewWindow<tauri::test::MockRuntime>, label: &str, cmd: &str) {
    match call(w, cmd) {
        Err(e) => assert!(e.contains("not allowed"), "{label} -> {cmd}: failed, but not by the ACL: {e}"),
        Ok(()) => panic!("{label} -> {cmd}: was ALLOWED, must be refused"),
    }
}

fn assert_allowed(w: &WebviewWindow<tauri::test::MockRuntime>, label: &str, cmd: &str) {
    if let Err(e) = call(w, cmd) {
        panic!("{label} -> {cmd}: refused, must be allowed (is the ACL live?): {e}");
    }
}

#[test]
fn m8_real_acl_panel_gets_nothing_main_gets_only_the_allow_list() {
    let app = mock_builder()
        .invoke_handler(|invoke| {
            invoke.resolver.respond(Ok(()));
            true
        })
        .build(crate::app_context())
        .expect("mock app with the real context");

    let panel = WebviewWindowBuilder::new(&app, PANEL_LABEL, Default::default()).build().unwrap();
    let main = WebviewWindowBuilder::new(&app, "main", Default::default()).build().unwrap();
    let other = WebviewWindowBuilder::new(&app, "recording_overlay", Default::default()).build().unwrap();

    // Positive controls: the ACL is live and the main window keeps what the visible panes need.
    for cmd in ["get_app_settings", "so_get_hotkey", "so_set_hotkey", "so_hotkey_options", "so_fn_check"] {
        assert_allowed(&main, "main", cmd);
    }

    // Panel: zero IPC. App commands, our own commands, and the event plugin are all refused.
    for cmd in [
        "get_app_settings",
        "so_get_hotkey",
        "so_set_hotkey",
        "so_fn_check",
        "start_handy_keys_recording",
        "change_autostart_setting",
        "plugin:event|listen",
        "plugin:event|emit",
        "plugin:process|restart",
        "plugin:os|locale",
    ] {
        assert_refused(&panel, PANEL_LABEL, cmd);
    }

    // Main: the floor commands (and powers it never needs) stay refused.
    for cmd in [
        "start_handy_keys_recording",
        "stop_handy_keys_recording",
        "change_autostart_setting",
        "plugin:event|emit",
        "plugin:window|close",
    ] {
        assert_refused(&main, "main", cmd);
    }

    // Any other label has no capability at all.
    assert_refused(&other, "recording_overlay", "get_app_settings");
}
