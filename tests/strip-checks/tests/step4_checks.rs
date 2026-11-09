// SPDX-License-Identifier: FSL-1.1-MIT
//! Step 4 static checks S1-S13 (+ K11, K6, D6 and structure checks). Read files only; never compile app
//! code. Same style as step3_checks.rs. Run: `cargo test --manifest-path tests/strip-checks/Cargo.toml`.
//! Numbers match draft/step-4-plan.md. Each scanner has a self-test that proves it can fail.

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
fn files_under(rel: &str, exts: &[&str]) -> Vec<PathBuf> {
    let mut v = vec![];
    walk(&p(rel), &mut v);
    v.retain(|f| f.extension().is_some_and(|x| exts.iter().any(|e| x == *e)));
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
fn assert_none(what: &str, hits: Vec<String>) {
    assert!(hits.is_empty(), "{what}: {} hit(s), first: {:#?}", hits.len(), &hits[..hits.len().min(8)]);
}
fn contains_word_start(line: &str, needle: &str) -> bool {
    let word = needle.chars().next().is_some_and(|c| c.is_alphanumeric() || c == '_');
    line.match_indices(needle).any(|(i, _)| !word || !line[..i].chars().next_back().is_some_and(|c| c.is_alphanumeric() || c == '_'))
}

const SO: &str = "desktop/src-tauri/src/so";
const BRAIN: &str = "desktop/src-tauri/src/so/brain";
const BASE_TAG: &str = "step-3";

/// Removes every `#[cfg(test)]` item (a mod, fn, impl, const...) so scans look at shipped code only, also
/// when a test-only fn sits in the middle of an impl (like `with_base`).
fn strip_cfg_test(src: &str) -> String {
    let lines: Vec<&str> = src.lines().collect();
    let mut out = String::new();
    let mut i = 0;
    while i < lines.len() {
        if lines[i].trim_start().starts_with("#[cfg(test)]") {
            i += 1;
            let mut depth: i32 = 0;
            let mut seen_brace = false;
            while i < lines.len() {
                let l = lines[i];
                for c in l.chars() {
                    match c {
                        '{' => {
                            depth += 1;
                            seen_brace = true;
                        }
                        '}' => depth -= 1,
                        _ => {}
                    }
                }
                i += 1;
                if (seen_brace && depth <= 0) || (!seen_brace && l.trim_end().ends_with(';')) {
                    break;
                }
            }
            continue;
        }
        out.push_str(lines[i]);
        out.push('\n');
        i += 1;
    }
    out
}

/// Shipped code lines (non-comment, non-test) of every `so/brain` file, optionally skipping one file name.
fn brain_code(skip: Option<&str>) -> Vec<(String, usize, String)> {
    let mut out = vec![];
    for f in files_under(BRAIN, &["rs"]) {
        if skip.is_some_and(|s| f.file_name().unwrap() == s) {
            continue;
        }
        let txt = strip_cfg_test(&fs::read_to_string(&f).unwrap_or_default());
        for (i, line) in txt.lines().enumerate() {
            if !is_comment(line) {
                out.push((relp(&f), i + 1, line.to_string()));
            }
        }
    }
    out
}
fn brain_hits(needles: &[&str], skip: Option<&str>) -> Vec<String> {
    brain_code(skip)
        .into_iter()
        .filter(|(_, _, l)| needles.iter().any(|n| contains_word_start(l, n)))
        .map(|(f, i, l)| format!("{f}:{i}: {}", l.trim()))
        .collect()
}
fn brain_file_code(name: &str) -> String {
    let txt = strip_cfg_test(&read(&format!("{BRAIN}/{name}")));
    txt.lines().filter(|l| !is_comment(l)).collect::<Vec<_>>().join("\n")
}

fn git_top(args: &[&str]) -> std::process::Output {
    Command::new("git").args(args).current_dir(root()).output().expect("run git")
}
fn require_tag() {
    let o = git_top(&["rev-parse", "--verify", "--quiet", &format!("{BASE_TAG}^{{commit}}")]);
    assert!(o.status.success(), "git tag {BASE_TAG} not found: fetch tags (`git fetch --tags`) to run S8/S9/S11/D6");
}
fn changed_since_tag(paths: &[&str], filter: &str) -> Vec<String> {
    require_tag();
    let mut a = vec!["diff", "--name-only", filter, BASE_TAG, "--"];
    a.extend(paths.iter().copied());
    let o = git_top(&a);
    assert!(o.status.success(), "git diff failed: {}", String::from_utf8_lossy(&o.stderr));
    String::from_utf8_lossy(&o.stdout).lines().map(String::from).collect()
}
fn at_tag(rel: &str) -> String {
    require_tag();
    let o = git_top(&["show", &format!("{BASE_TAG}:{rel}")]);
    assert!(o.status.success(), "git show {BASE_TAG}:{rel} failed");
    String::from_utf8_lossy(&o.stdout).to_string()
}

// ---- S1: only metrics.rs under so/brain may log; its functions take integers, bools and &[u8] only ----
const LOG_NEEDLES: [&str; 12] = [
    "log::", "tracing::", "info!(", "debug!(", "warn!(", "error!(", "trace!(", "println!(", "eprintln!(", "print!(", "eprint!(", "dbg!(",
];

#[test]
fn s01_the_scanner_can_fail() {
    assert!(contains_word_start("    log::info!(\"x\");", "log::"));
    assert!(!contains_word_start("    BlogEntry::new()", "log::"));
    let src = "fn a() {}\n#[cfg(test)]\nfn t() { log::info!(\"x\"); }\nfn b() { let x = 1; }\n";
    let kept = strip_cfg_test(src);
    assert!(!kept.contains("log::") && kept.contains("fn b()") && kept.contains("fn a()"), "{kept}");
    let nested = "impl A {\n    #[cfg(test)]\n    pub fn with_base(b: &str) -> Self {\n        if x { y }\n    }\n    pub fn keep() {}\n}\n";
    assert!(strip_cfg_test(nested).contains("pub fn keep()") && !strip_cfg_test(nested).contains("with_base"));
}

#[test]
fn s01_no_logging_or_printing_in_so_brain_except_metrics() {
    assert_none("log/print macros in so/brain (outside metrics.rs)", brain_hits(&LOG_NEEDLES, Some("metrics.rs")));
}

#[test]
fn s01_metrics_takes_only_numbers_and_logs_the_planned_line() {
    const OK: [&str; 14] = ["u8", "u16", "u32", "u64", "usize", "i8", "i16", "i32", "i64", "isize", "bool", "f32", "f64", "&[u8]"];
    let txt = brain_file_code("metrics.rs");
    let mut fns = 0;
    for line in txt.lines() {
        let t = line.trim();
        if !(t.starts_with("pub fn ") || t.starts_with("fn ") || t.starts_with("pub(crate) fn ")) {
            continue;
        }
        fns += 1;
        let args = &t[t.find('(').unwrap() + 1..t.rfind(')').unwrap_or(t.len())];
        for a in args.split(',').map(str::trim).filter(|a| !a.is_empty()) {
            let ty = a.split(':').nth(1).map(str::trim).unwrap_or("");
            assert!(OK.contains(&ty), "metrics fn takes a non-number argument `{a}` in: {t}");
        }
    }
    assert!(fns >= 1, "metrics.rs must define `brain`");
    for part in ["brain outcome=", "provider=", "ms=", "steps=", "http=", "reject=", "actions="] {
        assert!(txt.contains(part), "metrics.rs must log `{part}`");
    }
}

// ---- S2: no key in any URL; only the four hosts; all https ----
const HOSTS: [&str; 4] = ["api.anthropic.com", "generativelanguage.googleapis.com", "platform.claude.com", "aistudio.google.com"];

fn urls_in(line: &str) -> Vec<(String, String)> {
    let mut out = vec![];
    let mut rest = line;
    while let Some(i) = rest.find("://") {
        let before = &rest[..i];
        let scheme: String = before.chars().rev().take_while(|c| c.is_ascii_alphabetic()).collect::<Vec<_>>().into_iter().rev().collect();
        let after = &rest[i + 3..];
        let end = after.find(|c: char| !(c.is_ascii_alphanumeric() || c == '.' || c == '-')).unwrap_or(after.len());
        out.push((scheme.to_lowercase(), after[..end].to_lowercase()));
        rest = &after[end..];
    }
    out
}

#[test]
fn s02_the_url_scanner_can_fail() {
    assert_eq!(urls_in("let u = \"http://evil.example/x\";"), vec![("http".to_string(), "evil.example".to_string())]);
    assert_eq!(urls_in("\"https://api.anthropic.com/v1/messages\""), vec![("https".to_string(), "api.anthropic.com".to_string())]);
}

#[test]
fn s02_the_net_allowlist_has_the_four_provider_and_console_hosts() {
    let allow = read("tests/net-allowlist.txt");
    let hosts: Vec<&str> = allow.lines().map(str::trim).filter(|l| !l.is_empty() && !l.starts_with('#')).collect();
    for h in HOSTS {
        assert!(hosts.contains(&h), "tests/net-allowlist.txt must list {h}");
    }
}

#[test]
fn s02_only_the_four_hosts_all_https_and_no_key_in_a_url() {
    let mut found: Vec<String> = vec![];
    let mut bad = vec![];
    for (f, i, l) in brain_code(None) {
        for (scheme, host) in urls_in(&l) {
            if scheme != "https" || !HOSTS.contains(&host.as_str()) {
                bad.push(format!("{f}:{i}: {scheme}://{host}"));
            }
            found.push(host);
        }
        let low = l.to_lowercase();
        for needle in ["key=", "?key", "&key", "api_key=", "apikey=", "access_token="] {
            if low.contains(needle) {
                bad.push(format!("{f}:{i}: `{needle}` looks like a key in a URL: {}", l.trim()));
            }
        }
    }
    assert_none("URLs in so/brain that are not https to one of the four hosts, or that carry a key", bad);
    for h in HOSTS {
        assert!(found.iter().any(|x| x == h), "positive control: {h} must appear in so/brain (provider table / provider bases)");
    }
}

#[test]
fn h10_new_uses_the_fixed_https_bases() {
    assert!(brain_file_code("gemini.rs").contains("https://generativelanguage.googleapis.com"), "gemini.rs: fixed https base");
    assert!(brain_file_code("claude.rs").contains("https://api.anthropic.com"), "claude.rs: fixed https base");
}

// ---- S3: ApiKey never copies, prints or serialises its secret ----
#[test]
fn s03_api_key_has_no_clone_serialize_display_type_and_expose_is_crate_private() {
    let txt = brain_file_code("keys.rs");
    let lines: Vec<&str> = txt.lines().collect();
    let at = lines.iter().position(|l| l.contains("pub struct ApiKey")).expect("pub struct ApiKey");
    for l in &lines[at.saturating_sub(4)..at] {
        if l.contains("derive(") {
            for bad in ["Clone", "Copy", "Serialize", "Deserialize", "Type", "Debug", "Display", "PartialEq", "Hash"] {
                assert!(!l.contains(bad), "ApiKey must not derive {bad}: {l}");
            }
        }
    }
    for bad in [
        "Clone for ApiKey", "Display for ApiKey", "Serialize for ApiKey", "Type for ApiKey", "Deserialize<'de> for ApiKey", "Copy for ApiKey",
        "From<ApiKey>", "AsRef<str> for ApiKey", "Deref for ApiKey", "Into<String> for ApiKey",
    ] {
        assert!(!txt.contains(bad), "ApiKey must not implement: {bad}");
    }
    assert!(txt.contains("Debug for ApiKey"), "ApiKey needs a hand-written Debug impl");
    assert!(txt.contains("ApiKey(<redacted>)"), "its Debug text is `ApiKey(<redacted>)`");
    assert!(txt.contains("pub(crate) fn expose"), "expose must be pub(crate)");
    assert!(!txt.contains("pub fn expose"), "expose must not be public");
    // `expose()` is used once to build a header: never outside the key store and the two providers (and the debug fake).
    let mut bad = vec![];
    for f in files_under(SO, &["rs"]) {
        let r = relp(&f);
        if r.contains("/so/tests/") || r.ends_with("brain/keys.rs") || r.ends_with("brain/claude.rs") || r.ends_with("brain/gemini.rs") || r.ends_with("brain/fake.rs") {
            continue;
        }
        let code = strip_cfg_test(&fs::read_to_string(&f).unwrap_or_default());
        for (i, l) in code.lines().enumerate() {
            if !is_comment(l) && l.contains("expose(") {
                bad.push(format!("{r}:{}: {}", i + 1, l.trim()));
            }
        }
    }
    assert_none("expose() outside keys.rs / claude.rs / gemini.rs / fake.rs", bad);
}

// ---- S4: the HTTP client rules ----
#[test]
fn s04_https_only_no_redirects_timeouts_and_sensitive_key_headers() {
    let http = brain_file_code("http.rs");
    assert!(http.contains("redirect::Policy::none()"), "http.rs must use redirect::Policy::none()");
    assert!(http.contains("https_only(true)"), "http.rs must use https_only(true)");
    assert!(!http.contains("https_only(false)"), "https_only(false) in shipped code");
    assert!(http.contains("connect_timeout(") && http.contains("CONNECT_TIMEOUT"), "connect timeout from CONNECT_TIMEOUT");
    assert!(http.contains(".timeout(") && http.contains("TOTAL_TIMEOUT"), "total timeout from TOTAL_TIMEOUT");
    assert!(!http.contains("redirect::Policy::limited") && !http.contains("Policy::default"), "redirects must stay off");
    let all: String = ["http.rs", "claude.rs", "gemini.rs"].iter().map(|f| brain_file_code(f)).collect::<Vec<_>>().join("\n");
    assert!(all.matches("set_sensitive(true)").count() >= 2, "both key headers (Authorization and x-goog-api-key) need set_sensitive(true)");
    assert!(all.to_lowercase().contains("x-goog-api-key"), "Gemini key header");
    assert!(all.contains("Bearer"), "Claude key header is a Bearer token");
}

// ---- S5: keyring pin ----
#[test]
fn s05_keyring_is_pinned_to_3_6_3_with_only_apple_native() {
    let v: toml::Value = toml::from_str(&read("desktop/src-tauri/Cargo.toml")).unwrap();
    let mac = v
        .get("target")
        .and_then(|t| t.get("cfg(target_os = \"macos\")"))
        .and_then(|t| t.get("dependencies"))
        .expect("[target.'cfg(target_os = \"macos\")'.dependencies]");
    let k = mac.get("keyring").expect("keyring in the macOS target table");
    assert_eq!(k.get("version").and_then(|x| x.as_str()), Some("=3.6.3"), "keyring must be pinned with =3.6.3");
    assert_eq!(k.get("default-features").and_then(|x| x.as_bool()), Some(false), "default-features = false");
    let feats: Vec<&str> = k.get("features").and_then(|x| x.as_array()).expect("features").iter().filter_map(|x| x.as_str()).collect();
    assert_eq!(feats, vec!["apple-native"], "only the apple-native feature");
    // Not declared anywhere else (no Windows/Linux store, no other table).
    let raw = read("desktop/src-tauri/Cargo.toml");
    assert_eq!(raw.matches("keyring").count(), 1, "keyring must be declared exactly once");
    assert!(!raw.contains("keyring-core") && !raw.contains("keyring-store"));
}

#[test]
fn s05_no_mock_credential_builder_outside_tests() {
    assert_none("keyring mock store in so/brain", brain_hits(&["mock::", "set_default_credential_builder", "MockCredential", "mock_builder"], None));
}

// ---- S6: no file writes in so/brain ----
#[test]
fn s06_no_file_writes_in_so_brain() {
    assert_none(
        "file writes in so/brain",
        brain_hits(&["File::create", "fs::write", "temp_dir", "tempfile", "OpenOptions", "write_all(", "fs::copy", "fs::rename", "create_dir", "std::fs"], None),
    );
}

// ---- S7: no eval / innerHTML / emit ----
#[test]
fn s07_so_brain_never_evals_sets_inner_html_or_emits() {
    assert_none("eval/innerHTML/emit in so/brain", brain_hits(&["eval(", "innerHTML", ".emit(", "emit(", "emit_to(", "emit_all("], None));
}

// ---- S8: CSP unchanged; the webview never contacts a provider ----
#[test]
fn s08_csp_is_unchanged_since_step_3() {
    let now: serde_json::Value = serde_json::from_str(&read("desktop/src-tauri/tauri.conf.json")).unwrap();
    let then: serde_json::Value = serde_json::from_str(&at_tag("desktop/src-tauri/tauri.conf.json")).unwrap();
    assert_eq!(now["app"]["security"]["csp"], then["app"]["security"]["csp"], "csp changed");
    assert_eq!(now["app"]["security"]["devCsp"], then["app"]["security"]["devCsp"], "devCsp changed");
    let csp = now["app"]["security"]["csp"].as_str().unwrap();
    assert!(csp.contains("connect-src ipc: http://ipc.localhost;"), "connect-src must stay ipc only: {csp}");
}

#[test]
fn s08_no_network_api_or_provider_host_in_the_ui_code() {
    let mut hits = vec![];
    for f in files_under("desktop/src/so", &["ts", "tsx", "html"]) {
        let t = fs::read_to_string(&f).unwrap_or_default();
        for (i, l) in t.lines().enumerate() {
            if is_comment(l) {
                continue;
            }
            for needle in ["fetch(", "XMLHttpRequest", "WebSocket", "sendBeacon", "EventSource", "anthropic", "googleapis", "aistudio", "platform.claude"] {
                if l.contains(needle) {
                    hits.push(format!("{}:{}: {}", relp(&f), i + 1, l.trim()));
                }
            }
        }
    }
    assert_none("network API or provider host in desktop/src/so", hits);
}

// ---- S9: capabilities, build.rs, lib.rs: exactly the six commands, main window only ----
const CMDS: [&str; 6] = ["so_brain_status", "so_brain_set_provider", "so_brain_save_key", "so_brain_remove_key", "so_brain_check_key", "so_brain_open_console"];

#[test]
fn s09_six_allow_permissions_in_default_json_and_nothing_else_added() {
    let now: serde_json::Value = serde_json::from_str(&read("desktop/src-tauri/capabilities/default.json")).unwrap();
    let then: serde_json::Value = serde_json::from_str(&at_tag("desktop/src-tauri/capabilities/default.json")).unwrap();
    let perms = |v: &serde_json::Value| -> Vec<String> { v["permissions"].as_array().unwrap().iter().map(|x| x.as_str().unwrap().to_string()).collect() };
    let (n, t) = (perms(&now), perms(&then));
    let want: Vec<String> = CMDS.iter().map(|c| format!("allow-{}", c.replace('_', "-"))).collect();
    for w in &want {
        assert!(n.contains(w), "default.json must grant {w}");
    }
    let added: Vec<&String> = n.iter().filter(|x| !t.contains(x)).collect();
    assert_eq!(added.len(), 6, "exactly the six brain permissions are added, got {added:?}");
    assert!(t.iter().all(|x| n.contains(x)), "no permission was removed");
    for bad in ["opener", "shell", "http:", "fs:", "dialog", "core:default"] {
        assert!(!n.iter().any(|x| x.contains(bad)), "an {bad} permission must not be added");
    }
    assert_eq!(now["windows"], serde_json::json!(["main"]), "main window only");
}

#[test]
fn s09_no_other_capability_file_grants_the_brain_commands() {
    let mut hits = vec![];
    for f in files_under("desktop/src-tauri/capabilities", &["json"]) {
        if f.file_name().unwrap() == "default.json" {
            continue;
        }
        let t = fs::read_to_string(&f).unwrap_or_default();
        if t.contains("so-brain") || t.contains("so_brain") {
            hits.push(relp(&f));
        }
    }
    assert_none("capability files other than default.json granting brain commands", hits);
}

#[test]
fn s09_build_rs_and_lib_rs_list_the_six_commands() {
    let build = read("desktop/src-tauri/build.rs");
    let lib = read("desktop/src-tauri/src/lib.rs");
    for c in CMDS {
        assert!(build.contains(&format!("\"{c}\"")), "build.rs APP_COMMANDS is missing {c}");
        assert!(lib.contains(&format!("so::brain::commands::{c}")), "lib.rs does not register {c}");
    }
}

// ---- S10: the debug fake is compiled out of release ----
const GATE: &str = "cfg(debug_assertions)";
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
fn s10_the_gate_checker_can_fail() {
    assert_eq!(ungated("fn a() {\n    let p = FakeProvider::new();\n}\n", "FakeProvider"), vec![2]);
    assert!(ungated("#[cfg(debug_assertions)]\n{\n    let p = FakeProvider::new();\n}\n", "FakeProvider").is_empty());
}

#[test]
fn s10_fake_module_its_mod_line_and_every_call_site_are_gated() {
    let mod_rs = read(&format!("{BRAIN}/mod.rs"));
    assert!(mod_rs.contains("mod fake"), "so/brain/mod.rs must declare mod fake");
    assert!(ungated(&mod_rs, "mod fake").is_empty(), "`mod fake` is not under #[cfg(debug_assertions)]");
    let mut hits = vec![];
    let mut call_sites = 0;
    for f in files_under("desktop/src-tauri/src", &["rs"]) {
        let r = relp(&f);
        if r.contains("/so/tests/") || r.ends_with("brain/fake.rs") {
            continue;
        }
        let txt = strip_cfg_test(&fs::read_to_string(&f).unwrap_or_default());
        for needle in ["fake::", "FakeProvider", "SO_BRAIN_FAKE", "brain::fake"] {
            for line in ungated(&txt, needle) {
                hits.push(format!("{r}:{line}: `{needle}` is not under #[cfg(debug_assertions)]"));
            }
            if txt.contains(needle) && !r.ends_with("brain/mod.rs") {
                call_sites += 1;
            }
        }
    }
    assert_none("ungated debug-fake code", hits);
    assert!(call_sites > 0, "positive control: the app must wire the fake somewhere (so/stt/mod.rs), under cfg(debug_assertions)");
}

// ---- S11: Handy core changes only in Cargo.toml, lib.rs and bindings.ts ----
#[test]
fn s11_only_the_three_approved_handy_core_files_change_since_step_3() {
    let changed = changed_since_tag(&["desktop/src-tauri/src", "desktop/src", "desktop/src-tauri/Cargo.toml", "desktop/package.json"], "--diff-filter=ACDMR");
    let allowed = ["desktop/src-tauri/Cargo.toml", "desktop/src-tauri/src/lib.rs", "desktop/src/bindings.ts"];
    let core: Vec<&String> = changed
        .iter()
        .filter(|f| !f.starts_with("desktop/src-tauri/src/so/") && !f.starts_with("desktop/src/so/"))
        .filter(|f| !allowed.contains(&f.as_str()))
        .collect();
    assert!(core.is_empty(), "Handy core files changed beyond the three approved (gate 9 breach): {core:?}");
}

// ---- S12: no unwrap / expect in shipped so/brain code ----
// The plan says "on key or network results"; this is the safe superset (every shipped unwrap/expect except on
// a Mutex lock, which the project already writes as `unwrap_or_else(|e| e.into_inner())`).
#[test]
fn s12_no_unwrap_or_expect_in_shipped_so_brain_code() {
    let hits: Vec<String> = brain_code(None)
        .into_iter()
        .filter(|(_, _, l)| (l.contains(".unwrap()") || l.contains(".expect(") || l.contains(".unwrap_err()")) && !l.contains(".lock()"))
        .map(|(f, i, l)| format!("{f}:{i}: {}", l.trim()))
        .collect();
    assert_none("unwrap/expect in so/brain", hits);
}

// ---- S13: forbidden request fields and headers ----
#[test]
fn s13_no_temperature_tool_choice_or_x_api_key_in_request_code() {
    assert_none(
        "forbidden request strings in so/brain",
        brain_hits(&["temperature", "tool_choice", "x-api-key", "X-Api-Key", "top_p", "top_k", "topP", "topK", "\"tools\""], None),
    );
    // Positive control: the request code that must exist does.
    let claude = brain_file_code("claude.rs");
    assert!(claude.contains("output_config") && claude.contains("json_schema") && claude.contains("anthropic-version"), "claude.rs request body");
    let gemini = brain_file_code("gemini.rs");
    assert!(gemini.contains("responseJsonSchema") && gemini.contains("thinkingLevel") && gemini.contains("generateContent"), "gemini.rs request body");
}

// ---- K11 (static half): key-touching commands run keyring + network inside spawn_blocking ----
#[test]
fn k11_key_commands_use_spawn_blocking_and_only_save_key_takes_a_key() {
    let txt = brain_file_code("commands.rs");
    assert!(txt.matches("spawn_blocking").count() >= 4, "status, save_key, remove_key and check_key must each use spawn_blocking");
    for name in ["so_brain_status", "so_brain_save_key", "so_brain_remove_key", "so_brain_check_key"] {
        assert!(txt.contains(&format!("pub async fn {name}")), "{name} must be async");
    }
    // The only command with a String argument that can hold a key is save_key.
    let mut with_string_arg = vec![];
    for l in txt.lines() {
        let t = l.trim();
        if t.starts_with("pub async fn so_brain_") || t.starts_with("pub fn so_brain_") {
            if t.contains(": String") {
                with_string_arg.push(t.to_string());
            }
        }
    }
    assert_eq!(with_string_arg.len(), 1, "exactly one command takes a String: {with_string_arg:?}");
    assert!(with_string_arg[0].contains("so_brain_save_key"), "{with_string_arg:?}");
    // No command hands text back that could be a key.
    for l in txt.lines() {
        let t = l.trim();
        if t.starts_with("pub async fn so_brain_") || t.starts_with("pub fn so_brain_") {
            let ret = t.split("->").nth(1).unwrap_or("");
            assert!(!ret.contains("ApiKey"), "a command returns an ApiKey: {t}");
        }
    }
    let n = txt.matches("#[tauri::command]").count();
    assert_eq!(n, 6, "six commands");
    assert_none("tauri commands outside commands.rs in so/brain", brain_hits(&["#[tauri::command]"], Some("commands.rs")));
}

// ---- K6 (static half): the settings file never sees a key ----
#[test]
fn k6_config_rs_never_touches_keys() {
    let txt = brain_file_code("config.rs");
    for bad in ["ApiKey", "KeyStore", "expose", "keyring", "KeyringStore"] {
        assert!(!txt.contains(bad), "config.rs (so_settings.json) must not reference {bad}");
    }
    assert!(txt.contains("so_settings") || txt.contains("STORE_PATH") || txt.contains("store("), "config.rs must use our own settings store");
}

// ---- D6: earlier tests are not edited ----
#[test]
fn d6_step_3_and_earlier_test_files_are_unchanged_since_the_step_3_tag() {
    let changed = changed_since_tag(
        &["desktop/src-tauri/src/so/tests", "tests/strip-checks/tests", "tests/so"],
        "--diff-filter=MDR",
    );
    // mod.rs: one `mod` line per new test file. settings-ui.test.ts: V5's import allow-list gains "BrainSection"
    // (step 4 plan U8 adds BrainSection to GeneralPane, which the old list would reject).
    let allowed = ["desktop/src-tauri/src/so/tests/mod.rs", "tests/so/settings-ui.test.ts"];
    let bad: Vec<&String> = changed.iter().filter(|f| !allowed.contains(&f.as_str())).collect();
    assert!(bad.is_empty(), "earlier-step tests were edited or deleted: {bad:?}");
}

#[test]
fn d6_the_only_edit_to_settings_ui_test_is_the_one_allow_list_line() {
    require_tag();
    let o = git_top(&["diff", "--numstat", BASE_TAG, "--", "tests/so/settings-ui.test.ts"]);
    let out = String::from_utf8_lossy(&o.stdout).to_string();
    let mut it = out.split_whitespace();
    let (add, del) = (it.next().unwrap_or("0"), it.next().unwrap_or("0"));
    assert_eq!((add, del), ("1", "0"), "settings-ui.test.ts may only gain the BrainSection allow-list line: {out}");
}

#[test]
fn d6_the_only_edit_to_tests_mod_rs_is_added_mod_lines() {
    require_tag();
    let o = git_top(&["diff", "--numstat", BASE_TAG, "--", "desktop/src-tauri/src/so/tests/mod.rs"]);
    let out = String::from_utf8_lossy(&o.stdout).to_string();
    let del = out.split_whitespace().nth(1).unwrap_or("0");
    assert_eq!(del, "0", "mod.rs may only gain lines: {out}");
}

// ---- structure ----
#[test]
fn step4_files_exist() {
    for f in ["mod", "plan", "prompt", "provider", "http", "gemini", "claude", "keys", "brain", "config", "commands", "metrics", "fake"] {
        assert!(p(&format!("{BRAIN}/{f}.rs")).exists(), "missing so/brain/{f}.rs");
    }
    for f in [
        "brain_support", "brain_plan_tests", "brain_prompt_tests", "brain_http_tests", "brain_keys_tests", "brain_flow_tests",
        "brain_seam_tests", "brain_ipc_tests", "brain_release_tests", "brain_fake_tests", "brain_keychain_real",
    ] {
        assert!(p(&format!("{SO}/tests/{f}.rs")).exists(), "missing so/tests/{f}.rs");
    }
    for f in ["BrainSection.tsx", "brainView.ts"] {
        assert!(p(&format!("desktop/src/so/settings/{f}")).exists(), "missing desktop/src/so/settings/{f}");
    }
    assert!(p("tests/so/brainSection.test.ts").exists(), "missing tests/so/brainSection.test.ts");
}

#[test]
fn every_new_brain_file_has_an_spdx_header() {
    let mut missing = vec![];
    for f in files_under(BRAIN, &["rs"]) {
        let t = fs::read_to_string(&f).unwrap_or_default();
        if !t.lines().next().is_some_and(|l| l.contains("SPDX-License-Identifier: FSL-1.1-MIT")) {
            missing.push(relp(&f));
        }
    }
    assert_none("so/brain files without the SPDX header on line 1", missing);
}
