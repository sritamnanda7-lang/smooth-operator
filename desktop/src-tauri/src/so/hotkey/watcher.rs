// SPDX-License-Identifier: FSL-1.1-MIT
//! Forwarder thread + merged channel + watcher loop. STUB. Tests: `so/tests/watcher_tests.rs` (T16, T27, T31, T32).
//!
//! Contract:
//! - `run` blocks on `rx.recv()` while the engine does not need a tick (Idle / free-talk) and reads the
//!   clock ONLY when a message arrives or after a `recv_timeout(20 ms)` while a press is pending.
//!   (Tests prove "no timeout while Idle" by counting `Clock::now` calls.)
//! - `Msg::SetTarget` -> `Engine::set_target`; a resulting `Cancelled` goes to the sink at once.
//! - `Msg::SourceFailed` -> `panel.show` with one Warn line `SOURCE_FAILED_TEXT`; no panic; loop keeps going.
//! - `Msg::Shutdown` (or a closed channel) -> `run` returns.
//! - `spawn_forwarder`: loops `source.recv()`; Ok -> send `Msg::Key`; Err -> send `Msg::SourceFailed` and exit;
//!   if the send fails (channel closed) it exits.

use super::super::panel::PanelOut;
use super::super::settings::HotkeyChoice;
use super::session::TalkEvent;
use handy_keys::KeyEvent;
use std::sync::mpsc::{Receiver, Sender};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::Instant;

pub const SOURCE_FAILED_TEXT: &str = "Key watcher stopped: check Accessibility/Input Monitoring";

pub enum Msg {
    Key(KeyEvent),
    SetTarget(HotkeyChoice),
    Shutdown,
    SourceFailed,
}

#[derive(Debug)]
pub struct SourceError;

/// Real = handy-keys listener; fake = scripted events. Only the forwarder calls it.
pub trait KeySource: Send + Sync {
    fn recv(&self) -> Result<KeyEvent, SourceError>;
}

/// Step 1 = PanelSink; step 2 plugs speech-to-text here.
pub trait TalkSink: Send + Sync {
    fn on_event(&self, event: TalkEvent);
}

/// Injected time.
pub trait Clock: Send + Sync {
    fn now(&self) -> Instant;
}

pub fn run(
    _rx: Receiver<Msg>,
    _initial: HotkeyChoice,
    _sink: Arc<dyn TalkSink>,
    _panel: Arc<dyn PanelOut>,
    _clock: Arc<dyn Clock>,
) {
    todo!("step 1: watcher loop")
}

pub fn spawn_forwarder(_source: Box<dyn KeySource>, _tx: Sender<Msg>) -> JoinHandle<()> {
    todo!("step 1: forwarder thread")
}
