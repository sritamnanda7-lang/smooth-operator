// SPDX-License-Identifier: FSL-1.1-MIT
//! T16, T27, T31, T32: watcher loop, forwarder and start, with fake source / sink / panel / clock.
use crate::so::hotkey::session::Mode::{FreeTalk, PushToTalk};
use crate::so::hotkey::session::TalkEvent::{self, Cancelled, Started, Stopped};
use crate::so::hotkey::watcher::{run, spawn_forwarder, Clock, KeySource, Msg, SourceError, TalkSink, SOURCE_FAILED_TEXT};
use crate::so::hotkey::{start_with, Starter};
use crate::so::panel::{Kind, PanelOut, PanelPayload};
use crate::so::settings::HotkeyChoice;
use handy_keys::{KeyEvent, Modifiers};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

const FN: Modifiers = Modifiers::FN;

struct FakeClock {
    base: Instant,
    ms: Mutex<u64>,
    reads: AtomicUsize,
}
impl FakeClock {
    fn new() -> Arc<Self> {
        Arc::new(FakeClock { base: Instant::now(), ms: Mutex::new(0), reads: AtomicUsize::new(0) })
    }
    fn set(&self, ms: u64) {
        *self.ms.lock().unwrap() = ms;
    }
    fn reads(&self) -> usize {
        self.reads.load(Ordering::SeqCst)
    }
}
impl Clock for FakeClock {
    fn now(&self) -> Instant {
        self.reads.fetch_add(1, Ordering::SeqCst);
        self.base + Duration::from_millis(*self.ms.lock().unwrap())
    }
}

#[derive(Default)]
struct FakeSink(Mutex<Vec<TalkEvent>>);
impl TalkSink for FakeSink {
    fn on_event(&self, e: TalkEvent) {
        // Step 2 added Armed/Disarmed; these step-1 tests are about talks only.
        if matches!(e, TalkEvent::Armed | TalkEvent::Disarmed) {
            return;
        }
        self.0.lock().unwrap().push(e);
    }
}
impl FakeSink {
    fn events(&self) -> Vec<TalkEvent> {
        self.0.lock().unwrap().clone()
    }
    fn wait_for(&self, n: usize, max: Duration) -> Vec<TalkEvent> {
        let start = Instant::now();
        while start.elapsed() < max {
            if self.events().len() >= n {
                break;
            }
            thread::sleep(Duration::from_millis(5));
        }
        self.events()
    }
}

#[derive(Default)]
struct FakePanel {
    shows: Mutex<Vec<PanelPayload>>,
    hides: AtomicUsize,
}
impl PanelOut for FakePanel {
    fn show(&self, p: PanelPayload) {
        self.shows.lock().unwrap().push(p);
    }
    fn hide(&self) {
        self.hides.fetch_add(1, Ordering::SeqCst);
    }
}

struct Rig {
    tx: Sender<Msg>,
    clock: Arc<FakeClock>,
    sink: Arc<FakeSink>,
    panel: Arc<FakePanel>,
    done: Receiver<()>,
    handle: JoinHandle<()>,
}
impl Rig {
    fn new(initial: HotkeyChoice) -> Self {
        let (tx, rx) = channel();
        let clock = FakeClock::new();
        let sink = Arc::new(FakeSink::default());
        let panel = Arc::new(FakePanel::default());
        let (dtx, done) = channel();
        let (c, s, p) = (clock.clone(), sink.clone(), panel.clone());
        let handle = thread::spawn(move || {
            run(rx, initial, s, p, c);
            let _ = dtx.send(());
        });
        Rig { tx, clock, sink, panel, done, handle }
    }
    /// Move fake time, send, then let the watcher thread read the clock before time moves again.
    fn key(&self, ms: u64, ev: KeyEvent) {
        self.clock.set(ms);
        self.tx.send(Msg::Key(ev)).unwrap();
        thread::sleep(Duration::from_millis(50));
    }
    fn down(&self, ms: u64) {
        self.key(ms, KeyEvent { modifiers: FN, key: None, is_key_down: true, changed_modifier: Some(FN) });
    }
    fn up(&self, ms: u64) {
        self.key(ms, KeyEvent { modifiers: Modifiers::empty(), key: None, is_key_down: false, changed_modifier: Some(FN) });
    }
    /// Move fake time with no message; only a timeout can notice.
    fn time_passes(&self, ms: u64) {
        self.clock.set(ms);
    }
    fn finish(self) {
        let _ = self.tx.send(Msg::Shutdown);
        assert!(self.done.recv_timeout(Duration::from_secs(2)).is_ok(), "run returns on Shutdown");
        assert!(self.handle.join().is_ok(), "watcher did not panic");
    }
}

// ---------- T27 ----------
#[test]
fn t27_scripted_hold_gives_started_then_stopped() {
    let r = Rig::new(HotkeyChoice::Fn);
    r.down(1000);
    assert!(r.sink.events().is_empty());
    r.time_passes(1310); // the 20 ms recv_timeout notices the 300 ms hold
    assert_eq!(r.sink.wait_for(1, Duration::from_secs(1)), vec![Started(PushToTalk)]);
    r.up(1800);
    assert_eq!(r.sink.wait_for(2, Duration::from_secs(1)), vec![Started(PushToTalk), Stopped(PushToTalk)]);
    r.finish();
}

#[test]
fn t27_scripted_double_tap_then_ending_tap() {
    let r = Rig::new(HotkeyChoice::Fn);
    r.down(0);
    r.up(80);
    r.down(250);
    assert_eq!(r.sink.wait_for(1, Duration::from_secs(1)), vec![Started(FreeTalk)]);
    r.up(300);
    r.down(2000);
    assert_eq!(r.sink.events().len(), 1, "not stopped on down");
    r.up(2080);
    assert_eq!(r.sink.wait_for(2, Duration::from_secs(1)), vec![Started(FreeTalk), Stopped(FreeTalk)]);
    // T7 through the real loop: tap right after the ending tap does nothing.
    r.down(2200);
    r.up(2260);
    r.time_passes(4000);
    thread::sleep(Duration::from_millis(100));
    assert_eq!(r.sink.events().len(), 2);
    r.finish();
}

#[test]
fn t27_while_idle_the_loop_blocks_with_no_timeout() {
    let r = Rig::new(HotkeyChoice::Fn);
    thread::sleep(Duration::from_millis(300));
    assert_eq!(r.clock.reads(), 0, "idle watcher must not wake up or read the clock");
    // Also in free-talk (no timer needed): settle, then no further clock reads.
    r.down(0);
    r.up(80);
    r.down(250);
    r.up(300);
    r.sink.wait_for(1, Duration::from_secs(1));
    thread::sleep(Duration::from_millis(100));
    let before = r.clock.reads();
    thread::sleep(Duration::from_millis(300));
    assert_eq!(r.clock.reads(), before, "free-talk must block with no timeout");
    r.finish();
}

// ---------- T31 / T32 ----------
#[test]
fn t31_set_target_in_free_talk_with_no_key_events_cancels_at_once() {
    let r = Rig::new(HotkeyChoice::Fn);
    r.down(0);
    r.up(80);
    r.down(250);
    r.up(300);
    assert_eq!(r.sink.wait_for(1, Duration::from_secs(1)), vec![Started(FreeTalk)]);
    let t = Instant::now();
    r.tx.send(Msg::SetTarget(HotkeyChoice::RightCmd)).unwrap();
    let ev = r.sink.wait_for(2, Duration::from_secs(1));
    assert!(t.elapsed() < Duration::from_millis(100), "took {:?}", t.elapsed());
    assert_eq!(ev, vec![Started(FreeTalk), Cancelled(FreeTalk)]);
    r.finish();
}

#[test]
fn t32_shutdown_while_idle_returns_within_100ms() {
    let r = Rig::new(HotkeyChoice::Fn);
    thread::sleep(Duration::from_millis(50));
    let t = Instant::now();
    r.tx.send(Msg::Shutdown).unwrap();
    assert!(r.done.recv_timeout(Duration::from_millis(100)).is_ok(), "took {:?}", t.elapsed());
    assert!(r.handle.join().is_ok());
}

struct ChanSource(Mutex<Receiver<Result<KeyEvent, ()>>>);
impl KeySource for ChanSource {
    fn recv(&self) -> Result<KeyEvent, SourceError> {
        match self.0.lock().unwrap().recv() {
            Ok(Ok(e)) => Ok(e),
            _ => Err(SourceError),
        }
    }
}
fn fn_down() -> KeyEvent {
    KeyEvent { modifiers: FN, key: None, is_key_down: true, changed_modifier: Some(FN) }
}

#[test]
fn t32_forwarder_exits_when_the_channel_is_closed_at_its_next_event() {
    let (src_tx, src_rx) = channel();
    let (tx, rx) = channel();
    let fwd = spawn_forwarder(Box::new(ChanSource(Mutex::new(src_rx))), tx.clone());
    let clock = FakeClock::new();
    let sink = Arc::new(FakeSink::default());
    let panel = Arc::new(FakePanel::default());
    let (dtx, done) = channel();
    let (c, s, p) = (clock.clone(), sink.clone(), panel.clone());
    let h = thread::spawn(move || {
        run(rx, HotkeyChoice::Fn, s, p, c);
        let _ = dtx.send(());
    });
    tx.send(Msg::Shutdown).unwrap();
    drop(tx);
    assert!(done.recv_timeout(Duration::from_millis(100)).is_ok());
    h.join().unwrap();
    // The forwarder is still blocked in recv(); the next key makes its send fail and it exits.
    assert!(!fwd.is_finished());
    src_tx.send(Ok(fn_down())).unwrap();
    let start = Instant::now();
    while !fwd.is_finished() && start.elapsed() < Duration::from_secs(1) {
        thread::sleep(Duration::from_millis(5));
    }
    assert!(fwd.is_finished(), "forwarder must exit after its send fails");
    fwd.join().unwrap();
}

#[test]
fn t32_forwarder_passes_keys_into_the_channel() {
    let (src_tx, src_rx) = channel();
    let (tx, rx) = channel();
    let fwd = spawn_forwarder(Box::new(ChanSource(Mutex::new(src_rx))), tx);
    src_tx.send(Ok(fn_down())).unwrap();
    match rx.recv_timeout(Duration::from_secs(1)) {
        Ok(Msg::Key(e)) => assert!(e.is_key_down && e.modifiers.contains(FN)),
        _ => panic!("expected Msg::Key"),
    }
    drop(src_tx); // source now fails
    assert!(matches!(rx.recv_timeout(Duration::from_secs(1)), Ok(Msg::SourceFailed)));
    fwd.join().unwrap();
}

// ---------- T16 ----------
#[test]
fn t16_source_failed_shows_panel_warning_and_does_not_panic() {
    let r = Rig::new(HotkeyChoice::Fn);
    r.tx.send(Msg::SourceFailed).unwrap();
    let start = Instant::now();
    while r.panel.shows.lock().unwrap().is_empty() && start.elapsed() < Duration::from_secs(1) {
        thread::sleep(Duration::from_millis(5));
    }
    {
        let shows = r.panel.shows.lock().unwrap();
        assert_eq!(shows.len(), 1);
        assert_eq!(shows[0].lines.len(), 1);
        assert_eq!(shows[0].lines[0].kind, Kind::Warn);
        assert_eq!(shows[0].lines[0].text, "Key watcher stopped: check Accessibility/Input Monitoring");
        assert_eq!(shows[0].lines[0].text, SOURCE_FAILED_TEXT);
    }
    r.finish();
}

#[test]
fn t16_start_failure_shows_error_and_a_retry_works() {
    let starter = Starter::new();
    let panel = Arc::new(FakePanel::default());
    let sink = Arc::new(FakeSink::default());
    let clock = FakeClock::new();
    assert!(!starter.send(Msg::Shutdown), "not started: nothing to send to");

    let fail = || -> Result<Box<dyn KeySource>, SourceError> { Err(SourceError) };
    let r = start_with(&starter, HotkeyChoice::Fn, &fail, sink.clone(), panel.clone(), clock.clone());
    assert!(r.is_err());
    assert!(!starter.is_started(), "failed start must not count as started");
    {
        let shows = panel.shows.lock().unwrap();
        assert_eq!(shows.len(), 1, "error shown in the panel");
        assert_eq!(shows[0].lines.len(), 1);
        assert_eq!(shows[0].lines[0].kind, Kind::Warn);
        assert!(!shows[0].lines[0].text.trim().is_empty());
    }

    let calls = Arc::new(AtomicUsize::new(0));
    let (_keep_tx, src_rx) = channel::<Result<KeyEvent, ()>>();
    let src_rx = Arc::new(Mutex::new(Some(src_rx)));
    let c2 = calls.clone();
    let ok = move || -> Result<Box<dyn KeySource>, SourceError> {
        c2.fetch_add(1, Ordering::SeqCst);
        let rx = src_rx.lock().unwrap().take().expect("source made once");
        Ok(Box::new(ChanSource(Mutex::new(rx))))
    };
    assert!(start_with(&starter, HotkeyChoice::Fn, &ok, sink.clone(), panel.clone(), clock.clone()).is_ok());
    assert!(starter.is_started());
    assert_eq!(calls.load(Ordering::SeqCst), 1);

    // Idempotent: a second start does not build another source.
    assert!(start_with(&starter, HotkeyChoice::Fn, &ok, sink, panel, clock).is_ok());
    assert_eq!(calls.load(Ordering::SeqCst), 1);

    assert!(starter.send(Msg::SetTarget(HotkeyChoice::RightCmd)));
    assert!(starter.send(Msg::Shutdown));
}

#[test]
fn t16c_start_can_retry_after_the_key_source_failed() {
    let starter = Starter::new();
    let panel = Arc::new(FakePanel::default());
    let sink = Arc::new(FakeSink::default());
    let clock = FakeClock::new();
    // First source fails on its first read.
    let (src_tx, src_rx) = channel::<Result<KeyEvent, ()>>();
    let src_rx = Arc::new(Mutex::new(Some(src_rx)));
    let first = move || -> Result<Box<dyn KeySource>, SourceError> {
        let rx = src_rx.lock().unwrap().take().expect("made once");
        Ok(Box::new(ChanSource(Mutex::new(rx))))
    };
    assert!(start_with(&starter, HotkeyChoice::Fn, &first, sink.clone(), panel.clone(), clock.clone()).is_ok());
    assert!(starter.is_started());
    drop(src_tx); // source fails
    let start = Instant::now();
    while starter.is_started() && start.elapsed() < Duration::from_secs(1) {
        thread::sleep(Duration::from_millis(5));
    }
    assert!(!starter.is_started(), "an ended watcher must not count as started");
    assert!(!starter.send(Msg::Shutdown));
    // Retry builds a new source and works.
    let calls = Arc::new(AtomicUsize::new(0));
    let c2 = calls.clone();
    let (_keep, rx2) = channel::<Result<KeyEvent, ()>>();
    let rx2 = Arc::new(Mutex::new(Some(rx2)));
    let second = move || -> Result<Box<dyn KeySource>, SourceError> {
        c2.fetch_add(1, Ordering::SeqCst);
        Ok(Box::new(ChanSource(Mutex::new(rx2.lock().unwrap().take().unwrap()))))
    };
    assert!(start_with(&starter, HotkeyChoice::Fn, &second, sink, panel, clock).is_ok());
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert!(starter.is_started());
    assert!(starter.send(Msg::Shutdown));
}
