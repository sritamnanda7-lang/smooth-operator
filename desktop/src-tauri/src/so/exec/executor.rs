// SPDX-License-Identifier: FSL-1.1-MIT
//! Runs a plan task: risk gate, link state, Chrome to front once, request, retries, stop, deadline.
//! Risk is recomputed here with `op_risk`; the step's own risk field is never read. No logging here
//! except through `metrics.rs` (integers only).

use super::ops::{command_for, WallClock};
use super::{metrics, result_text, ExecConfig, Res, StepResult, Tail, BUSY_TEXT, CONNECTING_TEXT, RESULT_SECS, RUNS_NOW};
use crate::so::brain::plan::{op_risk, ops_of, step_line, Action, MediaArg, OpKind, Risk, Step, MAX_STEPS};
use crate::so::hotkey::watcher::Clock;
use crate::so::link::commands::LinkState;
use crate::so::link::protocol::{Command, ResultCode};
use crate::so::link::server::{CmdReply, LinkError};
use crate::so::panel::{Kind, Line, MAX_CHARS};
use crate::so::rules::apps::{resolve, shown_name, AppCatalog, Resolve};
use crate::so::rules::hands::{AppLauncher, MediaKeys};
use crate::so::rules::phrase::MediaKey;
use crate::so::rules::router::{NEXT_TEXT, NO_ACCESSIBILITY_TEXT, PLAY_PAUSE_TEXT, PREVIOUS_TEXT};
use crate::so::rules::PlanRunner;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

pub trait Browser: Send + Sync {
    fn link_state(&self) -> LinkState;
    fn request(&self, cmd: &Command, wait: Duration, stop: &AtomicBool) -> Result<CmdReply, LinkError>;
}

pub enum FrontError {
    NotInstalled,
    Failed,
}

pub trait ChromeFront: Send + Sync {
    fn front(&self) -> Result<(), FrontError>;
}

pub struct Effects {
    pub browser: Arc<dyn Browser>,
    pub chrome: Arc<dyn ChromeFront>,
    pub media: Arc<dyn MediaKeys>,
    pub apps: Arc<dyn AppCatalog>,
    pub launcher: Arc<dyn AppLauncher>,
    pub wall: Arc<dyn WallClock>,
    pub clock: Arc<dyn Clock>,
    /// Only called in slices of at most `poll`.
    pub sleep: Arc<dyn Fn(Duration) + Send + Sync>,
}

pub struct TaskReport {
    pub steps: Vec<StepResult>,
    pub not_run: usize,
    pub retries: u8,
    pub busy: bool,
}

/// Panel events while a task runs.
enum Ui<'a> {
    Working { i: usize, n: usize, action: &'a Action },
    Connecting,
}

/// State of the one running task.
struct Task<'a> {
    stop: &'a AtomicBool,
    start: Instant,
    retries: u8,
    fronted: bool,
    ui: &'a dyn Fn(Ui<'_>),
}

pub struct Executor {
    fx: Effects,
    cfg: ExecConfig,
    /// `Some(stop flag)` while a task runs. A fresh flag per task, so a stop with no task never lingers.
    running: Mutex<Option<Arc<AtomicBool>>>,
}

/// Clears the running mark when the task ends, also on a panic.
struct Running<'a>(&'a Executor);

impl Drop for Running<'_> {
    fn drop(&mut self) {
        *self.0.running.lock().unwrap_or_else(|e| e.into_inner()) = None;
    }
}

fn op_code(op: OpKind) -> u8 {
    match op {
        OpKind::AppOpen => 1,
        OpKind::AppSwitch => 2,
        OpKind::MediaKey => 3,
        OpKind::YoutubePlay => 4,
        OpKind::DocsOpen => 5,
        OpKind::DocsPaste => 6,
        OpKind::CalendarOpen => 7,
        OpKind::CalendarSave => 8,
        OpKind::OpenUrl => 9,
        OpKind::WebSearch => 10,
    }
}

/// Which extension answers fit which op (anything else is a bad result).
fn valid(op: OpKind, r: ResultCode) -> bool {
    use ResultCode::*;
    if op == OpKind::YoutubePlay {
        r != EditorOpen
    } else {
        matches!(r, EditorOpen | NeedsLogin | NoAccess | Timeout | BadCmd | Busy | Unsafe | Cancelled | Error)
    }
}

fn clip(s: String) -> String {
    if s.chars().count() <= MAX_CHARS {
        s
    } else {
        s.chars().take(MAX_CHARS).collect()
    }
}

impl Executor {
    pub fn new(fx: Effects, cfg: ExecConfig) -> Arc<Self> {
        Arc::new(Executor { fx, cfg, running: Mutex::new(None) })
    }

    /// Blocking. While another task runs: busy = true, steps empty, not_run = steps.len().
    pub fn run(&self, steps: &[Step]) -> TaskReport {
        self.go(steps, &|_| {}).0
    }

    /// Step-7 hook: sets the task's stop flag.
    pub fn stop(&self) {
        if let Some(flag) = self.running.lock().unwrap_or_else(|e| e.into_inner()).as_ref() {
            flag.store(true, Ordering::SeqCst);
        }
    }

    pub fn busy(&self) -> bool {
        self.running.lock().unwrap_or_else(|e| e.into_inner()).is_some()
    }

    /// The task: the report plus the panel text of each finished step.
    fn go(&self, steps: &[Step], ui: &dyn Fn(Ui<'_>)) -> (TaskReport, Vec<String>) {
        let stop = Arc::new(AtomicBool::new(false));
        {
            let mut g = self.running.lock().unwrap_or_else(|e| e.into_inner());
            if g.is_some() {
                metrics::exec(3, 0, 0, 0, 0);
                return (TaskReport { steps: Vec::new(), not_run: steps.len(), retries: 0, busy: true }, Vec::new());
            }
            *g = Some(stop.clone());
        }
        let _running = Running(self);
        let mut t = Task { stop: &stop, start: self.fx.clock.now(), retries: 0, fronted: false, ui };
        let mut done: Vec<StepResult> = Vec::new();
        let mut texts: Vec<String> = Vec::new();
        if steps.len() > MAX_STEPS {
            // Too long a plan: nothing runs.
            let first = steps.first().and_then(|s| ops_of(&s.action).first().copied()).unwrap_or(OpKind::OpenUrl);
            done.push(StepResult { first, res: Res::Refused, tail: Tail::None });
            texts.push(result_text(&done[0], false));
            metrics::exec(2, op_code(first), Res::Refused.code(), 0, 0);
        } else {
            for (i, step) in steps.iter().enumerate() {
                if t.stop.load(Ordering::SeqCst) {
                    break;
                }
                let began = self.fx.clock.now();
                let (sr, text) = if self.late(&t) {
                    let first = ops_of(&step.action).first().copied().unwrap_or(OpKind::OpenUrl);
                    let sr = StepResult { first, res: Res::Deadline, tail: Tail::None };
                    let text = result_text(&sr, false);
                    (sr, text)
                } else {
                    (t.ui)(Ui::Working { i: i + 1, n: steps.len(), action: &step.action });
                    self.step(&mut t, step)
                };
                let ms = self.fx.clock.now().saturating_duration_since(began).as_millis() as u64;
                metrics::exec(if sr.res.ok() { 1 } else { 2 }, op_code(sr.first), sr.res.code(), ms, t.retries);
                let go_on = sr.res == Res::NotYet || (sr.res.ok() && sr.tail != Tail::NeedsOk);
                done.push(sr);
                texts.push(text);
                if !go_on {
                    break;
                }
            }
        }
        let report = TaskReport { not_run: steps.len().saturating_sub(done.len()), steps: done, retries: t.retries, busy: false };
        (report, texts)
    }

    fn since(&self, t: Instant) -> Duration {
        self.fx.clock.now().saturating_duration_since(t)
    }

    fn late(&self, t: &Task<'_>) -> bool {
        self.since(t.start) >= self.cfg.task_deadline
    }

    /// Stop first, then the deadline.
    fn halted(&self, t: &Task<'_>) -> Option<Res> {
        if t.stop.load(Ordering::SeqCst) {
            Some(Res::Stopped)
        } else if self.late(t) {
            Some(Res::Deadline)
        } else {
            None
        }
    }

    /// Sleeps `total` in slices of at most `poll`; stop and deadline are checked between slices.
    fn nap(&self, t: &Task<'_>, total: Duration) -> Option<Res> {
        let slice = self.cfg.poll.max(Duration::from_millis(1));
        let mut left = total;
        while !left.is_zero() {
            if let Some(h) = self.halted(t) {
                return Some(h);
            }
            let s = left.min(slice);
            (self.fx.sleep)(s);
            left -= s;
        }
        self.halted(t)
    }

    /// One step: the risk gate, then its first op. Only the first op runs in step 6.
    fn step(&self, t: &mut Task<'_>, step: &Step) -> (StepResult, String) {
        let a = &step.action;
        let ops = ops_of(a);
        let Some(&first) = ops.first() else {
            let sr = StepResult { first: OpKind::OpenUrl, res: Res::Failed, tail: Tail::None };
            return (sr, result_text(&sr, false));
        };
        let tail = match ops.get(1..) {
            Some(rest) if !rest.is_empty() => {
                if rest.iter().any(|op| op_risk(*op, a) != Risk::Safe) {
                    Tail::NeedsOk
                } else {
                    Tail::Deferred
                }
            }
            _ => Tail::None,
        };
        let mut extra: Option<String> = None;
        let res = if op_risk(first, a) != Risk::Safe {
            Res::Refused
        } else if !RUNS_NOW.contains(&first) {
            extra = Some(format!("Not available yet: {}", step_line(a)));
            Res::NotYet
        } else {
            match first {
                OpKind::AppOpen | OpKind::AppSwitch | OpKind::MediaKey => {
                    let (res, text) = self.local(first, a);
                    extra = text;
                    res
                }
                OpKind::YoutubePlay | OpKind::DocsOpen | OpKind::CalendarOpen => self.web(t, first, a),
                _ => Res::Failed,
            }
        };
        let sr = StepResult { first, res, tail };
        let guests = matches!(a, Action::CalendarNewEvent { guests, .. } if !guests.is_empty());
        let text = extra.unwrap_or_else(|| result_text(&sr, guests));
        (sr, text)
    }

    /// Media key and app open/switch: the step-3 effects. The app comes from the installed list, never
    /// from the spoken name.
    fn local(&self, op: OpKind, a: &Action) -> (Res, Option<String>) {
        match (op, a) {
            (OpKind::MediaKey, Action::Media { key }) => {
                let (k, text) = match key {
                    MediaArg::PlayPause => (MediaKey::PlayPause, PLAY_PAUSE_TEXT),
                    MediaArg::Next => (MediaKey::Next, NEXT_TEXT),
                    MediaArg::Previous => (MediaKey::Previous, PREVIOUS_TEXT),
                };
                match self.fx.media.press(k) {
                    Ok(()) => (Res::Done, Some(text.to_string())),
                    Err(_) => (Res::Failed, Some(NO_ACCESSIBILITY_TEXT.to_string())),
                }
            }
            (OpKind::AppOpen | OpKind::AppSwitch, Action::OpenApp { name } | Action::SwitchApp { name }) => {
                let shown = shown_name(name);
                match resolve(name, &self.fx.apps.list()) {
                    Resolve::Found(app) => {
                        let verb = if op == OpKind::AppSwitch { "Switching to" } else { "Opening" };
                        match self.fx.launcher.open(&app.path) {
                            Ok(()) => (Res::Done, Some(format!("{verb} {shown}"))),
                            Err(_) => (Res::Failed, Some(format!("Couldn't open {shown}"))),
                        }
                    }
                    Resolve::NotFound => (Res::AppNotFound, Some(format!("I couldn't find an app called {shown}"))),
                    Resolve::Ambiguous(names) => {
                        let names: Vec<String> = names.iter().map(|n| shown_name(n)).collect();
                        (Res::AppAmbiguous, Some(format!("More than one app matches {shown}: {}. Say the full name.", names.join(", "))))
                    }
                }
            }
            _ => (Res::Failed, None),
        }
    }

    /// Link state, then Chrome to the front once per task, then wait for the extension to connect.
    /// NotPaired, PortBusy and KeychainError come first: Chrome is not touched for them.
    fn prepare(&self, t: &mut Task<'_>) -> Result<(), Res> {
        let gate = |s: LinkState| match s {
            LinkState::NotPaired => Err(Res::NotPaired),
            LinkState::PortBusy => Err(Res::PortBusy),
            LinkState::KeychainError => Err(Res::KeychainError),
            LinkState::Connected | LinkState::NotFound => Ok(s),
        };
        let mut state = gate(self.fx.browser.link_state())?;
        if !t.fronted {
            self.fx.chrome.front().map_err(|e| match e {
                FrontError::NotInstalled => Res::ChromeMissing,
                FrontError::Failed => Res::Failed,
            })?;
            t.fronted = true;
        }
        let began = self.fx.clock.now();
        let mut said = false;
        while state != LinkState::Connected {
            if let Some(h) = self.halted(t) {
                return Err(h);
            }
            let waited = self.since(began);
            if waited >= self.cfg.link_wait {
                return Err(Res::NotConnected);
            }
            if !said && waited >= self.cfg.connecting_after {
                (t.ui)(Ui::Connecting);
                said = true;
            }
            (self.fx.sleep)(self.cfg.poll.max(Duration::from_millis(1)).min(self.cfg.link_wait - waited));
            state = gate(self.fx.browser.link_state())?;
        }
        Ok(())
    }

    /// One web op with its retries. Retry only on not-sent (link NotConnected / Busy) and ext `busy`;
    /// idempotent ops (YouTube, calendar) also on ext timeout / error and link Timeout / Lost.
    /// `docs_open` never repeats once it may have made a document.
    fn web(&self, t: &mut Task<'_>, op: OpKind, a: &Action) -> Res {
        let cmd = match command_for(op, a, &*self.fx.wall) {
            Ok(c) => c,
            Err(r) => return r,
        };
        let idempotent = op != OpKind::DocsOpen;
        let base = if op == OpKind::YoutubePlay { self.cfg.yt_wait } else { self.cfg.page_wait };
        loop {
            if let Some(h) = self.halted(t) {
                return h;
            }
            if let Err(r) = self.prepare(t) {
                return r;
            }
            let wait = base.min(self.cfg.task_deadline.saturating_sub(self.since(t.start)));
            if wait.is_zero() {
                return Res::Deadline;
            }
            let (res, retry) = match self.fx.browser.request(&cmd, wait, t.stop) {
                Ok(rep) if !valid(op, rep.r) => (Res::BadResult, false),
                Ok(rep) => (
                    Res::Ext(rep.r),
                    rep.r == ResultCode::Busy || (idempotent && matches!(rep.r, ResultCode::Timeout | ResultCode::Error)),
                ),
                Err(LinkError::NotConnected) => (Res::NotConnected, true),
                Err(LinkError::Busy) => (Res::Failed, true),
                Err(LinkError::Timeout) => (Res::LinkTimeout, idempotent),
                Err(LinkError::Lost) => (Res::Lost, idempotent),
                Err(LinkError::TooLong) => (Res::TooLong, false),
                Err(LinkError::Stopped) => return Res::Stopped,
            };
            if !retry {
                // A deadline that passed wins over a timeout that is not retried (docs_open).
                if matches!(res, Res::LinkTimeout | Res::Ext(ResultCode::Timeout)) && self.late(t) {
                    return Res::Deadline;
                }
                return res;
            }
            if let Some(h) = self.halted(t) {
                return h;
            }
            if t.retries >= self.cfg.max_retries {
                return res;
            }
            t.retries += 1;
            if let Some(h) = self.nap(t, self.cfg.retry_pause) {
                return h;
            }
        }
    }
}

impl PlanRunner for Executor {
    fn run_plan(&self, steps: &[Step], show: &dyn Fn(Vec<Line>)) -> u64 {
        let working = Mutex::new(String::new());
        let ui = |u: Ui<'_>| {
            let mut lines = Vec::new();
            {
                let mut w = working.lock().unwrap_or_else(|e| e.into_inner());
                if let Ui::Working { i, n, action } = u {
                    *w = clip(format!("Working (step {i} of {n}): {}", step_line(action)));
                }
                lines.push(Line::new(w.clone(), Kind::Info));
            }
            if matches!(u, Ui::Connecting) {
                lines.push(Line::new(CONNECTING_TEXT, Kind::Info));
            }
            show(lines);
        };
        let (report, texts) = self.go(steps, &ui);
        let _ = report.retries; // already logged per op; kept on the report for `run` callers
        if report.busy {
            show(vec![Line::new(BUSY_TEXT, Kind::Warn)]);
            return RESULT_SECS;
        }
        if steps.len() > MAX_STEPS {
            // A plan over the limit never runs: one plain line, no numbering (test X12).
            show(vec![Line::new(texts.first().cloned().unwrap_or_default(), Kind::Warn)]);
            return RESULT_SECS;
        }
        // The last 4 results (a failure stays visible), numbered by step, then what was not run.
        let first = report.steps.len().saturating_sub(4);
        let mut lines: Vec<Line> = report
            .steps
            .iter()
            .zip(&texts)
            .enumerate()
            .skip(first)
            .map(|(i, (sr, text))| Line::new(clip(format!("{}. {text}", i + 1)), if sr.res.ok() || sr.res == Res::NotYet { Kind::Info } else { Kind::Warn }))
            .collect();
        if report.not_run > 0 {
            let n = report.not_run;
            lines.push(Line::new(format!("Not run: {n} more step{}", if n == 1 { "" } else { "s" }), Kind::Info));
        }
        if !lines.is_empty() {
            show(lines);
        }
        RESULT_SECS
    }
}

// `run` and `busy` are the plain-call API (tests, step 7); the app goes through `run_plan`. Same
// reference trick as so/mod.rs: no dead-code warning, no allow.
const _: () = {
    let _ = Executor::run;
    let _ = Executor::busy;
};
