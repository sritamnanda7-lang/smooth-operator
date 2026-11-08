// SPDX-License-Identifier: FSL-1.1-MIT
//! Normalising and the fixed phrase table. Pure functions. STUBS.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MediaKey {
    PlayPause,
    Next,
    Previous,
}

/// `String` = the normalised app name X.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FastCommand {
    Media(MediaKey),
    Open(String),
    SwitchTo(String),
}

pub const MAX_APP_WORDS: usize = 5;
pub const MAX_APP_CHARS: usize = 40;

pub fn normalise(_text: &str) -> String {
    todo!("step 3 increment 3")
}

pub fn match_command(_text: &str) -> Option<FastCommand> {
    todo!("step 3 increment 3")
}
