// SPDX-License-Identifier: FSL-1.1-MIT
//! Gates 1 + 2 + 6: the key watcher.
//! One `handy-keys` listener (a global keyboard hook) feeds a `Detector` per hotkey.
//! - It reads modifier flags and the key identity for comparison only. It never logs, stores
//!   or sends key values; the harness log gets event names such as "hold".
//! - Gate 2: while the hotkey is held, Enter and Esc are in the hook's blocking set, so the
//!   hook swallows their key-down AND key-up. Plain Enter/Esc are not in the set and pass.
//!   Caveat of handy-keys 0.3.4: key-up is judged on the modifiers held AT key-up time. If the
//!   hotkey is released before Enter, that one key-up passes through (shown by keytest.html).
//! - Gate 6 (Windows): the hook runs on its own thread, so a stalled main thread must not stop it.

use super::detector::{is_approve_or_cancel, Detector, Out};
use handy_keys::{Error, Hotkey, Key, KeyboardListener, Modifiers};
use std::collections::HashSet;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tauri::AppHandle;

/// (modifier, name shown in the panel). Mac: Fn/Globe (default) and Right Cmd (backup).
/// Windows: Right Ctrl. Fn is reported by macOS as a modifier flag, so it needs no special path.
#[cfg(target_os = "macos")]
const TARGETS: [(Modifiers, &str); 2] = [(Modifiers::FN, "fn"), (Modifiers::CMD_RIGHT, "rcmd")];
#[cfg(not(target_os = "macos"))]
const TARGETS: [(Modifiers, &str); 1] = [(Modifiers::CTRL_RIGHT, "rctrl")];

const SWALLOWED: [Key; 3] = [Key::Return, Key::KeypadEnter, Key::Escape];

pub fn start(app: AppHandle) {
    let mut set: HashSet<Hotkey> = HashSet::new();
    for (m, _) in TARGETS {
        for k in SWALLOWED {
            if let Ok(h) = Hotkey::new(m, k) {
                set.insert(h);
            }
        }
    }
    let blocking = Arc::new(Mutex::new(set));
    let listener = match KeyboardListener::new_with_blocking(blocking) {
        Ok(l) => l,
        Err(_) => {
            super::harness::event("g1", "listener_failed");
            super::panel::say(&app, "key hook failed: grant Accessibility / Input Monitoring");
            return;
        }
    };
    super::harness::event("g1", "listener_started");

    std::thread::Builder::new()
        .name("spike-keys".into())
        .spawn(move || run(app, listener))
        .ok();
}

fn run(app: AppHandle, listener: KeyboardListener) {
    let mut dets: Vec<(Modifiers, &'static str, Detector)> =
        TARGETS.iter().map(|(m, n)| (*m, *n, Detector::new(*m))).collect();
    loop {
        let ev = match listener.recv_timeout(Duration::from_millis(20)) {
            Ok(ev) => Some(ev),
            Err(Error::Timeout) => None,
            Err(_) => {
                super::harness::event("g1", "listener_stopped");
                return;
            }
        };
        let now = Instant::now();
        for (target, name, det) in dets.iter_mut() {
            let mut outs = Vec::new();
            if let Some(ev) = &ev {
                outs.extend(det.feed(ev, now));
                // Gate 2 bookkeeping: would the hook have blocked this key? Same rule as the hook.
                if is_approve_or_cancel(ev.key) && target.matches(ev.modifiers) {
                    let cancel = ev.key == Some(Key::Escape);
                    let name = match (cancel, ev.is_key_down) {
                        (false, true) => "swallow_approve_down",
                        (false, false) => "swallow_approve_up",
                        (true, true) => "swallow_cancel_down",
                        (true, false) => "swallow_cancel_up",
                    };
                    super::harness::event("g2", name);
                    if ev.is_key_down {
                        super::panel::say(&app, if cancel { "swallowed: cancel key" } else { "swallowed: approve key" });
                    }
                }
            }
            outs.extend(det.tick(now));
            for o in outs {
                match o {
                    Out::Down => {
                        super::harness::event("g1", "down");
                        super::speech::on_down(&app);
                    }
                    Out::Hold => {
                        super::harness::event("g1", "hold");
                        super::panel::say(&app, &format!("hold ({name})"));
                        super::speech::on_hold(&app);
                    }
                    Out::HoldEnd => {
                        super::harness::event("g1", "hold_end");
                        super::panel::say(&app, "released");
                        super::speech::on_release(&app, now);
                    }
                    Out::DoubleTap => {
                        super::harness::event("g1", "double-tap");
                        super::panel::say(&app, &format!("double-tap ({name})"));
                    }
                }
            }
        }
    }
}
