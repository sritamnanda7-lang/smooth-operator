// SPDX-License-Identifier: FSL-1.1-MIT
//! The provider and model choice, kept in `so_settings.json` (never a key).

use super::brain::BrainChoice;
use super::provider::ProviderId;
use tauri::AppHandle;

pub fn load_choice(_app: &AppHandle) -> BrainChoice {
    todo!()
}

pub fn save_provider(_app: &AppHandle, _p: ProviderId) -> Result<(), String> {
    todo!()
}

/// `[a-z0-9][a-z0-9.-]{0,63}`; a bad override falls back to the default model.
pub fn valid_model(_name: &str) -> bool {
    todo!()
}

// Stub phase: nothing in the app calls these yet (increments 1-6 wire them). This reference keeps them
// compiled without a dead-code warning and without an allow (strip-check s05).
const _: () = {
    let _ = load_choice;
    let _ = save_provider;
    let _ = valid_model;
};
