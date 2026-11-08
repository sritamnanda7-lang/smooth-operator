// SPDX-License-Identifier: FSL-1.1-MIT
//! The floating panel: payload, fixed strings, navigation rule (pure, tested) and the window code.
//! Tests: `so/tests/panel_tests.rs` (T23-T25), static checks S10, S12, S13.

use super::hotkey::session::{Mode, TalkEvent};
use super::hotkey::watcher::TalkSink;
use super::settings::HotkeyChoice;
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tauri::webview::{NewWindowResponse, PageLoadEvent};
use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindow};

#[cfg(target_os = "macos")]
use tauri_nspanel::{tauri_panel, CollectionBehavior, PanelBuilder, PanelLevel, StyleMask};

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
        Line {
            text: text.into(),
            kind,
        }
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
            .map(|l| Line {
                text: l.text.chars().take(MAX_CHARS).collect(),
                kind: l.kind,
            })
            .collect();
        PanelPayload { v: 1, lines }
    }

    /// The only JS ever sent to the panel: `window.soPanel.set(JSON.parse(<string literal>))`,
    /// where the literal is the payload JSON encoded as JSON a second time (double-encoded).
    pub fn eval_script(&self) -> String {
        // Encode twice: first to JSON text, then that text as a JSON string literal. A JSON string
        // literal is also a valid JS string literal, so quotes, backslashes and control characters
        // can never break out of it. serde_json escapes control characters (incl. newlines).
        let json =
            serde_json::to_string(self).unwrap_or_else(|_| String::from("{\"v\":1,\"lines\":[]}"));
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
    /// Armed / Disarmed: the panel does nothing (step 2).
    Ignore,
}

/// Fixed strings. Started(PushToTalk) -> "Listening" (Listening); Started(FreeTalk) ->
/// "Free talk: press <Fn|Right Cmd|Right Ctrl> to stop" (Listening); Stopped(any) -> "Stopped"
/// (Info), hidden after 3 s; Cancelled(any) -> Hide (no text).
pub fn action_for(event: TalkEvent, hotkey: HotkeyChoice) -> PanelAction {
    let one = |text: String, kind: Kind| PanelPayload::new(vec![Line::new(text, kind)]);
    match event {
        TalkEvent::Started(Mode::PushToTalk) => {
            PanelAction::Show(one("Listening".into(), Kind::Listening))
        }
        TalkEvent::Started(Mode::FreeTalk) => {
            let key = match hotkey {
                HotkeyChoice::Fn => "Fn",
                HotkeyChoice::RightCmd => "Right Cmd",
                HotkeyChoice::RightCtrl => "Right Ctrl",
            };
            PanelAction::Show(one(
                format!("Free talk: press {key} to stop"),
                Kind::Listening,
            ))
        }
        TalkEvent::Stopped(_) => {
            PanelAction::ShowThenHide(one("Stopped".into(), Kind::Info), HIDE_AFTER_STOP)
        }
        TalkEvent::Cancelled(_) => PanelAction::Hide,
        TalkEvent::Armed | TalkEvent::Disarmed => PanelAction::Ignore,
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
    pub fn new(
        panel: Arc<dyn PanelOut>,
        hotkey: Box<dyn Fn() -> HotkeyChoice + Send + Sync>,
    ) -> Self {
        PanelSink {
            panel,
            hotkey,
            generation: Arc::new(AtomicU64::new(0)),
        }
    }
}

impl TalkSink for PanelSink {
    fn on_event(&self, event: TalkEvent) {
        if matches!(event, TalkEvent::Armed | TalkEvent::Disarmed) {
            return; // never touches the panel or the hide timer
        }
        let gen = self.generation.fetch_add(1, Ordering::SeqCst) + 1;
        match action_for(event, (self.hotkey)()) {
            PanelAction::Show(p) => self.panel.show(p),
            PanelAction::Hide => self.panel.hide(),
            PanelAction::Ignore => {}
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

/// STEP-2 STUB (`todo!()` in `new`/`show`/`hide`). Talk-id-tagged panel writes: the newest talk owns
/// the panel. `show`/`hide` from a talk id lower than the highest id seen so far are dropped.
/// `hide` also clears the stored last payload (`set_last(None)`) when it is not dropped.
#[cfg_attr(not(test), expect(dead_code))] // step-2 stub: remove when wired
pub struct TaggedPanel {
    _panel: Arc<dyn PanelOut>,
}

#[cfg_attr(not(test), expect(dead_code))] // step-2 stub: remove when wired
impl TaggedPanel {
    pub fn new(_panel: Arc<dyn PanelOut>) -> Self {
        todo!("step 2: TaggedPanel::new")
    }
    pub fn show(&self, _talk_id: u64, _payload: PanelPayload) {
        todo!("step 2: TaggedPanel::show")
    }
    pub fn hide(&self, _talk_id: u64) {
        todo!("step 2: TaggedPanel::hide")
    }
}

/// Panel page navigation rule: own page only. Dev server URL only when `debug` is true.
pub fn nav_allowed(url: &tauri::Url, debug: bool) -> bool {
    super::webview::app_origin(url, debug) && url.path() == PAGE_PATH
}

// ---------------------------------------------------------------------------------------------
// The window. (A "non-activating panel" is a macOS window that can be shown without becoming the
// active app or taking the keyboard: typing keeps going into whatever app you were using.)
// ---------------------------------------------------------------------------------------------

pub const LABEL: &str = "so_panel";
const WIDTH: f64 = 440.0;
const HEIGHT: f64 = 200.0;
/// Distance from the top of the screen, in logical points / device-independent pixels.
const TOP_OFFSET: f64 = 40.0;

#[cfg(target_os = "macos")]
tauri_panel! {
    panel!(SoPanel {
        config: {
            can_become_key_window: false,
            is_floating_panel: true
        }
    })
}

/// The last payload sent. Resent when the page finishes loading, so text shown before the page
/// was ready (for example a start-up warning) is not lost.
static LAST: Mutex<Option<PanelPayload>> = Mutex::new(None);

pub fn last_payload() -> Option<PanelPayload> {
    LAST.lock().unwrap_or_else(|e| e.into_inner()).clone()
}

pub fn set_last(payload: Option<PanelPayload>) {
    *LAST.lock().unwrap_or_else(|e| e.into_inner()) = payload;
}

/// The only place that runs JS in the panel: one fixed call with the payload as a data literal.
fn push(win: &WebviewWindow, payload: &PanelPayload) {
    let _ = win.eval(payload.eval_script());
}

/// Monitor under the mouse, else the primary monitor.
///
/// Coordinate spaces (checked on macOS 26 + tao source, step 1): on macOS `cursor_position()` is
/// PHYSICAL (points x the PRIMARY monitor's scale) but `monitor_from_point` compares against
/// CGDisplayBounds, which are POINTS, so the cursor must be divided by the primary scale first.
/// On Windows both are physical, so no conversion.
fn monitor_for_cursor(app: &AppHandle) -> Option<tauri::Monitor> {
    let primary = app.primary_monitor().ok().flatten();
    let found = app.cursor_position().ok().and_then(|pos| {
        #[cfg(target_os = "macos")]
        let (x, y) = {
            let scale = primary.as_ref().map_or(1.0, |m| m.scale_factor());
            (pos.x / scale, pos.y / scale)
        };
        #[cfg(not(target_os = "macos"))]
        let (x, y) = (pos.x, pos.y);
        app.monitor_from_point(x, y).ok().flatten()
    });
    found.or(primary)
}

/// Top-center, `TOP_OFFSET` down, on the monitor under the mouse.
fn place(app: &AppHandle, win: &WebviewWindow) {
    if let Some(m) = monitor_for_cursor(app) {
        // Monitor position/size are physical, in that monitor's own scale: divide by it to get points.
        let s = m.scale_factor();
        let x = m.position().x as f64 / s + (m.size().width as f64 / s - WIDTH) / 2.0;
        let y = m.position().y as f64 / s + TOP_OFFSET;
        let _ = win.set_position(tauri::Position::Logical(tauri::LogicalPosition { x, y }));
    }
}

/// Show `payload`, then hide it after `after`, unless something else was shown in the meantime.
pub fn show_timed(panel: &Arc<dyn PanelOut>, payload: PanelPayload, after: Duration) {
    panel.show(payload.clone());
    let panel = panel.clone();
    std::thread::spawn(move || {
        std::thread::sleep(after);
        if last_payload().as_ref() == Some(&payload) {
            panel.hide();
        }
    });
}

/// Builds the hidden panel window and registers `PanelHandle` as app state.
pub fn create(app: &AppHandle) {
    let url = WebviewUrl::App(PAGE_PATH.trim_start_matches('/').into());
    let debug = cfg!(debug_assertions);
    // Own page only; every `window.open` is denied.
    let nav = move |u: &tauri::Url| nav_allowed(u, debug);
    let new_window =
        |_u: tauri::Url, _f: tauri::webview::NewWindowFeatures| NewWindowResponse::Deny;
    let on_load = |win: WebviewWindow, payload: tauri::webview::PageLoadPayload<'_>| {
        if payload.event() == PageLoadEvent::Finished {
            if let Some(last) = last_payload() {
                push(&win, &last);
            }
        }
    };

    #[cfg(target_os = "macos")]
    {
        let built = PanelBuilder::<_, SoPanel>::new(app, LABEL)
            .url(url)
            .title("Smooth Operator panel")
            .level(PanelLevel::Status)
            .size(tauri::Size::Logical(tauri::LogicalSize {
                width: WIDTH,
                height: HEIGHT,
            }))
            .has_shadow(false)
            .transparent(true)
            .no_activate(true)
            .corner_radius(0.0)
            .style_mask(StyleMask::empty().borderless().nonactivating_panel())
            .with_window(move |w| {
                w.decorations(false)
                    .transparent(true)
                    .focusable(false)
                    .on_navigation(nav)
                    .on_new_window(new_window)
                    .on_page_load(on_load)
            })
            .collection_behavior(
                CollectionBehavior::new()
                    .can_join_all_spaces()
                    .full_screen_auxiliary(),
            )
            .build();
        if let Ok(panel) = built {
            panel.hide();
        }
    }

    #[cfg(not(target_os = "macos"))]
    {
        let _ = tauri::WebviewWindowBuilder::new(app, LABEL, url)
            .title("Smooth Operator panel")
            .resizable(false)
            .inner_size(WIDTH, HEIGHT)
            .shadow(false)
            .maximizable(false)
            .minimizable(false)
            .closable(false)
            .decorations(false)
            .always_on_top(true)
            .skip_taskbar(true)
            .transparent(true)
            .focusable(false)
            .focused(false)
            .visible(false)
            .on_navigation(nav)
            .on_new_window(new_window)
            .on_page_load(on_load)
            .build();
    }

    // Step 1: the panel ignores the mouse, so a click goes to the app underneath.
    if let Some(win) = app.get_webview_window(LABEL) {
        let _ = win.set_ignore_cursor_events(true);
    }
    app.manage(PanelHandle(Arc::new(TauriPanel { app: app.clone() })));
}

/// Managed app state holding the real panel (so commands and start-up code can reach it).
pub struct PanelHandle(Arc<dyn PanelOut>);

/// The real panel, as a `PanelOut`.
pub fn panel_out(app: &AppHandle) -> Arc<dyn PanelOut> {
    app.state::<PanelHandle>().0.clone()
}

struct TauriPanel {
    app: AppHandle,
}

impl PanelOut for TauriPanel {
    fn show(&self, payload: PanelPayload) {
        set_last(Some(payload.clone()));
        let app = self.app.clone();
        // Windows must be touched on the main thread.
        let _ = self.app.run_on_main_thread(move || {
            if let Some(win) = app.get_webview_window(LABEL) {
                push(&win, &payload);
                place(&app, &win);
                let _ = win.show();
            }
        });
    }

    fn hide(&self) {
        set_last(None);
        let app = self.app.clone();
        let _ = self.app.run_on_main_thread(move || {
            if let Some(win) = app.get_webview_window(LABEL) {
                let _ = win.hide();
            }
        });
    }
}
