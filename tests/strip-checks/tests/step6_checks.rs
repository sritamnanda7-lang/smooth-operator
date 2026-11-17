// SPDX-License-Identifier: FSL-1.1-MIT
//! Step 6 static checks S1-S10. Read files only; never compile app code. Same style as step5_checks.rs.
//! Run: `cargo test --manifest-path tests/strip-checks/Cargo.toml --test step6_checks`.
//! Numbers match draft/step-6-plan.md (Rev 2). `minimum_chrome_version`: a number >= 116 (founder sets the final value after Z5).

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
const SO: &str = "desktop/src-tauri/src/so";
const EXEC: &str = "desktop/src-tauri/src/so/exec";
const EXT: &str = "chrome-extension";
const BASE_TAG: &str = "step-5";

/// Removes every `#[cfg(test)]` item so scans look at shipped code only.
fn strip_cfg_test(src: &str) -> String {
    let lines: Vec<&str> = src.lines().collect();
    let (mut out, mut i) = (String::new(), 0);
    while i < lines.len() {
        if lines[i].trim_start().starts_with("#[cfg(test)]") {
            i += 1;
            let (mut depth, mut seen) = (0i32, false);
            while i < lines.len() {
                let l = lines[i];
                for c in l.chars() {
                    match c {
                        '{' => {
                            depth += 1;
                            seen = true;
                        }
                        '}' => depth -= 1,
                        _ => {}
                    }
                }
                i += 1;
                if (seen && depth <= 0) || (!seen && l.trim_end().ends_with(';')) {
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
/// Shipped, non-comment code lines of every .rs under `dir`, optionally skipping one file name.
fn rust_code(dir: &str, skip: Option<&str>) -> Vec<(String, usize, String)> {
    let mut out = vec![];
    for f in files_under(dir, &["rs"]) {
        if skip.is_some_and(|s| f.file_name().unwrap() == s) {
            continue;
        }
        let txt = strip_cfg_test(&fs::read_to_string(&f).unwrap_or_default());
        for (i, l) in txt.lines().enumerate() {
            if !is_comment(l) {
                out.push((relp(&f), i + 1, l.to_string()));
            }
        }
    }
    out
}
fn exec_files_present() {
    for f in ["mod.rs", "ops.rs", "executor.rs", "metrics.rs"] {
        assert!(p(&format!("{EXEC}/{f}")).is_file(), "{EXEC}/{f} is missing");
    }
}
fn ext_code(exts: &[&str]) -> Vec<(String, usize, String)> {
    let mut out = vec![];
    for f in files_under(EXT, &["js", "html", "json"]) {
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
fn ext_hits(needles: &[&str], only: Option<&str>) -> Vec<String> {
    ext_code(&["js"]).into_iter().filter(|(f, _, l)| only.is_none_or(|o| f.ends_with(o)) && needles.iter().any(|n| l.contains(n))).map(|(f, i, _)| format!("{f}:{i}")).collect()
}
fn git_top(args: &[&str]) -> std::process::Output {
    Command::new("git").args(args).current_dir(root()).output().expect("run git")
}
fn require_tag() {
    let o = git_top(&["rev-parse", "--verify", "--quiet", &format!("{BASE_TAG}^{{commit}}")]);
    assert!(o.status.success(), "git tag {BASE_TAG} not found: run `git fetch --tags`");
}
fn changed_since_tag(paths: &[&str], filter: &str) -> Vec<String> {
    require_tag();
    let mut a = vec!["diff", "--name-only", filter, BASE_TAG, "--"];
    a.extend(paths.iter().copied());
    let o = git_top(&a);
    assert!(o.status.success(), "git diff failed");
    String::from_utf8_lossy(&o.stdout).lines().map(String::from).collect()
}
/// The text between the parens of every `needle(` call, balanced; string literals skipped.
fn call_args(src: &str, needle: &str) -> Vec<String> {
    let mut out = vec![];
    let mut from = 0;
    while let Some(i) = src[from..].find(needle) {
        let start = from + i + needle.len();
        let (mut depth, mut q, mut end) = (1, None::<char>, start);
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
fn fn_or_enum_body(src: &str, head: &str) -> String {
    let at = src.find(head).unwrap_or_else(|| panic!("{head} not found"));
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
    panic!("unbalanced braces after {head}")
}

#[test]
fn scanners_can_fail() {
    assert_eq!(call_args("a.executeScript({ func: f(1), args: [x] }); b.executeScript(y)", "executeScript("), vec!["{ func: f(1), args: [x] }".to_string(), "y".to_string()]);
    let kept = strip_cfg_test("fn a() {}\n#[cfg(test)]\nfn t() { log::info!(\"x\"); }\nfn b() {}\n");
    assert!(!kept.contains("log::") && kept.contains("fn b()"));
    assert!(fn_or_enum_body("pub enum E { A(String), B }", "pub enum E").contains("String"));
}

// ---- S1: manifest ----
#[test]
fn s01_manifest_permissions_hosts_and_forbidden_keys() {
    let m: serde_json::Value = serde_json::from_str(&read(&format!("{EXT}/manifest.json"))).unwrap();
    assert_eq!(m["permissions"], serde_json::json!(["storage", "alarms", "scripting"]));
    let mut hosts: Vec<String> = m["host_permissions"].as_array().expect("host_permissions").iter().map(|h| h.as_str().unwrap().to_string()).collect();
    hosts.sort();
    assert_eq!(hosts, vec!["https://calendar.google.com/*", "https://docs.google.com/*", "https://www.youtube.com/*"]);
    for k in ["content_scripts", "web_accessible_resources", "externally_connectable", "content_security_policy", "optional_permissions", "optional_host_permissions"] {
        assert!(m.get(k).is_none(), "manifest must not have {k}");
    }
    let all = m.to_string();
    for bad in ["\"tabs\"", "activeTab", "<all_urls>"] {
        assert!(!all.contains(bad), "manifest must not contain {bad}");
    }
    let min: u32 = m["minimum_chrome_version"].as_str().expect("minimum_chrome_version is a string").parse().expect("a number");
    assert!(min >= 116, "minimum_chrome_version {min}");
    let then: serde_json::Value = serde_json::from_str(&String::from_utf8_lossy(&git_top(&["show", &format!("{BASE_TAG}:{EXT}/manifest.json")]).stdout)).expect("manifest at tag");
    assert_eq!(m["key"], then["key"], "the key (and so the extension ID) must not change");
}

// ---- S2: no MAIN world, no clicks / events / clipboard ----
#[test]
fn s02_no_main_world_no_clicks_events_or_clipboard() {
    assert_none("MAIN world or world: key", ext_hits(&["\"MAIN\"", "'MAIN'", "world:", "world :"], None));
    assert_none("click / event / clipboard API", ext_hits(&[".click(", "dispatchEvent", "KeyboardEvent", "MouseEvent", "execCommand", "navigator.clipboard"], None));
    assert!(ext_code(&["js"]).iter().any(|(f, _, l)| f.ends_with("tasks.js") && l.contains("executeScript")), "positive control: tasks.js injects with executeScript");
}

// ---- S3: no network API anywhere in the extension; page.js is self-contained ----
#[test]
fn s03_no_network_api_and_page_js_is_self_contained() {
    assert_none("network API in the extension", ext_hits(&["fetch(", "XMLHttpRequest", "sendBeacon", "EventSource", "importScripts"], None));
    let page = ext_code(&["js"]).into_iter().filter(|(f, _, _)| f.ends_with("lib/page.js")).collect::<Vec<_>>();
    assert!(!page.is_empty(), "lib/page.js missing");
    for (f, i, l) in &page {
        assert!(!l.contains("chrome."), "{f}:{i}: page.js must not use chrome.*");
        assert!(!l.trim_start().starts_with("import") && !l.contains("import("), "{f}:{i}: page.js must not import");
    }
}

// ---- S4: setAccessLevel wrapped, before startLink ----
#[test]
fn s04_sw_wraps_set_access_level_in_promise_resolve_then_before_start_link() {
    let sw = read(&format!("{EXT}/sw.js"));
    let code: String = sw.lines().filter(|l| !is_comment(l)).collect::<Vec<_>>().join("\n");
    let a = code.find("setAccessLevel").expect("sw.js must call setAccessLevel");
    assert!(code.contains("TRUSTED_CONTEXTS"));
    let wrap = code.find("Promise.resolve().then(").expect("Promise.resolve().then( wrapper");
    assert!(wrap < a, "the call must sit inside the Promise.resolve().then( wrapper");
    let start = code.find("startLink({").expect("startLink call");
    assert!(a < start && wrap < start, "setAccessLevel comes before startLink");
    assert!(code.contains("const ready"), "the promise is named `ready` and handed to makeTasks");
}

// ---- S5: Rust shape ----
#[test]
fn s05_result_types_have_no_strings_and_exec_has_no_logging_unwrap_or_command() {
    let proto = read(&format!("{SO}/link/protocol.rs"));
    let rc = fn_or_enum_body(&proto, "pub enum ResultCode");
    assert!(!rc.contains("String") && !rc.contains("Vec"), "ResultCode carries no text");
    let ext = fn_or_enum_body(&proto, "pub enum ExtMsg");
    let at = ext.find("Result").expect("ExtMsg::Result");
    let line = ext[at..].lines().next().unwrap();
    assert!(!line.contains("String") && !line.contains("Vec"), "ExtMsg::Result has no String or Vec: {line}");
    exec_files_present();
    let log = ["log::", "tracing::", "info!(", "debug!(", "warn!(", "error!(", "trace!(", "println!(", "eprintln!(", "print!(", "eprint!(", "dbg!("];
    assert_none("logging in so/exec outside metrics.rs", rust_code(EXEC, Some("metrics.rs")).into_iter().filter(|(_, _, l)| log.iter().any(|n| l.contains(n))).map(|(f, i, _)| format!("{f}:{i}")).collect());
    assert_none("unwrap/expect in so/exec", rust_code(EXEC, None).into_iter().filter(|(_, _, l)| l.contains(".unwrap()") || l.contains(".expect(") || l.contains(".unwrap_err()")).map(|(f, i, _)| format!("{f}:{i}")).collect());
    assert_none("Command::new in so/exec", rust_code(EXEC, None).into_iter().filter(|(_, _, l)| l.contains("Command::new")).map(|(f, i, _)| format!("{f}:{i}")).collect());
    assert_none("panic paths in so/exec", rust_code(EXEC, None).into_iter().filter(|(_, _, l)| ["todo!(", "unimplemented!(", "panic!(", "unreachable!("].iter().any(|n| l.contains(n))).map(|(f, i, _)| format!("{f}:{i}")).collect());
}

// ---- S6: risk is recomputed ----
#[test]
fn s06_executor_calls_op_risk_and_exec_never_reads_step_risk() {
    exec_files_present();
    let ex: String = rust_code(EXEC, Some("metrics.rs")).into_iter().filter(|(f, _, _)| f.ends_with("executor.rs")).map(|x| x.2).collect::<Vec<_>>().join("\n");
    assert!(ex.contains("op_risk("), "executor.rs must call op_risk(");
    assert_none("`.risk` read in so/exec", rust_code(EXEC, None).into_iter().filter(|(_, _, l)| l.contains(".risk")).map(|(f, i, _)| format!("{f}:{i}")).collect());
}

// ---- S7: no Handy core file, Cargo files identical ----
#[test]
fn s07_no_core_file_changed_and_cargo_toml_and_lock_identical_to_step_5() {
    let changed = changed_since_tag(&["desktop/src-tauri/src", "desktop/src", "desktop/src-tauri/Cargo.toml", "desktop/src-tauri/Cargo.lock", "desktop/package.json"], "--diff-filter=ACDMR");
    let others = git_top(&["ls-files", "--others", "--exclude-standard", "--", "desktop/src-tauri/src", "desktop/src", "desktop/src-tauri/Cargo.toml", "desktop/src-tauri/Cargo.lock"]);
    let mut changed = changed;
    changed.extend(String::from_utf8_lossy(&others.stdout).lines().map(String::from));
    let core: Vec<&String> = changed.iter().filter(|f| !f.starts_with("desktop/src-tauri/src/so/") && !f.starts_with("desktop/src/so/")).collect();
    assert!(core.is_empty(), "core files changed (gate 9): {core:?}");
}

// ---- S8: SPDX headers on new files ----
#[test]
fn s08_every_new_file_has_an_spdx_header() {
    require_tag();
    let paths = ["desktop/src-tauri/src/so", "desktop/src/so", EXT, "tests", "scripts"];
    let mut files = changed_since_tag(&paths, "--diff-filter=A");
    let mut a = vec!["ls-files", "--others", "--exclude-standard", "--"];
    a.extend(paths);
    files.extend(String::from_utf8_lossy(&git_top(&a).stdout).lines().map(String::from));
    files.retain(|f| !f.contains("node_modules") && !f.ends_with("Cargo.lock") && !f.ends_with(".txt") && !f.ends_with(".license"));
    let mut missing = vec![];
    for f in &files {
        let Ok(text) = fs::read_to_string(p(f)) else { continue };
        let head: String = text.lines().take(12).collect::<Vec<_>>().join("\n");
        if !head.contains("SPDX-License-Identifier") && !p(&format!("{f}.license")).is_file() && !f.starts_with("chrome-extension/") {
            missing.push(f.clone());
        }
    }
    assert_none("new files without SPDX", missing);
    for f in ["lib/cmd.js", "lib/tasks.js", "lib/page.js"] {
        assert!(read(&format!("{EXT}/{f}")).lines().take(3).any(|l| l.contains("SPDX-License-Identifier")), "{f} needs an SPDX header");
    }
}

// ---- S9: every executeScript uses func + args + target ----
#[test]
fn s09_every_execute_script_call_uses_func_and_args() {
    let mut calls = 0;
    for f in files_under(EXT, &["js"]) {
        let src: String = fs::read_to_string(&f).unwrap_or_default().lines().filter(|l| !is_comment(l)).collect::<Vec<_>>().join("\n");
        for a in call_args(&src, "executeScript(") {
            calls += 1;
            for needle in ["target", "func", "args"] {
                assert!(a.contains(needle), "{}: an executeScript call lacks `{needle}`: {a}", relp(&f));
            }
            assert!(!a.contains("code:") && !a.contains("files:"), "{}: only func+args injection is allowed", relp(&f));
        }
    }
    assert!(calls >= 1, "positive control: at least one executeScript call in the extension");
}

// ---- S10: "res" built only in protocol.js ----
#[test]
fn s10_res_messages_are_built_only_in_protocol_js() {
    let hits: Vec<String> = ext_code(&["js"]).into_iter().filter(|(f, _, l)| !f.ends_with("lib/protocol.js") && (l.contains("\"res\"") || l.contains("'res'") || l.contains("t: \"res\""))).map(|(f, i, _)| format!("{f}:{i}")).collect();
    assert_none("\"res\" outside protocol.js", hits);
    assert!(read(&format!("{EXT}/lib/protocol.js")).contains("resMsg"), "protocol.js must export resMsg");
}

// ---- step5_checks.rs s06 was updated for step 6: its edit is pinned ----
/// New-file line ranges of every hunk of a `git diff -U0` (a pure deletion counts as its line and the next).
fn hunk_ranges(diff: &str) -> Vec<(usize, usize)> {
    let mut out = vec![];
    for l in diff.lines().filter(|l| l.starts_with("@@")) {
        let plus = l.split_whitespace().find(|w| w.starts_with('+')).unwrap_or("+0");
        let (c, d) = plus[1..].split_once(',').map_or((plus[1..].parse().unwrap_or(0), 1usize), |(a, b)| (a.parse().unwrap_or(0), b.parse().unwrap_or(0)));
        out.push((c, if d == 0 { c + 1 } else { c + d - 1 }));
    }
    out
}
/// 1-based (start, end) lines of `fn name`, with its leading attribute and doc lines.
fn fn_lines(src: &str, name: &str) -> (usize, usize) {
    let lines: Vec<&str> = src.lines().collect();
    let at = lines.iter().position(|l| l.contains(&format!("fn {name}("))).unwrap_or_else(|| panic!("fn {name} not found"));
    let mut start = at;
    while start > 0 && (lines[start - 1].trim_start().starts_with("#[") || lines[start - 1].trim_start().starts_with("///")) {
        start -= 1;
    }
    let (mut depth, mut seen) = (0i32, false);
    for (i, l) in lines.iter().enumerate().skip(at) {
        for c in l.chars() {
            match c {
                '{' => {
                    depth += 1;
                    seen = true;
                }
                '}' => depth -= 1,
                _ => {}
            }
        }
        if seen && depth == 0 {
            return (start + 1, i + 1);
        }
    }
    panic!("unbalanced braces in fn {name}")
}
fn hunks_outside(diff: &str, allowed: &[(usize, usize)]) -> Vec<(usize, usize)> {
    hunk_ranges(diff).into_iter().filter(|h| !allowed.iter().any(|a| h.0 >= a.0 && h.1 <= a.1)).collect()
}

#[test]
fn s11_the_step5_guard_can_fail() {
    let diff = "@@ -10,2 +12,3 @@\n+a\n@@ -90 +100 @@\n-x\n+y\n";
    assert_eq!(hunk_ranges(diff), vec![(12, 14), (100, 100)]);
    assert_eq!(hunks_outside(diff, &[(10, 20)]), vec![(100, 100)], "a hunk outside the allowed function is reported");
    assert!(hunks_outside(diff, &[(10, 20), (100, 100)]).is_empty());
}

#[test]
fn s11_step5_checks_edit_is_only_the_s06_manifest_test_with_an_exact_size() {
    require_tag();
    let rel = "tests/strip-checks/tests/step5_checks.rs";
    let diff = String::from_utf8_lossy(&git_top(&["diff", "-U0", BASE_TAG, "--", rel]).stdout).to_string();
    let src = fs::read_to_string(p(rel)).unwrap();
    let allowed = [fn_lines(&src, "s06_manifest_is_mv3_with_a_key_two_permissions_and_nothing_risky")];
    let outside = hunks_outside(&diff, &allowed);
    assert!(outside.is_empty(), "step5_checks.rs changed outside s06 (new-file lines {outside:?}); allowed {allowed:?}");
    let count = |c: char| diff.lines().filter(|l| l.starts_with(c) && !l.starts_with("+++") && !l.starts_with("---")).count();
    assert_eq!((count('+'), count('-')), (S5_ADDED, S5_REMOVED), "the exact edit size is pinned: another line needs a reviewer");
}
const S5_ADDED: usize = 7;
const S5_REMOVED: usize = 6;
