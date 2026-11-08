// SPDX-License-Identifier: FSL-1.1-MIT
//! Pure key-state detector (rewrite of the spike detector). STUB. Time is passed in, never read.
//! Tests: `so/tests/hotkey_tests.rs` (T29 here; T1-T15, T28, T30 through `Engine`).
//!
//! Contract the tests check:
//! - State comes from `ev.modifiers.contains(target)` on EVERY event (not from `changed_modifier`).
//!   down-while-down is ignored; up-while-Idle is ignored; flags show target up while we think it is
//!   down = a release.
//! - `Down`: a clean press began (target in flags, no other modifier held). Also sent for the 2nd press
//!   of a double-tap (then followed by `DoubleTap` in the same output).
//! - `Hold`: the press is still down 300 ms after `Down` (from `tick`).
//! - `Release{clean}`: target released after a `Down`; `clean` = no `Dirty` happened during that press.
//! - `Dirty`: during a press, any other key or modifier went down (except Enter/Esc/KeypadEnter, which
//!   never count). Also sent when a key goes down between a tap and its 2nd press (breaks the double-tap).
//!   Never sent when nothing is pending (e.g. Right Cmd+C after the 2nd release).
//! - `DoubleTap`: 2nd clean press started <= 400 ms after the first tap was released (tap = < 300 ms, clean).
//! - A press that starts with another modifier already held, or with the Left variant, gives nothing.
//! - `needs_tick()` is true only while a press is pending the 300 ms hold timer.

use handy_keys::{KeyEvent, Modifiers};
use std::time::{Duration, Instant};

pub const HOLD_AFTER: Duration = Duration::from_millis(300);
pub const DOUBLE_TAP_GAP: Duration = Duration::from_millis(400);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DetOut {
    Down,
    Hold,
    Release { clean: bool },
    Dirty,
    DoubleTap,
}

pub struct Detector {
    _target: Modifiers,
}

impl Detector {
    pub fn new(target: Modifiers) -> Self {
        Detector { _target: target }
    }

    pub fn feed(&mut self, _ev: &KeyEvent, _now: Instant) -> Vec<DetOut> {
        todo!("step 1: detector feed")
    }

    pub fn tick(&mut self, _now: Instant) -> Vec<DetOut> {
        todo!("step 1: detector tick")
    }

    pub fn needs_tick(&self) -> bool {
        todo!("step 1: detector needs_tick")
    }

    /// Forget a remembered tap (used after a free-talk ends so the ending tap cannot start a new double-tap).
    pub fn forget_tap(&mut self) {
        todo!("step 1: detector forget_tap")
    }
}
