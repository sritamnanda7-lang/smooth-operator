// SPDX-License-Identifier: FSL-1.1-MIT
//! Step 3: fast rules (no AI). Spoken text that is exactly a media or open/switch command is
//! handled here; everything else passes through. Plan: draft/step-3-plan.md.
//! Tests: `so/tests/rules_*.rs`. STUBS: bodies are `todo!()` until the tester's tests exist.
// Nothing is wired yet (increment 6), so unused-code warnings are silenced for the stubs; remove then.
#![allow(dead_code)]

pub mod apps;
#[cfg(debug_assertions)]
pub mod debug_input;
pub mod hands;
pub mod metrics;
pub mod phrase;
pub mod router;
#[cfg(target_os = "macos")]
pub mod real;

use crate::so::panel::Line;
use std::time::Instant;

/// What a router did with a transcript. `Handled` lines are shown under "Heard: ...".
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Route {
    PassThrough,
    Handled(Vec<Line>),
}

/// Plug-in point (trait object) between transcript text and actions. Tests plug in fakes.
pub trait TranscriptRouter: Send + Sync {
    fn route(&self, text: &str, stopped_at: Instant) -> Route;
}

/// Default router: handles nothing, so step-2 behaviour is unchanged.
pub struct PassThrough;

impl TranscriptRouter for PassThrough {
    fn route(&self, _text: &str, _stopped_at: Instant) -> Route {
        Route::PassThrough
    }
}

/// True only in debug builds; the release build has no test input.
pub const DEBUG_INPUT_COMPILED: bool = cfg!(debug_assertions);
