// SPDX-License-Identifier: FSL-1.1-MIT
//! Step 1 (T1-T32) and step 2 (T1-T28, speech-to-text) Rust tests. Fakes only; no OS calls. Time is injected.
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
