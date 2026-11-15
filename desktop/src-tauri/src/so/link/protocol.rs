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

pub fn parse_ext(_text: &str, _max: usize) -> Result<Untrusted<ExtMsg>, BadMsg> {
    todo!()
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

pub fn status_json(_s: &AppState) -> String {
    todo!()
}

pub fn pong_json() -> String {
    todo!()
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
