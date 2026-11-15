// SPDX-License-Identifier: FSL-1.1-MIT
//! Step 5 W1-W17: the session rules on a REAL loopback server (port 0), a tokio-tungstenite client over a
//! `TcpStream`, an in-memory token store, short `LinkConfig` timeouts and an injected clock/random.
//! W2/W3/W8/W9/W14 outcomes and log checks run in child processes (one logger per process, see
//! `rules_support::run_isolated`).
use super::link_support::*;
use super::rules_support::{captured, in_child, install_capture, run_isolated};
use crate::so::brain::keys::KeyError;
use crate::so::link::commands::LinkState;
use crate::so::link::pairing::PairError;
use crate::so::link::server::Link;
use crate::so::link::{LOG_SILENCED, PORT_BUSY_TEXT};
use std::sync::Arc;
use std::time::{Duration, Instant};

const TOK: &str = "abababababababababababababababababababababababababababababababab";
const TOK2: &str = "cdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcd";

async fn pair_with(port: u16, code: &str) -> (Ws, serde_json::Value) {
    let mut ws = expect_next_101(port).await;
    let _ = hello(&mut ws).await;
    send(&mut ws, &pair_text(code)).await;
    let m = recv_json(&mut ws).await;
    (ws, m)
}

// ---------- W1 ----------
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn w1_hello_comes_first_with_a_64_hex_nonce_and_each_connection_gets_a_new_one() {
    let rig = start(Some(TOK)).await;
    let mut a = expect_next_101(rig.port).await;
    let na = hello(&mut a).await;
    drop(a);
    let mut b = expect_next_101(rig.port).await;
    let nb = hello(&mut b).await;
    assert_ne!(na, nb, "a fresh nonce per connection");
}

// ---------- W2 ----------
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn w2_a_good_proof_gets_auth_ok_with_the_app_proof_and_the_state_is_connected() {
    let rig = start(Some(TOK)).await;
    assert_eq!(status(&rig.link).await.state, LinkState::NotFound, "token stored, nobody connected");
    let _ws = auth_ok(rig.port, TOK, &cnonce(1)).await;
    wait_state(&rig.link, LinkState::Connected).await;
}

// ---------- W3 ----------
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn w3_a_bad_proof_is_rejected_4401_state_not_found_and_the_slot_is_freed() {
    let rig = start(Some(TOK)).await;
    let mut ws = expect_next_101(rig.port).await;
    let nonce = hello(&mut ws).await;
    let mut proof = ext_proof(TOK, &nonce).into_bytes();
    proof[10] = if proof[10] == b'0' { b'1' } else { b'0' };
    send(&mut ws, &auth_text(std::str::from_utf8(&proof).unwrap(), &cnonce(1))).await;
    assert_eq!(recv_json(&mut ws).await, serde_json::json!({"t":"status","v":1,"state":"rejected"}));
    assert_eq!(expect_close(&mut ws).await, 4401);
    assert_eq!(status(&rig.link).await.state, LinkState::NotFound);
    // The pending slot was freed: the next connection gets 101, not 409.
    let _next = expect_next_101(rig.port).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn w3_a_proof_for_another_nonce_or_another_token_is_rejected() {
    let rig = start(Some(TOK)).await;
    for make in [|nonce: &str| ext_proof(TOK2, nonce), |_nonce: &str| ext_proof(TOK, &"00".repeat(32)), |nonce: &str| app_proof(TOK, nonce)] {
        let mut ws = expect_next_101(rig.port).await;
        let nonce = hello(&mut ws).await;
        send(&mut ws, &auth_text(&make(&nonce), &cnonce(2))).await;
        assert_eq!(recv_json(&mut ws).await["state"], "rejected");
        assert_eq!(expect_close(&mut ws).await, 4401);
    }
}

// ---------- W4 ----------
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn w4_no_first_message_in_auth_timeout_closes_4408_and_frees_the_slot() {
    let rig = start(Some(TOK)).await;
    let mut ws = expect_next_101(rig.port).await;
    let _ = hello(&mut ws).await;
    let t = Instant::now();
    assert_eq!(expect_close(&mut ws).await, 4408);
    assert!(t.elapsed() >= Duration::from_millis(300), "not before the 500 ms deadline: {:?}", t.elapsed());
    let _next = expect_next_101(rig.port).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn w4_a_tcp_connection_with_no_handshake_is_dropped_at_the_same_deadline() {
    let rig = start(Some(TOK)).await;
    let mut tcp = tokio::net::TcpStream::connect(("127.0.0.1", rig.port)).await.unwrap();
    let t = Instant::now();
    assert!(peer_closes_within(&mut tcp, Duration::from_millis(2500)).await, "a silent TCP connection must be dropped");
    assert!(t.elapsed() < Duration::from_millis(1800), "dropped at about the 500 ms auth deadline, took {:?}", t.elapsed());
    let _next = expect_next_101(rig.port).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn w4_a_client_that_drops_mid_auth_frees_the_slot() {
    let rig = start(Some(TOK)).await;
    let mut ws = expect_next_101(rig.port).await;
    let _ = hello(&mut ws).await;
    drop(ws);
    let _next = expect_next_101(rig.port).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn w4_a_slow_handshake_is_also_covered_by_the_deadline() {
    let rig = start(Some(TOK)).await;
    // Half a request, then silence: the 5 s (here 500 ms) deadline starts at TCP accept.
    use tokio::io::AsyncWriteExt;
    let mut tcp = tokio::net::TcpStream::connect(("127.0.0.1", rig.port)).await.unwrap();
    tcp.write_all(b"GET / HTTP/1.1\r\nHost: 127.0.0").await.unwrap();
    assert!(peer_closes_within(&mut tcp, Duration::from_millis(2500)).await);
    let _next = expect_next_101(rig.port).await;
}

// ---------- W5 ----------
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn w5_bad_first_messages_close_4400_and_the_next_connection_gets_101() {
    let rig = start(Some(TOK)).await;
    let first = [
        ping_text(),
        r#"{"t":"nope","v":1}"#.to_string(),
        r#"{"t":"pair","v":1,"code":"123456","extra":1}"#.to_string(),
        r#"{"t":"pair","v":2,"code":"123456"}"#.to_string(),
        "this is not json".to_string(),
        "".to_string(),
        r#"[]"#.to_string(),
    ];
    for m in first {
        let mut ws = expect_next_101(rig.port).await;
        let _ = hello(&mut ws).await;
        send(&mut ws, &m).await;
        assert_eq!(expect_close(&mut ws).await, 4400, "first message {m:?}");
    }
    // a binary frame
    let mut ws = expect_next_101(rig.port).await;
    let _ = hello(&mut ws).await;
    send_bin(&mut ws, b"\x00\x01binary").await;
    assert_eq!(expect_close(&mut ws).await, 4400, "binary frame");
    let _next = expect_next_101(rig.port).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn w5_after_auth_only_ping_is_allowed_pair_and_auth_close_4400() {
    let rig = start(Some(TOK)).await;
    let mut ws = auth_ok(rig.port, TOK, &cnonce(1)).await;
    send(&mut ws, &pair_text("123456")).await;
    assert_eq!(expect_close(&mut ws).await, 4400, "pair after auth");
    let mut ws = auth_ok(rig.port, TOK, &cnonce(2)).await;
    send(&mut ws, &auth_text(&"ab".repeat(32), &cnonce(3))).await;
    assert_eq!(expect_close(&mut ws).await, 4400, "auth after auth");
    let mut ws = auth_ok(rig.port, TOK, &cnonce(4)).await;
    send_bin(&mut ws, b"x").await;
    assert_eq!(expect_close(&mut ws).await, 4400, "binary after auth");
    let mut ws = auth_ok(rig.port, TOK, &cnonce(5)).await;
    send(&mut ws, r#"{"t":"status","v":1,"state":"hello"}"#).await;
    assert_eq!(expect_close(&mut ws).await, 4400, "a server-only message from the client");
}

// ---------- W6 ----------
fn padded_ping(len: usize) -> String {
    let base = ping_text();
    format!("{base}{}", " ".repeat(len - base.len()))
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn w6_4096_bytes_is_accepted_4097_is_closed_1009() {
    let rig = start(Some(TOK)).await;
    let mut ws = auth_ok(rig.port, TOK, &cnonce(1)).await;
    send(&mut ws, &padded_ping(4096)).await;
    assert_eq!(recv_json(&mut ws).await, serde_json::json!({"t":"pong","v":1}), "exactly 4,096 bytes passes the size check");
    send(&mut ws, &padded_ping(4097)).await;
    assert_eq!(expect_close(&mut ws).await, 1009);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn w6_a_huge_first_message_is_closed_1009_too() {
    let rig = start(Some(TOK)).await;
    let mut ws = expect_next_101(rig.port).await;
    let _ = hello(&mut ws).await;
    send(&mut ws, &format!(r#"{{"t":"pair","v":1,"code":"{}"}}"#, "1".repeat(8000))).await;
    assert_eq!(expect_close(&mut ws).await, 1009);
    let _next = expect_next_101(rig.port).await;
}

// ---------- W7 ----------
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn w7_a_second_connection_while_one_is_pending_gets_409() {
    let rig = start(Some(TOK)).await;
    let mut first = expect_next_101(rig.port).await;
    let _ = hello(&mut first).await;
    let (code, ws) = dial_status(rig.port).await;
    assert_eq!(code, 409);
    assert!(ws.is_none());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn w7_while_one_is_authenticated_a_second_is_allowed_and_when_it_authenticates_the_first_gets_4409() {
    let rig = start(Some(TOK)).await;
    let mut first = auth_ok(rig.port, TOK, &cnonce(1)).await;
    wait_state(&rig.link, LinkState::Connected).await;
    let mut second = auth_ok(rig.port, TOK, &cnonce(2)).await;
    assert_eq!(expect_close(&mut first).await, 4409, "the newest proven extension wins");
    // The second one is alive: ping works.
    send(&mut second, &ping_text()).await;
    assert_eq!(recv_json(&mut second).await, serde_json::json!({"t":"pong","v":1}));
    wait_state(&rig.link, LinkState::Connected).await;
}

// ---------- W8 ----------
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn w8_pair_with_the_right_code_gets_a_token_equal_to_the_stored_one_closes_4000_and_the_token_works() {
    let rig = start(None).await;
    assert_eq!(status(&rig.link).await.state, LinkState::NotPaired);
    let view = rig.link.start_pairing().ok().unwrap_or_else(|| panic!("start_pairing"));
    let (mut ws, m) = pair_with(rig.port, &view.code).await;
    assert_eq!(m["state"], "paired", "{m}");
    let token = m["token"].as_str().expect("token").to_string();
    assert!(is_hex64(&token), "{token}");
    assert_eq!(rig.store.token_hex().as_deref(), Some(token.as_str()), "the token sent is the token stored");
    assert_eq!(expect_close(&mut ws).await, 4000);
    assert_eq!(status(&rig.link).await.state, LinkState::NotFound, "paired, token exists, nobody authenticated yet");
    // The extension reconnects and authenticates normally with that token.
    let _ws = auth_ok(rig.port, &token, &cnonce(1)).await;
    wait_state(&rig.link, LinkState::Connected).await;
    // The window is closed.
    assert_eq!(rig.link.status().pairing_secs_left, None);
}

// ---------- W9 ----------
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn w9_a_wrong_code_gets_wrong_code_five_wrong_across_connections_lock_for_60_s() {
    let rig = start(None).await;
    let view = rig.link.start_pairing().ok().unwrap_or_else(|| panic!("start"));
    let wrong = if view.code == "000000" { "000001" } else { "000000" };
    for i in 0..4 {
        let (mut ws, m) = pair_with(rig.port, wrong).await;
        assert_eq!(m["state"], "wrong_code", "try {i}: {m}");
        assert_eq!(expect_close(&mut ws).await, 4401);
    }
    let (mut ws, m) = pair_with(rig.port, wrong).await;
    assert_eq!(m, serde_json::json!({"t":"status","v":1,"state":"locked","retry_s":60}), "the 5th wrong try locks");
    assert_eq!(expect_close(&mut ws).await, 4401);
    // Even the right code is refused during the lock.
    let (mut ws, m) = pair_with(rig.port, &view.code).await;
    assert_eq!(m["state"], "locked", "{m}");
    assert!(m["retry_s"].as_u64().is_some_and(|s| (1..=60).contains(&s)), "{m}");
    assert_eq!(expect_close(&mut ws).await, 4401);
    assert_eq!(rig.link.status().locked_secs, Some(60));
    assert!(matches!(rig.link.start_pairing().err(), Some(PairError::Locked { .. })));
    assert_eq!(rig.store.sets.load(std::sync::atomic::Ordering::SeqCst), 0, "no token was made");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn w9_a_replayed_good_code_is_not_pairing_and_an_old_code_is_expired() {
    let rig = start(None).await;
    let view = rig.link.start_pairing().ok().unwrap();
    let (mut ws, m) = pair_with(rig.port, &view.code).await;
    assert_eq!(m["state"], "paired");
    assert_eq!(expect_close(&mut ws).await, 4000);
    let (mut ws, m) = pair_with(rig.port, &view.code).await;
    assert_eq!(m["state"], "not_pairing", "a code works once: {m}");
    assert_eq!(expect_close(&mut ws).await, 4401);
    // Expired: a new window, the clock jumps 120 s.
    let view = rig.link.start_pairing().ok().unwrap();
    rig.clock.set(120_000);
    let (mut ws, m) = pair_with(rig.port, &view.code).await;
    assert_eq!(m["state"], "expired", "{m}");
    assert_eq!(expect_close(&mut ws).await, 4401);
    // No window at all.
    let rig2 = start(None).await;
    let (mut ws, m) = pair_with(rig2.port, "123456").await;
    assert_eq!(m["state"], "not_pairing", "{m}");
    assert_eq!(expect_close(&mut ws).await, 4401);
}

// ---------- W10 ----------
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn w10_ping_gets_pong_and_keeps_the_session_alive_silence_closes_1000_state_not_found() {
    let rig = start_with(test_cfg(500, 700), TestStore::new(Some(TOK))).await;
    let mut ws = auth_ok(rig.port, TOK, &cnonce(1)).await;
    // 4 pings 400 ms apart: 1.6 s in total, longer than the 700 ms idle timeout, so a ping must reset it.
    for _ in 0..4 {
        tokio::time::sleep(Duration::from_millis(400)).await;
        send(&mut ws, &ping_text()).await;
        assert_eq!(recv_json(&mut ws).await, serde_json::json!({"t":"pong","v":1}));
    }
    wait_state(&rig.link, LinkState::Connected).await;
    // Now silence.
    assert_eq!(expect_close(&mut ws).await, 1000);
    wait_state(&rig.link, LinkState::NotFound).await;
}

// ---------- W11 ----------
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn w11_auth_with_no_token_stored_is_rejected_4401_state_not_paired() {
    let rig = start(None).await;
    let mut ws = expect_next_101(rig.port).await;
    let nonce = hello(&mut ws).await;
    send(&mut ws, &auth_text(&ext_proof(TOK, &nonce), &cnonce(1))).await;
    assert_eq!(recv_json(&mut ws).await["state"], "rejected");
    assert_eq!(expect_close(&mut ws).await, 4401);
    assert_eq!(status(&rig.link).await.state, LinkState::NotPaired);
}

// ---------- W12 ----------
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn w12_a_busy_port_gives_none_state_port_busy_and_no_pairing() {
    let squatter = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = squatter.local_addr().unwrap().port();
    let link = Link::new(test_cfg(500, 5000), TestStore::new(Some(TOK)), super::rules_support::FakeClock::new(), FakeRng::prng(1));
    assert!(link.bind(port).await.is_none(), "bind must report the busy port, not panic or retry");
    assert_eq!(status(&link).await.state, LinkState::PortBusy);
    assert_eq!(link.start_pairing().err(), Some(PairError::PortBusy));
    assert_eq!(PORT_BUSY_TEXT, "Port 47815 is used by another app");
    drop(squatter);
}

// ---------- W13 ----------
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn w13_pairing_again_closes_the_old_session_4409_and_the_old_token_stops_working() {
    let rig = start(Some(TOK)).await;
    let mut old = auth_ok(rig.port, TOK, &cnonce(1)).await;
    wait_state(&rig.link, LinkState::Connected).await;
    let view = rig.link.start_pairing().ok().unwrap();
    let (mut ws, m) = pair_with(rig.port, &view.code).await;
    assert_eq!(m["state"], "paired");
    let new_token = m["token"].as_str().unwrap().to_string();
    assert_ne!(new_token, TOK, "a new token, not the old one");
    assert_eq!(expect_close(&mut ws).await, 4000);
    assert_eq!(expect_close(&mut old).await, 4409, "the old authenticated session is closed on rotation");
    // The old token's proof now fails; the new one works.
    let mut bad = expect_next_101(rig.port).await;
    let nonce = hello(&mut bad).await;
    send(&mut bad, &auth_text(&ext_proof(TOK, &nonce), &cnonce(2))).await;
    assert_eq!(recv_json(&mut bad).await["state"], "rejected");
    assert_eq!(expect_close(&mut bad).await, 4401);
    let _good = auth_ok(rig.port, &new_token, &cnonce(3)).await;
}

// ---------- W15 ----------
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn w15_a_keychain_error_on_get_is_rejected_4401_state_keychain_error() {
    let store = TestStore::new(Some(TOK));
    store.fail_get(Some(KeyError::Denied));
    let rig = start_with(test_cfg(500, 5000), store).await;
    let mut ws = expect_next_101(rig.port).await;
    let nonce = hello(&mut ws).await;
    send(&mut ws, &auth_text(&ext_proof(TOK, &nonce), &cnonce(1))).await;
    assert_eq!(recv_json(&mut ws).await["state"], "rejected");
    assert_eq!(expect_close(&mut ws).await, 4401);
    assert_eq!(status(&rig.link).await.state, LinkState::KeychainError);
}

// ---------- W16 ----------
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn w16_a_right_code_but_a_failed_save_is_rejected_4401_no_token_and_the_code_is_burned() {
    let store = TestStore::new(None);
    store.fail_set(Some(KeyError::Backend));
    let rig = start_with(test_cfg(500, 5000), store).await;
    assert_eq!(status(&rig.link).await.state, LinkState::NotPaired);
    let view = rig.link.start_pairing().ok().unwrap();
    let (mut ws, m) = pair_with(rig.port, &view.code).await;
    assert_eq!(m, serde_json::json!({"t":"status","v":1,"state":"rejected"}), "no token may be sent when the save failed");
    assert_eq!(expect_close(&mut ws).await, 4401);
    assert_eq!(rig.store.token_hex(), None);
    assert_eq!(status(&rig.link).await.state, LinkState::NotPaired, "token-exists stays false");
    let (mut ws, m) = pair_with(rig.port, &view.code).await;
    assert_eq!(m["state"], "not_pairing", "the code stays burned: {m}");
    assert_eq!(expect_close(&mut ws).await, 4401);
}

// ---------- W17 ----------
#[tokio::test(flavor = "multi_thread", worker_threads = 3)]
async fn w17_a_blocked_keychain_read_never_piles_up_a_second_auth_is_rejected_at_once() {
    let rig = start(Some(TOK)).await;
    rig.store.block_gets();
    // First auth: the read blocks, the session waits (nothing comes back) and closes at the 500 ms deadline.
    let mut first = expect_next_101(rig.port).await;
    let nonce = hello(&mut first).await;
    send(&mut first, &auth_text(&ext_proof(TOK, &nonce), &cnonce(1))).await;
    let quiet = tokio::time::timeout(Duration::from_millis(200), recv_text(&mut first)).await;
    assert!(quiet.is_err(), "the first auth must wait for the Keychain, not answer");
    assert_eq!(expect_close(&mut first).await, 4408, "the session closes at the auth deadline");
    // Second auth while the first read is still blocked: rejected AT ONCE (well under the deadline).
    let mut second = expect_next_101(rig.port).await;
    let nonce2 = hello(&mut second).await;
    let t = Instant::now();
    send(&mut second, &auth_text(&ext_proof(TOK, &nonce2), &cnonce(2))).await;
    assert_eq!(recv_json(&mut second).await["state"], "rejected");
    assert!(t.elapsed() < Duration::from_millis(300), "rejected at once, took {:?}", t.elapsed());
    assert_eq!(expect_close(&mut second).await, 4401);
    assert_eq!(status(&rig.link).await.state, LinkState::KeychainError);
    assert_eq!(rig.store.gets(), 1, "only ONE blocking read was ever started");
    // After release the flag clears and a new auth works.
    rig.store.release();
    let t = Instant::now();
    loop {
        let mut ws = expect_next_101(rig.port).await;
        let n = hello(&mut ws).await;
        send(&mut ws, &auth_text(&ext_proof(TOK, &n), &cnonce(3))).await;
        let m = recv_json(&mut ws).await;
        if m["state"] == "auth_ok" {
            break;
        }
        assert!(t.elapsed() < Duration::from_millis(2000), "after release a new auth must work, still {m}");
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert!(rig.store.gets() >= 2);
}

// ---------- SF1: a peer that floods and never reads cannot hold the slot ----------
#[tokio::test(flavor = "multi_thread", worker_threads = 3)]
async fn sf1_a_peer_that_floods_pings_and_never_reads_does_not_hold_the_pending_slot() {
    use tokio::io::AsyncWriteExt;
    let rig = start(Some(TOK)).await; // auth deadline 500 ms
    let host = format!("127.0.0.1:{}", rig.port);
    let (code, mut tcp) = raw_upgrade(rig.port, "/", Some(&host), Some(crate::so::link::ALLOWED_ORIGIN)).await;
    assert_eq!(code, 101);
    // ~3 MB of masked Ping frames, never reading the pongs. The writer runs on its own task: it may block.
    let flood = tokio::spawn(async move {
        // 125-byte payloads: every ping is answered by a 127-byte pong, so ~3 MB in = ~3 MB of pongs the peer never reads.
        let frame = masked_frame(0x9, &[0x41; 125]);
        let chunk: Vec<u8> = frame.iter().copied().cycle().take(frame.len() * 100).collect();
        for _ in 0..250 {
            if tokio::time::timeout(Duration::from_secs(3), tcp.write_all(&chunk)).await.is_err() {
                break;
            }
        }
        tokio::time::sleep(Duration::from_secs(5)).await; // keep the socket open, still not reading
        drop(tcp);
    });
    tokio::time::sleep(Duration::from_millis(500 + 1500)).await; // auth_timeout + 1.5 s
    let (code, _ws) = dial_status(rig.port).await;
    assert_eq!(code, 101, "the pending slot must be free 1.5 s after the auth deadline, even though the peer never reads");
    flood.abort();
}

// ---------- SF2: status() never piles up blocking Keychain reads ----------
#[tokio::test(flavor = "multi_thread", worker_threads = 3)]
async fn sf2_two_status_calls_with_a_blocked_store_start_one_read_and_the_second_says_keychain_error() {
    let rig = start(Some(TOK)).await;
    struct Release(Arc<TestStore>);
    impl Drop for Release {
        fn drop(&mut self) {
            self.0.release(); // a failed assertion must not leave the blocking thread stuck
        }
    }
    rig.store.block_gets();
    let _release = Release(rig.store.clone());
    let l = rig.link.clone();
    let first = tokio::task::spawn_blocking(move || l.status());
    let start = Instant::now();
    while rig.store.gets() < 1 {
        assert!(start.elapsed() < Duration::from_secs(2), "the first status() never reached the store");
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    let t = Instant::now();
    let second = tokio::time::timeout(Duration::from_millis(500), status(&rig.link)).await.unwrap_or_else(|_| panic!("the second status() waited for the blocked read instead of answering KeychainError"));
    assert_eq!(second.state, LinkState::KeychainError, "a read is already running: answer KeychainError without reading");
    assert!(t.elapsed() < Duration::from_millis(300), "must not wait for the blocked read: {:?}", t.elapsed());
    assert_eq!(rig.store.gets(), 1, "only one blocking read exists");
    rig.store.release();
    let _ = first.await.unwrap();
}

// ---------- nit 8 over the wire: empty code ----------
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn nit8_an_empty_pair_code_is_a_wrong_try() {
    let rig = start(None).await;
    let _view = rig.link.start_pairing().ok().unwrap_or_else(|| panic!("start"));
    let (mut ws, m) = pair_with(rig.port, "").await;
    assert_eq!(m["state"], "wrong_code", "{m}");
    assert_eq!(expect_close(&mut ws).await, 4401);
}

// ---------- nit 5: invalid UTF-8 in a text frame ----------
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn nit5_an_invalid_utf8_text_frame_closes_4400() {
    use tokio::io::AsyncWriteExt;
    let rig = start(Some(TOK)).await;
    let host = format!("127.0.0.1:{}", rig.port);
    let (code, mut tcp) = raw_upgrade(rig.port, "/", Some(&host), Some(crate::so::link::ALLOWED_ORIGIN)).await;
    assert_eq!(code, 101);
    tcp.write_all(&masked_frame(0x1, &[0xff, 0xfe, 0xfd])).await.unwrap();
    assert_eq!(raw_close_code(&mut tcp).await, Some(4400));
}

// ---------- nit 2: metrics for a client that leaves and for a dropped 5th handshake ----------
#[test]
fn nit2_metrics_log_closed_25_dropped_handshake_4_and_bad_utf8_23() {
    run_isolated(module_path!(), "nit2_inner");
}
#[test]
#[ignore]
fn nit2_inner() {
    use tokio::io::AsyncWriteExt;
    if !in_child() {
        return;
    }
    install_capture();
    let rt = tokio::runtime::Builder::new_multi_thread().worker_threads(3).enable_all().build().unwrap();
    rt.block_on(async {
        let recs = captured;
        // outcome 25: an authenticated client disconnects
        let rig = start(Some(TOK)).await;
        let from = link_records(&captured()).len();
        let ws = auth_ok(rig.port, TOK, &cnonce(1)).await;
        drop(ws);
        let r = wait_outcome(recs, from, 25);
        assert_eq!(r.0, 25);
        // outcome 23: invalid UTF-8 text frame
        let host = format!("127.0.0.1:{}", rig.port);
        let from = link_records(&captured()).len();
        let (_, mut tcp) = raw_upgrade(rig.port, "/", Some(&host), Some(crate::so::link::ALLOWED_ORIGIN)).await;
        tcp.write_all(&masked_frame(0x1, &[0xff, 0xfe])).await.unwrap();
        let r = wait_outcome(recs, from, 23);
        assert_eq!(r.1, 4400);
        drop(tcp);
        // outcome 4 code 0: the 5th concurrent unfinished handshake is dropped
        let rig = start_with(test_cfg(3000, 5000), TestStore::new(Some(TOK))).await;
        let mut held = vec![];
        for _ in 0..4 {
            held.push(tokio::net::TcpStream::connect(("127.0.0.1", rig.port)).await.unwrap());
            tokio::time::sleep(Duration::from_millis(60)).await;
        }
        let from = link_records(&captured()).len();
        let mut fifth = tokio::net::TcpStream::connect(("127.0.0.1", rig.port)).await.unwrap();
        assert!(peer_closes_within(&mut fifth, Duration::from_millis(1200)).await);
        let r = wait_outcome(recs, from, 4);
        assert_eq!(r.1, 0, "a dropped handshake has no HTTP status: outcome 4 code 0");
    });
}

// ---------- W14: log checks (child processes) ----------
const RECORDS_NOTE: &str = "records of the crate (not tungstenite, tokio, mio...)";
fn ours(records: &[String]) -> Vec<String> {
    records.iter().filter(|r| r.starts_with("handy_app_lib")).cloned().collect()
}

/// Runs the plan's W2, W3, W8, W9 flows plus one scenario per metrics outcome, collecting every secret.
async fn all_scenarios(secrets: &mut Vec<String>, check_outcomes: bool) {
    let recs = || captured();
    let base = |_: ()| link_records(&captured()).len();
    macro_rules! outcome {
        ($from:expr, $n:expr, $code:expr) => {
            if check_outcomes {
                let r = wait_outcome(recs, $from, $n);
                if let Some(c) = $code {
                    assert_eq!(r.1, c, "outcome {} must carry code {}", $n, c);
                }
            }
        };
    }
    let b = base(());
    let rig = start(Some(TOK)).await; // outcome 0: listening
    outcome!(b, 0, None::<u16>);
    secrets.push(TOK.into());
    let host = format!("127.0.0.1:{}", rig.port);
    // 2, 3, 5: rejected handshakes
    let b = base(());
    let _ = raw_upgrade(rig.port, "/", Some(&host), Some("https://evil.com")).await;
    outcome!(b, 2, Some(403u16));
    let b = base(());
    let _ = raw_upgrade(rig.port, "/", Some("evil.com:47815"), Some(crate::so::link::ALLOWED_ORIGIN)).await;
    outcome!(b, 3, Some(403u16));
    let b = base(());
    let _ = raw_upgrade(rig.port, "/?token=x", Some(&host), Some(crate::so::link::ALLOWED_ORIGIN)).await;
    outcome!(b, 5, Some(404u16));
    // 4: busy
    let b = base(());
    let mut pending = expect_next_101(rig.port).await;
    secrets.push(hello(&mut pending).await);
    assert_eq!(dial_status(rig.port).await.0, 409);
    outcome!(b, 4, Some(409u16));
    // 22: the pending one times out
    let b = base(());
    assert_eq!(expect_close(&mut pending).await, 4408);
    outcome!(b, 22, Some(4408u16));
    // 21: bad proof (W3)
    let b = base(());
    let mut ws = expect_next_101(rig.port).await;
    let n = hello(&mut ws).await;
    secrets.push(n.clone());
    let wrong = ext_proof(TOK2, &n);
    secrets.push(wrong.clone());
    send(&mut ws, &auth_text(&wrong, &cnonce(9))).await;
    secrets.push(cnonce(9));
    assert_eq!(expect_close_after_json(&mut ws).await, 4401);
    outcome!(b, 21, Some(4401u16));
    // 20: auth ok (W2)
    let b = base(());
    let mut ws = expect_next_101(rig.port).await;
    let n = hello(&mut ws).await;
    let cn = cnonce(1);
    let p = ext_proof(TOK, &n);
    send(&mut ws, &auth_text(&p, &cn)).await;
    let m = recv_json(&mut ws).await;
    assert_eq!(m["state"], "auth_ok");
    secrets.extend([n, cn.clone(), p, app_proof(TOK, &cn)]);
    outcome!(b, 20, None::<u16>);
    // 26: replaced
    let b = base(());
    let _second = auth_ok(rig.port, TOK, &cnonce(2)).await; // stays open: an authenticated session does not block a pending slot
    secrets.extend([cnonce(2), app_proof(TOK, &cnonce(2))]);
    assert_eq!(expect_close(&mut ws).await, 4409);
    outcome!(b, 26, Some(4409u16));
    // 23: bad message (a ping first is not allowed; use a fresh pending socket) and 24: too big
    let b = base(());
    let mut ws = expect_next_101(rig.port).await;
    secrets.push(hello(&mut ws).await);
    send(&mut ws, &ping_text()).await;
    assert_eq!(expect_close(&mut ws).await, 4400);
    outcome!(b, 23, Some(4400u16));
    let b = base(());
    let mut ws = expect_next_101(rig.port).await;
    secrets.push(hello(&mut ws).await);
    send(&mut ws, &format!(r#"{{"t":"pair","v":1,"code":"{}"}}"#, "1".repeat(8000))).await;
    assert_eq!(expect_close(&mut ws).await, 1009);
    outcome!(b, 24, Some(1009u16));
    // 11, 12, 13, 10: pairing (W9, W8)
    let b = base(());
    let (mut ws, _) = pair_with(rig.port, "123456").await; // no window yet
    let _ = expect_close(&mut ws).await;
    outcome!(b, 13, Some(4401u16));
    let view = rig.link.start_pairing().ok().unwrap();
    secrets.push(view.code.clone());
    let wrong_code = if view.code == "000000" { "000001" } else { "000000" };
    let b = base(());
    let (mut ws, _) = pair_with(rig.port, wrong_code).await;
    let _ = expect_close(&mut ws).await;
    outcome!(b, 11, Some(4401u16));
    let b = base(());
    for _ in 0..4 {
        let (mut ws, _) = pair_with(rig.port, wrong_code).await;
        let _ = expect_close(&mut ws).await;
    }
    outcome!(b, 12, Some(4401u16));
    rig.clock.set(61_000 + 120_000);
    let view = rig.link.start_pairing().ok().unwrap();
    secrets.push(view.code.clone());
    let b = base(());
    let (mut ws, m) = pair_with(rig.port, &view.code).await;
    assert_eq!(m["state"], "paired");
    secrets.push(m["token"].as_str().unwrap().to_string());
    let _ = expect_close(&mut ws).await;
    outcome!(b, 10, Some(4000u16));
    // 25: idle (own rig with a short idle timeout)
    let rig2 = start_with(test_cfg(500, 400), TestStore::new(Some(TOK))).await;
    let b = base(());
    let mut ws = auth_ok(rig2.port, TOK, &cnonce(4)).await;
    secrets.extend([cnonce(4), app_proof(TOK, &cnonce(4))]);
    assert_eq!(expect_close(&mut ws).await, 1000);
    outcome!(b, 25, Some(1000u16));
    // 27: keychain error
    let store = TestStore::new(Some(TOK));
    store.fail_get(Some(KeyError::Locked));
    let rig3 = start_with(test_cfg(500, 5000), store).await;
    let b = base(());
    let mut ws = expect_next_101(rig3.port).await;
    let n = hello(&mut ws).await;
    secrets.push(n.clone());
    send(&mut ws, &auth_text(&ext_proof(TOK, &n), &cnonce(5))).await;
    let _ = expect_close_after_json(&mut ws).await;
    outcome!(b, 27, Some(4401u16));
}
/// Reads the `rejected` status then returns the close code.
async fn expect_close_after_json(ws: &mut Ws) -> u16 {
    let _ = recv_json(ws).await;
    expect_close(ws).await
}

#[test]
fn w14a_nothing_secret_reaches_our_log_at_trace_and_every_link_record_is_integers_only() {
    run_isolated(module_path!(), "w14a_inner");
}
#[test]
#[ignore]
fn w14a_inner() {
    if !in_child() {
        return;
    }
    install_capture();
    // Positive control: the capture works and the scanner finds a deliberate leak.
    log::info!(target: "positive_control", "oops {TOK}");
    assert_eq!(leaked(&captured(), &[TOK.to_string()]).len(), 1, "the scanner cannot fail");
    let baseline = captured().len();
    let rt = tokio::runtime::Builder::new_multi_thread().worker_threads(3).enable_all().build().unwrap();
    let mut secrets = vec![];
    rt.block_on(all_scenarios(&mut secrets, true));
    std::thread::sleep(Duration::from_millis(200));
    let after: Vec<String> = captured().into_iter().skip(baseline).collect();
    // Positive control for the library: tungstenite DOES log at Trace here, so the app-side Off filter matters.
    assert!(after.iter().any(|r| r.starts_with("tungstenite")), "no tungstenite record at Trace: the scenario does not exercise the library ({RECORDS_NOTE})");
    let mine = ours(&after);
    assert!(!link_records(&mine).is_empty(), "no link metrics records at all: {mine:?}");
    assert_eq!(leaked(&mine, &secrets), Vec::<String>::new(), "a secret (text or ASCII hex) reached our log");
    // Every record that comes from so::link must be exactly the integer line.
    for r in mine.iter().filter(|r| r.contains("::so::link")) {
        assert!(parse_link_line(r), "not an integers-only link record: {r}");
    }
}
fn parse_link_line(r: &str) -> bool {
    link_records(std::slice::from_ref(&r.to_string())).len() == 1
}

#[test]
fn w14b_the_apps_filter_rule_drops_tungstenite_at_trace_and_keeps_link_records() {
    run_isolated(module_path!(), "w14b_inner");
}
struct Sink;
static SINK: std::sync::Mutex<Vec<String>> = std::sync::Mutex::new(Vec::new());
impl log::Log for Sink {
    fn enabled(&self, _: &log::Metadata) -> bool {
        true
    }
    fn log(&self, r: &log::Record) {
        SINK.lock().unwrap().push(format!("{} {}", r.target(), r.args()));
    }
    fn flush(&self) {}
}
#[test]
#[ignore]
fn w14b_inner() {
    if !in_child() {
        return;
    }
    // The same rule lib.rs applies: Trace everywhere, then `level_for(t, Off)` for each LOG_SILENCED entry.
    let mut d = tauri_plugin_log::fern::Dispatch::new().level(log::LevelFilter::Trace);
    for t in LOG_SILENCED {
        d = d.level_for(*t, log::LevelFilter::Off);
    }
    d.chain(Box::new(Sink) as Box<dyn log::Log>).apply().expect("one logger per process");
    // Positive control: synthetic records. A library at Trace is dropped; ours is kept.
    log::trace!(target: "tungstenite::protocol", "synthetic frame {TOK}");
    log::trace!(target: "tokio_tungstenite", "synthetic message {TOK}");
    log::info!(target: "handy_app_lib::so::link::metrics", "link outcome=20 code=0 ms=1");
    {
        let s = SINK.lock().unwrap();
        assert!(s.iter().all(|r| !r.starts_with("tungstenite") && !r.starts_with("tokio_tungstenite")), "{s:?}");
        assert_eq!(s.len(), 1, "exactly our record is kept: {s:?}");
    }
    // The real flows (W2, W3, W8, W9) through the real library with the filter on.
    let rt = tokio::runtime::Builder::new_multi_thread().worker_threads(3).enable_all().build().unwrap();
    let mut secrets = vec![];
    rt.block_on(all_scenarios(&mut secrets, false));
    std::thread::sleep(Duration::from_millis(200));
    let s = SINK.lock().unwrap().clone();
    assert!(s.iter().all(|r| !r.starts_with("tungstenite") && !r.starts_with("tokio_tungstenite")), "a silenced library reached the log");
    assert!(!link_records(&s).is_empty(), "our link records must survive the filter");
    assert_eq!(leaked(&s, &secrets), Vec::<String>::new(), "a secret reached the filtered log");
}

#[test]
fn w14b_log_silenced_names_both_libraries() {
    assert!(LOG_SILENCED.contains(&"tungstenite") && LOG_SILENCED.contains(&"tokio_tungstenite"));
}
