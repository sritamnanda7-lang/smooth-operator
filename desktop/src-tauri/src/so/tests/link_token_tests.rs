// SPDX-License-Identifier: FSL-1.1-MIT
//! Step 5 T1-T6 (+ nonce): the link token, the store, the proofs. Fakes only (no Keychain: that is X2 in
//! `link_keychain_real.rs`). Proof vectors are shared with tests/so/ext/proof.test.ts.
use super::link_support::*;
use crate::so::link::commands::{LinkState, LinkStatus};
use crate::so::link::pairing::{PairError, PairingView};
use crate::so::link::protocol::BadMsg;
use crate::so::link::server::HandshakeReject;
use crate::so::link::token::{
    client_proof, nonce_hex, server_proof, service, verify_client_proof, LinkToken, MemoryTokenStore, TokenStore, ACCOUNT, SERVICE_DEBUG,
    SERVICE_RELEASE,
};

const BYTES: [u8; 32] = [
    0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0a, 0x0b, 0x0c, 0x0d, 0x0e, 0x0f, 0x10, 0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17, 0x18, 0x19,
    0x1a, 0x1b, 0x1c, 0x1d, 0x1e, 0x1f,
];

// ---------- T1 ----------
#[test]
fn t1_generate_is_64_lowercase_hex_of_the_32_random_bytes() {
    let rng = FakeRng::queue(&[&BYTES]);
    let t = LinkToken::generate(&*rng).ok().unwrap_or_else(|| panic!("generate"));
    assert_eq!(t.to_hex(), kat_token());
    assert_eq!(rng.calls(), 1);
    assert!(LinkToken::generate(&*FakeRng::failing()).is_err(), "a failing random source fails closed");
    let other = LinkToken::generate(&*FakeRng::prng(9)).ok().unwrap().to_hex();
    assert!(is_hex64(&other), "{other}");
}

#[test]
fn t1_from_hex_accepts_exactly_64_lowercase_hex_and_round_trips() {
    assert_eq!(LinkToken::from_hex(&kat_token()).expect("valid").to_hex(), kat_token());
    let ok = "ab".repeat(32);
    assert!(LinkToken::from_hex(&ok).is_some());
    for bad in [
        &kat_token()[..63],
        &format!("{}0", kat_token()),
        &kat_token().to_uppercase(),
        &format!("g{}", &kat_token()[1..]),
        "",
        &format!(" {}", &kat_token()[1..]),
        &format!("{}\n", &kat_token()[..63]),
        &"é".repeat(32),
    ] {
        assert!(LinkToken::from_hex(bad).is_none(), "must reject {bad:?}");
    }
}

// ---------- T2 ----------
#[test]
fn t2_debug_never_shows_the_token() {
    let t = token_from(&kat_token());
    assert_eq!(format!("{t:?}"), "LinkToken(<redacted>)");
    assert_eq!(format!("{t:#?}"), "LinkToken(<redacted>)");
    let all = [
        format!("{t:?}"),
        format!("{:?}", LinkStatus { state: LinkState::Connected, pairing_secs_left: Some(3), locked_secs: None }),
        format!("{:?}", PairError::Locked { secs: 4 }),
        format!("{:?}", PairError::PortBusy),
        format!("{:?}", BadMsg::BadField),
        format!("{:?}", HandshakeReject::Origin),
    ];
    for s in &all {
        assert!(!s.contains(&kat_token()) && !s.contains(&hex(kat_token().as_bytes())), "{s}");
    }
    // PairingView has no Debug on purpose; it holds the code, so it must not be printable either.
    let _ = |v: &PairingView| v.code.len();
}

// ---------- T3 ----------
#[test]
fn t3_memory_store_set_get_and_set_again_replaces() {
    let s = MemoryTokenStore::new();
    assert!(matches!(s.get(), Ok(None)), "empty store reads Ok(None)");
    s.set(&token_from(&kat_token())).unwrap();
    assert_eq!(s.get().ok().flatten().expect("stored").to_hex(), kat_token());
    let b = "ab".repeat(32);
    s.set(&token_from(&b)).unwrap();
    assert_eq!(s.get().ok().flatten().unwrap().to_hex(), b, "the second set replaces the first");
}

// ---------- T4 ----------
#[test]
fn t4_proofs_equal_the_known_answers_made_with_openssl() {
    let t = token_from(&kat_token());
    assert_eq!(client_proof(&t, &kat_nonce()), KAT_CLIENT_PROOF, "HMAC-SHA256(raw token bytes, \"so-link-v1|ext|\" + nonce)");
    assert_eq!(server_proof(&t, &kat_cnonce()), KAT_SERVER_PROOF, "HMAC-SHA256(raw token bytes, \"so-link-v1|app|\" + cnonce)");
    // The two directions are not interchangeable.
    assert_ne!(client_proof(&t, &kat_nonce()), server_proof(&t, &kat_nonce()));
    // The support helper (independent hmac/sha2 code) agrees with the vectors too.
    assert_eq!(ext_proof(&kat_token(), &kat_nonce()), KAT_CLIENT_PROOF);
    assert_eq!(app_proof(&kat_token(), &kat_cnonce()), KAT_SERVER_PROOF);
}

// ---------- T5 ----------
#[test]
fn t5_verify_client_proof_accepts_the_right_proof_and_nothing_else() {
    let t = token_from(&kat_token());
    assert!(verify_client_proof(&t, &kat_nonce(), KAT_CLIENT_PROOF));
    // 1-bit change in every one of the 64 hex digits' first bit: flip the last nibble and the first.
    let mut flipped = KAT_CLIENT_PROOF.as_bytes().to_vec();
    flipped[63] = if flipped[63] == b'8' { b'9' } else { b'8' };
    assert!(!verify_client_proof(&t, &kat_nonce(), std::str::from_utf8(&flipped).unwrap()));
    let mut first = KAT_CLIENT_PROOF.as_bytes().to_vec();
    first[0] = if first[0] == b'1' { b'0' } else { b'1' };
    assert!(!verify_client_proof(&t, &kat_nonce(), std::str::from_utf8(&first).unwrap()));
    assert!(!verify_client_proof(&t, &kat_nonce(), &KAT_CLIENT_PROOF[..62]), "too short");
    assert!(!verify_client_proof(&t, &kat_nonce(), &format!("{KAT_CLIENT_PROOF}00")), "too long");
    assert!(!verify_client_proof(&t, &kat_nonce(), ""));
    assert!(!verify_client_proof(&t, &kat_nonce(), &KAT_CLIENT_PROOF.to_uppercase()), "uppercase hex is rejected");
    assert!(!verify_client_proof(&t, &kat_nonce(), KAT_SERVER_PROOF), "the app's proof is not a valid client proof");
    assert!(!verify_client_proof(&t, &kat_cnonce(), KAT_CLIENT_PROOF), "a proof for another nonce");
    assert!(!verify_client_proof(&token_from(&"ab".repeat(32)), &kat_nonce(), KAT_CLIENT_PROOF), "another token");
}

// ---------- T6 ----------
#[test]
fn t6_service_names() {
    assert_eq!(SERVICE_RELEASE, "com.sritamnanda.smoothoperator.link");
    assert_eq!(SERVICE_DEBUG, "com.sritamnanda.smoothoperator.link.dev");
    assert_eq!(ACCOUNT, "chrome");
    #[cfg(debug_assertions)]
    assert_eq!(service(), SERVICE_DEBUG, "debug builds use the .dev service");
    #[cfg(not(debug_assertions))]
    assert_eq!(service(), SERVICE_RELEASE);
}

// ---------- nonce ----------
#[test]
fn nonce_hex_is_32_random_bytes_as_64_hex() {
    assert_eq!(nonce_hex(&*FakeRng::queue(&[&BYTES])).unwrap(), kat_token());
    assert!(nonce_hex(&*FakeRng::failing()).is_err());
    let rng = FakeRng::prng(5);
    let (a, b) = (nonce_hex(&*rng).unwrap(), nonce_hex(&*rng).unwrap());
    assert!(is_hex64(&a) && is_hex64(&b));
    assert_ne!(a, b);
}
