// SPDX-License-Identifier: FSL-1.1-MIT
//! The pipeline: limits, key read, request, retry, parse, risk, render. Implements `SlowRouter`.
//! Every outcome is logged as integers only (`metrics.rs`); panel texts are fixed (no provider text).

use super::config::valid_model;
use super::http::{with_retry, RETRY};
use super::keys::{ApiKey, KeyStore};
use super::metrics;
use super::plan::{self, action_code, BrainOutput, Reject};
use super::prompt::{self, MAX_TRANSCRIPT_CHARS};
use super::provider::{Provider, ProviderError, ProviderId, PROVIDERS};
use crate::so::hotkey::watcher::Clock;
use crate::so::panel::{Kind, Line};
use crate::so::rules::{Reply, SlowRouter};
use std::collections::VecDeque;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

pub struct BrainChoice {
    pub provider: ProviderId,
    pub model: String,
}

pub struct Limits {
    pub max_in_flight: u32,
    pub max_per_hour: u32,
}

pub const MIN_WORDS: usize = 2;
pub const PLAN_SECS: u64 = 12;
pub const ERROR_SECS: u64 = 6;

const HOUR: Duration = Duration::from_secs(3600);

const TOO_LONG_TEXT: &str = "Too long: say a shorter command";
const NO_KEY_TEXT: &str = "AI not set up: Settings → General → AI";
const OFFLINE_TEXT: &str = "No internet: the AI can't be reached";
const TIMEOUT_TEXT: &str = "The AI took too long: try again";
const BAD_REPLY_TEXT: &str = "The AI's reply wasn't usable: try again";
const HOUR_LIMIT_TEXT: &str = "AI limit for this hour reached";
const IN_FLIGHT_TEXT: &str = "Still thinking about the last one";
const KEYCHAIN_TEXT: &str = "Can't read the AI key from Keychain";
const FAILED_TEXT: &str = "AI request failed: model or settings problem";

/// Calls in flight and the start times of calls in the last hour (the spend guard).
struct State {
    in_flight: u32,
    starts: VecDeque<Instant>,
}

pub struct Brain {
    providers: Vec<Arc<dyn Provider>>,
    keys: Arc<dyn KeyStore>,
    choice: Arc<dyn Fn() -> BrainChoice + Send + Sync>,
    clock: Arc<dyn Clock>,
    sleep: Arc<dyn Fn(Duration) + Send + Sync>,
    limits: Limits,
    state: Mutex<State>,
}

/// Gives the in-flight slot back when dropped, also when the call panics.
struct Slot<'a>(&'a Mutex<State>);

impl Drop for Slot<'_> {
    fn drop(&mut self) {
        let mut s = self.0.lock().unwrap_or_else(|e| e.into_inner());
        s.in_flight = s.in_flight.saturating_sub(1);
    }
}

/// What one call ended in, before it is turned into a panel reply and a log line.
struct Outcome {
    code: u8,
    lines: Vec<Line>,
    hold: u64,
    steps: u8,
    http: u16,
    reject: u8,
    actions: Vec<u8>,
}

impl Outcome {
    fn warn(code: u8, text: &str) -> Outcome {
        Outcome { code, lines: vec![Line::new(text, Kind::Warn)], hold: ERROR_SECS, steps: 0, http: 0, reject: 0, actions: Vec::new() }
    }
}

fn provider_code(p: ProviderId) -> u8 {
    match p {
        ProviderId::Claude => 1,
        ProviderId::Gemini => 2,
    }
}

fn label(p: ProviderId) -> &'static str {
    PROVIDERS.iter().find(|i| i.id == p).map_or("The AI", |i| i.label)
}

impl Brain {
    pub fn new(
        providers: Vec<Arc<dyn Provider>>,
        keys: Arc<dyn KeyStore>,
        choice: Arc<dyn Fn() -> BrainChoice + Send + Sync>,
        clock: Arc<dyn Clock>,
        sleep: Arc<dyn Fn(Duration) + Send + Sync>,
        limits: Limits,
    ) -> Self {
        Brain { providers, keys, choice, clock, sleep, limits, state: Mutex::new(State { in_flight: 0, starts: VecDeque::new() }) }
    }

    /// Checks both limits and, if there is room, takes a slot and records the start time.
    fn admit(&self) -> Result<Slot<'_>, Outcome> {
        let now = self.clock.now();
        let mut s = self.state.lock().unwrap_or_else(|e| e.into_inner());
        while s.starts.front().is_some_and(|t| now.saturating_duration_since(*t) >= HOUR) {
            s.starts.pop_front();
        }
        if s.in_flight >= self.limits.max_in_flight {
            return Err(Outcome::warn(11, IN_FLIGHT_TEXT));
        }
        if s.starts.len() >= self.limits.max_per_hour as usize {
            return Err(Outcome::warn(11, HOUR_LIMIT_TEXT));
        }
        s.in_flight += 1;
        s.starts.push_back(now);
        Ok(Slot(&self.state))
    }

    /// Steps 3 to 7 of the pipeline for one admitted call.
    fn run(&self, text: &str) -> (Outcome, u8) {
        let choice = (self.choice)();
        let p = choice.provider;
        let pcode = provider_code(p);
        let default_model = PROVIDERS.iter().find(|i| i.id == p).map_or("", |i| i.default_model);
        let model = if valid_model(&choice.model) { choice.model.as_str() } else { default_model };
        let secret: ApiKey = match self.keys.get(p) {
            Ok(Some(k)) => k,
            Ok(None) => return (Outcome::warn(3, NO_KEY_TEXT), pcode),
            Err(_) => return (Outcome::warn(13, KEYCHAIN_TEXT), pcode),
        };
        let Some(provider) = self.providers.iter().find(|x| x.id() == p) else {
            return (Outcome::warn(14, FAILED_TEXT), pcode);
        };
        let req = prompt::request(text);
        let asked = with_retry(&RETRY, &*self.sleep, || provider.ask(&req, model, &secret));
        drop(secret);
        let raw = match asked {
            Ok(raw) => raw,
            Err(e) => return (self.error_outcome(p, &e), pcode),
        };
        (self.reply_outcome(&raw), pcode)
    }

    fn error_outcome(&self, p: ProviderId, e: &ProviderError) -> Outcome {
        let name = label(p);
        let mut o = match e {
            ProviderError::Offline => Outcome::warn(4, OFFLINE_TEXT),
            ProviderError::Timeout => Outcome::warn(5, TIMEOUT_TEXT),
            ProviderError::KeyInvalid => Outcome::warn(6, &format!("{name} rejected the key: check Settings → General → AI")),
            ProviderError::RateLimited { .. } => Outcome::warn(7, &format!("{name} is busy: try again in a minute")),
            ProviderError::Server(_) => Outcome::warn(8, &format!("{name} is having problems: try again later")),
            ProviderError::BadReply | ProviderError::TooLarge => Outcome::warn(9, BAD_REPLY_TEXT),
            ProviderError::SpendLimit => Outcome::warn(15, &format!("{name} spending limit reached: raise it in the {name} console")),
            ProviderError::NotFound | ProviderError::Forbidden | ProviderError::BadRequest(_) => Outcome::warn(14, FAILED_TEXT),
        };
        o.http = match e {
            ProviderError::Server(s) | ProviderError::BadRequest(s) => *s,
            _ => 0,
        };
        o
    }

    /// Parses and checks the reply; the code (never the model) sets risk and the panel lines.
    fn reply_outcome(&self, raw: &str) -> Outcome {
        match plan::parse(raw) {
            Err(r) => bad_reply(r),
            Ok(out) => {
                let (code, steps, actions) = match &out {
                    BrainOutput::Plan(steps) => (1, steps.len().min(255) as u8, steps.iter().map(|s| action_code(&s.action)).collect()),
                    BrainOutput::Question(_) => (2, 0, Vec::new()),
                };
                Outcome { code, lines: plan::render(&out), hold: PLAN_SECS, steps, http: 0, reject: 0, actions }
            }
        }
    }
}

fn bad_reply(r: Reject) -> Outcome {
    let mut o = Outcome::warn(9, BAD_REPLY_TEXT);
    o.reject = r.code();
    o
}

impl SlowRouter for Brain {
    /// Two or more words after normalising: a word needs a letter or digit ("..." is none), and "please"
    /// is politeness, not a word ("pause please" is still one word, a fast rule's job).
    fn wants(&self, text: &str) -> bool {
        let words = text.split_whitespace().filter(|w| w.chars().any(char::is_alphanumeric));
        words.filter(|w| !w.trim_matches(|c: char| !c.is_alphanumeric()).eq_ignore_ascii_case("please")).count() >= MIN_WORDS
    }

    fn think(&self, text: &str, stopped_at: Instant) -> Reply {
        let ms = |clock: &Arc<dyn Clock>| clock.now().saturating_duration_since(stopped_at).as_millis() as u64;
        if text.chars().count() > MAX_TRANSCRIPT_CHARS {
            metrics::brain(10, 0, ms(&self.clock), 0, 0, 0, &[]);
            return Reply { lines: vec![Line::new(TOO_LONG_TEXT, Kind::Info)], hold_secs: ERROR_SECS };
        }
        let slot = match self.admit() {
            Ok(slot) => slot,
            Err(o) => {
                metrics::brain(o.code, 0, ms(&self.clock), 0, 0, 0, &[]);
                return Reply { lines: o.lines, hold_secs: o.hold };
            }
        };
        // A panic anywhere in the call is caught: the talk shows one fixed line, the slot is freed.
        let (o, pcode) = catch_unwind(AssertUnwindSafe(|| self.run(text)))
            .unwrap_or_else(|_| (Outcome::warn(14, FAILED_TEXT), 0));
        drop(slot);
        metrics::brain(o.code, pcode, ms(&self.clock), o.steps, o.http, o.reject, &o.actions);
        Reply { lines: o.lines, hold_secs: o.hold }
    }
}
