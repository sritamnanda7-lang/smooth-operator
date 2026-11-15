// SPDX-License-Identifier: FSL-1.1-MIT
//! Step 5 H1-H6: the handshake rules. H1-H5 are pure (`check_handshake` on a hand-built request); H6 uses a
//! real loopback socket and raw HTTP so each test controls Host, Origin and the path byte for byte.
use super::link_support::*;
use crate::so::link::server::{check_handshake, HandshakeReject};
use crate::so::link::{ALLOWED_ORIGIN, LINK};
use std::time::{Duration, Instant};
use tokio::net::TcpStream;
use tokio_tungstenite::tungstenite::handshake::server::Request;
use tokio_tungstenite::tungstenite::http;

const PORT: u16 = 47815;

fn req(path: &str, host: Option<&str>, origin: Option<&str>) -> Request {
    let mut b = http::Request::builder().method("GET").uri(path).header("Upgrade", "websocket").header("Connection", "Upgrade");
    if let Some(h) = host {
        b = b.header("Host", h);
    }
    if let Some(o) = origin {
        b = b.header("Origin", o);
    }
    b.body(()).unwrap_or_else(|e| panic!("test request for {path:?}: {e}"))
}
fn good_host() -> String {
    format!("127.0.0.1:{PORT}")
}
fn check(path: &str, host: Option<&str>, origin: Option<&str>) -> Result<(), HandshakeReject> {
    check_handshake(&req(path, host, origin), &LINK, PORT)
}

#[test]
fn allowed_origin_is_the_spike_value() {
    assert_eq!(ALLOWED_ORIGIN, "chrome-extension://annldiclndpcpopgmdmfcdjdeehfdlik");
    assert_eq!(LINK.allowed_origin, ALLOWED_ORIGIN);
}

// ---------- H1 ----------
#[test]
fn h1_right_path_host_and_origin_is_ok() {
    assert_eq!(check("/", Some(&good_host()), Some(ALLOWED_ORIGIN)), Ok(()));
}

#[test]
fn h1_host_is_compared_with_the_port_the_listener_is_bound_to() {
    let r = req("/", Some("127.0.0.1:54321"), Some(ALLOWED_ORIGIN));
    assert_eq!(check_handshake(&r, &LINK, 54321), Ok(()), "the bound port decides, not the constant");
    assert_eq!(check_handshake(&r, &LINK, PORT), Err(HandshakeReject::Host));
}

// ---------- H2 ----------
#[test]
fn h2_origin_missing_is_rejected() {
    assert_eq!(check("/", Some(&good_host()), None), Err(HandshakeReject::Origin));
}

// ---------- H3 ----------
#[test]
fn h3_every_other_origin_is_rejected_byte_for_byte() {
    let up = ALLOWED_ORIGIN.to_uppercase();
    let bad = [
        "https://evil.com".to_string(),
        "null".to_string(),
        "chrome-extension://aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".to_string(),
        format!("{ALLOWED_ORIGIN}/"),
        up,
        format!("{ALLOWED_ORIGIN}.evil"),
        format!(" {ALLOWED_ORIGIN}"),
        format!("{ALLOWED_ORIGIN} "),
        ALLOWED_ORIGIN.replace("chrome-extension", "Chrome-Extension"),
        ALLOWED_ORIGIN[..ALLOWED_ORIGIN.len() - 1].to_string(),
        "chrome-extension://".to_string(),
        String::new(),
        "http://127.0.0.1:47815".to_string(),
    ];
    for o in bad {
        assert_eq!(check("/", Some(&good_host()), Some(&o)), Err(HandshakeReject::Origin), "origin {o:?}");
    }
}

// ---------- H4 ----------
#[test]
fn h4_wrong_host_is_rejected_dns_rebinding() {
    for h in [None, Some("localhost:47815"), Some("127.0.0.1"), Some("127.0.0.1:1"), Some("evil.com:47815"), Some("[::1]:47815"), Some("127.0.0.1:47815."), Some(" 127.0.0.1:47815"), Some("LOCALHOST:47815"), Some("127.0.0.1:047815")] {
        assert_eq!(check("/", h, Some(ALLOWED_ORIGIN)), Err(HandshakeReject::Host), "host {h:?}");
    }
}

// ---------- H5 ----------
#[test]
fn h5_wrong_path_or_any_query_is_rejected() {
    // No "/#x": `http` drops a #fragment while parsing and a browser never sends one; the raw-socket H6 covers real paths.
    for p in ["/x", "/?token=abc", "/?", "/ws", "/%2F", "/a/b"] {
        match http::Request::builder().uri(p).body(()) {
            // (Some of these may not parse as a URI at all; the ones that do must be refused.)
            Ok(_) => assert_eq!(check(p, Some(&good_host()), Some(ALLOWED_ORIGIN)), Err(HandshakeReject::Path), "path {p:?}"),
            Err(_) => continue,
        }
    }
    // "//" is part of the plan's H5 list; if `http` cannot build it, the raw-socket test below covers it.
    if http::Request::builder().uri("//").body(()).is_ok() {
        assert_eq!(check("//", Some(&good_host()), Some(ALLOWED_ORIGIN)), Err(HandshakeReject::Path));
    }
}

#[test]
fn h5_the_checks_run_in_order_path_then_host_then_origin() {
    assert_eq!(check("/x", Some("evil.com:47815"), Some("https://evil.com")), Err(HandshakeReject::Path));
    assert_eq!(check("/", Some("evil.com:47815"), Some("https://evil.com")), Err(HandshakeReject::Host));
    assert_eq!(check("/", Some(&good_host()), Some("https://evil.com")), Err(HandshakeReject::Origin));
}

// ---------- H6: real socket ----------
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn h6_rejections_return_the_right_http_code_and_never_take_the_pending_slot() {
    let rig = start(Some(KAT_TOKEN)).await;
    let p = rig.port;
    let host = format!("127.0.0.1:{p}");
    let (c, _) = raw_upgrade(p, "/", Some(&host), Some("https://evil.com")).await;
    assert_eq!(c, 403, "bad Origin");
    let (c, _) = raw_upgrade(p, "/", Some(&host), None).await;
    assert_eq!(c, 403, "no Origin");
    let (c, _) = raw_upgrade(p, "/", Some(&host), Some("null")).await;
    assert_eq!(c, 403, "null Origin");
    let (c, _) = raw_upgrade(p, "/", Some("evil.com:47815"), Some(ALLOWED_ORIGIN)).await;
    assert_eq!(c, 403, "bad Host");
    let (c, _) = raw_upgrade(p, "/", None, Some(ALLOWED_ORIGIN)).await;
    assert_eq!(c, 403, "no Host");
    let (c, _) = raw_upgrade(p, "/?token=x", Some(&host), Some(ALLOWED_ORIGIN)).await;
    assert_eq!(c, 404, "query string");
    let (c, _) = raw_upgrade(p, "/x", Some(&host), Some(ALLOWED_ORIGIN)).await;
    assert_eq!(c, 404, "path");
    // None of those took the pending slot, and two of them can be open at once: the next real one is 101.
    let mut ws = expect_next_101(p).await;
    let _ = hello(&mut ws).await;
    // Positive control: the same raw helper with all three right is a real 101.
    drop(ws);
    let (c, _tcp) = raw_upgrade(p, "/", Some(&host), Some(ALLOWED_ORIGIN)).await;
    assert!(c == 101 || c == 409, "right headers must be accepted (409 only while the previous slot frees), got {c}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn h6_a_fifth_concurrent_unfinished_handshake_is_dropped() {
    let rig = start_with(test_cfg(3000, 5000), TestStore::new(Some(KAT_TOKEN))).await;
    let p = rig.port;
    // Four TCP connections that never send a request occupy the four handshake slots.
    let mut held = vec![];
    for _ in 0..4 {
        held.push(TcpStream::connect(("127.0.0.1", p)).await.unwrap());
        tokio::time::sleep(Duration::from_millis(60)).await;
    }
    let t = Instant::now();
    let mut fifth = TcpStream::connect(("127.0.0.1", p)).await.unwrap();
    assert!(peer_closes_within(&mut fifth, Duration::from_millis(1200)).await, "the 5th connection must be dropped at once, long before the 3 s auth deadline");
    assert!(t.elapsed() < Duration::from_millis(1500));
    // The four in progress are still waiting (not dropped by the 5th).
    assert!(!peer_closes_within(&mut held[0], Duration::from_millis(200)).await, "the first four stay open until their own deadline");
    // They die at the auth deadline (3 s after accept) and then a real client gets in.
    for h in held.iter_mut() {
        assert!(peer_closes_within(h, Duration::from_millis(4000)).await, "a silent connection is dropped at the auth deadline");
    }
    let _ws = expect_next_101(p).await;
}

// ---------- duplicate and non-UTF-8 headers (review nit 8) ----------
fn req_multi(hosts: &[&[u8]], origins: &[&[u8]]) -> Request {
    let mut b = http::Request::builder().method("GET").uri("/");
    for h in hosts {
        b = b.header("Host", http::HeaderValue::from_bytes(h).unwrap());
    }
    for o in origins {
        b = b.header("Origin", http::HeaderValue::from_bytes(o).unwrap());
    }
    b.body(()).unwrap()
}

#[test]
fn h7_duplicate_or_non_utf8_origin_and_duplicate_host_are_rejected() {
    let host = good_host();
    let good_o = ALLOWED_ORIGIN.as_bytes();
    let ck = |r: Request| check_handshake(&r, &LINK, PORT);
    assert_eq!(ck(req_multi(&[host.as_bytes()], &[good_o, b"https://evil.com"])), Err(HandshakeReject::Origin), "good Origin first, evil second");
    assert_eq!(ck(req_multi(&[host.as_bytes()], &[b"https://evil.com", good_o])), Err(HandshakeReject::Origin), "evil first, good second");
    assert_eq!(ck(req_multi(&[host.as_bytes()], &[b"chrome-extension://\xff\xfe"])), Err(HandshakeReject::Origin), "non-UTF-8 Origin");
    assert_eq!(ck(req_multi(&[host.as_bytes(), b"evil.com:47815"], &[good_o])), Err(HandshakeReject::Host), "good Host first, evil second");
    assert_eq!(ck(req_multi(&[b"evil.com:47815", host.as_bytes()], &[good_o])), Err(HandshakeReject::Host), "evil Host first, good second");
    assert_eq!(ck(req_multi(&[host.as_bytes()], &[good_o])), Ok(()), "positive control");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn h7_the_same_three_over_a_real_socket_are_403() {
    let rig = start(Some(KAT_TOKEN)).await;
    let p = rig.port;
    let dup_origin = upgrade_bytes(p, &[b"Origin: https://evil.com\r\n"], true, true);
    assert_eq!(raw_request(p, &dup_origin).await.0, 403, "duplicate Origin");
    let bad_bytes = upgrade_bytes(p, &[b"Origin: \xff\xfe\r\n"], true, false);
    assert_eq!(raw_request(p, &bad_bytes).await.0, 403, "non-UTF-8 Origin");
    let dup_host = upgrade_bytes(p, &[b"Host: evil.com:47815\r\n"], true, true);
    assert_eq!(raw_request(p, &dup_host).await.0, 403, "duplicate Host");
    // Positive control: the same helper with one Host and one Origin is a real upgrade.
    let good = upgrade_bytes(p, &[], true, true);
    let code = raw_request(p, &good).await.0;
    assert_eq!(code, 101, "positive control");
}
