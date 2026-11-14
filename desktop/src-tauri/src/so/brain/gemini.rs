// SPDX-License-Identifier: FSL-1.1-MIT
//! Gemini provider. Fixed https base; the key goes in the `x-goog-api-key` header, never the URL.

use super::config::valid_model;
use super::http::{client, into_body, send};
use super::keys::ApiKey;
use super::plan::schema_for;
use super::provider::{BrainRequest, Provider, ProviderError, ProviderId};
use reqwest::header::{HeaderMap, HeaderValue};
use serde_json::{json, Value};

pub const GEMINI_MAX_OUTPUT_TOKENS: u32 = 2048;
const BASE: &str = "https://generativelanguage.googleapis.com";

pub struct GeminiProvider {
    base: String,
    /// `true` = https only (shipped). Only the tests turn it off, for a local plain-http server.
    strict: bool,
}

impl GeminiProvider {
    pub fn new() -> Self {
        GeminiProvider { base: BASE.to_string(), strict: true }
    }

    #[cfg(test)]
    pub fn with_base(base: &str) -> Self {
        GeminiProvider { base: base.to_string(), strict: false }
    }

    fn headers(secret: &ApiKey) -> Result<HeaderMap, ProviderError> {
        let mut value = HeaderValue::from_str(secret.expose()).map_err(|_| ProviderError::KeyInvalid)?;
        value.set_sensitive(true);
        let mut map = HeaderMap::new();
        map.insert("x-goog-api-key", value);
        Ok(map)
    }
}

impl Default for GeminiProvider {
    fn default() -> Self {
        Self::new()
    }
}

/// The model's text: the non-thought parts of the first candidate joined; it must have finished normally.
fn extract(body: &[u8]) -> Result<String, ProviderError> {
    let v: Value = serde_json::from_slice(body).map_err(|_| ProviderError::BadReply)?;
    let cand = v.get("candidates").and_then(|c| c.get(0)).ok_or(ProviderError::BadReply)?;
    if cand.get("finishReason").is_some_and(|f| f != "STOP") {
        return Err(ProviderError::BadReply);
    }
    let parts = cand.get("content").and_then(|c| c.get("parts")).and_then(Value::as_array).ok_or(ProviderError::BadReply)?;
    let text: String = parts
        .iter()
        .filter(|p| p.get("thought").and_then(Value::as_bool) != Some(true))
        .filter_map(|p| p.get("text").and_then(Value::as_str))
        .collect();
    if text.is_empty() {
        return Err(ProviderError::BadReply);
    }
    Ok(text)
}

impl Provider for GeminiProvider {
    fn id(&self) -> ProviderId {
        ProviderId::Gemini
    }

    fn ask(&self, req: &BrainRequest, model: &str, secret: &ApiKey) -> Result<String, ProviderError> {
        if !valid_model(model) {
            return Err(ProviderError::NotFound);
        }
        let body = json!({
            "systemInstruction": {"parts": [{"text": req.system}]},
            "contents": [{"role": "user", "parts": [{"text": req.user}]}],
            "generationConfig": {
                "responseMimeType": "application/json",
                "responseJsonSchema": schema_for(ProviderId::Gemini),
                "maxOutputTokens": GEMINI_MAX_OUTPUT_TOKENS,
                "thinkingConfig": {"thinkingLevel": "MINIMAL"},
            },
        });
        let url = format!("{}/v1beta/models/{model}:generateContent", self.base);
        let http = client(self.strict)?;
        let call = http.post(url).headers(Self::headers(secret)?).json(&body);
        extract(&into_body(ProviderId::Gemini, send(call)?)?)
    }

    fn check_key(&self, secret: &ApiKey) -> Result<(), ProviderError> {
        let http = client(self.strict)?;
        let call = http.get(format!("{}/v1beta/models", self.base)).headers(Self::headers(secret)?);
        into_body(ProviderId::Gemini, send(call)?).map(|_| ())
    }
}
