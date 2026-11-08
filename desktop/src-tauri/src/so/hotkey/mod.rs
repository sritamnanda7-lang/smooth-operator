// SPDX-License-Identifier: FSL-1.1-MIT
//! Hotkey: detector -> session -> watcher. Tests: `so/tests/hotkey_tests.rs`, `watcher_tests.rs`.
//! This module reads modifier flags and key codes only. It never logs, stores or sends key values;
//! only `TalkEvent`s leave it.

pub mod detector;
pub mod session;
pub mod watcher;

use super::panel::{Kind, Line, PanelOut, PanelPayload};
use super::settings::{self, HotkeyChoice, Platform};
use detector::Detector;
use handy_keys::{KeyEvent, KeyboardListener};
use once_cell::sync::Lazy;
use session::{Mode, Session, TalkEvent};
use std::collections::HashSet;
use std::sync::mpsc::{channel, Sender};
use std::sync::{Arc, Mutex};
use std::time::Instant;
use tauri::AppHandle;
use watcher::{Clock, KeySource, Msg, SourceError, TalkSink};

/// Pure glue: Detector + Session. No threads, no clock reads. This is what `watcher::run` drives.
/// After a `Stopped(FreeTalk)` the engine calls `Detector::forget_tap` so the ending tap
/// cannot become the first tap of a new double-tap (T7).
pub struct Engine {
    detector: Detector,
    session: Session,
}

impl Engine {
    pub fn new(target: HotkeyChoice) -> Self {
        Engine {
            detector: Detector::new(target.modifier()),
            session: Session::new(),
        }
    }

    fn run_outs(&mut self, outs: Vec<detector::DetOut>) -> Vec<TalkEvent> {
        let mut events = Vec::new();
        for o in outs {
            if let Some(e) = self.session.on(o) {
                events.push(e);
            }
        }
        if events.contains(&TalkEvent::Stopped(Mode::FreeTalk)) {
            self.detector.forget_tap();
        }
        events
    }

    pub fn feed(&mut self, ev: &KeyEvent, now: Instant) -> Vec<TalkEvent> {
        let outs = self.detector.feed(ev, now);
        self.run_outs(outs)
    }

    pub fn tick(&mut self, now: Instant) -> Vec<TalkEvent> {
        let outs = self.detector.tick(now);
        self.run_outs(outs)
    }

    pub fn needs_tick(&self) -> bool {
        self.detector.needs_tick()
    }

    /// New hotkey: fresh detector, session reset. Returns Cancelled if a talk was active.
    pub fn set_target(&mut self, target: HotkeyChoice) -> Option<TalkEvent> {
        self.detector = Detector::new(target.modifier());
        self.session.reset()
    }
}

/// Remembers whether `start` already succeeded (idempotent start) and holds the channel sender.
pub struct Starter {
    tx: Mutex<Option<Sender<Msg>>>,
}

impl Default for Starter {
    fn default() -> Self {
        Self::new()
    }
}

impl Starter {
    pub fn new() -> Self {
        Starter {
            tx: Mutex::new(None),
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Option<Sender<Msg>>> {
        // A poisoned lock (another thread panicked) still holds valid data here.
        self.tx.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// True only after a successful `start_with`.
    pub fn is_started(&self) -> bool {
        self.lock().is_some()
    }

    /// Push a message into the watcher channel. False if not started.
    pub fn send(&self, msg: Msg) -> bool {
        match self.lock().as_ref() {
            Some(tx) => tx.send(msg).is_ok(),
            None => false,
        }
    }
}

pub const START_FAILED_TEXT: &str =
    "Key watcher could not start: check Accessibility/Input Monitoring";

/// Idempotent start. If already started: Ok, `make_source` is NOT called. Otherwise calls `make_source`;
/// on Err: shows ONE Warn line in the panel (non-empty text), stays not-started (so a retry works), returns Err.
/// On Ok: spawns forwarder + watcher thread, marks started.
pub fn start_with(
    starter: &Starter,
    initial: HotkeyChoice,
    make_source: &dyn Fn() -> Result<Box<dyn KeySource>, SourceError>,
    sink: Arc<dyn TalkSink>,
    panel: Arc<dyn PanelOut>,
    clock: Arc<dyn Clock>,
) -> Result<(), SourceError> {
    // The lock is held while the source is made, so two callers cannot both start.
    let mut guard = starter.lock();
    if guard.is_some() {
        return Ok(());
    }
    let source = match make_source() {
        Ok(s) => s,
        Err(e) => {
            panel.show(PanelPayload::new(vec![Line::new(
                START_FAILED_TEXT,
                Kind::Warn,
            )]));
            return Err(e);
        }
    };
    let (tx, rx) = channel();
    watcher::spawn_forwarder(source, tx.clone());
    let panel_for_run = panel.clone();
    let spawned = std::thread::Builder::new()
        .name("so-hotkey-watcher".into())
        .spawn(move || watcher::run(rx, initial, sink, panel_for_run, clock));
    if spawned.is_err() {
        panel.show(PanelPayload::new(vec![Line::new(
            START_FAILED_TEXT,
            Kind::Warn,
        )]));
        return Err(SourceError);
    }
    *guard = Some(tx);
    Ok(())
}

/// The real key source: the handy-keys listener (a global keyboard hook). It is created with an
/// EMPTY blocking set, so nothing is swallowed yet; step 7 only has to fill that set.
struct ListenerSource(Mutex<KeyboardListener>);

impl KeySource for ListenerSource {
    fn recv(&self) -> Result<KeyEvent, SourceError> {
        self.0
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .recv()
            .map_err(|_| SourceError)
    }
}

struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> Instant {
        Instant::now()
    }
}

static STARTER: Lazy<Starter> = Lazy::new(Starter::new);

/// Start the key watcher (once). Called from `initialize_shortcuts`, the same moment Handy's own
/// shortcuts start, so the permission timing is the same. A failure is shown in the panel and a
/// later call retries.
pub fn start(app: &AppHandle, panel: Arc<dyn PanelOut>) {
    if STARTER.is_started() {
        return;
    }
    let initial = settings::load_hotkey(app, Platform::current());
    let app_for_sink = app.clone();
    let sink: Arc<dyn TalkSink> = Arc::new(super::panel::PanelSink::new(
        panel.clone(),
        Box::new(move || settings::load_hotkey(&app_for_sink, Platform::current())),
    ));
    let make = || -> Result<Box<dyn KeySource>, SourceError> {
        let blocking = Arc::new(Mutex::new(HashSet::new()));
        let listener = KeyboardListener::new_with_blocking(blocking).map_err(|_| SourceError)?;
        Ok(Box::new(ListenerSource(Mutex::new(listener))))
    };
    let _ = start_with(&STARTER, initial, &make, sink, panel, Arc::new(SystemClock));
}

/// Tell the running watcher about a new hotkey. False if it is not running (it will read the
/// stored choice when it starts).
pub fn set_target(choice: HotkeyChoice) -> bool {
    STARTER.send(Msg::SetTarget(choice))
}

/// Ask the watcher to stop (the app is quitting).
pub fn shutdown() {
    let _ = STARTER.send(Msg::Shutdown);
}
