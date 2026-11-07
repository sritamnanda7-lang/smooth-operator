// SPDX-License-Identifier: FSL-1.1-MIT
//! Gate 7: a tiny WebSocket server for the spike extension.
//! - Listens on 127.0.0.1:51717 only (never 0.0.0.0).
//! - Accepts only an `Origin` that starts with `chrome-extension://` and a local `Host`.
//! - Protocol: client `{"type":"hello","ext":"so-spike"}` -> server `{"type":"ack","app":"so-spike"}`.
//! Written by hand (handshake + text frames, no new dependency) because the spike needs
//! exactly one message. The real step-5 server will use a reviewed library plus a pairing token.

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;
use tauri::AppHandle;

pub const ADDR: &str = "127.0.0.1:51717";
const MAX_HEADER: usize = 8 * 1024;
const MAX_PAYLOAD: usize = 4096;
const MAX_CONNECTIONS: usize = 4;
const GUID: &str = "258EAFA5-E914-47DA-95CA-C5AB0DC85B11";

static OPEN: AtomicUsize = AtomicUsize::new(0);

pub fn start(app: AppHandle) {
    let listener = match TcpListener::bind(ADDR) {
        Ok(l) => l,
        Err(_) => {
            super::harness::event("g7", "bind_failed");
            super::panel::say(&app, "ws: port 51717 busy");
            return;
        }
    };
    super::harness::event("g7", "listening");
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            if OPEN.load(Ordering::SeqCst) >= MAX_CONNECTIONS {
                continue; // dropped = closed
            }
            OPEN.fetch_add(1, Ordering::SeqCst);
            let app = app.clone();
            std::thread::spawn(move || {
                let _ = serve(stream, &app);
                OPEN.fetch_sub(1, Ordering::SeqCst);
            });
        }
    });
}

/// What the handshake decided.
#[derive(Debug, PartialEq, Eq)]
enum Handshake {
    Accept(String),
    Reject(&'static str),
}

/// Pure check of the HTTP upgrade request (unit tested).
fn check_request(req: &str) -> Handshake {
    let mut lines = req.split("\r\n");
    let first = lines.next().unwrap_or("");
    if !(first.starts_with("GET ") && first.ends_with(" HTTP/1.1")) {
        return Handshake::Reject("bad request line");
    }
    let (mut origin, mut host, mut key, mut upgrade) = (None, None, None, false);
    for l in lines {
        let Some((name, value)) = l.split_once(':') else { continue };
        let value = value.trim();
        match name.trim().to_ascii_lowercase().as_str() {
            "origin" => origin = Some(value.to_string()),
            "host" => host = Some(value.to_string()),
            "sec-websocket-key" => key = Some(value.to_string()),
            "upgrade" => upgrade = value.eq_ignore_ascii_case("websocket"),
            _ => {}
        }
    }
    // Origin check: only a browser extension page may connect.
    if !origin.as_deref().is_some_and(|o| o.starts_with("chrome-extension://")) {
        return Handshake::Reject("origin");
    }
    // Host check: stops DNS-rebinding (a web page pointing its own name at 127.0.0.1).
    if !matches!(host.as_deref(), Some("127.0.0.1:51717") | Some("localhost:51717")) {
        return Handshake::Reject("host");
    }
    match (upgrade, key) {
        (true, Some(k)) if !k.is_empty() && k.len() <= 64 => Handshake::Accept(accept_key(&k)),
        _ => Handshake::Reject("not a websocket upgrade"),
    }
}

fn serve(mut s: TcpStream, app: &AppHandle) -> std::io::Result<()> {
    s.set_read_timeout(Some(Duration::from_secs(5)))?;
    // Read the HTTP header block.
    let mut buf = Vec::new();
    let mut chunk = [0u8; 1024];
    while !buf.windows(4).any(|w| w == b"\r\n\r\n") {
        if buf.len() > MAX_HEADER {
            return Ok(());
        }
        let n = s.read(&mut chunk)?;
        if n == 0 {
            return Ok(());
        }
        buf.extend_from_slice(&chunk[..n]);
    }
    let end = buf.windows(4).position(|w| w == b"\r\n\r\n").unwrap_or(0) + 4;
    let req = String::from_utf8_lossy(&buf[..end]).to_string();
    let mut leftover = buf[end..].to_vec();

    match check_request(&req) {
        Handshake::Reject(why) => {
            super::harness::event("g7", match why {
                "origin" => "reject_origin",
                "host" => "reject_host",
                _ => "reject_other",
            });
            s.write_all(b"HTTP/1.1 403 Forbidden\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")?;
            return Ok(());
        }
        Handshake::Accept(accept) => {
            let resp = format!(
                "HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Accept: {accept}\r\n\r\n"
            );
            s.write_all(resp.as_bytes())?;
        }
    }
    super::harness::event("g7", "ws_open");
    s.set_read_timeout(None)?; // an idle extension stays connected

    loop {
        let Some((opcode, payload)) = read_frame(&mut s, &mut leftover)? else { return Ok(()) };
        match opcode {
            0x1 => {
                let ok = serde_json::from_slice::<serde_json::Value>(&payload)
                    .ok()
                    .is_some_and(|v| v["type"] == "hello" && v["ext"] == "so-spike");
                if ok {
                    write_frame(&mut s, 0x1, br#"{"type":"ack","app":"so-spike"}"#)?;
                    super::harness::event("g7", "hello_ack");
                    super::panel::say(app, "ext connected");
                } else {
                    super::harness::event("g7", "message_ignored");
                }
            }
            0x8 => {
                write_frame(&mut s, 0x8, &[])?;
                return Ok(());
            }
            0x9 => write_frame(&mut s, 0xA, &payload)?,
            _ => {}
        }
    }
}

/// Read one client frame. Client frames are always masked. Returns `None` on close/EOF.
fn read_frame(s: &mut TcpStream, leftover: &mut Vec<u8>) -> std::io::Result<Option<(u8, Vec<u8>)>> {
    let need = |s: &mut TcpStream, n: usize, buf: &mut Vec<u8>| -> std::io::Result<bool> {
        while buf.len() < n {
            let mut tmp = [0u8; 1024];
            let got = s.read(&mut tmp)?;
            if got == 0 {
                return Ok(false);
            }
            buf.extend_from_slice(&tmp[..got]);
        }
        Ok(true)
    };
    if !need(s, 2, leftover)? {
        return Ok(None);
    }
    let (b0, b1) = (leftover[0], leftover[1]);
    let masked = b1 & 0x80 != 0;
    let mut len = (b1 & 0x7F) as usize;
    let mut off = 2;
    if len == 126 {
        if !need(s, 4, leftover)? {
            return Ok(None);
        }
        len = u16::from_be_bytes([leftover[2], leftover[3]]) as usize;
        off = 4;
    } else if len == 127 {
        return Ok(None); // far too big for this spike
    }
    if !masked || len > MAX_PAYLOAD || b0 & 0x80 == 0 {
        return Ok(None); // unmasked, oversize or fragmented: drop the connection
    }
    if !need(s, off + 4 + len, leftover)? {
        return Ok(None);
    }
    let mask = [leftover[off], leftover[off + 1], leftover[off + 2], leftover[off + 3]];
    let payload: Vec<u8> = leftover[off + 4..off + 4 + len].iter().enumerate().map(|(i, b)| b ^ mask[i % 4]).collect();
    leftover.drain(..off + 4 + len);
    Ok(Some((b0 & 0x0F, payload)))
}

fn write_frame(s: &mut TcpStream, opcode: u8, payload: &[u8]) -> std::io::Result<()> {
    let mut f = vec![0x80 | opcode];
    if payload.len() < 126 {
        f.push(payload.len() as u8);
    } else {
        f.push(126);
        f.extend_from_slice(&(payload.len() as u16).to_be_bytes());
    }
    f.extend_from_slice(payload);
    s.write_all(&f)
}

fn accept_key(client_key: &str) -> String {
    let mut data = client_key.as_bytes().to_vec();
    data.extend_from_slice(GUID.as_bytes());
    base64(&sha1(&data))
}

/// SHA-1, needed only because RFC 6455 uses it for the handshake (not for security).
fn sha1(data: &[u8]) -> [u8; 20] {
    let mut h: [u32; 5] = [0x6745_2301, 0xEFCD_AB89, 0x98BA_DCFE, 0x1032_5476, 0xC3D2_E1F0];
    let mut msg = data.to_vec();
    msg.push(0x80);
    while msg.len() % 64 != 56 {
        msg.push(0);
    }
    msg.extend_from_slice(&((data.len() as u64) * 8).to_be_bytes());
    for block in msg.chunks(64) {
        let mut w = [0u32; 80];
        for i in 0..16 {
            w[i] = u32::from_be_bytes([block[4 * i], block[4 * i + 1], block[4 * i + 2], block[4 * i + 3]]);
        }
        for i in 16..80 {
            w[i] = (w[i - 3] ^ w[i - 8] ^ w[i - 14] ^ w[i - 16]).rotate_left(1);
        }
        let [mut a, mut b, mut c, mut d, mut e] = h;
        for (i, wi) in w.iter().enumerate() {
            let (f, k) = match i {
                0..=19 => ((b & c) | (!b & d), 0x5A82_7999),
                20..=39 => (b ^ c ^ d, 0x6ED9_EBA1),
                40..=59 => ((b & c) | (b & d) | (c & d), 0x8F1B_BCDC),
                _ => (b ^ c ^ d, 0xCA62_C1D6u32),
            };
            let t = a.rotate_left(5).wrapping_add(f).wrapping_add(e).wrapping_add(k).wrapping_add(*wi);
            e = d;
            d = c;
            c = b.rotate_left(30);
            b = a;
            a = t;
        }
        h = [h[0].wrapping_add(a), h[1].wrapping_add(b), h[2].wrapping_add(c), h[3].wrapping_add(d), h[4].wrapping_add(e)];
    }
    let mut out = [0u8; 20];
    for (i, v) in h.iter().enumerate() {
        out[4 * i..4 * i + 4].copy_from_slice(&v.to_be_bytes());
    }
    out
}

fn base64(data: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut s = String::new();
    for c in data.chunks(3) {
        let n = (c[0] as u32) << 16 | (*c.get(1).unwrap_or(&0) as u32) << 8 | *c.get(2).unwrap_or(&0) as u32;
        s.push(T[(n >> 18) as usize & 63] as char);
        s.push(T[(n >> 12) as usize & 63] as char);
        s.push(if c.len() > 1 { T[(n >> 6) as usize & 63] as char } else { '=' });
        s.push(if c.len() > 2 { T[n as usize & 63] as char } else { '=' });
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rfc6455_example_accept_key() {
        // Example from RFC 6455 section 1.3.
        assert_eq!(accept_key("dGhlIHNhbXBsZSBub25jZQ=="), "s3pPLMBiTxaQ9kYGzzhZRbK+xOo=");
    }

    fn req(origin: &str, host: &str) -> String {
        format!(
            "GET / HTTP/1.1\r\nHost: {host}\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nOrigin: {origin}\r\nSec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\nSec-WebSocket-Version: 13\r\n\r\n"
        )
    }

    #[test]
    fn extension_origin_accepted() {
        assert!(matches!(check_request(&req("chrome-extension://abcdef", "127.0.0.1:51717")), Handshake::Accept(_)));
    }

    #[test]
    fn web_page_origin_rejected() {
        assert_eq!(check_request(&req("https://evil.example", "127.0.0.1:51717")), Handshake::Reject("origin"));
        assert_eq!(check_request(&req("null", "127.0.0.1:51717")), Handshake::Reject("origin"));
    }

    #[test]
    fn missing_origin_rejected() {
        let r = "GET / HTTP/1.1\r\nHost: 127.0.0.1:51717\r\nUpgrade: websocket\r\nSec-WebSocket-Key: abc\r\n\r\n";
        assert_eq!(check_request(r), Handshake::Reject("origin"));
    }

    #[test]
    fn foreign_host_rejected() {
        assert_eq!(check_request(&req("chrome-extension://abc", "evil.example:51717")), Handshake::Reject("host"));
    }
}
