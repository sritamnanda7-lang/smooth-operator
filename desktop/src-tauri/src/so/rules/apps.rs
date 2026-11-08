// SPDX-License-Identifier: FSL-1.1-MIT
//! Installed-app list and name resolution. STUBS.

use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppEntry {
    pub name: String,
    pub path: PathBuf,
    pub running: bool,
}

/// `Ambiguous` holds at most 3 names, sorted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Resolve {
    Found(AppEntry),
    NotFound,
    Ambiguous(Vec<String>),
}

pub trait AppCatalog: Send + Sync {
    fn list(&self) -> Vec<AppEntry>;
}

/// (alias, app name). Filled in increment 4.
pub const ALIASES: &[(&str, &str)] = &[];
/// Leading vendor words that may be dropped. Filled in increment 4.
pub const VENDORS: &[&str] = &[];

pub fn resolve(_x: &str, _apps: &[AppEntry]) -> Resolve {
    todo!("step 3 increment 4")
}

/// Control chars removed, at most 40 chars plus "…".
pub fn shown_name(_x: &str) -> String {
    todo!("step 3 increment 4")
}

/// Scans folders for `.app` bundles.
pub struct FsAppCatalog {
    #[allow(dead_code)]
    dirs: Vec<PathBuf>,
}

impl FsAppCatalog {
    pub fn new(dirs: Vec<PathBuf>) -> Self {
        FsAppCatalog { dirs }
    }
}

impl AppCatalog for FsAppCatalog {
    fn list(&self) -> Vec<AppEntry> {
        todo!("step 3 increment 4")
    }
}
