// SPDX-License-Identifier: FSL-1.1-MIT
//! Word error rate for the model benchmark. Pure. Tests: `so/tests/wer_tests.rs` (T25, T26).

/// STEP-2 STUB (`todo!()`). Lower-case; punctuation (including the dots in "p.m.") is dropped;
/// "3pm" / "3 p.m." / "three pm" all become `["three", "pm"]` (digits 0-99 spelled out; a number
/// glued to letters is split).
pub fn normalize(_text: &str) -> Vec<String> {
    todo!("step 2: wer::normalize")
}

/// STEP-2 STUB (`todo!()`). Word-level Levenshtein distance (insert + delete + substitute) between the
/// normalized texts.
pub fn word_errors(_reference: &str, _hypothesis: &str) -> usize {
    todo!("step 2: wer::word_errors")
}

/// STEP-2 STUB (`todo!()`). `word_errors / reference word count`, as a ratio (0.5 = 50 %). Empty
/// reference: 0.0 if the hypothesis is empty too, else 1.0.
pub fn wer(_reference: &str, _hypothesis: &str) -> f64 {
    todo!("step 2: wer::wer")
}

/// Collects benchmark rows. It sees reference and hypothesis text to score them, but `render` never
/// prints either (test T26): only labels and numbers.
pub struct BenchAcc {
    _private: (),
}

impl BenchAcc {
    pub fn new() -> Self {
        BenchAcc { _private: () }
    }

    /// STEP-2 STUB (`todo!()`).
    pub fn add(
        &mut self,
        _model: &str,
        _accent: &str,
        _reference: &str,
        _hypothesis: &str,
        _decode_ms: u64,
    ) {
        todo!("step 2: BenchAcc::add")
    }

    /// STEP-2 STUB (`todo!()`). Markdown table: model, accent, n, WER % with one decimal ("50.0"),
    /// median and max decode ms.
    pub fn render(&self) -> String {
        todo!("step 2: BenchAcc::render")
    }
}

impl Default for BenchAcc {
    fn default() -> Self {
        Self::new()
    }
}
