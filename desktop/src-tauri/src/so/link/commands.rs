// SPDX-License-Identifier: FSL-1.1-MIT
//! The 3 IPC commands for Settings -> General -> Chrome extension (main window only). Thin wrappers
//! over `Link`. No command ever returns the token. Registered in increment 4.

use super::pairing::PairingView;
use serde::Serialize;
use specta::Type;
use tauri::AppHandle;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Type)]
pub enum LinkState {
    Connected,
    NotPaired,
    NotFound,
    PortBusy,
    KeychainError,
}

#[derive(Debug, Serialize, Type)]
pub struct LinkStatus {
    pub state: LinkState,
    pub pairing_secs_left: Option<u32>,
    pub locked_secs: Option<u32>,
}

/// Runs the possibly blocking Keychain read inside `spawn_blocking`.
#[tauri::command]
#[specta::specta]
pub async fn so_link_status(_app: AppHandle) -> LinkStatus {
    todo!()
}

/// Errors are fixed texts: "Too many wrong codes. Try again in N s." or `PORT_BUSY_TEXT`.
#[tauri::command]
#[specta::specta]
pub fn so_link_start_pairing(_app: AppHandle) -> Result<PairingView, String> {
    todo!()
}

#[tauri::command]
#[specta::specta]
pub fn so_link_cancel_pairing(_app: AppHandle) {
    todo!()
}

// Stub phase: nothing in the app calls these yet. This reference keeps them compiled without a
// dead-code warning and without an allow (strip-check s05).
const _: () = {
    let _ = (
        LinkState::Connected,
        LinkState::NotPaired,
        LinkState::NotFound,
        LinkState::PortBusy,
        LinkState::KeychainError,
    );
    let status = LinkStatus { state: LinkState::NotPaired, pairing_secs_left: None, locked_secs: None };
    let _ = (status.state, status.pairing_secs_left, status.locked_secs);
    let _ = so_link_status;
    let _ = so_link_start_pairing;
    let _ = so_link_cancel_pairing;
};
