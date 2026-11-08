// SPDX-License-Identifier: FSL-1.1-MIT
//! macOS effects: Enigo media keys, `/usr/bin/open`, folder + NSWorkspace app list. STUBS.
//! Constructor shapes are not in the plan's interface list (see report).

use super::apps::{AppCatalog, AppEntry};
use super::hands::{AppLauncher, MediaKeys};
use super::phrase::MediaKey;
use std::path::Path;

pub struct EnigoMediaKeys {
    #[allow(dead_code)]
    app: tauri::AppHandle,
}

impl EnigoMediaKeys {
    pub fn new(app: tauri::AppHandle) -> Self {
        EnigoMediaKeys { app }
    }
}

impl MediaKeys for EnigoMediaKeys {
    fn press(&self, _key: MediaKey) -> Result<(), String> {
        todo!("step 3 increment 5")
    }
}

pub struct OpenLauncher;

impl AppLauncher for OpenLauncher {
    fn open(&self, _app: &Path) -> Result<(), String> {
        todo!("step 3 increment 4")
    }
}

pub struct SystemCatalog;

impl AppCatalog for SystemCatalog {
    fn list(&self) -> Vec<AppEntry> {
        todo!("step 3 increment 4")
    }
}
