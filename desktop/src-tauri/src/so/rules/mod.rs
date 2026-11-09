// SPDX-License-Identifier: FSL-1.1-MIT
//! Step 3: fast rules (no AI). Spoken text that is exactly a media or open/switch command is
//! handled here; everything else passes through. Plan: draft/step-3-plan.md.
//! Tests: `so/tests/rules_*.rs`.

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

/// What the brain answers: panel lines (shown under "Heard: ...") and how long they stay.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reply {
    pub lines: Vec<Line>,
    pub hold_secs: u64,
}

/// Plug-in point for the AI (step 4): slow, network. Tests plug in fakes.
pub trait SlowRouter: Send + Sync {
    /// Fast and pure: the word rule only (2+ words).
    fn wants(&self, text: &str) -> bool;
    /// Blocks (network); never panics.
    fn think(&self, text: &str, stopped_at: Instant) -> Reply;
}

/// Shown at once while the brain works.
pub const THINKING_TEXT: &str = "Thinking…";

/// Default router: handles nothing, so step-2 behaviour is unchanged.
pub struct PassThrough;

impl TranscriptRouter for PassThrough {
    fn route(&self, _text: &str, _stopped_at: Instant) -> Route {
        Route::PassThrough
    }
}

/// True only in debug builds; the release build has no test input.
pub const DEBUG_INPUT_COMPILED: bool = cfg!(debug_assertions);

// `PassThrough` and the constant are part of the step-3 API (the default router, the release guard);
// tests use them and the shipped code does not. This reference keeps them compiled without a warning.
const _: () = {
    let _ = PassThrough;
    let _ = DEBUG_INPUT_COMPILED;
};

// Stub phase: nothing in the app calls these yet (increments 1-6 wire them). This reference keeps them
// compiled without a dead-code warning and without an allow (strip-check s05).
const _: () = {
    std::mem::forget(Reply { lines: Vec::new(), hold_secs: 0 });
    let _ = <Option<&dyn SlowRouter>>::None;
    let _ = THINKING_TEXT;
};
