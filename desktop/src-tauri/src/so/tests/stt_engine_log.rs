// SPDX-License-Identifier: FSL-1.1-MIT
//! Step 2 T28 (S1): no transcript reaches the app log, even at Trace, through the real engine worker.
//! `#[ignore]`: needs a model and a local WAV. COMPILE-ONLY for the tester; Claude runs it before review.
//!
//! Run alone (it installs the process-wide logger; do not mix with `stt_tests`):
//! `HANDY_TRANSCRIBE_WORKER_EXE=target/debug/handy HANDY_TEST_MODEL=<gguf> \
//!  SO_BENCH_DIR=~/so-bench SO_SENTINEL=<word spoken in sentinel.wav> \
//!  cargo test --lib stt_engine_log -- --ignored --nocapture --test-threads=1`
//!
//! `SO_BENCH_DIR/sentinel.wav` is a local 16 kHz mono recording of a sentence containing the sentinel word.
use crate::audio_toolkit::decode_audio_file;
use crate::engine_supervisor::{DeviceSelector, EngineSupervisor, LoadSpec};
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::Duration;
use transcribe_cpp::{Backend, RunOptions};

struct Capture;
static RECORDS: Mutex<Vec<String>> = Mutex::new(Vec::new());
static CAPTURE: Capture = Capture;
impl log::Log for Capture {
    fn enabled(&self, _: &log::Metadata) -> bool {
        true
    }
    fn log(&self, r: &log::Record) {
        RECORDS.lock().unwrap().push(format!("{} {}", r.target(), r.args()));
    }
    fn flush(&self) {}
}

#[test]
#[ignore]
fn t28_no_transcript_reaches_the_app_log_at_trace() {
    log::set_logger(&CAPTURE).expect("another logger is installed: run this test alone");
    log::set_max_level(log::LevelFilter::Trace); // the worker follows the app level
    let sentinel = std::env::var("SO_SENTINEL").expect("set SO_SENTINEL").to_lowercase();
    let bench = PathBuf::from(std::env::var_os("SO_BENCH_DIR").expect("set SO_BENCH_DIR"));
    let model = PathBuf::from(std::env::var_os("HANDY_TEST_MODEL").expect("set HANDY_TEST_MODEL"));
    let pcm = decode_audio_file(&bench.join("sentinel.wav")).expect("decode sentinel.wav");

    let engine = EngineSupervisor::new(true);
    engine
        .load(LoadSpec { path: model, backend: Backend::Auto, device: DeviceSelector::Auto })
        .expect("load model");
    let out = engine.transcribe(pcm, RunOptions::default()).expect("transcribe");
    // Positive control: the model really heard the sentinel, so its absence from the log means something.
    assert!(out.text.to_lowercase().contains(&sentinel), "positive control failed: sentinel not decoded");
    // Worker stderr is forwarded by a reader thread: give it time.
    std::thread::sleep(Duration::from_millis(800));
    engine.unload().wait();
    let records = RECORDS.lock().unwrap().clone();
    assert!(!records.is_empty(), "no log records captured: the capture is broken");
    let leaks: Vec<&String> = records.iter().filter(|r| r.to_lowercase().contains(&sentinel)).collect();
    assert!(leaks.is_empty(), "the transcript reached the log in {} record(s)", leaks.len());
}
