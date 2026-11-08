// SPDX-License-Identifier: FSL-1.1-MIT
//! macOS effects: Enigo media keys, `/usr/bin/open`, folder + NSWorkspace app list.

use super::apps::{AppCatalog, AppEntry, FsAppCatalog};
use super::hands::{AppLauncher, MediaKeys};
use super::phrase::MediaKey;
use crate::input::EnigoState;
use enigo::{Direction, Key, Keyboard};
use objc2_app_kit::{NSApplicationActivationPolicy, NSWorkspace};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc};
use tauri::Manager;
use std::time::{Duration, Instant};

/// Media keys through Handy's shared `Enigo` (keyboard-event library). Enigo must run on the app's
/// main thread on macOS, so the key press is sent there (`run_on_main_thread`) and we wait for the
/// result. The same 20-line pattern as Handy's private `clipboard::with_enigo`, copied here so no
/// Handy file changes.
pub struct EnigoMediaKeys {
    app: tauri::AppHandle,
}

impl EnigoMediaKeys {
    pub fn new(app: tauri::AppHandle) -> Self {
        EnigoMediaKeys { app }
    }
}

impl MediaKeys for EnigoMediaKeys {
    fn press(&self, key: MediaKey) -> Result<(), String> {
        // Not set up yet (e.g. Accessibility was granted after launch): Handy's own setup call.
        if self.app.try_state::<EnigoState>().is_none() {
            crate::commands::initialize_enigo(self.app.clone())?;
        }
        let key = match key {
            MediaKey::PlayPause => Key::MediaPlayPause,
            MediaKey::Next => Key::MediaNextTrack,
            MediaKey::Previous => Key::MediaPrevTrack,
        };
        let (tx, rx) = mpsc::sync_channel(1);
        let app = self.app.clone();
        // Set when we give up waiting: a request still queued on the main thread then sends nothing, so
        // a key never fires after the panel already said it failed.
        let cancelled = Arc::new(AtomicBool::new(false));
        let flag = cancelled.clone();
        self.app
            .run_on_main_thread(move || {
                let result = (|| {
                    if flag.load(Ordering::SeqCst) {
                        return Err("cancelled".to_string());
                    }
                    let state = app.try_state::<EnigoState>().ok_or("Enigo state not initialized")?;
                    let mut enigo = state.0.lock().map_err(|e| e.to_string())?;
                    enigo.key(key, Direction::Click).map_err(|e| e.to_string())
                })();
                let _ = tx.send(result);
            })
            .map_err(|e| e.to_string())?;
        // A dropped closure (event loop shutting down) closes the channel.
        match rx.recv_timeout(Duration::from_secs(2)) {
            Ok(result) => result,
            Err(e) => {
                cancelled.store(true, Ordering::SeqCst);
                Err(e.to_string())
            }
        }
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
        // A relative path could be read by `open` as an option or a name: only absolute paths go on.
        if !app.is_absolute() {
            return Err("not an absolute path".into());
        }
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
        if let Some(home) = std::env::var_os("HOME").map(PathBuf::from).filter(|h| h.is_absolute()) {
            dirs.push(home.join("Applications"));
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
