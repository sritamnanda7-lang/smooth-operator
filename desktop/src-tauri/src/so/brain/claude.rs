// SPDX-License-Identifier: FSL-1.1-MIT
//! Claude provider. Fixed https base; the key goes in an `Authorization: Bearer` header, never the URL.

use super::config::valid_model;
use super::http::{client, into_body, send};
use super::keys::ApiKey;
use super::plan::schema_for;
use super::provider::{BrainRequest, Provider, ProviderError, ProviderId, CLAUDE_API_VERSION};
use reqwest::header::{HeaderMap, HeaderValue};
use serde_json::{json, Value};

pub const CLAUDE_MAX_TOKENS: u32 = 1024;
const BASE: &str = "https://api.anthropic.com";

pub struct ClaudeProvider {
    base: String,
    /// `true` = https only (shipped). Only the tests turn it off, for a local plain-http server.
    strict: bool,
}

impl ClaudeProvider {
    pub fn new() -> Self {
        ClaudeProvider { base: BASE.to_string(), strict: true }
    }

    #[cfg(test)]
    pub fn with_base(base: &str) -> Self {
        ClaudeProvider { base: base.to_string(), strict: false }
    }

    fn headers(secret: &ApiKey) -> Result<HeaderMap, ProviderError> {
        let mut auth = HeaderValue::from_str(&format!("Bearer {}", secret.expose())).map_err(|_| ProviderError::KeyInvalid)?;
        auth.set_sensitive(true);
        let mut map = HeaderMap::new();
        map.insert("authorization", auth);
        map.insert("anthropic-version", HeaderValue::from_static(CLAUDE_API_VERSION));
        Ok(map)
    }
}

impl Default for ClaudeProvider {
    fn default() -> Self {
        Self::new()
    }
}

/// The text block of a reply that ended normally (not a refusal, not cut at the token limit).
fn extract(body: &[u8]) -> Result<String, ProviderError> {
    let v: Value = serde_json::from_slice(body).map_err(|_| ProviderError::BadReply)?;
    if v.get("stop_reason").and_then(Value::as_str) != Some("end_turn") {
        return Err(ProviderError::BadReply);
    }
    let blocks = v.get("content").and_then(Value::as_array).ok_or(ProviderError::BadReply)?;
    blocks
        .iter()
        .find(|b| b.get("type").and_then(Value::as_str) == Some("text"))
        .and_then(|b| b.get("text").and_then(Value::as_str))
        .map(str::to_string)
        .ok_or(ProviderError::BadReply)
}

impl Provider for ClaudeProvider {
    fn id(&self) -> ProviderId {
        ProviderId::Claude
    }

    fn ask(&self, req: &BrainRequest, model: &str, secret: &ApiKey) -> Result<String, ProviderError> {
        if !valid_model(model) {
            return Err(ProviderError::NotFound);
        }
        // Haiku has no between-tools mode; Sonnet gets the minimal one.
        let thinking = if model.contains("sonnet") { "between_tools" } else { "disabled" };
        let body = json!({
            "model": model,
            "max_tokens": CLAUDE_MAX_TOKENS,
            "system": req.system,
            "messages": [{"role": "user", "content": req.user}],
            "thinking": {"type": thinking},
            "output_config": {"format": {"type": "json_schema", "schema": schema_for(ProviderId::Claude)}},
        });
        let http = client(self.strict)?;
        let call = http.post(format!("{}/v1/messages", self.base)).headers(Self::headers(secret)?).json(&body);
        extract(&into_body(ProviderId::Claude, send(call)?)?)
    }

    fn check_key(&self, secret: &ApiKey) -> Result<(), ProviderError> {
        let http = client(self.strict)?;
        let call = http.get(format!("{}/v1/models", self.base)).headers(Self::headers(secret)?);
        into_body(ProviderId::Claude, send(call)?).map(|_| ())
    }
}
