// SPDX-License-Identifier: FSL-1.1-MIT
//! Debug-only typed-phrase input over a Unix socket (module declared under
//! `#[cfg(debug_assertions)]`, so a release build never contains it). Plan: draft/step-3-plan.md.
//!
//! A Unix socket is a local "pipe file": only programs on this Mac can open it, and only when the file
//! permissions allow. The folder is 0700 and the socket 0600; each connection must come from the same
//! user (`getpeereid`). No network, no webview path, no Tauri command. The phrase stays in memory:
//! nothing is written to disk and nothing but integer codes is logged.
//!
//! Protocol: one connection = one UTF-8 line of at most `MAX_LINE_BYTES` bytes ending in a newline,
//! read within 2 s. Bad input gets `err\n` and nothing is routed. Good input gets `ok\n` (never an echo).

use super::metrics;
use std::io::{Read, Write};
use std::os::unix::fs::{DirBuilderExt, FileTypeExt, MetadataExt, PermissionsExt};
use std::os::unix::io::AsRawFd;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

pub const ENV_FLAG: &str = "SO_DEBUG_INPUT";
pub const SOCKET_DIR: &str = "so-debug";
pub const SOCKET_NAME: &str = "input.sock";
pub const MAX_LINE_BYTES: usize = 200;

/// A client has this long to send its line.
const READ_WINDOW: Duration = Duration::from_secs(2);

// `reject` codes, as in `metrics::debug_input`.
const TOO_LONG: u8 = 1;
const BAD_UTF8: u8 = 2;
const EMPTY: u8 = 3;
const WRONG_USER: u8 = 4;
const TIMEOUT: u8 = 5;

/// `None` if `!enabled` or the bind fails. Otherwise a thread that accepts connections one at a time
/// (a blocking `accept` costs no CPU while idle) and calls `inject` with each good phrase.
pub fn start(
    data_dir: &Path,
    enabled: bool,
    inject: Arc<dyn Fn(String) + Send + Sync>,
) -> Option<std::thread::JoinHandle<()>> {
    if !enabled {
        metrics::debug_input(false, 0);
        return None;
    }
    let Some(listener) = bind(data_dir) else {
        metrics::debug_input(false, 0);
        return None;
    };
    metrics::debug_input(true, 0);
    std::thread::Builder::new()
        .name("so-debug-input".into())
        .spawn(move || serve(listener, inject))
        .ok()
}

/// Folder 0700 first, then the socket, then 0600. A leftover socket from an earlier run is removed;
/// anything else sitting at that path stops the start.
fn bind(data_dir: &Path) -> Option<UnixListener> {
    let dir = data_dir.join(SOCKET_DIR);
    // Never follow a link at the folder: a symlink could point the socket (and the chmod) elsewhere.
    match std::fs::symlink_metadata(&dir) {
        Ok(meta) if meta.file_type().is_symlink() => return None,
        _ => {}
    }
    std::fs::DirBuilder::new().recursive(true).mode(0o700).create(&dir).ok()?;
    let meta = std::fs::symlink_metadata(&dir).ok()?;
    // SAFETY: geteuid has no preconditions.
    if !meta.file_type().is_dir() || meta.uid() != unsafe { libc::geteuid() } {
        return None;
    }
    std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700)).ok()?;
    let path = dir.join(SOCKET_NAME);
    if let Ok(meta) = std::fs::symlink_metadata(&path) {
        if !meta.file_type().is_socket() {
            return None;
        }
        std::fs::remove_file(&path).ok()?;
    }
    let listener = UnixListener::bind(&path).ok()?;
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).ok()?;
    Some(listener)
}

fn serve(listener: UnixListener, inject: Arc<dyn Fn(String) + Send + Sync>) {
    for conn in listener.incoming() {
        let Ok(stream) = conn else {
            // Avoid a hot loop if accept keeps failing.
            std::thread::sleep(Duration::from_millis(100));
            continue;
        };
        handle(stream, &inject);
    }
}

/// True if the other end of the socket runs as our own user.
fn same_user(stream: &UnixStream) -> bool {
    let (mut uid, mut gid): (libc::uid_t, libc::gid_t) = (0, 0);
    // SAFETY: the fd is open (owned by `stream`) and both pointers point at live locals.
    let rc = unsafe { libc::getpeereid(stream.as_raw_fd(), &mut uid, &mut gid) };
    // SAFETY: geteuid has no preconditions.
    rc == 0 && uid == unsafe { libc::geteuid() }
}

fn handle(stream: UnixStream, inject: &Arc<dyn Fn(String) + Send + Sync>) {
    if !same_user(&stream) {
        metrics::debug_input(true, WRONG_USER);
        return;
    }
    match read_line(&stream) {
        Ok(text) => {
            inject(text);
            let _ = (&stream).write(b"ok\n");
        }
        Err(code) => {
            metrics::debug_input(true, code);
            let _ = (&stream).write(b"err\n");
        }
    }
}

/// Reads up to the first newline. Err = the reject code. The 2 s window covers the whole line, so a
/// client dripping one byte at a time cannot hold the listener.
fn read_line(stream: &UnixStream) -> Result<String, u8> {
    let deadline = Instant::now() + READ_WINDOW;
    let mut buf: Vec<u8> = Vec::with_capacity(MAX_LINE_BYTES + 2);
    let mut chunk = [0u8; 256];
    let end = loop {
        if let Some(i) = buf.iter().position(|&b| b == b'\n') {
            break i;
        }
        if buf.len() > MAX_LINE_BYTES {
            return Err(TOO_LONG);
        }
        let left = deadline.saturating_duration_since(Instant::now());
        if left.is_zero() {
            return Err(TIMEOUT);
        }
        stream.set_read_timeout(Some(left)).map_err(|_| TIMEOUT)?;
        match (&*stream).read(&mut chunk) {
            Ok(0) => return Err(TIMEOUT), // closed before a newline
            Ok(n) => buf.extend_from_slice(&chunk[..n]),
            Err(_) => return Err(TIMEOUT),
        }
    };
    if end > MAX_LINE_BYTES {
        return Err(TOO_LONG);
    }
    let text = std::str::from_utf8(&buf[..end]).map_err(|_| BAD_UTF8)?;
    if text.trim().is_empty() {
        return Err(EMPTY);
    }
    Ok(text.to_string())
}
