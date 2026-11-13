// SPDX-License-Identifier: FSL-1.1-MIT
//! API keys. `ApiKey` refuses to print, copy or serialise its secret. Keys live only in the macOS
//! Keychain (via `keyring`, added in increment 3).

use super::provider::ProviderId;
use std::collections::HashMap;
use std::sync::Mutex;

/// A wrapper type: no `Clone`, `Display`, `Serialize` or `specta::Type`; `Debug` prints "ApiKey(<redacted>)".
pub struct ApiKey(String);

/// Hand-written (not derived) so the secret can never appear in a debug print or a panic message.
impl std::fmt::Debug for ApiKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ApiKey(<redacted>)")
    }
}

impl ApiKey {
    /// Trims, then checks: visible ASCII only (no spaces), 20-256 characters. No provider prefix check
    /// (key formats change).
    pub fn parse(raw: &str) -> Result<ApiKey, KeyFormatError> {
        let text = raw.trim();
        if text.is_empty() {
            return Err(KeyFormatError::Empty);
        }
        if !text.bytes().all(|b| (0x21..=0x7e).contains(&b)) {
            return Err(KeyFormatError::BadChars);
        }
        match text.len() {
            0..=19 => Err(KeyFormatError::TooShort),
            20..=256 => Ok(ApiKey(text.to_string())),
            _ => Err(KeyFormatError::TooLong),
        }
    }

    pub(crate) fn expose(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyFormatError {
    Empty,
    TooShort,
    TooLong,
    BadChars,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyError {
    Locked,
    Denied,
    Backend,
}

pub trait KeyStore: Send + Sync {
    fn set(&self, p: ProviderId, key: &ApiKey) -> Result<(), KeyError>;
    fn get(&self, p: ProviderId) -> Result<Option<ApiKey>, KeyError>;
    /// A missing key is `Ok`.
    fn delete(&self, p: ProviderId) -> Result<(), KeyError>;
}

pub const SERVICE_RELEASE: &str = "com.sritamnanda.smoothoperator.brain";
pub const SERVICE_DEBUG: &str = "com.sritamnanda.smoothoperator.brain.dev";

/// Debug builds use their own Keychain service so dev and shipped keys never mix.
pub fn service() -> &'static str {
    if cfg!(debug_assertions) {
        SERVICE_DEBUG
    } else {
        SERVICE_RELEASE
    }
}

/// The account name of a provider inside a Keychain service: "claude" or "gemini".
fn account(p: ProviderId) -> &'static str {
    match p {
        ProviderId::Claude => "claude",
        ProviderId::Gemini => "gemini",
    }
}

/// The real macOS Keychain (the Mac's password safe), through the `keyring` crate. All calls block, so
/// callers use `spawn_blocking`. One internal lock makes calls on one store run one at a time.
pub struct KeyringStore {
    service: String,
    lock: Mutex<()>,
}

impl KeyringStore {
    pub fn new(service: &str) -> Self {
        KeyringStore { service: service.to_string(), lock: Mutex::new(()) }
    }
}

/// Maps a Keychain failure to a fixed code. The crate's error text is dropped: it could name the item.
#[cfg(target_os = "macos")]
use keyring::Error as Kc;

#[cfg(target_os = "macos")]
fn map_keyring(e: &Kc) -> KeyError {
    match e {
        Kc::NoStorageAccess(_) => KeyError::Locked,
        Kc::PlatformFailure(_) => KeyError::Denied,
        _ => KeyError::Backend,
    }
}

#[cfg(target_os = "macos")]
impl KeyStore for KeyringStore {
    fn set(&self, p: ProviderId, secret: &ApiKey) -> Result<(), KeyError> {
        let _guard = self.lock.lock().unwrap_or_else(|e| e.into_inner());
        let entry = keyring::Entry::new(&self.service, account(p)).map_err(|e| map_keyring(&e))?;
        entry.set_password(secret.expose()).map_err(|e| map_keyring(&e))
    }
    fn get(&self, p: ProviderId) -> Result<Option<ApiKey>, KeyError> {
        let _guard = self.lock.lock().unwrap_or_else(|e| e.into_inner());
        let entry = keyring::Entry::new(&self.service, account(p)).map_err(|e| map_keyring(&e))?;
        match entry.get_password() {
            Ok(text) => Ok(ApiKey::parse(&text).ok()),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(map_keyring(&e)),
        }
    }
    fn delete(&self, p: ProviderId) -> Result<(), KeyError> {
        let _guard = self.lock.lock().unwrap_or_else(|e| e.into_inner());
        let entry = keyring::Entry::new(&self.service, account(p)).map_err(|e| map_keyring(&e))?;
        match entry.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(map_keyring(&e)),
        }
    }
}

/// Other systems have no store yet (Windows comes in its own phase): every call fails closed.
#[cfg(not(target_os = "macos"))]
impl KeyStore for KeyringStore {
    fn set(&self, _p: ProviderId, _secret: &ApiKey) -> Result<(), KeyError> {
        let _ = (&self.service, &self.lock, account(_p));
        Err(KeyError::Backend)
    }
    fn get(&self, _p: ProviderId) -> Result<Option<ApiKey>, KeyError> {
        Err(KeyError::Backend)
    }
    fn delete(&self, _p: ProviderId) -> Result<(), KeyError> {
        Err(KeyError::Backend)
    }
}

/// In memory only: tests and the debug fake mode. Keys are lost when the app quits.
pub struct MemoryKeyStore {
    items: Mutex<HashMap<&'static str, String>>,
}

impl MemoryKeyStore {
    pub fn new() -> Self {
        MemoryKeyStore { items: Mutex::new(HashMap::new()) }
    }
}

impl Default for MemoryKeyStore {
    fn default() -> Self {
        Self::new()
    }
}

impl KeyStore for MemoryKeyStore {
    fn set(&self, p: ProviderId, secret: &ApiKey) -> Result<(), KeyError> {
        let mut items = self.items.lock().unwrap_or_else(|e| e.into_inner());
        items.insert(account(p), secret.expose().to_string());
        Ok(())
    }
    fn get(&self, p: ProviderId) -> Result<Option<ApiKey>, KeyError> {
        let items = self.items.lock().unwrap_or_else(|e| e.into_inner());
        Ok(items.get(account(p)).and_then(|t| ApiKey::parse(t).ok()))
    }
    fn delete(&self, p: ProviderId) -> Result<(), KeyError> {
        let mut items = self.items.lock().unwrap_or_else(|e| e.into_inner());
        items.remove(account(p));
        Ok(())
    }
}
