// SPDX-License-Identifier: FSL-1.1-MIT
//! Our own settings (own store file `so_settings.json`, never Handy's). Tests: `so/tests/settings_tests.rs` (T17).

use handy_keys::Modifiers;
use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::AppHandle;
use tauri_plugin_store::StoreExt;

/// Which OS rules apply. Passed in so tests can check both OSes on any host.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Platform {
    Mac,
    Windows,
}

impl Platform {
    pub fn current() -> Self {
        if cfg!(target_os = "windows") {
            Platform::Windows
        } else {
            Platform::Mac
        }
    }
}

/// The talk key. Serde names (stored in `so_settings.json`, sent over IPC): "fn", "right_cmd", "right_ctrl".
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum HotkeyChoice {
    Fn,
    RightCmd,
    RightCtrl,
}

impl HotkeyChoice {
    /// The modifier flag the detector watches. (Trivial mapping, real in the stub.)
    pub fn modifier(self) -> Modifiers {
        match self {
            HotkeyChoice::Fn => Modifiers::FN,
            HotkeyChoice::RightCmd => Modifiers::CMD_RIGHT,
            HotkeyChoice::RightCtrl => Modifiers::CTRL_RIGHT,
        }
    }

    /// Mac: Fn. Windows: RightCtrl.
    pub fn default_for(p: Platform) -> Self {
        match p {
            Platform::Mac => HotkeyChoice::Fn,
            Platform::Windows => HotkeyChoice::RightCtrl,
        }
    }

    /// Mac: [Fn, RightCmd] (default first). Windows: [RightCtrl].
    pub fn options_for(p: Platform) -> Vec<Self> {
        match p {
            Platform::Mac => vec![HotkeyChoice::Fn, HotkeyChoice::RightCmd],
            Platform::Windows => vec![HotkeyChoice::RightCtrl],
        }
    }

    /// The serde name ("fn", "right_cmd", "right_ctrl").
    pub fn as_str(self) -> &'static str {
        match self {
            HotkeyChoice::Fn => "fn",
            HotkeyChoice::RightCmd => "right_cmd",
            HotkeyChoice::RightCtrl => "right_ctrl",
        }
    }

    /// Exact name -> choice, only if this OS offers it.
    fn parse(value: &str, p: Platform) -> Option<Self> {
        Self::options_for(p)
            .into_iter()
            .find(|c| c.as_str() == value)
    }

    /// Stored value (None = never set). A missing, unknown or not-allowed-on-this-OS value gives the OS default.
    pub fn from_stored(stored: Option<&str>, p: Platform) -> Self {
        stored
            .and_then(|v| Self::parse(v, p))
            .unwrap_or_else(|| Self::default_for(p))
    }

    /// Value from an IPC command. A bad or not-allowed-on-this-OS value is an error (never silently replaced).
    pub fn from_command(value: &str, p: Platform) -> Result<Self, String> {
        Self::parse(value, p).ok_or_else(|| String::from("unsupported hotkey choice"))
    }
}

pub const STORE_PATH: &str = "so_settings.json";
const HOTKEY_KEY: &str = "hotkey";

/// The saved hotkey, or the OS default (also when the store cannot be read).
pub fn load_hotkey(app: &AppHandle, p: Platform) -> HotkeyChoice {
    let stored = app
        .store(crate::portable::store_path(STORE_PATH))
        .ok()
        .and_then(|store| store.get(HOTKEY_KEY))
        .and_then(|v| v.as_str().map(str::to_owned));
    HotkeyChoice::from_stored(stored.as_deref(), p)
}

pub fn save_hotkey(app: &AppHandle, choice: HotkeyChoice) -> Result<(), String> {
    let store = app
        .store(crate::portable::store_path(STORE_PATH))
        .map_err(|_| String::from("could not open settings"))?;
    store.set(
        HOTKEY_KEY,
        serde_json::Value::String(choice.as_str().to_owned()),
    );
    store
        .save()
        .map_err(|_| String::from("could not save settings"))
}
