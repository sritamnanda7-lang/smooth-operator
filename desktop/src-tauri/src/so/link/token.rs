// SPDX-License-Identifier: FSL-1.1-MIT
//! The link token (32 random bytes, kept in the Keychain) and the challenge-response proofs.
//! `LinkToken` refuses to print, copy or serialise its secret. It is read on each auth and dropped.

use super::pairing::{Rng, RngError};
use crate::so::brain::keys::{self, KeyError};

/// No `Clone`, `Display`, `Serialize` or `specta::Type`; `Debug` prints "LinkToken(<redacted>)".
pub struct LinkToken([u8; 32]);

/// Hand-written (not derived) so the secret can never appear in a debug print or a panic message.
impl std::fmt::Debug for LinkToken {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("LinkToken(<redacted>)")
    }
}

impl LinkToken {
    pub fn generate(_rng: &dyn Rng) -> Result<Self, RngError> {
        todo!()
    }
    /// Exactly 64 lowercase hex characters.
    pub fn from_hex(_s: &str) -> Option<Self> {
        todo!()
    }
    pub(crate) fn to_hex(&self) -> String {
        todo!()
    }
}

/// Blocking; callers use `spawn_blocking`.
pub trait TokenStore: Send + Sync {
    fn set(&self, t: &LinkToken) -> Result<(), KeyError>;
    /// A missing token is `Ok(None)`.
    fn get(&self) -> Result<Option<LinkToken>, KeyError>;
}

pub const SERVICE_RELEASE: &str = "com.sritamnanda.smoothoperator.link";
pub const SERVICE_DEBUG: &str = "com.sritamnanda.smoothoperator.link.dev";
pub const ACCOUNT: &str = "chrome";

/// Debug builds use their own Keychain service so dev and shipped tokens never mix.
pub fn service() -> &'static str {
    if cfg!(debug_assertions) {
        SERVICE_DEBUG
    } else {
        SERVICE_RELEASE
    }
}

/// The real macOS Keychain, through the shared helpers in `brain/keys.rs`.
pub struct KeyringTokenStore;

impl KeyringTokenStore {
    pub fn new(_service: &str) -> Self {
        KeyringTokenStore
    }
}

impl TokenStore for KeyringTokenStore {
    fn set(&self, _t: &LinkToken) -> Result<(), KeyError> {
        todo!()
    }
    fn get(&self) -> Result<Option<LinkToken>, KeyError> {
        todo!()
    }
}

/// In memory only: tests.
#[derive(Default)]
pub struct MemoryTokenStore;

impl MemoryTokenStore {
    pub fn new() -> Self {
        MemoryTokenStore
    }
}

impl TokenStore for MemoryTokenStore {
    fn set(&self, _t: &LinkToken) -> Result<(), KeyError> {
        todo!()
    }
    fn get(&self) -> Result<Option<LinkToken>, KeyError> {
        todo!()
    }
}

/// 32 random bytes as 64 lowercase hex.
pub fn nonce_hex(_rng: &dyn Rng) -> Result<String, RngError> {
    todo!()
}

/// HMAC-SHA256(token, "so-link-v1|ext|" + nonce), lowercase hex.
pub fn client_proof(_t: &LinkToken, _nonce_hex: &str) -> String {
    todo!()
}

/// Constant time (`Mac::verify_slice`).
pub fn verify_client_proof(_t: &LinkToken, _nonce_hex: &str, _proof_hex: &str) -> bool {
    todo!()
}

/// HMAC-SHA256(token, "so-link-v1|app|" + cnonce), lowercase hex.
pub fn server_proof(_t: &LinkToken, _cnonce_hex: &str) -> String {
    todo!()
}

// Stub phase: nothing in the app calls these yet. This reference keeps them compiled without a
// dead-code warning and without an allow (strip-check s05).
const _: () = {
    let t = LinkToken([0; 32]);
    let _ = t.0;
    let _ = LinkToken::generate;
    let _ = LinkToken::from_hex;
    let _ = LinkToken::to_hex;
    let _ = <KeyringTokenStore as TokenStore>::set;
    let _ = <KeyringTokenStore as TokenStore>::get;
    let _ = <MemoryTokenStore as TokenStore>::set;
    let _ = <MemoryTokenStore as TokenStore>::get;
    let _ = (SERVICE_RELEASE, SERVICE_DEBUG, ACCOUNT);
    let _ = service;
    let _ = KeyringTokenStore::new;
    let _ = MemoryTokenStore::new;
    let _ = nonce_hex;
    let _ = client_proof;
    let _ = verify_client_proof;
    let _ = server_proof;
    let _ = keys::keychain_set;
    let _ = keys::keychain_get;
};
