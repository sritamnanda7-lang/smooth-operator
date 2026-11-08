// SPDX-License-Identifier: FSL-1.1-MIT
//! Our own settings. STUB (step 1 sub-step 1): bodies are `todo!()`; the builder fills them.
//! Tests: `so/tests/settings_tests.rs` (T17).

use handy_keys::Modifiers;
use serde::{Deserialize, Serialize};

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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
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
    pub fn default_for(_p: Platform) -> Self {
        todo!("step 1: OS default")
    }

    /// Mac: [Fn, RightCmd] (default first). Windows: [RightCtrl].
    pub fn options_for(_p: Platform) -> Vec<Self> {
        todo!("step 1: options per OS")
    }

    /// Stored value (None = never set). A missing, unknown or not-allowed-on-this-OS value gives the OS default.
    pub fn from_stored(_stored: Option<&str>, _p: Platform) -> Self {
        todo!("step 1: tolerant read of stored value")
    }

    /// Value from an IPC command. A bad or not-allowed-on-this-OS value is an error (never silently replaced).
    pub fn from_command(_value: &str, _p: Platform) -> Result<Self, String> {
        todo!("step 1: strict parse of command value")
    }
}
