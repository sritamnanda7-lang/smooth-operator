// SPDX-License-Identifier: FSL-1.1-MIT
//! What is sent to the AI: the fixed system prompt and the wrapped transcript.

use super::provider::BrainRequest;

pub const SYSTEM_PROMPT: &str = "";
pub const MAX_TRANSCRIPT_CHARS: usize = 1000;

/// `<command>…</command>` around the cleaned transcript.
pub fn user_message(_transcript: &str) -> String {
    todo!()
}

pub fn request(_transcript: &str) -> BrainRequest {
    todo!()
}

// Stub phase: nothing in the app calls these yet (increments 1-6 wire them). This reference keeps them
// compiled without a dead-code warning and without an allow (strip-check s05).
const _: () = {
    let _ = SYSTEM_PROMPT;
    let _ = MAX_TRANSCRIPT_CHARS;
    let _ = user_message;
    let _ = request;
};
