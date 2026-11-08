// SPDX-License-Identifier: FSL-1.1-MIT
//! The floating panel: payload, fixed strings, navigation rule (pure, tested) and the window code.
//! Tests: `so/tests/panel_tests.rs` (T23-T25), static checks S10, S12, S13.

use super::hotkey::session::{Mode, TalkEvent};
use super::hotkey::watcher::TalkSink;
use super::settings::HotkeyChoice;
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

/// Path of the panel page (vite output keeps the source path, like the overlay).
pub const PAGE_PATH: &str = "/src/so/panel/index.html";
pub const MAX_LINES: usize = 6;
pub const MAX_CHARS: usize = 200;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    Info,
    Listening,
    Warn,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Line {
    pub text: String,
    pub kind: Kind,
}

impl Line {
    pub fn new(text: impl Into<String>, kind: Kind) -> Self {
        Line { text: text.into(), kind }
    }
}

/// Wire format (JSON): `{"v":1,"lines":[{"text":"..","kind":"info"}]}`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PanelPayload {
    pub v: u8,
    pub lines: Vec<Line>,
}

impl PanelPayload {
    /// Builds a v1 payload: keeps at most MAX_LINES lines, each cut to MAX_CHARS *characters*.
    pub fn new(lines: Vec<Line>) -> Self {
        let lines = lines
            .into_iter()
            .take(MAX_LINES)
            .map(|l| Line { text: l.text.chars().take(MAX_CHARS).collect(), kind: l.kind })
            .collect();
        PanelPayload { v: 1, lines }
    }

    /// The only JS ever sent to the panel: `window.soPanel.set(JSON.parse(<string literal>))`,
    /// where the literal is the payload JSON encoded as JSON a second time (double-encoded).
    pub fn eval_script(&self) -> String {
        // Encode twice: first to JSON text, then that text as a JSON string literal. A JSON string
        // literal is also a valid JS string literal, so quotes, backslashes and control characters
        // can never break out of it. serde_json escapes control characters (incl. newlines).
        let json = serde_json::to_string(self).unwrap_or_else(|_| String::from("{\"v\":1,\"lines\":[]}"));
        let literal = serde_json::to_string(&json).unwrap_or_else(|_| String::from("\"\""));
        // Also escape `<` (so `</script>` never appears raw) and U+2028/2029 (legal in JSON, but
        // older JS engines treat them as line breaks). `\u003c` etc. are valid JSON escapes too.
        let literal = literal
            .replace('<', "\\u003c")
            .replace('\u{2028}', "\\u2028")
            .replace('\u{2029}', "\\u2029");
        format!("window.soPanel.set(JSON.parse({literal}));")
    }
}

pub trait PanelOut: Send + Sync {
    fn show(&self, payload: PanelPayload);
    fn hide(&self);
}

/// What the panel does for a talk event.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PanelAction {
    Show(PanelPayload),
    ShowThenHide(PanelPayload, Duration),
    Hide,
}

/// Fixed strings. Started(PushToTalk) -> "Listening" (Listening); Started(FreeTalk) ->
/// "Free talk: press <Fn|Right Cmd|Right Ctrl> to stop" (Listening); Stopped(any) -> "Stopped"
/// (Info), hidden after 3 s; Cancelled(any) -> Hide (no text).
pub fn action_for(event: TalkEvent, hotkey: HotkeyChoice) -> PanelAction {
    let one = |text: String, kind: Kind| PanelPayload::new(vec![Line::new(text, kind)]);
    match event {
        TalkEvent::Started(Mode::PushToTalk) => PanelAction::Show(one("Listening".into(), Kind::Listening)),
        TalkEvent::Started(Mode::FreeTalk) => {
            let key = match hotkey {
                HotkeyChoice::Fn => "Fn",
                HotkeyChoice::RightCmd => "Right Cmd",
                HotkeyChoice::RightCtrl => "Right Ctrl",
            };
            PanelAction::Show(one(format!("Free talk: press {key} to stop"), Kind::Listening))
        }
        TalkEvent::Stopped(_) => PanelAction::ShowThenHide(one("Stopped".into(), Kind::Info), HIDE_AFTER_STOP),
        TalkEvent::Cancelled(_) => PanelAction::Hide,
    }
}

/// How long "Stopped" stays visible.
pub const HIDE_AFTER_STOP: Duration = Duration::from_secs(3);

/// Turns talk events into panel changes (step 2 adds speech-to-text beside it).
pub struct PanelSink {
    panel: Arc<dyn PanelOut>,
    hotkey: Box<dyn Fn() -> HotkeyChoice + Send + Sync>,
    /// Bumped on every event so an older "hide later" timer does nothing.
    generation: Arc<AtomicU64>,
}

impl PanelSink {
    pub fn new(panel: Arc<dyn PanelOut>, hotkey: Box<dyn Fn() -> HotkeyChoice + Send + Sync>) -> Self {
        PanelSink { panel, hotkey, generation: Arc::new(AtomicU64::new(0)) }
    }
}

impl TalkSink for PanelSink {
    fn on_event(&self, event: TalkEvent) {
        let gen = self.generation.fetch_add(1, Ordering::SeqCst) + 1;
        match action_for(event, (self.hotkey)()) {
            PanelAction::Show(p) => self.panel.show(p),
            PanelAction::Hide => self.panel.hide(),
            PanelAction::ShowThenHide(p, after) => {
                self.panel.show(p);
                let panel = self.panel.clone();
                let generation = self.generation.clone();
                std::thread::spawn(move || {
                    std::thread::sleep(after);
                    if generation.load(Ordering::SeqCst) == gen {
                        panel.hide();
                    }
                });
            }
        }
    }
}

/// Panel page navigation rule: own page only. Dev server URL only when `debug` is true.
pub fn nav_allowed(url: &tauri::Url, debug: bool) -> bool {
    super::webview::app_origin(url, debug) && url.path() == PAGE_PATH
}
