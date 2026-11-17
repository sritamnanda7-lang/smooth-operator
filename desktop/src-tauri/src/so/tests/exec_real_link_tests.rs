// SPDX-License-Identifier: FSL-1.1-MIT
//! Step 6 final review: the REAL path from the executor to a real `Link` (fake token store, no socket needed).
//! `LinkBrowser::link_state` runs on the executor's plain thread and does ONE guarded token read before the first
//! gate: never paired => NotPaired (before Chrome is fronted, no 35 s wait); a read already in flight => NotFound
//! (never KeychainError); a failed read => KeychainError; a token with no session => NotFound (connecting).
//! A blocked Keychain read must not stop the executor from stopping (Stopped within 1 s).
use super::exec_support::*;
use super::link_support::{test_cfg, FakeRng, TestStore};
use super::rules_support::{FakeCatalog, FakeClock, FakeLauncher, FakeMedia, Log};
use crate::so::brain::keys::KeyError;
use crate::so::exec::executor::{Browser, Effects, Executor};
use crate::so::exec::real::LinkBrowser;
use crate::so::exec::Res;
use crate::so::hotkey::SystemClock;
use crate::so::link::commands::LinkState;
use crate::so::link::server::Link;
use std::sync::Arc;
use std::time::{Duration, Instant};

const TOK: &str = "abababababababababababababababababababababababababababababababab";

fn link(store: Arc<TestStore>) -> Arc<Link> {
    Link::new(test_cfg(500, 5000), store, FakeClock::new(), FakeRng::prng(1))
}
/// `link_state()` on a plain thread, `None` if it did not answer within `ms`.
fn state_within(b: &Arc<LinkBrowser>, ms: u64) -> Option<LinkState> {
    let (tx, rx) = std::sync::mpsc::channel();
    let b = b.clone();
    std::thread::spawn(move || {
        let _ = tx.send(b.link_state());
    });
    rx.recv_timeout(Duration::from_millis(ms)).ok()
}

#[test]
fn never_paired_is_not_paired_after_one_guarded_read_without_any_status_call() {
    let store = TestStore::new(None);
    let b = Arc::new(LinkBrowser(link(store.clone())));
    assert_eq!(state_within(&b, 2000), Some(LinkState::NotPaired));
    assert_eq!(store.gets(), 1, "exactly one guarded read");
    assert_eq!(state_within(&b, 2000), Some(LinkState::NotPaired));
    assert_eq!(store.gets(), 1, "the answer is remembered: no second read");
}

#[test]
fn a_token_with_no_session_is_not_found_a_failed_read_is_keychain_error() {
    let b = Arc::new(LinkBrowser(link(TestStore::new(Some(TOK)))));
    assert_eq!(state_within(&b, 2000), Some(LinkState::NotFound), "paired but not connected yet = connecting");
    let store = TestStore::new(Some(TOK));
    store.fail_get(Some(KeyError::Denied));
    let b = Arc::new(LinkBrowser(link(store)));
    assert_eq!(state_within(&b, 2000), Some(LinkState::KeychainError));
}

#[test]
fn a_read_already_in_flight_reads_as_not_found_never_keychain_error() {
    let store = TestStore::new(None);
    store.block_gets();
    let l = link(store.clone());
    let first = Arc::new(LinkBrowser(l.clone()));
    let f2 = first.clone();
    let t = std::thread::spawn(move || f2.link_state());
    let start = Instant::now();
    while store.gets() == 0 {
        assert!(start.elapsed() < Duration::from_secs(2), "the first read never started");
        std::thread::sleep(Duration::from_millis(5));
    }
    let second = Arc::new(LinkBrowser(l));
    let s = state_within(&second, 500);
    store.release();
    let _ = t.join();
    assert!(matches!(s, Some(x) if x != LinkState::KeychainError), "{s:?}");
    assert_eq!(s, Some(LinkState::NotFound));
    let start = Instant::now();
    while state_within(&second, 500) != Some(LinkState::NotPaired) {
        assert!(start.elapsed() < Duration::from_secs(2), "after the read finished the state must settle to NotPaired");
    }
}

fn real_exec(browser: Arc<dyn Browser>, clock: Arc<dyn crate::so::hotkey::watcher::Clock>) -> (Arc<Executor>, Arc<FakeChrome>) {
    let log = Arc::new(Log::default());
    let chrome = FakeChrome::new(log);
    let exec = Executor::new(
        Effects {
            browser,
            chrome: chrome.clone(),
            media: Arc::new(FakeMedia::default()),
            apps: FakeCatalog::new(vec![]),
            launcher: Arc::new(FakeLauncher::default()),
            wall: FakeWall::india(),
            clock,
            sleep: Arc::new(std::thread::sleep),
        },
        cfg(),
    );
    (exec, chrome)
}

#[test]
fn the_executor_on_a_real_never_paired_link_says_not_paired_before_fronting_chrome() {
    let b: Arc<dyn Browser> = Arc::new(LinkBrowser(link(TestStore::new(None))));
    let (exec, chrome) = real_exec(b, Arc::new(SystemClock));
    let t = Instant::now();
    let rep = exec.run(&[yt("a b")]);
    assert_eq!(rep.steps[0].res, Res::NotPaired, "not NotConnected after a 35 s wait");
    assert_eq!(chrome.fronts(), 0, "Chrome must not be touched");
    assert!(t.elapsed() < Duration::from_secs(5), "no link wait: {:?}", t.elapsed());
}

#[test]
fn a_blocked_keychain_read_does_not_stop_the_executor_from_stopping() {
    let store = TestStore::new(None);
    store.block_gets();
    let b: Arc<dyn Browser> = Arc::new(LinkBrowser(link(store.clone())));
    let (exec, _chrome) = real_exec(b, Arc::new(SystemClock));
    let (tx, rx) = std::sync::mpsc::channel();
    let e2 = exec.clone();
    std::thread::spawn(move || {
        let _ = tx.send(e2.run(&[yt("a b")]));
    });
    std::thread::sleep(Duration::from_millis(150));
    let t = Instant::now();
    exec.stop();
    let rep = rx.recv_timeout(Duration::from_secs(1));
    store.release();
    let rep = rep.expect("run must return within 1 s of stop() although the Keychain read is blocked");
    assert!(t.elapsed() < Duration::from_secs(1));
    assert_eq!(rep.steps[0].res, Res::Stopped);
}
