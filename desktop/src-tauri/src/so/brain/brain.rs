// SPDX-License-Identifier: FSL-1.1-MIT
//! The pipeline: limits, key read, request, parse, risk, render. Implements `SlowRouter`.

use super::keys::KeyStore;
use super::provider::{Provider, ProviderId};
use crate::so::hotkey::watcher::Clock;
use crate::so::rules::{Reply, SlowRouter};
use std::sync::Arc;
use std::time::{Duration, Instant};

pub struct BrainChoice {
    pub provider: ProviderId,
    pub model: String,
}

pub struct Limits {
    pub max_in_flight: u32,
    pub max_per_hour: u32,
}

pub const MIN_WORDS: usize = 2;
pub const PLAN_SECS: u64 = 12;
pub const ERROR_SECS: u64 = 6;

pub struct Brain;

impl Brain {
    pub fn new(
        _providers: Vec<Arc<dyn Provider>>,
        _keys: Arc<dyn KeyStore>,
        _choice: Arc<dyn Fn() -> BrainChoice + Send + Sync>,
        _clock: Arc<dyn Clock>,
        _sleep: Arc<dyn Fn(Duration) + Send + Sync>,
        _limits: Limits,
    ) -> Self {
        todo!()
    }
}

impl SlowRouter for Brain {
    fn wants(&self, _text: &str) -> bool {
        todo!()
    }
    fn think(&self, _text: &str, _stopped_at: Instant) -> Reply {
        todo!()
    }
}

// Stub phase: nothing in the app calls these yet (increments 1-6 wire them). This reference keeps them
// compiled without a dead-code warning and without an allow (strip-check s05).
const _: () = {
    let choice = BrainChoice { provider: ProviderId::Claude, model: String::new() };
    let _ = (&choice.provider, &choice.model);
    std::mem::forget(choice);
    let limits = Limits { max_in_flight: 0, max_per_hour: 0 };
    let _ = (limits.max_in_flight, limits.max_per_hour);
    let _ = MIN_WORDS;
    let _ = PLAN_SECS;
    let _ = ERROR_SECS;
    let _ = Brain;
    let _ = Brain::new;
    let _ = <Brain as SlowRouter>::wants;
    let _ = <Brain as SlowRouter>::think;
};
