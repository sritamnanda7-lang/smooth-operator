// SPDX-License-Identifier: FSL-1.1-MIT
//! Step 5 protocol (pure, no socket): `parse_ext`, `status_json`, `pong_json`. Not a numbered plan test:
//! it pins the message shapes that W5/W6 and the extension's `parseApp` (E2) rely on.
use super::link_support::*;
use crate::so::link::protocol::*;

fn bad(text: &str) -> Option<BadMsg> {
    parse_ext(text, 4096).err()
}
fn good(text: &str) -> Untrusted<ExtMsg> {
    match parse_ext(text, 4096) {
        Ok(m) => m,
        Err(e) => panic!("{text} must parse, got {e:?}"),
    }
}
fn proof() -> String {
    "ab".repeat(32)
}

#[test]
fn constants() {
    assert_eq!(VERSION, 1);
    assert_eq!((CLOSE_PAIRED, CLOSE_BAD_MSG, CLOSE_AUTH, CLOSE_TIMEOUT, CLOSE_REPLACED), (4000, 4400, 4401, 4408, 4409));
}

#[test]
fn the_three_good_messages_parse() {
    assert!(matches!(good(r#"{"t":"ping","v":1}"#).get(), ExtMsg::Ping));
    assert!(matches!(good(r#"{"t":"pair","v":1,"code":"123456"}"#).get(), ExtMsg::Pair { code } if code == "123456"));
    let a = good(&auth_text(&proof(), &cnonce(9)));
    assert!(matches!(a.get(), ExtMsg::Auth { proof: p, cnonce: c } if *p == proof() && *c == cnonce(9)));
    // key order and whitespace do not matter
    assert!(matches!(good(r#"  { "v" : 1 , "t" : "ping" }  "#).get(), ExtMsg::Ping));
}

#[test]
fn a_pair_code_longer_than_16_chars_is_a_bad_field_16_is_fine() {
    assert_eq!(bad(&format!(r#"{{"t":"pair","v":1,"code":"{}"}}"#, "1".repeat(17))), Some(BadMsg::BadField));
    assert!(parse_ext(&format!(r#"{{"t":"pair","v":1,"code":"{}"}}"#, "1".repeat(16)), 4096).is_ok());
    // Not 6 digits is still a parseable pair message: `Pairing::try_code` counts it as a wrong try (P5).
    assert!(parse_ext(r#"{"t":"pair","v":1,"code":"abc"}"#, 4096).is_ok());
}

#[test]
fn each_failure_has_its_own_reason() {
    assert_eq!(bad("not json"), Some(BadMsg::NotJson));
    assert_eq!(bad(""), Some(BadMsg::NotJson));
    assert!(bad("[1,2]").is_some(), "an array is not a message");
    assert_eq!(bad(r#"{"t":"nope","v":1}"#), Some(BadMsg::UnknownType));
    assert_eq!(bad(r#"{"t":"ping","v":1,"extra":1}"#), Some(BadMsg::UnknownField));
    assert_eq!(bad(r#"{"t":"pair","v":1,"code":"123456","x":0}"#), Some(BadMsg::UnknownField));
    assert_eq!(bad(r#"{"t":"ping","v":2}"#), Some(BadMsg::BadVersion));
    assert!(bad(r#"{"t":"ping","v":"1"}"#).is_some(), "v must be the number 1");
    let p = proof();
    assert_eq!(bad(&auth_text(&p[..62], &cnonce(1))), Some(BadMsg::BadField), "short proof");
    assert_eq!(bad(&auth_text(&p.to_uppercase(), &cnonce(1))), Some(BadMsg::BadField), "uppercase proof");
    assert_eq!(bad(&auth_text(&p, &cnonce(1)[..63])), Some(BadMsg::BadField), "short cnonce");
    assert_eq!(bad(&auth_text(&p, &"zz".repeat(32))), Some(BadMsg::BadField), "non-hex cnonce");
    assert!(bad(r#"{"t":"auth","v":1,"proof":5,"cnonce":6}"#).is_some());
    assert!(bad(r#"{"t":"auth","v":1}"#).is_some(), "missing fields");
    assert!(bad(r#"{"t":"pair","v":1}"#).is_some(), "missing code");
}

#[test]
fn the_size_limit_is_checked_first_and_4096_is_allowed() {
    let pad = |n: usize| format!("{}{}", r#"{"t":"ping","v":1}"#, " ".repeat(n - 18));
    assert_eq!(pad(4096).len(), 4096);
    assert!(parse_ext(&pad(4096), 4096).is_ok(), "exactly the limit is accepted");
    assert_eq!(parse_ext(&pad(4097), 4096).err(), Some(BadMsg::TooBig));
    assert_eq!(parse_ext(&"x".repeat(5000), 4096).err(), Some(BadMsg::TooBig), "size beats syntax");
    assert_eq!(parse_ext(&pad(100), 99).err(), Some(BadMsg::TooBig), "the limit is a parameter");
}

fn j(s: &AppState) -> serde_json::Value {
    serde_json::from_str(&status_json(s)).expect("valid JSON")
}

#[test]
fn status_json_shapes() {
    let n = hex(&[7u8; 32]);
    assert_eq!(j(&AppState::Hello { nonce: n.clone() }), serde_json::json!({"t":"status","v":1,"state":"hello","nonce":n}));
    assert_eq!(j(&AppState::AuthOk { proof: proof() }), serde_json::json!({"t":"status","v":1,"state":"auth_ok","proof":proof()}));
    assert_eq!(j(&AppState::Paired { token: KAT_TOKEN.into() }), serde_json::json!({"t":"status","v":1,"state":"paired","token":KAT_TOKEN}));
    assert_eq!(j(&AppState::WrongCode), serde_json::json!({"t":"status","v":1,"state":"wrong_code"}));
    assert_eq!(j(&AppState::Locked { retry_s: 60 }), serde_json::json!({"t":"status","v":1,"state":"locked","retry_s":60}));
    assert_eq!(j(&AppState::Expired), serde_json::json!({"t":"status","v":1,"state":"expired"}));
    assert_eq!(j(&AppState::NotPairing), serde_json::json!({"t":"status","v":1,"state":"not_pairing"}));
    assert_eq!(j(&AppState::Rejected), serde_json::json!({"t":"status","v":1,"state":"rejected"}));
    assert_eq!(serde_json::from_str::<serde_json::Value>(&pong_json()).unwrap(), serde_json::json!({"t":"pong","v":1}));
}
