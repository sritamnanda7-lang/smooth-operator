// SPDX-License-Identifier: FSL-1.1-MIT
//! Read-only Fn clash check. STUB. Tests: `so/tests/fncheck_tests.rs` (T18-T22).
//! Never writes, never spawns a process, never logs the running-app list.

use super::settings::HotkeyChoice;
use serde::{Deserialize, Serialize};

/// Known apps that grab Fn. Data: `so/fn_apps.json` (a JSON array of `{name, bundle_ids}`).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct FnApp {
    pub name: String,
    pub bundle_ids: Vec<String>,
}

/// Parsed from `include_str!("../fn_apps.json")`.
pub fn known_apps() -> Result<Vec<FnApp>, String> {
    todo!("step 1: parse fn_apps.json")
}

/// OS reads, behind a trait so tests fake them. macOS reads named keys only; Windows returns None/empty.
pub trait FnEnv {
    /// AppleFnUsageType. None = key missing.
    fn globe_usage(&self) -> Option<i64>;
    /// Dictation shortcut on? None = could not tell.
    fn dictation_shortcut_on(&self) -> Option<bool>;
    fn running_bundle_ids(&self) -> Vec<String>;
}

/// Globe key setting. 0 = Do Nothing = Ok. 1, 2, 3 and missing = Warn(label, non-empty).
/// Any other number (not a known mode) = Unknown.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum Globe {
    Ok,
    Warn(String),
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum Dictation {
    Ok,
    Warn,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FnCheckReport {
    pub applies: bool,
    pub globe: Globe,
    pub dictation: Dictation,
    /// Names (not bundle IDs) of running known apps. Only matches are returned.
    pub apps: Vec<String>,
}

/// `applies` is true only when the hotkey is Fn. When false, nothing else is read (env is not called).
pub fn check(_env: &dyn FnEnv, _hotkey: HotkeyChoice) -> FnCheckReport {
    todo!("step 1: fn check")
}
