// SPDX-License-Identifier: FSL-1.1-MIT
//! Our IPC commands. Every one is allow-listed for the main window only (capabilities/default.json).
//! (A Tauri "command" is a Rust function the settings page may call by name.)

use super::hotkey;
use super::settings::{self, HotkeyChoice, Platform};
use tauri::AppHandle;

/// The saved hotkey (or the OS default).
#[tauri::command]
#[specta::specta]
pub fn so_get_hotkey(app: AppHandle) -> HotkeyChoice {
    settings::load_hotkey(&app, Platform::current())
}

/// Save a new hotkey, then tell the running watcher. A value this OS does not offer is an error.
#[tauri::command]
#[specta::specta]
pub fn so_set_hotkey(app: AppHandle, choice: String) -> Result<(), String> {
    let choice = HotkeyChoice::from_command(&choice, Platform::current())?;
    settings::save_hotkey(&app, choice)?;
    // Not running yet? Then it reads the saved value when it starts.
    hotkey::set_target(choice);
    Ok(())
}

/// The hotkeys this OS offers, default first.
#[tauri::command]
#[specta::specta]
pub fn so_hotkey_options() -> Vec<HotkeyChoice> {
    HotkeyChoice::options_for(Platform::current())
}
