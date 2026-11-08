// SPDX-License-Identifier: FSL-1.1-MIT
//! Step 1 (T1-T32), step 2 (T1-T28, speech-to-text) and step 3 (R/P/A/F/I, fast rules) Rust tests. Fakes only; no OS calls. Time is injected.
mod fncheck_tests;
mod hotkey_tests;
mod ipc_acl_tests;
mod nav_tests;
mod panel_tests;
mod settings_tests;
mod watcher_tests;
mod stt_bench;
mod stt_engine_log;
mod stt_model_tests;
mod stt_review_tests;
mod stt_tests;
mod wer_tests;
mod rules_support;
mod rules_seam_tests;
mod rules_tests;
#[cfg(debug_assertions)]
mod rules_debug_input_tests;
mod rules_release_tests;
