// SPDX-License-Identifier: FSL-1.1-MIT
//! Step 4 K7: the REAL compiled capabilities decide which webview may call the 6 `so_brain_*` commands. Same
//! method as `ipc_acl_tests.rs` (mock runtime, real `generate_context!`, a stub handler that answers `null`
//! to anything that gets past the ACL). The commands themselves take a real `AppHandle` (Wry), so the
//! "no sentinel in any reply" half of K7 runs on the pure `*_with` functions (brain_keys_tests K4/K5).
use crate::so::panel::LABEL as PANEL_LABEL;
use tauri::test::{get_ipc_response, mock_builder, INVOKE_KEY};
use tauri::webview::InvokeRequest;
use tauri::{ipc::CallbackFn, ipc::InvokeBody, WebviewWindow, WebviewWindowBuilder};

const COMMANDS: [&str; 6] = [
    "so_brain_status",
    "so_brain_set_provider",
    "so_brain_save_key",
    "so_brain_remove_key",
    "so_brain_check_key",
    "so_brain_open_console",
];

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

fn call(w: &WebviewWindow<tauri::test::MockRuntime>, cmd: &str) -> Result<(), String> {
    match get_ipc_response(w, request(cmd)) {
        Ok(_) => Ok(()),
        Err(v) => Err(v.to_string()),
    }
}

#[test]
fn k7_the_panel_and_other_windows_are_refused_all_six_main_is_allowed_all_six() {
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

    // Positive control: the ACL is live (a known step-1 command is allowed for main, refused for the panel).
    assert!(call(&main, "so_get_hotkey").is_ok(), "the ACL is not live for main");
    assert!(call(&panel, "so_get_hotkey").is_err(), "the ACL is not live for the panel");

    for cmd in COMMANDS {
        match call(&panel, cmd) {
            Err(e) => assert!(e.contains("not allowed"), "panel -> {cmd}: failed, but not by the ACL: {e}"),
            Ok(()) => panic!("panel -> {cmd}: was ALLOWED, must be refused"),
        }
        match call(&other, cmd) {
            Err(e) => assert!(e.contains("not allowed"), "other -> {cmd}: failed, but not by the ACL: {e}"),
            Ok(()) => panic!("recording_overlay -> {cmd}: was ALLOWED, must be refused"),
        }
        if let Err(e) = call(&main, cmd) {
            panic!("main -> {cmd}: refused, must be allowed (is it in build.rs APP_COMMANDS and capabilities/default.json?): {e}");
        }
    }
}
