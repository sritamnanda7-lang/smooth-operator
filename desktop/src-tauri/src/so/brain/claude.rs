// SPDX-License-Identifier: FSL-1.1-MIT
//! Claude provider (stub). Fixed https base; the key goes in a header, never the URL.

use super::keys::ApiKey;
use super::provider::{BrainRequest, Provider, ProviderError, ProviderId};

pub const CLAUDE_MAX_TOKENS: u32 = 1024;

pub struct ClaudeProvider;

impl ClaudeProvider {
    pub fn new() -> Self {
        todo!()
    }

    #[cfg(test)]
    pub fn with_base(_base: &str) -> Self {
        todo!()
    }
}

impl Provider for ClaudeProvider {
    fn id(&self) -> ProviderId {
        ProviderId::Claude
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
    let _ = ClaudeProvider::new;
    let _ = CLAUDE_MAX_TOKENS;
    let _ = <ClaudeProvider as Provider>::id;
    let _ = <ClaudeProvider as Provider>::ask;
    let _ = <ClaudeProvider as Provider>::check_key;
    #[cfg(test)]
    let _ = ClaudeProvider::with_base;
};
