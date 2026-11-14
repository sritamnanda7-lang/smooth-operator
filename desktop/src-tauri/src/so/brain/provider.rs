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

/// Model names and console URLs are from the Rev 2 research (resources/step-4-research.md).
pub const PROVIDERS: &[ProviderInfo] = &[
    ProviderInfo {
        id: ProviderId::Claude,
        label: "Claude",
        key_url: "https://platform.claude.com/settings/keys",
        spend_url: "https://platform.claude.com/settings/billing",
        default_model: "claude-haiku-5-5",
        models: &["claude-haiku-5-5", "claude-sonnet-5-5"],
        recommended: true,
    },
    ProviderInfo {
        id: ProviderId::Gemini,
        label: "Gemini",
        key_url: "https://aistudio.google.com/apikey",
        spend_url: "https://aistudio.google.com/spend",
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

// `models` is the documented model list (a later step offers a picker); nothing reads it yet.
const _: () = {
    let _ = PROVIDERS[0].models;
};
