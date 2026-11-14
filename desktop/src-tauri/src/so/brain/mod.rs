// SPDX-License-Identifier: FSL-1.1-MIT
//! Step 4: the brain. A spoken command that the fast rules did not handle becomes a checked plan
//! from a fixed action list, or one question. Nothing is run. Plan: draft/step-4-plan.md.
//! Increment 0 = stubs only: bodies are `todo!()` until the tests exist.

// The plan names the file `brain.rs`; clippy dislikes a module with its parent's name, so it is allowed here.
#[allow(clippy::module_inception)]
pub mod brain;
pub mod claude;
pub mod commands;
pub mod config;
#[cfg(debug_assertions)]
pub mod fake;
pub mod gemini;
pub mod http;
pub mod keys;
pub mod metrics;
pub mod plan;
pub mod prompt;
pub mod provider;
