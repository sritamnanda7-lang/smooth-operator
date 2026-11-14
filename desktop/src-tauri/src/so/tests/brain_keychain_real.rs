// SPDX-License-Identifier: FSL-1.1-MIT
//! Step 4 X1/X2: the REAL macOS Keychain. All `#[ignore]`: run by hand on the founder Mac
//! (`cargo test --lib brain_keychain_real -- --ignored --test-threads=1`). Each test uses its OWN unique
//! service name and deletes its entries afterwards, even when it fails. It never touches the release or the
//! debug service (`keys::SERVICE_RELEASE`, `SERVICE_DEBUG`) and never reads a password with `security -w`.
use super::brain_support::key;
use crate::so::brain::keys::{service, KeyStore, KeyringStore, SERVICE_DEBUG, SERVICE_RELEASE};
use crate::so::brain::provider::ProviderId;
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

const SECURITY: &str = "/usr/bin/security";
const DUMMY: &str = "AIzaDummyDummyDummyDummyDummy0000";
const DUMMY2: &str = "AIzaOtherOtherOtherOtherOther1111";

fn unique_service() -> String {
    let nanos = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
    let svc = format!("com.sritamnanda.smoothoperator.test-{}-{nanos}", std::process::id());
    assert!(svc != SERVICE_RELEASE && svc != SERVICE_DEBUG && svc != service(), "the test service must never be a real one");
    svc
}

/// Deletes the test entries (both accounts) when dropped, pass or fail.
struct Cleanup(String);
impl Drop for Cleanup {
    fn drop(&mut self) {
        for account in ["claude", "gemini"] {
            let _ = Command::new(SECURITY)
                .args(["delete-generic-password", "-s", &self.0, "-a", account])
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status();
        }
    }
}

/// Attributes only (no `-w`, no `-g`): exit 0 = the item exists in the real Keychain.
fn item_exists(svc: &str, account: &str) -> bool {
    Command::new(SECURITY)
        .args(["find-generic-password", "-s", svc, "-a", account])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .expect("run /usr/bin/security")
        .success()
}

#[test]
#[ignore]
fn x1_real_keychain_set_get_find_delete() {
    let svc = unique_service();
    let _cleanup = Cleanup(svc.clone());
    let store = KeyringStore::new(&svc);
    assert!(!item_exists(&svc, "gemini"), "the unique test service must start empty");
    assert!(matches!(store.get(ProviderId::Gemini), Ok(None)), "NoEntry reads as Ok(None)");
    store.set(ProviderId::Gemini, &key(DUMMY)).expect("set in the real Keychain");
    let got = store.get(ProviderId::Gemini).ok().flatten().expect("get returns the key");
    assert_eq!(got.expose(), DUMMY);
    assert!(item_exists(&svc, "gemini"), "`security find-generic-password` must find it: this proves the real Keychain, not the mock store");
    assert!(!item_exists(&svc, "claude"), "accounts are separate");
    store.set(ProviderId::Gemini, &key(DUMMY2)).expect("overwrite");
    assert_eq!(store.get(ProviderId::Gemini).ok().flatten().expect("get").expose(), DUMMY2, "set replaces");
    store.delete(ProviderId::Gemini).expect("delete");
    assert!(matches!(store.get(ProviderId::Gemini), Ok(None)));
    assert!(store.delete(ProviderId::Gemini).is_ok(), "deleting a missing entry is Ok");
    assert!(!item_exists(&svc, "gemini"), "the item is gone from the real Keychain");
}

#[test]
#[ignore]
fn x2_two_stores_and_many_threads_on_one_service_do_not_fail() {
    let svc = unique_service();
    let _cleanup = Cleanup(svc.clone());
    let store = Arc::new(KeyringStore::new(&svc));
    store.set(ProviderId::Claude, &key(DUMMY)).expect("set");
    let mut hs = vec![];
    for i in 0..8 {
        let s = store.clone();
        hs.push(std::thread::spawn(move || {
            let k = if i % 2 == 0 { DUMMY } else { DUMMY2 };
            for _ in 0..5 {
                s.set(ProviderId::Claude, &key(k)).expect("concurrent set");
                assert!(s.get(ProviderId::Claude).ok().flatten().is_some(), "concurrent get");
            }
        }));
    }
    for h in hs {
        h.join().expect("no thread failed");
    }
    let other = KeyringStore::new(&svc);
    assert!(other.get(ProviderId::Claude).ok().flatten().is_some(), "a second store object sees the same item");
    store.delete(ProviderId::Claude).expect("delete");
}
