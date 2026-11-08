// SPDX-License-Identifier: FSL-1.1-MIT
//! Forwarder thread + merged channel + watcher loop. Tests: `so/tests/watcher_tests.rs` (T16, T27, T31, T32).
//!
//! Contract:
//! - `run` blocks on `rx.recv()` while the engine does not need a tick (Idle / free-talk) and reads the
//!   clock ONLY when a message arrives or after a `recv_timeout(20 ms)` while a press is pending.
//!   (Tests prove "no timeout while Idle" by counting `Clock::now` calls.)
//! - `Msg::SetTarget` -> `Engine::set_target`; a resulting `Cancelled` goes to the sink at once.
//! - `Msg::SourceFailed` -> `panel.show` with one Warn line `SOURCE_FAILED_TEXT`; no panic; `run` returns (the Starter then allows a retry).
//! - `Msg::Shutdown` (or a closed channel) -> `run` returns.
//! - `spawn_forwarder`: loops `source.recv()`; Ok -> send `Msg::Key`; Err -> send `Msg::SourceFailed` and exit;
//!   if the send fails (channel closed) it exits.

use super::super::panel::{Kind, Line, PanelOut, PanelPayload};
use super::super::settings::HotkeyChoice;
use super::session::TalkEvent;
use super::Engine;
use handy_keys::KeyEvent;
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

/// How often the loop wakes while a press waits for the 300 ms hold timer.
const TICK: Duration = Duration::from_millis(20);

pub const SOURCE_FAILED_TEXT: &str = "Key watcher stopped: check Accessibility/Input Monitoring";

pub enum Msg {
    Key(KeyEvent),
    SetTarget(HotkeyChoice),
    Shutdown,
    SourceFailed,
    /// End talk number `seq` (see `Engine::force_stop`). Sent by the talk timer.
    ForceStop(u64),
}

#[derive(Debug)]
pub struct SourceError;

/// Real = handy-keys listener; fake = scripted events. Only the forwarder calls it.
pub trait KeySource: Send + Sync {
    fn recv(&self) -> Result<KeyEvent, SourceError>;
}

/// Step 1 = PanelSink; step 2 = SpeechSink (speech-to-text).
pub trait TalkSink: Send + Sync {
    fn on_event(&self, event: TalkEvent);
}

/// Injected time.
pub trait Clock: Send + Sync {
    fn now(&self) -> Instant;
}

/// The watcher loop. Blocks on `recv()` while no press waits for the hold timer; uses a 20 ms
/// timeout only while one does. So an idle app does not wake up at all.
pub fn run(
    rx: Receiver<Msg>,
    initial: HotkeyChoice,
    sink: Arc<dyn TalkSink>,
    panel: Arc<dyn PanelOut>,
    clock: Arc<dyn Clock>,
) {
    let mut engine = Engine::new(initial);
    loop {
        let msg = if engine.needs_tick() {
            match rx.recv_timeout(TICK) {
                Ok(m) => Some(m),
                Err(RecvTimeoutError::Timeout) => None,
                Err(RecvTimeoutError::Disconnected) => return,
            }
        } else {
            match rx.recv() {
                Ok(m) => Some(m),
                Err(_) => return,
            }
        };
        let events = match msg {
            Some(Msg::Key(ev)) => engine.feed(&ev, clock.now()),
            Some(Msg::SetTarget(choice)) => engine.set_target(choice).into_iter().collect(),
            Some(Msg::SourceFailed) => {
                panel.show(PanelPayload::new(vec![Line::new(
                    SOURCE_FAILED_TEXT,
                    Kind::Warn,
                )]));
                // The key source is gone: nothing more can arrive. Returning lets `start` retry.
                return;
            }
            Some(Msg::ForceStop(seq)) => engine.force_stop(seq),
            Some(Msg::Shutdown) => return,
            None => engine.tick(clock.now()),
        };
        for e in events {
            sink.on_event(e);
        }
    }
}

/// Reads the key source on its own thread and pushes each event into the channel. It exits when
/// the source fails (after sending `SourceFailed`) or when the channel is closed (the next send fails).
/// Returns an error if the OS cannot start a thread.
pub fn try_spawn_forwarder(
    source: Box<dyn KeySource>,
    tx: Sender<Msg>,
) -> std::io::Result<JoinHandle<()>> {
    std::thread::Builder::new()
        .name("so-key-forwarder".into())
        .spawn(move || loop {
            match source.recv() {
                Ok(ev) => {
                    if tx.send(Msg::Key(ev)).is_err() {
                        return;
                    }
                }
                Err(_) => {
                    let _ = tx.send(Msg::SourceFailed);
                    return;
                }
            }
        })
}

/// Same, for tests (a thread that cannot start is a test failure).
#[cfg(test)]
pub fn spawn_forwarder(source: Box<dyn KeySource>, tx: Sender<Msg>) -> JoinHandle<()> {
    try_spawn_forwarder(source, tx).expect("spawn key forwarder thread")
}
