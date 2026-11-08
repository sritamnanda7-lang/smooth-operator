// SPDX-License-Identifier: FSL-1.1-MIT
//! Idle / PushToTalk / FreeTalk state machine. STUB. Tests: `so/tests/hotkey_tests.rs`.
//!
//! Contract (from the plan's Behaviour spec):
//! - Idle: `Hold` -> Started(PushToTalk). `DoubleTap` -> Started(FreeTalk). Everything else: nothing.
//! - PushToTalk: `Release{clean:true}` -> Stopped; `Dirty` or `Release{clean:false}` -> Cancelled (then Idle).
//! - FreeTalk: the press that started it: `Dirty` before its release -> Cancelled(FreeTalk); its clean
//!   release -> nothing. Later: a new press (`Down`) ends the talk on its clean `Release` -> Stopped(FreeTalk);
//!   `Hold` during FreeTalk never starts push-to-talk; a `Dirty` press does not end it.
//! - `reset()` (hotkey changed): active talk -> Cancelled(mode); Idle -> None. Always ends in Idle.

use super::detector::DetOut;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    PushToTalk,
    FreeTalk,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TalkEvent {
    Started(Mode),
    Stopped(Mode),
    Cancelled(Mode),
}

#[derive(Default)]
pub struct Session {}

impl Session {
    pub fn new() -> Self {
        Session {}
    }

    pub fn on(&mut self, _out: DetOut) -> Option<TalkEvent> {
        todo!("step 1: session transitions")
    }

    pub fn reset(&mut self) -> Option<TalkEvent> {
        todo!("step 1: session reset")
    }
}
