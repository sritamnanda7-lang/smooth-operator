// SPDX-License-Identifier: FSL-1.1-MIT
//! `Recorder` over `AudioRecordingManager` (binding `so-talk`, `VadPolicy::Offline`) and
//! `Transcriber` over `TranscriptionManager`. Never writes audio or text to disk and never logs
//! text: only `metrics` numbers leave this file.
use super::metrics;
use super::sink::{ModelState, Recorder, Stopper, Transcriber};
use super::{model, BINDING};
use crate::audio_toolkit::VadPolicy;
use crate::managers::audio::AudioRecordingManager;
use crate::managers::model::ModelManager;
use crate::managers::transcription::TranscriptionManager;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;

pub struct RealRecorder {
    rec: Arc<AudioRecordingManager>,
    /// Handy's cancel counter at the start of the talk: a cancel since then drops the samples.
    cancel_gen: AtomicU64,
}

impl RealRecorder {
    pub fn new(rec: Arc<AudioRecordingManager>) -> Self {
        RealRecorder { rec, cancel_gen: AtomicU64::new(0) }
    }
}

impl Recorder for RealRecorder {
    fn start(&self) -> Result<(), String> {
        let started = Instant::now();
        match self.rec.try_start_recording(BINDING, VadPolicy::Offline) {
            Ok(readiness) => {
                self.cancel_gen.store(self.rec.cancel_generation(), Ordering::SeqCst);
                // Wait for the first audio chunk: from here on nothing said is lost. This runs on the
                // speech worker, never on the key watcher.
                let got_audio = readiness.wait();
                metrics::mic_ready(started.elapsed().as_millis() as u64, got_audio);
                Ok(())
            }
            Err(e) => Err(e.to_string()),
        }
    }

    fn stop(&self) -> Option<Vec<f32>> {
        let samples = self.rec.stop_recording(BINDING, self.cancel_gen.load(Ordering::SeqCst));
        metrics::talk_stopped(samples.as_ref().map_or(0, Vec::len), self.rec.last_input_silent());
        samples
    }

    fn cancel(&self) {
        self.rec.cancel_recording();
    }
}

pub struct RealTranscriber {
    tm: Arc<TranscriptionManager>,
    models: Arc<ModelManager>,
}

impl RealTranscriber {
    pub fn new(tm: Arc<TranscriptionManager>, models: Arc<ModelManager>) -> Self {
        RealTranscriber { tm, models }
    }
}

impl Transcriber for RealTranscriber {
    fn downloaded(&self) -> ModelState {
        match self.models.get_model_info(model::SO_MODEL_ID) {
            Some(m) if m.is_downloaded => ModelState::Ready,
            Some(m) if m.is_downloading => ModelState::Downloading,
            _ => ModelState::Missing,
        }
    }

    /// Starts a background load unless the model is loaded or loading (returns at once).
    fn preload(&self) {
        self.tm.initiate_model_load(false);
    }

    fn transcribe(&self, samples: Vec<f32>) -> Result<String, String> {
        // Make sure a load is under way even if `preload` never ran; `transcribe` then waits for it.
        self.tm.initiate_model_load(false);
        self.tm.transcribe(samples).map_err(|e| e.to_string())
    }
}

/// Asks the key watcher to end talk `seq`.
pub struct WatcherStopper;

impl Stopper for WatcherStopper {
    fn force_stop(&self, seq: u64) {
        crate::so::hotkey::force_stop(seq);
    }
}
