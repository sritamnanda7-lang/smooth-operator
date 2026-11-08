// SPDX-License-Identifier: FSL-1.1-MIT
//! Review fix S1/S2: commands the always-visible UI files call must be granted to `main`, and the
//! macOS-permissions plugin must be granted command by command (never `:default`).
use serde_json::Value;
use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

fn root() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..").canonicalize().unwrap()
}

fn granted() -> (BTreeSet<String>, Vec<String>) {
    let v: Value = serde_json::from_str(&fs::read_to_string(root().join("desktop/src-tauri/capabilities/default.json")).unwrap()).unwrap();
    let all: Vec<String> = v["permissions"].as_array().unwrap().iter().filter_map(|x| x.as_str().map(String::from)).collect();
    let cmds = all
        .iter()
        .filter(|p| p.starts_with("allow-") && !p.contains(':'))
        .map(|p| p.trim_start_matches("allow-").replace('-', "_"))
        .collect();
    (cmds, all)
}

fn snake(camel: &str) -> String {
    let mut out = String::new();
    for (i, c) in camel.chars().enumerate() {
        if c.is_uppercase() && i > 0 {
            out.push('_');
        }
        out.extend(c.to_lowercase());
    }
    out
}

/// Files that must exist (a rename would otherwise skip them unnoticed).
const MUST_EXIST: [&str; 8] = [
    "desktop/src/App.tsx",
    "desktop/src/components/model-selector/ModelSelector.tsx",
    "desktop/src/components/SecureInputWarning.tsx",
    "desktop/src/components/onboarding/usePermissions.ts",
    "desktop/src/components/onboarding/OnboardingFlow.tsx",
    "desktop/src/stores/modelStore.ts",
    "desktop/src/stores/noticeStore.ts",
    "desktop/src/hooks/useMachineTier.ts",
];

use std::path::PathBuf;

fn src_root() -> PathBuf {
    root().join("desktop/src")
}

fn resolve(from: &Path, spec: &str) -> Option<PathBuf> {
    let base = if let Some(r) = spec.strip_prefix("@/") {
        src_root().join(r)
    } else if spec.starts_with('.') {
        from.parent().unwrap().join(spec)
    } else {
        return None; // external package
    };
    let mut cands = vec![];
    for ext in ["ts", "tsx"] {
        cands.push(PathBuf::from(format!("{}.{ext}", base.display())));
        cands.push(base.join(format!("index.{ext}")));
    }
    cands.into_iter().find(|c| c.is_file()).map(|c| c.canonicalize().unwrap())
}

/// (spec, imported names). `None` names = whole module (default / namespace / side-effect / dynamic import).
fn imports_of(src: &str) -> Vec<(String, Option<Vec<String>>)> {
    let mut out = vec![];
    for q in ['"', '\''] {
        let pat_from = format!("from {q}");
        let mut rest = src;
        while let Some(i) = rest.find(&pat_from) {
            let after = &rest[i + pat_from.len()..];
            let spec: String = after.chars().take_while(|c| *c != q).collect();
            let before = &rest[..i];
            let start = before.rfind("import ").max(before.rfind("export ")).unwrap_or(0);
            let clause = &before[start..];
            let names = match (clause.find('{'), clause.find('}')) {
                (Some(o), Some(c)) if o < c && !clause.contains("* as") => Some(
                    clause[o + 1..c]
                        .split(',')
                        .map(|n| n.trim().split(" as ").next().unwrap().trim().trim_start_matches("type ").to_string())
                        .filter(|n| !n.is_empty())
                        .collect(),
                ),
                _ => None,
            };
            out.push((spec, names));
            rest = &rest[i + pat_from.len()..];
        }
        let pat_dyn = format!("import({q}");
        let mut rest = src;
        while let Some(i) = rest.find(&pat_dyn) {
            let after = &rest[i + pat_dyn.len()..];
            out.push((after.chars().take_while(|c| *c != q).collect(), None));
            rest = after;
        }
        let pat_side = format!("import {q}");
        let mut rest = src;
        while let Some(i) = rest.find(&pat_side) {
            let after = &rest[i + pat_side.len()..];
            out.push((after.chars().take_while(|c| *c != q).collect(), None));
            rest = after;
        }
    }
    out
}

fn is_barrel(src: &str) -> bool {
    src.lines().map(str::trim).filter(|l| !l.is_empty() && !l.starts_with("//")).all(|l| l.starts_with("export ") && l.contains(" from "))
}

/// Every UI file reachable by imports from the app entry (main.tsx), so only code a visible pane can run.
/// Barrel files (`components/settings/index.ts`) are followed per imported name, so the hidden Advanced pane,
/// which nothing imports any more, stays out. The generated bindings are not scanned; settingsStore.ts is
/// skipped (it lists every Handy setting command; which ones a visible item uses is checked by M7).
fn ui_files() -> Vec<String> {
    let entry = src_root().join("main.tsx").canonicalize().expect("desktop/src/main.tsx");
    let mut seen: BTreeSet<PathBuf> = BTreeSet::new();
    let mut queue: Vec<(PathBuf, Option<Vec<String>>)> = vec![(entry, None)];
    while let Some((f, names)) = queue.pop() {
        let src = fs::read_to_string(&f).unwrap();
        if is_barrel(&src) {
            // follow only the re-exports whose exported name was imported
            for (spec, n) in imports_of(&src) {
                let wanted = match (&names, &n) {
                    (None, _) => true,
                    (Some(want), Some(have)) => have.iter().any(|h| want.contains(h)),
                    (Some(_), None) => true,
                };
                if wanted {
                    if let Some(t) = resolve(&f, &spec) {
                        queue.push((t, None));
                    }
                }
            }
            continue;
        }
        if !seen.insert(f.clone()) {
            continue;
        }
        for (spec, n) in imports_of(&src) {
            if let Some(t) = resolve(&f, &spec) {
                queue.push((t, n));
            }
        }
    }
    let root = root().canonicalize().unwrap();
    seen.iter()
        .map(|f| f.strip_prefix(&root).unwrap().to_string_lossy().replace('\\', "/"))
        .filter(|f| !f.ends_with("bindings.ts") && !f.ends_with("settingsStore.ts") && !f.ends_with("/index.ts"))
        .collect()
}

/// Called from the UI but refused on purpose (the floor: never granted).
const REFUSED: [&str; 1] = ["change_autostart_setting"];

#[test]
fn ui_invoked_commands_are_granted() {
    let (granted, _) = granted();
    let mut missing = vec![];
    for f in MUST_EXIST {
        assert!(root().join(f).is_file(), "expected UI file is missing (renamed?): {f}");
    }
    let files = ui_files();
    assert!(files.len() > 30, "UI file scan found too few files: {}", files.len());
    for f in &files {
        let src = fs::read_to_string(root().join(f)).unwrap_or_else(|e| panic!("cannot read {f}: {e}"));
        for part in src.split("commands.").skip(1) {
            let name: String = part.chars().take_while(|c| c.is_alphanumeric()).collect();
            let cmd = snake(&name);
            if !granted.contains(&cmd) && !REFUSED.contains(&cmd.as_str()) {
                missing.push(format!("{f}: {cmd}"));
            }
        }
    }
    assert!(missing.is_empty(), "UI calls commands the main window may not call: {missing:#?}");
}

#[test]
fn macos_permissions_granted_exactly_four() {
    let (_, all) = granted();
    let mac: BTreeSet<&str> = all.iter().filter(|p| p.starts_with("macos-permissions:")).map(String::as_str).collect();
    let want: BTreeSet<&str> = [
        "macos-permissions:allow-check-accessibility-permission",
        "macos-permissions:allow-request-accessibility-permission",
        "macos-permissions:allow-check-microphone-permission",
        "macos-permissions:allow-request-microphone-permission",
    ]
    .into_iter()
    .collect();
    assert_eq!(mac, want);
}
