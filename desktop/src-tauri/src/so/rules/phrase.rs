// SPDX-License-Identifier: FSL-1.1-MIT
//! Normalising and the fixed phrase table. Pure functions (same input, same output, no side effects).

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

const PLAY_PAUSE: &[&str] = &["play", "resume", "play music", "resume music", "pause", "pause music", "pause the music"];
const NEXT: &[&str] = &["next", "next track", "next song", "skip", "skip song", "skip track", "skip this song"];
const PREVIOUS: &[&str] = &["previous", "previous track", "previous song", "last track", "last song"];

/// Same cleaning for spoken text and app names: lowercase; `&` becomes "and"; every character that is
/// not a letter, digit or space becomes a space; spaces collapse and are trimmed; then one leading and
/// one trailing "please" word is dropped.
pub fn normalise(text: &str) -> String {
    let spaced: String = text
        .to_lowercase()
        .replace('&', " and ")
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { ' ' })
        .collect();
    let mut words: Vec<&str> = spaced.split_whitespace().collect();
    if words.first() == Some(&"please") {
        words.remove(0);
    }
    if words.last() == Some(&"please") {
        words.pop();
    }
    words.join(" ")
}

/// The whole normalised text must equal a table row, or be `open X` / `switch to X` with X of 1 to
/// `MAX_APP_WORDS` words and at most `MAX_APP_CHARS` characters. No partial or fuzzy match.
pub fn match_command(text: &str) -> Option<FastCommand> {
    let n = normalise(text);
    for (rows, key) in [(PLAY_PAUSE, MediaKey::PlayPause), (NEXT, MediaKey::Next), (PREVIOUS, MediaKey::Previous)] {
        if rows.contains(&n.as_str()) {
            return Some(FastCommand::Media(key));
        }
    }
    let app = |x: &str| {
        let words = x.split(' ').count();
        (!x.is_empty() && words <= MAX_APP_WORDS && x.chars().count() <= MAX_APP_CHARS).then(|| x.to_string())
    };
    if let Some(x) = n.strip_prefix("open ") {
        return app(x).map(FastCommand::Open);
    }
    if let Some(x) = n.strip_prefix("switch to ") {
        return app(x).map(FastCommand::SwitchTo);
    }
    None
}
