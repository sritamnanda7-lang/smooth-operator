// SPDX-License-Identifier: FSL-1.1-MIT
//! The floating panel. STUB: only payload/view/navigation logic is tested in step 1 sub-step 1;
//! the window code (NSPanel etc., S12/S13) comes in the panel sub-step.
//! Tests: `so/tests/panel_tests.rs` (T23-T25).

use super::hotkey::session::TalkEvent;
use super::settings::HotkeyChoice;
use serde::{Deserialize, Serialize};
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
    pub fn new(_lines: Vec<Line>) -> Self {
        todo!("step 1: trim in Rust")
    }

    /// The only JS ever sent to the panel: `window.soPanel.set(JSON.parse(<string literal>))`,
    /// where the literal is the payload JSON encoded as JSON a second time (double-encoded).
    pub fn eval_script(&self) -> String {
        todo!("step 1: double-encoded eval literal")
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
pub fn action_for(_event: TalkEvent, _hotkey: HotkeyChoice) -> PanelAction {
    todo!("step 1: fixed panel strings")
}

/// Panel page navigation rule: own page only. Dev server URL only when `debug` is true.
pub fn nav_allowed(_url: &tauri::Url, _debug: bool) -> bool {
    todo!("step 1: panel navigation rule")
}
