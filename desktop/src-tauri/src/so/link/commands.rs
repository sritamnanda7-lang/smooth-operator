// SPDX-License-Identifier: FSL-1.1-MIT
//! The 3 IPC commands for Settings -> General -> Chrome extension (main window only). Thin wrappers
//! over `Link`. No command ever returns the token.

use super::pairing::{PairError, PairingView};
use super::PORT_BUSY_TEXT;
use super::server::Link;
use serde::Serialize;
use specta::Type;
use std::sync::Arc;
use tauri::{AppHandle, Manager};

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

/// The shared link, put into managed state by `link::setup`.
fn link_of(app: &AppHandle) -> Option<Arc<Link>> {
    app.try_state::<Arc<Link>>().map(|s| s.inner().clone())
}

/// Runs the possibly blocking Keychain read inside `spawn_blocking`.
#[tauri::command]
#[specta::specta]
pub async fn so_link_status(app: AppHandle) -> LinkStatus {
    let Some(link) = link_of(&app) else { return LinkStatus { state: LinkState::NotFound, pairing_secs_left: None, locked_secs: None } };
    // A panicked read (JoinError) is never shown: the line reads "not found".
    tauri::async_runtime::spawn_blocking(move || link.status())
        .await
        .unwrap_or(LinkStatus { state: LinkState::NotFound, pairing_secs_left: None, locked_secs: None })
}

/// Pure core of `so_link_start_pairing` so tests can reach it without an `AppHandle`: maps `PairError` to
/// the fixed texts "Too many wrong codes. Try again in N s." / `PORT_BUSY_TEXT` (and one fixed text for Rng).
pub fn start_pairing_with(link: &Link) -> Result<PairingView, String> {
    link.start_pairing().map_err(|e| match e {
        PairError::Locked { secs } => format!("Too many wrong codes. Try again in {secs} s."),
        PairError::PortBusy => PORT_BUSY_TEXT.to_string(),
        PairError::Rng => "Could not make a secure code. Try again.".to_string(),
    })
}

/// Errors are fixed texts: "Too many wrong codes. Try again in N s." or `PORT_BUSY_TEXT`.
#[tauri::command]
#[specta::specta]
pub fn so_link_start_pairing(app: AppHandle) -> Result<PairingView, String> {
    let link = link_of(&app).ok_or_else(|| "Could not start pairing".to_string())?;
    start_pairing_with(&link)
}

#[tauri::command]
#[specta::specta]
pub fn so_link_cancel_pairing(app: AppHandle) {
    if let Some(link) = link_of(&app) {
        link.cancel_pairing();
    }
}
