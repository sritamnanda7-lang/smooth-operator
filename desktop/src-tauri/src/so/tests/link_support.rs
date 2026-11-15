// SPDX-License-Identifier: FSL-1.1-MIT
//! Shared fakes for the step-5 tests (`link_*`). Fakes only: a REAL server on 127.0.0.1 port 0, short
//! `LinkConfig` timeouts, an in-memory token store, a seeded/queued random source, an injected clock.
//! No Chrome, no Keychain. Numbers and texts follow draft/step-5-plan.md.
//!
//! The proofs are computed here with the `hmac`/`sha2` crates directly, so a mistake in `token.rs` cannot
//! hide a mistake in the server (and the other way round). HMAC KEY = THE 32 RAW TOKEN BYTES (not the hex
//! text); MESSAGE = ASCII "so-link-v1|ext|" or "so-link-v1|app|" followed by the nonce's 64 hex characters
//! as text. See the known-answer vectors below (made with `openssl dgst -sha256 -mac HMAC -macopt hexkey:`).
use super::rules_support::{wait_until, FakeClock};
use crate::so::brain::keys::KeyError;
use crate::so::link::commands::{LinkState, LinkStatus};
use crate::so::link::pairing::{Rng, RngError};
use crate::so::link::server::Link;
use crate::so::link::token::{LinkToken, TokenStore};
use crate::so::link::{LinkConfig, ALLOWED_ORIGIN};
use futures_util::{SinkExt, StreamExt};
use hmac::{Hmac, Mac};
use sha2::Sha256;
use std::collections::VecDeque;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::{client_async, WebSocketStream};

// ---------- known-answer vectors (shared with tests/so/ext/*.test.ts) ----------
/// The known-answer INPUTS are built at run time from byte ranges (not string literals): token = bytes
/// 0x00..0x20, nonce = 0x20..0x40, cnonce = 0x40..0x60, each as 64 lowercase hex. The same vectors are in
/// tests/so/ext/proof.test.ts.
fn kat_hex(from: u8) -> String {
    hex(&(from..from + 32).collect::<Vec<u8>>())
}
pub fn kat_token() -> String {
    kat_hex(0x00)
}
pub fn kat_nonce() -> String {
    kat_hex(0x20)
}
pub fn kat_cnonce() -> String {
    kat_hex(0x40)
}
// Outputs (not inputs):
/// HMAC-SHA256(key = kat_token() bytes, "so-link-v1|ext|" + kat_nonce())
pub const KAT_CLIENT_PROOF: &str = "15e438d3c342dcd8cdf294af04a47a1c86af3a0f835436997c092aa8704a60f8";
/// HMAC-SHA256(key = kat_token() bytes, "so-link-v1|app|" + kat_cnonce())
pub const KAT_SERVER_PROOF: &str = "425ee6788af9596d8d82eb6576967acafae4b8a5aea8987678e9b1e6db47274d";

// ---------- hex + hmac ----------
pub fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}
pub fn unhex(s: &str) -> Vec<u8> {
    (0..s.len() / 2).map(|i| u8::from_str_radix(&s[2 * i..2 * i + 2], 16).unwrap()).collect()
}
pub fn hmac_hex(token_hex: &str, msg: &str) -> String {
    let mut m = Hmac::<Sha256>::new_from_slice(&unhex(token_hex)).unwrap();
    m.update(msg.as_bytes());
    hex(&m.finalize().into_bytes())
}
/// The proof the extension sends for the server's `nonce`.
pub fn ext_proof(token_hex: &str, nonce: &str) -> String {
    hmac_hex(token_hex, &format!("so-link-v1|ext|{nonce}"))
}
/// The proof the app must answer with for the extension's `cnonce`.
pub fn app_proof(token_hex: &str, cnonce: &str) -> String {
    hmac_hex(token_hex, &format!("so-link-v1|app|{cnonce}"))
}
pub fn token_from(hex_text: &str) -> LinkToken {
    LinkToken::from_hex(hex_text).expect("a valid 64-hex token")
}
pub fn is_hex64(s: &str) -> bool {
    s.len() == 64 && s.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

// ---------- fake random source ----------
enum Mode {
    /// Hands out these bytes in order; `Err` when they run out.
    Queue(VecDeque<u8>),
    /// Every byte is this value, forever.
    Const(u8),
    Fail,
    /// SplitMix64 stream: unlimited, deterministic, different on every call.
    Prng(u64),
}
pub struct FakeRng {
    mode: Mutex<Mode>,
    pub calls: AtomicUsize,
}
impl FakeRng {
    pub fn queue(chunks: &[&[u8]]) -> Arc<Self> {
        Arc::new(FakeRng { mode: Mutex::new(Mode::Queue(chunks.iter().flat_map(|c| c.iter().copied()).collect())), calls: AtomicUsize::new(0) })
    }
    pub fn constant(b: u8) -> Arc<Self> {
        Arc::new(FakeRng { mode: Mutex::new(Mode::Const(b)), calls: AtomicUsize::new(0) })
    }
    pub fn failing() -> Arc<Self> {
        Arc::new(FakeRng { mode: Mutex::new(Mode::Fail), calls: AtomicUsize::new(0) })
    }
    pub fn prng(seed: u64) -> Arc<Self> {
        Arc::new(FakeRng { mode: Mutex::new(Mode::Prng(seed)), calls: AtomicUsize::new(0) })
    }
    pub fn calls(&self) -> usize {
        self.calls.load(Ordering::SeqCst)
    }
}
impl Rng for FakeRng {
    fn fill(&self, buf: &mut [u8]) -> Result<(), RngError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        match &mut *self.mode.lock().unwrap() {
            Mode::Queue(q) => {
                if q.len() < buf.len() {
                    return Err(RngError);
                }
                for b in buf.iter_mut() {
                    *b = q.pop_front().unwrap();
                }
            }
            Mode::Const(c) => buf.fill(*c),
            Mode::Fail => return Err(RngError),
            Mode::Prng(state) => {
                for b in buf.iter_mut() {
                    *state = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
                    let mut z = *state;
                    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
                    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
                    *b = ((z ^ (z >> 31)) >> 24) as u8;
                }
            }
        }
        Ok(())
    }
}

// ---------- fake token store ----------
#[derive(Default)]
struct Gate {
    closed: bool,
}
/// In-memory `TokenStore` with failure switches, call counters and a gate that makes `get` block (W17).
pub struct TestStore {
    hex: Mutex<Option<String>>,
    get_err: Mutex<Option<KeyError>>,
    set_err: Mutex<Option<KeyError>>,
    gate: Mutex<Gate>,
    gate_cv: Condvar,
    pub gets: AtomicUsize,
    pub sets: AtomicUsize,
}
impl TestStore {
    pub fn new(token_hex: Option<&str>) -> Arc<Self> {
        Arc::new(TestStore {
            hex: Mutex::new(token_hex.map(String::from)),
            get_err: Mutex::new(None),
            set_err: Mutex::new(None),
            gate: Mutex::new(Gate::default()),
            gate_cv: Condvar::new(),
            gets: AtomicUsize::new(0),
            sets: AtomicUsize::new(0),
        })
    }
    pub fn token_hex(&self) -> Option<String> {
        self.hex.lock().unwrap().clone()
    }
    pub fn fail_get(&self, e: Option<KeyError>) {
        *self.get_err.lock().unwrap() = e;
    }
    pub fn fail_set(&self, e: Option<KeyError>) {
        *self.set_err.lock().unwrap() = e;
    }
    /// From now on `get` counts itself, then waits until `release`.
    pub fn block_gets(&self) {
        self.gate.lock().unwrap().closed = true;
    }
    pub fn release(&self) {
        self.gate.lock().unwrap().closed = false;
        self.gate_cv.notify_all();
    }
    pub fn gets(&self) -> usize {
        self.gets.load(Ordering::SeqCst)
    }
}
impl TokenStore for TestStore {
    fn set(&self, t: &LinkToken) -> Result<(), KeyError> {
        self.sets.fetch_add(1, Ordering::SeqCst);
        if let Some(e) = *self.set_err.lock().unwrap() {
            return Err(e);
        }
        *self.hex.lock().unwrap() = Some(t.to_hex());
        Ok(())
    }
    fn get(&self) -> Result<Option<LinkToken>, KeyError> {
        self.gets.fetch_add(1, Ordering::SeqCst);
        let mut g = self.gate.lock().unwrap();
        while g.closed {
            g = self.gate_cv.wait(g).unwrap();
        }
        drop(g);
        if let Some(e) = *self.get_err.lock().unwrap() {
            return Err(e);
        }
        Ok(self.hex.lock().unwrap().as_deref().and_then(LinkToken::from_hex))
    }
}

// ---------- the rig: a real Link on a real loopback port ----------
pub fn test_cfg(auth_ms: u64, idle_ms: u64) -> LinkConfig {
    LinkConfig {
        port: 0,
        allowed_origin: ALLOWED_ORIGIN,
        auth_timeout: Duration::from_millis(auth_ms),
        idle_timeout: Duration::from_millis(idle_ms),
        max_msg: 4096,
        read_buffer: 4096,
        max_handshakes: 4,
    }
}

pub struct Rig {
    pub link: Arc<Link>,
    pub port: u16,
    pub store: Arc<TestStore>,
    pub clock: Arc<FakeClock>,
    task: tokio::task::JoinHandle<()>,
}
impl Drop for Rig {
    fn drop(&mut self) {
        self.task.abort();
    }
}
/// Default timeouts: auth 500 ms, idle 5 s (long: only the idle tests shorten it).
pub async fn start(token_hex: Option<&str>) -> Rig {
    start_with(test_cfg(500, 5000), TestStore::new(token_hex)).await
}
pub async fn start_with(cfg: LinkConfig, store: Arc<TestStore>) -> Rig {
    let clock = FakeClock::new();
    let rng = FakeRng::prng(7);
    let link = Link::new(cfg, store.clone(), clock.clone(), rng.clone());
    let listener = link.bind(0).await.expect("bind 127.0.0.1:0 must work");
    let port = listener.local_addr().unwrap().port();
    assert!(listener.local_addr().unwrap().ip().is_loopback(), "the listener must be loopback only");
    let task = tokio::spawn(link.clone().serve(listener));
    Rig { link, port, store, clock, task }
}

/// `Link::status` blocks at most once, so it runs on the blocking pool.
pub async fn status(link: &Arc<Link>) -> LinkStatus {
    let l = link.clone();
    tokio::task::spawn_blocking(move || l.status()).await.unwrap()
}
pub async fn wait_state(link: &Arc<Link>, want: LinkState) {
    let start = Instant::now();
    loop {
        let s = status(link).await;
        if s.state == want {
            return;
        }
        assert!(start.elapsed() < Duration::from_secs(3), "state never became {want:?}, last {:?}", s.state);
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

// ---------- client side ----------
pub type Ws = WebSocketStream<TcpStream>;
const LONG: Duration = Duration::from_secs(4);

pub async fn dial_with_origin(port: u16, origin: &str) -> Result<Ws, tokio_tungstenite::tungstenite::Error> {
    let tcp = TcpStream::connect(("127.0.0.1", port)).await.unwrap();
    let mut req = format!("ws://127.0.0.1:{port}/").into_client_request().unwrap();
    req.headers_mut().insert("Origin", origin.parse().unwrap());
    client_async(req, tcp).await.map(|x| x.0)
}
pub async fn dial(port: u16) -> Result<Ws, tokio_tungstenite::tungstenite::Error> {
    dial_with_origin(port, ALLOWED_ORIGIN).await
}
/// The HTTP status a failed upgrade got, or 101 if it worked (the socket is returned in that case).
pub async fn dial_status(port: u16) -> (u16, Option<Ws>) {
    match dial(port).await {
        Ok(ws) => (101, Some(ws)),
        Err(tokio_tungstenite::tungstenite::Error::Http(r)) => (r.status().as_u16(), None),
        Err(e) => panic!("dial failed in an unexpected way: {e:?}"),
    }
}
/// "The next connection gets 101, not 409": retries only while the answer is 409 (the old slot is being
/// freed), for at most 1.5 s. A slot that never frees fails the test.
pub async fn expect_next_101(port: u16) -> Ws {
    let start = Instant::now();
    loop {
        let (code, ws) = dial_status(port).await;
        if let Some(ws) = ws {
            return ws;
        }
        assert_eq!(code, 409, "the next connection must get 101 (or 409 while the slot frees), got {code}");
        assert!(start.elapsed() < Duration::from_millis(1500), "the pending slot was never freed (still 409 after 1.5 s)");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}
pub async fn send(ws: &mut Ws, text: &str) {
    ws.send(Message::text(text)).await.expect("send");
}
pub async fn send_bin(ws: &mut Ws, bytes: &[u8]) {
    ws.send(Message::binary(bytes.to_vec())).await.expect("send binary");
}
/// The next text message from the server (fails on close, timeout or a non-text frame).
pub async fn recv_text(ws: &mut Ws) -> String {
    match tokio::time::timeout(LONG, ws.next()).await {
        Ok(Some(Ok(Message::Text(t)))) => t.to_string(),
        other => panic!("expected a text message, got {other:?}"),
    }
}
pub async fn recv_json(ws: &mut Ws) -> serde_json::Value {
    serde_json::from_str(&recv_text(ws).await).expect("the server sent invalid JSON")
}
/// Reads until the server's Close frame; returns its code (empty reason asserted). 1006 = the stream ended
/// without a Close frame. Panics on timeout.
pub async fn expect_close(ws: &mut Ws) -> u16 {
    loop {
        match tokio::time::timeout(LONG, ws.next()).await {
            Ok(Some(Ok(Message::Close(Some(f))))) => {
                assert_eq!(f.reason.as_str(), "", "close reasons are always empty strings");
                return u16::from(f.code);
            }
            Ok(Some(Ok(Message::Close(None)))) => return 1005,
            Ok(Some(Ok(_))) => continue,
            Ok(Some(Err(_))) | Ok(None) => return 1006,
            Err(_) => panic!("timed out waiting for the server to close the connection"),
        }
    }
}
/// First message after connecting: the hello with the nonce.
pub async fn hello(ws: &mut Ws) -> String {
    let m = recv_json(ws).await;
    assert_eq!(m["t"], "status", "{m}");
    assert_eq!(m["v"], 1, "{m}");
    assert_eq!(m["state"], "hello", "{m}");
    let nonce = m["nonce"].as_str().expect("hello has a nonce").to_string();
    assert!(is_hex64(&nonce), "the nonce is 64 lowercase hex: {nonce}");
    nonce
}
pub fn auth_text(proof: &str, cnonce: &str) -> String {
    serde_json::json!({"t":"auth","v":1,"proof":proof,"cnonce":cnonce}).to_string()
}
pub fn pair_text(code: &str) -> String {
    serde_json::json!({"t":"pair","v":1,"code":code}).to_string()
}
pub fn ping_text() -> String {
    r#"{"t":"ping","v":1}"#.to_string()
}
/// A client cnonce for tests (64 hex, distinct per `n`).
pub fn cnonce(n: u8) -> String {
    hex(&[n; 32])
}
/// dial, hello, send a correct proof for `token_hex`, check `auth_ok` and the app's proof. Returns the socket.
pub async fn auth_ok(port: u16, token_hex: &str, cn: &str) -> Ws {
    let mut ws = expect_next_101(port).await;
    let nonce = hello(&mut ws).await;
    send(&mut ws, &auth_text(&ext_proof(token_hex, &nonce), cn)).await;
    let m = recv_json(&mut ws).await;
    assert_eq!(m["state"], "auth_ok", "{m}");
    assert_eq!(m["proof"], app_proof(token_hex, cn), "the app's proof must match HMAC(token, so-link-v1|app|cnonce)");
    ws
}

/// A raw HTTP upgrade request (so a test controls Host, Origin and path exactly). Returns the HTTP status
/// and the socket. `host`/`origin` `None` = header omitted.
pub async fn raw_upgrade(port: u16, path: &str, host: Option<&str>, origin: Option<&str>) -> (u16, TcpStream) {
    let mut tcp = TcpStream::connect(("127.0.0.1", port)).await.unwrap();
    let mut req = format!("GET {path} HTTP/1.1\r\n");
    if let Some(h) = host {
        req.push_str(&format!("Host: {h}\r\n"));
    }
    req.push_str("Upgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\nSec-WebSocket-Version: 13\r\n");
    if let Some(o) = origin {
        req.push_str(&format!("Origin: {o}\r\n"));
    }
    req.push_str("\r\n");
    tcp.write_all(req.as_bytes()).await.unwrap();
    let mut buf = vec![0u8; 2048];
    let n = tokio::time::timeout(LONG, tcp.read(&mut buf)).await.expect("no HTTP answer").unwrap();
    let head = String::from_utf8_lossy(&buf[..n]).to_string();
    let code = head.split_whitespace().nth(1).and_then(|c| c.parse().ok()).unwrap_or_else(|| panic!("no HTTP status line in {head:?}"));
    (code, tcp)
}
/// True when the peer closes (EOF or reset) within `max`; false if it is still open.
pub async fn peer_closes_within(tcp: &mut TcpStream, max: Duration) -> bool {
    let mut b = [0u8; 64];
    match tokio::time::timeout(max, tcp.read(&mut b)).await {
        Ok(Ok(0)) | Ok(Err(_)) => true,
        Ok(Ok(_)) => false,
        Err(_) => false,
    }
}

// ---------- log capture records ----------
/// Parses "link outcome=N code=C ms=M" records (the args part of a captured "target args" string).
pub fn link_records(records: &[String]) -> Vec<(u8, u16, u64)> {
    records.iter().filter_map(|r| parse_link(r)).collect()
}
fn parse_link(r: &str) -> Option<(u8, u16, u64)> {
    let args = r.split_once(' ').map(|x| x.1).unwrap_or(r);
    let rest = args.strip_prefix("link outcome=")?;
    let (o, rest) = rest.split_once(" code=")?;
    let (c, m) = rest.split_once(" ms=")?;
    Some((o.parse().ok()?, c.parse().ok()?, m.parse().ok()?))
}
pub fn wait_outcome(records: impl Fn() -> Vec<String>, from: usize, outcome: u8) -> (u8, u16, u64) {
    let found = std::cell::Cell::new(None);
    wait_until(&format!("link outcome={outcome}"), Duration::from_secs(3), || {
        found.set(link_records(&records()).into_iter().skip(from).find(|r| r.0 == outcome));
        found.get().is_some()
    });
    found.get().unwrap()
}
/// Every secret as text and as the lowercase hex of its ASCII bytes (tungstenite-style frame dumps).
pub fn sentinel_forms(secrets: &[String]) -> Vec<String> {
    secrets.iter().flat_map(|s| [s.clone(), hex(s.as_bytes())]).collect()
}
pub fn leaked(records: &[String], secrets: &[String]) -> Vec<String> {
    let forms = sentinel_forms(secrets);
    records.iter().filter(|r| forms.iter().any(|f| r.contains(f.as_str()))).cloned().collect()
}

// ---------- raw frames and raw requests (step 5 review follow-up) ----------
/// One client-to-server frame: FIN set, masked (all-zero mask key, so the payload is sent as is). Payload <= 125 bytes.
pub fn masked_frame(opcode: u8, payload: &[u8]) -> Vec<u8> {
    assert!(payload.len() <= 125);
    let mut f = vec![0x80 | opcode, 0x80 | payload.len() as u8, 0, 0, 0, 0];
    f.extend_from_slice(payload);
    f
}
/// Sends `request` bytes as they are and returns the HTTP status and the socket.
pub async fn raw_request(port: u16, request: &[u8]) -> (u16, TcpStream) {
    let mut tcp = TcpStream::connect(("127.0.0.1", port)).await.unwrap();
    tcp.write_all(request).await.unwrap();
    let mut buf = vec![0u8; 2048];
    let n = tokio::time::timeout(LONG, tcp.read(&mut buf)).await.expect("no HTTP answer").unwrap();
    let head = String::from_utf8_lossy(&buf[..n]).to_string();
    let code = head.split_whitespace().nth(1).and_then(|c| c.parse().ok()).unwrap_or_else(|| panic!("no HTTP status line in {head:?}"));
    (code, tcp)
}
/// A valid upgrade request with extra raw header lines (each already ending in \r\n) inserted.
pub fn upgrade_bytes(port: u16, extra_headers: &[&[u8]], with_host: bool, with_origin: bool) -> Vec<u8> {
    let mut r = b"GET / HTTP/1.1\r\n".to_vec();
    if with_host {
        r.extend_from_slice(format!("Host: 127.0.0.1:{port}\r\n").as_bytes());
    }
    r.extend_from_slice(b"Upgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\nSec-WebSocket-Version: 13\r\n");
    if with_origin {
        r.extend_from_slice(format!("Origin: {ALLOWED_ORIGIN}\r\n").as_bytes());
    }
    for h in extra_headers {
        r.extend_from_slice(h);
    }
    r.extend_from_slice(b"\r\n");
    r
}
/// Reads until the peer closes (or 4 s) and returns the code of the first Close frame (0x88) found.
pub async fn raw_close_code(tcp: &mut TcpStream) -> Option<u16> {
    let mut all = vec![];
    let _ = tokio::time::timeout(LONG, async {
        let mut b = [0u8; 512];
        loop {
            match tcp.read(&mut b).await {
                Ok(0) | Err(_) => break,
                Ok(n) => all.extend_from_slice(&b[..n]),
            }
        }
    })
    .await;
    all.windows(4).find(|w| w[0] == 0x88 && w[1] >= 2).map(|w| u16::from_be_bytes([w[2], w[3]]))
}
