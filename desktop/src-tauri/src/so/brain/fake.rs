// SPDX-License-Identifier: FSL-1.1-MIT
//! Debug builds only: a phrase-driven fake provider (no network, no key) for manual runs.

use super::keys::ApiKey;
use super::provider::{BrainRequest, Provider, ProviderError, ProviderId};

pub const ENV_FLAG: &str = "SO_BRAIN_FAKE";

pub struct FakeProvider;

impl FakeProvider {
    pub fn new() -> Self {
        todo!()
    }
}

impl Provider for FakeProvider {
    fn id(&self) -> ProviderId {
        todo!()
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
    let _ = ENV_FLAG;
    let _ = FakeProvider;
    let _ = FakeProvider::new;
};
