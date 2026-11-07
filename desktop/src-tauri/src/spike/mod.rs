// SPDX-License-Identifier: FSL-1.1-MIT
//! Step 0 spike harness. Compiled only with `--features spike` (cargo feature = a switch
//! chosen at build time). Normal builds do not contain any of this code.
//! Gates: 1 hotkey, 2 swallow, 3 panel, 5 update (build side), 6 stall, 7 extension socket,
//! 8 speech. Everything is logged to `spike-harness.log` in the app data dir (see harness.rs).

mod controls;
mod detector;
mod harness;
mod keys;
mod panel;
mod rss;
mod speech;
mod ws;

use tauri::{AppHandle, Manager};

/// Called once from `setup`, after Handy's managers exist.
pub fn init(app: &AppHandle) {
    if let Ok(dir) = app.path().app_data_dir() {
        harness::init(dir);
    }
    harness::event("boot", "spike_init");
    panel::create(app);
    controls::create(app);
    keys::start(app.clone());
    ws::start(app.clone());
    speech::start_idle_watcher(app.clone());
}

/// Gate 6: block the main (UI) thread for 2 s. The key hook runs on its own thread, so its
/// events must keep appearing in the log between `stall_start` and `stall_end`.
#[tauri::command]
fn spike_stall(app: AppHandle) {
    let _ = app.run_on_main_thread(|| {
        harness::event("g6", "stall_start");
        std::thread::sleep(std::time::Duration::from_secs(2));
        harness::event("g6", "stall_end");
    });
}

/// Gate 3 self-test: panel tries `location=` and `window.open`; result shown in the controls.
#[tauri::command]
fn spike_nav_test(app: AppHandle) {
    std::thread::spawn(move || {
        let (nav, win) = panel::navigation_selftest(&app);
        controls::set(&app, &format!("navigation blocked: {nav}, window.open blocked: {win} (want 1 and 1)"));
    });
}

/// Gate 5: check the update endpoint, download, install, relaunch. Only exists in spike builds
/// (the updater plugin is a spike-only dependency). `async` = may wait without blocking the UI.
#[tauri::command]
async fn spike_update(app: AppHandle) {
    use tauri_plugin_updater::UpdaterExt;
    harness::event("g5", "check");
    let found = match app.updater() {
        Ok(u) => u.check().await,
        Err(e) => Err(e),
    };
    match found {
        Ok(Some(update)) => {
            harness::event("g5", "update_found");
            controls::set(&app, &format!("update {} found, installing", update.version));
            match update.download_and_install(|_, _| {}, || {}).await {
                Ok(()) => {
                    harness::event("g5", "installed");
                    app.restart();
                }
                Err(e) => {
                    harness::event("g5", "install_failed");
                    controls::set(&app, &format!("install failed: {e}"));
                }
            }
        }
        Ok(None) => {
            harness::event("g5", "no_update");
            controls::set(&app, "no update available");
        }
        Err(e) => {
            harness::event("g5", "check_failed");
            controls::set(&app, &format!("check failed: {e}"));
        }
    }
}

#[tauri::command]
fn spike_preload(app: AppHandle) {
    speech::preload(&app);
}

#[tauri::command]
fn spike_unload(app: AppHandle) {
    std::thread::spawn(move || speech::unload(&app));
}

#[tauri::command]
fn spike_rss(app: AppHandle) {
    std::thread::spawn(move || {
        let kb = rss::tree_kb();
        harness::with_rss("g8", "rss_manual", None, kb);
        controls::set(&app, &format!("RSS app tree: {} MB", kb.map_or("?".into(), |k| (k / 1024).to_string())));
    });
}

/// Adds our `spike_*` commands next to Handy's. (A Tauri "command" is a Rust function the
/// web page may call; the handler decides which function runs for a given name.)
pub fn wrap_handler<F>(inner: F) -> impl Fn(tauri::ipc::Invoke<tauri::Wry>) -> bool + Send + Sync + 'static
where
    F: Fn(tauri::ipc::Invoke<tauri::Wry>) -> bool + Send + Sync + 'static,
{
    let own: Box<dyn Fn(tauri::ipc::Invoke<tauri::Wry>) -> bool + Send + Sync> =
        Box::new(tauri::generate_handler![spike_stall, spike_nav_test, spike_preload, spike_unload, spike_rss, spike_update]);
    move |invoke| {
        if invoke.message.command().starts_with("spike_") {
            own(invoke)
        } else {
            inner(invoke)
        }
    }
}
