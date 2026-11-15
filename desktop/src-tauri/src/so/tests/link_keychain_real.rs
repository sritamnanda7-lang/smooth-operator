// SPDX-License-Identifier: FSL-1.1-MIT
//! Step 5 X2: the REAL macOS Keychain for the link token. `#[ignore]`: run by hand on the founder Mac
//! (`cargo test --lib link_keychain_real -- --ignored --test-threads=1`). Uses its OWN unique service name
//! `com.sritamnanda.smoothoperator.link.test-<pid>-<nanos>`, deletes it afterwards (also on failure) and
//! never touches the release or debug link service. Never reads a secret with `security -w`.
use crate::so::link::token::{service, KeyringTokenStore, LinkToken, TokenStore, ACCOUNT, SERVICE_DEBUG, SERVICE_RELEASE};
use std::process::{Command, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

const SECURITY: &str = "/usr/bin/security";
const TOKEN: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
const TOKEN2: &str = "fedcba9876543210fedcba9876543210fedcba9876543210fedcba9876543210";

fn unique_service() -> String {
    let nanos = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
    let svc = format!("com.sritamnanda.smoothoperator.link.test-{}-{nanos}", std::process::id());
    assert!(svc != SERVICE_RELEASE && svc != SERVICE_DEBUG && svc != service(), "the test service must never be a real one");
    svc
}
struct Cleanup(String);
impl Drop for Cleanup {
    fn drop(&mut self) {
        let _ = Command::new(SECURITY).args(["delete-generic-password", "-s", &self.0, "-a", ACCOUNT]).stdout(Stdio::null()).stderr(Stdio::null()).status();
    }
}
/// Attributes only (no `-w`, no `-g`): exit 0 = the item exists in the real Keychain.
fn item_exists(svc: &str) -> bool {
    Command::new(SECURITY).args(["find-generic-password", "-s", svc, "-a", ACCOUNT]).stdout(Stdio::null()).stderr(Stdio::null()).status().expect("run /usr/bin/security").success()
}

#[test]
#[ignore]
fn x2_real_keychain_set_get_find_delete_for_the_link_token() {
    let svc = unique_service();
    let _cleanup = Cleanup(svc.clone());
    let store = KeyringTokenStore::new(&svc);
    assert!(!item_exists(&svc), "the unique test service must start empty");
    assert!(matches!(store.get(), Ok(None)), "NoEntry reads as Ok(None)");
    store.set(&LinkToken::from_hex(TOKEN).unwrap()).expect("set in the real Keychain");
    assert_eq!(store.get().ok().flatten().expect("get returns the token").to_hex(), TOKEN);
    assert!(item_exists(&svc), "`security find-generic-password -s <svc> -a chrome` must find it: this proves the real Keychain");
    store.set(&LinkToken::from_hex(TOKEN2).unwrap()).expect("set again replaces");
    assert_eq!(store.get().ok().flatten().unwrap().to_hex(), TOKEN2);
    // Cleanup through `security`, then the store must read "missing" again.
    let del = Command::new(SECURITY).args(["delete-generic-password", "-s", &svc, "-a", ACCOUNT]).stdout(Stdio::null()).stderr(Stdio::null()).status().unwrap();
    assert!(del.success(), "security delete-generic-password");
    assert!(!item_exists(&svc));
    assert!(matches!(store.get(), Ok(None)));
}
