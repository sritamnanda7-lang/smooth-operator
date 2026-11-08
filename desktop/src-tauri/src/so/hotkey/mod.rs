// SPDX-License-Identifier: FSL-1.1-MIT
//! Hotkey: detector -> session -> watcher. STUB. Tests: `so/tests/hotkey_tests.rs`, `watcher_tests.rs`.

pub mod detector;
pub mod session;
pub mod watcher;

use super::panel::PanelOut;
use super::settings::HotkeyChoice;
use detector::Detector;
use handy_keys::KeyEvent;
use session::{Session, TalkEvent};
use std::sync::Arc;
use std::time::Instant;
use watcher::{Clock, KeySource, Msg, SourceError, TalkSink};

/// Pure glue: Detector + Session. No threads, no clock reads. This is what `watcher::run` drives.
/// Contract: after a `Stopped(FreeTalk)` the engine calls `Detector::forget_tap` so the ending tap
/// cannot become the first tap of a new double-tap (T7).
pub struct Engine {
    _detector: Detector,
    _session: Session,
}

impl Engine {
    pub fn new(_target: HotkeyChoice) -> Self {
        todo!("step 1: engine new")
    }
    pub fn feed(&mut self, _ev: &KeyEvent, _now: Instant) -> Vec<TalkEvent> {
        todo!("step 1: engine feed")
    }
    pub fn tick(&mut self, _now: Instant) -> Vec<TalkEvent> {
        todo!("step 1: engine tick")
    }
    pub fn needs_tick(&self) -> bool {
        todo!("step 1: engine needs_tick")
    }
    /// New hotkey: fresh detector, session reset. Returns Cancelled if a talk was active.
    pub fn set_target(&mut self, _target: HotkeyChoice) -> Option<TalkEvent> {
        todo!("step 1: engine set_target")
    }
}

/// Remembers whether `start` already succeeded (idempotent start) and holds the channel sender.
pub struct Starter {}

impl Default for Starter {
    fn default() -> Self {
        Self::new()
    }
}

impl Starter {
    pub fn new() -> Self {
        Starter {}
    }
    /// True only after a successful `start_with`.
    pub fn is_started(&self) -> bool {
        todo!("step 1: starter is_started")
    }
    /// Push a message into the watcher channel. False if not started.
    pub fn send(&self, _msg: Msg) -> bool {
        todo!("step 1: starter send")
    }
}

/// Idempotent start. If already started: Ok, `make_source` is NOT called. Otherwise calls `make_source`;
/// on Err: shows ONE Warn line in the panel (non-empty text), stays not-started (so a retry works), returns Err.
/// On Ok: spawns forwarder + watcher thread, marks started.
pub fn start_with(
    _starter: &Starter,
    _initial: HotkeyChoice,
    _make_source: &dyn Fn() -> Result<Box<dyn KeySource>, SourceError>,
    _sink: Arc<dyn TalkSink>,
    _panel: Arc<dyn PanelOut>,
    _clock: Arc<dyn Clock>,
) -> Result<(), SourceError> {
    todo!("step 1: start_with")
}
