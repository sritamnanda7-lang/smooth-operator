// SPDX-License-Identifier: FSL-1.1-MIT
//! SpeechSink: turns talk events into recording, transcription and panel text. Tests:
//! `so/tests/stt_tests.rs` (T1-T18, T27).
//!
//! STEP-2 STUBS: `SpeechSink::new`, `on_event` and `heard_line` are `todo!()`.
//!
//! Contract the tests check (details in draft/step-2-plan.md "Behaviour spec"):
//! - `on_event` only pushes `(event, talk_id)` into a channel and returns (<5 ms). One worker thread
//!   (`so-speech`) runs the recorder calls and preload in event order; each transcription runs on its
//!   own short-lived thread.
//! - `talk_id` goes up by 1 at every `Started`. `Armed`/`Disarmed` never write to the panel. Panel writes
//!   go through `TaggedPanel`, tagged with the talk id (newest talk owns the panel).
//! - Armed: model `Ready` -> `Transcriber::preload()` once, and (MIC_OPEN_AT = Down) `Recorder::start()`.
//!   Model `Missing`/`Downloading` -> nothing. A failed `start()` shows nothing yet; the Warn
//!   `MIC_FAILED_TEXT` is shown at `Started`.
//! - Disarmed / Cancelled: `Recorder::cancel()`, no transcribe. Cancelled also hides the panel.
//! - Started: model missing/downloading -> Warn `MODEL_MISSING_TEXT` / `MODEL_DOWNLOADING_TEXT`, mic never
//!   opened. Otherwise `panel::action_for(Started)` text; talk timers start (free-talk: silence + no-speech +
//!   60 s; push-to-talk: 60 s only), polling the injected clock every 100 ms; the end is requested once
//!   through `Stopper::force_stop(talk_id)`.
//! - Stopped: `Recorder::stop()`; no samples -> `DIDNT_CATCH_TEXT` and no transcribe; else
//!   `TRANSCRIBING_TEXT`, then `transcribe` -> `heard_line` (Info) / `DIDNT_CATCH_TEXT` (blank text) /
//!   `SPEECH_FAILED_TEXT` (Warn, no error detail). "Heard"/"Didn't catch" hides after `HEARD_SECS`.
use crate::so::hotkey::session::TalkEvent;
use crate::so::hotkey::watcher::{Clock, TalkSink};
use crate::so::panel::PanelOut;
use crate::so::settings::HotkeyChoice;
use std::sync::Arc;
use std::time::Instant;

pub const MODEL_MISSING_TEXT: &str = "Speech model not downloaded: open Settings → Models";
pub const MODEL_DOWNLOADING_TEXT: &str = "Speech model still downloading";
pub const TRANSCRIBING_TEXT: &str = "Transcribing…";
pub const DIDNT_CATCH_TEXT: &str = "Didn't catch that";
pub const SPEECH_FAILED_TEXT: &str = "Speech failed: try again";
pub const MIC_FAILED_TEXT: &str = "Microphone could not start: check Settings → Privacy → Microphone";
pub const HEARD_PREFIX: &str = "Heard: ";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModelState {
    Missing,
    Downloading,
    Ready,
}

/// Real = `AudioRecordingManager`. Errors are plain text and never shown to the user.
pub trait Recorder: Send + Sync {
    fn start(&self) -> Result<(), String>;
    /// Stops and returns the samples (16 kHz mono). `None` = nothing was recorded.
    fn stop(&self) -> Option<Vec<f32>>;
    /// Stops and drops the samples.
    fn cancel(&self);
}

/// Real = `TranscriptionManager`.
pub trait Transcriber: Send + Sync {
    fn downloaded(&self) -> ModelState;
    /// Start a background model load if none is loaded or running. Returns at once.
    fn preload(&self);
    /// Waits for a load in progress. Errors are plain text and never shown to the user.
    fn transcribe(&self, samples: Vec<f32>) -> Result<String, String>;
}

/// Last instant a speech frame passed the VAD.
pub trait SpeechTap: Send + Sync {
    fn last_speech(&self) -> Option<Instant>;
}

/// Asks the hotkey watcher to end talk number `seq` (real = `Msg::ForceStop(seq)`).
pub trait Stopper: Send + Sync {
    fn force_stop(&self, seq: u64);
}

/// STEP-2 STUB (`todo!()`): the panel line for a transcript: `"Heard: "` + the text trimmed, control
/// characters (and newlines) turned into spaces, at most 200 chars in all; if cut, the last char is `…`.
/// Quotes, backslashes, `</script>` and U+2028 are kept (the panel's JS encoding handles them).
pub fn heard_line(_text: &str) -> String {
    todo!("step 2: heard_line")
}

pub struct SpeechSink {
    _private: (),
}

impl SpeechSink {
    /// STEP-2 STUB (`todo!()`): spawns the `so-speech` worker.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        _recorder: Arc<dyn Recorder>,
        _transcriber: Arc<dyn Transcriber>,
        _panel: Arc<dyn PanelOut>,
        _clock: Arc<dyn Clock>,
        _tap: Arc<dyn SpeechTap>,
        _stopper: Arc<dyn Stopper>,
        _hotkey: Box<dyn Fn() -> HotkeyChoice + Send + Sync>,
    ) -> Self {
        todo!("step 2: SpeechSink::new")
    }
}

impl TalkSink for SpeechSink {
    /// STEP-2 STUB (`todo!()`).
    fn on_event(&self, _event: TalkEvent) {
        todo!("step 2: SpeechSink::on_event")
    }
}
