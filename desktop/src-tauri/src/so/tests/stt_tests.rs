// SPDX-License-Identifier: FSL-1.1-MIT
//! Step 2 tests T1-T20, T27 (and tap, T19). Fakes only: recorder, transcriber, clock, tap, stopper, panel.
//! No audio, no model, no OS calls. Real sleeps are only used to let threads run (and the 4 s hide timer).
//! Numbers follow draft/step-2-plan.md.
use crate::so::hotkey::detector::{DetOut, Detector};
use crate::so::hotkey::session::Mode::{FreeTalk, PushToTalk};
use crate::so::hotkey::session::{Session, TalkEvent};
use crate::so::hotkey::session::TalkEvent::{Armed, Cancelled, Disarmed, Started, Stopped};
use crate::so::hotkey::watcher::{run, Clock, Msg, TalkSink};
use crate::so::hotkey::Engine;
use crate::so::panel::{self, Kind, Line, PanelOut, PanelPayload};
use crate::so::settings::HotkeyChoice;
use crate::so::stt::silence::{End, SilenceWatch};
use crate::so::stt::sink::{
    heard_line, ModelState, Recorder, SpeechSink, SpeechTap, Stopper, Transcriber, DIDNT_CATCH_TEXT,
    MIC_FAILED_TEXT, MODEL_DOWNLOADING_TEXT, MODEL_MISSING_TEXT, SPEECH_FAILED_TEXT, TRANSCRIBING_TEXT,
};
use crate::so::stt::tap::{self, StaticTap};
use crate::so::stt::{MicOpenAt, BINDING, HEARD_SECS, MAX_TALK, MIC_OPEN_AT, NO_SPEECH, SILENCE_END};
use crate::audio_toolkit::vad::VAD_OFFLINE_HANGOVER_MS;
use handy_keys::{Key, KeyEvent, Modifiers};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Sender};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

const FN: Modifiers = Modifiers::FN;
const NONE: Modifiers = Modifiers::empty();

// =============================================================================================
// Part A: detector, session and engine (T20)
// =============================================================================================
fn modev(flags: Modifiers, down: bool, changed: Modifiers) -> KeyEvent {
    KeyEvent { modifiers: flags, key: None, is_key_down: down, changed_modifier: Some(changed) }
}
fn keyev(flags: Modifiers, key: Key, down: bool) -> KeyEvent {
    KeyEvent { modifiers: flags, key: Some(key), is_key_down: down, changed_modifier: None }
}

/// The pure engine, with a base instant so tests read in milliseconds. Nothing is filtered here.
struct E {
    e: Engine,
    t0: Instant,
}
impl E {
    fn new() -> Self {
        E { e: Engine::new(HotkeyChoice::Fn), t0: Instant::now() }
    }
    fn at(&self, ms: u64) -> Instant {
        self.t0 + Duration::from_millis(ms)
    }
    fn down(&mut self, ms: u64) -> Vec<TalkEvent> {
        let t = self.at(ms);
        self.e.feed(&modev(FN, true, FN), t)
    }
    fn up(&mut self, ms: u64) -> Vec<TalkEvent> {
        let t = self.at(ms);
        self.e.feed(&modev(NONE, false, FN), t)
    }
    fn key_held(&mut self, key: Key, ms: u64) -> Vec<TalkEvent> {
        let t = self.at(ms);
        self.e.feed(&keyev(FN, key, true), t)
    }
    fn tick(&mut self, ms: u64) -> Vec<TalkEvent> {
        let t = self.at(ms);
        self.e.tick(t)
    }
    /// Free-talk started by tap + 2nd press at +250; 2nd press released at +300.
    fn free_running(&mut self, ms: u64) {
        assert_eq!(self.down(ms), vec![Armed]);
        assert!(self.up(ms + 80).is_empty());
        assert_eq!(self.down(ms + 250), vec![Started(FreeTalk)]);
        assert!(self.up(ms + 300).is_empty());
    }
}

#[test]
fn t20_detector_tap_expired_after_the_400ms_window() {
    let t0 = Instant::now();
    let ms = |n: u64| t0 + Duration::from_millis(n);
    let mut d = Detector::new(FN);
    assert_eq!(d.feed(&modev(FN, true, FN), ms(0)), vec![DetOut::Down]);
    assert_eq!(d.feed(&modev(NONE, false, FN), ms(80)), vec![DetOut::Release { clean: true }]);
    assert!(d.needs_tick(), "a lone tap needs a tick so its window can expire");
    assert!(d.tick(ms(200)).is_empty());
    assert!(d.tick(ms(480)).is_empty(), "exactly 400 ms after the release is still a valid 2nd tap");
    assert_eq!(d.tick(ms(481)), vec![DetOut::TapExpired]);
    assert!(d.tick(ms(900)).is_empty(), "only once");
    assert!(!d.needs_tick());
}

#[test]
fn t20_detector_no_tap_expired_for_double_tap_chord_or_hold() {
    let t0 = Instant::now();
    let ms = |n: u64| t0 + Duration::from_millis(n);
    // Double-tap: the 2nd press uses the tap up.
    let mut d = Detector::new(FN);
    d.feed(&modev(FN, true, FN), ms(0));
    d.feed(&modev(NONE, false, FN), ms(80));
    assert_eq!(d.feed(&modev(FN, true, FN), ms(250)), vec![DetOut::Down, DetOut::DoubleTap]);
    assert!(d.tick(ms(1000)).is_empty());
    // Another key between the taps breaks the double-tap: Dirty, and no later expiry.
    let mut d = Detector::new(FN);
    d.feed(&modev(FN, true, FN), ms(0));
    d.feed(&modev(NONE, false, FN), ms(80));
    assert_eq!(d.feed(&keyev(NONE, Key::C, true), ms(150)), vec![DetOut::Dirty]);
    assert!(d.tick(ms(1000)).is_empty());
    assert!(!d.needs_tick());
    // A hold is not a tap.
    let mut d = Detector::new(FN);
    d.feed(&modev(FN, true, FN), ms(0));
    assert_eq!(d.tick(ms(310)), vec![DetOut::Hold]);
    d.feed(&modev(NONE, false, FN), ms(500));
    assert!(d.tick(ms(2000)).is_empty());
    assert!(!d.needs_tick());
}

#[test]
fn t20_session_armed_and_disarmed_table() {
    use DetOut::*;
    // Hold: Armed then Started, no Disarmed.
    let mut s = Session::new();
    assert_eq!(s.on(Down), Some(Armed));
    assert_eq!(s.on(Hold), Some(Started(PushToTalk)));
    assert_eq!(s.on(Release { clean: true }), Some(Stopped(PushToTalk)));
    // Lone tap: Disarmed only when the window expires.
    let mut s = Session::new();
    assert_eq!(s.on(Down), Some(Armed));
    assert_eq!(s.on(Release { clean: true }), None);
    assert_eq!(s.on(TapExpired), Some(Disarmed));
    assert_eq!(s.on(TapExpired), None);
    // Chord before the hold.
    let mut s = Session::new();
    assert_eq!(s.on(Down), Some(Armed));
    assert_eq!(s.on(Dirty), Some(Disarmed));
    assert_eq!(s.on(Release { clean: false }), None);
    // Tap, then a 2nd press: no Disarmed, no 2nd Armed; free-talk starts.
    let mut s = Session::new();
    assert_eq!(s.on(Down), Some(Armed));
    assert_eq!(s.on(Release { clean: true }), None);
    assert_eq!(s.on(Down), None);
    assert_eq!(s.on(DoubleTap), Some(Started(FreeTalk)));
    assert_eq!(s.on(Release { clean: true }), None);
    // Inside free-talk a press is never Armed.
    assert_eq!(s.on(Down), None);
    assert_eq!(s.on(Release { clean: true }), Some(Stopped(FreeTalk)));
    // A stray TapExpired while Idle does nothing. Reset: Armed -> Disarmed, Idle -> None.
    let mut s = Session::new();
    assert_eq!(s.on(TapExpired), None);
    assert_eq!(s.reset(), None);
    assert_eq!(s.on(Down), Some(Armed));
    assert_eq!(s.reset(), Some(Disarmed));
}

#[test]
fn t20_engine_hold_gives_armed_started_stopped() {
    let mut r = E::new();
    assert_eq!(r.down(0), vec![Armed]);
    assert_eq!(r.tick(310), vec![Started(PushToTalk)]);
    assert_eq!(r.up(900), vec![Stopped(PushToTalk)]);
}

#[test]
fn t20_engine_lone_tap_gives_armed_then_disarmed_after_400ms() {
    let mut r = E::new();
    assert_eq!(r.down(0), vec![Armed]);
    assert!(r.up(80).is_empty());
    assert!(r.tick(480).is_empty());
    assert_eq!(r.tick(481), vec![Disarmed]);
    assert!(r.tick(2000).is_empty());
}

#[test]
fn t20_engine_chord_before_hold_disarms_at_once() {
    let mut r = E::new();
    assert_eq!(r.down(0), vec![Armed]);
    assert_eq!(r.key_held(Key::Delete, 100), vec![Disarmed]);
    assert!(r.up(200).is_empty());
    assert!(r.tick(2000).is_empty());
}

#[test]
fn t20_engine_slow_second_tap_disarms_then_arms_again() {
    let mut r = E::new();
    assert_eq!(r.down(0), vec![Armed]);
    assert!(r.up(80).is_empty());
    assert_eq!(r.tick(481), vec![Disarmed]);
    assert_eq!(r.down(600), vec![Armed]);
}

#[test]
fn t20_engine_double_tap_has_one_armed_and_no_armed_inside_free_talk() {
    let mut r = E::new();
    r.free_running(0);
    assert!(r.down(2000).is_empty(), "no Armed during free-talk");
    assert_eq!(r.up(2080), vec![Stopped(FreeTalk)]);
}

#[test]
fn t20_engine_no_armed_when_another_modifier_is_held_first() {
    let mut r = E::new();
    let t = r.at(0);
    assert!(r.e.feed(&modev(FN | Modifiers::SHIFT_LEFT, true, FN), t).is_empty());
}

#[test]
fn t20_engine_hotkey_change_while_armed_disarms() {
    let mut r = E::new();
    assert_eq!(r.down(0), vec![Armed]);
    assert_eq!(r.e.set_target(HotkeyChoice::RightCmd), Some(Disarmed));
}

#[test]
fn t20_engine_force_stop_push_to_talk_then_release_is_ignored() {
    let mut r = E::new();
    assert_eq!(r.down(0), vec![Armed]);
    assert_eq!(r.tick(310), vec![Started(PushToTalk)]); // talk 1
    assert_eq!(r.e.force_stop(1), vec![Stopped(PushToTalk)]);
    assert!(r.up(900).is_empty(), "the later release of that talk is ignored");
    assert!(r.e.force_stop(1).is_empty(), "already ended");
}

#[test]
fn t20_engine_force_stop_free_talk_goes_idle_and_forgets_the_tap() {
    let mut r = E::new();
    r.free_running(0); // talk 1
    assert_eq!(r.e.force_stop(1), vec![Stopped(FreeTalk)]);
    // The next press is a fresh press, not an "ending tap" and not a 2nd tap.
    assert_eq!(r.down(2000), vec![Armed]);
    assert!(r.up(2060).is_empty());
    assert_eq!(r.tick(2461), vec![Disarmed]);
}

#[test]
fn t06_stale_force_stop_never_ends_a_newer_talk() {
    let mut r = E::new();
    r.free_running(0); // talk 1
    assert_eq!(r.down(2000), Vec::<TalkEvent>::new());
    assert_eq!(r.up(2080), vec![Stopped(FreeTalk)]); // ended by a press
    assert!(r.e.force_stop(1).is_empty(), "stale ForceStop after the user already ended it");
    assert_eq!(r.down(5000), vec![Armed]);
    assert_eq!(r.tick(5310), vec![Started(PushToTalk)]); // talk 2
    assert!(r.e.force_stop(1).is_empty(), "talk 1's message must not end talk 2");
    assert_eq!(r.e.force_stop(2), vec![Stopped(PushToTalk)]);
    assert!(r.e.force_stop(3).is_empty(), "unknown future seq does nothing");
}

#[test]
fn t20_engine_force_stop_with_nothing_running_is_empty() {
    let mut r = E::new();
    assert!(r.e.force_stop(1).is_empty());
    assert_eq!(r.down(0), vec![Armed]);
    assert!(r.e.force_stop(1).is_empty(), "armed is not a talk");
}

// =============================================================================================
// Part B: constants, silence decision (T19), tap
// =============================================================================================
#[test]
fn constants_match_the_decisions() {
    assert_eq!(MIC_OPEN_AT, MicOpenAt::Down); // founder Q1
    assert_eq!(SILENCE_END, Duration::from_millis(1500));
    assert_eq!(NO_SPEECH, Duration::from_secs(8)); // Q2
    assert_eq!(MAX_TALK, Duration::from_secs(60));
    assert_eq!(HEARD_SECS, 4);
    assert_eq!(BINDING, "so-talk");
}

#[test]
fn t19_silence_watch_decision_table() {
    let t0 = Instant::now();
    let ms = |n: u64| t0 + Duration::from_millis(n);
    let d = |started: u64, last: Option<u64>, now: u64| SilenceWatch::decide(ms(started), last.map(ms), ms(now));
    // No speech yet: only the 8 s limit counts (the silence timer is not armed).
    assert_eq!(d(0, None, 0), None);
    assert_eq!(d(0, None, 3000), None);
    assert_eq!(d(0, None, 7999), None);
    assert_eq!(d(0, None, 8000), Some(End::NoSpeech));
    assert_eq!(d(1000, None, 8999), None);
    assert_eq!(d(1000, None, 9000), Some(End::NoSpeech));
    // Speech heard: quiet for SILENCE_END minus the VAD hangover (1500 - 450 = 1050 ms) ends the talk.
    assert_eq!(QUIET_MS, 1050);
    assert_eq!(d(0, Some(500), 500), None);
    assert_eq!(d(0, Some(500), 500 + QUIET_MS - 1), None);
    assert_eq!(d(0, Some(500), 500 + QUIET_MS), Some(End::Silence));
    assert_eq!(d(0, Some(500), 3000), Some(End::Silence));
    // Speech keeps arriving: no end; the no-speech limit no longer applies once speech was heard.
    assert_eq!(d(0, Some(9000), 9500), None);
    assert_eq!(d(0, Some(30_000), 30_500), None);
    // 60 s cap, also while speech is still arriving. Wins over silence/no-speech.
    assert_eq!(d(0, Some(59_990), 59_999), None);
    assert_eq!(d(0, Some(59_990), 60_000), Some(End::MaxLen));
    assert_eq!(d(0, Some(60_000), 60_000), Some(End::MaxLen));
    // A stale tap from an earlier talk (before `started`) counts as no speech.
    assert_eq!(d(10_000, Some(2000), 17_999), None);
    assert_eq!(d(10_000, Some(2000), 18_000), Some(End::NoSpeech));
}

#[test]
fn tap_records_the_latest_speech_instant() {
    let before = Instant::now();
    tap::speech_frame();
    let a = tap::last_speech().expect("Some after a frame");
    assert!(a >= before && a <= Instant::now(), "instant is 'now'");
    thread::sleep(Duration::from_millis(20));
    tap::speech_frame();
    let b = tap::last_speech().unwrap();
    assert!(b > a, "later frame, later instant");
    assert_eq!(StaticTap.last_speech(), Some(b));
}

// =============================================================================================
// Part C: fakes for the sink
// =============================================================================================
#[derive(Default)]
struct Log(Mutex<Vec<String>>);
impl Log {
    fn add(&self, s: &str) {
        self.0.lock().unwrap().push(s.to_string());
    }
    fn all(&self) -> Vec<String> {
        self.0.lock().unwrap().clone()
    }
    fn count(&self, s: &str) -> usize {
        self.all().iter().filter(|x| *x == s).count()
    }
    fn of(&self, prefix: &str) -> Vec<String> {
        self.all().into_iter().filter(|x| x.starts_with(prefix)).collect()
    }
}

struct FakeRec {
    log: Arc<Log>,
    samples: Mutex<Option<Vec<f32>>>,
    start_delay: Mutex<Duration>,
    start_err: Mutex<Option<String>>,
}
impl Recorder for FakeRec {
    fn start(&self) -> Result<(), String> {
        self.log.add("rec.start");
        let d = *self.start_delay.lock().unwrap();
        thread::sleep(d);
        match self.start_err.lock().unwrap().clone() {
            Some(e) => Err(e),
            None => Ok(()),
        }
    }
    fn stop(&self) -> Option<Vec<f32>> {
        self.log.add("rec.stop");
        self.samples.lock().unwrap().clone()
    }
    fn cancel(&self) {
        self.log.add("rec.cancel");
    }
}

struct FakeTr {
    log: Arc<Log>,
    state: Mutex<ModelState>,
    result: Mutex<Result<String, String>>,
    delay: Mutex<Duration>,
    /// false = a cold model: loads `load_ms` after `preload`; `transcribe` waits for it. If `preload` was never
    /// called, a cold `transcribe` returns "" (the gate-8 symptom).
    loaded: Arc<AtomicBool>,
    load_started: AtomicBool,
    load_ms: Mutex<u64>,
    last_len: Mutex<Option<usize>>,
}
impl Transcriber for FakeTr {
    fn downloaded(&self) -> ModelState {
        *self.state.lock().unwrap()
    }
    fn preload(&self) {
        self.log.add("tr.preload");
        if !self.load_started.swap(true, Ordering::SeqCst) {
            let (loaded, ms) = (self.loaded.clone(), *self.load_ms.lock().unwrap());
            thread::spawn(move || {
                thread::sleep(Duration::from_millis(ms));
                loaded.store(true, Ordering::SeqCst);
            });
        }
    }
    fn transcribe(&self, samples: Vec<f32>) -> Result<String, String> {
        self.log.add("tr.transcribe");
        *self.last_len.lock().unwrap() = Some(samples.len());
        if self.load_started.load(Ordering::SeqCst) {
            while !self.loaded.load(Ordering::SeqCst) {
                thread::sleep(Duration::from_millis(5));
            }
        }
        let cold = !self.loaded.load(Ordering::SeqCst);
        thread::sleep(*self.delay.lock().unwrap());
        if cold {
            return Ok(String::new());
        }
        self.result.lock().unwrap().clone()
    }
}

#[derive(Debug, Clone, PartialEq)]
enum Op {
    Show(PanelPayload),
    Hide,
}
#[derive(Default)]
struct FakePanel {
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
    fn ops(&self) -> Vec<Op> {
        self.ops.lock().unwrap().clone()
    }
    /// First line of every show, in order.
    fn texts(&self) -> Vec<String> {
        self.ops()
            .into_iter()
            .filter_map(|o| match o {
                Op::Show(p) => p.lines.first().map(|l| l.text.clone()),
                Op::Hide => None,
            })
            .collect()
    }
    fn last_show(&self) -> Option<Line> {
        self.ops().into_iter().rev().find_map(|o| match o {
            Op::Show(p) => p.lines.first().cloned(),
            Op::Hide => None,
        })
    }
    fn hides(&self) -> usize {
        self.ops().iter().filter(|o| **o == Op::Hide).count()
    }
}

struct FakeClock {
    base: Instant,
    ms: Mutex<u64>,
}
impl FakeClock {
    fn new() -> Arc<Self> {
        Arc::new(FakeClock { base: Instant::now(), ms: Mutex::new(0) })
    }
    fn set(&self, ms: u64) {
        *self.ms.lock().unwrap() = ms;
    }
}
impl Clock for FakeClock {
    fn now(&self) -> Instant {
        self.base + Duration::from_millis(*self.ms.lock().unwrap())
    }
}

struct FakeTap {
    base: Instant,
    last: Mutex<Option<Instant>>,
}
impl SpeechTap for FakeTap {
    fn last_speech(&self) -> Option<Instant> {
        *self.last.lock().unwrap()
    }
}
impl FakeTap {
    /// Speech was last heard at clock time `ms`.
    fn speech_at(&self, ms: u64) {
        *self.last.lock().unwrap() = Some(self.base + Duration::from_millis(ms));
    }
}

#[derive(Default)]
struct FakeStopper(Mutex<Vec<u64>>);
impl Stopper for FakeStopper {
    fn force_stop(&self, seq: u64) {
        self.0.lock().unwrap().push(seq);
    }
}
impl FakeStopper {
    fn calls(&self) -> Vec<u64> {
        self.0.lock().unwrap().clone()
    }
}

/// Real watcher stopper: sends `Msg::ForceStop` into the watcher channel.
struct ChannelStopper(Mutex<Sender<Msg>>);
impl Stopper for ChannelStopper {
    fn force_stop(&self, seq: u64) {
        let _ = self.0.lock().unwrap().send(Msg::ForceStop(seq));
    }
}

const SAMPLES: usize = 1600;
/// Quiet after the last speech that ends a free-talk: SILENCE_END minus the VAD hangover.
const QUIET_MS: u64 = 1500 - VAD_OFFLINE_HANGOVER_MS;

struct Fakes {
    log: Arc<Log>,
    rec: Arc<FakeRec>,
    tr: Arc<FakeTr>,
    panel: Arc<FakePanel>,
    clock: Arc<FakeClock>,
    tap: Arc<FakeTap>,
}
impl Fakes {
    fn new() -> Self {
        let log = Arc::new(Log::default());
        let clock = FakeClock::new();
        Fakes {
            rec: Arc::new(FakeRec {
                log: log.clone(),
                samples: Mutex::new(Some(vec![0.1; SAMPLES])),
                start_delay: Mutex::new(Duration::ZERO),
                start_err: Mutex::new(None),
            }),
            tr: Arc::new(FakeTr {
                log: log.clone(),
                state: Mutex::new(ModelState::Ready),
                result: Mutex::new(Ok("hello world".into())),
                delay: Mutex::new(Duration::ZERO),
                loaded: Arc::new(AtomicBool::new(true)),
                load_started: AtomicBool::new(false),
                load_ms: Mutex::new(0),
                last_len: Mutex::new(None),
            }),
            panel: Arc::new(FakePanel::default()),
            tap: Arc::new(FakeTap { base: clock.base, last: Mutex::new(None) }),
            clock,
            log,
        }
    }
    /// A cold model: not loaded; the load finishes `ms` after the first `preload`.
    fn cold(&self, ms: u64) {
        self.tr.loaded.store(false, Ordering::SeqCst);
        *self.tr.load_ms.lock().unwrap() = ms;
    }
    fn sink(&self, stopper: Arc<dyn Stopper>) -> SpeechSink {
        SpeechSink::new(
            self.rec.clone(),
            self.tr.clone(),
            self.panel.clone(),
            self.clock.clone(),
            self.tap.clone(),
            stopper,
            Box::new(|| HotkeyChoice::Fn),
        )
    }
}

/// Fakes + a sink with a recording stopper.
struct H {
    f: Fakes,
    stopper: Arc<FakeStopper>,
    sink: SpeechSink,
}
impl H {
    fn new() -> Self {
        Self::from(Fakes::new())
    }
    fn from(f: Fakes) -> Self {
        let stopper = Arc::new(FakeStopper::default());
        let sink = f.sink(stopper.clone());
        H { f, stopper, sink }
    }
    fn ev(&self, e: TalkEvent) {
        self.sink.on_event(e);
    }
    /// Armed, Started(PushToTalk), Stopped(PushToTalk).
    fn ptt(&self) {
        self.ev(Armed);
        self.ev(Started(PushToTalk));
        self.ev(Stopped(PushToTalk));
    }
}

/// Poll up to `max` for `f` to become true.
fn wait_until(what: &str, max: Duration, f: impl Fn() -> bool) {
    let start = Instant::now();
    while start.elapsed() < max {
        if f() {
            return;
        }
        thread::sleep(Duration::from_millis(5));
    }
    panic!("timed out waiting for: {what}");
}
fn wait(what: &str, f: impl Fn() -> bool) {
    wait_until(what, Duration::from_secs(3), f)
}
fn settle() {
    thread::sleep(Duration::from_millis(150));
}

// =============================================================================================
// Part D: SpeechSink (T1-T4, T9-T18, T27)
// =============================================================================================
#[test]
fn t01_armed_preloads_once_and_opens_the_mic_without_touching_the_panel() {
    let h = H::new();
    h.ev(Armed);
    wait("rec.start", || h.f.log.count("rec.start") == 1);
    wait("preload", || h.f.log.count("tr.preload") == 1);
    settle();
    assert_eq!(h.f.log.count("rec.start"), 1);
    assert_eq!(h.f.log.count("tr.preload"), 1, "preload once per Armed");
    assert!(h.f.panel.ops().is_empty(), "Armed never writes to the panel");
}

#[test]
fn t02_lone_tap_cancels_the_recording_and_leaves_the_panel_alone() {
    let h = H::new();
    h.ev(Armed);
    h.ev(Disarmed);
    wait("rec.cancel", || h.f.log.count("rec.cancel") == 1);
    settle();
    assert_eq!(h.f.log.count("tr.transcribe"), 0);
    assert!(h.f.panel.ops().is_empty(), "no show, no hide");
}

#[test]
fn t03_hold_release_transcribes_the_samples_shows_heard_and_hides_after_4_s() {
    let h = H::new();
    h.ptt();
    wait("Heard", || h.f.panel.texts().last().is_some_and(|t| t == "Heard: hello world"));
    let shown_at = Instant::now();
    assert_eq!(h.f.panel.texts(), vec!["Listening", TRANSCRIBING_TEXT, "Heard: hello world"]);
    assert_eq!(h.f.panel.last_show().unwrap().kind, Kind::Info);
    assert_eq!(*h.f.tr.last_len.lock().unwrap(), Some(SAMPLES), "the recorder's samples are what is transcribed");
    assert_eq!(h.f.log.count("tr.transcribe"), 1);
    assert_eq!(h.f.log.of("rec."), vec!["rec.start", "rec.stop"]);
    assert_eq!(h.f.panel.hides(), 0, "still visible at once");
    wait_until("hide after 4 s", Duration::from_secs(6), || h.f.panel.hides() == 1);
    let visible = shown_at.elapsed();
    assert!(visible >= Duration::from_millis(3500), "hidden too early: {visible:?}");
    assert!(visible <= Duration::from_millis(5500), "hidden too late: {visible:?}");
}

#[test]
fn t04_chord_during_push_to_talk_cancels_and_never_transcribes() {
    let h = H::new();
    h.ev(Armed);
    h.ev(Started(PushToTalk));
    h.ev(Cancelled(PushToTalk));
    wait("rec.cancel", || h.f.log.count("rec.cancel") == 1);
    wait("hide", || h.f.panel.hides() == 1);
    settle();
    assert_eq!(h.f.log.count("tr.transcribe"), 0);
    assert_eq!(h.f.panel.texts(), vec!["Listening"]);
    assert_eq!(h.f.panel.ops().last(), Some(&Op::Hide), "the cancel hides the panel");
}

#[test]
fn t05_free_talk_silence_ends_the_talk_once() {
    let h = H::new();
    h.f.clock.set(0);
    h.ev(Armed);
    h.ev(Started(FreeTalk));
    wait("Started shown", || h.f.panel.texts().len() == 1);
    h.f.tap.speech_at(500);
    h.f.clock.set(500 + QUIET_MS - 50); // just under the limit
    thread::sleep(Duration::from_millis(350));
    assert_eq!(h.stopper.calls(), Vec::<u64>::new(), "not enough quiet yet");
    h.f.clock.set(500 + QUIET_MS + 50);
    wait("force_stop(1)", || h.stopper.calls() == vec![1]);
    thread::sleep(Duration::from_millis(400));
    assert_eq!(h.stopper.calls(), vec![1], "requested once, not every 100 ms");
}

#[test]
fn t05_the_timers_stop_when_the_talk_ends() {
    let h = H::new();
    h.f.clock.set(0);
    h.ev(Armed);
    h.ev(Started(FreeTalk));
    h.ev(Stopped(FreeTalk));
    wait("transcribe done", || h.f.panel.texts().iter().any(|t| t.starts_with("Heard")));
    h.f.clock.set(100_000);
    thread::sleep(Duration::from_millis(400));
    assert!(h.stopper.calls().is_empty(), "no timer outlives its talk");
}

#[test]
fn t07_no_speech_for_8_s_ends_the_talk_and_the_silence_timer_waits_for_speech() {
    let h = H::new();
    *h.f.rec.samples.lock().unwrap() = None;
    h.f.clock.set(0);
    h.ev(Armed);
    h.ev(Started(FreeTalk));
    wait("Started shown", || h.f.panel.texts().len() == 1);
    h.f.clock.set(3000); // far more than 1050 ms, but nobody spoke yet
    thread::sleep(Duration::from_millis(350));
    assert!(h.stopper.calls().is_empty(), "silence timer is not armed before the first speech");
    h.f.clock.set(7999);
    thread::sleep(Duration::from_millis(350));
    assert!(h.stopper.calls().is_empty());
    h.f.clock.set(8000);
    wait("force_stop(1)", || h.stopper.calls() == vec![1]);
    // The watcher answers with Stopped; nothing was recorded.
    h.ev(Stopped(FreeTalk));
    wait("Didn't catch that", || h.f.panel.texts().last().is_some_and(|t| t == DIDNT_CATCH_TEXT));
    assert_eq!(h.f.log.count("tr.transcribe"), 0);
}

#[test]
fn t08_push_to_talk_is_capped_at_60_s_but_not_ended_by_quiet() {
    let h = H::new();
    h.f.clock.set(0);
    h.ev(Armed);
    h.ev(Started(PushToTalk));
    wait("Started shown", || h.f.panel.texts().len() == 1);
    h.f.clock.set(30_000); // no speech for 30 s while the key is held: fine
    thread::sleep(Duration::from_millis(350));
    assert!(h.stopper.calls().is_empty(), "push-to-talk ignores the silence and no-speech limits");
    h.f.clock.set(60_000);
    wait("force_stop(1)", || h.stopper.calls() == vec![1]);
}

#[test]
fn t08_free_talk_is_capped_at_60_s_even_while_speech_continues() {
    let h = H::new();
    h.f.clock.set(0);
    h.ev(Armed);
    h.ev(Started(FreeTalk));
    wait("Started shown", || h.f.panel.texts().len() == 1);
    h.f.tap.speech_at(59_990);
    h.f.clock.set(59_999);
    thread::sleep(Duration::from_millis(350));
    assert!(h.stopper.calls().is_empty());
    h.f.clock.set(60_000);
    wait("force_stop(1)", || h.stopper.calls() == vec![1]);
}

#[test]
fn t09_hotkey_change_mid_talk_or_while_armed_discards_the_audio() {
    // Mid push-to-talk: the engine says Cancelled.
    let h = H::new();
    let mut e = E::new();
    assert_eq!(e.down(0), vec![Armed]);
    assert_eq!(e.tick(310), vec![Started(PushToTalk)]);
    for ev in [Armed, Started(PushToTalk)] {
        h.ev(ev);
    }
    let cancelled = e.e.set_target(HotkeyChoice::RightCmd).expect("Cancelled");
    assert_eq!(cancelled, Cancelled(PushToTalk));
    h.ev(cancelled);
    wait("rec.cancel", || h.f.log.count("rec.cancel") == 1);
    settle();
    assert_eq!(h.f.log.count("tr.transcribe"), 0);
    assert_eq!(h.f.panel.ops().last(), Some(&Op::Hide));
    // Only armed (mic open on Down): Disarmed cancels.
    let h = H::new();
    h.ev(Armed);
    h.ev(Disarmed);
    wait("rec.cancel", || h.f.log.count("rec.cancel") == 1);
    assert_eq!(h.f.log.count("tr.transcribe"), 0);
}

#[test]
fn t10_no_samples_or_blank_text_says_didnt_catch_that() {
    // No samples: the engine is not called.
    for samples in [None, Some(vec![])] {
        let h = H::new();
        *h.f.rec.samples.lock().unwrap() = samples;
        h.ptt();
        wait("Didn't catch that", || h.f.panel.texts().last().is_some_and(|t| t == DIDNT_CATCH_TEXT));
        settle();
        assert_eq!(h.f.log.count("tr.transcribe"), 0, "no samples: engine skipped");
    }
    // Blank text.
    for blank in ["", "   ", " \n\t "] {
        let h = H::new();
        *h.f.tr.result.lock().unwrap() = Ok(blank.to_string());
        h.ptt();
        wait("Didn't catch that", || h.f.panel.texts().last().is_some_and(|t| t == DIDNT_CATCH_TEXT));
        assert_eq!(h.f.log.count("tr.transcribe"), 1);
    }
}

#[test]
fn t11_model_missing_or_downloading_warns_and_never_opens_the_mic() {
    for (state, text) in [
        (ModelState::Missing, MODEL_MISSING_TEXT),
        (ModelState::Downloading, MODEL_DOWNLOADING_TEXT),
    ] {
        let h = H::new();
        *h.f.tr.state.lock().unwrap() = state;
        h.ev(Armed);
        settle();
        assert_eq!(h.f.log.count("rec.start"), 0, "Armed with no model starts nothing");
        assert_eq!(h.f.log.count("tr.preload"), 0);
        assert!(h.f.panel.ops().is_empty(), "Armed never writes");
        h.ev(Started(PushToTalk));
        wait("warn line", || h.f.panel.last_show().is_some_and(|l| l.text == text));
        assert_eq!(h.f.panel.last_show().unwrap().kind, Kind::Warn);
        h.ev(Stopped(PushToTalk));
        settle();
        assert_eq!(h.f.log.count("rec.start"), 0, "mic never opened");
        assert_eq!(h.f.log.count("tr.transcribe"), 0);
    }
    assert_ne!(MODEL_MISSING_TEXT, MODEL_DOWNLOADING_TEXT);
}

#[test]
fn t12_load_still_running_at_release_waits_and_gives_text() {
    // Cold-empty regression (gate 8): a release right after the press, with the load still running.
    let f = Fakes::new();
    f.cold(400);
    let h = H::from(f);
    h.ptt(); // Armed, Started, Stopped back to back
    wait("Heard", || h.f.panel.texts().last().is_some_and(|t| t == "Heard: hello world"));
    assert!(!h.f.panel.texts().iter().any(|t| t == DIDNT_CATCH_TEXT));
    assert_eq!(h.f.log.count("tr.preload"), 1, "the load was started at Armed");
}

#[test]
fn t13_transcribe_error_shows_a_fixed_warning_without_the_error_text() {
    let h = H::new();
    *h.f.tr.result.lock().unwrap() = Err("secret detail XYZ".into());
    h.ptt();
    wait("Speech failed", || h.f.panel.last_show().is_some_and(|l| l.text == SPEECH_FAILED_TEXT));
    assert_eq!(h.f.panel.last_show().unwrap().kind, Kind::Warn);
    assert!(h.f.panel.texts().iter().all(|t| !t.contains("XYZ")));
}

#[test]
fn t14_recorder_start_failure_warns_at_started_and_never_panics() {
    let h = H::new();
    *h.f.rec.start_err.lock().unwrap() = Some("device busy QQ".into());
    h.ev(Armed);
    wait("rec.start", || h.f.log.count("rec.start") == 1);
    settle();
    assert!(h.f.panel.ops().is_empty(), "Armed never writes, even when the mic fails");
    h.ev(Started(PushToTalk));
    wait("mic warning", || h.f.panel.last_show().is_some_and(|l| l.text == MIC_FAILED_TEXT));
    assert_eq!(h.f.panel.last_show().unwrap().kind, Kind::Warn);
    h.ev(Stopped(PushToTalk));
    settle();
    assert!(h.f.panel.texts().iter().all(|t| !t.contains("QQ")));
    // The worker survived: a later Armed still reaches the recorder.
    h.ev(Armed);
    wait("2nd rec.start", || h.f.log.count("rec.start") == 2);
}

fn literal_of(script: &str) -> &str {
    script
        .trim_end_matches(';')
        .strip_prefix("window.soPanel.set(JSON.parse(")
        .and_then(|r| r.strip_suffix("))"))
        .unwrap_or_else(|| panic!("unexpected eval shape: {script}"))
}

#[test]
fn t15_heard_line_trims_cuts_and_round_trips_nasty_text() {
    assert_eq!(heard_line("play blinding lights"), "Heard: play blinding lights");
    assert_eq!(heard_line("  hi\nthere\t\u{7}x  "), "Heard: hi there  x", "trim; control chars and newlines become spaces");
    // 200 chars in all, "Heard: " included.
    let fits = "a".repeat(193);
    assert_eq!(heard_line(&fits), format!("Heard: {fits}"));
    let long = "a".repeat(194);
    let cut = heard_line(&long);
    assert_eq!(cut.chars().count(), 200);
    assert!(cut.starts_with("Heard: ") && cut.ends_with('…'), "{cut}");
    let multibyte = heard_line(&"é😀".repeat(300));
    assert_eq!(multibyte.chars().count(), 200);
    assert!(multibyte.ends_with('…'));
    // Nasty text: kept as is, and the panel's JS encoding round-trips it.
    for text in ["say \"hi\"", "back\\slash \\\"", "</script><script>alert(1)</script>", "sep \u{2028} end"] {
        let line = heard_line(text);
        assert_eq!(line, format!("Heard: {text}"), "kept verbatim");
        let payload = PanelPayload::new(vec![Line::new(line, Kind::Info)]);
        let script = payload.eval_script();
        assert!(!script.contains("</script"), "{script}");
        assert!(!script.contains('\u{2028}'), "raw U+2028 in JS");
        let lit = literal_of(&script);
        let json: String = serde_json::from_str(lit).expect("literal is a JSON string");
        let back: PanelPayload = serde_json::from_str(&json).expect("payload JSON");
        assert_eq!(back, payload);
    }
}

#[test]
fn t16_after_heard_hides_the_stored_last_payload_is_none() {
    let h = H::new();
    let shown = PanelPayload::new(vec![Line::new("Heard: hello world", Kind::Info)]);
    panel::set_last(Some(shown.clone())); // what the real panel stores on `show`
    h.ptt();
    wait("Heard", || h.f.panel.texts().last().is_some_and(|t| t == "Heard: hello world"));
    // Other tests' timers share this static; keep it armed until OUR hide happens.
    let start = Instant::now();
    while h.f.panel.hides() == 0 {
        assert!(start.elapsed() < Duration::from_secs(7), "no hide after Heard");
        if panel::last_payload().is_none() {
            panel::set_last(Some(shown.clone()));
        }
        thread::sleep(Duration::from_millis(10));
    }
    thread::sleep(Duration::from_millis(50));
    assert_eq!(panel::last_payload(), None, "timed hide must clear the stored payload");
}

#[test]
fn t17_on_event_returns_at_once_even_when_recorder_and_transcriber_block() {
    let f = Fakes::new();
    *f.rec.start_delay.lock().unwrap() = Duration::from_millis(500);
    *f.tr.delay.lock().unwrap() = Duration::from_millis(500);
    let h = H::from(f);
    for e in [Armed, Started(PushToTalk), Stopped(PushToTalk), Armed, Disarmed] {
        let t = Instant::now();
        h.ev(e);
        assert!(t.elapsed() < Duration::from_millis(5), "{e:?} took {:?}", t.elapsed());
    }
    // Everything still happens, in order, once the slow calls finish.
    wait("transcribe", || h.f.log.count("tr.transcribe") == 1);
    wait("second rec.start and cancel", || h.f.log.count("rec.cancel") == 1);
}

struct Tee {
    events: Mutex<Vec<TalkEvent>>,
    inner: Arc<dyn TalkSink>,
}
impl TalkSink for Tee {
    fn on_event(&self, e: TalkEvent) {
        self.events.lock().unwrap().push(e);
        self.inner.on_event(e);
    }
}

/// A real watcher loop in front of a real SpeechSink (fake recorder/transcriber/clock).
struct Wired {
    f: Fakes,
    tx: Sender<Msg>,
    tee: Arc<Tee>,
    handle: thread::JoinHandle<()>,
}
impl Wired {
    fn new(f: Fakes) -> Self {
        let (tx, rx) = channel();
        let sink = Arc::new(f.sink(Arc::new(ChannelStopper(Mutex::new(tx.clone())))));
        let tee = Arc::new(Tee { events: Mutex::new(vec![]), inner: sink });
        let (t, c, p) = (tee.clone(), f.clock.clone(), f.panel.clone());
        let handle = thread::spawn(move || run(rx, HotkeyChoice::Fn, t, p, c));
        Wired { f, tx, tee, handle }
    }
    fn key(&self, ms: u64, ev: KeyEvent) {
        self.f.clock.set(ms);
        self.tx.send(Msg::Key(ev)).unwrap();
        thread::sleep(Duration::from_millis(50));
    }
    fn down(&self, ms: u64) {
        self.key(ms, modev(FN, true, FN));
    }
    fn up(&self, ms: u64) {
        self.key(ms, modev(NONE, false, FN));
    }
    fn events(&self) -> Vec<TalkEvent> {
        self.tee.events.lock().unwrap().clone()
    }
    fn wait_events(&self, n: usize) -> Vec<TalkEvent> {
        wait_until("events", Duration::from_secs(3), || self.events().len() >= n);
        self.events()
    }
    fn finish(self) {
        let _ = self.tx.send(Msg::Shutdown);
        assert!(self.handle.join().is_ok(), "watcher did not panic");
    }
}

/// The four B1 scripts: tap, hold, double-tap + ending tap, press-then-chord.
fn script(w: &Wired, which: &str) -> Vec<TalkEvent> {
    match which {
        "tap" => {
            w.down(0);
            w.up(80);
            w.f.clock.set(600); // the 20 ms timeout notices the 400 ms window
            w.wait_events(2)
        }
        "hold" => {
            w.down(0);
            w.f.clock.set(310);
            w.wait_events(2);
            w.up(900);
            w.wait_events(3)
        }
        "double" => {
            w.down(0);
            w.up(80);
            w.down(250);
            w.up(300);
            w.down(2000);
            w.up(2080);
            w.wait_events(3)
        }
        "chord" => {
            w.down(0);
            w.key(100, keyev(FN, Key::C, true));
            w.up(200);
            w.wait_events(2)
        }
        other => panic!("{other}"),
    }
}

#[test]
fn t17_watcher_scripts_give_the_same_events_with_blocking_and_instant_fakes() {
    let expected: [(&str, Vec<TalkEvent>); 4] = [
        ("tap", vec![Armed, Disarmed]),
        ("hold", vec![Armed, Started(PushToTalk), Stopped(PushToTalk)]),
        ("double", vec![Armed, Started(FreeTalk), Stopped(FreeTalk)]),
        ("chord", vec![Armed, Disarmed]),
    ];
    for (which, want) in expected {
        for blocking in [false, true] {
            let f = Fakes::new();
            if blocking {
                *f.rec.start_delay.lock().unwrap() = Duration::from_millis(500);
                *f.tr.delay.lock().unwrap() = Duration::from_millis(500);
            }
            let w = Wired::new(f);
            let t = Instant::now();
            let got = script(&w, which);
            assert_eq!(got, want, "{which} blocking={blocking}");
            assert!(t.elapsed() < Duration::from_millis(1500), "{which} blocking={blocking}: watcher was held up ({:?})", t.elapsed());
            w.finish();
        }
    }
}

#[test]
fn t05_wired_free_talk_silence_goes_through_the_watcher_and_transcribes() {
    let w = Wired::new(Fakes::new());
    w.down(0);
    w.up(80);
    w.down(250);
    w.up(300);
    assert_eq!(w.wait_events(2), vec![Armed, Started(FreeTalk)]);
    w.f.tap.speech_at(500);
    w.f.clock.set(1000); // 500 ms of quiet
    thread::sleep(Duration::from_millis(350));
    assert_eq!(w.events().len(), 2, "500 ms of quiet does not end the talk");
    w.f.clock.set(500 + QUIET_MS + 50);
    assert_eq!(w.wait_events(3), vec![Armed, Started(FreeTalk), Stopped(FreeTalk)]);
    wait("Heard", || w.f.panel.texts().last().is_some_and(|t| t == "Heard: hello world"));
    assert_eq!(w.f.log.count("tr.transcribe"), 1);
    // A later release/tap of that talk does not produce a second Stopped.
    thread::sleep(Duration::from_millis(300));
    assert_eq!(w.events().iter().filter(|e| matches!(e, Stopped(_))).count(), 1);
    w.finish();
}

// ---------- T18: no transcript or error text in any log record ----------
struct CaptureLogger;
static CAPTURED: Mutex<Vec<String>> = Mutex::new(Vec::new());
static LOGGER: CaptureLogger = CaptureLogger;
impl log::Log for CaptureLogger {
    fn enabled(&self, _: &log::Metadata) -> bool {
        true
    }
    fn log(&self, r: &log::Record) {
        CAPTURED.lock().unwrap().push(format!("{} {}", r.target(), r.args()));
    }
    fn flush(&self) {}
}

#[test]
fn t18_a_sentinel_transcript_or_error_never_reaches_the_logs_at_trace() {
    let _ = log::set_logger(&LOGGER);
    log::set_max_level(log::LevelFilter::Trace);
    const SENT: &str = "ZXQSENTINELTRANSCRIPT";
    const ERR: &str = "ZXQSENTINELERROR";
    // 1: a normal talk with the sentinel as text.
    let h = H::new();
    *h.f.tr.result.lock().unwrap() = Ok(SENT.into());
    h.ptt();
    wait("Heard sentinel", || h.f.panel.texts().last().is_some_and(|t| t.contains(SENT)));
    // 2: a failing transcription with the sentinel in the error. 3: a failing mic.
    let h2 = H::new();
    *h2.f.tr.result.lock().unwrap() = Err(ERR.into());
    h2.ptt();
    wait("Speech failed", || h2.f.panel.last_show().is_some_and(|l| l.text == SPEECH_FAILED_TEXT));
    let h3 = H::new();
    *h3.f.rec.start_err.lock().unwrap() = Some(ERR.into());
    h3.ev(Armed);
    h3.ev(Started(PushToTalk));
    wait("mic warning", || h3.f.panel.last_show().is_some_and(|l| l.text == MIC_FAILED_TEXT));
    settle();
    let logs = CAPTURED.lock().unwrap().clone();
    assert!(logs.iter().all(|l| !l.contains("ZXQSENTINEL")), "a sentinel reached the log: {:?}", logs.iter().find(|l| l.contains("ZXQSENTINEL")));
}

// ---------- T27: overlap, newest talk owns the panel ----------
#[test]
fn t27_text_from_an_older_talk_is_dropped_when_a_newer_talk_started() {
    let f = Fakes::new();
    *f.tr.delay.lock().unwrap() = Duration::from_millis(800);
    let h = H::from(f);
    h.ptt(); // talk 1, transcribing for 800 ms
    wait("Transcribing", || h.f.panel.texts().iter().any(|t| t == TRANSCRIBING_TEXT));
    h.ev(Armed);
    h.ev(Started(PushToTalk)); // talk 2
    wait("2nd Listening", || h.f.panel.texts().iter().filter(|t| *t == "Listening").count() == 2);
    thread::sleep(Duration::from_millis(1200)); // talk 1's text arrives meanwhile
    assert!(h.f.panel.texts().iter().all(|t| !t.starts_with("Heard")), "talk 1's Heard must be dropped: {:?}", h.f.panel.texts());
    assert_eq!(h.f.panel.last_show().unwrap().text, "Listening", "talk 2's line stays");
    assert_eq!(h.f.panel.hides(), 0);
    // Recorder: talk 1 was stopped before talk 2 could start.
    assert_eq!(h.f.log.of("rec."), vec!["rec.start", "rec.stop", "rec.start"]);
}

#[test]
fn t27_an_old_talks_4_s_hide_never_hides_the_newer_talk() {
    let h = H::new();
    h.ptt();
    wait("Heard", || h.f.panel.texts().last().is_some_and(|t| t == "Heard: hello world"));
    let heard_at = Instant::now();
    h.ev(Armed);
    h.ev(Started(PushToTalk)); // talk 2 shows Listening
    wait("2nd Listening", || h.f.panel.texts().last().is_some_and(|t| t == "Listening"));
    let until = heard_at + Duration::from_secs(HEARD_SECS) + Duration::from_millis(900);
    thread::sleep(until.saturating_duration_since(Instant::now()));
    assert_eq!(h.f.panel.hides(), 0, "talk 1's hide timer must not hide talk 2");
    assert_eq!(h.f.panel.last_show().unwrap().text, "Listening");
}

#[test]
fn t27_a_lone_tap_while_heard_shows_does_not_change_or_hide_the_line() {
    let h = H::new();
    h.ptt();
    wait("Heard", || h.f.panel.texts().last().is_some_and(|t| t == "Heard: hello world"));
    let before = h.f.panel.ops();
    h.ev(Armed);
    h.ev(Disarmed);
    wait("rec.cancel", || h.f.log.count("rec.cancel") == 1);
    settle();
    assert_eq!(h.f.panel.ops(), before, "Armed/Disarmed never write or hide");
}

#[test]
fn t27_a_chord_press_while_heard_shows_does_not_change_or_hide_the_line() {
    // Fn+Delete: Armed, then Disarmed at once.
    let h = H::new();
    h.ptt();
    wait("Heard", || h.f.panel.texts().last().is_some_and(|t| t == "Heard: hello world"));
    let before = h.f.panel.ops();
    h.ev(Armed);
    h.ev(Disarmed);
    settle();
    assert_eq!(h.f.panel.ops(), before);
}

// ---------- TaggedPanel ----------
#[test]
fn tagged_panel_drops_writes_from_older_talks() {
    use crate::so::panel::TaggedPanel;
    let fake = Arc::new(FakePanel::default());
    let tp = TaggedPanel::new(fake.clone());
    let p = |t: &str| PanelPayload::new(vec![Line::new(t, Kind::Info)]);
    tp.show(2, p("two"));
    tp.show(1, p("one"));
    assert_eq!(fake.texts(), vec!["two"], "older talk dropped");
    tp.hide(1);
    assert_eq!(fake.hides(), 0, "older hide dropped");
    tp.show(2, p("two again"));
    tp.hide(2);
    assert_eq!(fake.texts(), vec!["two", "two again"]);
    assert_eq!(fake.hides(), 1);
    tp.show(3, p("three"));
    tp.show(2, p("late two"));
    assert_eq!(fake.texts(), vec!["two", "two again", "three"]);
}

#[test]
fn panel_sink_ignores_armed_and_disarmed() {
    use crate::so::panel::{action_for, PanelAction};
    for e in [Armed, Disarmed] {
        assert_eq!(action_for(e, HotkeyChoice::Fn), PanelAction::Ignore);
    }
    // Step 1 behaviour unchanged.
    assert!(matches!(action_for(Started(PushToTalk), HotkeyChoice::Fn), PanelAction::Show(_)));
}
