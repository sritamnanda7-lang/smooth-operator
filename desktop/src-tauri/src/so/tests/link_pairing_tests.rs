// SPDX-License-Identifier: FSL-1.1-MIT
//! Step 5 P1-P8 (+ P9 OsRng): the pairing window. Pure: injected clock (`FakeClock::at`) and random
//! (`FakeRng`). Numbers follow draft/step-5-plan.md ("Pairing window").
use super::link_support::FakeRng;
use super::rules_support::FakeClock;
use crate::so::link::pairing::{OsRng, PairError, PairResult, Pairing, Rng, CODE_TTL, LOCK_TIME, MAX_DRAWS, MAX_WRONG};

/// A window started at t=0 whose code is `code`'s draw (4 bytes, BIG-endian u32 -> mod 1,000,000).
fn started(code_n: u32) -> (Pairing, std::sync::Arc<FakeClock>, String) {
    let clock = FakeClock::new();
    let rng = FakeRng::queue(&[&code_n.to_be_bytes()]);
    let mut p = Pairing::new();
    let view = p.start(clock.at(0), &*rng).ok().unwrap_or_else(|| panic!("start"));
    (p, clock, view.code)
}

#[test]
fn constants_match_the_plan() {
    assert_eq!(CODE_TTL.as_secs(), 120);
    assert_eq!(MAX_WRONG, 5);
    assert_eq!(LOCK_TIME.as_secs(), 60);
    assert_eq!(MAX_DRAWS, 16);
}

// ---------- P1 ----------
#[test]
fn p1_code_is_6_digits_and_secs_left_is_120() {
    let clock = FakeClock::new();
    let rng = FakeRng::prng(1);
    for _ in 0..50 {
        let mut p = Pairing::new();
        let v = p.start(clock.at(0), &*rng).ok().unwrap();
        assert_eq!(v.code.len(), 6, "{}", v.code);
        assert!(v.code.bytes().all(|b| b.is_ascii_digit()), "{}", v.code);
        assert_eq!(v.secs_left, 120);
        assert_eq!(p.secs_left(clock.at(0)), Some(120));
    }
}

#[test]
fn p1_a_draw_at_or_above_4_294_000_000_is_redrawn() {
    let clock = FakeClock::new();
    // 0xFFFFFFFF = 4,294,967,295 (rejected), 4,294,000,000 exactly (rejected: "redraw if >= limit"), then 42.
    let rng = FakeRng::queue(&[&u32::MAX.to_be_bytes(), &4_294_000_000u32.to_be_bytes(), &42u32.to_be_bytes()]);
    let mut p = Pairing::new();
    let v = p.start(clock.at(0), &*rng).ok().unwrap();
    assert_eq!(v.code, "000042", "leading zeros are kept; the two high draws were skipped");
    assert_eq!(rng.calls(), 3, "one 4-byte draw per try");
}

#[test]
fn p1_a_draw_just_below_the_limit_is_accepted_and_reduced_mod_1_000_000() {
    let clock = FakeClock::new();
    let rng = FakeRng::queue(&[&4_293_999_999u32.to_be_bytes()]);
    let mut p = Pairing::new();
    assert_eq!(p.start(clock.at(0), &*rng).ok().unwrap().code, "999999");
    assert_eq!(rng.calls(), 1);
}

/// Tester-chosen: the u32 is read big-endian (`u32::from_be_bytes`). The plan only says "4 random bytes as
/// u32". If the builder reads little-endian, flip this one test (and tell the tester).
#[test]
fn p1_the_four_bytes_are_one_big_endian_u32() {
    let (_p, _c, code) = started(1_000_042);
    assert_eq!(code, "000042", "1,000,042 mod 1,000,000 = 42");
}

#[test]
fn p1_always_above_the_limit_gives_err_rng_after_16_draws() {
    let clock = FakeClock::new();
    let rng = FakeRng::constant(0xFF);
    let mut p = Pairing::new();
    assert_eq!(p.start(clock.at(0), &*rng).err(), Some(PairError::Rng));
    assert_eq!(rng.calls(), MAX_DRAWS as usize, "exactly 16 draws, then fail closed");
    assert_eq!(p.secs_left(clock.at(0)), None, "no window was opened");
}

// ---------- P2 ----------
#[test]
fn p2_right_code_at_119_999_ms_is_ok_and_the_window_closes_replay_is_not_pairing() {
    let (mut p, clock, code) = started(123_456);
    assert_eq!(code, "123456");
    assert_eq!(p.try_code(clock.at(119_999), &code), PairResult::Ok);
    assert_eq!(p.try_code(clock.at(120_000), &code), PairResult::NotPairing, "replay of a used code");
    assert_eq!(p.secs_left(clock.at(120_000)), None);
}

// ---------- P3 ----------
#[test]
fn p3_exactly_120_s_is_expired_and_so_is_120_s_plus_1_ms() {
    let (mut p, clock, code) = started(654_321);
    assert_eq!(p.try_code(clock.at(120_000), &code), PairResult::Expired, "elapsed < TTL is the rule: 120.000 s is too late");
    let (mut p, clock, code) = started(654_321);
    assert_eq!(p.try_code(clock.at(120_001), &code), PairResult::Expired);
    let (mut p, clock, _code) = started(654_321);
    assert_eq!(p.try_code(clock.at(500_000), "000000"), PairResult::Expired, "a wrong code after expiry is Expired, not Wrong");
}

#[test]
fn p3_secs_left_counts_down_and_is_none_when_expired() {
    let (p, clock, _) = started(5);
    assert_eq!(p.secs_left(clock.at(30_000)), Some(90));
    assert_eq!(p.secs_left(clock.at(120_000)), None);
}

// ---------- P4 ----------
fn wrong(n: u32) -> String {
    // a 6-digit code that is never the window's code (the window's code is 111111)
    format!("{:06}", 222_222 + n)
}

#[test]
fn p4_four_wrong_then_right_is_ok() {
    let (mut p, clock, code) = started(111_111);
    for i in 0..4 {
        assert_eq!(p.try_code(clock.at(1000 + i), &wrong(i as u32)), PairResult::Wrong);
    }
    assert_eq!(p.try_code(clock.at(2000), &code), PairResult::Ok);
}

#[test]
fn p4_the_fifth_wrong_locks_for_60_s_the_right_code_is_locked_start_is_refused_then_the_code_is_burned() {
    let (mut p, clock, code) = started(111_111);
    for i in 0..4 {
        assert_eq!(p.try_code(clock.at(1000), &wrong(i)), PairResult::Wrong);
    }
    // Tester reading of "5 wrong tries -> locked 60 s": the 5th wrong try itself answers Locked{60}.
    assert_eq!(p.try_code(clock.at(1000), &wrong(4)), PairResult::Locked { secs: 60 });
    assert_eq!(p.locked_secs(clock.at(1000)), Some(60));
    // During the lock even the right code is Locked.
    match p.try_code(clock.at(31_000), &code) {
        PairResult::Locked { secs } => assert!((1..=30).contains(&secs), "about 30 s left, got {secs}"),
        other => panic!("right code during the lock must be Locked, got {other:?}"),
    }
    // `start` during the lock is refused.
    let rng = FakeRng::prng(3);
    match p.start(clock.at(31_000), &*rng).err() {
        Some(PairError::Locked { secs }) => assert!((1..=30).contains(&secs), "{secs}"),
        other => panic!("start during the lock must be Err(Locked), got {other:?}"),
    }
    // After the lock (60 s after it began) the code is burned: the founder must click Pair again.
    assert_eq!(p.try_code(clock.at(61_000), &code), PairResult::NotPairing);
    assert_eq!(p.locked_secs(clock.at(61_000)), None);
    assert!(p.start(clock.at(61_000), &*rng).is_ok(), "a new window can be started after the lock");
}

// ---------- P5 ----------
#[test]
fn p5_anything_but_exactly_6_ascii_digits_is_a_wrong_try() {
    let bad = ["12345", "1234567", "12a456", " 123456", "123456 ", "", "١٢٣٤٥٦", "+23456", "12-456", "123456\n"];
    for input in bad {
        let (mut p, clock, code) = started(123_456);
        assert_eq!(p.try_code(clock.at(1000), input), PairResult::Wrong, "{input:?}");
        // It was counted but did not burn the code.
        assert_eq!(p.try_code(clock.at(1001), &code), PairResult::Ok, "after {input:?} the right code still works");
    }
    // ...and they count: 5 of them lock.
    let (mut p, clock, _) = started(123_456);
    for _ in 0..4 {
        assert_eq!(p.try_code(clock.at(1000), "12a456"), PairResult::Wrong);
    }
    assert_eq!(p.try_code(clock.at(1000), "1234567"), PairResult::Locked { secs: 60 });
}

// ---------- P6 ----------
#[test]
fn p6_start_twice_the_old_code_is_dead() {
    let clock = FakeClock::new();
    let rng = FakeRng::queue(&[&1u32.to_be_bytes(), &2u32.to_be_bytes()]);
    let mut p = Pairing::new();
    let a = p.start(clock.at(0), &*rng).ok().unwrap().code;
    let b = p.start(clock.at(1000), &*rng).ok().unwrap().code;
    assert_eq!((a.as_str(), b.as_str()), ("000001", "000002"));
    assert_eq!(p.try_code(clock.at(2000), &a), PairResult::Wrong, "the old code is dead");
    assert_eq!(p.try_code(clock.at(2001), &b), PairResult::Ok);
}

// ---------- P7 ----------
#[test]
fn p7_cancel_means_no_window() {
    let (mut p, clock, code) = started(777_777);
    p.cancel();
    assert_eq!(p.secs_left(clock.at(1000)), None);
    assert_eq!(p.try_code(clock.at(1000), &code), PairResult::NotPairing);
    let mut fresh = Pairing::new();
    assert_eq!(fresh.try_code(clock.at(0), "123456"), PairResult::NotPairing, "no window ever started");
}

// ---------- P8 ----------
#[test]
fn p8_random_failure_fails_closed() {
    let clock = FakeClock::new();
    let mut p = Pairing::new();
    assert_eq!(p.start(clock.at(0), &*FakeRng::failing()).err(), Some(PairError::Rng));
    assert_eq!(p.secs_left(clock.at(0)), None);
    // A queue that runs dry mid-way is also a failure.
    let mut p = Pairing::new();
    assert_eq!(p.start(clock.at(0), &*FakeRng::queue(&[&[1, 2]])).err(), Some(PairError::Rng));
}

// ---------- P9: the real random source ----------
#[test]
fn p9_os_rng_fills_different_nonzero_bytes() {
    let (mut a, mut b) = ([0u8; 32], [0u8; 32]);
    OsRng.fill(&mut a).expect("getrandom");
    OsRng.fill(&mut b).expect("getrandom");
    assert_ne!(a, [0u8; 32]);
    assert_ne!(a, b);
}
