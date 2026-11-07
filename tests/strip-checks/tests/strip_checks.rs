// SPDX-License-Identifier: FSL-1.1-MIT
//! Step-0 strip/rebrand/pin checks. Read files only; never compile app code.
//! Standalone crate (not desktop/src-tauri) so it runs in seconds without
//! building transcribe-cpp/CMake. Run: `cargo test --manifest-path tests/strip-checks/Cargo.toml`.
//! Test numbers match draft/step-0-plan.md and the orchestrator decisions D1-D7.

use serde_json::Value;
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
    v.retain(|f| {
        exts.is_empty() || f.extension().map_or(false, |x| exts.contains(&x.to_string_lossy().as_ref()))
    });
    v
}
fn rel(f: &Path) -> String {
    f.strip_prefix(root()).unwrap().to_string_lossy().to_string()
}
fn is_comment(line: &str) -> bool {
    let t = line.trim_start();
    t.starts_with("//") || t.starts_with("/*") || t.starts_with('*')
}
/// Lines (file, number, text) under dirs matching any needle; skip_comments drops //-style lines.
fn grep(dirs: &[&str], exts: &[&str], needles: &[&str], skip_comments: bool) -> Vec<String> {
    let mut hits = vec![];
    for d in dirs {
        for f in files_under(d, exts) {
            // generated bindings are rebuilt from Rust; still scanned on purpose
            let Ok(txt) = fs::read_to_string(&f) else { continue };
            for (i, line) in txt.lines().enumerate() {
                if skip_comments && is_comment(line) {
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
const RS: &[&str] = &["rs"];
const SRC_DIRS: [&str; 2] = ["desktop/src-tauri/src", "desktop/src"];
const CONF: &str = "desktop/src-tauri/tauri.conf.json";

// ---- 1 identity ----
#[test]
fn t01_identity_and_no_handy_signing_or_updater() {
    let c = json(CONF);
    assert_eq!(c["identifier"], "com.sritamnanda.smoothoperator");
    assert_eq!(c["productName"], "Smooth Operator");
    assert_eq!(c["mainBinaryName"], "smooth-operator");
    assert_eq!(c["bundle"]["macOS"]["minimumSystemVersion"], "26.0");
    assert_eq!(c["bundle"]["createUpdaterArtifacts"], Value::Bool(false));
    assert!(c["plugins"].get("updater").is_none(), "plugins.updater must be gone (spike conf only)");
    for f in ["desktop/src-tauri/tauri.conf.json", "desktop/src-tauri/tauri.windows.conf.json"] {
        let t = read(f);
        assert!(!t.contains("signCommand"), "{f}: signCommand");
        assert!(!t.contains("cjpais/Handy"), "{f}: Handy updater URL");
        assert!(!t.contains("dW50cnVzdGVkIGNvbW1lbnQ6"), "{f}: Handy updater pubkey");
        assert!(!t.contains("codesigning.azure.net"), "{f}: Handy signing endpoint");
    }
    assert!(c["bundle"].get("linux").is_none(), "bundle.linux section must be removed");
}

// ---- 2 CSP / capabilities ----
fn csp_directives(c: &Value) -> Vec<(String, String)> {
    match &c["app"]["security"]["csp"] {
        Value::String(s) => s
            .split(';')
            .filter_map(|d| {
                let d = d.trim();
                let (k, v) = d.split_once(char::is_whitespace).unwrap_or((d, ""));
                (!k.is_empty()).then(|| (k.to_string(), v.to_string()))
            })
            .collect(),
        Value::Object(m) => m
            .iter()
            .map(|(k, v)| {
                let v = match v {
                    Value::String(s) => s.clone(),
                    Value::Array(a) => a.iter().filter_map(|x| x.as_str()).collect::<Vec<_>>().join(" "),
                    _ => String::new(),
                };
                (k.clone(), v)
            })
            .collect(),
        _ => vec![],
    }
}
#[test]
fn t02_csp_strict() {
    let c = json(CONF);
    assert!(!c["app"]["security"]["csp"].is_null(), "csp must not be null");
    let d = csp_directives(&c);
    let get = |k: &str| d.iter().find(|(n, _)| n == k).map(|(_, v)| v.clone());
    assert!(get("default-src").map_or(false, |v| v.split_whitespace().any(|x| x == "'self'")), "default-src must include 'self'");
    for (k, v) in &d {
        assert!(!v.contains("'unsafe-eval'"), "{k}: unsafe-eval");
    }
    let script = get("script-src").or_else(|| get("default-src")).unwrap_or_default();
    assert!(!script.contains("'unsafe-inline'"), "script-src (or default-src fallback) has unsafe-inline");
    for (k, v) in &d {
        if k == "script-src" || k == "default-src" || k == "script-src-elem" {
            assert!(!v.contains("http:") && !v.contains("https:") && !v.contains('*'), "{k}: remote script source allowed: {v}");
        }
    }
}
#[test]
fn t02_global_tauri_and_asset_protocol() {
    let c = json(CONF);
    assert_ne!(c["app"]["withGlobalTauri"], Value::Bool(true), "withGlobalTauri must be false");
    let ap = &c["app"]["security"]["assetProtocol"];
    let enabled = ap["enable"].as_bool().unwrap_or(false);
    let wide = ap["scope"]["allow"].as_array().map_or(false, |a| a.iter().any(|x| x == "**"))
        || ap["scope"].as_array().map_or(false, |a| a.iter().any(|x| x == "**"));
    assert!(!(enabled && wide), "assetProtocol scope \"**\" must be removed/narrowed");
}
#[test]
fn t02_capabilities() {
    let mut n = 0;
    for f in files_under("desktop/src-tauri/capabilities", &["json"]) {
        n += 1;
        let v: Value = serde_json::from_str(&fs::read_to_string(&f).unwrap()).unwrap();
        let perms: Vec<String> = v["permissions"]
            .as_array()
            .unwrap_or(&vec![])
            .iter()
            .map(|x| x.as_str().map(String::from).unwrap_or_else(|| x["identifier"].as_str().unwrap_or("").to_string()))
            .collect();
        let wins: Vec<String> = v["windows"].as_array().unwrap_or(&vec![]).iter().filter_map(|x| x.as_str().map(String::from)).collect();
        assert!(!perms.iter().any(|x| x.starts_with("updater:")), "D5 {}: updater permission present", rel(&f));
        if perms.iter().any(|x| x.starts_with("process:")) {
            let bad = wins.iter().any(|w| w.contains("overlay") || w.contains("panel") || w == "*");
            assert!(!bad, "D5 {}: process:* in a capability that lists the overlay/panel window {:?}", rel(&f), wins);
        }
    }
    assert!(n > 0, "no capability files found");
}

// ---- 3 post-processing / LLM (D4: local custom-word cleanup allowed) ----
#[test]
fn t03_no_llm_post_processing() {
    let needles = [
        "post_process_api_keys", "post_process_provider", "post_process_prompt", "post_process_enabled",
        "post_process_models", "post_process_selected", "PostProcessProvider", "transcribe_with_post_process",
        "llm_client", "LLMClient", "LlmClient", "build_system_prompt", "apple_intelligence", "AppleIntelligence",
    ];
    assert_none("LLM post-process symbols", grep(&SRC_DIRS, &[], &needles, false)
        .into_iter().filter(|h| !h.contains("/i18n/locales/") ).collect());
}

// ---- 4 no old bundle id ----
#[test]
fn t04_no_old_bundle_id() {
    let needle = ["com.pais", ".handy"].concat();
    let mut hits = vec![];
    let mut all = vec![];
    walk(&p("desktop"), &mut all);
    for e in ["LICENSE", "NOTICE"] {
        let _ = e;
    }
    for f in all {
        let name = f.file_name().unwrap().to_string_lossy().to_string();
        if name == "LICENSE" || name == "NOTICE" || name == "Cargo.lock" || name == "bun.lock" {
            continue;
        }
        if let Ok(t) = fs::read_to_string(&f) {
            if t.contains(&needle) {
                hits.push(rel(&f));
            }
        }
    }
    assert_none("old bundle id", hits);
}

// ---- 5 network allowlist ----
fn hosts_in(line: &str) -> Vec<String> {
    let mut out = vec![];
    let mut rest = line;
    while let Some(i) = rest.find("://") {
        let before = &rest[..i];
        let scheme_ok = before.ends_with("http") || before.ends_with("https") || before.ends_with("ws") || before.ends_with("wss");
        let after = &rest[i + 3..];
        let end = after.find(|c: char| !(c.is_ascii_alphanumeric() || c == '.' || c == '-')).unwrap_or(after.len());
        if scheme_ok && end > 0 {
            out.push(after[..end].to_lowercase());
        }
        rest = &after[end..];
    }
    out
}
#[test]
fn t05_network_hosts_allowlist() {
    let allow: Vec<String> = read("tests/net-allowlist.txt")
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .map(str::to_lowercase)
        .collect();
    let ok = |h: &str| allow.iter().any(|a| h == a || h.ends_with(&format!(".{a}")));
    let mut bad = vec![];
    let mut targets: Vec<PathBuf> = vec![];
    for d in SRC_DIRS {
        targets.extend(files_under(d, &["rs", "ts", "tsx", "html", "json"]));
    }
    for f in ["desktop/src-tauri/build.rs", "desktop/src-tauri/tauri.conf.json", "desktop/src-tauri/tauri.windows.conf.json", "desktop/index.html", "desktop/src/overlay/index.html"] {
        if p(f).exists() {
            targets.push(p(f));
        }
    }
    targets.extend(files_under("desktop/src-tauri/capabilities", &["json"]));
    for f in targets {
        let r = rel(&f);
        if r.contains("/i18n/locales/") || r.ends_with("bindings.ts") {
            continue;
        }
        let Ok(t) = fs::read_to_string(&f) else { continue };
        for (i, line) in t.lines().enumerate() {
            if is_comment(line) {
                continue;
            }
            for h in hosts_in(line) {
                if !ok(&h) {
                    bad.push(format!("{r}:{}: host {h}", i + 1));
                }
            }
        }
    }
    assert_none("hosts not in tests/net-allowlist.txt", bad);
}

// ---- 6 CLI flags (D3) ----
#[test]
fn t06_cli_flags_only_three() {
    let t = read("desktop/src-tauri/src/cli.rs");
    let mut fields: Vec<String> = t
        .lines()
        .map(str::trim)
        .filter(|l| l.starts_with("pub ") && l.contains(':'))
        .map(|l| l[4..l.find(':').unwrap()].trim().to_string())
        .collect();
    fields.sort();
    assert_eq!(fields, vec!["debug", "no_tray", "start_hidden"], "CliArgs fields");
    let removed = [
        "toggle_transcription", "toggle_post_process", "cancel", "transcribe_file", "model", "device_index",
        "list_devices", "list_models", "repeat", "json", "--toggle-transcription", "--toggle-post-process",
        "--cancel", "--transcribe-file", "--model", "--device-index", "--list-devices", "--list-models",
        "--repeat", "--json",
    ];
    let hits: Vec<String> = t
        .lines()
        .enumerate()
        .filter(|(_, l)| !is_comment(l) && (l.contains("short =") || l.contains("long =")))
        .filter(|(_, l)| removed.iter().any(|r| l.contains(&format!("\"{}\"", r.trim_start_matches("--")))))
        .map(|(i, l)| format!("cli.rs:{}: {}", i + 1, l.trim()))
        .collect();
    assert_none("removed flag definitions", hits);
    assert!(!t.contains("short = 'f'"), "-f short flag");
    // no other place forwards or defines the removed flags
    assert_none("removed flags used elsewhere", grep(&["desktop/src-tauri/src"], RS,
        &["--toggle-transcription", "--toggle-post-process", "--transcribe-file", "--list-models", "--list-devices", "--device-index"], true));
}
#[test]
fn t06_no_file_drop_transcribe_command_or_external_script() {
    assert_none("transcribe_file command/symbol", grep(&["desktop/src-tauri/src"], RS, &["transcribe_file", "TranscribeFile"], true));
    assert_none("transcribe_file in UI", grep(&["desktop/src"], &["ts", "tsx"], &["transcribeFile", "FileDropOverlay"], true)
        .into_iter().filter(|h| !h.ends_with("bindings.ts")).collect());
    assert_none("external_script paste method", grep(&SRC_DIRS, &[], &["external_script_path", "ExternalScript", "external_script"], false));
}
#[test]
fn t06_single_instance_forwards_nothing() {
    assert_none("single-instance toggle forwarding", grep(&["desktop/src-tauri/src"], RS,
        &["toggle_transcription", "toggle_post_process", "send_transcription_input", "args.cancel", "args.transcribe"], true));
}

// ---- 7 files absent ----
#[test]
fn t07_files_absent() {
    let names = ["transcription_hook.rs", "llm_client.rs", "apple_intelligence.rs", "apple_intelligence.swift", "apple_intelligence_stub.swift"];
    let mut all = vec![];
    walk(&p("desktop"), &mut all);
    let mut hits: Vec<String> = all.iter().filter(|f| names.contains(&f.file_name().unwrap().to_string_lossy().as_ref())).map(|f| rel(f)).collect();
    for f in ["desktop/src-tauri/src/managers/history.rs", "desktop/src-tauri/src/commands/history.rs", "desktop/src-tauri/src/managers/transcription_jobs.rs"] {
        if p(f).exists() {
            hits.push(f.to_string());
        }
    }
    assert_none("files that must be gone", hits);
    assert_none("hooks/transcription", grep(&["desktop/src-tauri/src"], RS, &["hooks/transcription"], false));
    assert!(!read("desktop/src-tauri/build.rs").contains("swiftc"), "build.rs still calls swiftc");
}

// ---- 8 signals ----
#[test]
fn t08_no_sigusr_handlers() {
    assert!(!p("desktop/src-tauri/src/signal_handle.rs").exists(), "signal_handle.rs");
    assert_none("SIGUSR/signal_hook", grep(&["desktop/src-tauri/src"], RS, &["SIGUSR1", "SIGUSR2", "signal_hook"], true));
    assert!(!read("desktop/src-tauri/Cargo.toml").contains("signal-hook"), "signal-hook dep");
}

// ---- 9 updater only under spike ----
#[test]
fn t09_updater_plugin_only_under_spike_feature() {
    let mut bad = vec![];
    for f in files_under("desktop/src-tauri/src", &["rs"]) {
        let r = rel(&f);
        if r.contains("/src/spike/") {
            continue;
        }
        let lines: Vec<String> = fs::read_to_string(&f).unwrap().lines().map(String::from).collect();
        for (i, l) in lines.iter().enumerate() {
            if is_comment(l) || !l.contains("tauri_plugin_updater") {
                continue;
            }
            let lo = i.saturating_sub(4);
            let gated = lines[lo..=i].iter().any(|x| x.replace(' ', "").contains("cfg(feature=\"spike\")"));
            if !gated {
                bad.push(format!("{r}:{}: {}", i + 1, l.trim()));
            }
        }
    }
    assert_none("updater registered outside cfg(feature = \"spike\")", bad);
    let cargo: toml::Value = read("desktop/src-tauri/Cargo.toml").parse().unwrap();
    if let Some(dep) = cargo.get("dependencies").and_then(|d| d.get("tauri-plugin-updater")) {
        assert_eq!(dep.get("optional"), Some(&toml::Value::Boolean(true)), "tauri-plugin-updater must be optional");
        assert!(cargo.get("features").and_then(|f| f.get("spike")).is_some(), "cargo feature `spike` missing");
    }
}

// ---- 10 pins (D1) ----
fn walk_toml(v: &toml::Value, path: &str, out: &mut Vec<(String, toml::Value)>) {
    if let Some(t) = v.as_table() {
        if t.contains_key("git") {
            out.push((path.to_string(), v.clone()));
        }
        for (k, c) in t {
            walk_toml(c, &format!("{path}.{k}"), out);
        }
    }
}
#[test]
fn t10_pins_and_no_rdev() {
    let cargo: toml::Value = read("desktop/src-tauri/Cargo.toml").parse().unwrap();
    let txt = read("desktop/src-tauri/Cargo.toml");
    assert!(!txt.lines().any(|l| l.trim_start().starts_with("rdev")), "D1: rdev dep must be removed");
    let mut gits = vec![];
    walk_toml(&cargo, "", &mut gits);
    for (name, d) in &gits {
        let rev = d.get("rev").and_then(|r| r.as_str()).unwrap_or("");
        assert!(rev.len() >= 40, "git dep {name} needs a full rev= (got {rev:?})");
        assert!(d.get("branch").is_none(), "git dep {name} must not use branch");
    }
    for need in ["hf-hub", "tauri-nspanel", "tao", "tao-macros"] {
        assert!(gits.iter().any(|(n, _)| n.ends_with(&format!(".{need}"))), "git dep {need} missing or not pinned");
    }
    let hk = cargo["dependencies"]["handy-keys"].as_str().expect("handy-keys must be a plain version string");
    assert_eq!(hk, "=0.3.4");
}

// ---- D2 postinstall ----
#[test]
fn d2_no_postinstall_fetching_tools() {
    let pk = json("desktop/package.json");
    if let Some(s) = pk["scripts"].as_object() {
        for (k, v) in s {
            let v = v.as_str().unwrap_or("");
            if ["preinstall", "install", "postinstall", "prepare"].contains(&k.as_str()) {
                assert!(!v.contains("bunx") && !v.contains("bun2nix") && !v.contains("check-nix-deps"), "D2: {k} = {v}");
            }
        }
    }
    assert!(!p("desktop/scripts/check-nix-deps.ts").exists(), "check-nix-deps.ts should be removed");
}

// ---- D7 logger cap on the key path ----
#[test]
fn d7_debug_does_not_raise_key_hook_logging() {
    let lib = read("desktop/src-tauri/src/lib.rs");
    let lines: Vec<&str> = lib.lines().filter(|l| !is_comment(l)).collect();
    let capped = lines.iter().any(|l| {
        l.contains("handy_keys")
            && (l.contains("level_for") || l.contains("target"))
            && ["LevelFilter::Info", "LevelFilter::Warn", "LevelFilter::Error", "LevelFilter::Off"].iter().any(|x| l.contains(x))
    });
    assert!(capped, "D7: lib.rs logger setup needs an explicit cap (<= Info) on the handy_keys log target, e.g. .level_for(\"handy_keys\", LevelFilter::Info)");
    assert!(
        !lines.iter().any(|l| l.contains("handy_keys") && (l.contains("LevelFilter::Trace") || l.contains("LevelFilter::Debug"))),
        "D7: handy_keys target must not be set to Debug/Trace"
    );
    // key-path code must not log key values at debug/trace
    assert_none("debug!/trace! with hotkey value in shortcut code", grep(&["desktop/src-tauri/src/shortcut"], RS, &["trace!("], true));
    let hk = read("desktop/src-tauri/src/shortcut/handy_keys.rs");
    assert!(
        !hk.lines().any(|l| !is_comment(l) && l.contains("handy-keys event") && l.contains("hotkey")),
        "D7: handy-keys event log line still prints the hotkey string"
    );
}

// ---- repo-level licence files (feeds Vitest test 11) ----
#[test]
fn licence_files_exist() {
    for f in ["LICENSE", "NOTICE", "REUSE.toml", "CONTRIBUTING.md", "desktop/LICENSE"] {
        assert!(p(f).exists(), "missing {f}");
    }
    assert!(read("LICENSE").contains("Functional Source License"), "root LICENSE must be FSL-1.1-MIT");
    assert!(read("desktop/LICENSE").contains("MIT License"), "desktop/LICENSE must stay MIT");
}
