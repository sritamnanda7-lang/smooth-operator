// SPDX-License-Identifier: FSL-1.1-MIT
//! Talk timers (founder Q2). `SilenceWatch::decide` is pure. Tests: `so/tests/stt_tests.rs` (T19).
use super::{MAX_TALK, NO_SPEECH, SILENCE_END};
use crate::audio_toolkit::vad::VAD_OFFLINE_HANGOVER_MS;
use std::time::{Duration, Instant};

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
    /// What to do about a running talk, given when it started, when speech was last heard and the
    /// time now. `last_speech` older than `started` (a stale tap from an earlier talk) counts as no
    /// speech. Limits are inclusive. The VAD already keeps a 450 ms tail after speech, so the quiet
    /// needed is `SILENCE_END` minus that tail (1050 ms). `MaxLen` wins over the other two.
    pub fn decide(started: Instant, last_speech: Option<Instant>, now: Instant) -> Option<End> {
        if now.saturating_duration_since(started) >= MAX_TALK {
            return Some(End::MaxLen);
        }
        let quiet = SILENCE_END.saturating_sub(Duration::from_millis(VAD_OFFLINE_HANGOVER_MS));
        match last_speech.filter(|last| *last >= started) {
            None => (now.saturating_duration_since(started) >= NO_SPEECH).then_some(End::NoSpeech),
            Some(last) => (now.saturating_duration_since(last) >= quiet).then_some(End::Silence),
        }
    }
}
