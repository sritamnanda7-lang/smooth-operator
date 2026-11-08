// SPDX-License-Identifier: FSL-1.1-MIT
//! Read-only Fn clash check. Tests: `so/tests/fncheck_tests.rs` (T18-T22).
//! Never writes, never spawns a process, never logs the running-app list.

use super::settings::HotkeyChoice;
use serde::{Deserialize, Serialize};
use specta::Type;

#[cfg(target_os = "macos")]
mod macos;

/// Known apps that grab Fn. Data: `so/fn_apps.json` (a JSON array of `{name, bundle_ids}`).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct FnApp {
    pub name: String,
    pub bundle_ids: Vec<String>,
}

/// Parsed from `include_str!("../fn_apps.json")`.
pub fn known_apps() -> Result<Vec<FnApp>, String> {
    serde_json::from_str(include_str!("../fn_apps.json")).map_err(|e| e.to_string())
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
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
pub enum Globe {
    Ok,
    Warn(String),
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Type)]
pub enum Dictation {
    Ok,
    Warn,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
pub struct FnCheckReport {
    pub applies: bool,
    pub globe: Globe,
    pub dictation: Dictation,
    /// Names (not bundle IDs) of running known apps. Only matches are returned.
    pub apps: Vec<String>,
}

/// `applies` is true only when the hotkey is Fn. When false, nothing else is read (env is not called).
pub fn check(env: &dyn FnEnv, hotkey: HotkeyChoice) -> FnCheckReport {
    if hotkey != HotkeyChoice::Fn {
        return FnCheckReport {
            applies: false,
            globe: Globe::Unknown,
            dictation: Dictation::Unknown,
            apps: Vec::new(),
        };
    }
    let globe = match env.globe_usage() {
        Some(0) => Globe::Ok,
        Some(1) => Globe::Warn("Globe key set to Change Input Source".into()),
        Some(2) => Globe::Warn("Globe key set to Show Emoji & Symbols".into()),
        Some(3) => Globe::Warn("Globe key set to Start Dictation".into()),
        // Not set at all: macOS then uses its own default, which is not "Do Nothing".
        None => Globe::Warn("Globe key not set to Do Nothing (macOS default)".into()),
        Some(_) => Globe::Unknown,
    };
    let dictation = match env.dictation_shortcut_on() {
        Some(true) => Dictation::Warn,
        Some(false) => Dictation::Ok,
        None => Dictation::Unknown,
    };
    // Only the names of known apps leave this function; unlisted bundle IDs are dropped here.
    let running = env.running_bundle_ids();
    let apps = known_apps()
        .unwrap_or_default()
        .into_iter()
        .filter(|a| a.bundle_ids.iter().any(|id| running.contains(id)))
        .map(|a| a.name)
        .collect();
    FnCheckReport {
        applies: true,
        globe,
        dictation,
        apps,
    }
}

/// One line for the panel at launch when the check finds a problem (shown for `LAUNCH_WARNING_SECS`).
pub const LAUNCH_WARNING: &str =
    "Fn may clash with macOS or another app. Open Settings for details.";
pub const LAUNCH_WARNING_SECS: u64 = 6;

impl FnCheckReport {
    /// A real problem was found (unknown results are not problems).
    pub fn has_problem(&self) -> bool {
        self.applies
            && (matches!(self.globe, Globe::Warn(_))
                || self.dictation == Dictation::Warn
                || !self.apps.is_empty())
    }
}

/// The real environment: macOS reads named preference keys; other systems report nothing.
pub fn system_env() -> Box<dyn FnEnv> {
    #[cfg(target_os = "macos")]
    {
        Box::new(macos::MacFnEnv)
    }
    #[cfg(not(target_os = "macos"))]
    {
        Box::new(NoEnv)
    }
}

#[cfg(not(target_os = "macos"))]
struct NoEnv;

#[cfg(not(target_os = "macos"))]
impl FnEnv for NoEnv {
    fn globe_usage(&self) -> Option<i64> {
        None
    }
    fn dictation_shortcut_on(&self) -> Option<bool> {
        None
    }
    fn running_bundle_ids(&self) -> Vec<String> {
        Vec::new()
    }
}
