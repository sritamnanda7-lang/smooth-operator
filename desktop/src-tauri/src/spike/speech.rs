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

/// Parakeet V3 in Handy's model catalog.
const MODEL_ID: &str = "parakeet-tdt-0.6b-v3";
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
        match tm.load_model(MODEL_ID) {
            Ok(()) => {
                let ms = t.elapsed().as_millis();
                super::harness::with_rss("g8", "model_loaded", Some(ms), super::rss::tree_kb());
                super::panel::say(&app, &format!("model loaded {ms} ms"));
            }
            Err(_) => {
                super::harness::event("g8", "model_load_failed");
                super::panel::say(&app, "model load failed (download Parakeet V3 in Settings > Models)");
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
        Err(_) => {
            super::harness::event("g8", "recording_start_failed");
            super::panel::say(app, "microphone failed (permission?)");
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
            Err(_) => {
                super::harness::event("g8", "transcribe_failed");
                super::panel::say(&app, "transcribe failed");
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
