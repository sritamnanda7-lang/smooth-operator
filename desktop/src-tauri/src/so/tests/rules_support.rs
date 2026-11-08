// SPDX-License-Identifier: FSL-1.1-MIT
//! Shared fakes for the step-3 tests (`rules_*`). Fakes only: no real keys, apps, microphone or panel.
//! Numbers follow draft/step-3-plan.md.
//!
//! LOGGING TESTS RUN IN A CHILD PROCESS. `log::set_logger` works once per process and step 2's T18
//! already installs a logger in the shared lib test binary. So every test that reads log output has two
//! parts: a normal `#[test]` that calls `run_isolated(module_path!(), "<name>_inner")`, and an
//! `#[ignore]` inner test that does the real work with its own logger. The child sets `SO_ISOLATED=1`;
//! without it an inner test returns at once, so `cargo test -- --ignored` stays harmless.
use crate::so::hotkey::session::Mode::PushToTalk;
use crate::so::hotkey::session::TalkEvent::{self, Armed, Started, Stopped};
use crate::so::hotkey::watcher::{Clock, TalkSink};
use crate::so::panel::{Kind, Line, PanelOut, PanelPayload};
use crate::so::rules::apps::{AppCatalog, AppEntry};
use crate::so::rules::hands::{AppLauncher, MediaKeys};
use crate::so::rules::phrase::MediaKey;
use crate::so::rules::{Route, TranscriptRouter};
use crate::so::settings::HotkeyChoice;
use crate::so::stt::sink::{ModelState, Recorder, SpeechSink, SpeechTap, Stopper, Transcriber};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

pub const SAMPLES: usize = 1600;

// ---------- waiting ----------
pub fn wait_until(what: &str, max: Duration, f: impl Fn() -> bool) {
    let start = Instant::now();
    while start.elapsed() < max {
        if f() {
            return;
        }
        thread::sleep(Duration::from_millis(5));
    }
    panic!("timed out waiting for: {what}");
}
pub fn wait(what: &str, f: impl Fn() -> bool) {
    wait_until(what, Duration::from_secs(3), f)
}
pub fn settle() {
    thread::sleep(Duration::from_millis(150));
}

// ---------- event log (order of calls) ----------
#[derive(Default)]
pub struct Log(Mutex<Vec<String>>);
impl Log {
    pub fn add(&self, s: &str) {
        self.0.lock().unwrap().push(s.to_string());
    }
    pub fn all(&self) -> Vec<String> {
        self.0.lock().unwrap().clone()
    }
    pub fn count(&self, s: &str) -> usize {
        self.all().iter().filter(|x| *x == s).count()
    }
}

// ---------- sink fakes ----------
pub struct FakeRec {
    pub samples: Mutex<Option<Vec<f32>>>,
}
impl Recorder for FakeRec {
    fn start(&self) -> Result<(), String> {
        Ok(())
    }
    fn stop(&self) -> Option<Vec<f32>> {
        self.samples.lock().unwrap().clone()
    }
    fn cancel(&self) {}
}

pub struct FakeTr {
    pub log: Arc<Log>,
    pub result: Mutex<Result<String, String>>,
    pub delay: Mutex<Duration>,
}
impl Transcriber for FakeTr {
    fn downloaded(&self) -> ModelState {
        ModelState::Ready
    }
    fn preload(&self) {}
    fn transcribe(&self, _samples: Vec<f32>) -> Result<String, String> {
        self.log.add("tr.transcribe");
        thread::sleep(*self.delay.lock().unwrap());
        self.result.lock().unwrap().clone()
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Op {
    Show(PanelPayload),
    Hide,
}
#[derive(Default)]
pub struct FakePanel {
    ops: Mutex<Vec<Op>>,
}
impl PanelOut for FakePanel {
    fn show(&self, p: PanelPayload) {
        self.ops.lock().unwrap().push(Op::Show(p));
    }
    fn hide(&self) {
        self.ops.lock().unwrap().push(Op::Hide);
    }
}
impl FakePanel {
    pub fn ops(&self) -> Vec<Op> {
        self.ops.lock().unwrap().clone()
    }
    pub fn shows(&self) -> Vec<PanelPayload> {
        self.ops()
            .into_iter()
            .filter_map(|o| match o {
                Op::Show(p) => Some(p),
                Op::Hide => None,
            })
            .collect()
    }
    /// First line of every show, in order.
    pub fn texts(&self) -> Vec<String> {
        self.shows().iter().filter_map(|p| p.lines.first().map(|l| l.text.clone())).collect()
    }
    /// Every line of every show.
    pub fn all_line_texts(&self) -> Vec<String> {
        self.shows().iter().flat_map(|p| p.lines.iter().map(|l| l.text.clone())).collect()
    }
    pub fn last_payload(&self) -> Option<PanelPayload> {
        self.shows().pop()
    }
    pub fn hides(&self) -> usize {
        self.ops().iter().filter(|o| **o == Op::Hide).count()
    }
}

pub struct FakeClock {
    pub base: Instant,
    ms: Mutex<u64>,
}
impl FakeClock {
    pub fn new() -> Arc<Self> {
        Arc::new(FakeClock { base: Instant::now(), ms: Mutex::new(0) })
    }
    pub fn set(&self, ms: u64) {
        *self.ms.lock().unwrap() = ms;
    }
    pub fn at(&self, ms: u64) -> Instant {
        self.base + Duration::from_millis(ms)
    }
}
impl Clock for FakeClock {
    fn now(&self) -> Instant {
        self.at(*self.ms.lock().unwrap())
    }
}

pub struct FakeTap;
impl SpeechTap for FakeTap {
    fn last_speech(&self) -> Option<Instant> {
        None
    }
}
#[derive(Default)]
pub struct FakeStopper;
impl Stopper for FakeStopper {
    fn force_stop(&self, _seq: u64) {}
}

// ---------- router fake ----------
/// Records every call; optionally blocks; returns a fixed `Route`.
pub struct FakeRouter {
    pub log: Arc<Log>,
    pub calls: Mutex<Vec<(String, Instant)>>,
    pub delay: Mutex<Duration>,
    pub reply: Mutex<Route>,
}
impl FakeRouter {
    pub fn new(log: Arc<Log>, reply: Route) -> Arc<Self> {
        Arc::new(FakeRouter { log, calls: Mutex::new(vec![]), delay: Mutex::new(Duration::ZERO), reply: Mutex::new(reply) })
    }
    pub fn texts(&self) -> Vec<String> {
        self.calls.lock().unwrap().iter().map(|c| c.0.clone()).collect()
    }
}
impl TranscriptRouter for FakeRouter {
    fn route(&self, text: &str, stopped_at: Instant) -> Route {
        self.log.add("router.route");
        self.calls.lock().unwrap().push((text.to_string(), stopped_at));
        thread::sleep(*self.delay.lock().unwrap());
        self.reply.lock().unwrap().clone()
    }
}

// ---------- effect fakes (hands) ----------
#[derive(Default)]
pub struct FakeMedia {
    pub presses: Mutex<Vec<MediaKey>>,
    pub err: Mutex<Option<String>>,
}
impl MediaKeys for FakeMedia {
    fn press(&self, key: MediaKey) -> Result<(), String> {
        self.presses.lock().unwrap().push(key);
        match self.err.lock().unwrap().clone() {
            Some(e) => Err(e),
            None => Ok(()),
        }
    }
}
impl FakeMedia {
    pub fn presses(&self) -> Vec<MediaKey> {
        self.presses.lock().unwrap().clone()
    }
}

#[derive(Default)]
pub struct FakeLauncher {
    pub opens: Mutex<Vec<PathBuf>>,
    pub err: Mutex<Option<String>>,
}
impl AppLauncher for FakeLauncher {
    fn open(&self, app: &Path) -> Result<(), String> {
        self.opens.lock().unwrap().push(app.to_path_buf());
        match self.err.lock().unwrap().clone() {
            Some(e) => Err(e),
            None => Ok(()),
        }
    }
}
impl FakeLauncher {
    pub fn opens(&self) -> Vec<PathBuf> {
        self.opens.lock().unwrap().clone()
    }
}

pub struct FakeCatalog {
    pub apps: Vec<AppEntry>,
    pub lists: AtomicUsize,
}
impl FakeCatalog {
    pub fn new(apps: Vec<AppEntry>) -> Arc<Self> {
        Arc::new(FakeCatalog { apps, lists: AtomicUsize::new(0) })
    }
    pub fn lists(&self) -> usize {
        self.lists.load(Ordering::SeqCst)
    }
}
impl AppCatalog for FakeCatalog {
    fn list(&self) -> Vec<AppEntry> {
        self.lists.fetch_add(1, Ordering::SeqCst);
        self.apps.clone()
    }
}

/// An app as the catalog would list it: `/Applications/<name>.app`.
pub fn app(name: &str, running: bool) -> AppEntry {
    AppEntry { name: name.to_string(), path: PathBuf::from(format!("/Applications/{name}.app")), running }
}

pub fn info(text: &str) -> Line {
    Line::new(text, Kind::Info)
}
pub fn warn(text: &str) -> Line {
    Line::new(text, Kind::Warn)
}

// ---------- the rig: a sink with fakes ----------
pub struct Rig {
    pub log: Arc<Log>,
    pub rec: Arc<FakeRec>,
    pub tr: Arc<FakeTr>,
    pub panel: Arc<FakePanel>,
    pub clock: Arc<FakeClock>,
    pub sink: Arc<SpeechSink>,
}
impl Rig {
    /// A sink with the given router (built with `with_router`).
    pub fn with_router(clock: Arc<FakeClock>, log: Arc<Log>, router: Arc<dyn TranscriptRouter>, text: &str) -> Self {
        let (rec, tr, panel) = Self::fakes(&log, text);
        let sink = SpeechSink::with_router(
            rec.clone(),
            tr.clone(),
            panel.clone(),
            clock.clone(),
            Arc::new(FakeTap),
            Arc::new(FakeStopper),
            Box::new(|| HotkeyChoice::Fn),
            router,
        );
        Rig { log, rec, tr, panel, clock, sink: Arc::new(sink) }
    }
    /// A sink built with `SpeechSink::new` (the default router).
    pub fn plain(text: &str) -> Self {
        let log = Arc::new(Log::default());
        let clock = FakeClock::new();
        let (rec, tr, panel) = Self::fakes(&log, text);
        let sink = SpeechSink::new(
            rec.clone(),
            tr.clone(),
            panel.clone(),
            clock.clone(),
            Arc::new(FakeTap),
            Arc::new(FakeStopper),
            Box::new(|| HotkeyChoice::Fn),
        );
        Rig { log, rec, tr, panel, clock, sink: Arc::new(sink) }
    }
    fn fakes(log: &Arc<Log>, text: &str) -> (Arc<FakeRec>, Arc<FakeTr>, Arc<FakePanel>) {
        (
            Arc::new(FakeRec { samples: Mutex::new(Some(vec![0.1; SAMPLES])) }),
            Arc::new(FakeTr { log: log.clone(), result: Mutex::new(Ok(text.into())), delay: Mutex::new(Duration::ZERO) }),
            Arc::new(FakePanel::default()),
        )
    }
    pub fn ev(&self, e: TalkEvent) {
        self.sink.on_event(e);
    }
    /// Armed, Started(PushToTalk), Stopped(PushToTalk).
    pub fn ptt(&self) {
        self.ev(Armed);
        self.ev(Started(PushToTalk));
        self.ev(Stopped(PushToTalk));
    }
    pub fn say(&self, text: &str) {
        *self.tr.result.lock().unwrap() = Ok(text.into());
        self.ptt();
    }
    /// Waits until some panel payload has a line with exactly this text.
    pub fn wait_line(&self, text: &str) {
        let t = text.to_string();
        wait(&format!("panel line {text:?}"), || self.panel.all_line_texts().contains(&t));
    }
}

// ---------- isolated (child-process) log capture ----------
pub fn in_child() -> bool {
    std::env::var("SO_ISOLATED").is_ok_and(|v| v == "1")
}

struct Capture;
static RECORDS: Mutex<Vec<String>> = Mutex::new(Vec::new());
static CAPTURE: Capture = Capture;
impl log::Log for Capture {
    fn enabled(&self, _: &log::Metadata) -> bool {
        true
    }
    fn log(&self, r: &log::Record) {
        RECORDS.lock().unwrap().push(format!("{} {}", r.target(), r.args()));
    }
    fn flush(&self) {}
}
/// Child process only: this is the one logger of the process.
pub fn install_capture() {
    log::set_logger(&CAPTURE).expect("another logger is installed: the inner test must run in its own process");
    log::set_max_level(log::LevelFilter::Trace);
}
pub fn captured() -> Vec<String> {
    RECORDS.lock().unwrap().clone()
}

/// Runs the `#[ignore]` test `name` of `module` alone in a child process (this same test binary) with
/// `SO_ISOLATED=1` and fails if it did not pass. `module` is `module_path!()` of the caller.
pub fn run_isolated(module: &str, name: &str) {
    let rel = module.split_once("::").map(|x| x.1).unwrap_or(module);
    let full = format!("{rel}::{name}");
    let exe = std::env::current_exe().expect("test binary path");
    let out = std::process::Command::new(exe)
        .args([full.as_str(), "--exact", "--ignored", "--test-threads=1", "--nocapture"])
        .env("SO_ISOLATED", "1")
        .output()
        .expect("spawn the isolated test");
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    let tail = |s: &str| s.lines().rev().take(25).collect::<Vec<_>>().into_iter().rev().collect::<Vec<_>>().join("\n");
    assert!(
        out.status.success() && stdout.contains("1 passed"),
        "isolated test {full} failed (status {:?}).\n--- stdout ---\n{}\n--- stderr ---\n{}",
        out.status.code(),
        tail(&stdout),
        tail(&stderr)
    );
}
