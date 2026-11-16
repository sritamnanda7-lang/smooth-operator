// SPDX-License-Identifier: FSL-1.1-MIT
//! Runs a plan task: risk gate, link state, Chrome to front once, request, retries, stop, deadline. Stubs.

use super::ops::WallClock;
use super::ExecConfig;
use super::StepResult;
use crate::so::brain::plan::Step;
use crate::so::hotkey::watcher::Clock;
use crate::so::link::commands::LinkState;
use crate::so::link::protocol::Command;
use crate::so::link::server::{CmdReply, LinkError};
use crate::so::panel::Line;
use crate::so::rules::apps::AppCatalog;
use crate::so::rules::hands::{AppLauncher, MediaKeys};
use crate::so::rules::PlanRunner;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use std::time::Duration;

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

pub struct Executor;

impl Executor {
    pub fn new(_fx: Effects, _cfg: ExecConfig) -> Arc<Self> {
        todo!("step 6 inc 3")
    }
    /// Blocking. While another task runs: busy = true, steps empty, not_run = steps.len().
    pub fn run(&self, _steps: &[Step]) -> TaskReport {
        todo!("step 6 inc 3")
    }
    /// Step-7 hook: sets the task's stop flag.
    pub fn stop(&self) {
        todo!("step 6 inc 3")
    }
    pub fn busy(&self) -> bool {
        todo!("step 6 inc 3")
    }
}

impl PlanRunner for Executor {
    fn run_plan(&self, _steps: &[Step], _show: &dyn Fn(Vec<Line>)) -> u64 {
        todo!("step 6 inc 3")
    }
}
