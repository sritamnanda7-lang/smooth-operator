// SPDX-License-Identifier: FSL-1.1-MIT
//! The speech tap: one static "last speech instant", updated by `StreamRouter::feed` for every frame
//! that passed the VAD. Lock-free (an atomic). Tests: `so/tests/stt_tests.rs` (tap).
use super::sink::SpeechTap;
use std::time::Instant;

/// STEP-2 STUB (`todo!()`): record "speech heard now".
pub fn speech_frame() {
    todo!("step 2: tap::speech_frame")
}

/// STEP-2 STUB (`todo!()`): the instant of the latest `speech_frame`, `None` if there never was one.
pub fn last_speech() -> Option<Instant> {
    todo!("step 2: tap::last_speech")
}

/// The real `SpeechTap`: reads the static.
pub struct StaticTap;

impl SpeechTap for StaticTap {
    fn last_speech(&self) -> Option<Instant> {
        last_speech()
    }
}
