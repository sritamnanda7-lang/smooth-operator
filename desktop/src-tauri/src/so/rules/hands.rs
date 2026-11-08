// SPDX-License-Identifier: FSL-1.1-MIT
//! The two effects a fast rule can have, behind traits so tests use fakes.
//! Error strings are never shown or logged.

use super::phrase::MediaKey;
use std::path::Path;

pub trait MediaKeys: Send + Sync {
    fn press(&self, key: MediaKey) -> Result<(), String>;
}

pub trait AppLauncher: Send + Sync {
    fn open(&self, app: &Path) -> Result<(), String>;
}
