// SPDX-License-Identifier: FSL-1.1-MIT
//! The 6 IPC commands for the AI settings group (main window only). Thin wrappers over pure `*_with`
//! functions that take a `&dyn KeyStore`. `so_brain_save_key` is the ONLY inbound key path.

use super::brain::BrainChoice;
use super::keys::KeyStore;
use super::provider::ProviderId;
use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::AppHandle;

#[derive(Debug, Serialize, Type)]
pub struct ProviderView {
    pub id: ProviderId,
    pub label: String,
    pub recommended: bool,
    pub key_saved: bool,
    pub model: String,
}

#[derive(Debug, Serialize, Type)]
pub struct BrainStatus {
    pub provider: ProviderId,
    pub providers: Vec<ProviderView>,
}

#[derive(Debug, Serialize, Type)]
pub enum KeyCheck {
    Ok,
    Rejected,
    LimitReached,
    RateLimited,
    Offline,
    Failed,
    NoKey,
}

/// Which console page "Get a key" or "Spend limit" opens.
#[derive(Debug, Deserialize, Type)]
pub enum ConsolePage {
    Key,
    Spend,
}

pub fn save_key_with(_store: &dyn KeyStore, _p: ProviderId, _raw: String) -> Result<(), String> {
    todo!()
}

pub fn status_with(_store: &dyn KeyStore, _choice: &BrainChoice) -> BrainStatus {
    todo!()
}

#[tauri::command]
#[specta::specta]
pub async fn so_brain_status(_app: AppHandle) -> BrainStatus {
    todo!()
}

#[tauri::command]
#[specta::specta]
pub fn so_brain_set_provider(_app: AppHandle, _provider: ProviderId) -> Result<(), String> {
    todo!()
}

/// The only inbound key path: page to Rust, once, one way. Nothing is sent back.
#[tauri::command]
#[specta::specta]
pub async fn so_brain_save_key(_app: AppHandle, _provider: ProviderId, _key: String) -> Result<(), String> {
    todo!()
}

#[tauri::command]
#[specta::specta]
pub async fn so_brain_remove_key(_app: AppHandle, _provider: ProviderId) -> Result<(), String> {
    todo!()
}

/// Key-touching commands are async: keyring and network run inside `spawn_blocking` (increment 6).
#[tauri::command]
#[specta::specta]
pub async fn so_brain_check_key(_app: AppHandle, _provider: ProviderId) -> KeyCheck {
    todo!()
}

/// Rust opens the fixed key or spend-limit URL from `PROVIDERS`; the page passes only the provider id.
#[tauri::command]
#[specta::specta]
pub fn so_brain_open_console(_app: AppHandle, _provider: ProviderId, _page: ConsolePage) -> Result<(), String> {
    todo!()
}

// Stub phase: nothing in the app calls these yet (increments 1-6 wire them). This reference keeps them
// compiled without a dead-code warning and without an allow (strip-check s05).
const _: () = {
    std::mem::forget(ProviderView { id: ProviderId::Claude, label: String::new(), recommended: false, key_saved: false, model: String::new() });
    std::mem::forget(BrainStatus { provider: ProviderId::Claude, providers: Vec::new() });
    let _ = KeyCheck::Ok;
    let _ = (ConsolePage::Key, ConsolePage::Spend);
    let _ = save_key_with;
    let _ = status_with;
    let _ = so_brain_status;
    let _ = so_brain_set_provider;
    let _ = so_brain_save_key;
    let _ = so_brain_remove_key;
    let _ = so_brain_check_key;
    let _ = so_brain_open_console;
    let _ = (KeyCheck::Rejected, KeyCheck::LimitReached, KeyCheck::RateLimited, KeyCheck::Offline, KeyCheck::Failed, KeyCheck::NoKey);
};
