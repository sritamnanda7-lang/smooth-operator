// SPDX-License-Identifier: FSL-1.1-MIT
//! Step 2 model benchmark harness (plan sub-step 2). `#[ignore]`: needs real models and audio that are
//! never in the repo. `#[ignore]` = a test that only runs when asked for with `-- --ignored`.
//!
//! Build the worker once, then run (release, one thread):
//! `cargo build --release && SO_BENCH_DIR=~/so-bench \
//!  cargo test --release --lib stt_bench -- --ignored --nocapture --test-threads=1`
//! (the engine runs models in a child process, a copy of this app's binary: the test finds it at
//! `target/release/handy`, or at `HANDY_TRANSCRIBE_WORKER_EXE`).
//!
//! Layout of `SO_BENCH_DIR` (outside both repos, nothing is copied or downloaded here):
//! - `models/<name>.gguf`  one file per candidate (the file name is the label in the report)
//! - `indian/`  founder's `01.m4a`..`31.m4a`; the sentence for file N comes from the row with ID N of
//!   `SO_BENCH_REFS` (default `~/Working/draft/step-2-recording-script.md`, a `| ID | sentence |` table);
//!   or, if the folder has a `refs.tsv`, from that
//! - `us/`, `au/`  16 kHz mono WAVs plus `refs.tsv`: `path<TAB>sentence` (path relative to the folder)
//! - `results.md`  written here: labels and numbers only (T26)
//!
//! `.m4a`, `.mp3` and `.wav` are decoded in memory (no temp files, no converter tool).
use crate::audio_toolkit::decode_audio_file;
use crate::engine_supervisor::{DeviceSelector, EngineSupervisor, LoadSpec};
use crate::so::stt::wer::BenchAcc;
use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Instant;
use transcribe_cpp::{Backend, RunOptions};

const ACCENTS: [&str; 3] = ["indian", "us", "au"];

struct Clip {
    accent: &'static str,
    pcm: Vec<f32>,
    reference: String,
}

fn bench_dir() -> PathBuf {
    PathBuf::from(std::env::var_os("SO_BENCH_DIR").expect("set SO_BENCH_DIR"))
}

/// The engine spawns the app binary as its worker. Default: the release build next to the test binary.
fn point_at_worker() {
    if std::env::var_os("HANDY_TRANSCRIBE_WORKER_EXE").is_some() {
        return;
    }
    let exe = std::env::current_exe().expect("test exe path");
    let worker = exe
        .parent()
        .and_then(Path::parent)
        .expect("target dir")
        .join("handy");
    assert!(
        worker.exists(),
        "worker {} missing: run `cargo build --release` first",
        worker.display()
    );
    std::env::set_var("HANDY_TRANSCRIBE_WORKER_EXE", worker);
}

/// `| 07 | Play |` rows of a markdown table -> (7, "Play"). The sentence may sit in quotes.
fn md_refs(path: &Path) -> BTreeMap<u32, String> {
    let text =
        std::fs::read_to_string(path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    text.lines()
        .filter_map(|l| {
            let mut cells = l.trim().trim_start_matches('|').split('|');
            let id = cells.next()?.trim().parse::<u32>().ok()?;
            let say = cells.next()?.trim().trim_matches('"').to_string();
            (!say.is_empty()).then_some((id, say))
        })
        .collect()
}

fn tsv_refs(path: &Path) -> BTreeMap<String, String> {
    let text =
        std::fs::read_to_string(path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    text.lines()
        .filter(|l| !l.trim().is_empty() && !l.starts_with('#'))
        .filter_map(|l| {
            let (file, sentence) = l.split_once('\t')?;
            let name = Path::new(file.trim())
                .file_name()?
                .to_string_lossy()
                .to_string();
            Some((name, sentence.trim().to_string()))
        })
        .collect()
}

fn is_audio(p: &Path) -> bool {
    p.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| matches!(e.to_lowercase().as_str(), "m4a" | "wav" | "mp3"))
}

fn load_clips(dir: &Path) -> Vec<Clip> {
    let mut clips = Vec::new();
    for accent in ACCENTS {
        let sub = dir.join(accent);
        if !sub.is_dir() {
            eprintln!("bench: no {accent}/ folder, skipped");
            continue;
        }
        let tsv = sub.join("refs.tsv");
        let by_name = tsv.exists().then(|| tsv_refs(&tsv));
        let by_id = (by_name.is_none() && accent == "indian").then(|| {
            let default = std::env::var_os("HOME")
                .map(PathBuf::from)
                .unwrap_or_default()
                .join("Working/draft/step-2-recording-script.md");
            md_refs(
                &std::env::var_os("SO_BENCH_REFS")
                    .map(PathBuf::from)
                    .unwrap_or(default),
            )
        });
        let mut files: Vec<PathBuf> = std::fs::read_dir(&sub)
            .expect("read accent folder")
            .flatten()
            .map(|e| e.path())
            .filter(|p| is_audio(p))
            .collect();
        files.sort();
        let mut skipped = 0;
        for f in files {
            let name = f.file_name().unwrap().to_string_lossy().to_string();
            let reference = match (&by_name, &by_id) {
                (Some(m), _) => m.get(&name).cloned(),
                (None, Some(m)) => f
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .and_then(|s| s.parse::<u32>().ok())
                    .and_then(|id| m.get(&id).cloned()),
                _ => None,
            };
            let Some(reference) = reference else {
                skipped += 1;
                continue;
            };
            let pcm =
                decode_audio_file(&f).unwrap_or_else(|e| panic!("decode {}: {e}", f.display()));
            clips.push(Clip {
                accent,
                pcm,
                reference,
            });
        }
        eprintln!(
            "bench: {accent}: {} clips, {skipped} without a reference (skipped)",
            clips.iter().filter(|c| c.accent == accent).count()
        );
    }
    clips
}

fn models(dir: &Path) -> Vec<(String, PathBuf)> {
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

/// Resident memory in MB of this process's child processes (the engine worker), read with `ps`.
/// RSS = memory pages currently in RAM. 0 where `ps`/`pgrep` are missing.
fn child_rss_mb() -> u64 {
    let me = std::process::id().to_string();
    let Ok(kids) = Command::new("pgrep").args(["-P", &me]).output() else {
        return 0;
    };
    let pids: Vec<String> = String::from_utf8_lossy(&kids.stdout)
        .split_whitespace()
        .map(str::to_string)
        .collect();
    if pids.is_empty() {
        return 0;
    }
    let Ok(ps) = Command::new("ps")
        .args(["-o", "rss=", "-p", &pids.join(",")])
        .output()
    else {
        return 0;
    };
    let kb: u64 = String::from_utf8_lossy(&ps.stdout)
        .split_whitespace()
        .filter_map(|n| n.parse::<u64>().ok())
        .sum();
    kb / 1024
}

fn run_en() -> RunOptions {
    RunOptions {
        language: Some("en".into()),
        ..RunOptions::default()
    }
}

#[test]
#[ignore]
fn stt_bench() {
    point_at_worker();
    let dir = bench_dir();
    let clips = load_clips(&dir);
    assert!(
        !clips.is_empty(),
        "no clips with references under {}",
        dir.display()
    );
    let mut acc = BenchAcc::new();
    let mut extra = String::from("| model | load ms | cold first decode ms | loaded RSS MB | wrong language (auto-detect) | clips |\n|---|---|---|---|---|---|\n");
    let mut names = Vec::new();
    for (name, path) in models(&dir) {
        eprintln!("bench: model {name}");
        names.push(name.clone());
        let engine = EngineSupervisor::new(true);
        let t = Instant::now();
        engine
            .load(LoadSpec {
                path,
                backend: Backend::Auto,
                device: DeviceSelector::Auto,
            })
            .expect("load model");
        let load_ms = t.elapsed().as_millis() as u64;
        // Cold: the first decode after load (lazy setup happens here). Not scored.
        let t = Instant::now();
        let _ = engine.transcribe(clips[0].pcm.clone(), run_en());
        let cold_ms = t.elapsed().as_millis() as u64;
        let rss_mb = child_rss_mb();
        let mut wrong_lang = 0u64;
        for clip in &clips {
            // Language auto-detect pass (not timed): counts clips the model hears as non-English.
            // Models without language ID report none.
            if let Ok(out) = engine.transcribe(clip.pcm.clone(), RunOptions::default()) {
                if out
                    .language
                    .as_deref()
                    .is_some_and(|l| !l.starts_with("en"))
                {
                    wrong_lang += 1;
                }
            }
            // Scored pass: language forced to English, as the shipped app will do. Falls back to the
            // default options if a model refuses a language hint.
            let t = Instant::now();
            let out = engine
                .transcribe(clip.pcm.clone(), run_en())
                .or_else(|_| engine.transcribe(clip.pcm.clone(), RunOptions::default()))
                .expect("transcribe");
            let ms = t.elapsed().as_millis() as u64;
            acc.add(&name, clip.accent, &clip.reference, &out.text, ms);
        }
        let _ = writeln!(
            extra,
            "| {name} | {load_ms} | {cold_ms} | {rss_mb} | {wrong_lang} | {} |",
            clips.len()
        );
        engine.unload().wait();
    }
    assert!(
        !names.is_empty(),
        "no .gguf files in {}/models",
        dir.display()
    );
    // Decision-rule input: mean of the per-accent pooled WERs, equal weight.
    let mut mean = String::from("| model | mean WER % over accents |\n|---|---|\n");
    for n in &names {
        let v: Vec<f64> = ACCENTS
            .iter()
            .filter_map(|a| acc.pooled_wer_pct(n, a))
            .collect();
        if !v.is_empty() {
            let _ = writeln!(
                mean,
                "| {n} | {:.1} |",
                v.iter().sum::<f64>() / v.len() as f64
            );
        }
    }
    let report = format!(
        "# STT benchmark (numbers only)\n\nWarm decode = one clip, model already loaded. WER pooled per model and accent.\n\n{}\n{extra}\n{mean}",
        acc.render()
    );
    std::fs::write(dir.join("results.md"), &report).expect("write results.md");
    println!("{report}");
}
