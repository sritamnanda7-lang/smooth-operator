// SPDX-License-Identifier: FSL-1.1-MIT
//! Step 5: the link between this app and the Chrome extension: a local WebSocket server on
//! 127.0.0.1, a pairing code, a Keychain token and a proof handshake. Nothing else travels over
//! it in step 5. Plan: draft/step-5-plan.md.

pub mod commands;
pub mod metrics;
pub mod pairing;
pub mod protocol;
pub mod server;
pub mod token;

use crate::so::hotkey::SystemClock;
use pairing::OsRng;
use server::Link;
use std::sync::Arc;
use std::time::Duration;
use tauri::{AppHandle, Manager};
use token::KeyringTokenStore;

/// The one fixed port (plan decision b). No fallback, no random port.
pub const LINK_PORT: u16 = 47815;

/// The one allowed Origin: the extension ID Chrome printed in spike S0 (from the self-made manifest `key`).
/// The handshake compares the whole string exactly. A Web Store ID would change this one line (step 9).
pub const ALLOWED_ORIGIN: &str = "chrome-extension://annldiclndpcpopgmdmfcdjdeehfdlik";

/// Libraries that must never reach a log: tungstenite writes message text and frame bytes at Trace.
/// `lib.rs` turns each of these Off in both log builders.
pub const LOG_SILENCED: &[&str] = &["tungstenite", "tokio_tungstenite"];

/// Shown in Settings and returned by `so_link_start_pairing` when the port is taken.
pub const PORT_BUSY_TEXT: &str = "Port 47815 is used by another app";

/// Limits in one place so tests can use short timeouts.
pub struct LinkConfig {
    pub port: u16,
    pub allowed_origin: &'static str,
    pub auth_timeout: Duration,
    pub idle_timeout: Duration,
    pub max_msg: usize,
    pub read_buffer: usize,
    pub max_handshakes: usize,
}

pub const LINK: LinkConfig = LinkConfig {
    port: LINK_PORT,
    allowed_origin: ALLOWED_ORIGIN,
    auth_timeout: Duration::from_secs(5),
    idle_timeout: Duration::from_secs(60),
    max_msg: 4096,
    read_buffer: 4096,
    max_handshakes: 4,
};

/// Called once from `so::setup`: builds the shared `Link` (managed state for the commands) and starts
/// listening in the background. A busy port is shown in Settings ("Port busy"), never a crash.
pub fn setup(app: &AppHandle) {
    let cfg = LINK;
    let port = cfg.port;
    let link = Link::new(cfg, Arc::new(KeyringTokenStore::new(token::service())), Arc::new(SystemClock), Arc::new(OsRng));
    app.manage(link.clone());
    tauri::async_runtime::spawn(async move {
        if let Some(listener) = link.bind(port).await {
            link.serve(listener).await;
        }
    });
}
