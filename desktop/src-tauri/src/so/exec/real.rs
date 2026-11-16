// SPDX-License-Identifier: FSL-1.1-MIT
//! macOS effects for the executor: the real link, Chrome via `OpenLauncher`, the local zone. Stubs.

use super::executor::{Browser, ChromeFront, FrontError};
use super::ops::WallClock;
use crate::so::link::commands::LinkState;
use crate::so::link::protocol::Command;
use crate::so::link::server::{CmdReply, Link, LinkError};
use chrono::{DateTime, LocalResult, NaiveDateTime, Utc};
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use std::time::Duration;

pub struct LinkBrowser(pub Arc<Link>);

impl Browser for LinkBrowser {
    fn link_state(&self) -> LinkState {
        todo!("step 6 inc 4")
    }
    fn request(&self, _cmd: &Command, _wait: Duration, _stop: &AtomicBool) -> Result<CmdReply, LinkError> {
        todo!("step 6 inc 4")
    }
}

/// Finds "Google Chrome" in `SystemCatalog` and opens it with `OpenLauncher`.
pub struct ChromeOpener;

impl ChromeFront for ChromeOpener {
    fn front(&self) -> Result<(), FrontError> {
        todo!("step 6 inc 4")
    }
}

pub struct LocalWall;

impl WallClock for LocalWall {
    fn now_local(&self) -> NaiveDateTime {
        todo!("step 6 inc 4")
    }
    fn to_utc(&self, _local: NaiveDateTime) -> LocalResult<DateTime<Utc>> {
        todo!("step 6 inc 4")
    }
}
