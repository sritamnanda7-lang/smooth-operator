// SPDX-License-Identifier: FSL-1.1-MIT
//! T18-T22: Fn check, with a fake FnEnv.
use crate::so::fncheck::{check, known_apps, Dictation, FnEnv, Globe};
use crate::so::settings::HotkeyChoice;
use std::cell::Cell;

struct Fake {
    globe: Option<i64>,
    dict: Option<bool>,
    running: Vec<String>,
    calls: Cell<u32>,
}
impl Fake {
    fn new(globe: Option<i64>, dict: Option<bool>, running: &[&str]) -> Self {
        Fake { globe, dict, running: running.iter().map(|s| s.to_string()).collect(), calls: Cell::new(0) }
    }
}
impl FnEnv for Fake {
    fn globe_usage(&self) -> Option<i64> {
        self.calls.set(self.calls.get() + 1);
        self.globe
    }
    fn dictation_shortcut_on(&self) -> Option<bool> {
        self.calls.set(self.calls.get() + 1);
        self.dict
    }
    fn running_bundle_ids(&self) -> Vec<String> {
        self.calls.set(self.calls.get() + 1);
        self.running.clone()
    }
}

#[test]
fn t18_globe_zero_is_ok() {
    let r = check(&Fake::new(Some(0), Some(false), &[]), HotkeyChoice::Fn);
    assert!(r.applies);
    assert_eq!(r.globe, Globe::Ok);
}

#[test]
fn t18_globe_1_2_3_and_missing_warn_with_label() {
    for g in [Some(1), Some(2), Some(3), None] {
        let r = check(&Fake::new(g, Some(false), &[]), HotkeyChoice::Fn);
        match r.globe {
            Globe::Warn(label) => assert!(!label.trim().is_empty(), "label for {g:?}"),
            other => panic!("{g:?} should warn, got {other:?}"),
        }
    }
}

#[test]
fn t18_globe_unknown_value_is_unknown() {
    // Not a known mode (negative or > 3): we cannot tell what it does.
    for g in [-1, 4, 99] {
        let r = check(&Fake::new(Some(g), Some(false), &[]), HotkeyChoice::Fn);
        assert_eq!(r.globe, Globe::Unknown, "{g}");
    }
}

#[test]
fn t19_dictation_on_warns_off_ok_unknown_unknown() {
    assert_eq!(check(&Fake::new(Some(0), Some(true), &[]), HotkeyChoice::Fn).dictation, Dictation::Warn);
    assert_eq!(check(&Fake::new(Some(0), Some(false), &[]), HotkeyChoice::Fn).dictation, Dictation::Ok);
    assert_eq!(check(&Fake::new(Some(0), None, &[]), HotkeyChoice::Fn).dictation, Dictation::Unknown);
}

#[test]
fn t20_only_listed_bundle_ids_reported_by_name() {
    let env = Fake::new(
        Some(0),
        Some(false),
        &["com.apple.Safari", "ai.perplexity.macv3", "com.google.Chrome", "com.electron.wispr-flow"],
    );
    let mut apps = check(&env, HotkeyChoice::Fn).apps;
    apps.sort();
    assert_eq!(apps, vec!["Perplexity".to_string(), "Wispr Flow".to_string()]);
}

#[test]
fn t20_superwhisper_both_bundle_ids() {
    for id in ["com.superduper.superwhisper", "com.superduper.superwhisper-setapp"] {
        let env = Fake::new(Some(0), Some(false), &[id]);
        assert_eq!(check(&env, HotkeyChoice::Fn).apps, vec!["Superwhisper".to_string()], "{id}");
    }
}

#[test]
fn t18_labels_name_the_setting() {
    // Sub-step 0: 1 Change Input Source, 2 Emoji, 3 Start Dictation; absent = default (clash).
    let label = |g| match check(&Fake::new(g, Some(false), &[]), HotkeyChoice::Fn).globe {
        Globe::Warn(l) => l.to_lowercase(),
        o => panic!("{o:?}"),
    };
    assert!(label(Some(1)).contains("input"));
    assert!(label(Some(2)).contains("emoji"));
    assert!(label(Some(3)).contains("dictation"));
}

#[test]
fn t20_no_known_app_running_gives_empty() {
    let env = Fake::new(Some(0), Some(false), &["com.apple.Safari", "com.google.Chrome"]);
    assert!(check(&env, HotkeyChoice::Fn).apps.is_empty());
}

#[test]
fn t20_report_never_contains_bundle_ids_of_unlisted_apps() {
    let env = Fake::new(Some(0), Some(false), &["com.secret.private.app", "ai.perplexity.macv3"]);
    let r = check(&env, HotkeyChoice::Fn);
    let dump = format!("{r:?}");
    assert!(!dump.contains("com.secret"), "unlisted app leaked into report: {dump}");
}

#[test]
fn t21_hotkey_not_fn_does_not_apply_and_reads_nothing() {
    for hk in [HotkeyChoice::RightCmd, HotkeyChoice::RightCtrl] {
        let env = Fake::new(Some(1), Some(true), &["ai.perplexity.macv3"]);
        let r = check(&env, hk);
        assert!(!r.applies);
        assert!(r.apps.is_empty(), "no app list when not applicable");
        assert_eq!(env.calls.get(), 0, "env must not be read when not applicable");
    }
}

#[test]
fn t22_fn_apps_json_parses_with_3_entries_and_real_ids() {
    let raw = include_str!("../fn_apps.json");
    let v: serde_json::Value = serde_json::from_str(raw).expect("fn_apps.json is valid JSON");
    let arr = v.as_array().expect("top level is an array");
    assert!(arr.len() >= 3, "need at least 3 known apps");
    for e in arr {
        let name = e["name"].as_str().unwrap_or("");
        assert!(!name.trim().is_empty(), "name must not be empty: {e}");
        let ids = e["bundle_ids"].as_array().expect("bundle_ids is an array");
        assert!(!ids.is_empty(), "{name}: bundle_ids must not be empty");
        for id in ids {
            let id = id.as_str().unwrap_or("");
            assert!(!id.trim().is_empty(), "{name}: empty bundle id");
            assert!(!id.to_uppercase().contains("TODO"), "{name}: placeholder bundle id: {id}");
        }
    }
}

#[test]
fn t22_known_apps_loads_same_entries() {
    let apps = known_apps().expect("parses");
    assert!(apps.len() >= 3);
    assert!(apps.iter().any(|a| a.name == "Perplexity" && a.bundle_ids.contains(&"ai.perplexity.macv3".to_string())));
    assert!(apps.iter().any(|a| a.name == "Wispr Flow" && a.bundle_ids.contains(&"com.electron.wispr-flow".to_string())));
}
