// SPDX-License-Identifier: FSL-1.1-MIT
//! API keys. `ApiKey` refuses to print, copy or serialise its secret. Keys live only in the macOS
//! Keychain (via `keyring`, added in increment 3).

use super::provider::ProviderId;

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

/// The real Keychain (stub until the `keyring` dependency lands in increment 3).
pub struct KeyringStore;

impl KeyringStore {
    pub fn new(_service: &str) -> Self {
        todo!()
    }
}

impl KeyStore for KeyringStore {
    fn set(&self, _p: ProviderId, _key: &ApiKey) -> Result<(), KeyError> {
        todo!()
    }
    fn get(&self, _p: ProviderId) -> Result<Option<ApiKey>, KeyError> {
        todo!()
    }
    fn delete(&self, _p: ProviderId) -> Result<(), KeyError> {
        todo!()
    }
}

/// In memory only: tests and the debug fake mode.
pub struct MemoryKeyStore;

impl MemoryKeyStore {
    pub fn new() -> Self {
        todo!()
    }
}

impl KeyStore for MemoryKeyStore {
    fn set(&self, _p: ProviderId, _key: &ApiKey) -> Result<(), KeyError> {
        todo!()
    }
    fn get(&self, _p: ProviderId) -> Result<Option<ApiKey>, KeyError> {
        todo!()
    }
    fn delete(&self, _p: ProviderId) -> Result<(), KeyError> {
        todo!()
    }
}

// Stub phase: nothing in the app calls these yet (increments 1-6 wire them). This reference keeps them
// compiled without a dead-code warning and without an allow (strip-check s05).
const _: () = {
    std::mem::forget(ApiKey(String::new()));
    let _ = ApiKey::parse;
    let _ = ApiKey::expose;
    let _ = KeyFormatError::Empty;
    let _ = KeyError::Locked;
    let _ = <Option<&dyn KeyStore>>::None;
    let _ = SERVICE_RELEASE;
    let _ = service;
    let _ = KeyringStore;
    let _ = KeyringStore::new;
    let _ = MemoryKeyStore;
    let _ = MemoryKeyStore::new;
    let _ = (KeyFormatError::TooShort, KeyFormatError::TooLong, KeyFormatError::BadChars);
    let _ = (KeyError::Denied, KeyError::Backend);
    let _ = <MemoryKeyStore as KeyStore>::set;
    let _ = <MemoryKeyStore as KeyStore>::get;
    let _ = <MemoryKeyStore as KeyStore>::delete;
};
