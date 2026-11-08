// SPDX-License-Identifier: FSL-1.1-MIT
//! Pure key-state detector (rewrite of the spike detector). Time is passed in, never read.
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

use handy_keys::{Key, KeyEvent, Modifiers};
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

/// Where the target key is, as far as we know. (A Rust `enum` is a type that is exactly one of a
/// fixed list of options; `match` then handles each option.)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Phase {
    /// Target key up.
    Up,
    /// Target key is down but we do not track it (another modifier was already held, or we missed
    /// its key-down). Produces nothing until the flags show it released.
    Ignored,
    /// A clean press is in progress.
    Down {
        start: Instant,
        hold_sent: bool,
        dirty: bool,
        /// This is the 2nd press of a double-tap (no hold timer needed).
        second: bool,
    },
}

pub struct Detector {
    target: Modifiers,
    phase: Phase,
    /// Release time of a clean short tap that may become the 1st half of a double-tap.
    last_tap: Option<Instant>,
}

/// Enter / Esc are the approve / skip keys of later steps: they never count as "another key".
fn is_approve_or_cancel(key: Option<Key>) -> bool {
    matches!(
        key,
        Some(Key::Return) | Some(Key::KeypadEnter) | Some(Key::Escape)
    )
}

impl Detector {
    pub fn new(target: Modifiers) -> Self {
        Detector {
            target,
            phase: Phase::Up,
            last_tap: None,
        }
    }

    /// Modifiers that count as "another modifier". Fn is ignored unless it is the target: macOS
    /// sets the Fn flag on arrow and F keys by itself.
    fn others(&self, flags: Modifiers) -> Modifiers {
        let mut o = flags.difference(self.target);
        if self.target != Modifiers::FN {
            o = o.difference(Modifiers::FN);
        }
        o
    }

    /// True if this event is a key (or another modifier) going down that is not the target and
    /// not Enter/Esc.
    fn is_other_down(&self, ev: &KeyEvent) -> bool {
        if !ev.is_key_down {
            return false;
        }
        match ev.key {
            Some(_) => !is_approve_or_cancel(ev.key),
            None => match ev.changed_modifier {
                Some(m) => {
                    m != self.target && !(m == Modifiers::FN && self.target != Modifiers::FN)
                }
                None => false,
            },
        }
    }

    /// Feed one key event. State comes from the OS flags on every event (`ev.modifiers`), so a
    /// lost key-up is noticed at the next event of any kind. Returns what became true.
    pub fn feed(&mut self, ev: &KeyEvent, now: Instant) -> Vec<DetOut> {
        let mut out = Vec::new();
        // A hold that nobody ticked yet (the release arrived first) still counts as a hold.
        out.extend(self.tick(now));
        let down_now = ev.modifiers.contains(self.target);

        match self.phase {
            Phase::Down {
                start,
                hold_sent,
                dirty,
                second,
            } => {
                if !down_now {
                    // Released (also when the key-up event itself was lost).
                    out.push(DetOut::Release { clean: !dirty });
                    let tap =
                        !dirty && !hold_sent && !second && now.duration_since(start) < HOLD_AFTER;
                    self.last_tap = if tap { Some(now) } else { None };
                    self.phase = Phase::Up;
                } else if !dirty && self.is_other_down(ev) {
                    self.phase = Phase::Down {
                        start,
                        hold_sent,
                        dirty: true,
                        second,
                    };
                    out.push(DetOut::Dirty);
                }
            }
            Phase::Ignored => {
                if !down_now {
                    self.phase = Phase::Up;
                }
            }
            Phase::Up => {
                // Never trust `is_key_down` for the target: handy-keys derives it for Fn from a stored
                // toggle that one lost event inverts for good. The OS flags (`down_now`) decide.
                let target_press = ev.key.is_none() && ev.changed_modifier == Some(self.target);
                if down_now && target_press {
                    if self.others(ev.modifiers).is_empty() {
                        let second = self
                            .last_tap
                            .is_some_and(|t| now.duration_since(t) <= DOUBLE_TAP_GAP);
                        self.last_tap = None;
                        self.phase = Phase::Down {
                            start: now,
                            hold_sent: false,
                            dirty: false,
                            second,
                        };
                        out.push(DetOut::Down);
                        if second {
                            out.push(DetOut::DoubleTap);
                        }
                    } else {
                        self.last_tap = None;
                        self.phase = Phase::Ignored;
                    }
                } else if down_now && ev.key.is_none() {
                    // Target shows as down but we never saw it go down.
                    self.last_tap = None;
                    self.phase = Phase::Ignored;
                } else if self.is_other_down(ev) && self.last_tap.is_some() {
                    // Something else was pressed between the two taps: no double-tap.
                    self.last_tap = None;
                    out.push(DetOut::Dirty);
                }
            }
        }
        out
    }

    /// Call while `needs_tick()` so a hold is noticed while the key is still down.
    pub fn tick(&mut self, now: Instant) -> Vec<DetOut> {
        if let Phase::Down {
            start,
            hold_sent: false,
            dirty: false,
            second: false,
        } = self.phase
        {
            if now.duration_since(start) >= HOLD_AFTER {
                self.phase = Phase::Down {
                    start,
                    hold_sent: true,
                    dirty: false,
                    second: false,
                };
                return vec![DetOut::Hold];
            }
        }
        Vec::new()
    }

    /// True only while a press waits for the 300 ms hold timer.
    pub fn needs_tick(&self) -> bool {
        matches!(
            self.phase,
            Phase::Down {
                hold_sent: false,
                dirty: false,
                second: false,
                ..
            }
        )
    }

    /// Forget a remembered tap (used after a free-talk ends so the ending tap cannot start a new double-tap).
    pub fn forget_tap(&mut self) {
        self.last_tap = None;
    }
}
