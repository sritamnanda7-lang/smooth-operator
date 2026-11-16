// SPDX-License-Identifier: FSL-1.1-MIT
//! Step 6: the executor. Runs the safe ops of a plan (open/switch app, media key, YouTube play, open a
//! blank Doc, prefill a calendar event) and reports fixed texts. Stubs: logic arrives in increments 1-4.
//! Plan: draft/step-6-plan.md.

#![allow(dead_code)] // stubs: remove when wired (step 6 inc 4)

pub mod executor;
pub mod metrics;
pub mod ops;
#[cfg(target_os = "macos")]
pub mod real;

use crate::so::brain::plan::OpKind;
use crate::so::link::protocol::ResultCode;
use crate::so::link::PORT_BUSY_TEXT;
use std::time::Duration;

/// Every limit in one place so tests can use short ones.
pub struct ExecConfig {
    pub link_wait: Duration,
    pub connecting_after: Duration,
    pub poll: Duration,
    pub retry_pause: Duration,
    pub max_retries: u8,
    pub yt_wait: Duration,
    pub page_wait: Duration,
    pub task_deadline: Duration,
}

/// yt/page waits are provisional (spike Z7 set them).
pub const EXEC: ExecConfig = ExecConfig {
    link_wait: Duration::from_secs(35),
    connecting_after: Duration::from_secs(1),
    poll: Duration::from_millis(200),
    retry_pause: Duration::from_secs(1),
    max_retries: 3,
    yt_wait: Duration::from_secs(40),
    page_wait: Duration::from_secs(20),
    task_deadline: Duration::from_secs(90),
};

/// The only ops that may run in step 6 (and only when `op_risk` is Safe).
pub const RUNS_NOW: &[OpKind] =
    &[OpKind::AppOpen, OpKind::AppSwitch, OpKind::MediaKey, OpKind::YoutubePlay, OpKind::DocsOpen, OpKind::CalendarOpen];
pub const RESULT_SECS: u64 = 10;
pub const MAX_DAYS_AHEAD: i64 = 400;

/// Outcome of one op.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Res {
    Done,
    Ext(ResultCode),
    ChromeMissing,
    NotPaired,
    NotConnected,
    PortBusy,
    KeychainError,
    LinkTimeout,
    Lost,
    BadResult,
    TooLong,
    PastTime,
    NoSuchTime,
    TooFar,
    AppNotFound,
    AppAmbiguous,
    Failed,
    NotYet,
    Refused,
    Stopped,
    Deadline,
}

impl Res {
    /// Fixed integer for the log (never changes meaning once shipped).
    pub fn code(&self) -> u8 {
        use ResultCode as R;
        match self {
            Res::Done => 1,
            Res::Ext(c) => match c {
                R::Playing => 10,
                R::Ad => 11,
                R::Paused => 12,
                R::NoVideo => 13,
                R::EditorOpen => 14,
                R::NeedsLogin => 15,
                R::Consent => 16,
                R::NoAccess => 17,
                R::Timeout => 18,
                R::BadCmd => 19,
                R::Busy => 20,
                R::Unsafe => 21,
                R::Cancelled => 22,
                R::Error => 23,
            },
            Res::ChromeMissing => 30,
            Res::NotPaired => 31,
            Res::NotConnected => 32,
            Res::PortBusy => 33,
            Res::KeychainError => 34,
            Res::LinkTimeout => 35,
            Res::Lost => 36,
            Res::BadResult => 37,
            Res::TooLong => 38,
            Res::PastTime => 39,
            Res::NoSuchTime => 40,
            Res::TooFar => 41,
            Res::AppNotFound => 42,
            Res::AppAmbiguous => 43,
            Res::Failed => 44,
            Res::NotYet => 45,
            Res::Refused => 46,
            Res::Stopped => 47,
            Res::Deadline => 48,
        }
    }
    /// True: Done, Ext(Playing | Ad | Paused | EditorOpen).
    pub fn ok(&self) -> bool {
        matches!(self, Res::Done | Res::Ext(ResultCode::Playing | ResultCode::Ad | ResultCode::Paused | ResultCode::EditorOpen))
    }
}

/// What is left after a step's first op: nothing, a safe op not built yet, or an op that needs approval.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tail {
    None,
    Deferred,
    NeedsOk,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StepResult {
    pub first: OpKind,
    pub res: Res,
    pub tail: Tail,
}

pub const PAIR_TEXT: &str = "Fix connection: pair the extension in Settings → General → Chrome extension";
pub const NOT_CONNECTED_TEXT: &str = "Fix connection: open Chrome and check the Smooth Operator extension is on";
pub const CONNECTING_TEXT: &str = "Connecting to Chrome…";
pub const BUSY_TEXT: &str = "Still working on the last task";
const KEYCHAIN_TEXT: &str = "Can't read the Chrome link key from the Keychain. Unlock your Mac and try again.";
const DOCS_TEXT: &str = "Blank Google Doc opened. Writing the text comes in a later update";
const CAL_TEXT: &str = "Event filled in, not saved: check it in Chrome";
const GUESTS_SENTENCE: &str = "Guests are added in a later update";
const LOGIN_TEXT: &str = "Google needs you in Chrome (sign-in or consent): finish it, then try again";
const NO_ACCESS_TEXT: &str =
    "Allow the Smooth Operator extension on YouTube and Google sites: chrome://extensions → Details → Site access";
const FAILED_TEXT: &str = "That step failed: try again";

/// The fixed panel text for a step result. Never contains a URL, page text or extension string.
/// (App names and the "Not available yet" step line are added by the executor, which knows the action.)
pub fn result_text(s: &StepResult, guests_named: bool) -> String {
    use ResultCode as R;
    let t = match s.res {
        Res::Done => "Done",
        Res::Ext(R::Playing) => "Playing on YouTube",
        Res::Ext(R::Ad) => "An ad is playing; your video starts after it",
        Res::Ext(R::Paused) => "Video opened but paused: press play in Chrome",
        Res::Ext(R::NoVideo) => "No playable video found for that search",
        Res::Ext(R::EditorOpen) => {
            if s.first == OpKind::CalendarOpen {
                return if guests_named { format!("{CAL_TEXT}. {GUESTS_SENTENCE}") } else { CAL_TEXT.to_string() };
            }
            DOCS_TEXT
        }
        Res::Ext(R::NeedsLogin | R::Consent) => LOGIN_TEXT,
        Res::Ext(R::NoAccess) => NO_ACCESS_TEXT,
        Res::Ext(R::Timeout) => "Chrome didn't finish loading: try again",
        Res::NotYet => "Not available yet",
        Res::Refused => "Blocked: handed back to you",
        Res::ChromeMissing => "Google Chrome not found: install it for web tasks",
        Res::NotPaired => PAIR_TEXT,
        Res::NotConnected => NOT_CONNECTED_TEXT,
        Res::PortBusy => PORT_BUSY_TEXT,
        Res::KeychainError => KEYCHAIN_TEXT,
        Res::PastTime => "That time has already passed",
        Res::NoSuchTime => "That time doesn't exist (clock change)",
        Res::TooFar => "That date is too far ahead",
        Res::AppNotFound => "I couldn't find that app",
        Res::AppAmbiguous => "More than one app matches: say the full name",
        Res::Stopped => "Stopped",
        Res::Deadline => "Took too long: stopped",
        Res::Ext(R::BadCmd | R::Busy | R::Unsafe | R::Cancelled | R::Error)
        | Res::LinkTimeout
        | Res::Lost
        | Res::BadResult
        | Res::TooLong
        | Res::Failed => FAILED_TEXT,
    };
    t.to_string()
}
