// SPDX-License-Identifier: FSL-1.1-MIT
//! Smooth Operator code. Everything we write fresh lives under `so/` so that
//! Handy files stay thin (gate 9: at most 15 Handy files modified).

pub mod actions;
pub mod commands;
pub mod fncheck;
pub mod hotkey;
pub mod panel;
pub mod settings;
pub mod webview;

// Step 1 tests (written first; the stubs above make them compile and fail).
#[cfg(test)]
mod tests;
