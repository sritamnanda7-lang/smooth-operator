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
    /// Fixed integer for the log (stub).
    pub fn code(&self) -> u8 {
        todo!("step 6 inc 3")
    }
    /// True: Done, Ext(Playing | Ad | Paused | EditorOpen) (stub).
    pub fn ok(&self) -> bool {
        todo!("step 6 inc 3")
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

/// The fixed panel text for a step result (stub).
pub fn result_text(_s: &StepResult, _guests_named: bool) -> String {
    todo!("step 6 inc 3")
}
