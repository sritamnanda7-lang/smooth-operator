// SPDX-License-Identifier: FSL-1.1-MIT
//! T1-T15, T28-T30: detector + session through the pure `Engine`. No threads, injected time.
use crate::so::hotkey::detector::{DetOut, Detector};
use crate::so::hotkey::session::Mode::{FreeTalk, PushToTalk};
use crate::so::hotkey::session::TalkEvent::{self, Cancelled, Started, Stopped};
use crate::so::hotkey::Engine;
use crate::so::settings::HotkeyChoice;
use handy_keys::{Key, KeyEvent, Modifiers};
use std::time::{Duration, Instant};

const FN: Modifiers = Modifiers::FN;
const RC: Modifiers = Modifiers::CMD_RIGHT;
const NONE: Modifiers = Modifiers::empty();

fn modev(flags: Modifiers, down: bool, changed: Modifiers) -> KeyEvent {
    KeyEvent { modifiers: flags, key: None, is_key_down: down, changed_modifier: Some(changed) }
}
fn keyev(flags: Modifiers, key: Key, down: bool) -> KeyEvent {
    KeyEvent { modifiers: flags, key: Some(key), is_key_down: down, changed_modifier: None }
}

/// Engine + a base instant, so tests read in milliseconds.
struct Rig {
    e: Engine,
    t0: Instant,
    target: Modifiers,
}
impl Rig {
    fn new(choice: HotkeyChoice) -> Self {
        Rig { e: Engine::new(choice), t0: Instant::now(), target: choice.modifier() }
    }
    fn at(&self, ms: u64) -> Instant {
        self.t0 + Duration::from_millis(ms)
    }
    fn down(&mut self, ms: u64) -> Vec<TalkEvent> {
        let ev = modev(self.target, true, self.target);
        self.e.feed(&ev, self.at(ms))
    }
    fn up(&mut self, ms: u64) -> Vec<TalkEvent> {
        let ev = modev(NONE, false, self.target);
        self.e.feed(&ev, self.at(ms))
    }
    /// Normal key while the target is held.
    fn key_held(&mut self, key: Key, down: bool, ms: u64) -> Vec<TalkEvent> {
        let ev = keyev(self.target, key, down);
        self.e.feed(&ev, self.at(ms))
    }
    /// Normal key with no modifier held.
    fn key_free(&mut self, key: Key, down: bool, ms: u64) -> Vec<TalkEvent> {
        self.e.feed(&keyev(NONE, key, down), self.at(ms))
    }
    fn tick(&mut self, ms: u64) -> Vec<TalkEvent> {
        self.e.tick(self.at(ms))
    }
    /// Press (clean), hold past 300 ms: returns events from the tick.
    fn start_ptt(&mut self, ms: u64) {
        assert!(self.down(ms).is_empty());
        assert_eq!(self.tick(ms + 310), vec![Started(PushToTalk)]);
    }
    /// Tap, then 2nd press at +250: leaves the 2nd press still down.
    fn start_free(&mut self, ms: u64) {
        assert!(self.down(ms).is_empty());
        assert!(self.up(ms + 80).is_empty());
        assert_eq!(self.down(ms + 250), vec![Started(FreeTalk)]);
    }
    /// Start free-talk and release the 2nd press cleanly.
    fn free_running(&mut self, ms: u64) {
        self.start_free(ms);
        assert!(self.up(ms + 300).is_empty());
    }
}

// ---------- T1-T5 ----------
#[test]
fn t01_hold_300ms_starts_push_to_talk_release_stops() {
    let mut r = Rig::new(HotkeyChoice::Fn);
    assert!(r.down(0).is_empty());
    assert!(r.e.needs_tick());
    assert!(r.tick(100).is_empty());
    assert!(r.tick(299).is_empty());
    assert_eq!(r.tick(300), vec![Started(PushToTalk)]);
    assert!(!r.e.needs_tick(), "no timer needed once the hold started");
    assert!(r.tick(500).is_empty());
    assert_eq!(r.up(800), vec![Stopped(PushToTalk)]);
}

#[test]
fn t02_lone_tap_does_nothing() {
    let mut r = Rig::new(HotkeyChoice::Fn);
    assert!(r.down(0).is_empty());
    assert!(r.up(120).is_empty());
    assert!(!r.e.needs_tick());
    assert!(r.tick(1000).is_empty());
}

#[test]
fn t03_double_tap_starts_free_talk_on_second_down() {
    let mut r = Rig::new(HotkeyChoice::Fn);
    assert!(r.down(0).is_empty());
    assert!(r.up(80).is_empty());
    assert_eq!(r.down(250), vec![Started(FreeTalk)]);
    assert!(r.up(300).is_empty(), "clean 2nd release does nothing");
    assert!(r.tick(2000).is_empty());
}

#[test]
fn t04_free_talk_ends_on_release_of_a_clean_tap_not_on_down() {
    let mut r = Rig::new(HotkeyChoice::Fn);
    r.free_running(0);
    assert!(r.down(2000).is_empty(), "not on down");
    assert_eq!(r.up(2080), vec![Stopped(FreeTalk)]);
}

#[test]
fn t05_free_talk_clean_hold_never_starts_push_to_talk_and_ends_on_release() {
    let mut r = Rig::new(HotkeyChoice::Fn);
    r.free_running(0);
    assert!(r.down(2000).is_empty());
    assert!(r.tick(2400).is_empty(), "no Started(PushToTalk) during free-talk");
    assert_eq!(r.up(3000), vec![Stopped(FreeTalk)]);
}

// ---------- T6-T7 ----------
#[test]
fn t06_chord_after_second_release_keeps_free_talk_running() {
    let mut r = Rig::new(HotkeyChoice::RightCmd);
    r.free_running(0);
    // Right Cmd+C
    assert!(r.down(2000).is_empty());
    assert!(r.key_held(Key::C, true, 2040).is_empty());
    assert!(r.key_held(Key::C, false, 2090).is_empty());
    assert!(r.up(2120).is_empty(), "chord press does not end free-talk");
    assert!(r.tick(5000).is_empty());
    // Still running: a clean tap now ends it.
    assert!(r.down(6000).is_empty());
    assert_eq!(r.up(6080), vec![Stopped(FreeTalk)]);
}

#[test]
fn t07_tap_right_after_the_ending_tap_does_not_start_free_talk() {
    let mut r = Rig::new(HotkeyChoice::Fn);
    r.free_running(0);
    assert!(r.down(2000).is_empty());
    assert_eq!(r.up(2080), vec![Stopped(FreeTalk)]);
    // Within 400 ms of the ending tap's release:
    assert!(r.down(2200).is_empty(), "ending tap must not count as 1st tap of a double-tap");
    assert!(r.up(2260).is_empty());
    assert!(r.tick(4000).is_empty());
}

#[test]
fn t07_a_real_double_tap_later_still_works() {
    let mut r = Rig::new(HotkeyChoice::Fn);
    r.free_running(0);
    assert!(r.down(2000).is_empty());
    assert_eq!(r.up(2080), vec![Stopped(FreeTalk)]);
    r.free_running(5000);
}

// ---------- T8-T12 ----------
#[test]
fn t08_second_tap_after_400ms_is_not_a_double_tap() {
    let mut r = Rig::new(HotkeyChoice::Fn);
    assert!(r.down(0).is_empty());
    assert!(r.up(80).is_empty());
    assert!(r.down(700).is_empty()); // 620 ms after release
    assert!(r.up(760).is_empty());
    assert!(r.tick(3000).is_empty());
}

#[test]
fn t08_second_tap_just_inside_400ms_counts() {
    let mut r = Rig::new(HotkeyChoice::Fn);
    assert!(r.down(0).is_empty());
    assert!(r.up(80).is_empty());
    assert_eq!(r.down(480), vec![Started(FreeTalk)]); // 400 ms after release
}

#[test]
fn t09_chord_within_300ms_does_nothing() {
    let mut r = Rig::new(HotkeyChoice::RightCmd);
    assert!(r.down(0).is_empty());
    assert!(r.key_held(Key::C, true, 50).is_empty());
    assert!(r.key_held(Key::C, false, 90).is_empty());
    assert!(r.tick(400).is_empty(), "chord killed the hold timer");
    assert!(r.up(500).is_empty());
}

#[test]
fn t10_another_modifier_held_first_does_nothing() {
    let mut r = Rig::new(HotkeyChoice::RightCmd);
    let both = RC | Modifiers::SHIFT_LEFT;
    assert!(r.e.feed(&modev(both, true, RC), r.at(0)).is_empty());
    assert!(r.tick(500).is_empty());
    assert!(r.e.feed(&modev(Modifiers::SHIFT_LEFT, false, RC), r.at(600)).is_empty());
}

#[test]
fn t11_left_cmd_is_not_right_cmd() {
    let mut r = Rig::new(HotkeyChoice::RightCmd);
    let l = Modifiers::CMD_LEFT;
    assert!(r.e.feed(&modev(l, true, l), r.at(0)).is_empty());
    assert!(r.tick(500).is_empty());
    assert!(r.e.feed(&modev(NONE, false, l), r.at(600)).is_empty());
    assert!(r.e.feed(&modev(l, true, l), r.at(700)).is_empty());
    assert!(r.e.feed(&modev(NONE, false, l), r.at(750)).is_empty());
}

#[test]
fn t12_fn_flag_on_arrow_without_fn_press_does_nothing() {
    let mut r = Rig::new(HotkeyChoice::Fn);
    // macOS sets the Fn flag on arrow / F-keys by itself.
    assert!(r.e.feed(&keyev(FN, Key::LeftArrow, true), r.at(0)).is_empty());
    assert!(r.e.feed(&keyev(FN, Key::LeftArrow, false), r.at(40)).is_empty());
    assert!(r.e.feed(&keyev(FN, Key::F1, true), r.at(80)).is_empty());
    assert!(!r.e.needs_tick());
    assert!(r.tick(1000).is_empty());
    // And a real tap after that is still just a tap.
    assert!(r.down(2000).is_empty());
    assert!(r.up(2100).is_empty());
}

#[test]
fn t12_fn_plus_arrow_is_a_chord() {
    let mut r = Rig::new(HotkeyChoice::Fn);
    assert!(r.down(0).is_empty());
    assert!(r.key_held(Key::LeftArrow, true, 60).is_empty());
    assert!(r.tick(400).is_empty());
    assert!(r.up(500).is_empty());
}

// ---------- T13-T15 ----------
#[test]
fn t13_key_during_push_to_talk_cancels() {
    let mut r = Rig::new(HotkeyChoice::Fn);
    r.start_ptt(0);
    assert_eq!(r.key_held(Key::Delete, true, 500), vec![Cancelled(PushToTalk)]);
    assert!(r.key_held(Key::Delete, false, 540).is_empty());
    assert!(r.up(600).is_empty(), "nothing after a cancel");
    assert!(r.tick(2000).is_empty());
}

#[test]
fn t13_modifier_during_push_to_talk_cancels() {
    let mut r = Rig::new(HotkeyChoice::RightCmd);
    r.start_ptt(0);
    let both = RC | Modifiers::SHIFT_LEFT;
    let out = r.e.feed(&modev(both, true, Modifiers::SHIFT_LEFT), r.at(500));
    assert_eq!(out, vec![Cancelled(PushToTalk)]);
}

#[test]
fn t14_enter_and_esc_never_cancel_before_or_during_hold() {
    for key in [Key::Return, Key::KeypadEnter, Key::Escape] {
        let mut r = Rig::new(HotkeyChoice::Fn);
        assert!(r.down(0).is_empty());
        assert!(r.key_held(key, true, 40).is_empty());
        assert!(r.key_held(key, false, 60).is_empty());
        assert_eq!(r.tick(320), vec![Started(PushToTalk)], "{key:?} before hold");
        assert!(r.key_held(key, true, 400).is_empty(), "{key:?} during hold");
        assert!(r.key_held(key, false, 420).is_empty());
        assert_eq!(r.up(900), vec![Stopped(PushToTalk)]);
    }
}

#[test]
fn t14_enter_esc_do_not_cancel_free_talk_press() {
    let mut r = Rig::new(HotkeyChoice::Fn);
    r.start_free(0);
    assert!(r.key_held(Key::Return, true, 280).is_empty());
    assert!(r.key_held(Key::Escape, true, 290).is_empty());
    assert!(r.up(320).is_empty());
    assert!(r.down(2000).is_empty());
    assert_eq!(r.up(2060), vec![Stopped(FreeTalk)], "free-talk survived Enter/Esc");
}

#[test]
fn t15_target_change_mid_push_to_talk_cancels_and_goes_idle() {
    let mut r = Rig::new(HotkeyChoice::Fn);
    r.start_ptt(0);
    assert_eq!(r.e.set_target(HotkeyChoice::RightCmd), Some(Cancelled(PushToTalk)));
    assert!(r.up(900).is_empty(), "old key release is ignored (Fn flag gone)");
    assert!(!r.e.needs_tick());
    // New key works; old key does not.
    r.target = RC;
    r.start_ptt(2000);
    assert_eq!(r.up(2800), vec![Stopped(PushToTalk)]);
    r.target = FN;
    assert!(r.down(4000).is_empty());
    assert!(r.tick(4400).is_empty(), "Fn does nothing now");
    assert!(r.up(4500).is_empty());
}

#[test]
fn t15_target_change_mid_free_talk_cancels() {
    let mut r = Rig::new(HotkeyChoice::Fn);
    r.free_running(0);
    assert_eq!(r.e.set_target(HotkeyChoice::RightCmd), Some(Cancelled(FreeTalk)));
    r.target = RC;
    assert!(r.down(3000).is_empty());
    assert!(r.up(3080).is_empty(), "no leftover free-talk to stop");
}

#[test]
fn t15_target_change_while_idle_or_pending_is_quiet_and_kills_timer() {
    let mut r = Rig::new(HotkeyChoice::Fn);
    assert_eq!(r.e.set_target(HotkeyChoice::RightCmd), None);
    let mut r = Rig::new(HotkeyChoice::Fn);
    assert!(r.down(0).is_empty());
    assert_eq!(r.e.set_target(HotkeyChoice::RightCmd), None);
    assert!(!r.e.needs_tick());
    assert!(r.tick(400).is_empty(), "old hold timer must not fire");
}

// ---------- T28 ----------
#[test]
fn t28_fn_tap_then_fn_left_cancels_free_talk() {
    let mut r = Rig::new(HotkeyChoice::Fn);
    assert!(r.down(0).is_empty());
    assert!(r.up(80).is_empty());
    assert_eq!(r.down(200), vec![Started(FreeTalk)]);
    assert_eq!(r.key_held(Key::LeftArrow, true, 260), vec![Cancelled(FreeTalk)]);
    assert!(r.key_held(Key::LeftArrow, false, 300).is_empty());
    assert!(r.up(340).is_empty());
    // Really cancelled: a clean tap later does not "stop" anything.
    assert!(r.down(3000).is_empty());
    assert!(r.up(3080).is_empty());
}

#[test]
fn t28_fn_tap_then_fn_delete_cancels_free_talk() {
    let mut r = Rig::new(HotkeyChoice::Fn);
    assert!(r.down(0).is_empty());
    assert!(r.up(80).is_empty());
    assert_eq!(r.down(200), vec![Started(FreeTalk)]);
    assert_eq!(r.key_held(Key::Delete, true, 260), vec![Cancelled(FreeTalk)]);
}

#[test]
fn t28_right_cmd_tap_then_cmd_tab_cancels_free_talk() {
    let mut r = Rig::new(HotkeyChoice::RightCmd);
    assert!(r.down(0).is_empty());
    assert!(r.up(80).is_empty());
    assert_eq!(r.down(200), vec![Started(FreeTalk)]);
    assert_eq!(r.key_held(Key::Tab, true, 260), vec![Cancelled(FreeTalk)]);
    assert!(r.key_held(Key::Tab, false, 300).is_empty());
    assert!(r.up(340).is_empty());
}

#[test]
fn t28_key_between_taps_breaks_the_double_tap() {
    let mut r = Rig::new(HotkeyChoice::Fn);
    assert!(r.down(0).is_empty());
    assert!(r.up(80).is_empty());
    assert!(r.key_free(Key::A, true, 150).is_empty());
    assert!(r.key_free(Key::A, false, 170).is_empty());
    assert!(r.down(250).is_empty(), "typed key between taps: not a double-tap");
    assert!(r.up(300).is_empty());
}

// ---------- T29: detector flag-derived state ----------
#[test]
fn t29_down_while_down_is_one_down() {
    let t0 = Instant::now();
    let mut d = Detector::new(FN);
    assert_eq!(d.feed(&modev(FN, true, FN), t0), vec![DetOut::Down]);
    assert!(d.feed(&modev(FN, true, FN), t0 + Duration::from_millis(10)).is_empty());
}

#[test]
fn t29_up_while_idle_is_ignored() {
    let t0 = Instant::now();
    let mut d = Detector::new(FN);
    assert!(d.feed(&modev(NONE, false, FN), t0).is_empty());
    assert!(!d.needs_tick());
}

#[test]
fn t29_flags_show_target_up_while_we_think_down_is_a_release() {
    let t0 = Instant::now();
    let mut d = Detector::new(FN);
    assert_eq!(d.feed(&modev(FN, true, FN), t0), vec![DetOut::Down]);
    // The Fn-up event was lost. The next event (A key-up) carries flags without Fn.
    let out = d.feed(&keyev(NONE, Key::A, false), t0 + Duration::from_millis(100));
    assert_eq!(out, vec![DetOut::Release { clean: true }]);
    assert!(!d.needs_tick());
}

#[test]
fn t29_detector_hold_dirty_double_tap_outputs() {
    let t0 = Instant::now();
    let ms = |n| t0 + Duration::from_millis(n);
    let mut d = Detector::new(RC);
    assert_eq!(d.feed(&modev(RC, true, RC), ms(0)), vec![DetOut::Down]);
    assert!(d.needs_tick());
    assert!(d.tick(ms(299)).is_empty());
    assert_eq!(d.tick(ms(300)), vec![DetOut::Hold]);
    assert_eq!(d.feed(&keyev(RC, Key::C, true), ms(400)), vec![DetOut::Dirty]);
    assert_eq!(d.feed(&modev(NONE, false, RC), ms(500)), vec![DetOut::Release { clean: false }]);
    // clean tap then 2nd press
    assert_eq!(d.feed(&modev(RC, true, RC), ms(2000)), vec![DetOut::Down]);
    assert_eq!(d.feed(&modev(NONE, false, RC), ms(2080)), vec![DetOut::Release { clean: true }]);
    assert_eq!(d.feed(&modev(RC, true, RC), ms(2300)), vec![DetOut::Down, DetOut::DoubleTap]);
}

#[test]
fn t29_enter_esc_are_not_dirty_in_detector() {
    let t0 = Instant::now();
    let mut d = Detector::new(FN);
    d.feed(&modev(FN, true, FN), t0);
    assert!(d.feed(&keyev(FN, Key::Return, true), t0 + Duration::from_millis(30)).is_empty());
    assert!(d.feed(&keyev(FN, Key::Escape, true), t0 + Duration::from_millis(40)).is_empty());
}

// ---------- T30: lost key-up ----------
#[test]
fn t30_lost_up_during_tap_then_next_press_works() {
    let mut r = Rig::new(HotkeyChoice::RightCmd);
    assert!(r.down(0).is_empty());
    // Up lost. Next event is a normal key with flags showing Right Cmd not held.
    assert!(r.key_free(Key::A, true, 100).is_empty());
    assert!(r.key_free(Key::A, false, 130).is_empty());
    // Next real press, much later, works normally.
    r.start_ptt(5000);
    assert_eq!(r.up(5800), vec![Stopped(PushToTalk)]);
}

#[test]
fn t30_lost_up_during_hold_ends_talk_at_next_event_then_next_press_works() {
    let mut r = Rig::new(HotkeyChoice::RightCmd);
    r.start_ptt(0);
    // Up lost. Flags on the next event show Right Cmd up: that is the release.
    let out = r.key_free(Key::A, true, 900);
    assert_eq!(out, vec![Stopped(PushToTalk)]);
    r.start_ptt(5000);
    assert_eq!(r.up(5800), vec![Stopped(PushToTalk)]);
}

#[test]
fn t30_fn_lost_up_is_corrected_by_the_very_next_event() {
    let mut r = Rig::new(HotkeyChoice::Fn);
    r.start_ptt(0);
    // Arrow key-down carrying no Fn flag: Fn is up (flags are fresh for Fn).
    let out = r.e.feed(&keyev(NONE, Key::LeftArrow, true), r.at(900));
    assert_eq!(out, vec![Stopped(PushToTalk)]);
}
