// SPDX-License-Identifier: FSL-1.1-MIT
//! Step 3 debug test input: I1-I6 (I7 = static, in step3_checks.rs; I8, I9 = rules_release_tests.rs).
//! The whole file is compiled in debug builds only, like the module it tests. A real Unix socket in a
//! temp folder is used (no other OS call). Log tests run in a child process (see `rules_support`).
//! Numbers follow draft/step-3-plan.md.
use super::rules_support::*;
use crate::so::hotkey::session::Mode::PushToTalk;
use crate::so::hotkey::session::TalkEvent::{Armed, Started};
use crate::so::rules::debug_input::{start, MAX_LINE_BYTES, SOCKET_DIR, SOCKET_NAME};
use crate::so::rules::phrase::MediaKey;
use crate::so::rules::router::FastRouter;
use crate::so::rules::Route;
use crate::so::panel::PanelPayload;
use std::io::{Read, Write};
use std::os::unix::fs::{FileTypeExt, PermissionsExt};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

type Injected = Arc<Mutex<Vec<String>>>;

fn sock(dir: &Path) -> PathBuf {
    dir.join(SOCKET_DIR).join(SOCKET_NAME)
}

/// A recording `inject` closure.
fn recorder() -> (Injected, Arc<dyn Fn(String) + Send + Sync>) {
    let seen: Injected = Arc::new(Mutex::new(vec![]));
    let s = seen.clone();
    (seen, Arc::new(move |t| s.lock().unwrap().push(t)))
}

fn tmp() -> tempfile::TempDir {
    let t = tempfile::tempdir().unwrap();
    // The macOS limit for a socket path is 104 bytes: the plan's own path is about 85.
    assert!(sock(t.path()).as_os_str().len() < 100, "temp dir too deep for a Unix socket: {:?}", sock(t.path()));
    t
}

fn connect(dir: &Path) -> UnixStream {
    let path = sock(dir);
    let t = Instant::now();
    loop {
        match UnixStream::connect(&path) {
            Ok(s) => return s,
            Err(e) if t.elapsed() > Duration::from_secs(2) => panic!("cannot connect to {path:?}: {e}"),
            Err(_) => thread::sleep(Duration::from_millis(10)),
        }
    }
}

/// Sends `bytes`, keeps the write side open if `half_close` is false, reads the reply until the peer
/// closes, a newline arrives, or `max` passes. Returns what came back (lossy) and how long it took.
fn talk(dir: &Path, bytes: &[u8], half_close: bool, max: Duration) -> (String, Duration) {
    let mut s = connect(dir);
    let t = Instant::now();
    let _ = s.write_all(bytes);
    if half_close {
        let _ = s.shutdown(std::net::Shutdown::Write);
    }
    s.set_read_timeout(Some(max)).unwrap();
    let mut out = Vec::new();
    let mut buf = [0u8; 64];
    while t.elapsed() < max {
        match s.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => {
                out.extend_from_slice(&buf[..n]);
                if out.contains(&b'\n') {
                    break;
                }
            }
            Err(_) => break,
        }
    }
    (String::from_utf8_lossy(&out).to_string(), t.elapsed())
}

fn rejected(reply: &str) -> bool {
    reply == "err\n" || reply.is_empty() // "err\n" or just closed
}

// ---------- I1 ----------
#[test]
fn i1_disabled_starts_nothing_and_creates_nothing() {
    let dir = tmp();
    let (seen, inject) = recorder();
    assert!(start(dir.path(), false, inject).is_none());
    thread::sleep(Duration::from_millis(100));
    assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0, "no folder, no socket");
    assert!(seen.lock().unwrap().is_empty());
}

// ---------- I2 ----------
#[test]
fn i2_enabled_makes_a_0700_folder_and_a_0600_socket() {
    let dir = tmp();
    let (_seen, inject) = recorder();
    let handle = start(dir.path(), true, inject);
    assert!(handle.is_some(), "start returns the listener thread");
    let s = sock(dir.path());
    wait("socket exists", || s.exists());
    let folder = std::fs::metadata(dir.path().join(SOCKET_DIR)).unwrap();
    assert!(folder.is_dir());
    assert_eq!(folder.permissions().mode() & 0o777, 0o700, "folder mode");
    let meta = std::fs::metadata(&s).unwrap();
    assert!(meta.file_type().is_socket(), "input.sock must be a Unix socket");
    assert_eq!(meta.permissions().mode() & 0o777, 0o600, "socket mode");
}

#[test]
fn i2_a_stale_socket_is_replaced_at_start() {
    let dir = tmp();
    // A real stale socket: bound, then abandoned (the file stays behind, nobody listens).
    std::fs::create_dir_all(dir.path().join(SOCKET_DIR)).unwrap();
    drop(std::os::unix::net::UnixListener::bind(sock(dir.path())).unwrap());
    assert!(sock(dir.path()).exists());
    assert!(UnixStream::connect(sock(dir.path())).is_err(), "stale: nobody listens");
    let (seen, inject) = recorder();
    assert!(start(dir.path(), true, inject).is_some(), "a stale socket must not stop the start");
    let (reply, _) = talk(dir.path(), b"pause\n", true, Duration::from_secs(3));
    assert_eq!(reply, "ok\n");
    wait("injected", || seen.lock().unwrap().len() == 1);
}

// ---------- I3 ----------
fn injected_rig(dir: &Path, router: Arc<dyn crate::so::rules::TranscriptRouter>) -> Rig {
    let rig = Rig::with_router(FakeClock::new(), Arc::new(Log::default()), router, "pause");
    let sink = rig.sink.clone();
    assert!(start(dir, true, Arc::new(move |t| sink.inject_transcript(t))).is_some());
    rig
}

#[test]
fn i3_a_typed_phrase_enters_like_speech_and_the_reply_is_ok() {
    let dir = tmp();
    let reply_lines = vec![info("Play/pause sent")];
    // Speech reference.
    let log = Arc::new(Log::default());
    let r_speech = FakeRouter::new(log.clone(), Route::Handled(reply_lines.clone()));
    let speech = Rig::with_router(FakeClock::new(), log, r_speech.clone(), "pause");
    speech.ptt();
    speech.wait_line("Play/pause sent");
    // Same router through the socket.
    let router = FakeRouter::new(Arc::new(Log::default()), Route::Handled(reply_lines));
    let rig = injected_rig(dir.path(), router.clone());
    let (reply, _) = talk(dir.path(), b"pause\n", true, Duration::from_secs(3));
    assert_eq!(reply, "ok\n", "plain ok, never an echo");
    rig.wait_line("Play/pause sent");
    settle();
    assert_eq!(router.texts(), vec!["pause"], "router called once with the phrase");
    assert_eq!(rig.panel.last_payload(), speech.panel.last_payload(), "same panel lines as the speech path");
    assert_eq!(rig.panel.last_payload().unwrap(), PanelPayload::new(vec![info("Heard: pause"), info("Play/pause sent")]));
    assert!(rig.panel.all_line_texts().iter().all(|t| t != "Listening"), "no Listening cue for typed text");
    // Hidden after 4 s like speech.
    wait_until("hide after 4 s", Duration::from_secs(6), || rig.panel.hides() == 1);
}

#[test]
fn i3_the_phrase_is_cleaned_by_the_same_heard_line_rules_and_a_client_that_keeps_its_end_open_works() {
    let dir = tmp();
    let router = FakeRouter::new(Arc::new(Log::default()), Route::PassThrough);
    let rig = injected_rig(dir.path(), router.clone());
    // `printf 'x\n' | nc -U` closes its write end; a plain client may not.
    let (reply, _) = talk(dir.path(), "caf\u{E9} pause\n".as_bytes(), false, Duration::from_secs(3));
    assert_eq!(reply, "ok\n");
    rig.wait_line("Heard: caf\u{E9} pause");
    assert_eq!(router.texts(), vec!["caf\u{E9} pause"]);
}

#[test]
fn i3_a_typed_phrase_reaches_the_real_fast_router_and_presses_the_key() {
    // Positive control: the whole chain with fakes only at the edges.
    let dir = tmp();
    let clock = FakeClock::new();
    let media = Arc::new(FakeMedia::default());
    let router = Arc::new(FastRouter::new(media.clone(), FakeCatalog::new(vec![]), Arc::new(FakeLauncher::default()), clock.clone()));
    let rig = Rig::with_router(clock, Arc::new(Log::default()), router, "x");
    let sink = rig.sink.clone();
    assert!(start(dir.path(), true, Arc::new(move |t| sink.inject_transcript(t))).is_some());
    let (reply, _) = talk(dir.path(), b"next track\n", true, Duration::from_secs(3));
    assert_eq!(reply, "ok\n");
    wait("key pressed", || media.presses() == vec![MediaKey::Next]);
    rig.wait_line("Next track");
}

#[test]
fn i3_typed_and_spoken_phrases_log_the_same_outcome_code() {
    run_isolated(module_path!(), "i3_outcome_inner");
}
#[test]
#[ignore]
fn i3_outcome_inner() {
    if !in_child() {
        return;
    }
    install_capture();
    let dir = tmp();
    let clock = FakeClock::new();
    let media = Arc::new(FakeMedia::default());
    let router = Arc::new(FastRouter::new(media.clone(), FakeCatalog::new(vec![]), Arc::new(FakeLauncher::default()), clock.clone()));
    let rig = Rig::with_router(clock, Arc::new(Log::default()), router, "pause");
    let sink = rig.sink.clone();
    assert!(start(dir.path(), true, Arc::new(move |t| sink.inject_transcript(t))).is_some());
    rig.ptt(); // spoken "pause"
    wait("spoken key", || media.presses().len() == 1);
    let (reply, _) = talk(dir.path(), b"pause\n", true, Duration::from_secs(3)); // typed "pause"
    assert_eq!(reply, "ok\n");
    wait("typed key", || media.presses().len() == 2);
    settle();
    let n = captured().iter().filter(|l| l.contains("fast_rule outcome=1 ")).count();
    assert_eq!(n, 2, "spoken and typed 'pause' must both log outcome 1: {:?}", captured());
    assert!(captured().iter().any(|l| l.contains("debug_input listening=true")), "start logs listening=true");
}

// ---------- I4 ----------
#[test]
fn i4_an_injected_phrase_makes_an_in_flight_speech_text_stale() {
    let dir = tmp();
    let router = FakeRouter::new(Arc::new(Log::default()), Route::Handled(vec![info("Play/pause sent")]));
    let rig = injected_rig(dir.path(), router.clone());
    *rig.tr.result.lock().unwrap() = Ok("spoken words".into());
    *rig.tr.delay.lock().unwrap() = Duration::from_millis(800);
    rig.ptt(); // talk 1: transcribing for 800 ms
    wait("Transcribing", || rig.panel.texts().iter().any(|t| t.starts_with("Transcribing")));
    let (reply, _) = talk(dir.path(), b"pause\n", true, Duration::from_secs(3)); // talk 2 by typing
    assert_eq!(reply, "ok\n");
    rig.wait_line("Play/pause sent");
    thread::sleep(Duration::from_millis(1100)); // the spoken text arrives meanwhile
    assert_eq!(router.texts(), vec!["pause"], "the spoken text is stale: never routed");
    assert!(rig.panel.all_line_texts().iter().all(|t| t != "Heard: spoken words"));
}

#[test]
fn i4_a_speech_talk_that_starts_while_the_injected_phrase_is_routing_owns_the_panel() {
    // The window between the stale check and the action is microseconds, so the action itself cannot be
    // held back deterministically (plan: harmless, both are Safe). What must hold: the older typed
    // phrase never writes over the newer talk's panel.
    let dir = tmp();
    let router = FakeRouter::new(Arc::new(Log::default()), Route::Handled(vec![info("Play/pause sent")]));
    *router.delay.lock().unwrap() = Duration::from_millis(600);
    let rig = injected_rig(dir.path(), router.clone());
    let (reply, _) = talk(dir.path(), b"pause\n", true, Duration::from_secs(3));
    assert_eq!(reply, "ok\n");
    wait("router entered", || router.texts().len() == 1);
    rig.ev(Armed);
    rig.ev(Started(PushToTalk)); // newer speech talk
    wait("Listening", || rig.panel.texts().iter().any(|t| t == "Listening"));
    thread::sleep(Duration::from_millis(900)); // the router returns meanwhile
    assert_eq!(rig.panel.texts().last().unwrap(), "Listening", "the newest talk owns the panel: {:?}", rig.panel.texts());
    assert!(rig.panel.all_line_texts().iter().all(|t| t != "Play/pause sent"));
}

// ---------- I5 ----------
#[test]
fn i5_bad_lines_are_rejected_without_routing_and_the_listener_keeps_working() {
    let dir = tmp();
    let (seen, inject) = recorder();
    assert!(start(dir.path(), true, inject).is_some());
    let long = format!("{}\n", "a".repeat(MAX_LINE_BYTES + 1));
    for (what, bytes) in [
        ("201 bytes", long.as_bytes().to_vec()),
        ("invalid utf-8", b"pa\xff\xfeuse\n".to_vec()),
        ("empty line", b"\n".to_vec()),
        ("1000 bytes, no newline", vec![b'a'; 1000]),
    ] {
        let (reply, _) = talk(dir.path(), &bytes, true, Duration::from_secs(3));
        assert!(rejected(&reply), "{what}: got {reply:?}");
    }
    // No newline within 2 s: closed with err (or just closed), not held open for ever.
    let (reply, took) = talk(dir.path(), b"pau", false, Duration::from_secs(4));
    assert!(rejected(&reply), "no newline: got {reply:?}");
    assert!(took >= Duration::from_millis(1500) && took < Duration::from_millis(3500), "2 s read timeout, took {took:?}");
    assert!(seen.lock().unwrap().is_empty(), "nothing routed: {:?}", seen.lock().unwrap());
    // Boundary and recovery: exactly 200 bytes is fine, and the listener is still alive.
    let ok200 = format!("{}\n", "a".repeat(MAX_LINE_BYTES));
    let (reply, _) = talk(dir.path(), ok200.as_bytes(), true, Duration::from_secs(3));
    assert_eq!(reply, "ok\n", "a 200-byte line is allowed");
    let (reply, _) = talk(dir.path(), b"pause\n", true, Duration::from_secs(3));
    assert_eq!(reply, "ok\n");
    wait("both accepted lines injected", || seen.lock().unwrap().len() == 2);
    assert_eq!(seen.lock().unwrap()[1], "pause");
}

#[test]
fn i5_each_reject_is_logged_as_a_code_and_never_as_text() {
    run_isolated(module_path!(), "i5_log_inner");
}
#[test]
#[ignore]
fn i5_log_inner() {
    if !in_child() {
        return;
    }
    install_capture();
    let dir = tmp();
    let (_seen, inject) = recorder();
    assert!(start(dir.path(), true, inject).is_some());
    let long = format!("{}\n", "z".repeat(MAX_LINE_BYTES + 1));
    for bytes in [long.as_bytes().to_vec(), b"\xff\xfe\n".to_vec(), b"\n".to_vec()] {
        let _ = talk(dir.path(), &bytes, true, Duration::from_secs(3));
    }
    let _ = talk(dir.path(), b"pau", false, Duration::from_secs(4)); // timeout
    settle();
    let logs = captured();
    for code in [1, 2, 3, 5] {
        assert!(logs.iter().any(|l| l.contains(&format!("debug_input listening=true rejected={code}"))), "no rejected={code} record: {logs:?}");
    }
    assert!(logs.iter().all(|l| !l.contains("zzzz") && !l.contains("pau\n")), "typed text reached the log");
}

// ---------- I6 ----------
#[test]
fn i6_a_sentinel_phrase_never_reaches_the_log_or_the_disk() {
    run_isolated(module_path!(), "i6_inner");
}
#[test]
#[ignore]
fn i6_inner() {
    if !in_child() {
        return;
    }
    install_capture();
    let dir = tmp();
    let clock = FakeClock::new();
    let router = Arc::new(FastRouter::new(
        Arc::new(FakeMedia::default()),
        FakeCatalog::new(vec![app("Zxqsentinelapp", false)]),
        Arc::new(FakeLauncher::default()),
        clock.clone(),
    ));
    let rig = Rig::with_router(clock, Arc::new(Log::default()), router, "x");
    let sink = rig.sink.clone();
    assert!(start(dir.path(), true, Arc::new(move |t| sink.inject_transcript(t))).is_some());
    for phrase in ["zxqsentinelphrase says hello", "open zxqsentinelapp", "open zxqsentinelmissing"] {
        let (reply, _) = talk(dir.path(), format!("{phrase}\n").as_bytes(), true, Duration::from_secs(3));
        assert_eq!(reply, "ok\n");
        rig.wait_line(&format!("Heard: {phrase}")); // positive control: the phrase really went through
    }
    settle();
    let logs = captured();
    assert!(logs.iter().any(|l| l.contains("debug_input listening=true")), "positive control: the log capture works");
    assert!(logs.iter().any(|l| l.contains("fast_rule outcome=2 ")), "positive control: the open was routed");
    assert!(logs.iter().all(|l| !l.to_lowercase().contains("zxq")), "a sentinel reached the log: {logs:?}");
    // Nothing on disk except the folder and the socket.
    let mut found = vec![];
    for e in std::fs::read_dir(dir.path()).unwrap() {
        let e = e.unwrap();
        found.push(e.file_name().to_string_lossy().to_string());
        if e.file_type().unwrap().is_dir() {
            for f in std::fs::read_dir(e.path()).unwrap() {
                found.push(format!("{}/{}", e.file_name().to_string_lossy(), f.unwrap().file_name().to_string_lossy()));
            }
        }
    }
    found.sort();
    assert_eq!(found, vec![SOCKET_DIR.to_string(), format!("{SOCKET_DIR}/{SOCKET_NAME}")], "files created: {found:?}");
}
