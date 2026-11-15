// SPDX-License-Identifier: FSL-1.1-MIT
//! Step 5 C1-C3: the 3 IPC commands. C1/C3 run on a real `Link` (the commands take a real `AppHandle`, so
//! like step 4 K7 the "no token in any reply" half runs on the pure functions: `Link::status`,
//! `start_pairing_with`). C2 is the real compiled ACL on the mock runtime.
use super::link_support::*;
use crate::so::brain::keys::KeyError;
use crate::so::link::commands::{start_pairing_with, LinkState};
use crate::so::link::server::Link;
use crate::so::link::PORT_BUSY_TEXT;
use crate::so::panel::LABEL as PANEL_LABEL;
use tauri::test::{get_ipc_response, mock_builder, INVOKE_KEY};
use tauri::webview::InvokeRequest;
use tauri::{ipc::CallbackFn, ipc::InvokeBody, WebviewWindow, WebviewWindowBuilder};

const TOK: &str = "abababababababababababababababababababababababababababababababab";

fn json_of(s: &crate::so::link::commands::LinkStatus) -> String {
    serde_json::to_string(s).expect("LinkStatus serialises")
}

// ---------- C1 ----------
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn c1_status_maps_each_case_and_its_json_has_no_token() {
    // NotPaired
    let rig = start(None).await;
    let s = status(&rig.link).await;
    assert_eq!((s.state, s.pairing_secs_left, s.locked_secs), (LinkState::NotPaired, None, None));
    assert_eq!(serde_json::from_str::<serde_json::Value>(&json_of(&s)).unwrap(), serde_json::json!({"state":"NotPaired","pairing_secs_left":null,"locked_secs":null}));
    // NotFound (token exists, nobody connected) + the pairing countdown
    let rig = start(Some(TOK)).await;
    assert_eq!(status(&rig.link).await.state, LinkState::NotFound);
    let view = rig.link.start_pairing().ok().unwrap();
    assert_eq!((view.code.len(), view.secs_left), (6, 120));
    assert_eq!(status(&rig.link).await.pairing_secs_left, Some(120));
    rig.clock.set(30_000);
    assert_eq!(status(&rig.link).await.pairing_secs_left, Some(90));
    rig.clock.set(121_000);
    assert_eq!(status(&rig.link).await.pairing_secs_left, None, "expired window");
    rig.clock.set(0);
    // Connected
    let _ws = auth_ok(rig.port, TOK, &cnonce(1)).await;
    wait_state(&rig.link, LinkState::Connected).await;
    let s = status(&rig.link).await;
    let j = json_of(&s);
    assert!(!j.contains(TOK) && !j.contains(&hex(TOK.as_bytes())), "{j}");
    assert!(j.contains("\"Connected\""), "{j}");
    // Locked
    let rig2 = start(None).await;
    let view = rig2.link.start_pairing().ok().unwrap();
    let wrong = if view.code == "000000" { "000001" } else { "000000" };
    for _ in 0..5 {
        let mut ws = expect_next_101(rig2.port).await;
        let _ = hello(&mut ws).await;
        send(&mut ws, &pair_text(wrong)).await;
        let _ = recv_json(&mut ws).await;
        let _ = expect_close(&mut ws).await;
    }
    assert_eq!(status(&rig2.link).await.locked_secs, Some(60));
    rig2.clock.set(61_000);
    assert_eq!(status(&rig2.link).await.locked_secs, None);
    // KeychainError
    let store = TestStore::new(Some(TOK));
    store.fail_get(Some(KeyError::Denied));
    let rig3 = start_with(test_cfg(500, 5000), store).await;
    assert_eq!(status(&rig3.link).await.state, LinkState::KeychainError);
    // PortBusy
    let squatter = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let busy = Link::new(test_cfg(500, 5000), TestStore::new(Some(TOK)), super::rules_support::FakeClock::new(), FakeRng::prng(1));
    assert!(busy.bind(squatter.local_addr().unwrap().port()).await.is_none());
    assert_eq!(status(&busy).await.state, LinkState::PortBusy);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn c1_a_dropped_session_goes_back_to_not_found() {
    let rig = start(Some(TOK)).await;
    let ws = auth_ok(rig.port, TOK, &cnonce(1)).await;
    wait_state(&rig.link, LinkState::Connected).await;
    drop(ws);
    wait_state(&rig.link, LinkState::NotFound).await;
}

// ---------- C2 ----------
const COMMANDS: [&str; 3] = ["so_link_status", "so_link_start_pairing", "so_link_cancel_pairing"];

fn request(cmd: &str) -> InvokeRequest {
    InvokeRequest {
        cmd: cmd.into(),
        callback: CallbackFn(0),
        error: CallbackFn(1),
        url: if cfg!(any(windows, target_os = "android")) { "http://tauri.localhost" } else { "tauri://localhost" }.parse().unwrap(),
        body: InvokeBody::default(),
        headers: Default::default(),
        invoke_key: INVOKE_KEY.to_string(),
    }
}
fn call(w: &WebviewWindow<tauri::test::MockRuntime>, cmd: &str) -> Result<serde_json::Value, String> {
    get_ipc_response(w, request(cmd)).map(|b| b.deserialize::<serde_json::Value>().unwrap_or(serde_json::Value::Null)).map_err(|v| v.to_string())
}

#[test]
fn c2_the_panel_and_other_windows_are_refused_all_three_main_is_allowed_all_three() {
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
    // Positive control: the ACL is live.
    assert!(call(&main, "so_get_hotkey").is_ok(), "the ACL is not live for main");
    assert!(call(&panel, "so_get_hotkey").is_err(), "the ACL is not live for the panel");
    for cmd in COMMANDS {
        for (name, w) in [("panel", &panel), ("recording_overlay", &other)] {
            match call(w, cmd) {
                Err(e) => assert!(e.contains("not allowed"), "{name} -> {cmd}: failed, but not by the ACL: {e}"),
                Ok(_) => panic!("{name} -> {cmd}: was ALLOWED, must be refused"),
            }
        }
        match call(&main, cmd) {
            Ok(v) => assert!(!v.to_string().contains("token"), "main -> {cmd}: reply must not mention a token: {v}"),
            Err(e) => panic!("main -> {cmd}: refused, must be allowed (build.rs APP_COMMANDS + capabilities/default.json): {e}"),
        }
    }
}

// ---------- C3 ----------
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn c3_start_pairing_returns_the_code_and_120_and_the_error_texts_are_fixed() {
    let rig = start(Some(TOK)).await;
    let l = rig.link.clone();
    let v = tokio::task::spawn_blocking(move || start_pairing_with(&l)).await.unwrap().ok().unwrap_or_else(|| panic!("start_pairing_with"));
    assert_eq!(v.code.len(), 6);
    assert!(v.code.bytes().all(|b| b.is_ascii_digit()));
    assert_eq!(v.secs_left, 120);
    // Locked: the exact text with N seconds.
    let wrong = if v.code == "000000" { "000001" } else { "000000" };
    for _ in 0..5 {
        let mut ws = expect_next_101(rig.port).await;
        let _ = hello(&mut ws).await;
        send(&mut ws, &pair_text(wrong)).await;
        let _ = recv_json(&mut ws).await;
        let _ = expect_close(&mut ws).await;
    }
    rig.clock.set(10_000);
    let l = rig.link.clone();
    let e = tokio::task::spawn_blocking(move || start_pairing_with(&l)).await.unwrap().err().expect("locked");
    let n: u32 = e
        .strip_prefix("Too many wrong codes. Try again in ")
        .and_then(|r| r.strip_suffix(" s."))
        .and_then(|r| r.parse().ok())
        .unwrap_or_else(|| panic!("exact text \"Too many wrong codes. Try again in N s.\", got {e:?}"));
    assert!((1..=60).contains(&n), "{n}");
    // PortBusy: exactly PORT_BUSY_TEXT.
    let squatter = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let busy = Link::new(test_cfg(500, 5000), TestStore::new(Some(TOK)), super::rules_support::FakeClock::new(), FakeRng::prng(1));
    assert!(busy.bind(squatter.local_addr().unwrap().port()).await.is_none());
    let e = start_pairing_with(&busy).err().expect("port busy");
    assert_eq!(e, PORT_BUSY_TEXT);
    assert_eq!(e, "Port 47815 is used by another app");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn c3_a_failing_random_source_gives_one_fixed_text_that_carries_no_digits() {
    let link = Link::new(test_cfg(500, 5000), TestStore::new(Some(TOK)), super::rules_support::FakeClock::new(), FakeRng::failing());
    let _listener = link.bind(0).await.expect("bind");
    let e = start_pairing_with(&link).err().expect("rng failure is an error");
    assert!(!e.is_empty() && !e.chars().any(|c| c.is_ascii_digit()), "{e:?}");
}
