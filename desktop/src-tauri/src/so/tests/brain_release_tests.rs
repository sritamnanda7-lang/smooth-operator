// SPDX-License-Identifier: FSL-1.1-MIT
//! Step 4 S10 (release half): the shipped binary has no debug fake provider. Same method as step 3's I8.
use std::path::PathBuf;

/// Strings that must never be in a release binary: the fake's env flag and its phrases.
const FORBIDDEN: [&str; 5] = ["SO_BRAIN_FAKE", "fake garbage", "fake question", "fake nokey", "fake slow"];

fn contains(hay: &[u8], needle: &[u8]) -> bool {
    !needle.is_empty() && hay.windows(needle.len()).any(|w| w == needle)
}

fn leaks(bytes: &[u8]) -> Vec<&'static str> {
    FORBIDDEN.iter().copied().filter(|n| contains(bytes, n.as_bytes())).collect()
}

#[test]
fn the_byte_search_finds_a_planted_string_and_stays_quiet_on_clean_bytes() {
    assert_eq!(leaks(b"xx SO_BRAIN_FAKE yy"), vec!["SO_BRAIN_FAKE"]);
    assert_eq!(leaks(b"..fake garbage fake question.."), vec!["fake garbage", "fake question"]);
    assert!(leaks(b"brain outcome=1 provider=0").is_empty());
}

/// `#[ignore]`: needs a release binary. COMPILE-ONLY for the tester; Claude runs it in M16:
/// `SO_RELEASE_BIN=<path to Contents/MacOS/handy> cargo test --lib brain_release -- --ignored`
#[test]
#[ignore]
fn s10_the_release_binary_contains_no_debug_fake_provider_strings() {
    let path = PathBuf::from(std::env::var_os("SO_RELEASE_BIN").expect("set SO_RELEASE_BIN to the release app binary"));
    let bytes = std::fs::read(&path).unwrap_or_else(|e| panic!("cannot read {path:?}: {e}"));
    // Positive controls: this is our app binary AND it has the step-4 brain code (so a clean result means something).
    assert!(contains(&bytes, b"Heard: "), "{path:?} does not look like the app binary");
    assert!(contains(&bytes, b"brain outcome="), "the release binary has no brain metrics string: is it a step-4 build?");
    assert!(contains(&bytes, b"smoothoperator.brain"), "the release Keychain service name is missing");
    assert!(bytes.len() > 1_000_000, "binary suspiciously small: {} bytes", bytes.len());
    let found = leaks(&bytes);
    assert!(found.is_empty(), "debug fake strings found in the release binary: {found:?}");
}
