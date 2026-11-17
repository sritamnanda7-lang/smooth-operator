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
    Result { id: u32, r: ResultCode, ms: u32 },
}

/// The 14 result codes the extension may answer with (step 6). Never page text, titles, URLs or ids.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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
pub enum Command {
    YtPlay { url: String },
    DocsOpen,
    CalOpen { url: String },
}

pub const MAX_CMD_ID: u32 = 2_147_483_647;
pub const MAX_RESULT_MS: u32 = 120_000;

/// `{"t":"cmd","v":1,"id":N,"op":...}`. Built by serde_json, so quotes and backslashes in a URL are escaped.
pub fn cmd_json(id: u32, c: &Command) -> String {
    use serde_json::json;
    let v = VERSION;
    match c {
        Command::YtPlay { url } => json!({"t":"cmd","v":v,"id":id,"op":"yt_play","url":url}),
        Command::DocsOpen => json!({"t":"cmd","v":v,"id":id,"op":"docs_open"}),
        Command::CalOpen { url } => json!({"t":"cmd","v":v,"id":id,"op":"cal_open","url":url}),
    }
    .to_string()
}

/// `{"t":"cancel","v":1,"id":N}`.
pub fn cancel_json(id: u32) -> String {
    serde_json::json!({"t":"cancel","v":VERSION,"id":id}).to_string()
}

impl ResultCode {
    /// Exactly the 14 fixed names; anything else (also other case) is refused.
    fn from_name(s: &str) -> Option<Self> {
        Some(match s {
            "playing" => Self::Playing,
            "ad" => Self::Ad,
            "paused" => Self::Paused,
            "no_video" => Self::NoVideo,
            "editor_open" => Self::EditorOpen,
            "needs_login" => Self::NeedsLogin,
            "consent" => Self::Consent,
            "no_access" => Self::NoAccess,
            "timeout" => Self::Timeout,
            "bad_cmd" => Self::BadCmd,
            "busy" => Self::Busy,
            "unsafe" => Self::Unsafe,
            "cancelled" => Self::Cancelled,
            "error" => Self::Error,
            _ => return None,
        })
    }
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
        Some("res") => &["t", "v", "id", "r", "ms"],
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
        Some("res") => {
            // as_u64 refuses negatives, floats, strings and null.
            let num = |name: &str| obj.get(name).and_then(|v| v.as_u64()).and_then(|n| u32::try_from(n).ok()).ok_or(BadMsg::BadField);
            let (id, ms) = (num("id")?, num("ms")?);
            let r = ResultCode::from_name(field("r")?).ok_or(BadMsg::BadField)?;
            if !(1..=MAX_CMD_ID).contains(&id) || ms > MAX_RESULT_MS {
                return Err(BadMsg::BadField);
            }
            ExtMsg::Result { id, r, ms }
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
