// SPDX-License-Identifier: FSL-1.1-MIT
//! Step 2 tests T25 (WER table) and T26 (bench report has only numbers and labels). Pure.
use crate::so::stt::wer::{normalize, wer, word_errors, BenchAcc};

fn words(v: &[&str]) -> Vec<String> {
    v.iter().map(|s| s.to_string()).collect()
}

#[test]
fn t25_normalize_lowercases_and_drops_punctuation() {
    assert_eq!(normalize("Play Blinding Lights!"), words(&["play", "blinding", "lights"]));
    assert_eq!(normalize("  Hello,   WORLD. "), words(&["hello", "world"]));
    assert_eq!(normalize(""), Vec::<String>::new());
    assert_eq!(normalize("?!..."), Vec::<String>::new());
}

#[test]
fn t25_three_pm_in_all_its_spellings_is_the_same() {
    let want = words(&["three", "pm"]);
    for s in ["3pm", "3 p.m.", "three pm", "3 PM", "Three P.M."] {
        assert_eq!(normalize(s), want, "{s}");
    }
    assert_eq!(wer("meeting tomorrow at 3pm called review", "Meeting tomorrow at three p.m. called Review."), 0.0);
}

#[test]
fn t25_word_errors_insert_delete_substitute() {
    assert_eq!(word_errors("a b c", "a b c"), 0);
    assert_eq!(word_errors("a b c", "a b"), 1, "delete");
    assert_eq!(word_errors("a b c", "a b c d"), 1, "insert");
    assert_eq!(word_errors("a b c", "a x c"), 1, "substitute");
    assert_eq!(word_errors("a b c", "x y z"), 3);
    assert_eq!(word_errors("a b c", ""), 3);
    assert_eq!(word_errors("", "a b"), 2);
    assert_eq!(word_errors("the cat sat", "a cat sat down"), 2, "1 substitute + 1 insert");
}

#[test]
fn t25_wer_is_errors_over_reference_words() {
    assert_eq!(wer("hello world", "hello there"), 0.5);
    assert_eq!(wer("one two three four", "one two three four"), 0.0);
    assert_eq!(wer("a b", "a b c d"), 1.0, "insertions can push WER to 1 or more");
    assert_eq!(wer("a b c d", ""), 1.0);
    assert_eq!(wer("", ""), 0.0);
    assert_eq!(wer("", "anything"), 1.0);
    assert!((wer("a b c", "a x c") - 1.0 / 3.0).abs() < 1e-9);
}

#[test]
fn t26_bench_report_has_only_labels_and_numbers() {
    const REF: &str = "SENTINELREFERENCE words here";
    const HYP: &str = "SENTINELHYPOTHESIS words there";
    let mut acc = BenchAcc::new();
    acc.add("parakeet-v3", "indian", REF, HYP, 410);
    acc.add("parakeet-v3", "indian", "hello world", "hello there", 520);
    let out = acc.render();
    assert!(!out.contains("SENTINEL"), "report leaked text:\n{out}");
    for w in ["hello", "world", "there"] {
        assert!(!out.contains(w), "report leaked the word {w:?}:\n{out}");
    }
    assert!(out.contains("parakeet-v3") && out.contains("indian"), "labels:\n{out}");
    assert!(out.contains("50.0"), "WER of the 2nd row (50.0 %) must appear:\n{out}");
    assert!(out.contains("520"), "max decode ms:\n{out}");
}
