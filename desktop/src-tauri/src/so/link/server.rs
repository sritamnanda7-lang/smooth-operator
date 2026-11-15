// SPDX-License-Identifier: FSL-1.1-MIT
//! The local WebSocket server: handshake checks (path, Host, Origin, slots), then the session
//! (hello nonce, auth or pair, ping). Bound to 127.0.0.1 only.

use super::commands::{LinkState, LinkStatus};
use super::pairing::{PairError, PairResult, Pairing, PairingView, Rng};
use super::protocol::{
    parse_ext, pong_json, status_json, AppState, BadMsg, ExtMsg, Untrusted, CLOSE_AUTH, CLOSE_BAD_MSG, CLOSE_PAIRED, CLOSE_REPLACED, CLOSE_TIMEOUT,
};
use super::token::{nonce_hex, server_proof, verify_client_proof, LinkToken, TokenStore};
use super::{metrics, LinkConfig};
use crate::so::hotkey::watcher::Clock;
use futures_util::{SinkExt, StreamExt};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::oneshot;
use tokio::time::timeout_at;
use tokio_tungstenite::tungstenite::handshake::server::{Callback, ErrorResponse, Request, Response};
use tokio_tungstenite::tungstenite::http::StatusCode;
use tokio_tungstenite::tungstenite::protocol::frame::coding::CloseCode;
use tokio_tungstenite::tungstenite::protocol::{CloseFrame, WebSocketConfig};
use tokio_tungstenite::tungstenite::{Error as WsError, Message};
use tokio_tungstenite::{accept_hdr_async_with_config, WebSocketStream};

type Ws = WebSocketStream<TcpStream>;

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

/// What the app knows about the stored token. Plain facts, never the token itself.
#[derive(Clone, Copy, PartialEq, Eq)]
enum TokenState {
    Unknown,
    Exists(bool),
    KeychainError,
}

/// The one authenticated session. `close` tells it "a newer one won, or the token was replaced" (4409).
struct AuthSlot {
    id: u64,
    close: oneshot::Sender<()>,
}

struct Inner {
    pairing: Pairing,
    /// Handshaken, not yet authenticated (at most one).
    pending: bool,
    auth: Option<AuthSlot>,
    token: TokenState,
    port_busy: bool,
}

/// Holds the pairing window, the slots (drop guard), the Keychain read-in-flight flag and whether a
/// token exists.
pub struct Link {
    cfg: LinkConfig,
    tokens: Arc<dyn TokenStore>,
    clock: Arc<dyn Clock>,
    rng: Arc<dyn Rng>,
    inner: Mutex<Inner>,
    /// True while a blocking Keychain read runs. `spawn_blocking` cannot be cancelled, so a second auth
    /// is refused at once instead of starting another read.
    read_in_flight: Arc<AtomicBool>,
    /// TCP connections still in their handshake.
    handshakes: AtomicUsize,
    next_id: AtomicUsize,
}

/// Drop guard: frees the pending slot on every exit path (success, close, client drop, timeout, panic).
struct PendingGuard(Arc<Link>);
impl Drop for PendingGuard {
    fn drop(&mut self) {
        self.0.lock().pending = false;
    }
}

/// Drop guard for the authenticated slot: clears it only if it still belongs to this session.
struct AuthGuard {
    link: Arc<Link>,
    id: u64,
}
impl Drop for AuthGuard {
    fn drop(&mut self) {
        let mut g = self.link.lock();
        if g.auth.as_ref().is_some_and(|a| a.id == self.id) {
            g.auth = None;
        }
    }
}

/// Counts a TCP connection while it is in its handshake.
struct HandshakeGuard(Arc<Link>);
impl Drop for HandshakeGuard {
    fn drop(&mut self) {
        self.0.handshakes.fetch_sub(1, Ordering::SeqCst);
    }
}

/// Clears `read_in_flight` when the blocking read ends, even if it panics.
struct ReadFlag(Arc<AtomicBool>);
impl ReadFlag {
    /// Takes the flag, or `None` when a read is already running.
    fn take(flag: &Arc<AtomicBool>) -> Option<ReadFlag> {
        if flag.swap(true, Ordering::SeqCst) {
            None
        } else {
            Some(ReadFlag(flag.clone()))
        }
    }
}
impl Drop for ReadFlag {
    fn drop(&mut self) {
        self.0.store(false, Ordering::SeqCst);
    }
}

/// What came off the socket.
enum Got {
    Msg(Untrusted<ExtMsg>),
    /// Close with this code; log this outcome.
    Bad(u16, u8),
    Gone,
}

fn error_response(status: StatusCode) -> ErrorResponse {
    let mut r = ErrorResponse::new(None);
    *r.status_mut() = status;
    r
}

/// The handshake callback: runs the pure checks, then takes the pending slot. A struct (not a closure) so
/// the large `ErrorResponse` in the trait signature is the library's, not ours.
struct Shake<'a> {
    link: &'a Arc<Link>,
    port: u16,
    t0: Instant,
    pending: &'a mut Option<PendingGuard>,
}

impl Callback for Shake<'_> {
    fn on_request(self, req: &Request, response: Response) -> Result<Response, ErrorResponse> {
        let ms = self.t0.elapsed().as_millis() as u64;
        if let Err(why) = check_handshake(req, &self.link.cfg, self.port) {
            let (outcome, status) = match why {
                HandshakeReject::Path => (5, StatusCode::NOT_FOUND),
                HandshakeReject::Host => (3, StatusCode::FORBIDDEN),
                HandshakeReject::Origin => (2, StatusCode::FORBIDDEN),
                HandshakeReject::Busy => (4, StatusCode::CONFLICT),
            };
            metrics::link(outcome, status.as_u16(), ms);
            return Err(error_response(status));
        }
        match self.link.take_pending() {
            Some(g) => *self.pending = Some(g),
            None => {
                metrics::link(4, 409, ms);
                return Err(error_response(StatusCode::CONFLICT));
            }
        }
        Ok(response)
    }
}

/// Next text message from the extension, parsed. Control frames are skipped; the caller's timeout covers them.
async fn next_ext(ws: &mut Ws, max: usize) -> Got {
    loop {
        match ws.next().await {
            Some(Ok(Message::Text(t))) => {
                return match parse_ext(t.as_str(), max) {
                    Ok(m) => Got::Msg(m),
                    Err(BadMsg::TooBig) => Got::Bad(1009, 24),
                    Err(_) => Got::Bad(CLOSE_BAD_MSG, 23),
                }
            }
            Some(Ok(Message::Binary(_))) => return Got::Bad(CLOSE_BAD_MSG, 23),
            Some(Ok(Message::Ping(_) | Message::Pong(_) | Message::Frame(_))) => continue,
            Some(Err(WsError::Capacity(_))) => return Got::Bad(1009, 24),
            Some(Err(WsError::Utf8(_))) => return Got::Bad(CLOSE_BAD_MSG, 23),
            Some(Ok(Message::Close(_))) | Some(Err(_)) | None => return Got::Gone,
        }
    }
}

/// A write to an untrusted peer that never reads must not hold a slot: 1 s at most.
const WRITE_LIMIT: Duration = Duration::from_secs(1);

async fn send_text(ws: &mut Ws, text: String) -> bool {
    matches!(tokio::time::timeout(WRITE_LIMIT, ws.send(Message::text(text))).await, Ok(Ok(())))
}

async fn send_state(ws: &mut Ws, s: &AppState) -> bool {
    send_text(ws, status_json(s)).await
}

/// Sends the Close frame (empty reason), then ends our side and reads what the peer still sends for a
/// moment, so the close code arrives instead of a connection reset.
async fn finish(ws: &mut Ws, code: u16) {
    let _ = tokio::time::timeout(WRITE_LIMIT, async {
        let frame = CloseFrame { code: CloseCode::from(code), reason: "".into() };
        let _ = ws.close(Some(frame)).await;
        let tcp = ws.get_mut();
        let _ = tcp.shutdown().await;
        let mut buf = [0u8; 1024];
        let _ = tokio::time::timeout(Duration::from_millis(300), async {
            while matches!(tcp.read(&mut buf).await, Ok(n) if n > 0) {}
        })
        .await;
    })
    .await;
}

impl Link {
    pub fn new(cfg: LinkConfig, tokens: Arc<dyn TokenStore>, clock: Arc<dyn Clock>, rng: Arc<dyn Rng>) -> Arc<Self> {
        Arc::new(Link {
            cfg,
            tokens,
            clock,
            rng,
            inner: Mutex::new(Inner { pairing: Pairing::new(), pending: false, auth: None, token: TokenState::Unknown, port_busy: false }),
            read_in_flight: Arc::new(AtomicBool::new(false)),
            handshakes: AtomicUsize::new(0),
            next_id: AtomicUsize::new(1),
        })
    }

    /// A poisoned lock (a panic elsewhere) must not take the whole link down.
    fn lock(&self) -> MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn set_token(&self, t: TokenState) {
        self.lock().token = t;
    }

    /// `None` means the port is busy (outcome 1).
    pub async fn bind(&self, port: u16) -> Option<TcpListener> {
        match TcpListener::bind(("127.0.0.1", port)).await {
            Ok(l) => {
                metrics::link(0, 0, 0);
                Some(l)
            }
            Err(_) => {
                self.lock().port_busy = true;
                metrics::link(1, 0, 0);
                None
            }
        }
    }

    /// Accepts forever. More than `max_handshakes` connections still in their handshake: the extra ones are dropped.
    pub async fn serve(self: Arc<Self>, listener: TcpListener) {
        loop {
            let Ok((tcp, _)) = listener.accept().await else {
                tokio::time::sleep(Duration::from_millis(100)).await;
                continue;
            };
            if self.handshakes.fetch_add(1, Ordering::SeqCst) >= self.cfg.max_handshakes {
                self.handshakes.fetch_sub(1, Ordering::SeqCst);
                metrics::link(4, 0, 0);
                continue;
            }
            tokio::spawn(self.clone().connection(tcp, HandshakeGuard(self.clone())));
        }
    }

    /// Blocks at most once (the first token-exists read).
    pub fn status(&self) -> LinkStatus {
        let now = self.clock.now();
        let (busy, connected, token, pairing_secs_left, locked_secs) = {
            let g = self.lock();
            (g.port_busy, g.auth.is_some(), g.token, g.pairing.secs_left(now), g.pairing.locked_secs(now))
        };
        let state = if busy {
            LinkState::PortBusy
        } else if connected {
            LinkState::Connected
        } else {
            match token {
                TokenState::Exists(true) => LinkState::NotFound,
                TokenState::Exists(false) => LinkState::NotPaired,
                TokenState::KeychainError => LinkState::KeychainError,
                TokenState::Unknown => match ReadFlag::take(&self.read_in_flight) {
                    // A read is already running: do not start a second one.
                    None => LinkState::KeychainError,
                    Some(_flag) => match self.tokens.get() {
                        Ok(t) => {
                            self.set_token(TokenState::Exists(t.is_some()));
                            if t.is_some() {
                                LinkState::NotFound
                            } else {
                                LinkState::NotPaired
                            }
                        }
                        Err(_) => {
                            self.set_token(TokenState::KeychainError);
                            LinkState::KeychainError
                        }
                    },
                },
            }
        };
        LinkStatus { state, pairing_secs_left, locked_secs }
    }

    pub fn start_pairing(&self) -> Result<PairingView, PairError> {
        let mut g = self.lock();
        if g.port_busy {
            return Err(PairError::PortBusy);
        }
        g.pairing.start(self.clock.now(), &*self.rng)
    }

    pub fn cancel_pairing(&self) {
        self.lock().pairing.cancel();
    }

    fn take_pending(self: &Arc<Self>) -> Option<PendingGuard> {
        let mut g = self.lock();
        if g.pending {
            return None;
        }
        g.pending = true;
        Some(PendingGuard(self.clone()))
    }

    /// Makes `tx` the authenticated session; an older one is told to close (4409).
    fn register(self: &Arc<Self>, tx: oneshot::Sender<()>) -> AuthGuard {
        let id = self.next_id.fetch_add(1, Ordering::SeqCst) as u64;
        if let Some(old) = self.lock().auth.replace(AuthSlot { id, close: tx }) {
            let _ = old.close.send(());
        }
        AuthGuard { link: self.clone(), id }
    }

    /// After a token was saved: it exists now, and the old authenticated session (old token) is closed.
    fn after_pair(&self) {
        let mut g = self.lock();
        g.token = TokenState::Exists(true);
        if let Some(old) = g.auth.take() {
            let _ = old.close.send(());
        }
    }

    /// One TCP connection from accept to close. The 5 s auth deadline starts here, so it also covers a
    /// slow handshake. Rejections log outcome + HTTP code only, never header values.
    async fn connection(self: Arc<Self>, tcp: TcpStream, hs: HandshakeGuard) {
        let t0 = Instant::now();
        let ms = || t0.elapsed().as_millis() as u64;
        let deadline = tokio::time::Instant::now() + self.cfg.auth_timeout;
        let Ok(port) = tcp.local_addr().map(|a| a.port()) else { return };
        let ws_cfg = WebSocketConfig::default()
            .max_message_size(Some(self.cfg.max_msg))
            .max_frame_size(Some(self.cfg.max_msg))
            .read_buffer_size(self.cfg.read_buffer);
        let mut pending: Option<PendingGuard> = None;
        let callback = Shake { link: &self, port, t0, pending: &mut pending };
        let shake = timeout_at(deadline, accept_hdr_async_with_config(tcp, callback, Some(ws_cfg))).await;
        drop(hs);
        let mut ws = match shake {
            Ok(Ok(ws)) => ws,
            Ok(Err(_)) => return,
            Err(_) => {
                metrics::link(22, 0, ms());
                return;
            }
        };
        let Some(pending) = pending else { return };
        let Ok(nonce) = nonce_hex(&*self.rng) else {
            finish(&mut ws, 1011).await;
            return;
        };
        if !send_state(&mut ws, &AppState::Hello { nonce: nonce.clone() }).await {
            return;
        }
        let first = match timeout_at(deadline, next_ext(&mut ws, self.cfg.max_msg)).await {
            Err(_) => {
                metrics::link(22, CLOSE_TIMEOUT, ms());
                finish(&mut ws, CLOSE_TIMEOUT).await;
                return;
            }
            Ok(Got::Gone) => {
                metrics::link(25, 0, ms());
                return;
            }
            Ok(Got::Bad(code, outcome)) => {
                metrics::link(outcome, code, ms());
                finish(&mut ws, code).await;
                return;
            }
            Ok(Got::Msg(m)) => m,
        };
        // Pre-auth outcomes share one shape: tell the extension, log, close 4401.
        let (state, outcome) = match first.get() {
            ExtMsg::Ping => {
                metrics::link(23, CLOSE_BAD_MSG, ms());
                finish(&mut ws, CLOSE_BAD_MSG).await;
                return;
            }
            ExtMsg::Pair { code } => {
                let result = self.lock().pairing.try_code(self.clock.now(), code);
                match result {
                    PairResult::Ok => match self.save_new_token(deadline).await {
                        Saved::Timeout => {
                            metrics::link(22, CLOSE_TIMEOUT, ms());
                            finish(&mut ws, CLOSE_TIMEOUT).await;
                            return;
                        }
                        Saved::Failed => (AppState::Rejected, 27),
                        Saved::Ok(token_hex) => {
                            drop(pending);
                            if send_state(&mut ws, &AppState::Paired { token: token_hex }).await {
                                metrics::link(10, CLOSE_PAIRED, ms());
                                finish(&mut ws, CLOSE_PAIRED).await;
                            }
                            return;
                        }
                    },
                    PairResult::Wrong => (AppState::WrongCode, 11),
                    PairResult::Locked { secs } => (AppState::Locked { retry_s: secs }, 12),
                    PairResult::Expired => (AppState::Expired, 13),
                    PairResult::NotPairing => (AppState::NotPairing, 13),
                }
            }
            ExtMsg::Auth { proof, cnonce } => match self.read_token(deadline, &nonce, proof, cnonce).await {
                Read::Timeout => {
                    metrics::link(22, CLOSE_TIMEOUT, ms());
                    finish(&mut ws, CLOSE_TIMEOUT).await;
                    return;
                }
                Read::Failed => (AppState::Rejected, 27),
                Read::Refused => (AppState::Rejected, 21),
                Read::Proved(app_proof) => {
                    let (tx, rx) = oneshot::channel();
                    let _auth = self.register(tx);
                    drop(pending);
                    if send_state(&mut ws, &AppState::AuthOk { proof: app_proof }).await {
                        metrics::link(20, 0, ms());
                        self.authenticated(&mut ws, rx, t0).await;
                    }
                    return;
                }
            },
        };
        // The refusal path: the pending slot is freed by `pending` going out of scope at the end.
        let _ = send_state(&mut ws, &state).await;
        metrics::link(outcome, CLOSE_AUTH, ms());
        finish(&mut ws, CLOSE_AUTH).await;
    }

    /// Reads the token on the blocking pool and checks the extension's proof there, so the token never
    /// leaves that closure. Only one read exists at a time (`read_in_flight`).
    async fn read_token(self: &Arc<Self>, deadline: tokio::time::Instant, nonce: &str, proof: &str, cnonce: &str) -> Read {
        let Some(flag) = ReadFlag::take(&self.read_in_flight) else {
            self.set_token(TokenState::KeychainError);
            return Read::Failed;
        };
        let link = self.clone();
        let (nonce, proof, cnonce) = (nonce.to_string(), proof.to_string(), cnonce.to_string());
        let job = tokio::task::spawn_blocking(move || {
            let _flag = flag;
            let r = link.tokens.get();
            link.set_token(match &r {
                Ok(t) => TokenState::Exists(t.is_some()),
                Err(_) => TokenState::KeychainError,
            });
            r.map(|t| t.filter(|t| verify_client_proof(t, &nonce, &proof)).map(|t| server_proof(&t, &cnonce)))
        });
        match timeout_at(deadline, job).await {
            Err(_) => Read::Timeout,
            Ok(Ok(Ok(Some(app_proof)))) => Read::Proved(app_proof),
            Ok(Ok(Ok(None))) => Read::Refused,
            Ok(_) => Read::Failed,
        }
    }

    /// Makes and saves a new token. Returns its hex only after the save worked.
    async fn save_new_token(self: &Arc<Self>, deadline: tokio::time::Instant) -> Saved {
        // shortcut: a save that finishes after the 5 s deadline still rotates the token (the extension never got
        // it, so the founder clicks Pair again); upgrade if that ever confuses users.
        let Ok(token) = LinkToken::generate(&*self.rng) else { return Saved::Failed };
        let token_hex = token.to_hex();
        let link = self.clone();
        let job = tokio::task::spawn_blocking(move || {
            let r = link.tokens.set(&token);
            if r.is_ok() {
                link.after_pair();
            }
            r
        });
        match timeout_at(deadline, job).await {
            Err(_) => Saved::Timeout,
            Ok(Ok(Ok(()))) => Saved::Ok(token_hex),
            Ok(_) => Saved::Failed,
        }
    }

    /// The authenticated session: ping/pong, idle timeout, and "replaced" (4409).
    async fn authenticated(&self, ws: &mut Ws, mut replaced: oneshot::Receiver<()>, t0: Instant) {
        let ms = || t0.elapsed().as_millis() as u64;
        let idle = self.cfg.idle_timeout;
        let mut idle_end = tokio::time::Instant::now() + idle;
        loop {
            let got = tokio::select! {
                _ = &mut replaced => {
                    metrics::link(26, CLOSE_REPLACED, ms());
                    finish(ws, CLOSE_REPLACED).await;
                    return;
                }
                r = timeout_at(idle_end, next_ext(ws, self.cfg.max_msg)) => r,
            };
            match got {
                Err(_) => {
                    metrics::link(25, 1000, ms());
                    finish(ws, 1000).await;
                    return;
                }
                Ok(Got::Gone) => {
                    metrics::link(25, 0, ms());
                    return;
                }
                Ok(Got::Bad(code, outcome)) => {
                    metrics::link(outcome, code, ms());
                    finish(ws, code).await;
                    return;
                }
                Ok(Got::Msg(m)) => match m.get() {
                    ExtMsg::Ping => {
                        if !send_text(ws, pong_json()).await {
                            return;
                        }
                        idle_end = tokio::time::Instant::now() + idle;
                    }
                    _ => {
                        metrics::link(23, CLOSE_BAD_MSG, ms());
                        finish(ws, CLOSE_BAD_MSG).await;
                        return;
                    }
                },
            }
        }
    }
}

enum Read {
    /// Right proof: the app's answer proof.
    Proved(String),
    /// No token stored, or a wrong proof.
    Refused,
    Failed,
    Timeout,
}

enum Saved {
    Ok(String),
    Failed,
    Timeout,
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
