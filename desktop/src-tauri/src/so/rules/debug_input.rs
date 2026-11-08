// SPDX-License-Identifier: FSL-1.1-MIT
//! Debug-only typed-phrase input over a Unix socket (module declared under
//! `#[cfg(debug_assertions)]`). STUB.

use std::path::Path;
use std::sync::Arc;

pub const ENV_FLAG: &str = "SO_DEBUG_INPUT";
pub const SOCKET_DIR: &str = "so-debug";
pub const SOCKET_NAME: &str = "input.sock";
pub const MAX_LINE_BYTES: usize = 200;

/// `None` if `!enabled` or bind fails.
pub fn start(
    _data_dir: &Path,
    _enabled: bool,
    _inject: Arc<dyn Fn(String) + Send + Sync>,
) -> Option<std::thread::JoinHandle<()>> {
    todo!("step 3 increment 2")
}
