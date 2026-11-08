// SPDX-License-Identifier: FSL-1.1-MIT
//! `Recorder` over `AudioRecordingManager` (binding `so-talk`, `VadPolicy::Offline`) and
//! `Transcriber` over `TranscriptionManager`. Never writes audio or text to disk and never logs
//! text: only `metrics` numbers leave this file.
use super::metrics;
use super::sink::{ModelState, Recorder, Stopper, Transcriber};
use super::{model, BINDING};
use crate::audio_toolkit::VadPolicy;
use crate::managers::audio::{AudioRecordingManager, MicrophoneMode};
use crate::managers::model::ModelManager;
use crate::managers::transcription::TranscriptionManager;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// How long the speech worker waits for the first audio chunk before treating the microphone as failed.
pub const READY_TIMEOUT: Duration = Duration::from_secs(2);

/// Run `wait` (a call that may block for ever) on a helper thread and give it `timeout`. False if it
/// answered false or did not answer in time. The helper ends by itself when the call returns.
pub fn wait_ready(wait: impl FnOnce() -> bool + Send + 'static, timeout: Duration) -> bool {
    let (tx, rx) = mpsc::channel();
    let spawned = std::thread::Builder::new().name("so-mic-ready".into()).spawn(move || {
        let _ = tx.send(wait());
    });
    spawned.is_ok() && rx.recv_timeout(timeout).unwrap_or(false)
}

/// Switches the microphone to open-only-during-a-talk, live (Handy picks the mode once, at
/// construction, before our setup runs).
pub trait MicModeSwitch {
    fn force_on_demand(&self);
}

impl MicModeSwitch for AudioRecordingManager {
    fn force_on_demand(&self) {
        let _ = self.update_mode(MicrophoneMode::OnDemand);
    }
}

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
                // Wait (at most READY_TIMEOUT) for the first audio chunk: from here on nothing said is
                // lost. This runs on the speech worker, never on the key watcher. No audio in time =
                // the microphone failed: close it and say so. (Cancelling drops Handy's sink, which
                // also ends the helper thread's wait.)
                let got_audio = wait_ready(move || readiness.wait(), READY_TIMEOUT);
                metrics::mic_ready(started.elapsed().as_millis() as u64, got_audio);
                if got_audio {
                    Ok(())
                } else {
                    self.rec.cancel_recording();
                    Err("no audio from the microphone".to_string())
                }
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
