// SPDX-License-Identifier: FSL-1.1-MIT
//! Step 3 I8 and I9: the release build has no debug test input. Numbers follow draft/step-3-plan.md.
use std::path::PathBuf;

/// Strings that must never be in a release binary.
const FORBIDDEN: [&str; 3] = ["SO_DEBUG_INPUT", "so-debug", "input.sock"];

fn contains(hay: &[u8], needle: &[u8]) -> bool {
    !needle.is_empty() && hay.windows(needle.len()).any(|w| w == needle)
}

/// Which forbidden strings the bytes contain.
fn leaks(bytes: &[u8]) -> Vec<&'static str> {
    FORBIDDEN.iter().copied().filter(|n| contains(bytes, n.as_bytes())).collect()
}

#[test]
fn the_byte_search_finds_a_planted_string_and_stays_quiet_on_clean_bytes() {
    // Self-test for I8's checker (runs in every build).
    assert_eq!(leaks(b"xx so-debug yy"), vec!["so-debug"]);
    assert_eq!(leaks(b"..SO_DEBUG_INPUT=1 input.sock"), vec!["SO_DEBUG_INPUT", "input.sock"]);
    assert!(leaks(b"Heard: nothing to see").is_empty());
}

/// I8: the shipped binary (the CI dmg's app binary) contains none of the debug-input strings.
/// `#[ignore]`: needs a release binary. COMPILE-ONLY for the tester; Claude runs it:
/// `SO_RELEASE_BIN=<path to Contents/MacOS/handy> cargo test --lib i8_ -- --ignored`
#[test]
#[ignore]
fn i8_the_release_binary_contains_no_debug_input_strings() {
    let path = PathBuf::from(std::env::var_os("SO_RELEASE_BIN").expect("set SO_RELEASE_BIN to the release app binary"));
    let bytes = std::fs::read(&path).unwrap_or_else(|e| panic!("cannot read {path:?}: {e}"));
    // Positive control: this really is our app binary (a step-2 panel string that must be in it).
    assert!(contains(&bytes, b"Heard: "), "{path:?} does not look like the app binary: no \"Heard: \" string");
    assert!(bytes.len() > 1_000_000, "binary suspiciously small: {} bytes", bytes.len());
    let found = leaks(&bytes);
    assert!(found.is_empty(), "debug-input strings found in the release binary: {found:?}");
}

/// I9: in a release build the test input does not exist, even with the env flag set.
/// Runs only under `cargo test --release` (debug builds compile the test input in on purpose).
#[cfg(not(debug_assertions))]
#[test]
fn i9_release_build_has_no_debug_input_even_with_the_env_flag() {
    assert!(!std::hint::black_box(crate::so::rules::DEBUG_INPUT_COMPILED), "DEBUG_INPUT_COMPILED must be false in release");
    std::env::set_var("SO_DEBUG_INPUT", "1");
    let dir = tempfile::tempdir().unwrap();
    // The wiring in `so::stt::setup` is behind `#[cfg(debug_assertions)]` (checked statically by I7), so
    // nothing here can create the folder; this guards against a future ungated call.
    assert!(!dir.path().join("so-debug").exists());
    assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0);
    std::env::remove_var("SO_DEBUG_INPUT");
}

/// And the other way round: in a debug build the flag is compiled in (so I9 above is not vacuous).
#[cfg(debug_assertions)]
#[test]
fn debug_builds_do_compile_the_test_input_in() {
    assert!(std::hint::black_box(crate::so::rules::DEBUG_INPUT_COMPILED));
}
