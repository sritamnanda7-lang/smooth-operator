// SPDX-License-Identifier: FSL-1.1-MIT
//! Our IPC commands. Every one is allow-listed for the main window only (capabilities/default.json).
//! (A Tauri "command" is a Rust function the settings page may call by name.)

use super::fncheck::{self, FnCheckReport};
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

/// Read-only Fn clash check (macOS Globe setting, Dictation shortcut, known voice apps). Never
/// changes a system setting.
#[tauri::command]
#[specta::specta]
pub fn so_fn_check(app: AppHandle) -> FnCheckReport {
    let hotkey = settings::load_hotkey(&app, Platform::current());
    fncheck::check(&*fncheck::system_env(), hotkey)
}
