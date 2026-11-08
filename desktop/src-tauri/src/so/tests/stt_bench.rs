// SPDX-License-Identifier: FSL-1.1-MIT
//! Step 2 model benchmark harness (plan sub-step 2). `#[ignore]`: needs real models and audio that are
//! never in the repo. COMPILE-ONLY for the tester; the builder finishes the details (RSS, wrong-language
//! count column) and Claude runs it.
//!
//! Run (release, one thread):
//! `SO_BENCH_DIR=~/so-bench HANDY_TRANSCRIBE_WORKER_EXE=target/release/handy \
//!  cargo test --release --lib stt_bench -- --ignored --nocapture --test-threads=1`
//!
//! Layout of `SO_BENCH_DIR` (outside both repos):
//! - `models/<name>.gguf`   one file per candidate (name = label in the report)
//! - `manifest.tsv`         `file<TAB>accent<TAB>reference text` (paths relative to the dir; stays local)
//! - `results.md`           written here: labels and numbers only (T26)
use crate::audio_toolkit::decode_audio_file;
use crate::engine_supervisor::{DeviceSelector, EngineSupervisor, LoadSpec};
use crate::so::stt::wer::BenchAcc;
use std::fmt::Write as _;
use std::path::PathBuf;
use std::time::Instant;
use transcribe_cpp::{Backend, RunOptions};

struct Clip {
    path: PathBuf,
    accent: String,
    reference: String,
}

fn bench_dir() -> PathBuf {
    PathBuf::from(std::env::var_os("SO_BENCH_DIR").expect("set SO_BENCH_DIR"))
}

fn manifest(dir: &std::path::Path) -> Vec<Clip> {
    let text = std::fs::read_to_string(dir.join("manifest.tsv")).expect("read manifest.tsv");
    text.lines()
        .filter(|l| !l.trim().is_empty() && !l.starts_with('#'))
        .map(|l| {
            let mut c = l.splitn(3, '\t');
            let file = c.next().expect("file column");
            Clip {
                path: dir.join(file),
                accent: c.next().expect("accent column").to_string(),
                reference: c.next().expect("reference column").to_string(),
            }
        })
        .collect()
}

fn models(dir: &std::path::Path) -> Vec<(String, PathBuf)> {
    let mut v: Vec<(String, PathBuf)> = std::fs::read_dir(dir.join("models"))
        .expect("read models/")
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "gguf"))
        .map(|p| (p.file_stem().unwrap().to_string_lossy().to_string(), p))
        .collect();
    v.sort();
    v
}

#[test]
#[ignore]
fn stt_bench() {
    let dir = bench_dir();
    let clips = manifest(&dir);
    assert!(!clips.is_empty(), "manifest.tsv has no rows");
    let mut acc = BenchAcc::new();
    let mut extra = String::new(); // numbers only: load ms, cold ms, wrong-language count
    for (name, path) in models(&dir) {
        let engine = EngineSupervisor::new(true);
        let t = Instant::now();
        engine
            .load(LoadSpec { path, backend: Backend::Auto, device: DeviceSelector::Auto })
            .expect("load model");
        let load_ms = t.elapsed().as_millis() as u64;
        let (mut cold_ms, mut wrong_lang) = (0u64, 0u64);
        for (i, clip) in clips.iter().enumerate() {
            let pcm = decode_audio_file(&clip.path).expect("decode clip");
            let t = Instant::now();
            let out = engine.transcribe(pcm, RunOptions::default()).expect("transcribe");
            let ms = t.elapsed().as_millis() as u64;
            if i == 0 {
                cold_ms = ms;
            }
            if out.language.as_deref().is_some_and(|l| l != "en") {
                wrong_lang += 1;
            }
            acc.add(&name, &clip.accent, &clip.reference, &out.text, ms);
        }
        // TODO(builder): loaded worker RSS in MB (ps on the worker pid) as one more number here.
        let _ = writeln!(extra, "| {name} | load_ms={load_ms} | cold_first_ms={cold_ms} | wrong_language={wrong_lang} |");
        engine.unload().wait();
    }
    let report = format!("{}\n\n{}", acc.render(), extra);
    std::fs::write(dir.join("results.md"), &report).expect("write results.md");
    println!("{report}");
}
