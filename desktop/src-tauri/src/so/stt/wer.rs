// SPDX-License-Identifier: FSL-1.1-MIT
//! Word error rate for the model benchmark. Pure. Tests: `so/tests/wer_tests.rs` (T25, T26).
//!
//! WER = (substituted + deleted + inserted words) / words in the reference. "Levenshtein" is the
//! classic edit-distance table; here the unit is a word, not a letter.

use std::collections::BTreeMap;

const ONES: [&str; 20] = [
    "zero", "one", "two", "three", "four", "five", "six", "seven", "eight", "nine", "ten", "eleven",
    "twelve", "thirteen", "fourteen", "fifteen", "sixteen", "seventeen", "eighteen", "nineteen",
];
const TENS: [&str; 8] = ["twenty", "thirty", "forty", "fifty", "sixty", "seventy", "eighty", "ninety"];

/// Lower-case; punctuation (including the dots in "p.m.") is dropped; "3pm" / "3 p.m." / "three pm"
/// all become `["three", "pm"]` (digits 0-99 spelled out; a number glued to letters is split).
pub fn normalize(text: &str) -> Vec<String> {
    let chars: Vec<char> = text.to_lowercase().chars().collect();
    // Pass 1: drop the dot inside an abbreviation (single letter, dot, letter) so "p.m." is "pm".
    let mut flat = String::with_capacity(chars.len());
    for (i, &c) in chars.iter().enumerate() {
        if c == '.' && i >= 1 && chars[i - 1].is_alphabetic() && chars.get(i + 1).is_some_and(|n| n.is_alphabetic()) {
            let single = i < 2 || !chars[i - 2].is_alphabetic();
            if single {
                continue;
            }
        }
        flat.push(c);
    }
    // Pass 2: split on everything but letters, digits and apostrophes; split digit/letter borders.
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut flush = |cur: &mut String, out: &mut Vec<String>| {
        if cur.is_empty() {
            return;
        }
        match cur.parse::<u32>() {
            Ok(n) if n < 100 => out.extend(spell(n).split(' ').map(str::to_string)),
            _ => out.push(std::mem::take(cur)),
        }
        cur.clear();
    };
    let mut prev_digit = false;
    for c in flat.chars() {
        if c.is_alphanumeric() || c == '\'' {
            let digit = c.is_ascii_digit();
            if !cur.is_empty() && digit != prev_digit && c != '\'' {
                flush(&mut cur, &mut out);
            }
            cur.push(c);
            prev_digit = digit;
        } else {
            flush(&mut cur, &mut out);
        }
    }
    flush(&mut cur, &mut out);
    out.retain(|w| w.chars().any(char::is_alphanumeric));
    out
}

/// 0-99 in words ("21" -> "twenty one").
fn spell(n: u32) -> String {
    if n < 20 {
        return ONES[n as usize].to_string();
    }
    let tens = TENS[(n / 10 - 2) as usize];
    match n % 10 {
        0 => tens.to_string(),
        u => format!("{tens} {}", ONES[u as usize]),
    }
}

fn distance(r: &[String], h: &[String]) -> usize {
    let mut prev: Vec<usize> = (0..=h.len()).collect();
    for (i, rw) in r.iter().enumerate() {
        let mut cur = vec![i + 1];
        for (j, hw) in h.iter().enumerate() {
            let sub = prev[j] + usize::from(rw != hw);
            cur.push(sub.min(prev[j + 1] + 1).min(cur[j] + 1));
        }
        prev = cur;
    }
    prev[h.len()]
}

/// Word-level Levenshtein distance (insert + delete + substitute) between the normalized texts.
pub fn word_errors(reference: &str, hypothesis: &str) -> usize {
    distance(&normalize(reference), &normalize(hypothesis))
}

/// `word_errors / reference word count`, as a ratio (0.5 = 50 %). Empty reference: 0.0 if the
/// hypothesis is empty too, else 1.0.
pub fn wer(reference: &str, hypothesis: &str) -> f64 {
    let n = normalize(reference).len();
    let e = word_errors(reference, hypothesis);
    if n == 0 {
        return if e == 0 { 0.0 } else { 1.0 };
    }
    e as f64 / n as f64
}

#[derive(Default)]
struct Cell {
    errors: usize,
    words: usize,
    exact: usize,
    clip_wer: Vec<f64>,
    ms: Vec<u64>,
}

/// Collects benchmark rows. It sees reference and hypothesis text to score them, but `render` never
/// prints either (test T26): only labels and numbers.
pub struct BenchAcc {
    cells: BTreeMap<(String, String), Cell>,
}

impl BenchAcc {
    pub fn new() -> Self {
        BenchAcc { cells: BTreeMap::new() }
    }

    pub fn add(&mut self, model: &str, accent: &str, reference: &str, hypothesis: &str, decode_ms: u64) {
        let c = self.cells.entry((model.to_string(), accent.to_string())).or_default();
        let e = word_errors(reference, hypothesis);
        c.errors += e;
        c.words += normalize(reference).len();
        c.exact += usize::from(e == 0);
        c.clip_wer.push(wer(reference, hypothesis));
        c.ms.push(decode_ms);
    }

    /// Pooled WER % (all errors / all reference words) for one model x accent, if it has rows.
    pub fn pooled_wer_pct(&self, model: &str, accent: &str) -> Option<f64> {
        let c = self.cells.get(&(model.to_string(), accent.to_string()))?;
        Some(if c.words == 0 { 0.0 } else { 100.0 * c.errors as f64 / c.words as f64 })
    }

    /// Markdown table: model, accent, n, pooled WER % with one decimal ("50.0"), exact-match %, best
    /// and worst clip WER %, median and max decode ms.
    pub fn render(&self) -> String {
        let mut s = String::from(
            "| model | accent | n | WER % | exact % | best clip WER % | worst clip WER % | median ms | max ms |\n\
             |---|---|---|---|---|---|---|---|---|\n",
        );
        for ((model, accent), c) in &self.cells {
            let n = c.clip_wer.len();
            let wer_pct = if c.words == 0 { 0.0 } else { 100.0 * c.errors as f64 / c.words as f64 };
            let best = c.clip_wer.iter().copied().fold(f64::INFINITY, f64::min) * 100.0;
            let worst = c.clip_wer.iter().copied().fold(0.0, f64::max) * 100.0;
            let mut ms = c.ms.clone();
            ms.sort_unstable();
            let median = match ms.len() {
                0 => 0,
                l if l % 2 == 1 => ms[l / 2],
                l => (ms[l / 2 - 1] + ms[l / 2]) / 2,
            };
            let max = ms.last().copied().unwrap_or(0);
            s.push_str(&format!(
                "| {model} | {accent} | {n} | {wer_pct:.1} | {:.1} | {best:.1} | {worst:.1} | {median} | {max} |\n",
                100.0 * c.exact as f64 / n.max(1) as f64
            ));
        }
        s
    }
}

impl Default for BenchAcc {
    fn default() -> Self {
        Self::new()
    }
}
