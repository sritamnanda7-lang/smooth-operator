// SPDX-License-Identifier: FSL-1.1-MIT
//! `FastRouter`: phrase table + app matching + effects. Every outcome is logged as an integer code
//! with the milliseconds since the talk ended; never the text, an app name or a path.

use super::apps::{resolve, shown_name, AppCatalog, Resolve};
use super::metrics;
use super::phrase::{match_command, FastCommand, MediaKey};
use crate::so::panel::{Kind, Line};
use super::hands::{AppLauncher, MediaKeys};
use super::{Route, TranscriptRouter};
use crate::so::hotkey::watcher::Clock;
use std::sync::Arc;
use std::time::Instant;

pub const PLAY_PAUSE_TEXT: &str = "Play/pause sent";
pub const NEXT_TEXT: &str = "Next track";
pub const PREVIOUS_TEXT: &str = "Previous track";
pub const NO_ACCESSIBILITY_TEXT: &str = "Media keys need Accessibility: System Settings → Privacy & Security → Accessibility";

pub struct FastRouter {
    media: Arc<dyn MediaKeys>,
    apps: Arc<dyn AppCatalog>,
    launcher: Arc<dyn AppLauncher>,
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

    /// Logs the outcome code and the time from the end of the talk, and returns the panel line.
    fn done(&self, outcome: u8, stopped_at: Instant, line: Line) -> Route {
        let ms = self.clock.now().saturating_duration_since(stopped_at).as_millis() as u64;
        metrics::fast_rule(outcome, ms);
        Route::Handled(vec![line])
    }

    fn media(&self, key: MediaKey, stopped_at: Instant) -> Route {
        let text = match key {
            MediaKey::PlayPause => PLAY_PAUSE_TEXT,
            MediaKey::Next => NEXT_TEXT,
            MediaKey::Previous => PREVIOUS_TEXT,
        };
        match self.media.press(key) {
            Ok(()) => self.done(1, stopped_at, Line::new(text, Kind::Info)),
            Err(_) => self.done(6, stopped_at, Line::new(NO_ACCESSIBILITY_TEXT, Kind::Warn)),
        }
    }

    /// `open X` or `switch to X`: the app comes from the installed list, never from spoken text.
    fn app(&self, x: &str, switch: bool, stopped_at: Instant) -> Route {
        let shown = shown_name(x);
        let (found, ambiguous, failed) = if switch { (3, 5, 6) } else { (2, 5, 6) };
        match resolve(x, &self.apps.list()) {
            Resolve::Found(app) => {
                // Plan table: "Switching to X" for every switch (also an app that is not running yet, which
                // is then launched: Q2 option A; test F7 expects this wording).
                let verb = if switch { "Switching to" } else { "Opening" };
                match self.launcher.open(&app.path) {
                    Ok(()) => self.done(found, stopped_at, Line::new(format!("{verb} {shown}"), Kind::Info)),
                    Err(_) => self.done(failed, stopped_at, Line::new(format!("Couldn't open {shown}"), Kind::Warn)),
                }
            }
            Resolve::NotFound => {
                self.done(4, stopped_at, Line::new(format!("I couldn't find an app called {shown}"), Kind::Info))
            }
            Resolve::Ambiguous(names) => {
                let names: Vec<String> = names.iter().map(|n| shown_name(n)).collect();
                let text = format!("More than one app matches {shown}: {}. Say the full name.", names.join(", "));
                self.done(ambiguous, stopped_at, Line::new(text, Kind::Info))
            }
        }
    }
}

impl TranscriptRouter for FastRouter {
    fn route(&self, text: &str, stopped_at: Instant) -> Route {
        match match_command(text) {
            Some(FastCommand::Media(key)) => self.media(key, stopped_at),
            Some(FastCommand::Open(x)) => self.app(&x, false, stopped_at),
            Some(FastCommand::SwitchTo(x)) => self.app(&x, true, stopped_at),
            None => {
                let ms = self.clock.now().saturating_duration_since(stopped_at).as_millis() as u64;
                metrics::fast_rule(0, ms);
                Route::PassThrough
            }
        }
    }
}
