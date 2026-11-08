// SPDX-License-Identifier: FSL-1.1-MIT
//! Step 1 static checks S1-S13. Read files only; never compile app code. Same style as strip_checks.rs.
//! Run: `cargo test --manifest-path tests/strip-checks/Cargo.toml`. Numbers match draft/step-1-plan.md.

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
const CAPS: &str = "desktop/src-tauri/capabilities";
const CONF: &str = "desktop/src-tauri/tauri.conf.json";
const FLOOR: [&str; 3] = ["start_handy_keys_recording", "stop_handy_keys_recording", "change_autostart_setting"];

/// Last path segment of every item in `collect_commands![ ... ]`.
fn lib_commands() -> BTreeSet<String> {
    let src = read(LIB);
    let start = src.find("collect_commands![").expect("collect_commands! in lib.rs") + "collect_commands![".len();
    let end = start + src[start..].find(']').expect("closing ]");
    src[start..end]
        .lines()
        .filter(|l| !is_comment(l) && !l.trim_start().starts_with('#'))
        .flat_map(|l| l.split(',').map(str::trim).map(String::from).collect::<Vec<_>>())
        .filter(|s| !s.is_empty())
        .map(|s| s.rsplit("::").next().unwrap().to_string())
        .collect()
}
/// Names listed in build.rs `APP_COMMANDS`.
fn build_commands() -> BTreeSet<String> {
    let src = read(BUILD);
    let at = src.find("APP_COMMANDS").expect("build.rs must define APP_COMMANDS (S1)");
    let rest = &src[at..];
    let open = rest.find('[').expect("APP_COMMANDS array");
    let close = open + rest[open..].find(']').unwrap();
    rest[open + 1..close]
        .split(',')
        .map(|s| s.trim().trim_matches('"').to_string())
        .filter(|s| !s.is_empty() && !s.starts_with("//"))
        .collect()
}

#[test]
fn s01_build_rs_app_commands_equal_collect_commands() {
    let lib = lib_commands();
    let build = build_commands();
    assert!(!build.is_empty(), "APP_COMMANDS empty");
    assert_eq!(build, lib, "build.rs APP_COMMANDS must equal collect_commands! names in lib.rs");
    for c in ["so_get_hotkey", "so_set_hotkey", "so_hotkey_options", "so_fn_check"] {
        assert!(lib.contains(c), "lib.rs must register {c}");
    }
    assert!(read(BUILD).contains("AppManifest"), "build.rs must use AppManifest");
}

fn cap_files() -> Vec<PathBuf> {
    files_under(CAPS, &["json"])
}

#[test]
fn s02_capabilities_each_file_one_window_no_wildcards() {
    let files = cap_files();
    assert!(!files.is_empty());
    for f in files {
        let v: Value = serde_json::from_str(&fs::read_to_string(&f).unwrap()).unwrap();
        let name = rel(&f);
        let wins = v["windows"].as_array().unwrap_or_else(|| panic!("{name}: windows array"));
        assert_eq!(wins.len(), 1, "{name}: exactly 1 window");
        let w = wins[0].as_str().unwrap();
        assert!(!w.contains('*'), "{name}: no wildcard window");
        assert!(w != "so_panel" && w != "recording_overlay", "{name}: panel/overlay must have no capability, found {w}");
        assert!(v.get("remote").is_none(), "{name}: no remote key");
        assert!(v.get("webviews").is_none(), "{name}: no webviews key");
    }
}

fn default_permissions() -> Vec<String> {
    let v = json(&format!("{CAPS}/default.json"));
    v["permissions"].as_array().unwrap().iter().filter_map(|x| x.as_str().map(String::from)).collect()
}

#[test]
fn s02_default_is_allow_list_not_core_default_and_floor_never_granted() {
    let perms = default_permissions();
    assert!(!perms.iter().any(|x| x == "core:default"), "core:default must be replaced by exact core permissions");
    for cmd in FLOOR {
        let id = format!("allow-{}", cmd.replace('_', "-"));
        assert!(!perms.iter().any(|x| *x == id), "{id} must never be granted");
    }
    for cmd in ["so-get-hotkey", "so-set-hotkey", "so-hotkey-options", "so-fn-check"] {
        let id = format!("allow-{cmd}");
        assert!(perms.iter().any(|x| *x == id), "default.json must grant {id}");
    }
}

#[test]
fn s02_every_granted_app_command_exists_in_app_commands() {
    let known = build_commands();
    for perm in default_permissions() {
        // App commands have no `plugin:` prefix: `allow-<cmd-with-dashes>`.
        if perm.contains(':') || !perm.starts_with("allow-") {
            continue;
        }
        let cmd = perm.trim_start_matches("allow-").replace('-', "_");
        assert!(known.contains(&cmd), "{perm} grants {cmd}, which is not in APP_COMMANDS");
    }
}

#[test]
fn s03_isolation_pattern_and_files() {
    let c = json(CONF);
    assert_eq!(c["app"]["security"]["pattern"]["use"], "isolation", "pattern.use must be isolation");
    let dir = c["app"]["security"]["pattern"]["options"]["dir"].as_str().unwrap_or("../src-isolation");
    assert!(dir.ends_with("src-isolation"), "isolation dir: {dir}");
    for f in ["desktop/src-isolation/index.html", "desktop/src-isolation/index.js"] {
        let t = read(f);
        for bad in ["http://", "https://", "src=\"//", "href=\"//", "import(", "fetch(", "XMLHttpRequest", "WebSocket"] {
            assert!(!t.contains(bad), "{f} must not contain {bad}");
        }
    }
    let tauri_toml = read("desktop/src-tauri/Cargo.toml");
    let feats = toml::from_str::<toml::Value>(&tauri_toml).unwrap()["dependencies"]["tauri"]["features"].clone();
    let feats: Vec<String> = feats.as_array().unwrap().iter().map(|f| f.as_str().unwrap().to_string()).collect();
    assert!(feats.iter().any(|f| f == "isolation"), "tauri feature `isolation` is required");
}

#[test]
fn s04_no_handy_keys_event_broadcast() {
    assert_none(
        "emit(\"handy-keys-event\" (use emit_to(\"main\", ..))",
        grep(&["desktop/src-tauri/src"], &["rs"], &["emit(\"handy-keys-event\""]),
    );
}

#[test]
fn s05_no_blanket_dead_code_allow() {
    assert!(!read(LIB).contains("#![allow(dead_code)]"), "lib.rs: drop #![allow(dead_code)]");
    assert_none("allow(dead_code) under so/", grep(&["desktop/src-tauri/src/so"], &["rs"], &["allow(dead_code)"]));
}

#[test]
fn s06_settings_not_logged_whole() {
    assert_none("Loaded settings: {:?}", grep(&["desktop/src-tauri/src"], &["rs"], &["Loaded settings: {:?}"]));
}

#[test]
fn s07_hotkey_module_never_logs_or_writes() {
    let mut hits = vec![];
    for f in files_under("desktop/src-tauri/src/so/hotkey", &["rs"]) {
        let src = non_test(&fs::read_to_string(&f).unwrap());
        for (i, line) in src.lines().enumerate() {
            if is_comment(line) {
                continue;
            }
            for n in [
                "log::", "info!(", "debug!(", "warn!(", "error!(", "trace!(", "println!", "eprintln!", "print!(", "eprint!(",
                "dbg!(", "fs::write", "File::create", "OpenOptions", "std::fs", "tauri_plugin_log",
            ] {
                if line.contains(n) {
                    hits.push(format!("{}:{}: {}", rel(&f), i + 1, line.trim()));
                }
            }
        }
    }
    assert_none("so/hotkey must not log, print or write files", hits);
}

#[test]
fn s08_autostart_forced_off_and_onboarding_at_login_false() {
    let lib = read(LIB);
    let calls: Vec<&str> = lib.lines().filter(|l| l.contains("apply_autostart(") && !is_comment(l)).collect();
    assert!(!calls.is_empty(), "lib.rs must still call apply_autostart (to remove old login items)");
    for c in &calls {
        assert!(c.trim_end().trim_end_matches(';').ends_with("false)"), "apply_autostart must be forced false: {c}");
    }
    let ob = read("desktop/src/components/onboarding/onboardingStore.ts");
    assert!(ob.contains("atLogin: false,"), "onboardingStore default atLogin must be false");
    assert!(!ob.contains("atLogin: true"), "no atLogin: true left");
}

#[test]
fn s09_tauri_features_exclude_devtools() {
    let t: toml::Value = toml::from_str(&read("desktop/src-tauri/Cargo.toml")).unwrap();
    let feats = t["dependencies"]["tauri"]["features"].as_array().unwrap();
    assert!(!feats.iter().any(|f| f.as_str() == Some("devtools")), "tauri `devtools` feature must stay off");
}

#[test]
fn s10_panel_files_exist_and_have_no_unsafe_dom() {
    let dir = "desktop/src/so/panel";
    for f in ["index.html", "main.ts", "render.ts", "panel.css"] {
        assert!(p(&format!("{dir}/{f}")).exists(), "{dir}/{f} must exist");
    }
    let mut hits = vec![];
    for f in files_under(dir, &[]) {
        let src = fs::read_to_string(&f).unwrap_or_default();
        let name = rel(&f);
        for (i, line) in src.lines().enumerate() {
            if is_comment(line) {
                continue;
            }
            for n in ["innerHTML", "outerHTML", "insertAdjacentHTML", "document.write", "eval(", "new Function"] {
                if line.contains(n) {
                    hits.push(format!("{name}:{}: {}", i + 1, line.trim()));
                }
            }
        }
        if name.ends_with(".html") {
            let lower = src.to_lowercase();
            // every <script> tag must be external (src=) and empty; no <style> block, no style= attribute
            let mut rest = lower.as_str();
            while let Some(at) = rest.find("<script") {
                let tag_end = rest[at..].find('>').unwrap() + at;
                let tag = &rest[at..tag_end];
                if !tag.contains("src=") {
                    hits.push(format!("{name}: inline <script>"));
                }
                let close = rest[tag_end..].find("</script>").map(|c| c + tag_end).unwrap_or(tag_end);
                if !rest[tag_end + 1..close].trim().is_empty() {
                    hits.push(format!("{name}: <script> with body"));
                }
                rest = &rest[tag_end..];
            }
            if lower.contains("<style") {
                hits.push(format!("{name}: inline <style>"));
            }
            if lower.contains(" style=") {
                hits.push(format!("{name}: style= attribute"));
            }
            for h in [" onload=", " onerror=", " onclick=", "javascript:"] {
                if lower.contains(h) {
                    hits.push(format!("{name}: inline handler {h}"));
                }
            }
        }
    }
    assert_none("panel page must have no inline script/style and no unsafe DOM API", hits);
}

#[test]
fn s11_fncheck_is_read_only() {
    assert_none(
        "so/fncheck must never write or spawn",
        grep(
            &["desktop/src-tauri/src/so/fncheck"],
            &["rs"],
            &["Command::new", "setObject", "setValue", "CFPreferencesSet", "synchronize", "removeObject", "set_object", "set_value"],
        ),
    );
}

#[test]
fn s12_window_builders_have_navigation_and_new_window_deny() {
    let lib = read(LIB);
    let at = lib.find("WebviewWindowBuilder::new(app, \"main\"").expect("main window builder in lib.rs");
    let end = at + lib[at..].find(".build()").expect("main builder .build()");
    let main_builder = &lib[at..end];
    assert!(main_builder.contains(".on_navigation("), "main window builder needs on_navigation");
    assert!(main_builder.contains("main_nav_allowed"), "main window must use so::webview::main_nav_allowed");
    assert!(main_builder.contains(".on_new_window("), "main window builder needs on_new_window");
    assert!(lib.contains("NewWindowResponse::Deny"), "lib.rs must return Deny for new windows");

    let panel = read("desktop/src-tauri/src/so/panel.rs");
    let code = non_test(&panel);
    assert!(count_code(&code, ".on_navigation(") >= 2, "panel.rs: on_navigation in both OS branches");
    assert!(count_code(&code, ".on_new_window(") >= 2, "panel.rs: on_new_window in both OS branches");
    assert!(count_code(&code, "NewWindowResponse::Deny") >= 1, "panel.rs must Deny new windows");
    assert!(code.contains("cfg(target_os = \"macos\")") && code.contains("cfg(not(target_os = \"macos\"))"), "both OS branches present");
}

#[test]
fn s13_panel_rs_one_eval_and_no_focus_flags() {
    let panel = non_test(&read("desktop/src-tauri/src/so/panel.rs"));
    assert_eq!(count_code(&panel, ".eval("), 1, "panel.rs must have exactly one .eval(");
    for needle in ["focusable(false)", "no_activate(true)", "nonactivating_panel()"] {
        assert!(count_code(&panel, needle) >= 1, "panel.rs must contain {needle}");
    }
}
