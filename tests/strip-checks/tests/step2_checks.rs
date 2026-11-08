// SPDX-License-Identifier: FSL-1.1-MIT
//! Step 2 static checks S1-S9 (speech-to-text). Read files only; never compile app code. Same style as step1_checks.rs.
//! Run: `cargo test --manifest-path tests/strip-checks/Cargo.toml`. Numbers match draft/step-2-plan.md.

use serde_json::Value;
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..").canonicalize().unwrap()
}
fn p(rel: &str) -> PathBuf {
    root().join(rel)
}
fn read(rel: &str) -> String {
    fs::read_to_string(p(rel)).unwrap_or_else(|e| panic!("cannot read {rel}: {e}"))
}
fn json(rel: &str) -> Value {
    serde_json::from_str(&read(rel)).unwrap_or_else(|e| panic!("bad json {rel}: {e}"))
}
const SKIP_DIRS: [&str; 6] = ["target", "node_modules", "dist", ".git", "gen", "strip-checks"];
fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(rd) = fs::read_dir(dir) else { return };
    for e in rd.flatten() {
        let path = e.path();
        let name = e.file_name().to_string_lossy().to_string();
        if path.is_dir() {
            if !SKIP_DIRS.contains(&name.as_str()) {
                walk(&path, out);
            }
        } else {
            out.push(path);
        }
    }
}
fn files_under(rel: &str, exts: &[&str]) -> Vec<PathBuf> {
    let mut v = vec![];
    walk(&p(rel), &mut v);
    v.retain(|f| exts.is_empty() || f.extension().map_or(false, |x| exts.contains(&x.to_string_lossy().as_ref())));
    v
}
fn rel(f: &Path) -> String {
    f.strip_prefix(root()).unwrap().to_string_lossy().to_string()
}
fn is_comment(line: &str) -> bool {
    let t = line.trim_start();
    t.starts_with("//") || t.starts_with("/*") || t.starts_with('*')
}
/// Non-comment lines containing any needle: (file, line no, text).
fn grep(dirs: &[&str], exts: &[&str], needles: &[&str]) -> Vec<String> {
    let mut hits = vec![];
    for d in dirs {
        for f in files_under(d, exts) {
            let Ok(txt) = fs::read_to_string(&f) else { continue };
            for (i, line) in txt.lines().enumerate() {
                if is_comment(line) {
                    continue;
                }
                if needles.iter().any(|n| line.contains(n)) {
                    hits.push(format!("{}:{}: {}", rel(&f), i + 1, line.trim()));
                }
            }
        }
    }
    hits
}
fn assert_none(what: &str, hits: Vec<String>) {
    assert!(hits.is_empty(), "{what}: {} hit(s), first: {:#?}", hits.len(), &hits[..hits.len().min(8)]);
}
/// Non-test part of a Rust file (drop everything from `#[cfg(test)]` on).
fn non_test(src: &str) -> String {
    src.split("#[cfg(test)]").next().unwrap().to_string()
}
fn count_code(src: &str, needle: &str) -> usize {
    src.lines().filter(|l| !is_comment(l)).map(|l| l.matches(needle).count()).sum()
}

const LIB: &str = "desktop/src-tauri/src/lib.rs";
const BUILD: &str = "desktop/src-tauri/build.rs";
const STT: &str = "desktop/src-tauri/src/so/stt";
const CAPS: &str = "desktop/src-tauri/capabilities";

/// Every line, comments included, of every file in the S1 scan scope.
fn scope_files() -> Vec<PathBuf> {
    let mut v = files_under("desktop/src", &[]);
    v.extend(files_under("desktop/src-tauri/src", &[]));
    v.extend(files_under(CAPS, &[]));
    v.push(p(BUILD));
    v.push(p("desktop/index.html"));
    for f in files_under("desktop/src-tauri", &["json"]) {
        let n = f.file_name().unwrap().to_string_lossy().to_string();
        if n.starts_with("tauri") && f.parent() == Some(&p("desktop/src-tauri")) {
            v.push(f);
        }
    }
    v
}

// ---- S1: no blob.handy.computer; allowlist = huggingface.co + local only ----
#[test]
fn s01_no_handy_blob_host_anywhere_in_the_scan_scope() {
    let mut hits = vec![];
    for f in scope_files() {
        let Ok(txt) = fs::read_to_string(&f) else { continue };
        for (i, line) in txt.lines().enumerate() {
            if line.contains("blob.handy.computer") {
                hits.push(format!("{}:{}: {}", rel(&f), i + 1, line.trim()));
            }
        }
    }
    assert_none("blob.handy.computer (comments and bindings.ts included)", hits);
}

#[test]
fn s01_net_allowlist_has_huggingface_but_no_handy_blob_or_hf_cdn_hosts() {
    let hosts: Vec<String> = read("tests/net-allowlist.txt")
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .map(String::from)
        .collect();
    assert!(hosts.iter().any(|h| h == "huggingface.co"), "huggingface.co must stay allowed");
    for gone in ["blob.handy.computer", "cdn-lfs.huggingface.co", "cdn-lfs.hf.co", "cas-bridge.xethub.hf.co"] {
        assert!(!hosts.iter().any(|h| h == gone), "{gone} must be removed from tests/net-allowlist.txt");
    }
}

// ---- S2: only metrics.rs under so/stt may log; its functions take integers and bools only ----
const LOG_NEEDLES: [&str; 11] = [
    "log::", "info!(", "debug!(", "warn!(", "error!(", "trace!(", "println!(", "eprintln!(", "print!(", "eprint!(", "dbg!(",
];

#[test]
fn s02_no_logging_or_printing_in_so_stt_except_metrics() {
    let mut hits = vec![];
    for f in files_under(STT, &["rs"]) {
        if f.file_name().unwrap() == "metrics.rs" {
            continue;
        }
        let Ok(txt) = fs::read_to_string(&f) else { continue };
        for (i, line) in non_test(&txt).lines().enumerate() {
            if !is_comment(line) && LOG_NEEDLES.iter().any(|n| line.contains(n)) {
                hits.push(format!("{}:{}: {}", rel(&f), i + 1, line.trim()));
            }
        }
    }
    assert_none("log/print macros in so/stt (outside metrics.rs)", hits);
}

#[test]
fn s02_metrics_functions_take_only_integers_and_bools() {
    const OK: [&str; 13] = ["u8", "u16", "u32", "u64", "usize", "i8", "i16", "i32", "i64", "isize", "bool", "f32", "f64"];
    let txt = non_test(&read(&format!("{STT}/metrics.rs")));
    for line in txt.lines().filter(|l| !is_comment(l)) {
        let t = line.trim();
        if !(t.starts_with("pub fn ") || t.starts_with("fn ") || t.starts_with("pub(crate) fn ")) {
            continue;
        }
        let args = &t[t.find('(').unwrap() + 1..t.rfind(')').unwrap_or(t.len())];
        for a in args.split(',').map(str::trim).filter(|a| !a.is_empty()) {
            let ty = a.split(':').nth(1).map(str::trim).unwrap_or("");
            assert!(OK.contains(&ty), "metrics fn takes a non-number/bool argument `{a}` in: {t}");
        }
    }
}

// ---- S3: no audio or temp-file writing ----
#[test]
fn s03_no_audio_or_temp_file_writes_in_so_or_the_recording_managers() {
    const N: [&str; 5] = ["save_wav_file", "WavWriter", "File::create", "temp_dir", "tempfile"];
    let mut files: Vec<PathBuf> = files_under("desktop/src-tauri/src/so", &["rs"])
        .into_iter()
        .filter(|f| !rel(f).contains("/so/tests/"))
        .collect();
    files.push(p("desktop/src-tauri/src/managers/audio.rs"));
    files.push(p("desktop/src-tauri/src/managers/transcription.rs"));
    let mut hits = vec![];
    for f in files {
        let txt = fs::read_to_string(&f).unwrap_or_default();
        for (i, line) in non_test(&txt).lines().enumerate() {
            if !is_comment(line) && N.iter().any(|n| line.contains(n)) {
                hits.push(format!("{}:{}: {}", rel(&f), i + 1, line.trim()));
            }
        }
    }
    assert_none("audio/temp-file writes", hits);
}

// ---- S4: the transcript is never logged ----
#[test]
fn s04_no_transcription_result_text_log_line() {
    assert_none(
        "\"Transcription result:\" log line",
        grep(&["desktop/src-tauri/src"], &["rs"], &["Transcription result:", "Transcription result: {"]),
    );
}

// ---- S5: so/stt uses only the one-shot path ----
#[test]
fn s05_so_stt_never_streams_or_emits() {
    assert_none(
        "start_stream / transcribe_recording / emit in so/stt",
        grep(&[STT], &["rs"], &["start_stream", "transcribe_recording", ".emit(", "emit("]),
    );
}

// ---- S6: the UI uses our two components ----
#[test]
fn s06_sidebar_uses_model_pane_and_onboarding_uses_so_model_step() {
    assert!(read("desktop/src/components/Sidebar.tsx").contains("so/settings/ModelPane"), "Sidebar must use so/settings/ModelPane");
    assert!(
        read("desktop/src/components/onboarding/OnboardingFlow.tsx").contains("so/onboarding/SoModelStep"),
        "OnboardingFlow must use so/onboarding/SoModelStep"
    );
    assert!(p("desktop/src/so/settings/ModelPane.tsx").exists());
    assert!(p("desktop/src/so/onboarding/SoModelStep.tsx").exists());
}

// ---- S7: capabilities after the model-table removal (step-1 checks S1-S13 run in step1_checks.rs) ----
fn granted() -> BTreeSet<String> {
    let v = json(&format!("{CAPS}/default.json"));
    v["permissions"].as_array().unwrap().iter().filter_map(|x| x.as_str().map(String::from)).collect()
}

#[test]
fn s07_the_commands_model_pane_needs_are_still_granted() {
    let g = granted();
    for c in ["allow-get-available-models", "allow-download-model", "allow-cancel-download", "allow-delete-model"] {
        assert!(g.contains(c), "{c} must stay granted (ModelPane)");
    }
}

#[test]
fn s07_commands_only_handys_model_table_used_are_not_granted() {
    // Reading of the plan: rescanning HF cache / custom dirs and Handy's retired-model migration.
    let g = granted();
    for c in ["allow-rescan-local-models", "allow-get-retired-models", "allow-delete-retired-model"] {
        assert!(!g.contains(c), "{c} must be removed from capabilities/default.json");
    }
}

// ---- S8: recorder policy, binding name, one MIC_OPEN_AT ----
#[test]
fn s08_so_stt_uses_offline_vad_binding_so_talk_and_one_mic_open_at() {
    let files = files_under(STT, &["rs"]);
    let code = |f: &PathBuf| non_test(&fs::read_to_string(f).unwrap_or_default());
    let all: String = files.iter().map(|f| code(f).lines().filter(|l| !is_comment(l)).collect::<Vec<_>>().join("\n")).collect::<Vec<_>>().join("\n");
    assert!(all.contains("VadPolicy::Offline"), "so/stt must record with VadPolicy::Offline");
    assert!(!all.contains("VadPolicy::Streaming"), "no streaming VAD policy in so/stt");
    assert_eq!(all.matches("\"so-talk\"").count(), 1, "binding \"so-talk\" is defined exactly once");
    let defs: usize = files.iter().map(|f| count_code(&code(f), "const MIC_OPEN_AT")).sum();
    assert_eq!(defs, 1, "MIC_OPEN_AT must be defined exactly once");
}

// ---- S9: download_language_detector is gone from every command list ----
#[test]
fn s09_download_language_detector_is_not_registered_anywhere() {
    let lib = read(LIB);
    let collect = lib.split("collect_commands!").nth(1).expect("collect_commands! in lib.rs");
    let collect = collect.split("]);").next().unwrap_or(collect);
    assert!(!collect.contains("download_language_detector"), "still in collect_commands!");
    let build = read(BUILD);
    let app_cmds = build.split("APP_COMMANDS").nth(1).expect("APP_COMMANDS in build.rs");
    let app_cmds = app_cmds.split("];").next().unwrap_or(app_cmds);
    assert!(!app_cmds.contains("download_language_detector"), "still in APP_COMMANDS");
    for f in files_under(CAPS, &["json"]) {
        let t = fs::read_to_string(&f).unwrap();
        assert!(!t.contains("language-detector") && !t.contains("language_detector"), "{} still grants the detector", rel(&f));
    }
    let b = read("desktop/src/bindings.ts");
    assert!(!b.contains("downloadLanguageDetector") && !b.contains("download_language_detector"), "bindings.ts still has the detector");
}

// ---- structure ----
#[test]
fn step2_files_exist() {
    for f in ["mod", "sink", "silence", "tap", "model", "real", "metrics", "wer"] {
        assert!(p(&format!("{STT}/{f}.rs")).exists(), "missing so/stt/{f}.rs");
    }
    for f in ["stt_tests", "stt_model_tests", "wer_tests", "stt_bench", "stt_engine_log"] {
        assert!(p(&format!("desktop/src-tauri/src/so/tests/{f}.rs")).exists(), "missing so/tests/{f}.rs");
    }
}

// ---- gate 9: scripts/fork-size.sh never counts our so/ dirs, still counts real Handy files ----
#[test]
fn fork_size_skips_so_dirs_but_counts_handy_files() {
    use std::process::Command;
    let tmp = std::env::temp_dir().join(format!("fork-size-selftest-{}", std::process::id()));
    let _ = fs::remove_dir_all(&tmp);
    let w = |rel: &str, body: &str| {
        let f = tmp.join(rel);
        fs::create_dir_all(f.parent().unwrap()).unwrap();
        fs::write(f, body).unwrap();
    };
    let git = |args: &[&str]| {
        let o = Command::new("git").args(args).current_dir(&tmp).output().unwrap();
        assert!(o.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&o.stderr));
    };
    fs::create_dir_all(tmp.join("scripts")).unwrap();
    fs::copy(p("scripts/fork-size.sh"), tmp.join("scripts/fork-size.sh")).unwrap();
    fs::write(tmp.join("scripts/fork-size-exclude.txt"), "# none\n").unwrap();
    for f in ["desktop/src-tauri/src/so/ours.rs", "desktop/src/so/ours.ts", "desktop/src-tauri/src/handy.rs"] {
        w(f, "a\n");
    }
    git(&["init", "-q"]);
    git(&["-c", "user.name=t", "-c", "user.email=t@t", "add", "-A"]);
    git(&["-c", "user.name=t", "-c", "user.email=t@t", "commit", "-qm", "base"]);
    git(&["tag", "base"]);
    for f in ["desktop/src-tauri/src/so/ours.rs", "desktop/src/so/ours.ts", "desktop/src-tauri/src/handy.rs"] {
        w(f, "a\nb\nc\n");
    }
    git(&["-c", "user.name=t", "-c", "user.email=t@t", "commit", "-qam", "head"]);
    let o = Command::new("bash").args(["scripts/fork-size.sh", "--base", "base"]).current_dir(&tmp).output().unwrap();
    let out = String::from_utf8_lossy(&o.stdout).to_string();
    let _ = fs::remove_dir_all(&tmp);
    assert!(out.contains("files modified: 1 "), "{out}");
    assert!(out.contains("src-tauri/src/handy.rs"), "{out}");
    assert!(!out.contains("/so/"), "so/ file was counted: {out}");
}

// ---- review B1: the microphone mode cannot be changed from the UI, and setup forces on-demand ----
#[test]
fn b1_update_microphone_mode_is_not_granted_and_setup_forces_on_demand() {
    assert!(!granted().contains("allow-update-microphone-mode"), "update_microphone_mode must not be granted");
    let setup = read(&format!("{STT}/mod.rs"));
    assert!(setup.contains("force_on_demand"), "stt::setup must switch the live manager to on-demand");
    let real = read(&format!("{STT}/real.rs"));
    assert!(real.contains("update_mode(MicrophoneMode::OnDemand)"));
}

#[test]
fn rescan_local_models_is_not_a_registered_command() {
    let lib = read(LIB);
    let collect = lib.split("collect_commands!").nth(1).unwrap().split("]);").next().unwrap().to_string();
    assert!(!collect.contains("rescan_local_models"));
}
