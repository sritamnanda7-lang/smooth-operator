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

/// UI files that run in the main window whatever pane is open.
const FILES: [&str; 8] = [
    "desktop/src/App.tsx",
    "desktop/src/components/model-selector/ModelSelector.tsx",
    "desktop/src/components/SecureInputWarning.tsx",
    "desktop/src/components/onboarding/usePermissions.ts",
    "desktop/src/components/onboarding/OnboardingFlow.tsx",
    "desktop/src/stores/modelStore.ts",
    "desktop/src/stores/noticeStore.ts",
    "desktop/src/hooks/useMachineTier.ts",
];
/// Called from the UI but refused on purpose (the floor: never granted).
const REFUSED: [&str; 1] = ["change_autostart_setting"];

#[test]
fn ui_invoked_commands_are_granted() {
    let (granted, _) = granted();
    let mut missing = vec![];
    for f in FILES {
        let Ok(src) = fs::read_to_string(root().join(f)) else { continue };
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
