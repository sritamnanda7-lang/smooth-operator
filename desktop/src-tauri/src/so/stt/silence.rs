// SPDX-License-Identifier: FSL-1.1-MIT
//! Talk timers (founder Q2). `SilenceWatch::decide` is pure. Tests: `so/tests/stt_tests.rs` (T19).
use std::time::Instant;

/// Why a talk is ended by the app instead of by the user.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum End {
    /// Speech was heard, then `SILENCE_END - VAD_OFFLINE_HANGOVER_MS` of quiet.
    Silence,
    /// No speech within `NO_SPEECH` of the start.
    NoSpeech,
    /// `MAX_TALK` reached.
    MaxLen,
}

pub struct SilenceWatch;

impl SilenceWatch {
    /// STEP-2 STUB (`todo!()`). `last_speech` older than `started` (a stale tap from an earlier talk)
    /// counts as no speech. Limits are inclusive: exactly 8 s with no speech -> `NoSpeech`;
    /// exactly 1050 ms (`SILENCE_END` 1500 ms minus `VAD_OFFLINE_HANGOVER_MS` 450 ms) after the last speech -> `Silence`; exactly 60 s -> `MaxLen`. `MaxLen` wins
    /// over the other two when speech is still arriving at 60 s.
    pub fn decide(
        _started: Instant,
        _last_speech: Option<Instant>,
        _now: Instant,
    ) -> Option<End> {
        todo!("step 2: SilenceWatch::decide")
    }
}
