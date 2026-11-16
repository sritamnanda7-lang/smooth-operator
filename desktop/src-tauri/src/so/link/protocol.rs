// SPDX-License-Identifier: FSL-1.1-MIT
//! The JSON messages on the link. Everything the extension sends is parsed into `Untrusted<ExtMsg>`:
//! the type name is the label. No extension string ever reaches the panel, a log, Settings or a file.

pub const VERSION: u8 = 1;

/// Close codes (sent with an empty reason string).
pub const CLOSE_PAIRED: u16 = 4000;
pub const CLOSE_BAD_MSG: u16 = 4400;
pub const CLOSE_AUTH: u16 = 4401;
pub const CLOSE_TIMEOUT: u16 = 4408;
pub const CLOSE_REPLACED: u16 = 4409;

/// Data from the extension. No `Display`, no `Serialize`: it cannot be printed or forwarded by accident.
pub struct Untrusted<T>(T);

impl<T> Untrusted<T> {
    pub fn get(&self) -> &T {
        &self.0
    }
}

/// serde tag `t`, unknown fields denied, `v` checked.
pub enum ExtMsg {
    Pair { code: String },
    Auth { proof: String, cnonce: String },
    Ping,
    /// "res": keys t,v,id,r,ms only (step 6). No string field.
    #[allow(dead_code)] // stub: remove when wired (step 6)
    Result { id: u32, r: ResultCode, ms: u32 },
}

/// The 14 result codes the extension may answer with (step 6). Never page text, titles, URLs or ids.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)] // stub: remove when wired (step 6)
pub enum ResultCode {
    Playing,
    Ad,
    Paused,
    NoVideo,
    EditorOpen,
    NeedsLogin,
    Consent,
    NoAccess,
    Timeout,
    BadCmd,
    Busy,
    Unsafe,
    Cancelled,
    Error,
}

/// A command for the extension (step 6). URLs are built by `exec::ops`, never from page data.
#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(dead_code)] // stub: remove when wired (step 6)
pub enum Command {
    YtPlay { url: String },
    DocsOpen,
    CalOpen { url: String },
}

#[allow(dead_code)] // stub: remove when wired (step 6)
pub const MAX_CMD_ID: u32 = 2_147_483_647;
#[allow(dead_code)] // stub: remove when wired (step 6)
pub const MAX_RESULT_MS: u32 = 120_000;

/// `{"t":"cmd","v":1,"id":N,"op":...}` (stub).
#[allow(dead_code)] // stub: remove when wired (step 6)
pub fn cmd_json(_id: u32, _c: &Command) -> String {
    todo!("step 6 inc 2")
}

/// `{"t":"cancel","v":1,"id":N}` (stub).
#[allow(dead_code)] // stub: remove when wired (step 6)
pub fn cancel_json(_id: u32) -> String {
    todo!("step 6 inc 2")
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BadMsg {
    TooBig,
    NotJson,
    UnknownType,
    UnknownField,
    BadVersion,
    BadField,
}

/// Checks in this order: size, JSON, `t`, unknown fields, `v`, field formats. Hand-checked on a JSON
/// value (instead of a derived serde enum) so each failure gets its own reason.
pub fn parse_ext(text: &str, max: usize) -> Result<Untrusted<ExtMsg>, BadMsg> {
    if text.len() > max {
        return Err(BadMsg::TooBig);
    }
    let value: serde_json::Value = serde_json::from_str(text).map_err(|_| BadMsg::NotJson)?;
    let obj = value.as_object().ok_or(BadMsg::NotJson)?;
    let allowed: &[&str] = match obj.get("t").and_then(|t| t.as_str()) {
        Some("ping") => &["t", "v"],
        Some("pair") => &["t", "v", "code"],
        Some("auth") => &["t", "v", "proof", "cnonce"],
        _ => return Err(BadMsg::UnknownType),
    };
    if obj.keys().any(|k| !allowed.contains(&k.as_str())) {
        return Err(BadMsg::UnknownField);
    }
    if obj.get("v").and_then(|v| v.as_u64()) != Some(VERSION as u64) {
        return Err(BadMsg::BadVersion);
    }
    let field = |name: &str| obj.get(name).and_then(|v| v.as_str()).ok_or(BadMsg::BadField);
    let hex64 = |s: &str| s.len() == 64 && s.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'));
    let msg = match obj.get("t").and_then(|t| t.as_str()) {
        Some("pair") => {
            let code = field("code")?;
            if code.chars().count() > 16 {
                return Err(BadMsg::BadField);
            }
            ExtMsg::Pair { code: code.to_string() }
        }
        Some("auth") => {
            let (proof, cnonce) = (field("proof")?, field("cnonce")?);
            if !hex64(proof) || !hex64(cnonce) {
                return Err(BadMsg::BadField);
            }
            ExtMsg::Auth { proof: proof.to_string(), cnonce: cnonce.to_string() }
        }
        _ => ExtMsg::Ping,
    };
    Ok(Untrusted(msg))
}

/// What the app sends. No `Debug`: `Paired` and `AuthOk` carry secrets.
pub enum AppState {
    Hello { nonce: String },
    AuthOk { proof: String },
    Paired { token: String },
    WrongCode,
    Locked { retry_s: u32 },
    Expired,
    NotPairing,
    Rejected,
}

pub fn status_json(s: &AppState) -> String {
    use serde_json::json;
    let v = VERSION;
    match s {
        AppState::Hello { nonce } => json!({"t":"status","v":v,"state":"hello","nonce":nonce}),
        AppState::AuthOk { proof } => json!({"t":"status","v":v,"state":"auth_ok","proof":proof}),
        AppState::Paired { token } => json!({"t":"status","v":v,"state":"paired","token":token}),
        AppState::WrongCode => json!({"t":"status","v":v,"state":"wrong_code"}),
        AppState::Locked { retry_s } => json!({"t":"status","v":v,"state":"locked","retry_s":retry_s}),
        AppState::Expired => json!({"t":"status","v":v,"state":"expired"}),
        AppState::NotPairing => json!({"t":"status","v":v,"state":"not_pairing"}),
        AppState::Rejected => json!({"t":"status","v":v,"state":"rejected"}),
    }
    .to_string()
}

pub fn pong_json() -> String {
    serde_json::json!({"t":"pong","v":VERSION}).to_string()
}
