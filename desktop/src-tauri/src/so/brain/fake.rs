// SPDX-License-Identifier: FSL-1.1-MIT
//! Debug builds only (this module is `#[cfg(debug_assertions)]`): a phrase-driven fake provider with no
//! network and no key, for manual runs. Every reply goes through the REAL parser and pipeline.
//! Phrases (start of the command): "fake question", "fake garbage", "fake risk", "fake slow" (25 s),
//! "fake 401", "fake 429", "fake 500", "fake offline", "fake spend", "fake nokey". Anything else gives a
//! valid two-step plan.

use super::brain::{Brain, BrainChoice, Limits};
use super::keys::{ApiKey, KeyStore, MemoryKeyStore};
use super::provider::{BrainRequest, Provider, ProviderError, ProviderId};
use crate::so::rules::{Reply, SlowRouter};
use std::sync::Arc;
use std::time::{Duration, Instant};

pub const ENV_FLAG: &str = "SO_BRAIN_FAKE";

const FAKE_KEY: &str = "FAKE-KEY-for-debug-runs-only-0000";
const DEFAULT_PLAN: &str = r#"{"v":1,"kind":"plan","steps":[{"action":"youtube_play","args":{"query":"Blinding Lights"}},{"action":"media","args":{"key":"next"}}]}"#;
const QUESTION: &str = r#"{"v":1,"kind":"question","question":"Which one do you mean?"}"#;
const RISK_PLAN: &str = r#"{"v":1,"kind":"plan","steps":[{"action":"docs_new","args":{"topic":"t"},"risk":"safe"}]}"#;

pub struct FakeProvider;

impl FakeProvider {
    pub fn new() -> Self {
        FakeProvider
    }
}

impl Default for FakeProvider {
    fn default() -> Self {
        Self::new()
    }
}

/// The spoken command inside `<command>...</command>`, lowercased.
fn phrase(req: &BrainRequest) -> String {
    let user = req.user.strip_prefix("<command>").unwrap_or(&req.user);
    user.strip_suffix("</command>").unwrap_or(user).to_lowercase()
}

impl Provider for FakeProvider {
    fn id(&self) -> ProviderId {
        ProviderId::Gemini
    }

    fn ask(&self, req: &BrainRequest, _model: &str, _key: &ApiKey) -> Result<String, ProviderError> {
        let said = phrase(req);
        let starts = |p: &str| said.starts_with(p);
        if starts("fake slow") {
            std::thread::sleep(Duration::from_secs(25));
        }
        if starts("fake question") {
            Ok(QUESTION.to_string())
        } else if starts("fake garbage") {
            Ok("this is not json".to_string())
        } else if starts("fake risk") {
            Ok(RISK_PLAN.to_string())
        } else if starts("fake 401") {
            Err(ProviderError::KeyInvalid)
        } else if starts("fake 429") {
            Err(ProviderError::RateLimited { retry_after_s: None })
        } else if starts("fake 500") {
            Err(ProviderError::Server(500))
        } else if starts("fake offline") {
            Err(ProviderError::Offline)
        } else if starts("fake spend") {
            Err(ProviderError::SpendLimit)
        } else {
            Ok(DEFAULT_PLAN.to_string())
        }
    }

    fn check_key(&self, _key: &ApiKey) -> Result<(), ProviderError> {
        Ok(())
    }
}

/// The real `Brain` on the fake provider and an in-memory key store. "fake nokey ..." removes the
/// fake key for that one call, so the pipeline's own "not set up" path runs.
struct FakeFront {
    brain: Brain,
    keys: Arc<MemoryKeyStore>,
}

impl SlowRouter for FakeFront {
    fn wants(&self, text: &str) -> bool {
        self.brain.wants(text)
    }

    fn think(&self, text: &str, stopped_at: Instant) -> Reply {
        let no_key = text.trim().to_lowercase().starts_with("fake nokey");
        if no_key {
            let _ = self.keys.delete(ProviderId::Gemini);
        }
        let reply = self.brain.think(text, stopped_at);
        if no_key {
            if let Ok(k) = ApiKey::parse(FAKE_KEY) {
                let _ = self.keys.set(ProviderId::Gemini, &k);
            }
        }
        reply
    }
}

/// The brain for `SO_BRAIN_FAKE=1`: no network, no Keychain.
pub fn front() -> Arc<dyn SlowRouter> {
    let keys = Arc::new(MemoryKeyStore::new());
    if let Ok(k) = ApiKey::parse(FAKE_KEY) {
        let _ = keys.set(ProviderId::Gemini, &k);
    }
    let brain = Brain::new(
        vec![Arc::new(FakeProvider::new()) as Arc<dyn Provider>],
        keys.clone(),
        Arc::new(|| BrainChoice { provider: ProviderId::Gemini, model: "fake-model".to_string() }),
        Arc::new(crate::so::hotkey::SystemClock),
        Arc::new(std::thread::sleep),
        Limits { max_in_flight: 2, max_per_hour: 120 },
    );
    Arc::new(FakeFront { brain, keys })
}
