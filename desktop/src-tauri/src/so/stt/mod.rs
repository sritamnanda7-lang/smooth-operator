// SPDX-License-Identifier: FSL-1.1-MIT
//! Step 2: speech-to-text. Tests: `so/tests/{stt_tests,stt_model_tests,wer_tests}.rs`.
//!
//! STEP-2 STUBS: the modules below declare the interfaces the tests use. Bodies marked `todo!()` are
//! for the builder (sub-step 4/5). Remove the `expect` below when the code is wired in (step-1 check S5
//! forbids a blanket allow of dead code under `so/`; unused stubs only warn outside tests).
#![cfg_attr(not(test), expect(dead_code))]

pub mod metrics;
pub mod model;
pub mod real;
pub mod silence;
pub mod sink;
pub mod tap;
pub mod wer;

use crate::settings::AppSettings;
use std::time::Duration;
use tauri::AppHandle;

/// Where the microphone opens. One constant (test S8): `Down` = on the clean hotkey press (founder Q1),
/// `Started` = only after the 300 ms hold.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MicOpenAt {
    Down,
    Started,
}

pub const MIC_OPEN_AT: MicOpenAt = MicOpenAt::Down;
/// Free-talk ends this long after the last speech (the VAD hangover is part of it).
pub const SILENCE_END: Duration = Duration::from_millis(1500);
/// No speech at all within this time of a free-talk start -> end, "Didn't catch that".
pub const NO_SPEECH: Duration = Duration::from_secs(8);
/// Every talk is ended and transcribed at this length.
pub const MAX_TALK: Duration = Duration::from_secs(60);
/// How long "Heard: ..." stays visible.
pub const HEARD_SECS: u64 = 4;
/// Recorder binding name.
pub const BINDING: &str = "so-talk";

/// STEP-2 STUB (`todo!()`): force the launch settings on `settings` and return true if anything
/// changed (false when already right, so it is idempotent): `selected_model = model::SO_MODEL_ID`,
/// `selected_language = "en"`, `model_unload_timeout = Min2`, `always_on_microphone = false`,
/// `lazy_stream_close = false`.
pub fn enforce_settings(_settings: &mut AppSettings) -> bool {
    todo!("step 2: enforce_settings")
}

/// STEP-2 STUB (`todo!()`): called once from Handy's setup: enforce settings, build the SpeechSink and
/// its worker, and hand it to the hotkey watcher.
pub fn setup(_app: &AppHandle) {
    todo!("step 2: stt::setup")
}
