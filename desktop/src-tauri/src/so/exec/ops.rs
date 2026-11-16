// SPDX-License-Identifier: FSL-1.1-MIT
//! Builds the command for an op: URLs in Rust (percent-encoded by the `url` crate), calendar time in UTC.
//! Stubs.

use super::Res;
use crate::so::brain::plan::{Action, OpKind};
use crate::so::link::protocol::Command;
use chrono::{DateTime, LocalResult, NaiveDateTime, Utc};

/// Injected zone/time so tests can script a zone and DST.
pub trait WallClock: Send + Sync {
    fn now_local(&self) -> NaiveDateTime;
    fn to_utc(&self, local: NaiveDateTime) -> LocalResult<DateTime<Utc>>;
}

pub fn yt_url(_query: &str) -> String {
    todo!("step 6 inc 1")
}

/// Errors: PastTime, NoSuchTime, TooFar.
pub fn cal_url(_title: &str, _date: &str, _time: &str, _minutes: Option<u16>, _wall: &dyn WallClock) -> Result<String, Res> {
    todo!("step 6 inc 1")
}

pub fn command_for(_op: OpKind, _a: &Action, _wall: &dyn WallClock) -> Result<Command, Res> {
    todo!("step 6 inc 1")
}
