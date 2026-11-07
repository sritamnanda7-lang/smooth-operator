// SPDX-License-Identifier: FSL-1.1-MIT
//! Shortcut actions. Handy's dictation action (record, transcribe, paste) is
//! gone; later steps add our own actions here (hold / double-tap talk).

use once_cell::sync::Lazy;
use std::collections::HashMap;
use std::sync::Arc;
use tauri::{AppHandle, Manager};

use crate::managers::audio::AudioRecordingManager;

/// What a shortcut does. `start` runs on key press, `stop` on key release.
/// (A Rust "trait" is a set of methods a type promises to have, like an interface.)
pub trait ShortcutAction: Send + Sync {
    fn start(&self, app: &AppHandle, binding_id: &str, shortcut_str: &str);
    fn stop(&self, app: &AppHandle, binding_id: &str, shortcut_str: &str);
}

/// Cancel: only acts while a recording is running.
struct CancelAction;

impl ShortcutAction for CancelAction {
    fn start(&self, app: &AppHandle, _binding_id: &str, _shortcut_str: &str) {
        if app.state::<Arc<AudioRecordingManager>>().is_recording() {
            crate::utils::cancel_current_operation(app);
        }
    }

    fn stop(&self, _app: &AppHandle, _binding_id: &str, _shortcut_str: &str) {}
}

/// Binding id to action. Built once on first use (`Lazy`).
pub static ACTION_MAP: Lazy<HashMap<String, Arc<dyn ShortcutAction>>> = Lazy::new(|| {
    let mut map: HashMap<String, Arc<dyn ShortcutAction>> = HashMap::new();
    map.insert("cancel".to_string(), Arc::new(CancelAction));
    map
});
