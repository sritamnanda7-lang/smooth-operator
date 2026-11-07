// SPDX-License-Identifier: FSL-1.1-MIT
//! Harness log: one line per event, `unix_ms gate event ms rss_kb n note` (`-` = none; `n` = a count or length).
//! Never contains key values or transcript text (only a length). A background
//! thread writes the file so key handling never waits on disk or on the RSS tool.

use std::io::Write;
use std::path::PathBuf;
use std::sync::mpsc::{channel, Sender};
use std::sync::OnceLock;
use std::time::{SystemTime, UNIX_EPOCH};

struct Line {
    ts_ms: u128,
    gate: &'static str,
    event: &'static str,
    ms: Option<u128>,
    n: Option<u64>,
    /// Error text for failure events (never transcript text); whitespace folded to `_`.
    note: Option<String>,
    /// `Some` = caller measured RSS at the exact moment; `None` = writer measures.
    rss_kb: Option<u64>,
    measure_rss: bool,
}

/// A global that is set once. (`OnceLock` = a cell that can be filled one time, safely from any thread.)
static TX: OnceLock<Sender<Line>> = OnceLock::new();

/// Start the writer thread. `dir` is the app data dir.
pub fn init(dir: PathBuf) {
    let (tx, rx) = channel::<Line>();
    if TX.set(tx).is_err() {
        return;
    }
    let _ = std::fs::create_dir_all(&dir);
    let path = dir.join("spike-harness.log");
    std::thread::spawn(move || {
        let Ok(mut file) = std::fs::OpenOptions::new().create(true).append(true).open(&path) else {
            return;
        };
        for l in rx {
            let rss = if l.rss_kb.is_some() {
                l.rss_kb
            } else if l.measure_rss {
                super::rss::tree_kb()
            } else {
                None
            };
            let _ = writeln!(
                file,
                "{} {} {} {} {} {} {}",
                l.ts_ms,
                l.gate,
                l.event,
                l.ms.map_or("-".to_string(), |m| m.to_string()),
                rss.map_or("-".to_string(), |r| r.to_string()),
                l.n.map_or("-".to_string(), |v| v.to_string()),
                l.note.as_deref().unwrap_or("-")
            );
        }
    });
}

fn now_ms() -> u128 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis()).unwrap_or(0)
}

fn send(gate: &'static str, event: &'static str, ms: Option<u128>, n: Option<u64>, note: Option<String>, rss_kb: Option<u64>, measure_rss: bool) {
    if let Some(tx) = TX.get() {
        let _ = tx.send(Line { ts_ms: now_ms(), gate, event, ms, n, note, rss_kb, measure_rss });
    }
}

/// Event with no extra numbers; the writer adds the RSS.
pub fn event(gate: &'static str, event: &'static str) {
    send(gate, event, None, None, None, None, true);
}

/// Event with a duration in ms; the writer adds the RSS.
pub fn timed(gate: &'static str, event: &'static str, ms: u128) {
    send(gate, event, Some(ms), None, None, None, true);
}

/// Event with an RSS the caller already measured (gate 8 pre-load / loaded / after-unload).
pub fn with_rss(gate: &'static str, event: &'static str, ms: Option<u128>, rss_kb: Option<u64>) {
    send(gate, event, ms, None, None, rss_kb, false);
}

/// Event with a duration and a count/length (gate 8: transcript length, never the text).
pub fn timed_n(gate: &'static str, event: &'static str, ms: u128, n: u64) {
    send(gate, event, Some(ms), Some(n), None, None, true);
}

/// Failure event with the error text. Folded to one token, cut to 200 chars.
pub fn failed(gate: &'static str, event: &'static str, error: &str) {
    send(gate, event, None, None, Some(fold(error)), None, true);
}

fn fold(text: &str) -> String {
    text.chars().take(200).map(|c| if c.is_whitespace() { '_' } else { c }).collect()
}

#[cfg(test)]
mod tests {
    #[test]
    fn fold_keeps_one_token() {
        assert_eq!(super::fold("Model not found: x\ny"), "Model_not_found:_x_y");
        assert_eq!(super::fold(&"a".repeat(500)).len(), 200);
    }
}
