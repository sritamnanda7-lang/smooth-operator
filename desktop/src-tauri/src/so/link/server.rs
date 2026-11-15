// SPDX-License-Identifier: FSL-1.1-MIT
//! The local WebSocket server: handshake checks (path, Host, Origin, slots), then the session
//! (hello nonce, auth or pair, ping). Bound to 127.0.0.1 only.

use super::commands::LinkStatus;
use super::pairing::{PairError, PairingView, Rng};
use super::token::TokenStore;
use super::LinkConfig;
use crate::so::hotkey::watcher::Clock;
use std::sync::Arc;
use tokio::net::TcpListener;
use tokio_tungstenite::tungstenite::handshake::server::Request;

/// Why a handshake was refused: HTTP 404, 403, 403, 409.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HandshakeReject {
    Path,
    Host,
    Origin,
    Busy,
}

/// Pure checks, in order: path, Host, Origin. (Slots are checked by the session code.)
pub fn check_handshake(req: &Request, cfg: &LinkConfig, port: u16) -> Result<(), HandshakeReject> {
    // Path exactly "/" and no query (even an empty one), so a token can never ride in the URL.
    if req.uri().path() != "/" || req.uri().query().is_some() {
        return Err(HandshakeReject::Path);
    }
    // Exactly one Host header, byte for byte "127.0.0.1:<bound port>" (stops DNS rebinding).
    let want_host = format!("127.0.0.1:{port}");
    let mut hosts = req.headers().get_all("host").iter();
    if !matches!((hosts.next(), hosts.next()), (Some(h), None) if h.as_bytes() == want_host.as_bytes()) {
        return Err(HandshakeReject::Host);
    }
    // Exactly one Origin header, byte for byte the allowed one. Missing, "null" or duplicated = refused.
    let mut origins = req.headers().get_all("origin").iter();
    if !matches!((origins.next(), origins.next()), (Some(o), None) if o.as_bytes() == cfg.allowed_origin.as_bytes()) {
        return Err(HandshakeReject::Origin);
    }
    Ok(())
}

/// Holds the pairing window, the slots (drop guard), the Keychain read-in-flight flag and whether a
/// token exists.
pub struct Link;

impl Link {
    pub fn new(
        _cfg: LinkConfig,
        _tokens: Arc<dyn TokenStore>,
        _clock: Arc<dyn Clock>,
        _rng: Arc<dyn Rng>,
    ) -> Arc<Self> {
        todo!()
    }
    /// `None` means the port is busy (outcome 1).
    pub async fn bind(&self, _port: u16) -> Option<TcpListener> {
        todo!()
    }
    /// Accepts forever.
    pub async fn serve(self: Arc<Self>, _listener: TcpListener) {
        todo!()
    }
    /// Blocks at most once (the first token-exists read).
    pub fn status(&self) -> LinkStatus {
        todo!()
    }
    pub fn start_pairing(&self) -> Result<PairingView, PairError> {
        todo!()
    }
    pub fn cancel_pairing(&self) {
        todo!()
    }
}

// Stub phase: nothing in the app calls these yet. This reference keeps them compiled without a
// dead-code warning and without an allow (strip-check s05).
const _: () = {
    let _ = (HandshakeReject::Path, HandshakeReject::Host, HandshakeReject::Origin, HandshakeReject::Busy);
    let _ = check_handshake;
    let _ = Link::new;
    let _ = Link::bind;
    let _ = Link::serve;
    let _ = Link::status;
    let _ = Link::start_pairing;
    let _ = Link::cancel_pairing;
};
