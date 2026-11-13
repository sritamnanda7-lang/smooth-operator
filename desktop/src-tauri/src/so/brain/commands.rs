// SPDX-License-Identifier: FSL-1.1-MIT
//! The 6 IPC commands for the AI settings group (main window only). Thin wrappers over pure `*_with`
//! functions that take a `&dyn KeyStore`. `so_brain_save_key` is the ONLY inbound key path.

use super::brain::BrainChoice;
use super::claude::ClaudeProvider;
use super::config;
use super::gemini::GeminiProvider;
use super::keys::{self, ApiKey, KeyStore, KeyringStore, MemoryKeyStore};
use super::provider::{Provider, ProviderError, ProviderId, ProviderInfo, PROVIDERS};
use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::AppHandle;
use tauri_plugin_opener::OpenerExt;

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

/// Fixed texts for the page: never an echo of the input, never an error detail.
pub const KEY_WRONG_TEXT: &str = "Key looks wrong: paste the whole key";
pub const KEY_NOT_SAVED_TEXT: &str = "Couldn't save the key";
pub const KEY_NOT_REMOVED_TEXT: &str = "Couldn't remove the key";

/// Checks the format, then stores the key. The raw text is dropped at once and never echoed.
pub fn save_key_with(store: &dyn KeyStore, p: ProviderId, raw: String) -> Result<(), String> {
    let secret = ApiKey::parse(&raw).map_err(|_| KEY_WRONG_TEXT.to_string())?;
    drop(raw);
    store.set(p, &secret).map_err(|_| KEY_NOT_SAVED_TEXT.to_string())
}

/// One row per provider. `key_saved` comes from reading the key and dropping it at once; a locked or
/// denied Keychain reads as "no key". The chosen provider shows the chosen model, the other its default.
pub fn status_with(store: &dyn KeyStore, choice: &BrainChoice) -> BrainStatus {
    let providers = PROVIDERS
        .iter()
        .map(|info| ProviderView {
            id: info.id,
            label: info.label.to_string(),
            recommended: info.recommended,
            key_saved: matches!(store.get(info.id), Ok(Some(_))),
            model: if info.id == choice.provider { choice.model.clone() } else { info.default_model.to_string() },
        })
        .collect();
    BrainStatus { provider: choice.provider, providers }
}

fn info(p: ProviderId) -> Option<&'static ProviderInfo> {
    PROVIDERS.iter().find(|i| i.id == p)
}

fn provider_for(p: ProviderId) -> Box<dyn Provider> {
    match p {
        ProviderId::Claude => Box::new(ClaudeProvider::new()),
        ProviderId::Gemini => Box::new(GeminiProvider::new()),
    }
}

/// A fresh handle each call: the Keychain is the one source of truth, nothing is cached.
fn store() -> KeyringStore {
    KeyringStore::new(keys::service())
}

/// Maps a provider error to the fixed result shown on the page.
pub fn key_check_of(e: &ProviderError) -> KeyCheck {
    match e {
        ProviderError::KeyInvalid => KeyCheck::Rejected,
        ProviderError::SpendLimit => KeyCheck::LimitReached,
        ProviderError::RateLimited { .. } => KeyCheck::RateLimited,
        ProviderError::Offline | ProviderError::Timeout => KeyCheck::Offline,
        _ => KeyCheck::Failed,
    }
}

/// Check the saved key with one cheap call. Runs on a plain thread (blocking keyring and network).
fn check_key_with(store: &dyn KeyStore, provider: &dyn Provider) -> KeyCheck {
    match store.get(provider.id()) {
        Ok(Some(key)) => provider.check_key(&key).map_or_else(|e| key_check_of(&e), |()| KeyCheck::Ok),
        Ok(None) => KeyCheck::NoKey,
        Err(_) => KeyCheck::Failed,
    }
}

#[tauri::command]
#[specta::specta]
pub async fn so_brain_status(app: AppHandle) -> BrainStatus {
    let choice = config::load_choice(&app);
    let fallback = BrainChoice { provider: choice.provider, model: choice.model.clone() };
    // A panicked task (JoinError) is never shown; the page gets an all-"No key" status.
    match tauri::async_runtime::spawn_blocking(move || status_with(&store(), &choice)).await {
        Ok(s) => s,
        Err(_) => status_with(&MemoryKeyStore::new(), &fallback),
    }
}

#[tauri::command]
#[specta::specta]
pub fn so_brain_set_provider(app: AppHandle, provider: ProviderId) -> Result<(), String> {
    config::save_provider(&app, provider)
}

/// The only inbound key path: page to Rust, once, one way. Nothing is sent back.
#[tauri::command]
#[specta::specta]
pub async fn so_brain_save_key(_app: AppHandle, provider: ProviderId, key: String) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || save_key_with(&store(), provider, key))
        .await
        .unwrap_or_else(|_| Err(KEY_NOT_SAVED_TEXT.to_string()))
}

#[tauri::command]
#[specta::specta]
pub async fn so_brain_remove_key(_app: AppHandle, provider: ProviderId) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || store().delete(provider).map_err(|_| KEY_NOT_REMOVED_TEXT.to_string()))
        .await
        .unwrap_or_else(|_| Err(KEY_NOT_REMOVED_TEXT.to_string()))
}

/// Key-touching commands are async: keyring and network run inside `spawn_blocking`.
#[tauri::command]
#[specta::specta]
pub async fn so_brain_check_key(_app: AppHandle, provider: ProviderId) -> KeyCheck {
    tauri::async_runtime::spawn_blocking(move || check_key_with(&store(), &*provider_for(provider))).await.unwrap_or(KeyCheck::Failed)
}

/// Rust opens the fixed key or spend-limit URL from `PROVIDERS`; the page passes only the provider id.
#[tauri::command]
#[specta::specta]
pub fn so_brain_open_console(app: AppHandle, provider: ProviderId, page: ConsolePage) -> Result<(), String> {
    let i = info(provider).ok_or_else(|| String::from("unknown provider"))?;
    let url = match page {
        ConsolePage::Key => i.key_url,
        ConsolePage::Spend => i.spend_url,
    };
    app.opener().open_url(url, None::<String>).map_err(|_| String::from("Couldn't open the page"))
}
