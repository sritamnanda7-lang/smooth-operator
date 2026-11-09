// SPDX-License-Identifier: FSL-1.1-MIT
//! Provider interface: one trait, two providers (Claude, Gemini). Errors carry no text, ever.

use super::keys::ApiKey;
use serde::{Deserialize, Serialize};
use specta::Type;

/// Which AI service. `snake_case` in JSON: "claude", "gemini".
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum ProviderId {
    Claude,
    Gemini,
}

pub struct ProviderInfo {
    pub id: ProviderId,
    pub label: &'static str,
    pub key_url: &'static str,
    pub spend_url: &'static str,
    pub default_model: &'static str,
    pub models: &'static [&'static str],
    pub recommended: bool,
}

/// `key_url` and `spend_url` are empty in the stub: tests/net-allowlist.txt (a test file) must list the
/// console hosts before the URLs go in.
pub const PROVIDERS: &[ProviderInfo] = &[
    ProviderInfo {
        id: ProviderId::Claude,
        label: "Claude",
        key_url: "",
        spend_url: "",
        default_model: "claude-haiku-5-5",
        models: &["claude-haiku-5-5", "claude-sonnet-5-5"],
        recommended: true,
    },
    ProviderInfo {
        id: ProviderId::Gemini,
        label: "Gemini",
        key_url: "",
        spend_url: "",
        default_model: "gemini-3.5-flash-lite",
        models: &["gemini-3.5-flash-lite"],
        recommended: false,
    },
];

pub const CLAUDE_API_VERSION: &str = "2026-10-21";

/// Token caps and thinking settings are set per provider (see `CLAUDE_MAX_TOKENS`, `GEMINI_MAX_OUTPUT_TOKENS`).
pub struct BrainRequest {
    pub system: &'static str,
    pub user: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProviderError {
    KeyInvalid,
    Forbidden,
    NotFound,
    BadRequest(u16),
    SpendLimit,
    RateLimited { retry_after_s: Option<u64> },
    Server(u16),
    Timeout,
    Offline,
    TooLarge,
    BadReply,
}

pub trait Provider: Send + Sync {
    fn id(&self) -> ProviderId;
    /// Blocking. Returns the raw plan JSON text.
    fn ask(&self, req: &BrainRequest, model: &str, key: &ApiKey) -> Result<String, ProviderError>;
    fn check_key(&self, key: &ApiKey) -> Result<(), ProviderError>;
}

// Stub phase: nothing in the app calls these yet (increments 1-6 wire them). This reference keeps them
// compiled without a dead-code warning and without an allow (strip-check s05).
const _: () = {
    let _ = PROVIDERS;
    let _ = CLAUDE_API_VERSION;
    let req = BrainRequest { system: "", user: String::new() };
    let _ = (req.system, &req.user);
    std::mem::forget(req);
    let _ = ProviderError::Offline;
    let _ = <Option<&dyn Provider>>::None;
    let info = &PROVIDERS[0];
    let _ = (info.id, info.label, info.key_url, info.spend_url, info.default_model, info.models, info.recommended);
    let _ = (
        ProviderError::KeyInvalid, ProviderError::SpendLimit, ProviderError::Forbidden, ProviderError::NotFound, ProviderError::BadRequest(0),
        ProviderError::RateLimited { retry_after_s: None }, ProviderError::Server(0), ProviderError::Timeout,
        ProviderError::TooLarge, ProviderError::BadReply,
    );
};
