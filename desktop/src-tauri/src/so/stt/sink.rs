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
use crate::so::rules::{metrics as rules_metrics, PassThrough, Route, TranscriptRouter};
use crate::so::panel::{self, Kind, Line, PanelAction, PanelOut, PanelPayload, TaggedPanel};
use crate::so::settings::HotkeyChoice;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::collections::VecDeque;
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
    /// Real time the event reached the sink (`at` comes from the injected clock).
    arrived: Instant,
}

/// The "Listening" cue is dropped if the talk ended this long before the microphone was ready: it
/// would flash up for a talk that is already over. (Jitter between events is far smaller.)
const STALE_CUE_GRACE: Duration = Duration::from_millis(20);

/// What the microphone is doing, as the worker knows it.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Mic {
    Closed,
    Open,
    Failed,
}

pub struct SpeechSink {
    tx: Sender<Job>,
    /// Number of the newest talk of any kind: +1 at every `Started` and at every typed phrase. Shared
    /// with `Delivery` (an `Arc` is a shared pointer: both sides see the same counter).
    talk: Arc<AtomicU64>,
    /// Number of the live speech talk: set only at `Started`. Armed, Stopped and Cancelled use it, so a
    /// typed phrase (which takes a newer `talk` number) never renumbers a speech talk in progress.
    speech: Arc<AtomicU64>,
    clock: Arc<dyn Clock>,
    /// Only the typed-phrase input (debug builds) needs it.
    #[cfg(debug_assertions)]
    delivery: Arc<Delivery>,
}

/// Where a non-blank transcript goes (the seam): stale check, router, panel lines. One value is shared
/// by the speech worker and (debug builds) the typed-phrase input, so both take the same path.
struct Delivery {
    talk: Arc<AtomicU64>,
    clock: Arc<dyn Clock>,
    router: Arc<dyn TranscriptRouter>,
    panel: Arc<TaggedPanel>,
}

impl Delivery {
    /// 1. A newer talk started since this one: drop the text (no routing, no panel write), outcome 7.
    ///    You must never get an action you cannot see on the panel.
    /// 2. Otherwise `router.route`; the panel shows "Heard: ..." plus the router's lines, then hides
    ///    after `HEARD_SECS`.
    fn deliver(&self, talk: u64, text: &str, stopped_at: Instant) {
        if self.talk.load(Ordering::SeqCst) > talk {
            let ms = self.clock.now().saturating_duration_since(stopped_at).as_millis() as u64;
            rules_metrics::fast_rule(7, ms);
            return;
        }
        let mut lines = vec![Line::new(heard_line(text), Kind::Info)];
        if let Route::Handled(extra) = self.router.route(text, stopped_at) {
            lines.extend(extra);
        }
        self.panel.show(talk, PanelPayload::new(lines));
        schedule_hide(&self.panel, talk);
    }
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
        Self::with_router(recorder, transcriber, panel, clock, tap, stopper, hotkey, Arc::new(PassThrough))
    }

    /// Same as `new`, with a router for transcripts (`new` plugs in `PassThrough`).
    #[allow(clippy::too_many_arguments)]
    pub fn with_router(
        recorder: Arc<dyn Recorder>,
        transcriber: Arc<dyn Transcriber>,
        panel: Arc<dyn PanelOut>,
        clock: Arc<dyn Clock>,
        tap: Arc<dyn SpeechTap>,
        stopper: Arc<dyn Stopper>,
        hotkey: Box<dyn Fn() -> HotkeyChoice + Send + Sync>,
        router: Arc<dyn TranscriptRouter>,
    ) -> Self {
        let (tx, rx) = channel();
        let talk = Arc::new(AtomicU64::new(0));
        let panel = Arc::new(TaggedPanel::new(panel));
        let delivery = Arc::new(Delivery {
            talk: talk.clone(),
            clock: clock.clone(),
            router,
            panel: panel.clone(),
        });
        let worker = Worker {
            rec: recorder,
            tr: transcriber,
            panel,
            delivery: delivery.clone(),
            clock: clock.clone(),
            tap,
            stopper,
            hotkey,
            mic: Mic::Closed,
            ready_at: None,
            blocked: false,
            timer: None,
            rx,
            pending: VecDeque::new(),
        };
        // If the thread cannot start, events pile up unread: speech is off, the rest still works.
        let _ = std::thread::Builder::new()
            .name("so-speech".into())
            .spawn(move || worker.run());
        SpeechSink {
            tx,
            talk,
            speech: Arc::new(AtomicU64::new(0)),
            clock,
            #[cfg(debug_assertions)]
            delivery,
        }
    }

    /// Debug builds only: a typed phrase enters as a new talk (talk id +1, `stopped_at` = now), then
    /// `deliver`, on its own thread so the caller returns at once. No "Listening" cue.
    #[cfg(debug_assertions)]
    pub fn inject_transcript(&self, text: String) {
        let talk = self.talk.fetch_add(1, Ordering::SeqCst) + 1;
        let stopped_at = self.clock.now();
        let delivery = self.delivery.clone();
        let _ = std::thread::Builder::new()
            .name("so-inject".into())
            .spawn(move || delivery.deliver(talk, &text, stopped_at));
    }
}

// `new` (default router) is used by tests; the app builds its sink with `with_router`. Same trick as
// so/mod.rs: this reference avoids a dead-code warning without an allow.
const _: () = {
    let _ = SpeechSink::new;
};

impl TalkSink for SpeechSink {
    fn on_event(&self, event: TalkEvent) {
        let talk = match event {
            TalkEvent::Started(_) => {
                let n = self.talk.fetch_add(1, Ordering::SeqCst) + 1;
                self.speech.store(n, Ordering::SeqCst);
                n
            }
            _ => self.speech.load(Ordering::SeqCst),
        };
        let _ = self.tx.send(Job { event, talk, at: self.clock.now(), arrived: Instant::now() });
    }
}

struct Worker {
    rec: Arc<dyn Recorder>,
    tr: Arc<dyn Transcriber>,
    panel: Arc<TaggedPanel>,
    delivery: Arc<Delivery>,
    clock: Arc<dyn Clock>,
    tap: Arc<dyn SpeechTap>,
    stopper: Arc<dyn Stopper>,
    hotkey: Box<dyn Fn() -> HotkeyChoice + Send + Sync>,
    mic: Mic,
    /// When the microphone last delivered its first audio (`start` returned).
    ready_at: Option<Instant>,
    rx: Receiver<Job>,
    /// Events read ahead of time (to see whether a talk already ended).
    pending: VecDeque<Job>,
    /// The current talk could not start (no model, no mic): its Stopped does nothing.
    blocked: bool,
    /// Cancel flag of the running talk's timer thread.
    timer: Option<Arc<AtomicBool>>,
}

impl Worker {
    /// Blocks on `recv()` while idle, so it costs no CPU.
    fn run(mut self) {
        loop {
            let job = match self.pending.pop_front() {
                Some(job) => job,
                None => match self.rx.recv() {
                    Ok(job) => job,
                    Err(_) => break,
                },
            };
            self.handle(job);
        }
    }

    fn drain(&mut self) {
        while let Ok(job) = self.rx.try_recv() {
            self.pending.push_back(job);
        }
    }

    /// True if this talk's end (Stopped/Cancelled) reached us clearly before the microphone was ready.
    fn ended_before_ready(&mut self, talk: u64) -> bool {
        self.drain();
        let Some(ready) = self.ready_at else { return false };
        self.pending.iter().any(|j| {
            j.talk == talk
                && matches!(j.event, TalkEvent::Stopped(_) | TalkEvent::Cancelled(_))
                && ready.saturating_duration_since(j.arrived) > STALE_CUE_GRACE
        })
    }

    fn handle(&mut self, job: Job) {
        let Job { event, talk, at, .. } = job;
        match event {
            TalkEvent::Armed => self.armed(),
            TalkEvent::Disarmed => self.drop_mic(),
            TalkEvent::Started(mode) => self.started(mode, talk, at),
            TalkEvent::Stopped(_) => self.stopped(talk, at),
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
        self.ready_at = (self.mic == Mic::Open).then(Instant::now);
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
        // The cue ("Listening") appears only now, when the microphone is delivering audio (`start`
        // returned), never earlier. If the talk already ended before that, no cue at all.
        if self.ended_before_ready(talk) {
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

    fn stopped(&mut self, talk: u64, stopped_at: Instant) {
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
        let (tr, panel, delivery) = (self.tr.clone(), self.panel.clone(), self.delivery.clone());
        let spawned = std::thread::Builder::new().name("so-transcribe".into()).spawn(move || {
            let started = Instant::now();
            let result = tr.transcribe(samples);
            let ms = started.elapsed().as_millis() as u64;
            let blank = matches!(&result, Ok(t) if t.trim().is_empty());
            let chars = result.as_ref().map_or(0, |t| t.chars().count());
            metrics::transcribed(ms, if blank { 0 } else { chars }, Self::model_code(state), result.is_ok());
            match result {
                // Non-blank text goes through the seam: stale check, router, "Heard: ..." panel.
                Ok(text) if !blank => delivery.deliver(talk, &text, stopped_at),
                other => {
                    let payload = match other {
                        Ok(_) => one_line(DIDNT_CATCH_TEXT, Kind::Info),
                        Err(_) => one_line(SPEECH_FAILED_TEXT, Kind::Warn),
                    };
                    panel.show(talk, payload);
                    schedule_hide(&panel, talk);
                }
            }
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
