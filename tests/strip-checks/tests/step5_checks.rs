// SPDX-License-Identifier: FSL-1.1-MIT
//! Step 5 static checks S1-S15 (+ S16 constants agree). Read files only; never compile app code. Same style
//! as step4_checks.rs. Run: `cargo test --manifest-path tests/strip-checks/Cargo.toml --test step5_checks`.
//! Numbers match draft/step-5-plan.md. Each scanner has a self-test that proves it can fail.

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
const LINK: &str = "desktop/src-tauri/src/so/link";
const EXT: &str = "chrome-extension";
const BASE_TAG: &str = "step-4";

/// Removes every `#[cfg(test)]` item so scans look at shipped code only.
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

/// Shipped code lines (non-comment, non-test) of every `so/link` file, optionally skipping one file name.
fn link_code(skip: Option<&str>) -> Vec<(String, usize, String)> {
    let files = files_under(LINK, &["rs"]);
    assert!(files.len() >= 7, "so/link must hold mod, pairing, token, protocol, server, commands, metrics: {files:?}");
    let mut out = vec![];
    for f in files {
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
fn link_hits(needles: &[&str], skip: Option<&str>) -> Vec<String> {
    link_code(skip)
        .into_iter()
        .filter(|(_, _, l)| needles.iter().any(|n| contains_word_start(l, n)))
        .map(|(f, i, _)| format!("{f}:{i}"))
        .collect()
}
fn link_file_code(name: &str) -> String {
    let txt = strip_cfg_test(&read(&format!("{LINK}/{name}")));
    txt.lines().filter(|l| !is_comment(l)).collect::<Vec<_>>().join("\n")
}

fn git_top(args: &[&str]) -> std::process::Output {
    Command::new("git").args(args).current_dir(root()).output().expect("run git")
}
fn require_tag() {
    let o = git_top(&["rev-parse", "--verify", "--quiet", &format!("{BASE_TAG}^{{commit}}")]);
    assert!(o.status.success(), "git tag {BASE_TAG} not found: fetch tags (`git fetch --tags`) to run S8/S9/S10/S12/S14");
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

// ======================= extension helpers =======================
/// The extension's own files (manifest, sw, popup, lib/*), comments excluded line by line for JS.
fn ext_files() -> Vec<PathBuf> {
    files_under(EXT, &["js", "html", "json"]).into_iter().filter(|f| !relp(f).contains("node_modules")).collect()
}
fn ext_code_lines(exts: &[&str]) -> Vec<(String, usize, String)> {
    let mut out = vec![];
    for f in ext_files() {
        if !f.extension().is_some_and(|x| exts.iter().any(|e| x == *e)) {
            continue;
        }
        for (i, l) in fs::read_to_string(&f).unwrap_or_default().lines().enumerate() {
            if !is_comment(l) {
                out.push((relp(&f), i + 1, l.to_string()));
            }
        }
    }
    out
}
fn ext_hits(needles: &[&str], exts: &[&str]) -> Vec<String> {
    ext_code_lines(exts).into_iter().filter(|(_, _, l)| needles.iter().any(|n| l.contains(n))).map(|(f, i, _)| format!("{f}:{i}")).collect()
}
fn require_ext_files() {
    for f in ["manifest.json", "sw.js", "popup.html", "popup.js", "lib/config.js", "lib/link.js", "lib/protocol.js", "lib/proof.js", "lib/state.js", "lib/backoff.js", "lib/view.js"] {
        assert!(p(&format!("{EXT}/{f}")).is_file(), "{EXT}/{f} is missing (increment 5)");
    }
}
/// The text between the parens of every `needle(` call, balanced; string literals are skipped.
fn call_args(src: &str, needle: &str) -> Vec<String> {
    let mut out = vec![];
    let mut from = 0;
    while let Some(i) = src[from..].find(needle) {
        let start = from + i + needle.len();
        let mut depth = 1;
        let mut q: Option<char> = None;
        let mut end = start;
        for (k, c) in src[start..].char_indices() {
            if let Some(quote) = q {
                if c == quote {
                    q = None;
                }
                continue;
            }
            match c {
                '"' | '\'' | '`' => q = Some(c),
                '(' => depth += 1,
                ')' => {
                    depth -= 1;
                    if depth == 0 {
                        end = start + k;
                        break;
                    }
                }
                _ => {}
            }
        }
        out.push(src[start..end].to_string());
        from = end.max(start);
    }
    out
}
/// Top-level keys of an object literal `{ a: 1, b, "c": {x: 1} }`; `None` if it is not an object literal.
fn object_keys(arg: &str) -> Option<Vec<String>> {
    let t = arg.trim();
    let inner = t.strip_prefix('{')?.strip_suffix('}')?;
    let (mut depth, mut parts, mut cur) = (0, vec![], String::new());
    for c in inner.chars() {
        match c {
            '{' | '[' | '(' => depth += 1,
            '}' | ']' | ')' => depth -= 1,
            ',' if depth == 0 => {
                parts.push(std::mem::take(&mut cur));
                continue;
            }
            _ => {}
        }
        cur.push(c);
    }
    parts.push(cur);
    let mut keys = vec![];
    for part in parts.iter().map(|x| x.trim()).filter(|x| !x.is_empty()) {
        let key = part.split(':').next().unwrap().trim().trim_matches(|c| c == '"' || c == '\'' || c == '`').to_string();
        if key.starts_with('[') || key.starts_with("...") {
            return None;
        }
        keys.push(key);
    }
    Some(keys)
}

// ======================= self-tests of the scanners =======================
#[test]
fn scanners_can_fail() {
    assert!(contains_word_start("    log::info!(\"x\");", "log::"));
    assert!(!contains_word_start("    BlogEntry::new()", "log::"));
    let src = "fn a() {}\n#[cfg(test)]\nfn t() { log::info!(\"x\"); }\nfn b() { let x = 1; }\n";
    let kept = strip_cfg_test(src);
    assert!(!kept.contains("log::") && kept.contains("fn b()") && kept.contains("fn a()"), "{kept}");
    assert_eq!(call_args("a.set({ a: f(1, 2), 'b': 3 }); b.set(x)", "set("), vec!["{ a: f(1, 2), 'b': 3 }".to_string(), "x".to_string()]);
    assert_eq!(object_keys("{ a: f(1, 2), 'b': 3, c }").unwrap(), vec!["a", "b", "c"]);
    assert_eq!(object_keys("{ link: { state, retry_s }, token }").unwrap(), vec!["link", "token"]);
    assert!(object_keys("data").is_none() && object_keys("{ [k]: 1 }").is_none() && object_keys("{ ...rest }").is_none());
}

// ---- S1: only metrics.rs under so/link may log; its fn takes integers only ----
const LOG_NEEDLES: [&str; 12] = ["log::", "tracing::", "info!(", "debug!(", "warn!(", "error!(", "trace!(", "println!(", "eprintln!(", "print!(", "eprint!(", "dbg!("];

#[test]
fn s01_no_logging_or_printing_in_so_link_except_metrics() {
    assert_none("log/print macros in so/link (outside metrics.rs)", link_hits(&LOG_NEEDLES, Some("metrics.rs")));
}

#[test]
fn s01_metrics_takes_only_numbers_and_logs_the_planned_line() {
    const OK: [&str; 14] = ["u8", "u16", "u32", "u64", "usize", "i8", "i16", "i32", "i64", "isize", "bool", "f32", "f64", "&[u8]"];
    let txt = link_file_code("metrics.rs");
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
    assert_eq!(fns, 1, "metrics.rs defines exactly `link`");
    for part in ["link outcome=", "code=", "ms="] {
        assert!(txt.contains(part), "metrics.rs must log `{part}`");
    }
}

// ---- S2: loopback only ----
#[test]
fn s02_bind_is_127_0_0_1_only() {
    let server = link_file_code("server.rs");
    assert!(server.contains("\"127.0.0.1\""), "server.rs must bind the literal \"127.0.0.1\"");
    assert_none(
        "wildcard or name binds in so/link",
        link_hits(&["0.0.0.0", "\"::\"", "localhost", "UNSPECIFIED", "Ipv6", "[::", "::1", "to_socket_addrs"], None),
    );
}

// ---- S3: LinkToken never copies, prints or serialises its secret ----
#[test]
fn s03_link_token_has_no_clone_serialize_display_type_and_to_hex_is_crate_private() {
    let txt = link_file_code("token.rs");
    let lines: Vec<&str> = txt.lines().collect();
    let at = lines.iter().position(|l| l.contains("pub struct LinkToken")).expect("pub struct LinkToken");
    for l in &lines[at.saturating_sub(4)..at] {
        if l.contains("derive(") {
            for bad in ["Clone", "Copy", "Serialize", "Deserialize", "Type", "Debug", "Display", "PartialEq", "Hash", "Default"] {
                assert!(!l.contains(bad), "LinkToken must not derive {bad}");
            }
        }
    }
    for bad in [
        "Clone for LinkToken", "Display for LinkToken", "Serialize for LinkToken", "Type for LinkToken", "Deserialize<'de> for LinkToken", "Copy for LinkToken",
        "From<LinkToken>", "AsRef<str> for LinkToken", "AsRef<[u8]> for LinkToken", "Deref for LinkToken", "Into<String> for LinkToken", "PartialEq for LinkToken",
    ] {
        assert!(!txt.contains(bad), "LinkToken must not implement: {bad}");
    }
    assert!(txt.contains("Debug for LinkToken"), "LinkToken needs a hand-written Debug impl");
    assert!(txt.contains("LinkToken(<redacted>)"), "its Debug text is `LinkToken(<redacted>)`");
    assert!(txt.contains("pub(crate) fn to_hex"), "to_hex must be pub(crate)");
    assert!(!txt.contains("pub fn to_hex"), "to_hex must not be public");
    // `to_hex()` is used only where the secret legitimately leaves the token: token.rs and server.rs (pairing reply, store).
    let mut bad = vec![];
    for f in files_under(SO, &["rs"]) {
        let r = relp(&f);
        if r.contains("/so/tests/") || r.ends_with("link/token.rs") || r.ends_with("link/server.rs") {
            continue;
        }
        let code = strip_cfg_test(&fs::read_to_string(&f).unwrap_or_default());
        for (i, l) in code.lines().enumerate() {
            if !is_comment(l) && l.contains("to_hex(") {
                bad.push(format!("{r}:{}", i + 1));
            }
        }
    }
    assert_none("to_hex() outside token.rs / server.rs", bad);
}

// ---- S4: the extension's socket URL ----
#[test]
fn s04_ws_url_is_the_fixed_loopback_url_without_query_or_fragment() {
    let cfg = read(&format!("{EXT}/lib/config.js"));
    let line = cfg.lines().find(|l| l.contains("WS_URL") && l.contains("export")).expect("config.js exports WS_URL");
    assert!(line.contains("\"ws://127.0.0.1:47815/\""), "WS_URL must be exactly \"ws://127.0.0.1:47815/\"");
    let url = "ws://127.0.0.1:47815/";
    assert!(!url.contains('?') && !url.contains('#'));
    for (f, i, l) in ext_code_lines(&["js"]) {
        if f.ends_with("config.js") {
            continue;
        }
        assert!(!l.contains("ws://") && !l.contains("wss://"), "{f}:{i}: only config.js may spell a socket URL");
    }
}

#[test]
fn s04_every_websocket_is_opened_on_ws_url_alone_and_no_secret_goes_into_a_url() {
    require_ext_files();
    let mut sockets = 0;
    for f in ext_files().into_iter().filter(|f| f.extension().is_some_and(|x| x == "js")) {
        let src: String = fs::read_to_string(&f).unwrap().lines().filter(|l| !is_comment(l)).collect::<Vec<_>>().join("\n");
        for arg in call_args(&src, "new WebSocket(") {
            sockets += 1;
            assert_eq!(arg.trim(), "WS_URL", "{}: `new WebSocket(..)` must pass exactly WS_URL (no concatenation, no query)", relp(&f));
        }
    }
    assert!(sockets >= 1, "positive control: link.js must open the socket with `new WebSocket(WS_URL)`");
    // Lines that build or mention a URL never also mention a secret.
    for (f, i, l) in ext_code_lines(&["js"]) {
        if l.contains("WS_URL") || l.contains("new WebSocket") || l.contains("ws://") || l.contains("URLSearchParams") || l.contains("new URL(") {
            for secret in ["token", "code", "proof", "nonce"] {
                assert!(!l.to_lowercase().contains(secret), "{f}:{i}: a URL line mentions a secret-named word");
            }
        }
    }
    assert_none("console.* in the extension", ext_hits(&["console."], &["js", "html"]));
}

// ---- S5: no HTML injection, eval or remote script in the extension ----
#[test]
fn s05_no_innerhtml_eval_remote_or_inline_script() {
    require_ext_files();
    assert_none("HTML injection / eval in the extension", ext_hits(&["innerHTML", "outerHTML", "insertAdjacentHTML", "eval(", "new Function", "document.write", "setTimeout(\"", "setInterval(\""], &["js", "html"]));
    let html = read(&format!("{EXT}/popup.html"));
    let scripts: Vec<&str> = html.match_indices("<script").map(|(i, _)| &html[i..html[i..].find('>').map_or(html.len(), |e| i + e)]).collect();
    assert!(!scripts.is_empty(), "popup.html must load popup.js");
    for s in scripts {
        assert!(s.contains("src=\"popup.js\"") || s.contains("src=\"./popup.js\""), "only the local popup.js may be loaded, found: {s}");
        assert!(!s.contains("http"), "remote script: {s}");
    }
    assert!(!["onclick=", "onload=", "onerror=", "onsubmit=", "onchange=", "oninput="].iter().any(|h| html.contains(h)), "inline event handlers are inline scripts");
    assert!(!html.contains("javascript:"));
}

// ---- S6: the manifest ----
#[test]
fn s06_manifest_is_mv3_with_a_key_two_permissions_and_nothing_risky() {
    let m: serde_json::Value = serde_json::from_str(&read(&format!("{EXT}/manifest.json"))).expect("manifest.json is JSON");
    assert_eq!(m["manifest_version"], 3);
    let key = m["key"].as_str().expect("`key` present");
    assert!(key.len() > 100 && key.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'+' || b == b'/' || b == b'='), "key must be the base64 public key");
    let min: u32 = m["minimum_chrome_version"].as_str().expect("minimum_chrome_version").split('.').next().unwrap().parse().unwrap();
    assert!(min >= 116, "minimum_chrome_version must be >= 116, got {min}");
    // Step 6 (approved Q4): + scripting and exactly three https hosts. step6_checks S1 is the source of truth.
    assert_eq!(m["permissions"], serde_json::json!(["storage", "alarms", "scripting"]), "permissions exactly storage + alarms + scripting");
    let mut hosts: Vec<&str> = m["host_permissions"].as_array().expect("host_permissions").iter().map(|x| x.as_str().unwrap()).collect();
    hosts.sort();
    assert_eq!(hosts, ["https://calendar.google.com/*", "https://docs.google.com/*", "https://www.youtube.com/*"]);
    let then: serde_json::Value = serde_json::from_str(&String::from_utf8_lossy(&git_top(&["show", "step-5:chrome-extension/manifest.json"]).stdout)).expect("manifest at step-5");
    assert_eq!(m["key"], then["key"], "the key must not change");
    for bad in ["content_scripts", "externally_connectable", "web_accessible_resources", "content_security_policy", "optional_permissions", "optional_host_permissions", "update_url"] {
        assert!(m.get(bad).is_none(), "manifest must not have {bad}");
    }
    assert!(!m.to_string().contains("<all_urls>") && !m.to_string().contains("\"tabs\""));
    assert_eq!(m["background"]["service_worker"], "sw.js");
    assert_eq!(m["background"]["type"], "module");
    assert_eq!(m["action"]["default_popup"], "popup.html");
}

// ---- S7: storage keys ----
#[test]
fn s07_storage_writes_only_token_and_link_the_code_never_and_no_removal_of_the_token() {
    require_ext_files();
    let mut sets = 0;
    for f in ext_files().into_iter().filter(|f| f.extension().is_some_and(|x| x == "js")) {
        let src: String = fs::read_to_string(&f).unwrap().lines().filter(|l| !is_comment(l)).collect::<Vec<_>>().join("\n");
        for arg in call_args(&src, "local.set(") {
            sets += 1;
            let keys = object_keys(&arg).unwrap_or_else(|| panic!("{}: storage.local.set must take an object literal", relp(&f)));
            for k in &keys {
                assert!(k == "token" || k == "link", "{}: storage key `{k}` is not allowed (only token, link)", relp(&f));
            }
            assert!(!arg.contains("code"), "{}: the pairing code must never reach storage", relp(&f));
        }
        for arg in call_args(&src, "local.remove(") {
            assert!(!arg.contains("token"), "{}: the token is never removed", relp(&f));
        }
        assert!(!src.contains("local.clear("), "{}: storage.local.clear is not allowed", relp(&f));
        assert!(!src.contains("storage.sync") && !src.contains("storage.session"), "{}: only storage.local", relp(&f));
    }
    assert!(sets >= 1, "positive control: link.js writes storage.local (token and link)");
}

// ---- S8: Cargo.lock vs tag step-4, Cargo.toml pins ----
fn lock_pairs(src: &str) -> std::collections::BTreeSet<(String, String)> {
    let v: toml::Value = toml::from_str(src).expect("Cargo.lock is TOML");
    v["package"].as_array().unwrap().iter().map(|x| (x["name"].as_str().unwrap().to_string(), x["version"].as_str().unwrap().to_string())).collect()
}

#[test]
fn s08_cargo_lock_gains_exactly_five_packages_and_changes_no_old_one() {
    let old = lock_pairs(&at_tag("desktop/src-tauri/Cargo.lock"));
    let new = lock_pairs(&read("desktop/src-tauri/Cargo.lock"));
    let removed: Vec<_> = old.difference(&new).collect();
    assert!(removed.is_empty(), "an old package changed version or vanished: {removed:?}");
    let added: Vec<_> = new.difference(&old).cloned().collect();
    let names: Vec<&str> = added.iter().map(|x| x.0.as_str()).collect();
    assert_eq!(names, vec!["data-encoding", "hmac", "sha1", "tokio-tungstenite", "tungstenite"], "new packages must be exactly these five: {added:?}");
    let ver = |n: &str| added.iter().find(|x| x.0 == n).unwrap().1.clone();
    assert_eq!(ver("hmac"), "0.12.1");
    assert_eq!(ver("sha1"), "0.10.7");
    assert_eq!(ver("tokio-tungstenite"), "0.29.0");
    assert_eq!(ver("tungstenite"), "0.29.0");
}

#[test]
fn s08_cargo_toml_pins_the_new_crates() {
    let v: toml::Value = toml::from_str(&read("desktop/src-tauri/Cargo.toml")).unwrap();
    let d = &v["dependencies"];
    let tt = &d["tokio-tungstenite"];
    assert_eq!(tt["version"].as_str(), Some("=0.29.0"), "tokio-tungstenite must be pinned =0.29.0");
    assert_eq!(tt["default-features"].as_bool(), Some(false));
    let feats: Vec<&str> = tt["features"].as_array().expect("features").iter().filter_map(|x| x.as_str()).collect();
    assert_eq!(feats, vec!["handshake"], "tokio-tungstenite: handshake only (no connect, no TLS)");
    assert_eq!(d["hmac"].as_str(), Some("=0.12.1"), "hmac must be pinned =0.12.1");
    assert!(d.get("subtle").is_some() && d.get("getrandom").is_some(), "subtle and getrandom are direct dependencies");
    let tokio: Vec<&str> = d["tokio"]["features"].as_array().unwrap().iter().filter_map(|x| x.as_str()).collect();
    for f in ["process", "net", "time"] {
        assert!(tokio.contains(&f), "tokio feature {f} is missing: {tokio:?}");
    }
    assert!(!read("desktop/src-tauri/Cargo.toml").contains("native-tls") || !read("desktop/src-tauri/Cargo.toml").contains("tokio-tungstenite-tls"));
}

// ---- S9: capabilities, build.rs, lib.rs: exactly three commands, main window only ----
const CMDS: [&str; 3] = ["so_link_status", "so_link_start_pairing", "so_link_cancel_pairing"];

#[test]
fn s09_three_allow_permissions_in_default_json_only() {
    let now: serde_json::Value = serde_json::from_str(&read("desktop/src-tauri/capabilities/default.json")).unwrap();
    let then: serde_json::Value = serde_json::from_str(&at_tag("desktop/src-tauri/capabilities/default.json")).unwrap();
    let perms = |v: &serde_json::Value| -> Vec<String> { v["permissions"].as_array().unwrap().iter().map(|x| x.as_str().unwrap().to_string()).collect() };
    let (n, t) = (perms(&now), perms(&then));
    let want: Vec<String> = CMDS.iter().map(|c| format!("allow-{}", c.replace('_', "-"))).collect();
    for w in &want {
        assert!(n.contains(w), "default.json must grant {w}");
    }
    let added: Vec<&String> = n.iter().filter(|x| !t.contains(x)).collect();
    assert_eq!(added.len(), 3, "exactly the three link permissions are added, got {added:?}");
    assert!(t.iter().all(|x| n.contains(x)), "no permission was removed");
    assert_eq!(now["windows"], serde_json::json!(["main"]), "main window only");
}

#[test]
fn s09_no_other_capability_file_grants_the_link_commands_and_build_rs_and_lib_rs_list_them() {
    let mut hits = vec![];
    for f in files_under("desktop/src-tauri/capabilities", &["json"]) {
        if f.file_name().unwrap() == "default.json" {
            continue;
        }
        let t = fs::read_to_string(&f).unwrap_or_default();
        if t.contains("so-link") || t.contains("so_link") {
            hits.push(relp(&f));
        }
    }
    assert_none("capability files other than default.json granting link commands", hits);
    let build = read("desktop/src-tauri/build.rs");
    let lib = read("desktop/src-tauri/src/lib.rs");
    for c in CMDS {
        assert!(build.contains(&format!("\"{c}\"")), "build.rs APP_COMMANDS is missing {c}");
        assert!(lib.contains(&format!("so::link::commands::{c}")), "lib.rs does not register {c}");
    }
}

// ---- S10: Handy core changes only in Cargo.toml, lib.rs, bindings.ts ----
#[test]
fn s10_only_the_three_approved_handy_core_files_change_since_step_4() {
    let changed = changed_since_tag(&["desktop/src-tauri/src", "desktop/src", "desktop/src-tauri/Cargo.toml", "desktop/package.json"], "--diff-filter=ACDMR");
    let allowed = ["desktop/src-tauri/Cargo.toml", "desktop/src-tauri/src/lib.rs", "desktop/src/bindings.ts"];
    let core: Vec<&String> = changed
        .iter()
        .filter(|f| !f.starts_with("desktop/src-tauri/src/so/") && !f.starts_with("desktop/src/so/"))
        .filter(|f| !allowed.contains(&f.as_str()))
        .collect();
    assert!(core.is_empty(), "Handy core files changed beyond the three approved (gate 9 breach): {core:?}");
}

// ---- S11: no unwrap / expect in shipped so/link code ----
#[test]
fn s11_no_unwrap_or_expect_in_shipped_so_link_code() {
    let hits: Vec<String> = link_code(None)
        .into_iter()
        .filter(|(_, _, l)| (l.contains(".unwrap()") || l.contains(".expect(") || l.contains(".unwrap_err()")) && !l.contains(".lock()"))
        .map(|(f, i, _)| format!("{f}:{i}"))
        .collect();
    assert_none("unwrap/expect in so/link", hits);
}

// ---- S11b: no panic paths in the finished link files (commands.rs and mod.rs still hold stubs until increment 4) ----
#[test]
fn s11b_no_unreachable_panic_todo_in_token_pairing_protocol_server() {
    let hits: Vec<String> = link_code(None)
        .into_iter()
        .filter(|(f, _, _)| ["token.rs", "pairing.rs", "protocol.rs", "server.rs"].iter().any(|n| f.ends_with(n)))
        .filter(|(_, _, l)| ["unreachable!(", "panic!(", "todo!(", "unimplemented!("].iter().any(|n| l.contains(n)))
        .map(|(f, i, _)| format!("{f}:{i}"))
        .collect();
    assert_none("panic paths in shipped so/link code", hits);
}

// ---- S12: the UI still has no network API; CSP unchanged ----
#[test]
fn s12_no_network_api_in_the_ui_code_and_csp_unchanged_since_step_4() {
    let mut hits = vec![];
    for f in files_under("desktop/src/so", &["ts", "tsx", "html"]) {
        let t = fs::read_to_string(&f).unwrap_or_default();
        for (i, l) in t.lines().enumerate() {
            if is_comment(l) {
                continue;
            }
            for needle in ["fetch(", "XMLHttpRequest", "WebSocket", "sendBeacon", "EventSource", "anthropic", "googleapis", "aistudio", "platform.claude"] {
                if l.contains(needle) {
                    hits.push(format!("{}:{}", relp(&f), i + 1));
                }
            }
        }
    }
    assert_none("network API or provider host in desktop/src/so", hits);
    let now: serde_json::Value = serde_json::from_str(&read("desktop/src-tauri/tauri.conf.json")).unwrap();
    let then: serde_json::Value = serde_json::from_str(&at_tag("desktop/src-tauri/tauri.conf.json")).unwrap();
    assert_eq!(now["app"]["security"]["csp"], then["app"]["security"]["csp"], "csp changed");
    assert_eq!(now["app"]["security"]["devCsp"], then["app"]["security"]["devCsp"], "devCsp changed");
}

// ---- S13: Origin compare is whole-string `==`; the code compare is constant time ----
fn fn_body(src: &str, name: &str) -> String {
    let at = src.find(&format!("fn {name}")).unwrap_or_else(|| panic!("fn {name} not found"));
    let open = at + src[at..].find('{').expect("body");
    let mut depth = 0;
    for (i, c) in src[open..].char_indices() {
        match c {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return src[open..open + i + 1].to_string();
                }
            }
            _ => {}
        }
    }
    panic!("unbalanced braces in fn {name}")
}

#[test]
fn s13_check_handshake_compares_the_whole_origin_with_equality_and_never_normalises() {
    let body = fn_body(&link_file_code("server.rs"), "check_handshake");
    assert!(body.contains("allowed_origin"), "check_handshake must compare against cfg.allowed_origin");
    assert!(body.contains("==") || body.contains("!="), "the Origin compare is `==` / `!=`");
    for bad in ["starts_with", "ends_with", "contains", "to_lowercase", "to_uppercase", "to_ascii_lowercase", "eq_ignore_ascii_case", "trim", "strip_prefix", "strip_suffix", "split", "find("] {
        assert!(!body.contains(bad), "check_handshake must not use `{bad}`: no prefix/case/slash tolerance");
    }
    assert!(body.contains("Origin") && body.contains("Host") && body.contains("Path"), "all three rejections live in check_handshake");
}

#[test]
fn s13_the_pairing_code_compare_is_constant_time() {
    let pairing = link_file_code("pairing.rs");
    assert!(pairing.contains("ct_eq"), "pairing.rs must compare the code with subtle's ct_eq");
    let body = fn_body(&pairing, "try_code");
    assert!(body.contains("ct_eq"), "try_code must use ct_eq");
    assert!(!body.contains("input ==") && !body.contains("== input") && !body.contains("code =="), "no `==` on the code in try_code");
    let token = link_file_code("token.rs");
    assert!(token.contains("verify_slice"), "token.rs must verify proofs with Mac::verify_slice (constant time)");
}

// ---- S14: SPDX headers; REUSE covers chrome-extension/** ----
fn glob_match(pat: &str, s: &str) -> bool {
    fn go(p: &[u8], s: &[u8]) -> bool {
        match p.first() {
            None => s.is_empty(),
            Some(b'*') if p.get(1) == Some(&b'*') => {
                let rest = if p.get(2) == Some(&b'/') { &p[3..] } else { &p[2..] };
                (0..=s.len()).any(|i| go(rest, &s[i..]))
            }
            Some(b'*') => (0..=s.len()).take_while(|&i| i == 0 || s[i - 1] != b'/').any(|i| go(&p[1..], &s[i..])),
            Some(c) => s.first() == Some(c) && go(&p[1..], &s[1..]),
        }
    }
    go(pat.as_bytes(), s.as_bytes())
}
fn reuse_paths() -> Vec<(String, String)> {
    let v: toml::Value = toml::from_str(&read("REUSE.toml")).unwrap();
    let mut out = vec![];
    for a in v["annotations"].as_array().unwrap() {
        let lic = a["SPDX-License-Identifier"].as_str().unwrap_or("").to_string();
        let paths: Vec<String> = match &a["path"] {
            toml::Value::String(s) => vec![s.clone()],
            toml::Value::Array(x) => x.iter().filter_map(|y| y.as_str().map(String::from)).collect(),
            _ => vec![],
        };
        for p in paths {
            out.push((p, lic.clone()));
        }
    }
    out
}

#[test]
fn s14_the_glob_matcher_can_fail() {
    assert!(glob_match("chrome-extension/**", "chrome-extension/lib/a.js"));
    assert!(glob_match("chrome-extension/**", "chrome-extension/manifest.json"));
    assert!(!glob_match("chrome-extension/**", "other/manifest.json"));
    assert!(!glob_match("a/*.js", "a/b/c.js"));
}

#[test]
fn s14_reuse_toml_covers_the_extension_as_fsl() {
    let hit = reuse_paths().into_iter().find(|(p, _)| p == "chrome-extension/**");
    assert_eq!(hit.map(|x| x.1).as_deref(), Some("FSL-1.1-MIT"), "REUSE.toml must list chrome-extension/** under FSL-1.1-MIT");
}

#[test]
fn s14_every_new_file_has_an_spdx_header_or_a_reuse_entry() {
    require_tag();
    let mut files = changed_since_tag(&["desktop/src-tauri/src/so", "desktop/src/so", EXT, "tests", "scripts"], "--diff-filter=A");
    let others = git_top(&["ls-files", "--others", "--exclude-standard", "--", "desktop/src-tauri/src/so", "desktop/src/so", EXT, "tests", "scripts"]);
    files.extend(String::from_utf8_lossy(&others.stdout).lines().map(String::from));
    files.retain(|f| !f.contains("node_modules") && !f.ends_with("Cargo.lock"));
    let reuse = reuse_paths();
    let mut missing = vec![];
    for f in &files {
        let Ok(text) = fs::read_to_string(p(f)) else { continue };
        let head: String = text.lines().take(12).collect::<Vec<_>>().join("\n");
        let sidecar = p(&format!("{f}.license")).is_file();
        if !head.contains("SPDX-License-Identifier") && !sidecar && !reuse.iter().any(|(g, _)| glob_match(g, f)) {
            missing.push(f.clone());
        }
    }
    assert_none("new files with no SPDX header and no REUSE.toml entry", missing);
}

// ---- S15: lib.rs silences the WebSocket libraries in BOTH log builders ----
fn lib_rs() -> String {
    read("desktop/src-tauri/src/lib.rs")
}
fn silenced_in(section: &str) -> bool {
    let off = section.contains("LevelFilter::Off");
    let by_list = section.contains("LOG_SILENCED");
    let literal = section.contains("\"tungstenite\"") && section.contains("\"tokio_tungstenite\"");
    off && (by_list || literal)
}

#[test]
fn s15_the_log_silencer_can_fail() {
    assert!(silenced_in("for t in LOG_SILENCED { b.filter_module(t, log::LevelFilter::Off); }"));
    assert!(silenced_in(".level_for(\"tungstenite\", log::LevelFilter::Off).level_for(\"tokio_tungstenite\", log::LevelFilter::Off)"));
    assert!(!silenced_in(".level_for(\"tungstenite\", log::LevelFilter::Off)"));
    assert!(!silenced_in("for t in LOG_SILENCED { b.filter_module(t, log::LevelFilter::Info); }"));
}

#[test]
fn s15_lib_rs_applies_the_filter_in_the_console_builder() {
    let src = lib_rs();
    let at = src.find("fn build_console_filter").expect("build_console_filter (the RUST_LOG console filter)");
    let body = fn_body(&src[at..], "build_console_filter");
    assert!(silenced_in(&body), "build_console_filter must turn tungstenite and tokio_tungstenite Off (via LOG_SILENCED) after parsing RUST_LOG");
    let spec = body.find("try_parse").expect("RUST_LOG parse");
    let off = body.rfind("LevelFilter::Off").expect("Off");
    assert!(off > spec, "the Off filters must come AFTER the RUST_LOG parse so a user directive cannot switch them back on");
}

#[test]
fn s15_lib_rs_file_log_builder_iterates_log_silenced_not_literals() {
    let src = lib_rs();
    let at = src.find("LogBuilder::new()").expect("the tauri-plugin-log builder");
    let end = src[at..].find(".max_file_size").map_or(src.len(), |e| at + e);
    let sec = &src[at..end];
    assert!(sec.contains("LOG_SILENCED") && sec.contains("LevelFilter::Off"), "the file LogBuilder must turn the libraries Off by iterating so::link::LOG_SILENCED (one list, no literals)");
    assert!(!sec.contains("\"tungstenite\"") && !sec.contains("\"tokio_tungstenite\""), "no literal library names in the file LogBuilder: iterate LOG_SILENCED");
}

#[test]
fn s15_log_silenced_names_both_libraries() {
    let m = link_file_code("mod.rs");
    assert!(m.contains("\"tungstenite\"") && m.contains("\"tokio_tungstenite\""), "LOG_SILENCED must contain tungstenite and tokio_tungstenite");
}

// ---- S16: the two sides of the link agree ----
#[test]
fn s16_port_and_origin_constants_agree_across_app_and_extension() {
    let m = link_file_code("mod.rs");
    assert!(m.contains("pub const LINK_PORT: u16 = 47815;"));
    assert!(m.contains("pub const ALLOWED_ORIGIN: &str = \"chrome-extension://annldiclndpcpopgmdmfcdjdeehfdlik\";"), "ALLOWED_ORIGIN is the spike S0 value");
    let cfg = read(&format!("{EXT}/lib/config.js"));
    assert!(cfg.contains("PORT = 47815") && cfg.contains("MAX_MSG = 4096") && cfg.contains("PING_MS = 20000") && cfg.contains("ALARM_MIN = 0.5"));
}
