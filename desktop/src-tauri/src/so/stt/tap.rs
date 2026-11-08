// SPDX-License-Identifier: FSL-1.1-MIT
//! The speech tap: one static "last speech instant", updated by `StreamRouter::feed` for every frame
//! that passed the VAD. Lock-free (an atomic: a number that threads can read and write without a lock).
//! Tests: `so/tests/stt_tests.rs` (tap).
use super::sink::SpeechTap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::OnceLock;
use std::time::{Duration, Instant};

/// Time zero for the stored number (an `Instant` cannot live in an atomic, so we store the
/// nanoseconds since this base, plus 1; 0 means "never").
static BASE: OnceLock<Instant> = OnceLock::new();
static LAST: AtomicU64 = AtomicU64::new(0);

/// Record "speech heard now".
pub fn speech_frame() {
    let base = *BASE.get_or_init(Instant::now);
    let ns = Instant::now().saturating_duration_since(base).as_nanos() as u64;
    LAST.store(ns.saturating_add(1), Ordering::Relaxed);
}

/// The instant of the latest `speech_frame`, `None` if there never was one.
pub fn last_speech() -> Option<Instant> {
    match LAST.load(Ordering::Relaxed) {
        0 => None,
        n => BASE.get().map(|base| *base + Duration::from_nanos(n - 1)),
    }
}

/// The real `SpeechTap`: reads the static.
pub struct StaticTap;

impl SpeechTap for StaticTap {
    fn last_speech(&self) -> Option<Instant> {
        last_speech()
    }
}
