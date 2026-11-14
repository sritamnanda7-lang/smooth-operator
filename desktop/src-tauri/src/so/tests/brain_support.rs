// SPDX-License-Identifier: FSL-1.1-MIT
//! Shared fakes for the step-4 tests (`brain_*`). Fakes only: a local 127.0.0.1 HTTP server, scripted
//! providers, an in-memory key store, a fake brain for the seam. No real key, no real Keychain, no real AI.
//! Numbers and texts follow draft/step-4-plan.md.
//!
//! SENTINELS ARE REAL WORDS (not random bytes) so the no-leak checks look like real speech, plan text and
//! error bodies. Every no-leak test first proves its scanner can fail (a deliberate leak is found).
use super::rules_support::{FakeClock, Log, Rig, FakeRec, FakeTap, FakeStopper, FakePanel, FakeTr, SAMPLES};
use crate::so::brain::brain::{Brain, BrainChoice, Limits};
use crate::so::brain::keys::{ApiKey, KeyError, KeyStore, MemoryKeyStore};
use crate::so::brain::provider::{BrainRequest, Provider, ProviderError, ProviderId};
use crate::so::panel::Line;
use crate::so::rules::{Reply, SlowRouter, TranscriptRouter};
use crate::so::settings::HotkeyChoice;
use crate::so::stt::sink::SpeechSink;
use std::collections::VecDeque;
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::thread;
use std::time::{Duration, Instant};

// ---------- sentinels (real words) ----------
/// A key-shaped value (20-256 visible ASCII) made of real words.
pub const KEY_SENTINEL: &str = "AIzaSyPumpernickelWalrus0123456789";
pub const KEY_WORD: &str = "pumpernickel";
/// Transcript words (what the user said).
pub const SAID_WORD: &str = "walrus";
/// Words inside the AI's reply (plan args, question text).
pub const REPLY_WORD: &str = "kumquat";
/// Words inside an error body from the provider.
pub const BODY_WORD: &str = "pomegranate";
/// Every sentinel word, lowercase.
pub const ALL_WORDS: [&str; 4] = [KEY_WORD, SAID_WORD, REPLY_WORD, BODY_WORD];

/// Records (lowercased) that contain any sentinel word.
pub fn leaks(records: &[String]) -> Vec<String> {
    records.iter().filter(|r| ALL_WORDS.iter().any(|w| r.to_lowercase().contains(w))).cloned().collect()
}

// ---------- keys ----------
pub fn key(s: &str) -> ApiKey {
    match ApiKey::parse(s) {
        Ok(k) => k,
        Err(e) => panic!("test key {s:?} was refused by ApiKey::parse: {e:?}"),
    }
}
/// The key text as the store holds it (`None` = no key).
pub fn stored(store: &dyn KeyStore, p: ProviderId) -> Option<String> {
    store.get(p).ok().flatten().map(|k| k.expose().to_string())
}

/// A key store that fails every call with a fixed error (Keychain locked / denied).
pub struct FailStore(pub KeyError);
impl KeyStore for FailStore {
    fn set(&self, _p: ProviderId, _key: &ApiKey) -> Result<(), KeyError> {
        Err(self.0)
    }
    fn get(&self, _p: ProviderId) -> Result<Option<ApiKey>, KeyError> {
        Err(self.0)
    }
    fn delete(&self, _p: ProviderId) -> Result<(), KeyError> {
        Err(self.0)
    }
}

// ---------- the local HTTP server ----------
#[derive(Debug, Clone)]
pub struct Req {
    pub method: String,
    /// Request target as sent: path plus query.
    pub target: String,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}
impl Req {
    pub fn path(&self) -> &str {
        self.target.split('?').next().unwrap_or("")
    }
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers.iter().find(|(k, _)| k.eq_ignore_ascii_case(name)).map(|(_, v)| v.as_str())
    }
    pub fn header_names(&self) -> Vec<String> {
        self.headers.iter().map(|(k, _)| k.to_lowercase()).collect()
    }
    pub fn body_text(&self) -> String {
        String::from_utf8_lossy(&self.body).to_string()
    }
    pub fn json(&self) -> serde_json::Value {
        serde_json::from_slice(&self.body).unwrap_or_else(|e| panic!("request body is not JSON ({e}): {}", self.body_text()))
    }
}

pub type Handler = Arc<dyn Fn(&Req, &mut TcpStream) + Send + Sync>;

pub struct TestServer {
    pub addr: SocketAddr,
    reqs: Arc<Mutex<Vec<Req>>>,
    stop: Arc<AtomicBool>,
}

fn read_request(s: &mut TcpStream) -> Option<Req> {
    s.set_read_timeout(Some(Duration::from_secs(5))).ok()?;
    let mut buf: Vec<u8> = Vec::new();
    let mut chunk = [0u8; 4096];
    let head_end = loop {
        if let Some(i) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
            break i;
        }
        let n = s.read(&mut chunk).ok()?;
        if n == 0 {
            return None;
        }
        buf.extend_from_slice(&chunk[..n]);
    };
    let head = String::from_utf8_lossy(&buf[..head_end]).to_string();
    let mut lines = head.lines();
    let first = lines.next()?;
    let mut parts = first.split(' ');
    let method = parts.next()?.to_string();
    let target = parts.next()?.to_string();
    let headers: Vec<(String, String)> =
        lines.filter_map(|l| l.split_once(':').map(|(k, v)| (k.trim().to_string(), v.trim().to_string()))).collect();
    let want: usize = headers
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case("content-length"))
        .and_then(|(_, v)| v.parse().ok())
        .unwrap_or(0);
    let mut body = buf[head_end + 4..].to_vec();
    while body.len() < want {
        let n = s.read(&mut chunk).ok()?;
        if n == 0 {
            break;
        }
        body.extend_from_slice(&chunk[..n]);
    }
    Some(Req { method, target, headers, body })
}

impl TestServer {
    pub fn start(handler: Handler) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind a local test port");
        let addr = listener.local_addr().unwrap();
        let reqs: Arc<Mutex<Vec<Req>>> = Arc::new(Mutex::new(vec![]));
        let stop = Arc::new(AtomicBool::new(false));
        let (r2, s2) = (reqs.clone(), stop.clone());
        thread::spawn(move || {
            for conn in listener.incoming() {
                if s2.load(Ordering::SeqCst) {
                    break;
                }
                let Ok(mut stream) = conn else { continue };
                let (h, r) = (handler.clone(), r2.clone());
                thread::spawn(move || {
                    if let Some(req) = read_request(&mut stream) {
                        r.lock().unwrap().push(req.clone());
                        h(&req, &mut stream);
                    }
                });
            }
        });
        TestServer { addr, reqs, stop }
    }
    /// Always the same answer.
    pub fn fixed(status: u16, headers: Vec<(String, String)>, body: String) -> Self {
        Self::start(Arc::new(move |_req, s| {
            let hs: Vec<(&str, &str)> = headers.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
            respond(s, status, &hs, body.as_bytes());
        }))
    }
    pub fn json(status: u16, body: &str) -> Self {
        Self::fixed(status, vec![], body.to_string())
    }
    pub fn base(&self) -> String {
        format!("http://{}", self.addr)
    }
    pub fn requests(&self) -> Vec<Req> {
        self.reqs.lock().unwrap().clone()
    }
}
impl Drop for TestServer {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        let _ = TcpStream::connect(self.addr); // wake the accept loop
    }
}

fn reason(status: u16) -> &'static str {
    match status {
        200 => "OK",
        302 => "Found",
        400 => "Bad Request",
        401 => "Unauthorized",
        402 => "Payment Required",
        403 => "Forbidden",
        404 => "Not Found",
        429 => "Too Many Requests",
        500 => "Internal Server Error",
        503 => "Service Unavailable",
        _ => "Status",
    }
}

/// A complete answer with a Content-Length.
pub fn respond(s: &mut TcpStream, status: u16, headers: &[(&str, &str)], body: &[u8]) {
    let mut head = format!("HTTP/1.1 {status} {}\r\n", reason(status));
    if !headers.iter().any(|(k, _)| k.eq_ignore_ascii_case("content-type")) {
        head.push_str("Content-Type: application/json\r\n");
    }
    for (k, v) in headers {
        head.push_str(&format!("{k}: {v}\r\n"));
    }
    head.push_str(&format!("Content-Length: {}\r\nConnection: close\r\n\r\n", body.len()));
    let _ = s.write_all(head.as_bytes());
    let _ = s.write_all(body);
    let _ = s.flush();
}

/// Headers of a chunked answer (no Content-Length).
pub fn chunked_head(s: &mut TcpStream, extra: &[(&str, &str)]) -> bool {
    let mut head = String::from("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n");
    for (k, v) in extra {
        head.push_str(&format!("{k}: {v}\r\n"));
    }
    head.push_str("\r\n");
    s.write_all(head.as_bytes()).is_ok()
}
/// One chunk; false once the peer has gone.
pub fn chunk(s: &mut TcpStream, data: &[u8]) -> bool {
    s.write_all(format!("{:x}\r\n", data.len()).as_bytes()).is_ok() && s.write_all(data).is_ok() && s.write_all(b"\r\n").is_ok() && s.flush().is_ok()
}

/// A port nobody listens on.
pub fn closed_port_base() -> String {
    let l = TcpListener::bind("127.0.0.1:0").unwrap();
    let a = l.local_addr().unwrap();
    drop(l);
    format!("http://{a}")
}

// ---------- gzip of N copies of 'a' (a tiny stream that decodes to a lot) ----------
struct Bits {
    out: Vec<u8>,
    acc: u32,
    n: u32,
}
impl Bits {
    fn new() -> Self {
        Bits { out: vec![], acc: 0, n: 0 }
    }
    /// `count` bits of `v`, least significant bit first (deflate packing for header and extra bits).
    fn lsb(&mut self, v: u32, count: u32) {
        for i in 0..count {
            self.bit((v >> i) & 1);
        }
    }
    /// A Huffman code: most significant bit first.
    fn msb(&mut self, v: u32, count: u32) {
        for i in (0..count).rev() {
            self.bit((v >> i) & 1);
        }
    }
    fn bit(&mut self, b: u32) {
        self.acc |= b << self.n;
        self.n += 1;
        if self.n == 8 {
            self.out.push(self.acc as u8);
            self.acc = 0;
            self.n = 0;
        }
    }
    fn finish(mut self) -> Vec<u8> {
        if self.n > 0 {
            self.out.push(self.acc as u8);
        }
        self.out
    }
}
fn crc32(data: &[u8]) -> u32 {
    let mut table = [0u32; 256];
    for (i, t) in table.iter_mut().enumerate() {
        let mut c = i as u32;
        for _ in 0..8 {
            c = if c & 1 == 1 { 0xEDB8_8320 ^ (c >> 1) } else { c >> 1 };
        }
        *t = c;
    }
    let mut c = 0xFFFF_FFFFu32;
    for b in data {
        c = table[((c ^ *b as u32) & 0xFF) as usize] ^ (c >> 8);
    }
    !c
}
/// A valid gzip stream that decodes to `1 + 258 * k` bytes of 'a' (at least `decoded` bytes). About 13 bits
/// per 258 bytes: far under the 64 KB cap while the decoded size is far over it.
pub fn gzip_of_a(decoded: usize) -> (Vec<u8>, usize) {
    let k = decoded.div_ceil(258);
    let total = 1 + 258 * k;
    let mut b = Bits::new();
    b.lsb(1, 1); // last block
    b.lsb(1, 2); // fixed Huffman codes
    b.msb(0x30 + b'a' as u32, 8); // literal 'a'
    for _ in 0..k {
        b.msb(0xC0 + (285 - 280), 8); // length 258
        b.msb(0, 5); // distance 1
    }
    b.msb(0, 7); // end of block
    let body = b.finish();
    let mut gz = vec![0x1f, 0x8b, 8, 0, 0, 0, 0, 0, 0, 0xff];
    gz.extend_from_slice(&body);
    gz.extend_from_slice(&crc32(&vec![b'a'; total]).to_le_bytes());
    gz.extend_from_slice(&(total as u32).to_le_bytes());
    (gz, total)
}

// ---------- scripted provider ----------
#[derive(Debug, Clone)]
pub struct Call {
    pub model: String,
    pub key: String,
    pub system: String,
    pub user: String,
}

/// Lets a test hold calls inside the provider and release them later.
#[derive(Default)]
pub struct Gate {
    open: Mutex<bool>,
    cv: Condvar,
    pub entered: AtomicUsize,
}
impl Gate {
    pub fn wait_open(&self) {
        self.entered.fetch_add(1, Ordering::SeqCst);
        let mut o = self.open.lock().unwrap();
        while !*o {
            o = self.cv.wait(o).unwrap();
        }
    }
    pub fn release(&self) {
        *self.open.lock().unwrap() = true;
        self.cv.notify_all();
    }
}

pub struct Scripted {
    id: ProviderId,
    queue: Mutex<VecDeque<Result<String, ProviderError>>>,
    fallback: Mutex<Result<String, ProviderError>>,
    pub calls: Mutex<Vec<Call>>,
    pub gate: Mutex<Option<Arc<Gate>>>,
    pub panic_with: Mutex<Option<String>>,
    pub checks: Mutex<Vec<String>>,
}
impl Scripted {
    pub fn new(id: ProviderId, fallback: Result<String, ProviderError>) -> Arc<Self> {
        Arc::new(Scripted {
            id,
            queue: Mutex::new(VecDeque::new()),
            fallback: Mutex::new(fallback),
            calls: Mutex::new(vec![]),
            gate: Mutex::new(None),
            panic_with: Mutex::new(None),
            checks: Mutex::new(vec![]),
        })
    }
    /// The next calls answer in order; after that the fallback answers.
    pub fn then(&self, r: Result<String, ProviderError>) {
        self.queue.lock().unwrap().push_back(r);
    }
    pub fn set_fallback(&self, r: Result<String, ProviderError>) {
        *self.fallback.lock().unwrap() = r;
    }
    pub fn count(&self) -> usize {
        self.calls.lock().unwrap().len()
    }
    pub fn last(&self) -> Call {
        self.calls.lock().unwrap().last().cloned().expect("the provider was never called")
    }
}
impl Provider for Scripted {
    fn id(&self) -> ProviderId {
        self.id
    }
    fn ask(&self, req: &BrainRequest, model: &str, key: &ApiKey) -> Result<String, ProviderError> {
        self.calls.lock().unwrap().push(Call {
            model: model.to_string(),
            key: key.expose().to_string(),
            system: req.system.to_string(),
            user: req.user.clone(),
        });
        let gate = self.gate.lock().unwrap().clone();
        if let Some(g) = gate {
            g.wait_open();
        }
        let boom = self.panic_with.lock().unwrap().clone(); // a local: the guard must not live through the panic
        if let Some(msg) = boom {
            panic!("{msg}");
        }
        let next = self.queue.lock().unwrap().pop_front();
        next.unwrap_or_else(|| self.fallback.lock().unwrap().clone())
    }
    fn check_key(&self, key: &ApiKey) -> Result<(), ProviderError> {
        self.checks.lock().unwrap().push(key.expose().to_string());
        Ok(())
    }
}

// ---------- the brain rig ----------
pub const PLAN_OK: &str = r#"{"v":1,"kind":"plan","steps":[{"action":"youtube_play","args":{"query":"Blinding Lights"}},{"action":"media","args":{"key":"next"}}]}"#;
pub const QUESTION_OK: &str = r#"{"v":1,"kind":"question","question":"Which song called Hello: Adele or Lionel Richie?"}"#;
pub const GEMINI_KEY: &str = "AIzaSyGeminiKeyAAAAAAAAAAAAAAAAAAAA";
pub const CLAUDE_KEY: &str = "sk-ant-api03-ClaudeKeyBBBBBBBBBBBBBBBBB";

pub struct BR {
    pub brain: Arc<Brain>,
    pub claude: Arc<Scripted>,
    pub gemini: Arc<Scripted>,
    pub keys: Arc<MemoryKeyStore>,
    pub sleeps: Arc<Mutex<Vec<Duration>>>,
    pub clock: Arc<FakeClock>,
    pub choice: Arc<Mutex<(ProviderId, String)>>,
}
impl BR {
    /// Gemini chosen with its default model; both keys stored; both providers answer `PLAN_OK`.
    pub fn new() -> Self {
        Self::build(true, Limits { max_in_flight: 2, max_per_hour: 120 })
    }
    pub fn with_limits(l: Limits) -> Self {
        Self::build(true, l)
    }
    pub fn no_keys() -> Self {
        Self::build(false, Limits { max_in_flight: 2, max_per_hour: 120 })
    }
    fn build(with_keys: bool, limits: Limits) -> Self {
        let claude = Scripted::new(ProviderId::Claude, Ok(PLAN_OK.to_string()));
        let gemini = Scripted::new(ProviderId::Gemini, Ok(PLAN_OK.to_string()));
        let keys = Arc::new(MemoryKeyStore::new());
        if with_keys {
            keys.set(ProviderId::Gemini, &key(GEMINI_KEY)).expect("store the gemini key");
            keys.set(ProviderId::Claude, &key(CLAUDE_KEY)).expect("store the claude key");
        }
        let sleeps: Arc<Mutex<Vec<Duration>>> = Arc::new(Mutex::new(vec![]));
        let s2 = sleeps.clone();
        let clock = FakeClock::new();
        let choice = Arc::new(Mutex::new((ProviderId::Gemini, "gemini-3.5-flash-lite".to_string())));
        let c2 = choice.clone();
        let brain = Brain::new(
            vec![claude.clone() as Arc<dyn Provider>, gemini.clone() as Arc<dyn Provider>],
            keys.clone(),
            Arc::new(move || {
                let c = c2.lock().unwrap();
                BrainChoice { provider: c.0, model: c.1.clone() }
            }),
            clock.clone(),
            Arc::new(move |d| s2.lock().unwrap().push(d)),
            limits,
        );
        BR { brain: Arc::new(brain), claude, gemini, keys, sleeps, clock, choice }
    }
    pub fn think(&self, text: &str) -> Reply {
        self.brain.think(text, self.clock.at(0))
    }
    pub fn choose(&self, p: ProviderId, model: &str) {
        *self.choice.lock().unwrap() = (p, model.to_string());
    }
    pub fn sleeps(&self) -> Vec<Duration> {
        self.sleeps.lock().unwrap().clone()
    }
    /// Provider fakes for the chosen one (Gemini by default).
    pub fn chosen(&self) -> Arc<Scripted> {
        match self.choice.lock().unwrap().0 {
            ProviderId::Claude => self.claude.clone(),
            ProviderId::Gemini => self.gemini.clone(),
        }
    }
}

pub fn texts(r: &Reply) -> Vec<String> {
    r.lines.iter().map(|l| l.text.clone()).collect()
}

// ---------- metrics lines ----------
/// The integer `name=` of a `brain outcome=...` record, if the record is one.
pub fn field(record: &str, name: &str) -> Option<u64> {
    if !record.contains("brain outcome=") {
        return None;
    }
    let pat = format!("{name}=");
    let i = record.find(&pat)? + pat.len();
    let digits: String = record[i..].chars().take_while(|c| c.is_ascii_digit()).collect();
    digits.parse().ok()
}
/// The digits after `actions=` (separators and brackets ignored), e.g. `[5, 2]` gives `[5, 2]`.
pub fn actions_of(record: &str) -> Vec<u8> {
    let Some(i) = record.find("actions=") else { return vec![] };
    let rest = &record[i + "actions=".len()..];
    let rest = rest.lines().next().unwrap_or("");
    rest.chars().filter(|c| c.is_ascii_digit()).map(|c| c as u8 - b'0').collect()
}
/// Outcome numbers of every brain metrics record, in order.
pub fn outcomes(records: &[String]) -> Vec<u64> {
    records.iter().filter_map(|r| field(r, "outcome")).collect()
}
pub fn brain_records(records: &[String]) -> Vec<String> {
    records.iter().filter(|r| r.contains("brain outcome=")).cloned().collect()
}

// ---------- the seam rig ----------
/// A `SlowRouter` fake: records calls, optional delay, fixed reply.
pub struct FakeBrain {
    pub min_words: Mutex<usize>,
    pub reply: Mutex<Reply>,
    pub delay: Mutex<Duration>,
    pub calls: Mutex<Vec<(String, Instant, String)>>,
    pub wants_calls: AtomicUsize,
}
impl FakeBrain {
    pub fn new(reply: Reply) -> Arc<Self> {
        Arc::new(FakeBrain {
            min_words: Mutex::new(2),
            reply: Mutex::new(reply),
            delay: Mutex::new(Duration::ZERO),
            calls: Mutex::new(vec![]),
            wants_calls: AtomicUsize::new(0),
        })
    }
    pub fn texts(&self) -> Vec<String> {
        self.calls.lock().unwrap().iter().map(|c| c.0.clone()).collect()
    }
}
impl SlowRouter for FakeBrain {
    fn wants(&self, text: &str) -> bool {
        self.wants_calls.fetch_add(1, Ordering::SeqCst);
        text.split_whitespace().count() >= *self.min_words.lock().unwrap()
    }
    fn think(&self, text: &str, stopped_at: Instant) -> Reply {
        self.calls.lock().unwrap().push((text.to_string(), stopped_at, thread::current().name().unwrap_or("").to_string()));
        thread::sleep(*self.delay.lock().unwrap());
        self.reply.lock().unwrap().clone()
    }
}

/// A sink built with `with_brain` and the same fakes the step-3 rig uses.
pub fn brain_rig(clock: Arc<FakeClock>, log: Arc<Log>, router: Arc<dyn TranscriptRouter>, brain: Arc<dyn SlowRouter>, text: &str) -> Rig {
    let rec = Arc::new(FakeRec { samples: Mutex::new(Some(vec![0.1; SAMPLES])) });
    let tr = Arc::new(FakeTr { log: log.clone(), result: Mutex::new(Ok(text.into())), delay: Mutex::new(Duration::ZERO) });
    let panel = Arc::new(FakePanel::default());
    let sink = SpeechSink::with_brain(
        rec.clone(),
        tr.clone(),
        panel.clone(),
        clock.clone(),
        Arc::new(FakeTap),
        Arc::new(FakeStopper),
        Box::new(|| HotkeyChoice::Fn),
        router,
        brain,
    );
    Rig { log, rec, tr, panel, clock, sink: Arc::new(sink) }
}

pub fn reply(lines: Vec<Line>, hold_secs: u64) -> Reply {
    Reply { lines, hold_secs }
}
