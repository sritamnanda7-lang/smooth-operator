// SPDX-License-Identifier: FSL-1.1-MIT
//! Gate 8: speech. Parakeet V3 through Handy's existing engine.
//! Preload on hotkey-down, record while held, transcribe on release, unload when idle.
//! The text goes to the panel only: never pasted, never stored. No audio is stored.
//! The log gets timings, memory and the text LENGTH, never the text.

use crate::audio_toolkit::audio::VadPolicy;
use crate::managers::audio::AudioRecordingManager;
use crate::managers::transcription::TranscriptionManager;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Manager};

/// Parakeet V3 file (Q8_0). Handy registers HF models under "<repo>/<file>", not the short
/// catalog id, so the registered id is looked up by file name (see `pick_model`).
const MODEL_FILE: &str = "parakeet-tdt-0.6b-v3-Q8_0.gguf";

/// Registered id of the downloaded Parakeet V3 model: `(id, filename, is_downloaded)` rows in,
/// the id of the first downloaded row with that file name out.
fn pick_model(rows: &[(String, String, bool)]) -> Option<String> {
    rows.iter().find(|(_, f, d)| *d && f == MODEL_FILE).map(|(id, _, _)| id.clone())
}
const BINDING: &str = "spike";
/// Spike value so a test run does not wait minutes. The product value is decided in step 2.
const IDLE_UNLOAD_SECS: u64 = 20;

static LAST_ACTIVITY_MS: AtomicU64 = AtomicU64::new(0);
static RECORDING: AtomicBool = AtomicBool::new(false);

fn now_ms() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}

fn touch() {
    LAST_ACTIVITY_MS.store(now_ms(), Ordering::Relaxed);
}

/// Hotkey went down: start loading the model (and the VAD) in the background.
pub fn on_down(app: &AppHandle) {
    touch();
    preload(app);
}

pub fn preload(app: &AppHandle) {
    let tm = Arc::clone(&app.state::<Arc<TranscriptionManager>>());
    if tm.is_model_loaded() {
        return;
    }
    // `try_start_loading` returns a guard only if no load is running. While the guard
    // lives the engine knows a load is in progress, so a transcribe call waits for it.
    let Some(guard) = tm.try_start_loading() else { return };
    let rm = Arc::clone(&app.state::<Arc<AudioRecordingManager>>());
    let app = app.clone();
    std::thread::spawn(move || {
        let _guard = guard;
        let _ = rm.preload_vad();
        super::harness::with_rss("g8", "rss_preload", None, super::rss::tree_kb());
        let t = Instant::now();
        let rows: Vec<(String, String, bool)> = app
            .state::<Arc<crate::managers::model::ModelManager>>()
            .get_available_models()
            .into_iter()
            .map(|m| (m.id, m.filename, m.is_downloaded))
            .collect();
        let result = match pick_model(&rows) {
            Some(id) => tm.load_model(&id),
            None => Err(anyhow::anyhow!("Parakeet V3 ({MODEL_FILE}) is not downloaded")),
        };
        match result {
            Ok(()) => {
                let ms = t.elapsed().as_millis();
                super::harness::with_rss("g8", "model_loaded", Some(ms), super::rss::tree_kb());
                super::panel::say(&app, &format!("model loaded {ms} ms"));
            }
            Err(e) => {
                super::harness::failed("g8", "model_load_failed", &format!("{e:#}"));
                super::panel::say(&app, &format!("model load failed: {e:#}"));
            }
        }
    });
}

/// Held for 0.3 s: open the microphone.
pub fn on_hold(app: &AppHandle) {
    touch();
    let rm = Arc::clone(&app.state::<Arc<AudioRecordingManager>>());
    match rm.try_start_recording(BINDING, VadPolicy::Disabled) {
        Ok(_ready) => {
            RECORDING.store(true, Ordering::SeqCst);
            super::harness::event("g8", "recording_start");
        }
        Err(e) => {
            super::harness::failed("g8", "recording_start_failed", &format!("{e:#}"));
            super::panel::say(app, &format!("recording failed: {e:#}"));
        }
    }
}

/// Hotkey released after a hold: stop, transcribe, show the text. `released` is when the
/// key came up, so the logged time is the real release-to-text delay.
pub fn on_release(app: &AppHandle, released: Instant) {
    if !RECORDING.swap(false, Ordering::SeqCst) {
        return;
    }
    let app = app.clone();
    std::thread::spawn(move || {
        let rm = Arc::clone(&app.state::<Arc<AudioRecordingManager>>());
        let tm = Arc::clone(&app.state::<Arc<TranscriptionManager>>());
        let Some(samples) = rm.stop_recording(BINDING, rm.cancel_generation()) else {
            super::harness::event("g8", "no_audio");
            return;
        };
        match tm.transcribe(samples) {
            Ok(text) => {
                let ms = released.elapsed().as_millis();
                super::harness::timed_n("g8", "release_to_text", ms, text.chars().count() as u64);
                super::panel::say(&app, &format!("{ms} ms: {text}"));
            }
            Err(e) => {
                super::harness::failed("g8", "transcribe_failed", &format!("{e:#}"));
                super::panel::say(&app, &format!("transcribe failed: {e:#}"));
            }
        }
        touch();
    });
}

pub fn unload(app: &AppHandle) {
    let tm = Arc::clone(&app.state::<Arc<TranscriptionManager>>());
    if !tm.is_model_loaded() {
        return;
    }
    let t = Instant::now();
    let _ = tm.unload_model(); // waits until the worker process has exited
    let ms = t.elapsed().as_millis();
    super::harness::with_rss("g8", "rss_after_unload", Some(ms), super::rss::tree_kb());
    super::panel::say(app, &format!("model unloaded {ms} ms"));
}

/// Unload the model after `IDLE_UNLOAD_SECS` without use (checks every 2 s).
pub fn start_idle_watcher(app: AppHandle) {
    touch();
    std::thread::spawn(move || loop {
        std::thread::sleep(Duration::from_secs(2));
        let idle = now_ms().saturating_sub(LAST_ACTIVITY_MS.load(Ordering::Relaxed));
        if idle > IDLE_UNLOAD_SECS * 1000 && !RECORDING.load(Ordering::SeqCst) {
            unload(&app);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::pick_model;

    fn row(id: &str, file: &str, d: bool) -> (String, String, bool) {
        (id.into(), file.into(), d)
    }

    #[test]
    fn finds_registered_id_by_file_name() {
        let rows = vec![
            row("whisper-small", "ggml-small.bin", true),
            row("handy-computer/parakeet-tdt-0.6b-v3-gguf/parakeet-tdt-0.6b-v3-Q8_0.gguf", MODEL_FILE_T, true),
        ];
        assert_eq!(pick_model(&rows).as_deref(), Some("handy-computer/parakeet-tdt-0.6b-v3-gguf/parakeet-tdt-0.6b-v3-Q8_0.gguf"));
    }

    #[test]
    fn ignores_not_downloaded_and_missing() {
        assert_eq!(pick_model(&[row("x", MODEL_FILE_T, false)]), None);
        assert_eq!(pick_model(&[]), None);
    }

    const MODEL_FILE_T: &str = "parakeet-tdt-0.6b-v3-Q8_0.gguf";
}
