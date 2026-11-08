// SPDX-License-Identifier: FSL-1.1-MIT
//! Step 3 static checks S1-S6, I7 and R7 (fast rules + debug test input). Read files only; never compile
//! app code. Same style as step2_checks.rs. Run: `cargo test --manifest-path tests/strip-checks/Cargo.toml`.
//! Numbers match draft/step-3-plan.md.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..").canonicalize().unwrap()
}
fn p(rel: &str) -> PathBuf {
    root().join(rel)
}
fn read(rel: &str) -> String {
    fs::read_to_string(p(rel)).unwrap_or_else(|e| panic!("cannot read {rel}: {e}"))
}
fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(rd) = fs::read_dir(dir) else { return };
    for e in rd.flatten() {
        let path = e.path();
        if path.is_dir() {
            walk(&path, out);
        } else {
            out.push(path);
        }
    }
}
fn rs_files_under(rel: &str) -> Vec<PathBuf> {
    let mut v = vec![];
    walk(&p(rel), &mut v);
    v.retain(|f| f.extension().is_some_and(|x| x == "rs"));
    v.sort();
    v
}
fn relp(f: &Path) -> String {
    f.strip_prefix(root()).unwrap().to_string_lossy().to_string()
}
fn is_comment(line: &str) -> bool {
    let t = line.trim_start();
    t.starts_with("//") || t.starts_with("/*") || t.starts_with('*')
}
fn non_test(src: &str) -> String {
    src.split("#[cfg(test)]").next().unwrap().to_string()
}
fn assert_none(what: &str, hits: Vec<String>) {
    assert!(hits.is_empty(), "{what}: {} hit(s), first: {:#?}", hits.len(), &hits[..hits.len().min(8)]);
}
/// `line` contains `needle`, and if the needle starts with a letter, digit or `_` the match does not start
/// inside a longer identifier (so `log::` does not match `FsAppCatalog::`).
fn contains_word_start(line: &str, needle: &str) -> bool {
    let word = needle.chars().next().is_some_and(|c| c.is_alphanumeric() || c == '_');
    line.match_indices(needle).any(|(i, _)| {
        !word || !line[..i].chars().next_back().is_some_and(|c| c.is_alphanumeric() || c == '_')
    })
}
/// Non-comment, non-test lines of every `so/rules` file (optionally skipping one file name) containing a needle.
fn rules_hits(needles: &[&str], skip: Option<&str>) -> Vec<String> {
    let mut hits = vec![];
    for f in rs_files_under(RULES) {
        if skip.is_some_and(|s| f.file_name().unwrap() == s) {
            continue;
        }
        let txt = non_test(&fs::read_to_string(&f).unwrap_or_default());
        for (i, line) in txt.lines().enumerate() {
            if !is_comment(line) && needles.iter().any(|n| contains_word_start(line, n)) {
                hits.push(format!("{}:{}: {}", relp(&f), i + 1, line.trim()));
            }
        }
    }
    hits
}

const RULES: &str = "desktop/src-tauri/src/so/rules";
const SO: &str = "desktop/src-tauri/src/so";
const BASE_TAG: &str = "step-2";

fn git(args: &[&str]) -> std::process::Output {
    Command::new("git").args(args).current_dir(root()).output().expect("run git")
}
fn require_tag() {
    let o = git(&["rev-parse", "--verify", "--quiet", &format!("{BASE_TAG}^{{commit}}")]);
    assert!(o.status.success(), "git tag {BASE_TAG} not found: fetch tags (`git fetch --tags`) to run S3/R7");
}
/// Paths (relative to the repo root of the app clone) changed since the step-2 tag, within `paths`.
fn changed_since_step2(paths: &[&str]) -> Vec<String> {
    require_tag();
    let top = String::from_utf8(git(&["rev-parse", "--show-toplevel"]).stdout).unwrap().trim().to_string();
    let prefix = root().strip_prefix(&top).unwrap().to_string_lossy().to_string();
    let specs: Vec<String> = paths.iter().map(|x| if prefix.is_empty() { x.to_string() } else { format!("{prefix}/{x}") }).collect();
    let mut a = vec!["diff", "--name-only", BASE_TAG, "--"];
    a.extend(specs.iter().map(String::as_str));
    let o = Command::new("git").args(&a).current_dir(&top).output().unwrap();
    assert!(o.status.success(), "git diff failed: {}", String::from_utf8_lossy(&o.stderr));
    String::from_utf8_lossy(&o.stdout).lines().map(String::from).collect()
}

// ---- S1: only metrics.rs under so/rules may log; its functions take integers and bools only ----
const LOG_NEEDLES: [&str; 11] = [
    "log::", "info!(", "debug!(", "warn!(", "error!(", "trace!(", "println!(", "eprintln!(", "print!(", "eprint!(", "dbg!(",
];

#[test]
fn s01_no_logging_or_printing_in_so_rules_except_metrics() {
    assert_none("log/print macros in so/rules (outside metrics.rs)", rules_hits(&LOG_NEEDLES, Some("metrics.rs")));
}

#[test]
fn s01_metrics_functions_take_only_integers_and_bools() {
    const OK: [&str; 13] = ["u8", "u16", "u32", "u64", "usize", "i8", "i16", "i32", "i64", "isize", "bool", "f32", "f64"];
    let txt = non_test(&read(&format!("{RULES}/metrics.rs")));
    let mut fns = 0;
    for line in txt.lines().filter(|l| !is_comment(l)) {
        let t = line.trim();
        if !(t.starts_with("pub fn ") || t.starts_with("fn ") || t.starts_with("pub(crate) fn ")) {
            continue;
        }
        fns += 1;
        let args = &t[t.find('(').unwrap() + 1..t.rfind(')').unwrap_or(t.len())];
        for a in args.split(',').map(str::trim).filter(|a| !a.is_empty()) {
            let ty = a.split(':').nth(1).map(str::trim).unwrap_or("");
            assert!(OK.contains(&ty), "metrics fn takes a non-number/bool argument `{a}` in: {t}");
        }
    }
    assert!(fns >= 2, "metrics.rs must define fast_rule and debug_input");
}

// ---- S2: the launcher is /usr/bin/open, no shell, no osascript ----
#[test]
fn s02_real_launcher_uses_usr_bin_open_and_no_shell_or_osascript() {
    let txt = read(&format!("{RULES}/real.rs"));
    let code: Vec<&str> = txt.lines().filter(|l| !is_comment(l)).collect();
    let code = code.join("\n");
    assert!(code.contains("\"/usr/bin/open\""), "real.rs must run the literal \"/usr/bin/open\"");
    for bad in ["\"sh\"", "\"bash\"", "\"zsh\"", "\"-c\"", "osascript", "\"/bin/sh\"", "shell"] {
        assert!(!code.contains(bad), "real.rs must not use {bad}");
    }
}

// ---- S3: no core/Handy config files changed vs the step-2 tag (gate 9) ----
#[test]
fn s03_cargo_lib_build_capabilities_and_bindings_are_unchanged_since_step_2() {
    let changed = changed_since_step2(&[
        "desktop/src-tauri/Cargo.toml",
        "desktop/src-tauri/src/lib.rs",
        "desktop/src-tauri/build.rs",
        "desktop/src-tauri/capabilities",
        "desktop/src/bindings.ts",
    ]);
    assert!(changed.is_empty(), "files that must not change in step 3: {changed:?}");
}

// ---- S4: no file writes in so/rules outside tests ----
#[test]
fn s04_no_file_writes_in_so_rules() {
    assert_none(
        "file writes in so/rules",
        rules_hits(&["File::create", "fs::write", "temp_dir", "tempfile", "OpenOptions", "write_all(", "fs::copy", "fs::rename"], None),
    );
}

#[test]
fn s04_debug_input_creates_only_its_folder_and_socket() {
    // The socket and its folder are the only filesystem entries; nothing is created in other rules files.
    assert_none("create_dir / bind outside debug_input.rs", rules_hits(&["create_dir", "UnixListener::bind"], Some("debug_input.rs")));
    let d = non_test(&read(&format!("{RULES}/debug_input.rs")));
    assert!(d.contains("UnixListener::bind"), "debug_input.rs must bind a Unix socket");
}

// ---- S5: no eval / innerHTML / emit ----
#[test]
fn s05_so_rules_never_evals_sets_inner_html_or_emits() {
    assert_none("eval/innerHTML/emit in so/rules", rules_hits(&["eval(", "innerHTML", ".emit(", "emit("], None));
}

// ---- S6: PassThrough is the SpeechSink::new default ----
#[test]
fn s06_speech_sink_new_plugs_in_pass_through() {
    let sink = non_test(&read(&format!("{SO}/stt/sink.rs")));
    let new_fn = sink.split("pub fn new(").nth(1).expect("SpeechSink::new");
    let new_fn = new_fn.split("pub fn with_router").next().unwrap();
    assert!(new_fn.contains("with_router("), "new must delegate to with_router");
    assert!(new_fn.contains("Arc::new(PassThrough)"), "new must plug in PassThrough");
    assert!(!new_fn.contains("FastRouter"), "new must not default to FastRouter");
}

// ---- I7: debug input is compiled out of release ----
const GATE: &str = "cfg(debug_assertions)";

/// Lines (1-based) that mention `needle` in code but have no `#[cfg(debug_assertions)]` on the line or in the
/// 8 lines before it.
fn ungated(src: &str, needle: &str) -> Vec<usize> {
    let lines: Vec<&str> = src.lines().collect();
    let mut out = vec![];
    for (i, l) in lines.iter().enumerate() {
        if is_comment(l) || !l.contains(needle) {
            continue;
        }
        let from = i.saturating_sub(8);
        if !lines[from..=i].iter().any(|x| x.contains(GATE)) {
            out.push(i + 1);
        }
    }
    out
}

#[test]
fn i7_the_checker_flags_an_ungated_call_and_accepts_a_gated_one() {
    let bad = "fn setup() {\n    let x = 1;\n    debug_input::start(a, b, c);\n}\n";
    assert_eq!(ungated(bad, "debug_input::"), vec![3]);
    let good = "fn setup() {\n    #[cfg(debug_assertions)]\n    {\n        debug_input::start(a, b, c);\n    }\n}\n";
    assert!(ungated(good, "debug_input::").is_empty());
}

#[test]
fn i7_debug_input_module_inject_transcript_and_every_call_site_are_gated() {
    let mut hits = vec![];
    for f in rs_files_under(SO) {
        let r = relp(&f);
        if r.contains("/so/tests/") || r.ends_with("rules/debug_input.rs") {
            continue;
        }
        let txt = non_test(&fs::read_to_string(&f).unwrap_or_default());
        for needle in ["mod debug_input", "debug_input::", "inject_transcript"] {
            for line in ungated(&txt, needle) {
                hits.push(format!("{r}:{line}: `{needle}` is not under #[cfg(debug_assertions)]"));
            }
        }
    }
    assert_none("ungated debug-input code", hits);
    // Positive control: the gated declarations exist at all.
    assert!(read(&format!("{RULES}/mod.rs")).contains("mod debug_input"), "so/rules/mod.rs must declare mod debug_input");
    assert!(read(&format!("{SO}/stt/sink.rs")).contains("fn inject_transcript"), "sink.rs must define inject_transcript");
}

#[test]
fn i7_release_profile_does_not_turn_debug_assertions_on() {
    let v: toml::Value = toml::from_str(&read("desktop/src-tauri/Cargo.toml")).unwrap();
    let rel = v.get("profile").and_then(|p| p.get("release")).expect("[profile.release]");
    assert!(rel.get("debug-assertions").is_none(), "[profile.release] must not set debug-assertions");
    assert!(rel.get("inherits").is_none(), "release must not inherit from another profile");
    if let Some(pk) = rel.get("package").and_then(|x| x.as_table()) {
        for (name, t) in pk {
            assert!(t.get("debug-assertions").is_none(), "[profile.release.package.{name}] sets debug-assertions");
        }
    }
    assert!(rel.get("overrides").is_none());
}

#[test]
fn i7_debug_input_has_no_command_invoke_capability_or_network_listener() {
    let txt = non_test(&read(&format!("{RULES}/debug_input.rs")));
    let code: Vec<&str> = txt.lines().filter(|l| !is_comment(l)).collect();
    let code = code.join("\n");
    for bad in ["#[tauri::command]", "invoke", "capability", "TcpListener", "TcpStream", "UdpSocket", "emit(", "AppHandle"] {
        assert!(!code.contains(bad), "debug_input.rs must not contain {bad}");
    }
    assert!(code.contains("UnixListener"), "the transport is a Unix socket");
    assert!(code.contains("getpeereid"), "each connection is checked with getpeereid");
    for f in fs::read_dir(p("desktop/src-tauri/capabilities")).unwrap().flatten().map(|e| e.path()) {
        let t = fs::read_to_string(&f).unwrap_or_default();
        assert!(!t.contains("debug-input") && !t.contains("debug_input"), "{} grants the debug input", f.display());
    }
}

// ---- R7: step-2 tests are not edited ----
#[test]
fn r7_step_2_test_files_are_unchanged_since_the_step_2_tag() {
    let changed = changed_since_step2(&[
        "desktop/src-tauri/src/so/tests/stt_tests.rs",
        "desktop/src-tauri/src/so/tests/stt_model_tests.rs",
        "desktop/src-tauri/src/so/tests/stt_review_tests.rs",
        "desktop/src-tauri/src/so/tests/stt_bench.rs",
        "desktop/src-tauri/src/so/tests/stt_engine_log.rs",
        "desktop/src-tauri/src/so/tests/wer_tests.rs",
        "desktop/src-tauri/src/so/tests/watcher_tests.rs",
        "desktop/src-tauri/src/so/tests/hotkey_tests.rs",
        "desktop/src-tauri/src/so/tests/panel_tests.rs",
        "desktop/src-tauri/src/so/tests/settings_tests.rs",
        "desktop/src-tauri/src/so/tests/fncheck_tests.rs",
        "desktop/src-tauri/src/so/tests/ipc_acl_tests.rs",
        "desktop/src-tauri/src/so/tests/nav_tests.rs",
        "tests/strip-checks/tests/step1_checks.rs",
        "tests/strip-checks/tests/step2_checks.rs",
    ]);
    assert!(changed.is_empty(), "earlier-step tests were edited: {changed:?}");
}

// ---- structure ----
#[test]
fn step3_files_exist() {
    for f in ["mod", "phrase", "apps", "hands", "router", "real", "debug_input", "metrics"] {
        assert!(p(&format!("{RULES}/{f}.rs")).exists(), "missing so/rules/{f}.rs");
    }
    for f in ["rules_support", "rules_seam_tests", "rules_tests", "rules_debug_input_tests", "rules_release_tests"] {
        assert!(p(&format!("{SO}/tests/{f}.rs")).exists(), "missing so/tests/{f}.rs");
    }
}
