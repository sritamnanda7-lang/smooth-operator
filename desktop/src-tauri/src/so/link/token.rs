// SPDX-License-Identifier: FSL-1.1-MIT
//! The link token (32 random bytes, kept in the Keychain) and the challenge-response proofs.
//! `LinkToken` refuses to print, copy or serialise its secret. It is read on each auth and dropped.

use super::pairing::{Rng, RngError};
use crate::so::brain::keys::{self, KeyError};
use hmac::{Hmac, Mac};
use sha2::Sha256;
use std::sync::Mutex;

/// No `Clone`, `Display`, `Serialize` or `specta::Type`; `Debug` prints "LinkToken(<redacted>)".
pub struct LinkToken([u8; 32]);

/// Hand-written (not derived) so the secret can never appear in a debug print or a panic message.
impl std::fmt::Debug for LinkToken {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("LinkToken(<redacted>)")
    }
}

impl LinkToken {
    pub fn generate(rng: &dyn Rng) -> Result<Self, RngError> {
        let mut b = [0u8; 32];
        rng.fill(&mut b)?;
        Ok(LinkToken(b))
    }
    /// Exactly 64 lowercase hex characters.
    pub fn from_hex(s: &str) -> Option<Self> {
        Some(LinkToken(unhex32(s)?))
    }
    pub(crate) fn to_hex(&self) -> String {
        hex(&self.0)
    }
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

/// 64 lowercase hex characters -> 32 bytes; anything else is `None`.
fn unhex32(s: &str) -> Option<[u8; 32]> {
    let s = s.as_bytes();
    if s.len() != 64 {
        return None;
    }
    let nib = |c: u8| match c {
        b'0'..=b'9' => Some(c - b'0'),
        b'a'..=b'f' => Some(c - b'a' + 10),
        _ => None,
    };
    let mut out = [0u8; 32];
    for (o, pair) in out.iter_mut().zip(s.chunks_exact(2)) {
        *o = (nib(pair[0])? << 4) | nib(pair[1])?;
    }
    Some(out)
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
pub struct KeyringTokenStore {
    service: String,
}

impl KeyringTokenStore {
    pub fn new(service: &str) -> Self {
        KeyringTokenStore { service: service.to_string() }
    }
}

impl TokenStore for KeyringTokenStore {
    fn set(&self, t: &LinkToken) -> Result<(), KeyError> {
        keys::keychain_set(&self.service, ACCOUNT, &t.to_hex())
    }
    /// A damaged entry (not 64 hex) reads as "no token": pairing again overwrites it.
    fn get(&self) -> Result<Option<LinkToken>, KeyError> {
        Ok(keys::keychain_get(&self.service, ACCOUNT)?.and_then(|s| LinkToken::from_hex(&s)))
    }
}

/// In memory only: tests.
#[derive(Default)]
pub struct MemoryTokenStore {
    item: Mutex<Option<String>>,
}

impl MemoryTokenStore {
    pub fn new() -> Self {
        Self::default()
    }
}

impl TokenStore for MemoryTokenStore {
    fn set(&self, t: &LinkToken) -> Result<(), KeyError> {
        *self.item.lock().unwrap_or_else(|e| e.into_inner()) = Some(t.to_hex());
        Ok(())
    }
    fn get(&self) -> Result<Option<LinkToken>, KeyError> {
        Ok(self.item.lock().unwrap_or_else(|e| e.into_inner()).as_deref().and_then(LinkToken::from_hex))
    }
}

/// 32 random bytes as 64 lowercase hex.
pub fn nonce_hex(rng: &dyn Rng) -> Result<String, RngError> {
    let mut b = [0u8; 32];
    rng.fill(&mut b)?;
    Ok(hex(&b))
}

/// HMAC-SHA256 keyed with the 32 raw token bytes over `prefix` + the nonce text.
fn mac(t: &LinkToken, prefix: &str, nonce_hex: &str) -> Hmac<Sha256> {
    // new_from_slice accepts any key length, so this cannot fail for 32 bytes.
    let mut m = Hmac::<Sha256>::new_from_slice(&t.0).unwrap_or_else(|_| unreachable!());
    m.update(prefix.as_bytes());
    m.update(nonce_hex.as_bytes());
    m
}

/// HMAC-SHA256(token, "so-link-v1|ext|" + nonce), lowercase hex.
pub fn client_proof(t: &LinkToken, nonce_hex: &str) -> String {
    hex(&mac(t, "so-link-v1|ext|", nonce_hex).finalize().into_bytes())
}

/// Constant time (`Mac::verify_slice`).
pub fn verify_client_proof(t: &LinkToken, nonce_hex: &str, proof_hex: &str) -> bool {
    let Some(proof) = unhex32(proof_hex) else { return false };
    mac(t, "so-link-v1|ext|", nonce_hex).verify_slice(&proof).is_ok()
}

/// HMAC-SHA256(token, "so-link-v1|app|" + cnonce), lowercase hex.
pub fn server_proof(t: &LinkToken, cnonce_hex: &str) -> String {
    hex(&mac(t, "so-link-v1|app|", cnonce_hex).finalize().into_bytes())
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
