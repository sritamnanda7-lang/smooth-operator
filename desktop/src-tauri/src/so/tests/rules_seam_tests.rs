// SPDX-License-Identifier: FSL-1.1-MIT
//! Step 3 seam tests R1-R6, R8, R9 (R7 = step-2 tests unchanged, checked in step3_checks.rs).
//! Fakes only. Log tests run in a child process (see `rules_support`). Numbers follow draft/step-3-plan.md.
use super::rules_support::*;
use crate::so::hotkey::session::Mode::PushToTalk;
use crate::so::hotkey::session::TalkEvent::{Armed, Disarmed, Started, Stopped};
use crate::so::hotkey::watcher::TalkSink;
use crate::so::panel::{Kind, Line, PanelPayload};
use crate::so::rules::phrase::MediaKey;
use crate::so::rules::router::FastRouter;
use crate::so::rules::Route;
use crate::so::stt::sink::{DIDNT_CATCH_TEXT, SPEECH_FAILED_TEXT, TRANSCRIBING_TEXT};
use crate::so::stt::HEARD_SECS;
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

fn routed(reply: Route, text: &str) -> (Rig, Arc<FakeRouter>) {
    let log = Arc::new(Log::default());
    let router = FakeRouter::new(log.clone(), reply);
    let rig = Rig::with_router(FakeClock::new(), log, router.clone(), text);
    (rig, router)
}

// ---------- R1 ----------
#[test]
fn r1_a_transcript_is_routed_once_after_transcribe_and_the_panel_shows_heard_plus_the_router_lines() {
    let (rig, router) = routed(Route::Handled(vec![info("Play/pause sent")]), "pause");
    rig.clock.set(1000); // the clock when the talk ends
    rig.ptt();
    rig.wait_line("Play/pause sent");
    let shown_at = Instant::now();
    assert_eq!(router.texts(), vec!["pause"], "router called once, with the transcript text");
    let log = rig.log.all();
    let tr = log.iter().position(|e| e == "tr.transcribe").unwrap();
    let ro = log.iter().position(|e| e == "router.route").unwrap();
    assert!(tr < ro, "router must run after transcribe: {log:?}");
    assert_eq!(
        router.calls.lock().unwrap()[0].1,
        rig.clock.at(1000),
        "stopped_at is the clock reading at the Stopped event"
    );
    assert_eq!(
        rig.panel.last_payload().unwrap(),
        PanelPayload::new(vec![info("Heard: pause"), info("Play/pause sent")]),
        "Heard line, then the router's lines"
    );
    assert_eq!(rig.panel.hides(), 0, "still visible at once");
    wait_until("hide after 4 s", Duration::from_secs(6), || rig.panel.hides() == 1);
    let visible = shown_at.elapsed();
    assert!(visible >= Duration::from_millis(3500), "hidden too early: {visible:?}");
    assert!(visible <= Duration::from_millis(5500), "hidden too late: {visible:?}");
    assert_eq!(HEARD_SECS, 4);
}

// ---------- R2 ----------
#[test]
fn r2_pass_through_leaves_the_panel_exactly_as_in_step_2() {
    // Same expectations as step-2 T3, through `SpeechSink::new` (default router) and an explicit PassThrough.
    let rig = Rig::plain("hello world");
    rig.ptt();
    rig.wait_line("Heard: hello world");
    assert_eq!(rig.panel.texts(), vec!["Listening", TRANSCRIBING_TEXT, "Heard: hello world"]);
    assert_eq!(rig.panel.last_payload().unwrap(), PanelPayload::new(vec![info("Heard: hello world")]));

    let log = Arc::new(Log::default());
    let router = FakeRouter::new(log.clone(), Route::PassThrough);
    let rig2 = Rig::with_router(FakeClock::new(), log, router.clone(), "hello world");
    rig2.ptt();
    rig2.wait_line("Heard: hello world");
    assert_eq!(rig2.panel.texts(), rig.panel.texts(), "a PassThrough router changes nothing");
    assert_eq!(rig2.panel.last_payload(), rig.panel.last_payload());
    assert_eq!(router.texts(), vec!["hello world"], "positive control: the router was asked");
}

// ---------- R3 ----------
fn stale_rig() -> (Rig, Arc<FakeRouter>) {
    let (rig, router) = routed(Route::Handled(vec![info("Play/pause sent")]), "pause");
    *rig.tr.delay.lock().unwrap() = Duration::from_millis(800);
    rig.ptt(); // talk 1, transcribing for 800 ms
    wait("Transcribing", || rig.panel.texts().iter().any(|t| t == TRANSCRIBING_TEXT));
    rig.ev(Armed);
    rig.ev(Started(PushToTalk)); // talk 2
    wait("2nd Listening", || rig.panel.texts().iter().filter(|t| *t == "Listening").count() == 2);
    thread::sleep(Duration::from_millis(1200)); // talk 1's text arrives meanwhile
    (rig, router)
}

#[test]
fn r3_text_that_arrives_after_a_newer_talk_started_is_never_routed() {
    let (rig, router) = stale_rig();
    assert!(router.texts().is_empty(), "stale text must not be routed: {:?}", router.texts());
    assert!(rig.panel.all_line_texts().iter().all(|t| !t.starts_with("Heard") && t != "Play/pause sent"));
    assert_eq!(rig.panel.texts().last().unwrap(), "Listening", "talk 2's line stays");
}

#[test]
fn r3_a_current_talk_after_a_dropped_one_is_routed() {
    // Positive control: the drop is about staleness, not about this text.
    let (rig, router) = stale_rig();
    *rig.tr.delay.lock().unwrap() = Duration::ZERO;
    rig.ev(Stopped(PushToTalk)); // talk 2 ends
    rig.wait_line("Play/pause sent");
    assert_eq!(router.texts(), vec!["pause"]);
}

#[test]
fn r3_stale_text_is_logged_as_outcome_7() {
    run_isolated(module_path!(), "r3_inner");
}
#[test]
#[ignore]
fn r3_inner() {
    if !in_child() {
        return;
    }
    install_capture();
    let (_rig, router) = stale_rig();
    assert!(router.texts().is_empty());
    let logs = captured();
    assert!(
        logs.iter().any(|l| l.contains("fast_rule outcome=7 ms_from_stop=")),
        "no outcome 7 record for the dropped text: {logs:?}"
    );
    assert!(logs.iter().all(|l| !l.contains("fast_rule outcome=1 ")), "dropped text must not log an action");
}

// ---------- R4 ----------
#[test]
fn r4_blank_text_a_transcribe_error_or_no_samples_never_reach_the_router() {
    // blank
    let (rig, router) = routed(Route::Handled(vec![info("X")]), "   \n ");
    rig.ptt();
    rig.wait_line(DIDNT_CATCH_TEXT);
    // error
    let (rig2, router2) = routed(Route::Handled(vec![info("X")]), "x");
    *rig2.tr.result.lock().unwrap() = Err("boom".into());
    rig2.ptt();
    rig2.wait_line(SPEECH_FAILED_TEXT);
    // no samples
    let (rig3, router3) = routed(Route::Handled(vec![info("X")]), "x");
    *rig3.rec.samples.lock().unwrap() = None;
    rig3.ptt();
    rig3.wait_line(DIDNT_CATCH_TEXT);
    settle();
    assert!(router.texts().is_empty() && router2.texts().is_empty() && router3.texts().is_empty());
    assert_eq!(rig3.log.count("tr.transcribe"), 0, "no samples: no transcription either");
    // positive control: real text is routed with the same wiring
    let (rig4, router4) = routed(Route::PassThrough, "real words");
    rig4.ptt();
    rig4.wait_line("Heard: real words");
    assert_eq!(router4.texts(), vec!["real words"]);
}

// ---------- R5 ----------
#[test]
fn r5_a_handled_warn_line_is_shown_as_warn_with_no_error_detail() {
    let (rig, _router) = routed(Route::Handled(vec![warn("Couldn't open slack")]), "open slack");
    rig.ptt();
    rig.wait_line("Couldn't open slack");
    let p = rig.panel.last_payload().unwrap();
    assert_eq!(p.lines.len(), 2);
    assert_eq!(p.lines[0], Line::new("Heard: open slack", Kind::Info));
    assert_eq!(p.lines[1], Line::new("Couldn't open slack", Kind::Warn));
    let all = rig.panel.all_line_texts();
    assert!(all.iter().all(|t| !t.to_lowercase().contains("error") && !t.contains("Err(")), "{all:?}");
}

// ---------- R6: sentinels never reach the log ----------
#[test]
fn r6_sentinels_in_transcripts_and_app_names_never_reach_the_log_at_trace() {
    run_isolated(module_path!(), "r6_inner");
}
#[test]
#[ignore]
fn r6_inner() {
    if !in_child() {
        return;
    }
    install_capture();
    let clock = FakeClock::new();
    let media = Arc::new(FakeMedia::default());
    let launcher = Arc::new(FakeLauncher::default());
    let catalog = FakeCatalog::new(vec![
        app("Zxqsentinelapp", false),
        app("Google Zxqsentineldrive", false),
        app("Adobe Zxqsentineldrive", false),
    ]);
    let router = Arc::new(FastRouter::new(media.clone(), catalog, launcher.clone(), clock.clone()));
    let rig = Rig::with_router(clock, Arc::new(Log::default()), router, "pause");
    let say = |text: &str| {
        rig.say(text);
        rig.wait_line(&format!("Heard: {text}"));
    };
    say("pause"); // media, outcome 1
    say("open zxqsentinelapp"); // open, outcome 2
    say("open zxqsentinelnothing"); // not found, outcome 4
    say("open zxqsentineldrive"); // ambiguous via the vendor-trim tier (Google/Adobe), outcome 5
    *media.err.lock().unwrap() = Some("ZXQSENTINELERR".into());
    say("next"); // action failed, outcome 6
    *launcher.err.lock().unwrap() = Some("ZXQSENTINELERR /zxqpath".into());
    say("switch to zxqsentinelapp"); // action failed, outcome 6
    say("zxqsentineltranscript says hello"); // pass through
    settle();
    let logs = captured();
    // Positive control: the metrics line exists for each category, so a clean log means something.
    for code in [1, 2, 4, 5, 6] {
        assert!(logs.iter().any(|l| l.contains(&format!("fast_rule outcome={code} "))), "no outcome {code} record: {logs:?}");
    }
    for l in &logs {
        let low = l.to_lowercase();
        assert!(!low.contains("zxq"), "a sentinel reached the log: {l}");
        assert!(!low.contains("/applications/"), "an app path reached the log: {l}");
    }
}

// ---------- R8 ----------
#[test]
fn r8_on_event_returns_at_once_while_the_router_blocks() {
    let (rig, router) = routed(Route::Handled(vec![info("Play/pause sent")]), "pause");
    *router.delay.lock().unwrap() = Duration::from_millis(500);
    for e in [Armed, Started(PushToTalk), Stopped(PushToTalk), Armed, Disarmed] {
        let t = Instant::now();
        rig.sink.on_event(e);
        assert!(t.elapsed() < Duration::from_millis(5), "{e:?} took {:?}", t.elapsed());
    }
    wait("router called", || router.texts().len() == 1);
    let t = Instant::now();
    rig.sink.on_event(Armed);
    assert!(t.elapsed() < Duration::from_millis(5), "on_event blocked while the router was running");
}

// ---------- R9 ----------
#[test]
fn r9_ms_from_stop_is_the_fake_clock_time_from_stopped_to_action() {
    run_isolated(module_path!(), "r9_inner");
}
#[test]
#[ignore]
fn r9_inner() {
    if !in_child() {
        return;
    }
    install_capture();
    let clock = FakeClock::new();
    let media = Arc::new(FakeMedia::default());
    let router = Arc::new(FastRouter::new(
        media.clone(),
        FakeCatalog::new(vec![]),
        Arc::new(FakeLauncher::default()),
        clock.clone(),
    ));
    let rig = Rig::with_router(clock.clone(), Arc::new(Log::default()), router, "pause");
    *rig.tr.delay.lock().unwrap() = Duration::from_millis(150);
    clock.set(500);
    rig.ev(Armed);
    rig.ev(Started(PushToTalk));
    clock.set(2000);
    rig.ev(Stopped(PushToTalk)); // stopped_at = clock reading here (2000)
    clock.set(2137); // the action happens 137 ms later on the fake clock
    wait("media key pressed", || media.presses() == vec![MediaKey::PlayPause]);
    settle();
    let logs = captured();
    assert!(
        logs.iter().any(|l| l.contains("fast_rule outcome=1 ms_from_stop=137")),
        "expected `fast_rule outcome=1 ms_from_stop=137` in {logs:?}"
    );
}
