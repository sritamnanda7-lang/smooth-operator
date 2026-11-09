// SPDX-License-Identifier: FSL-1.1-MIT
//! Step 2: speech-to-text. Tests: `so/tests/{stt_tests,stt_model_tests,wer_tests}.rs`.

pub mod metrics;
pub mod model;
pub mod real;
pub mod silence;
pub mod sink;
pub mod tap;

use crate::managers::audio::AudioRecordingManager;
use crate::managers::model::ModelManager;
use crate::managers::transcription::TranscriptionManager;
use crate::settings::{get_settings, write_settings, AppSettings, ModelUnloadTimeout};
use crate::so::hotkey::watcher::TalkSink;
use crate::so::rules::{SlowRouter, TranscriptRouter};
use crate::so::settings::{self as so_settings, Platform};
use std::sync::{Arc, OnceLock};
use std::time::Duration;
use tauri::{AppHandle, Manager};

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
/// Audio kept after the key is released (Handy's `extra_recording_buffer_ms`). People release the key
/// while still saying the last syllable; the founder's M7 logs show every talk was still in speech at
/// release. It delays the transcription start by this much.
pub const RELEASE_TAIL_MS: u64 = 150;
/// Recorder binding name.
pub const BINDING: &str = "so-talk";

/// The speech sink, once `setup` built it. The key watcher takes it from here when it starts.
static SINK: OnceLock<Arc<dyn TalkSink>> = OnceLock::new();

/// The sink the key watcher should use, if speech is set up.
pub fn talk_sink() -> Option<Arc<dyn TalkSink>> {
    SINK.get().cloned()
}

/// Force the launch settings on `settings` and return true if anything changed (false when already
/// right, so it is idempotent): `selected_model = model::SO_MODEL_ID`, `selected_language = "en"`,
/// `model_unload_timeout = Min2` (Handy's idle watcher then unloads 120-130 s after last use),
/// `always_on_microphone = false` (the mic is open only during a talk) and
/// `lazy_stream_close = false` (it closes at once after a talk) and
/// `extra_recording_buffer_ms = RELEASE_TAIL_MS`.
pub fn enforce_settings(settings: &mut AppSettings) -> bool {
    let mut changed = false;
    if settings.selected_model != model::SO_MODEL_ID {
        settings.selected_model = model::SO_MODEL_ID.to_string();
        changed = true;
    }
    if settings.selected_language != "en" {
        settings.selected_language = "en".to_string();
        changed = true;
    }
    if settings.model_unload_timeout != ModelUnloadTimeout::Min2 {
        settings.model_unload_timeout = ModelUnloadTimeout::Min2;
        changed = true;
    }
    if settings.extra_recording_buffer_ms != RELEASE_TAIL_MS {
        settings.extra_recording_buffer_ms = RELEASE_TAIL_MS;
        changed = true;
    }
    if settings.always_on_microphone {
        settings.always_on_microphone = false;
        changed = true;
    }
    if settings.lazy_stream_close {
        settings.lazy_stream_close = false;
        changed = true;
    }
    changed
}

/// Called once from our setup (after Handy's managers exist): enforce the settings, build the
/// speech sink and keep it for the key watcher. If a manager is missing, speech stays off and the
/// watcher falls back to the plain panel sink.
pub fn setup(app: &AppHandle) {
    let mut settings = get_settings(app);
    if enforce_settings(&mut settings) {
        write_settings(app, settings);
    }
    let (Some(rec), Some(tr), Some(models)) = (
        app.try_state::<Arc<AudioRecordingManager>>(),
        app.try_state::<Arc<TranscriptionManager>>(),
        app.try_state::<Arc<ModelManager>>(),
    ) else {
        return;
    };
    // Handy chose the microphone mode at construction, before our settings were forced: switch the
    // live manager to on-demand so the mic is open only during a talk.
    real::MicModeSwitch::force_on_demand(&**rec.inner());
    let app_for_hotkey = app.clone();
    let brain = app_brain(app);
    #[cfg(debug_assertions)]
    let brain = fake_brain().unwrap_or(brain);
    let sink = Arc::new(sink::SpeechSink::with_brain(
        Arc::new(real::RealRecorder::new(rec.inner().clone())),
        Arc::new(real::RealTranscriber::new(tr.inner().clone(), models.inner().clone())),
        crate::so::panel::panel_out(app),
        Arc::new(crate::so::hotkey::SystemClock),
        Arc::new(tap::StaticTap),
        Arc::new(real::WatcherStopper),
        Box::new(move || so_settings::load_hotkey(&app_for_hotkey, Platform::current())),
        fast_router(app),
        brain,
    ));
    #[cfg(debug_assertions)]
    start_debug_input(app, &sink);
    let _ = SINK.set(sink);
}

/// The AI for talks the fast rules pass through (step 4): Claude and Gemini, the key read from the macOS
/// Keychain on each call, the choice read from our settings file.
fn app_brain(app: &AppHandle) -> Arc<dyn SlowRouter> {
    use crate::so::brain::{brain::{Brain, Limits}, claude::ClaudeProvider, config, gemini::GeminiProvider, keys};
    let app = app.clone();
    Arc::new(Brain::new(
        vec![Arc::new(ClaudeProvider::new()), Arc::new(GeminiProvider::new())],
        Arc::new(keys::KeyringStore::new(keys::service())),
        Arc::new(move || config::load_choice(&app)),
        Arc::new(crate::so::hotkey::SystemClock),
        Arc::new(std::thread::sleep),
        Limits { max_in_flight: 2, max_per_hour: 120 },
    ))
}

/// Debug builds only: `SO_BRAIN_FAKE=1` swaps in the fake provider (no network, no key).
#[cfg(debug_assertions)]
fn fake_brain() -> Option<Arc<dyn SlowRouter>> {
    use crate::so::brain::fake;
    std::env::var(fake::ENV_FLAG).is_ok_and(|v| v == "1").then(fake::front)
}

/// The router that handles exact media and open/switch phrases (step 3). Other platforms keep step 2's
/// pass-through until their own effects exist.
#[cfg(target_os = "macos")]
fn fast_router(app: &AppHandle) -> Arc<dyn TranscriptRouter> {
    use crate::so::rules::{real, router::FastRouter};
    Arc::new(
        FastRouter::new(
            Arc::new(real::EnigoMediaKeys::new(app.clone())),
            Arc::new(real::SystemCatalog),
            Arc::new(real::OpenLauncher),
            Arc::new(crate::so::hotkey::SystemClock),
        )
        .not_found_to_brain(),
    )
}

#[cfg(not(target_os = "macos"))]
fn fast_router(_app: &AppHandle) -> Arc<dyn TranscriptRouter> {
    Arc::new(crate::so::rules::PassThrough)
}

/// Debug builds only, and only when launched with `SO_DEBUG_INPUT=1` (read once, here): opens the
/// typed-phrase socket in the app data folder.
#[cfg(debug_assertions)]
fn start_debug_input(app: &AppHandle, sink: &Arc<sink::SpeechSink>) {
    let enabled = std::env::var(crate::so::rules::debug_input::ENV_FLAG).is_ok_and(|v| v == "1");
    let Ok(dir) = app.path().app_data_dir() else { return };
    let sink = sink.clone();
    let inject = Arc::new(move |text: String| sink.inject_transcript(text));
    let _ = crate::so::rules::debug_input::start(&dir, enabled, inject);
}

/// Benchmark scoring only (the `#[ignore]` bench and its tests); not part of the shipped app.
#[cfg(test)]
pub mod wer;
