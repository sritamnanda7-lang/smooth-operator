// SPDX-License-Identifier: FSL-1.1-MIT
//! Review fixes B1 (n/a here, see strip-checks), S1 (mic-ready timeout), S2 (watcher end resets the
//! sink), S3 (a forgotten tap is announced). Fakes only.
use crate::so::hotkey::detector::{DetOut, Detector};
use crate::so::hotkey::session::Mode::PushToTalk;
use crate::so::hotkey::session::TalkEvent::{self, Armed, Cancelled, Disarmed, Started};
use crate::so::hotkey::watcher::{run, Clock, Msg, TalkSink};
use crate::so::hotkey::Engine;
use crate::so::panel::{Kind, PanelOut, PanelPayload};
use crate::so::settings::HotkeyChoice;
use crate::so::stt::real::wait_ready;
use crate::so::stt::sink::{ModelState, Recorder, SpeechSink, SpeechTap, Stopper, Transcriber, MIC_FAILED_TEXT};
use handy_keys::{KeyEvent, Modifiers};
use std::sync::mpsc::{channel, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

const FN: Modifiers = Modifiers::FN;
const NONE: Modifiers = Modifiers::empty();
fn modev(flags: Modifiers, down: bool, changed: Modifiers) -> KeyEvent {
    KeyEvent { modifiers: flags, key: None, is_key_down: down, changed_modifier: Some(changed) }
}

// ---------------- S1 ----------------
#[test]
fn s1_wait_ready_gives_up_on_a_call_that_never_returns() {
    let (hold, rx) = channel::<()>();
    let t = Instant::now();
    assert!(!wait_ready(move || rx.recv().is_ok(), Duration::from_millis(200)));
    assert!(t.elapsed() < Duration::from_secs(2));
    drop(hold); // lets the helper thread end
    assert!(wait_ready(|| true, Duration::from_millis(500)));
    assert!(!wait_ready(|| false, Duration::from_millis(500)));
}

struct Log(Mutex<Vec<String>>);
struct NeverReadyRec {
    log: Arc<Log>,
    hold: Mutex<Option<std::sync::mpsc::Receiver<()>>>,
}
impl Recorder for NeverReadyRec {
    fn start(&self) -> Result<(), String> {
        self.log.0.lock().unwrap().push("start".into());
        // The real recorder's shape: wait for audio with a limit; no audio = Err.
        let rx = self.hold.lock().unwrap().take();
        let ok = wait_ready(move || rx.is_some_and(|r| r.recv().is_ok()), Duration::from_millis(200));
        if ok { Ok(()) } else { Err("no audio".into()) }
    }
    fn stop(&self) -> Option<Vec<f32>> {
        self.log.0.lock().unwrap().push("stop".into());
        None
    }
    fn cancel(&self) {
        self.log.0.lock().unwrap().push("cancel".into());
    }
}
struct Tr;
impl Transcriber for Tr {
    fn downloaded(&self) -> ModelState {
        ModelState::Ready
    }
    fn preload(&self) {}
    fn transcribe(&self, _: Vec<f32>) -> Result<String, String> {
        Ok("x".into())
    }
}
struct Tap;
impl SpeechTap for Tap {
    fn last_speech(&self) -> Option<Instant> {
        None
    }
}
struct NoStop;
impl Stopper for NoStop {
    fn force_stop(&self, _: u64) {}
}
struct Clk;
impl Clock for Clk {
    fn now(&self) -> Instant {
        Instant::now()
    }
}
#[derive(Default)]
struct Panel {
    shown: Mutex<Vec<(String, Kind)>>,
    hides: Mutex<usize>,
    sticky: std::sync::atomic::AtomicBool,
    /// Order of writes: "show:<text>" / "hide".
    order: Mutex<Vec<String>>,
}
impl PanelOut for Panel {
    fn show(&self, p: PanelPayload) {
        self.shown.lock().unwrap().push((p.lines[0].text.clone(), p.lines[0].kind));
        self.order.lock().unwrap().push(format!("show:{}", p.lines[0].text));
    }
    fn hide(&self) {
        *self.hides.lock().unwrap() += 1;
        self.order.lock().unwrap().push("hide".into());
    }
    fn set_sticky(&self, on: bool) {
        self.sticky.store(on, std::sync::atomic::Ordering::SeqCst);
    }
    fn sticky(&self) -> bool {
        self.sticky.load(std::sync::atomic::Ordering::SeqCst)
    }
}

#[test]
fn s1_a_mic_that_never_yields_audio_warns_and_the_worker_keeps_going() {
    let log = Arc::new(Log(Mutex::new(vec![])));
    let (hold, rx) = channel::<()>();
    let rec = Arc::new(NeverReadyRec { log: log.clone(), hold: Mutex::new(Some(rx)) });
    let panel = Arc::new(Panel::default());
    let sink = SpeechSink::new(rec, Arc::new(Tr), panel.clone(), Arc::new(Clk), Arc::new(Tap), Arc::new(NoStop), Box::new(|| HotkeyChoice::Fn));
    sink.on_event(Armed);
    sink.on_event(Started(PushToTalk));
    let start = Instant::now();
    while !panel.shown.lock().unwrap().iter().any(|(t, k)| t == MIC_FAILED_TEXT && *k == Kind::Warn) {
        assert!(start.elapsed() < Duration::from_secs(5), "no mic warning");
        std::thread::sleep(Duration::from_millis(20));
    }
    // A later event is still processed: the worker is not stuck.
    sink.on_event(TalkEvent::Stopped(PushToTalk));
    sink.on_event(Armed);
    sink.on_event(Disarmed);
    let start = Instant::now();
    while log.0.lock().unwrap().iter().filter(|s| *s == "cancel").count() < 2 {
        assert!(start.elapsed() < Duration::from_secs(5), "worker stuck: {:?}", log.0.lock().unwrap());
        std::thread::sleep(Duration::from_millis(20));
    }
    drop(hold);
}

// ---------------- S2 ----------------
struct Rec(Mutex<Vec<TalkEvent>>);
impl TalkSink for Rec {
    fn on_event(&self, e: TalkEvent) {
        self.0.lock().unwrap().push(e);
    }
}
struct Fake(Mutex<u64>);
impl Clock for Fake {
    fn now(&self) -> Instant {
        static BASE: std::sync::OnceLock<Instant> = std::sync::OnceLock::new();
        *BASE.get_or_init(Instant::now) + Duration::from_millis(*self.0.lock().unwrap())
    }
}

fn watcher_ends(end: Msg) -> Vec<TalkEvent> {
    let (tx, rx): (Sender<Msg>, _) = channel();
    let sink = Arc::new(Rec(Mutex::new(vec![])));
    let clock = Arc::new(Fake(Mutex::new(0)));
    let (s, c, p) = (sink.clone(), clock.clone(), Arc::new(Panel::default()));
    let h = std::thread::spawn(move || run(rx, HotkeyChoice::Fn, s, p, c));
    tx.send(Msg::Key(modev(FN, true, FN))).unwrap();
    std::thread::sleep(Duration::from_millis(50));
    *clock.0.lock().unwrap() = 400;
    std::thread::sleep(Duration::from_millis(100)); // the 20 ms tick sees the hold
    tx.send(end).unwrap();
    h.join().unwrap();
    let v = sink.0.lock().unwrap().clone();
    v
}

#[test]
fn s2_source_failure_or_shutdown_cancels_the_running_talk_in_the_sink() {
    for end in [Msg::SourceFailed, Msg::Shutdown] {
        assert_eq!(watcher_ends(end), vec![Armed, Started(PushToTalk), Cancelled(PushToTalk)]);
    }
}

#[test]
fn s2_engine_reset_while_armed_disarms() {
    let mut e = Engine::new(HotkeyChoice::Fn);
    assert!(e.reset().is_none());
    let t = Instant::now();
    assert_eq!(e.feed(&modev(FN, true, FN), t), vec![Armed]);
    assert_eq!(e.reset(), Some(Disarmed));
}

// ---------------- S3 ----------------
#[test]
fn s3_second_press_with_another_modifier_held_announces_the_forgotten_tap() {
    let t0 = Instant::now();
    let ms = |n: u64| t0 + Duration::from_millis(n);
    let mut d = Detector::new(FN);
    d.feed(&modev(FN, true, FN), ms(0));
    d.feed(&modev(NONE, false, FN), ms(80));
    let out = d.feed(&modev(FN | Modifiers::SHIFT_LEFT, true, FN), ms(200));
    assert_eq!(out, vec![DetOut::TapExpired]);
    assert!(!d.needs_tick());
}

#[test]
fn s3_target_seen_down_without_its_key_down_announces_the_forgotten_tap() {
    let t0 = Instant::now();
    let ms = |n: u64| t0 + Duration::from_millis(n);
    let mut d = Detector::new(FN);
    d.feed(&modev(FN, true, FN), ms(0));
    d.feed(&modev(NONE, false, FN), ms(80));
    let out = d.feed(&modev(FN | Modifiers::SHIFT_LEFT, true, Modifiers::SHIFT_LEFT), ms(200));
    assert_eq!(out, vec![DetOut::TapExpired]);
}

#[test]
fn s3_engine_goes_back_to_disarmed_when_the_tap_is_forgotten() {
    let mut e = Engine::new(HotkeyChoice::Fn);
    let t0 = Instant::now();
    let ms = |n: u64| t0 + Duration::from_millis(n);
    assert_eq!(e.feed(&modev(FN, true, FN), ms(0)), vec![Armed]);
    assert!(e.feed(&modev(NONE, false, FN), ms(80)).is_empty());
    assert_eq!(e.feed(&modev(FN | Modifiers::SHIFT_LEFT, true, FN), ms(200)), vec![Disarmed]);
}

// ---------------- source-failure warning wins over a stale talk hide ----------------
#[test]
fn a_source_failure_warning_is_not_cleared_by_the_cancelled_talks_hide() {
    use crate::so::hotkey::watcher::SOURCE_FAILED_TEXT;
    let log = Arc::new(Log(Mutex::new(vec![])));
    let rec = Arc::new(NeverReadyRec { log, hold: Mutex::new(None) });
    let panel = Arc::new(Panel::default());
    let clock = Arc::new(Fake(Mutex::new(0)));
    let sink: Arc<dyn TalkSink> = Arc::new(SpeechSink::new(rec, Arc::new(Tr), panel.clone(), clock.clone(), Arc::new(Tap), Arc::new(NoStop), Box::new(|| HotkeyChoice::Fn)));
    let (tx, rx): (Sender<Msg>, _) = channel();
    let (s, c, p) = (sink, clock.clone(), panel.clone());
    let h = std::thread::spawn(move || run(rx, HotkeyChoice::Fn, s, p, c));
    tx.send(Msg::Key(modev(FN, true, FN))).unwrap();
    std::thread::sleep(Duration::from_millis(50));
    *clock.0.lock().unwrap() = 400;
    std::thread::sleep(Duration::from_millis(500)); // Started shown (the fake mic gives up after 200 ms)
    tx.send(Msg::SourceFailed).unwrap();
    h.join().unwrap();
    std::thread::sleep(Duration::from_millis(500)); // the sink's Cancelled hide arrives late
    let order = panel.order.lock().unwrap().clone();
    assert_eq!(order.last().map(String::as_str), Some(format!("show:{SOURCE_FAILED_TEXT}").as_str()), "{order:?}");
    assert!(!order.iter().skip_while(|o| !o.starts_with("show:Key watcher")).any(|o| o == "hide"), "{order:?}");
}
