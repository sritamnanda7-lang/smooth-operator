// SPDX-License-Identifier: FSL-1.1-MIT
//! Gemini provider (stub). Fixed https base; the key goes in a header, never the URL.

use super::keys::ApiKey;
use super::provider::{BrainRequest, Provider, ProviderError, ProviderId};

pub const GEMINI_MAX_OUTPUT_TOKENS: u32 = 2048;

pub struct GeminiProvider;

impl GeminiProvider {
    pub fn new() -> Self {
        todo!()
    }

    #[cfg(test)]
    pub fn with_base(_base: &str) -> Self {
        todo!()
    }
}

impl Provider for GeminiProvider {
    fn id(&self) -> ProviderId {
        ProviderId::Gemini
    }
    fn ask(&self, _req: &BrainRequest, _model: &str, _key: &ApiKey) -> Result<String, ProviderError> {
        todo!()
    }
    fn check_key(&self, _key: &ApiKey) -> Result<(), ProviderError> {
        todo!()
    }
}

// Stub phase: nothing in the app calls these yet (increments 1-6 wire them). This reference keeps them
// compiled without a dead-code warning and without an allow (strip-check s05).
const _: () = {
    let _ = GeminiProvider::new;
    let _ = GEMINI_MAX_OUTPUT_TOKENS;
    let _ = <GeminiProvider as Provider>::id;
    let _ = <GeminiProvider as Provider>::ask;
    let _ = <GeminiProvider as Provider>::check_key;
    #[cfg(test)]
    let _ = GeminiProvider::with_base;
};
