// SPDX-License-Identifier: MIT
// Modified from Handy v0.10.0 (Copyright (c) 2025 CJ Pais): dictation routing removed.
//! Shared shortcut event handling logic
//!
//! This module contains the common logic for handling shortcut events,
//! used by both the Tauri and handy-keys implementations.

use log::warn;
use tauri::AppHandle;

use crate::so::actions::ACTION_MAP;

/// Handle a shortcut event from either implementation: look up the action in
/// `ACTION_MAP` and run `start` on press, `stop` on release.
pub fn handle_shortcut_event(
    app: &AppHandle,
    binding_id: &str,
    hotkey_string: &str,
    is_pressed: bool,
) {
    let Some(action) = ACTION_MAP.get(binding_id) else {
        // Log the binding id only: the key string is configuration, but the
        // key path never logs key values.
        warn!(
            "No action defined in ACTION_MAP for shortcut ID '{}', pressed: {}",
            binding_id, is_pressed
        );
        return;
    };

    if is_pressed {
        action.start(app, binding_id, hotkey_string);
    } else {
        action.stop(app, binding_id, hotkey_string);
    }
}
