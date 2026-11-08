// SPDX-License-Identifier: FSL-1.1-MIT
//! `FastRouter`: phrase table + app matching + effects. STUB.

use super::apps::AppCatalog;
use super::hands::{AppLauncher, MediaKeys};
use super::{Route, TranscriptRouter};
use crate::so::hotkey::watcher::Clock;
use std::sync::Arc;
use std::time::Instant;

pub struct FastRouter {
    #[allow(dead_code)]
    media: Arc<dyn MediaKeys>,
    #[allow(dead_code)]
    apps: Arc<dyn AppCatalog>,
    #[allow(dead_code)]
    launcher: Arc<dyn AppLauncher>,
    #[allow(dead_code)]
    clock: Arc<dyn Clock>,
}

impl FastRouter {
    pub fn new(
        media: Arc<dyn MediaKeys>,
        apps: Arc<dyn AppCatalog>,
        launcher: Arc<dyn AppLauncher>,
        clock: Arc<dyn Clock>,
    ) -> Self {
        FastRouter { media, apps, launcher, clock }
    }
}

impl TranscriptRouter for FastRouter {
    fn route(&self, _text: &str, _stopped_at: Instant) -> Route {
        todo!("step 3 increments 3-5")
    }
}
