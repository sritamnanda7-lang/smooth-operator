// SPDX-License-Identifier: FSL-1.1-MIT
//! macOS effects for the executor: the real link, Chrome via `OpenLauncher`, the local zone, and the
//! one place the `Executor` is built (called from `stt::setup`).

use super::executor::{Browser, ChromeFront, Effects, Executor, FrontError};
use super::ops::WallClock;
use super::{register, EXEC};
use crate::so::hotkey::SystemClock;
use crate::so::link::commands::LinkState;
use crate::so::link::protocol::Command;
use crate::so::link::server::{CmdReply, Link, LinkError};
use crate::so::rules::apps::AppCatalog;
use crate::so::rules::hands::AppLauncher;
use crate::so::rules::real::{EnigoMediaKeys, OpenLauncher, SystemCatalog};
use chrono::{DateTime, Local, LocalResult, NaiveDateTime, TimeZone, Utc};
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use std::time::Duration;
use tauri::{AppHandle, Manager};

/// The real link: both calls go straight to `Link` (`request` blocks, so the executor runs on a plain thread).
pub struct LinkBrowser(pub Arc<Link>);

impl Browser for LinkBrowser {
    fn link_state(&self) -> LinkState {
        self.0.settle_token_state();
        self.0.state()
    }
    fn request(&self, cmd: &Command, wait: Duration, stop: &AtomicBool) -> Result<CmdReply, LinkError> {
        self.0.request(cmd, wait, stop)
    }
}

/// Finds "Google Chrome" in `SystemCatalog` and opens it with `OpenLauncher` (starts it, or fronts it).
pub struct ChromeOpener;

impl ChromeFront for ChromeOpener {
    fn front(&self) -> Result<(), FrontError> {
        let chrome = SystemCatalog.list().into_iter().find(|a| a.name.eq_ignore_ascii_case("Google Chrome"));
        let Some(chrome) = chrome else { return Err(FrontError::NotInstalled) };
        OpenLauncher.open(&chrome.path).map_err(|_| FrontError::Failed)
    }
}

/// The Mac's own time zone, DST included (chrono reads it from the system).
pub struct LocalWall;

impl WallClock for LocalWall {
    fn now_local(&self) -> NaiveDateTime {
        Local::now().naive_local()
    }
    fn to_utc(&self, local: NaiveDateTime) -> LocalResult<DateTime<Utc>> {
        Local.from_local_datetime(&local).map(|d| d.with_timezone(&Utc))
    }
}

/// Builds the one executor with the real effects and registers it so quitting can stop its task.
/// `None` if the link was not set up (the app then keeps step 4's behaviour).
pub fn build(app: &AppHandle) -> Option<Arc<Executor>> {
    let link = app.try_state::<Arc<Link>>()?.inner().clone();
    let fx = Effects {
        browser: Arc::new(LinkBrowser(link)),
        chrome: Arc::new(ChromeOpener),
        media: Arc::new(EnigoMediaKeys::new(app.clone())),
        apps: Arc::new(SystemCatalog),
        launcher: Arc::new(OpenLauncher),
        wall: Arc::new(LocalWall),
        clock: Arc::new(SystemClock),
        sleep: Arc::new(std::thread::sleep),
    };
    let ex = Executor::new(fx, EXEC);
    register(ex.clone());
    Some(ex)
}
