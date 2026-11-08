// SPDX-License-Identifier: FSL-1.1-MIT
//! SpeechSink: turns talk events into recording, transcription and panel text. Tests:
//! `so/tests/stt_tests.rs` (T1-T18, T27).
//!
//! Contract the tests check (details in draft/step-2-plan.md "Behaviour spec"):
//! - `on_event` only pushes `(event, talk_id)` into a channel and returns (<5 ms). One worker thread
//!   (`so-speech`) runs the recorder calls and preload in event order; each transcription runs on its
//!   own short-lived thread.
//! - `talk_id` goes up by 1 at every `Started`. `Armed`/`Disarmed` never write to the panel. Panel writes
//!   go through `TaggedPanel`, tagged with the talk id (newest talk owns the panel).
//! - Armed: model `Ready` -> `Transcriber::preload()` once, and (MIC_OPEN_AT = Down) `Recorder::start()`.
//!   Model `Missing`/`Downloading` -> nothing. A failed `start()` shows nothing yet; the Warn
//!   `MIC_FAILED_TEXT` is shown at `Started`.
//! - Disarmed / Cancelled: `Recorder::cancel()`, no transcribe. Cancelled also hides the panel.
//! - Started: model missing/downloading -> Warn `MODEL_MISSING_TEXT` / `MODEL_DOWNLOADING_TEXT`, mic never
//!   opened. Otherwise `panel::action_for(Started)` text; talk timers start (free-talk: silence + no-speech +
//!   60 s; push-to-talk: 60 s only), polling the injected clock every 100 ms; the end is requested once
//!   through `Stopper::force_stop(talk_id)`.
//! - Stopped: `Recorder::stop()`; no samples -> `DIDNT_CATCH_TEXT` and no transcribe; else
//!   `TRANSCRIBING_TEXT`, then `transcribe` -> `heard_line` (Info) / `DIDNT_CATCH_TEXT` (blank text) /
//!   `SPEECH_FAILED_TEXT` (Warn, no error detail). "Heard"/"Didn't catch" hides after `HEARD_SECS`.
use super::metrics;
use super::silence::{End, SilenceWatch};
use super::{HEARD_SECS, MIC_OPEN_AT, MicOpenAt};
use crate::so::hotkey::session::{Mode, TalkEvent};
use crate::so::hotkey::watcher::{Clock, TalkSink};
use crate::so::panel::{self, Kind, Line, PanelAction, PanelOut, PanelPayload, TaggedPanel};
use crate::so::settings::HotkeyChoice;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Arc;
use std::time::{Duration, Instant};

pub const MODEL_MISSING_TEXT: &str = "Speech model not downloaded: open Settings → Models";
pub const MODEL_DOWNLOADING_TEXT: &str = "Speech model still downloading";
pub const TRANSCRIBING_TEXT: &str = "Transcribing…";
pub const DIDNT_CATCH_TEXT: &str = "Didn't catch that";
pub const SPEECH_FAILED_TEXT: &str = "Speech failed: try again";
pub const MIC_FAILED_TEXT: &str = "Microphone could not start: check Settings → Privacy → Microphone";
pub const HEARD_PREFIX: &str = "Heard: ";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModelState {
    Missing,
    Downloading,
    Ready,
}

/// Real = `AudioRecordingManager`. Errors are plain text and never shown to the user.
pub trait Recorder: Send + Sync {
    fn start(&self) -> Result<(), String>;
    /// Stops and returns the samples (16 kHz mono). `None` = nothing was recorded.
    fn stop(&self) -> Option<Vec<f32>>;
    /// Stops and drops the samples.
    fn cancel(&self);
}

/// Real = `TranscriptionManager`.
pub trait Transcriber: Send + Sync {
    fn downloaded(&self) -> ModelState;
    /// Start a background model load if none is loaded or running. Returns at once.
    fn preload(&self);
    /// Waits for a load in progress. Errors are plain text and never shown to the user.
    fn transcribe(&self, samples: Vec<f32>) -> Result<String, String>;
}

/// Last instant a speech frame passed the VAD.
pub trait SpeechTap: Send + Sync {
    fn last_speech(&self) -> Option<Instant>;
}

/// Asks the hotkey watcher to end talk number `seq` (real = `Msg::ForceStop(seq)`).
pub trait Stopper: Send + Sync {
    fn force_stop(&self, seq: u64);
}

/// Longest "Heard" line, prefix included.
const HEARD_MAX_CHARS: usize = 200;
/// How often a running talk checks the clock for its end.
const TIMER_POLL: Duration = Duration::from_millis(100);

/// The panel line for a transcript: `"Heard: "` + the text trimmed, control characters (and
/// newlines) turned into spaces, at most 200 chars in all; if cut, the last char is `…`.
/// Quotes, backslashes, `</script>` and U+2028 are kept (the panel's JS encoding handles them).
pub fn heard_line(text: &str) -> String {
    let clean: String = text
        .trim()
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect();
    let line = format!("{HEARD_PREFIX}{clean}");
    if line.chars().count() <= HEARD_MAX_CHARS {
        return line;
    }
    let mut cut: String = line.chars().take(HEARD_MAX_CHARS - 1).collect();
    cut.push('…');
    cut
}

/// One talk event with its talk id and the time it reached the sink.
struct Job {
    event: TalkEvent,
    talk: u64,
    at: Instant,
}

/// What the microphone is doing, as the worker knows it.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Mic {
    Closed,
    Open,
    Failed,
}

pub struct SpeechSink {
    tx: Sender<Job>,
    /// Number of the newest talk: +1 at every `Started`. Armed/Disarmed never change it.
    talk: AtomicU64,
    clock: Arc<dyn Clock>,
}

impl SpeechSink {
    /// Spawns the `so-speech` worker. `on_event` only queues; the worker does every recorder call
    /// and model-load start in event order, so a slow microphone can never hold up key timing.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        recorder: Arc<dyn Recorder>,
        transcriber: Arc<dyn Transcriber>,
        panel: Arc<dyn PanelOut>,
        clock: Arc<dyn Clock>,
        tap: Arc<dyn SpeechTap>,
        stopper: Arc<dyn Stopper>,
        hotkey: Box<dyn Fn() -> HotkeyChoice + Send + Sync>,
    ) -> Self {
        let (tx, rx) = channel();
        let worker = Worker {
            rec: recorder,
            tr: transcriber,
            panel: Arc::new(TaggedPanel::new(panel)),
            clock: clock.clone(),
            tap,
            stopper,
            hotkey,
            mic: Mic::Closed,
            blocked: false,
            timer: None,
        };
        // If the thread cannot start, events pile up unread: speech is off, the rest still works.
        let _ = std::thread::Builder::new()
            .name("so-speech".into())
            .spawn(move || worker.run(rx));
        SpeechSink { tx, talk: AtomicU64::new(0), clock }
    }
}

impl TalkSink for SpeechSink {
    fn on_event(&self, event: TalkEvent) {
        let talk = match event {
            TalkEvent::Started(_) => self.talk.fetch_add(1, Ordering::SeqCst) + 1,
            _ => self.talk.load(Ordering::SeqCst),
        };
        let _ = self.tx.send(Job { event, talk, at: self.clock.now() });
    }
}

struct Worker {
    rec: Arc<dyn Recorder>,
    tr: Arc<dyn Transcriber>,
    panel: Arc<TaggedPanel>,
    clock: Arc<dyn Clock>,
    tap: Arc<dyn SpeechTap>,
    stopper: Arc<dyn Stopper>,
    hotkey: Box<dyn Fn() -> HotkeyChoice + Send + Sync>,
    mic: Mic,
    /// The current talk could not start (no model, no mic): its Stopped does nothing.
    blocked: bool,
    /// Cancel flag of the running talk's timer thread.
    timer: Option<Arc<AtomicBool>>,
}

impl Worker {
    /// Blocks on `recv()` while idle, so it costs no CPU.
    fn run(mut self, rx: Receiver<Job>) {
        while let Ok(job) = rx.recv() {
            self.handle(job);
        }
    }

    fn handle(&mut self, job: Job) {
        let Job { event, talk, at } = job;
        match event {
            TalkEvent::Armed => self.armed(),
            TalkEvent::Disarmed => self.drop_mic(),
            TalkEvent::Started(mode) => self.started(mode, talk, at),
            TalkEvent::Stopped(_) => self.stopped(talk),
            TalkEvent::Cancelled(_) => {
                self.stop_timer();
                self.drop_mic();
                self.blocked = false;
                self.panel.hide(talk);
            }
        }
    }

    fn model_code(state: ModelState) -> u8 {
        match state {
            ModelState::Missing => 0,
            ModelState::Downloading => 1,
            ModelState::Ready => 2,
        }
    }

    fn open_mic(&mut self) {
        if self.mic == Mic::Open {
            self.rec.cancel();
        }
        self.mic = if self.rec.start().is_ok() { Mic::Open } else { Mic::Failed };
    }

    /// Cancel the recorder (drops the samples) if it was started.
    fn drop_mic(&mut self) {
        if self.mic != Mic::Closed {
            self.rec.cancel();
            self.mic = Mic::Closed;
        }
    }

    fn stop_timer(&mut self) {
        if let Some(flag) = self.timer.take() {
            flag.store(true, Ordering::SeqCst);
        }
    }

    /// A clean press: start loading the model and (default) open the microphone, so nothing said
    /// right after the press is lost. Nothing is shown; the press may still turn out to be a tap.
    fn armed(&mut self) {
        self.stop_timer();
        self.blocked = false;
        if self.tr.downloaded() != ModelState::Ready {
            return;
        }
        self.tr.preload();
        if MIC_OPEN_AT == MicOpenAt::Down {
            self.open_mic();
        }
    }

    fn started(&mut self, mode: Mode, talk: u64, at: Instant) {
        self.stop_timer();
        self.blocked = false;
        let warn = match self.tr.downloaded() {
            ModelState::Missing => Some(MODEL_MISSING_TEXT),
            ModelState::Downloading => Some(MODEL_DOWNLOADING_TEXT),
            ModelState::Ready => {
                // Opens here when `MIC_OPEN_AT` says so, or as a safety net if `Armed` did not.
                if MIC_OPEN_AT == MicOpenAt::Started || self.mic == Mic::Closed {
                    self.open_mic();
                }
                (self.mic == Mic::Failed).then_some(MIC_FAILED_TEXT)
            }
        };
        if let Some(text) = warn {
            self.drop_mic();
            self.blocked = true;
            self.panel.show(talk, one_line(text, Kind::Warn));
            return;
        }
        let hotkey = (self.hotkey)();
        match panel::action_for(TalkEvent::Started(mode), hotkey) {
            PanelAction::Show(p) | PanelAction::ShowThenHide(p, _) => self.panel.show(talk, p),
            PanelAction::Hide => self.panel.hide(talk),
            PanelAction::Ignore => {}
        }
        self.start_timer(mode, talk, at);
    }

    /// Free-talk: silence, no-speech and 60 s limits. Push-to-talk: the 60 s limit only. The end is
    /// asked for once, through the stopper (which tells the key watcher), then the thread exits.
    fn start_timer(&mut self, mode: Mode, talk: u64, started: Instant) {
        let cancel = Arc::new(AtomicBool::new(false));
        self.timer = Some(cancel.clone());
        let (clock, tap, stopper) = (self.clock.clone(), self.tap.clone(), self.stopper.clone());
        let _ = std::thread::Builder::new().name("so-talk-timer".into()).spawn(move || loop {
            std::thread::sleep(TIMER_POLL);
            if cancel.load(Ordering::SeqCst) {
                return;
            }
            let end = SilenceWatch::decide(started, tap.last_speech(), clock.now());
            let wanted = match (mode, end) {
                (_, Some(End::MaxLen)) => Some(2),
                (Mode::FreeTalk, Some(End::Silence)) => Some(0),
                (Mode::FreeTalk, Some(End::NoSpeech)) => Some(1),
                _ => None,
            };
            if let Some(reason) = wanted {
                metrics::timer_end(reason);
                stopper.force_stop(talk);
                return;
            }
        });
    }

    fn stopped(&mut self, talk: u64) {
        self.stop_timer();
        let state = self.tr.downloaded();
        if self.blocked || self.mic != Mic::Open {
            // The warning from Started is on screen; let it go after a moment.
            self.blocked = false;
            self.mic = Mic::Closed;
            schedule_hide(&self.panel, talk);
            return;
        }
        let samples = self.rec.stop();
        self.mic = Mic::Closed;
        let Some(samples) = samples.filter(|s| !s.is_empty()) else {
            self.panel.show(talk, one_line(DIDNT_CATCH_TEXT, Kind::Info));
            schedule_hide(&self.panel, talk);
            return;
        };
        self.panel.show(talk, one_line(TRANSCRIBING_TEXT, Kind::Info));
        let (tr, panel) = (self.tr.clone(), self.panel.clone());
        let spawned = std::thread::Builder::new().name("so-transcribe".into()).spawn(move || {
            let started = Instant::now();
            let result = tr.transcribe(samples);
            let ms = started.elapsed().as_millis() as u64;
            let (payload, chars, ok) = match result {
                Ok(text) if text.trim().is_empty() => (one_line(DIDNT_CATCH_TEXT, Kind::Info), 0, true),
                Ok(text) => (one_line(&heard_line(&text), Kind::Info), text.chars().count(), true),
                Err(_) => (one_line(SPEECH_FAILED_TEXT, Kind::Warn), 0, false),
            };
            metrics::transcribed(ms, chars, Self::model_code(state), ok);
            panel.show(talk, payload);
            schedule_hide(&panel, talk);
        });
        if spawned.is_err() {
            self.panel.show(talk, one_line(SPEECH_FAILED_TEXT, Kind::Warn));
            schedule_hide(&self.panel, talk);
        }
    }
}

fn one_line(text: &str, kind: Kind) -> PanelPayload {
    PanelPayload::new(vec![Line::new(text, kind)])
}

/// Hide the panel after `HEARD_SECS`. The tagged panel drops it if a newer talk owns the panel.
fn schedule_hide(panel: &Arc<TaggedPanel>, talk: u64) {
    let panel = panel.clone();
    let _ = std::thread::Builder::new().name("so-panel-hide".into()).spawn(move || {
        std::thread::sleep(Duration::from_secs(HEARD_SECS));
        panel.hide(talk);
    });
}
