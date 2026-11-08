// SPDX-License-Identifier: FSL-1.1-MIT
//! The only file under `so/stt/**` allowed to log (test S2). Functions here take integers and bools
//! only: ms, sample counts, char counts, state codes. Never text.

/// Microphone start: how long until the first audio arrived, and whether it did.
pub fn mic_ready(ms: u64, got_audio: bool) {
    log::info!("stt mic_ready ms={ms} got_audio={got_audio}");
}

/// A talk ended: samples recorded and whether the device sent only digital silence.
pub fn talk_stopped(samples: usize, input_silent: bool) {
    log::info!("stt talk_stopped samples={samples} input_silent={input_silent}");
}

/// A transcription finished. `model_state`: 0 missing, 1 downloading, 2 ready (at Stopped).
pub fn transcribed(ms: u64, chars: usize, model_state: u8, ok: bool) {
    log::info!("stt transcribed ms={ms} chars={chars} model_state={model_state} ok={ok}");
}

/// Free-talk or push-to-talk was ended by a timer. `reason`: 0 silence, 1 no speech, 2 max length.
pub fn timer_end(reason: u8) {
    log::info!("stt timer_end reason={reason}");
}
