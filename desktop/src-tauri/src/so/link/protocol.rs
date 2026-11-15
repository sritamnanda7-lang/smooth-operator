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

// Stub phase: nothing in the app calls these yet. This reference keeps them compiled without a
// dead-code warning and without an allow (strip-check s05).
const _: () = {
    let _ = VERSION;
    let _ = (CLOSE_PAIRED, CLOSE_BAD_MSG, CLOSE_AUTH, CLOSE_TIMEOUT, CLOSE_REPLACED);
    let _ = Untrusted::<ExtMsg>::get;
    std::mem::forget(ExtMsg::Pair { code: String::new() });
    std::mem::forget(ExtMsg::Auth { proof: String::new(), cnonce: String::new() });
    let _ = ExtMsg::Ping;
    let _ = (
        BadMsg::TooBig,
        BadMsg::NotJson,
        BadMsg::UnknownType,
        BadMsg::UnknownField,
        BadMsg::BadVersion,
        BadMsg::BadField,
    );
    let _ = parse_ext;
    // Reads the fields so rustc sees them used; real code replaces this in increment 2.
    let _ = |m: &ExtMsg| match m {
        ExtMsg::Pair { code } => code.len(),
        ExtMsg::Auth { proof, cnonce } => proof.len() + cnonce.len(),
        ExtMsg::Ping => 0,
    };
    let _ = |s: &AppState| match s {
        AppState::Hello { nonce } => nonce.len(),
        AppState::AuthOk { proof } => proof.len(),
        AppState::Paired { token } => token.len(),
        AppState::Locked { retry_s } => *retry_s as usize,
        AppState::WrongCode | AppState::Expired | AppState::NotPairing | AppState::Rejected => 0,
    };
    std::mem::forget(AppState::Hello { nonce: String::new() });
    std::mem::forget(AppState::AuthOk { proof: String::new() });
    std::mem::forget(AppState::Paired { token: String::new() });
    let _ = (AppState::WrongCode, AppState::Expired, AppState::NotPairing, AppState::Rejected);
    let _ = AppState::Locked { retry_s: 0 };
    let _ = status_json;
    let _ = pong_json;
};
