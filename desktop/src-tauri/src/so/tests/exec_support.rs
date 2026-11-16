// SPDX-License-Identifier: FSL-1.1-MIT
//! Shared fakes for the step-6 tests (`exec_*`). Fakes only: a scripted browser (the extension link), a
//! scripted Chrome-to-front, the step-3 media/catalog/launcher fakes, a scripted zone/wall clock, a test
//! clock and a recording sleep that advances that clock (no real sleeping). Numbers follow
//! draft/step-6-plan.md (Rev 2).
use super::rules_support::{app, FakeCatalog, FakeClock, FakeLauncher, FakeMedia, Log};
use crate::so::brain::plan::{Action, MediaArg, Risk, Step};
use crate::so::exec::executor::{Browser, ChromeFront, Effects, Executor, FrontError, TaskReport};
use crate::so::exec::ops::WallClock;
use crate::so::exec::ExecConfig;
use crate::so::link::commands::LinkState;
use crate::so::link::protocol::{Command, ResultCode};
use crate::so::link::server::{CmdReply, LinkError};
use crate::so::panel::Line;
use crate::so::rules::PlanRunner;
use chrono::{DateTime, Duration as CDuration, LocalResult, NaiveDate, NaiveDateTime, Utc};
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;

// ---------- the config the tests use (distinct from EXEC so a hard-coded constant fails) ----------
pub const LINK_WAIT: Duration = Duration::from_secs(35);
pub const CONNECTING_AFTER: Duration = Duration::from_secs(1);
pub const POLL: Duration = Duration::from_millis(200);
pub const RETRY_PAUSE: Duration = Duration::from_secs(1);
pub const YT_WAIT: Duration = Duration::from_secs(41);
pub const PAGE_WAIT: Duration = Duration::from_secs(21);
pub const DEADLINE: Duration = Duration::from_secs(90);

pub fn cfg() -> ExecConfig {
    ExecConfig {
        link_wait: LINK_WAIT,
        connecting_after: CONNECTING_AFTER,
        poll: POLL,
        retry_pause: RETRY_PAUSE,
        max_retries: 3,
        yt_wait: YT_WAIT,
        page_wait: PAGE_WAIT,
        task_deadline: DEADLINE,
    }
}

// ---------- steps ----------
pub fn yt(query: &str) -> Step {
    Step { action: Action::YoutubePlay { query: query.into() }, risk: Risk::Safe }
}
pub fn docs(topic: &str) -> Step {
    Step { action: Action::DocsNew { topic: topic.into(), words: Some(300) }, risk: Risk::Approve }
}
pub fn cal(title: &str, guests: &[&str]) -> Step {
    let guests: Vec<String> = guests.iter().map(|g| g.to_string()).collect();
    let risk = if guests.is_empty() { Risk::Safe } else { Risk::Approve };
    Step { action: Action::CalendarNewEvent { title: title.into(), date: "tomorrow".into(), time: "15:00".into(), minutes: None, guests }, risk }
}
pub fn media(key: MediaArg) -> Step {
    Step { action: Action::Media { key }, risk: Risk::Safe }
}
pub fn open_app(name: &str) -> Step {
    Step { action: Action::OpenApp { name: name.into() }, risk: Risk::Safe }
}
pub fn switch_app(name: &str) -> Step {
    Step { action: Action::SwitchApp { name: name.into() }, risk: Risk::Safe }
}
pub fn open_url(url: &str) -> Step {
    Step { action: Action::OpenUrl { url: url.into() }, risk: Risk::Safe }
}
pub fn web_search(q: &str) -> Step {
    Step { action: Action::WebSearch { query: q.into() }, risk: Risk::Safe }
}

// ---------- the scripted zone / wall clock ----------
/// A fixed UTC offset plus scripted DST trouble: `gaps` are local times that do not exist, `overlaps` are
/// local times that happen twice (`early` and `late` offsets in seconds east of UTC).
pub struct FakeWall {
    pub now: Mutex<NaiveDateTime>,
    pub offset_secs: i32,
    pub gaps: Vec<NaiveDateTime>,
    pub overlaps: Vec<(NaiveDateTime, i32, i32)>,
}
pub fn dt(y: i32, m: u32, d: u32, h: u32, min: u32) -> NaiveDateTime {
    NaiveDate::from_ymd_opt(y, m, d).unwrap().and_hms_opt(h, min, 0).unwrap()
}
impl FakeWall {
    /// Now = 2026-11-15 10:00 (a Sunday) local, zone UTC+05:30.
    pub fn india() -> Arc<Self> {
        Self::at(dt(2026, 11, 15, 10, 0), 19_800)
    }
    pub fn at(now: NaiveDateTime, offset_secs: i32) -> Arc<Self> {
        Arc::new(FakeWall { now: Mutex::new(now), offset_secs, gaps: vec![], overlaps: vec![] })
    }
    pub fn with_dst(now: NaiveDateTime, offset_secs: i32, gaps: Vec<NaiveDateTime>, overlaps: Vec<(NaiveDateTime, i32, i32)>) -> Arc<Self> {
        Arc::new(FakeWall { now: Mutex::new(now), offset_secs, gaps, overlaps })
    }
    fn utc(local: NaiveDateTime, offset: i32) -> DateTime<Utc> {
        DateTime::from_naive_utc_and_offset(local - CDuration::seconds(offset as i64), Utc)
    }
}
impl WallClock for FakeWall {
    fn now_local(&self) -> NaiveDateTime {
        *self.now.lock().unwrap()
    }
    fn to_utc(&self, local: NaiveDateTime) -> LocalResult<DateTime<Utc>> {
        if self.gaps.contains(&local) {
            return LocalResult::None;
        }
        if let Some((_, early, late)) = self.overlaps.iter().find(|o| o.0 == local) {
            return LocalResult::Ambiguous(Self::utc(local, *early), Self::utc(local, *late));
        }
        LocalResult::Single(Self::utc(local, self.offset_secs))
    }
}

// ---------- the scripted browser (the link to the extension) ----------
pub type Answer = Result<CmdReply, LinkError>;
pub fn ok(r: ResultCode) -> Answer {
    Ok(CmdReply { r, ms: 1234 })
}
pub fn err(e: LinkError) -> Answer {
    Err(e)
}

type StateFn = Box<dyn Fn() -> LinkState + Send + Sync>;
pub type SleepHook = Arc<Mutex<Option<Box<dyn Fn(usize) + Send + Sync>>>>;
type ReqHook = Box<dyn Fn(usize, &AtomicBool) + Send + Sync>;

pub struct FakeBrowser {
    pub log: Arc<Log>,
    state: Mutex<StateFn>,
    script: Mutex<VecDeque<Answer>>,
    pub reqs: Mutex<Vec<(Command, Duration)>>,
    /// Called at the start of every request with its index and the stop flag the executor passed.
    pub hook: Mutex<Option<ReqHook>>,
    hold: Mutex<bool>,
    hold_cv: Condvar,
    pub state_calls: AtomicUsize,
    /// True: do not turn a set stop flag into `Stopped` (a request that finished just before the stop).
    pub ignore_stop: AtomicBool,
}
impl FakeBrowser {
    pub fn new(log: Arc<Log>) -> Arc<Self> {
        Arc::new(FakeBrowser {
            log,
            state: Mutex::new(Box::new(|| LinkState::Connected)),
            script: Mutex::new(VecDeque::new()),
            reqs: Mutex::new(vec![]),
            hook: Mutex::new(None),
            hold: Mutex::new(false),
            hold_cv: Condvar::new(),
            state_calls: AtomicUsize::new(0),
            ignore_stop: AtomicBool::new(false),
        })
    }
    pub fn set_state(&self, f: impl Fn() -> LinkState + Send + Sync + 'static) {
        *self.state.lock().unwrap() = Box::new(f);
    }
    pub fn fixed(&self, s: LinkState) {
        self.set_state(move || s);
    }
    pub fn script(&self, answers: Vec<Answer>) {
        *self.script.lock().unwrap() = answers.into();
    }
    pub fn set_hook(&self, f: impl Fn(usize, &AtomicBool) + Send + Sync + 'static) {
        *self.hook.lock().unwrap() = Some(Box::new(f));
    }
    /// From now on `request` blocks (after logging) until `release`.
    pub fn hold(&self) {
        *self.hold.lock().unwrap() = true;
    }
    pub fn release(&self) {
        *self.hold.lock().unwrap() = false;
        self.hold_cv.notify_all();
    }
    pub fn reqs(&self) -> Vec<(Command, Duration)> {
        self.reqs.lock().unwrap().clone()
    }
    pub fn cmds(&self) -> Vec<Command> {
        self.reqs().into_iter().map(|r| r.0).collect()
    }
}
impl Browser for FakeBrowser {
    fn link_state(&self) -> LinkState {
        self.state_calls.fetch_add(1, Ordering::SeqCst);
        self.log.add("state");
        (self.state.lock().unwrap())()
    }
    fn request(&self, cmd: &Command, wait: Duration, stop: &AtomicBool) -> Answer {
        self.log.add("request");
        let index = {
            let mut r = self.reqs.lock().unwrap();
            r.push((cmd.clone(), wait));
            r.len() - 1
        };
        if let Some(h) = self.hook.lock().unwrap().as_ref() {
            h(index, stop);
        }
        let mut g = self.hold.lock().unwrap();
        let start = std::time::Instant::now();
        while *g && start.elapsed() < Duration::from_secs(5) {
            g = self.hold_cv.wait_timeout(g, Duration::from_millis(50)).unwrap().0;
        }
        drop(g);
        // The real `Link::request` returns Stopped as soon as the flag is set.
        if !self.ignore_stop.load(Ordering::SeqCst) && stop.load(Ordering::SeqCst) {
            return Err(LinkError::Stopped);
        }
        self.script.lock().unwrap().pop_front().unwrap_or(Err(LinkError::Lost))
    }
}

pub struct FakeChrome {
    pub log: Arc<Log>,
    pub result: Mutex<Result<(), FrontKind>>,
    pub fronts: AtomicUsize,
}
#[derive(Clone, Copy)]
pub enum FrontKind {
    NotInstalled,
    Failed,
}
impl FakeChrome {
    pub fn new(log: Arc<Log>) -> Arc<Self> {
        Arc::new(FakeChrome { log, result: Mutex::new(Ok(())), fronts: AtomicUsize::new(0) })
    }
    pub fn fails(&self, k: FrontKind) {
        *self.result.lock().unwrap() = Err(k);
    }
    pub fn fronts(&self) -> usize {
        self.fronts.load(Ordering::SeqCst)
    }
}
impl ChromeFront for FakeChrome {
    fn front(&self) -> Result<(), FrontError> {
        self.fronts.fetch_add(1, Ordering::SeqCst);
        self.log.add("front");
        match *self.result.lock().unwrap() {
            Ok(()) => Ok(()),
            Err(FrontKind::NotInstalled) => Err(FrontError::NotInstalled),
            Err(FrontKind::Failed) => Err(FrontError::Failed),
        }
    }
}

// ---------- the rig ----------
pub struct Shown {
    pub at_ms: u64,
    pub lines: Vec<String>,
}
pub struct Rig {
    pub exec: Arc<Executor>,
    pub log: Arc<Log>,
    pub browser: Arc<FakeBrowser>,
    pub chrome: Arc<FakeChrome>,
    pub media: Arc<FakeMedia>,
    pub launcher: Arc<FakeLauncher>,
    pub catalog: Arc<FakeCatalog>,
    pub wall: Arc<FakeWall>,
    pub clock: Arc<FakeClock>,
    pub sleeps: Arc<Mutex<Vec<Duration>>>,
    /// Called with the index of every sleep (0-based); tests stop the task from here.
    pub sleep_hook: SleepHook,
}
impl Rig {
    pub fn new() -> Self {
        Self::with(cfg(), FakeWall::india())
    }
    pub fn with(cfg: ExecConfig, wall: Arc<FakeWall>) -> Self {
        let log = Arc::new(Log::default());
        let browser = FakeBrowser::new(log.clone());
        let chrome = FakeChrome::new(log.clone());
        let media = Arc::new(FakeMedia::default());
        let launcher = Arc::new(FakeLauncher::default());
        let catalog = FakeCatalog::new(vec![app("Notes", false), app("Adobe Drive", false), app("Google Drive", false)]);
        let clock = FakeClock::new();
        let sleeps: Arc<Mutex<Vec<Duration>>> = Arc::new(Mutex::new(vec![]));
        let sleep_hook: SleepHook = Arc::new(Mutex::new(None));
        let (s2, c2, h2) = (sleeps.clone(), clock.clone(), sleep_hook.clone());
        let sleep: Arc<dyn Fn(Duration) + Send + Sync> = Arc::new(move |d| {
            let index = {
                let mut s = s2.lock().unwrap();
                s.push(d);
                s.len() - 1
            };
            c2.set(elapsed_ms(&c2) + d.as_millis() as u64);
            if let Some(h) = h2.lock().unwrap().as_ref() {
                h(index);
            }
        });
        let exec = Executor::new(
            Effects {
                browser: browser.clone(),
                chrome: chrome.clone(),
                media: media.clone(),
                apps: catalog.clone(),
                launcher: launcher.clone(),
                wall: wall.clone(),
                clock: clock.clone(),
                sleep,
            },
            cfg,
        );
        Rig { exec, log, browser, chrome, media, launcher, catalog, wall, clock, sleeps, sleep_hook }
    }
    /// The task as the seam runs it: lines in, hold seconds out.
    pub fn plan(&self, steps: &[Step]) -> (u64, Vec<Shown>) {
        let shown: Mutex<Vec<Shown>> = Mutex::new(vec![]);
        let clock = self.clock.clone();
        let hold = self.exec.run_plan(steps, &|lines: Vec<Line>| {
            shown.lock().unwrap().push(Shown { at_ms: elapsed_ms(&clock), lines: lines.into_iter().map(|l| l.text).collect() });
        });
        (hold, shown.into_inner().unwrap())
    }
    pub fn run(&self, steps: &[Step]) -> TaskReport {
        self.exec.run(steps)
    }
    pub fn sleeps(&self) -> Vec<Duration> {
        self.sleeps.lock().unwrap().clone()
    }
    pub fn slept_total(&self) -> Duration {
        self.sleeps().iter().sum()
    }
}

/// Milliseconds the fake clock has advanced since it was made.
pub fn elapsed_ms(c: &FakeClock) -> u64 {
    use crate::so::hotkey::watcher::Clock;
    c.now().duration_since(c.base).as_millis() as u64
}

/// The lines of the last show, without a "Heard: ..." line (the sink adds that one).
pub fn last_body(shown: &[Shown]) -> Vec<String> {
    shown.last().map(|s| s.lines.iter().filter(|l| !l.starts_with("Heard: ")).cloned().collect()).unwrap_or_default()
}
pub fn all_lines(shown: &[Shown]) -> Vec<String> {
    shown.iter().flat_map(|s| s.lines.clone()).collect()
}

pub const PAIR_TEXT: &str = "Fix connection: pair the extension in Settings → General → Chrome extension";
pub const NOT_CONNECTED_TEXT: &str = "Fix connection: open Chrome and check the Smooth Operator extension is on";
pub const CONNECTING_TEXT: &str = "Connecting to Chrome…";
pub const BUSY_TEXT: &str = "Still working on the last task";
pub const YT_PLAYING: &str = "Playing on YouTube";
pub const DOCS_TEXT: &str = "Blank Google Doc opened. Writing the text comes in a later update";
pub const CAL_TEXT: &str = "Event filled in, not saved: check it in Chrome";
pub const GUESTS_SENTENCE: &str = "Guests are added in a later update";
pub const NO_VIDEO_TEXT: &str = "No playable video found for that search";
pub const TIMEOUT_TEXT: &str = "Chrome didn't finish loading: try again";
pub const LOGIN_TEXT: &str = "Google needs you in Chrome (sign-in or consent): finish it, then try again";
pub const NO_ACCESS_TEXT: &str = "Allow the Smooth Operator extension on YouTube and Google sites: chrome://extensions → Details → Site access";
pub const BLOCKED_TEXT: &str = "Blocked: handed back to you";
pub const STOPPED_TEXT: &str = "Stopped";
pub const DEADLINE_TEXT: &str = "Took too long: stopped";
pub const FAILED_TEXT: &str = "That step failed: try again";
pub const CHROME_MISSING_TEXT: &str = "Google Chrome not found: install it for web tasks";

// ---------- a SlowRouter that also returns steps, and a PlanRunner fake (seam tests) ----------
use crate::so::rules::{Reply, SlowRouter};
use std::time::Instant;

pub struct PlanBrain {
    pub reply: Mutex<Reply>,
    pub steps: Mutex<Vec<Step>>,
    pub delay: Mutex<Duration>,
    pub think_calls: AtomicUsize,
    pub plan_calls: AtomicUsize,
}
impl PlanBrain {
    pub fn new(reply: Reply, steps: Vec<Step>) -> Arc<Self> {
        Arc::new(PlanBrain {
            reply: Mutex::new(reply),
            steps: Mutex::new(steps),
            delay: Mutex::new(Duration::ZERO),
            think_calls: AtomicUsize::new(0),
            plan_calls: AtomicUsize::new(0),
        })
    }
}
impl SlowRouter for PlanBrain {
    fn wants(&self, text: &str) -> bool {
        text.split_whitespace().count() >= 2
    }
    fn think(&self, _text: &str, _stopped_at: Instant) -> Reply {
        self.think_calls.fetch_add(1, Ordering::SeqCst);
        std::thread::sleep(*self.delay.lock().unwrap());
        self.reply.lock().unwrap().clone()
    }
    fn think_plan(&self, _text: &str, _stopped_at: Instant) -> (Reply, Vec<Step>) {
        self.plan_calls.fetch_add(1, Ordering::SeqCst);
        std::thread::sleep(*self.delay.lock().unwrap());
        (self.reply.lock().unwrap().clone(), self.steps.lock().unwrap().clone())
    }
}

pub struct FakeRunner {
    pub calls: Mutex<Vec<Vec<Step>>>,
    pub lines: Mutex<Vec<Vec<Line>>>,
    pub hold: u64,
}
impl FakeRunner {
    pub fn new(lines: Vec<Vec<Line>>, hold: u64) -> Arc<Self> {
        Arc::new(FakeRunner { calls: Mutex::new(vec![]), lines: Mutex::new(lines), hold })
    }
    pub fn calls(&self) -> Vec<Vec<Step>> {
        self.calls.lock().unwrap().clone()
    }
}
impl PlanRunner for FakeRunner {
    fn run_plan(&self, steps: &[Step], show: &dyn Fn(Vec<Line>)) -> u64 {
        self.calls.lock().unwrap().push(steps.to_vec());
        for l in self.lines.lock().unwrap().clone() {
            show(l);
        }
        self.hold
    }
}
