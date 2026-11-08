// SPDX-License-Identifier: FSL-1.1-MIT
//! macOS effects: Enigo media keys, `/usr/bin/open`, folder + NSWorkspace app list.

use super::apps::{AppCatalog, AppEntry, FsAppCatalog};
use super::hands::{AppLauncher, MediaKeys};
use super::phrase::MediaKey;
use objc2_app_kit::{NSApplicationActivationPolicy, NSWorkspace};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::{Duration, Instant};

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

/// Opens an app with `/usr/bin/open <absolute .app path>`. `std::process::Command` runs the program
/// directly, with no command interpreter, so spoken words can never become a command: the path always
/// comes from the installed-app list. macOS launches a closed app or brings a running one to the front.
pub struct OpenLauncher;

/// How long we wait for `open` to report back.
const OPEN_WAIT: Duration = Duration::from_secs(2);

impl AppLauncher for OpenLauncher {
    fn open(&self, app: &Path) -> Result<(), String> {
        let mut child = std::process::Command::new("/usr/bin/open")
            .arg(app)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| e.to_string())?;
        let start = Instant::now();
        loop {
            match child.try_wait() {
                Ok(Some(status)) if status.success() => return Ok(()),
                Ok(Some(_)) => return Err("open failed".into()),
                Ok(None) if start.elapsed() < OPEN_WAIT => std::thread::sleep(Duration::from_millis(10)),
                Ok(None) => {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err("open timed out".into());
                }
                Err(e) => return Err(e.to_string()),
            }
        }
    }
}

/// The installed-app list for the real Mac: the standard folders, then regular running apps from
/// `NSWorkspace` (marked `running`). Built fresh at each command: no cache, no background watcher.
pub struct SystemCatalog;

fn running_apps() -> Vec<(String, PathBuf)> {
    NSWorkspace::sharedWorkspace()
        .runningApplications()
        .iter()
        .filter(|a| a.activationPolicy() == NSApplicationActivationPolicy::Regular)
        .filter_map(|a| {
            let path = PathBuf::from(a.bundleURL()?.path()?.to_string());
            let name = path.file_name()?.to_str()?.strip_suffix(".app")?.to_string();
            Some((name, path))
        })
        .collect()
}

impl AppCatalog for SystemCatalog {
    fn list(&self) -> Vec<AppEntry> {
        let mut dirs: Vec<PathBuf> = ["/Applications", "/Applications/Utilities", "/System/Applications", "/System/Applications/Utilities"]
            .iter()
            .map(PathBuf::from)
            .collect();
        if let Some(home) = std::env::var_os("HOME") {
            dirs.push(PathBuf::from(home).join("Applications"));
        }
        let mut list = FsAppCatalog::new(dirs).list();
        for (name, path) in running_apps() {
            match list.iter_mut().find(|e| e.name == name) {
                Some(e) => e.running = true,
                None => list.push(AppEntry { name, path, running: true }),
            }
        }
        list
    }
}
