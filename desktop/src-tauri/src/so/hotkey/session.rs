// SPDX-License-Identifier: FSL-1.1-MIT
//! Idle / PushToTalk / FreeTalk state machine. Tests: `so/tests/hotkey_tests.rs`.
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

/// Free-talk is "listening until a hotkey press ends it". These are the steps inside it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Free {
    /// The 2nd press of the double-tap that started it is still down.
    Starting,
    /// Running, no press in progress.
    Running,
    /// A press is in progress; `dirty` = another key joined it (then it is a chord, not an end).
    Ending { dirty: bool },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum State {
    Idle,
    PushToTalk,
    FreeTalk(Free),
}

/// The talk state machine: Idle / PushToTalk / FreeTalk. `on` takes what the detector saw and
/// returns at most one event for the rest of the app.
pub struct Session {
    state: State,
}

impl Default for Session {
    fn default() -> Self {
        Self::new()
    }
}

impl Session {
    pub fn new() -> Self {
        Session { state: State::Idle }
    }

    pub fn on(&mut self, out: DetOut) -> Option<TalkEvent> {
        use Mode::{FreeTalk, PushToTalk};
        match (self.state, out) {
            (State::Idle, DetOut::Hold) => {
                self.state = State::PushToTalk;
                Some(TalkEvent::Started(PushToTalk))
            }
            (State::Idle, DetOut::DoubleTap) => {
                self.state = State::FreeTalk(Free::Starting);
                Some(TalkEvent::Started(FreeTalk))
            }
            (State::PushToTalk, DetOut::Release { clean: true }) => {
                self.state = State::Idle;
                Some(TalkEvent::Stopped(PushToTalk))
            }
            (State::PushToTalk, DetOut::Dirty)
            | (State::PushToTalk, DetOut::Release { clean: false }) => {
                self.state = State::Idle;
                Some(TalkEvent::Cancelled(PushToTalk))
            }
            (State::FreeTalk(Free::Starting), DetOut::Dirty)
            | (State::FreeTalk(Free::Starting), DetOut::Release { clean: false }) => {
                self.state = State::Idle;
                Some(TalkEvent::Cancelled(FreeTalk))
            }
            (State::FreeTalk(Free::Starting), DetOut::Release { clean: true }) => {
                self.state = State::FreeTalk(Free::Running);
                None
            }
            (State::FreeTalk(Free::Running), DetOut::Down) => {
                self.state = State::FreeTalk(Free::Ending { dirty: false });
                None
            }
            (State::FreeTalk(Free::Ending { .. }), DetOut::Dirty) => {
                self.state = State::FreeTalk(Free::Ending { dirty: true });
                None
            }
            (State::FreeTalk(Free::Ending { dirty }), DetOut::Release { clean }) => {
                if clean && !dirty {
                    self.state = State::Idle;
                    Some(TalkEvent::Stopped(FreeTalk))
                } else {
                    // A chord such as Right Cmd+C: free-talk keeps running.
                    self.state = State::FreeTalk(Free::Running);
                    None
                }
            }
            // Hold during free-talk never starts push-to-talk; everything else is not interesting.
            _ => None,
        }
    }

    /// Hotkey changed: an active talk is cancelled. Always ends in Idle.
    pub fn reset(&mut self) -> Option<TalkEvent> {
        let ev = match self.state {
            State::Idle => None,
            State::PushToTalk => Some(TalkEvent::Cancelled(Mode::PushToTalk)),
            State::FreeTalk(_) => Some(TalkEvent::Cancelled(Mode::FreeTalk)),
        };
        self.state = State::Idle;
        ev
    }
}
