// SPDX-License-Identifier: FSL-1.1-MIT
//! Smooth Operator code. Everything we write fresh lives under `so/` so that
//! Handy files stay thin (gate 9: at most 15 Handy files modified).

pub mod actions;
pub mod commands;
pub mod fncheck;
pub mod hotkey;
pub mod panel;
pub mod settings;
pub mod webview;

use panel::{Kind, Line, PanelPayload};
use settings::Platform;
use std::time::Duration;
use tauri::AppHandle;

/// Called once from Handy's core setup, in place of Handy's overlay window: builds our panel and,
/// when the hotkey is Fn and the read-only check finds a clash, shows one warning line for 6 s.
pub fn setup(app: &AppHandle) {
    panel::create(app);
    let hotkey = settings::load_hotkey(app, Platform::current());
    let report = fncheck::check(&*fncheck::system_env(), hotkey);
    if report.has_problem() {
        let line = PanelPayload::new(vec![Line::new(fncheck::LAUNCH_WARNING, Kind::Warn)]);
        panel::show_timed(&panel::panel_out(app), line, Duration::from_secs(fncheck::LAUNCH_WARNING_SECS));
    }
}

// Step 1 tests (written first; the stubs above make them compile and fail).
#[cfg(test)]
mod tests;
