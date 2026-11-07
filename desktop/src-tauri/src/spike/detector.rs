// SPDX-License-Identifier: FSL-1.1-MIT
//! Gate 1: turns raw modifier press/release events into "hold" and "double-tap".
//! Pure logic with no OS calls, so it is unit tested. Time is passed in, never read.
//! It reads modifier flags only; it never sees or stores which letter keys were typed.

use handy_keys::{Key, KeyEvent, Modifiers};
use std::time::{Duration, Instant};

/// How long the key must stay down to count as "hold" (plan: 0.3 s).
pub const HOLD_AFTER: Duration = Duration::from_millis(300);
/// Second press must start within this time after the first tap was released.
pub const DOUBLE_TAP_GAP: Duration = Duration::from_millis(400);

/// What the detector reports. (A Rust `enum` is a type that is exactly one of
/// a fixed list of options.)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Out {
    /// The lone modifier went down (use: preload the model).
    Down,
    /// Still down after `HOLD_AFTER`.
    Hold,
    /// Released after a hold (use: stop recording, transcribe).
    HoldEnd,
    /// Second quick tap began.
    DoubleTap,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum State {
    Idle,
    /// Down, not yet 0.3 s.
    Pending(Instant),
    Holding,
    /// Second tap of a double-tap, still down.
    DoubleHeld,
}

pub struct Detector {
    target: Modifiers,
    state: State,
    last_tap_release: Option<Instant>,
}

/// Keys that the app will later use for approve / skip. They are swallowed
/// while the hotkey is held (gate 2) and must not cancel a hold.
pub fn is_approve_or_cancel(key: Option<Key>) -> bool {
    matches!(key, Some(Key::Return) | Some(Key::KeypadEnter) | Some(Key::Escape))
}

impl Detector {
    pub fn new(target: Modifiers) -> Self {
        Self { target, state: State::Idle, last_tap_release: None }
    }

    /// True while the hotkey is physically down (any state except Idle).
    pub fn is_down(&self) -> bool {
        self.state != State::Idle
    }

    fn abort(&mut self) {
        self.state = State::Idle;
        self.last_tap_release = None;
    }

    /// Feed one key event. Returns what became true because of it.
    pub fn feed(&mut self, ev: &KeyEvent, now: Instant) -> Vec<Out> {
        let mut out = Vec::new();
        let others = ev.modifiers.difference(self.target).difference(Modifiers::FN);

        if ev.changed_modifier == Some(self.target) {
            if ev.is_key_down {
                // A press only counts when nothing else is held (a lone key).
                if !others.is_empty() {
                    self.abort();
                    return out;
                }
                let second = self.state == State::Idle
                    && self.last_tap_release.is_some_and(|t| now.duration_since(t) <= DOUBLE_TAP_GAP);
                out.push(Out::Down);
                if second {
                    self.state = State::DoubleHeld;
                    self.last_tap_release = None;
                    out.push(Out::DoubleTap);
                } else {
                    self.state = State::Pending(now);
                }
            } else {
                match self.state {
                    State::Pending(_) => self.last_tap_release = Some(now),
                    State::Holding => {
                        out.push(Out::HoldEnd);
                        self.last_tap_release = None;
                    }
                    State::DoubleHeld | State::Idle => self.last_tap_release = None,
                }
                self.state = State::Idle;
            }
            return out;
        }

        // Another modifier changed, or a normal key went down, while we wait:
        // this is a chord such as Cmd+C, not a lone key. Cancel the hold.
        if self.is_down() && ev.is_key_down {
            let other_modifier = ev.changed_modifier.is_some();
            let chord_key = ev.key.is_some() && !is_approve_or_cancel(ev.key);
            if (other_modifier || chord_key) && matches!(self.state, State::Pending(_)) {
                self.abort();
            }
        } else if !self.is_down() && ev.is_key_down {
            // Any other key between taps breaks a double-tap.
            self.last_tap_release = None;
        }
        out
    }

    /// Call regularly (every ~20 ms) so a hold is noticed while still pressed.
    pub fn tick(&mut self, now: Instant) -> Vec<Out> {
        if let State::Pending(t) = self.state {
            if now.duration_since(t) >= HOLD_AFTER {
                self.state = State::Holding;
                return vec![Out::Hold];
            }
        }
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn modev(mods: Modifiers, down: bool, changed: Modifiers) -> KeyEvent {
        KeyEvent { modifiers: mods, key: None, is_key_down: down, changed_modifier: Some(changed) }
    }
    fn keyev(mods: Modifiers, key: Key, down: bool) -> KeyEvent {
        KeyEvent { modifiers: mods, key: Some(key), is_key_down: down, changed_modifier: None }
    }
    const T: Modifiers = Modifiers::CMD_RIGHT;
    fn ms(n: u64) -> Duration {
        Duration::from_millis(n)
    }

    #[test]
    fn hold_then_release() {
        let t0 = Instant::now();
        let mut d = Detector::new(T);
        assert_eq!(d.feed(&modev(T, true, T), t0), vec![Out::Down]);
        assert!(d.tick(t0 + ms(100)).is_empty());
        assert_eq!(d.tick(t0 + ms(310)), vec![Out::Hold]);
        assert!(d.tick(t0 + ms(400)).is_empty());
        assert_eq!(d.feed(&modev(Modifiers::empty(), false, T), t0 + ms(900)), vec![Out::HoldEnd]);
        assert!(!d.is_down());
    }

    #[test]
    fn double_tap() {
        let t0 = Instant::now();
        let mut d = Detector::new(T);
        d.feed(&modev(T, true, T), t0);
        assert!(d.feed(&modev(Modifiers::empty(), false, T), t0 + ms(80)).is_empty());
        let o = d.feed(&modev(T, true, T), t0 + ms(250));
        assert_eq!(o, vec![Out::Down, Out::DoubleTap]);
        assert!(d.feed(&modev(Modifiers::empty(), false, T), t0 + ms(300)).is_empty());
    }

    #[test]
    fn slow_second_tap_is_not_double() {
        let t0 = Instant::now();
        let mut d = Detector::new(T);
        d.feed(&modev(T, true, T), t0);
        d.feed(&modev(Modifiers::empty(), false, T), t0 + ms(80));
        assert_eq!(d.feed(&modev(T, true, T), t0 + ms(700)), vec![Out::Down]);
    }

    #[test]
    fn single_tap_reports_nothing() {
        let t0 = Instant::now();
        let mut d = Detector::new(T);
        d.feed(&modev(T, true, T), t0);
        assert!(d.tick(t0 + ms(100)).is_empty());
        assert!(d.feed(&modev(Modifiers::empty(), false, T), t0 + ms(120)).is_empty());
    }

    #[test]
    fn chord_cancels_hold() {
        let t0 = Instant::now();
        let mut d = Detector::new(T);
        d.feed(&modev(T, true, T), t0);
        d.feed(&keyev(T, Key::C, true), t0 + ms(50));
        assert!(!d.is_down());
        assert!(d.tick(t0 + ms(400)).is_empty());
    }

    #[test]
    fn approve_and_cancel_keys_do_not_cancel_hold() {
        let t0 = Instant::now();
        let mut d = Detector::new(T);
        d.feed(&modev(T, true, T), t0);
        d.feed(&keyev(T, Key::Return, true), t0 + ms(50));
        d.feed(&keyev(T, Key::Return, false), t0 + ms(60));
        assert_eq!(d.tick(t0 + ms(320)), vec![Out::Hold]);
    }

    #[test]
    fn other_modifier_down_first_is_ignored() {
        let t0 = Instant::now();
        let mut d = Detector::new(T);
        let both = T | Modifiers::SHIFT_LEFT;
        assert!(d.feed(&modev(both, true, T), t0).is_empty());
        assert!(!d.is_down());
    }

    #[test]
    fn left_key_is_not_the_target() {
        let t0 = Instant::now();
        let mut d = Detector::new(T);
        assert!(d.feed(&modev(Modifiers::CMD_LEFT, true, Modifiers::CMD_LEFT), t0).is_empty());
    }

    #[test]
    fn fn_hold_and_double_tap() {
        let t0 = Instant::now();
        let mut d = Detector::new(Modifiers::FN);
        let f = Modifiers::FN;
        assert_eq!(d.feed(&modev(f, true, f), t0), vec![Out::Down]);
        assert_eq!(d.tick(t0 + ms(310)), vec![Out::Hold]);
        assert_eq!(d.feed(&modev(Modifiers::empty(), false, f), t0 + ms(800)), vec![Out::HoldEnd]);
        d.feed(&modev(f, true, f), t0 + ms(1000));
        d.feed(&modev(Modifiers::empty(), false, f), t0 + ms(1060));
        assert_eq!(d.feed(&modev(f, true, f), t0 + ms(1200)), vec![Out::Down, Out::DoubleTap]);
    }

    #[test]
    fn fn_enter_does_not_cancel_hold() {
        let t0 = Instant::now();
        let mut d = Detector::new(Modifiers::FN);
        let f = Modifiers::FN;
        d.feed(&modev(f, true, f), t0);
        d.feed(&keyev(f, Key::Return, true), t0 + ms(40));
        d.feed(&keyev(f, Key::Escape, true), t0 + ms(50));
        assert_eq!(d.tick(t0 + ms(320)), vec![Out::Hold]);
    }

    #[test]
    fn fn_ignored_when_target_is_right_cmd() {
        let t0 = Instant::now();
        let mut d = Detector::new(T);
        // Fn pressed first, then Right Cmd: Fn is not a "different modifier" for this target.
        let m = T | Modifiers::FN;
        assert_eq!(d.feed(&modev(m, true, T), t0), vec![Out::Down]);
    }
}
