// SPDX-License-Identifier: FSL-1.1-MIT
//! Step 4 seam tests N1-N8: how a talk reaches the brain (`SpeechSink::with_brain`, `not_found_to_brain`).
//! Fakes only (`FakeBrain`, the step-3 rig fakes). Log tests run in a child process (see `rules_support`).
use super::brain_support::*;
use super::rules_support::*;
use crate::so::hotkey::session::Mode::PushToTalk;
use crate::so::hotkey::session::TalkEvent::{Armed, Started, Stopped};
use crate::so::hotkey::watcher::TalkSink;
use crate::so::panel::{Kind, Line, PanelPayload};
use crate::so::rules::phrase::MediaKey;
use crate::so::rules::router::FastRouter;
use crate::so::rules::{Reply, Route, THINKING_TEXT};
use crate::so::stt::sink::TRANSCRIBING_TEXT;
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

fn plan_reply(hold: u64) -> Reply {
    reply(vec![info("Plan, not run (2 steps):"), info("Play on YouTube: Blinding Lights")], hold)
}
const PLAN_LINE: &str = "Play on YouTube: Blinding Lights";

/// A rig whose router passes everything through to the brain.
fn pass_rig(brain: Arc<FakeBrain>, text: &str) -> Rig {
    let log = Arc::new(Log::default());
    let router = FakeRouter::new(log.clone(), Route::PassThrough);
    brain_rig(FakeClock::new(), log, router, brain, text)
}
fn payload_texts(p: &PanelPayload) -> Vec<String> {
    p.lines.iter().map(|l| l.text.clone()).collect()
}

// ---------- N3: Thinking at once, then the plan ----------
#[test]
fn n3_a_two_word_phrase_shows_thinking_at_once_then_heard_plus_the_reply_lines() {
    let brain = FakeBrain::new(plan_reply(12));
    *brain.delay.lock().unwrap() = Duration::from_millis(300);
    let rig = pass_rig(brain.clone(), "play blinding");
    rig.ptt();
    rig.wait_line(THINKING_TEXT);
    let thinking_at = Instant::now();
    assert!(brain.calls.lock().unwrap().len() <= 1);
    rig.wait_line(PLAN_LINE);
    assert!(thinking_at.elapsed() >= Duration::from_millis(200), "the plan came before the brain finished");
    let shows: Vec<Vec<String>> = rig.panel.shows().iter().map(payload_texts).collect();
    let t = shows.iter().position(|s| *s == vec!["Heard: play blinding".to_string(), THINKING_TEXT.to_string()]).expect("a Heard + Thinking… payload");
    let f = shows.iter().position(|s| s.last().map(String::as_str) == Some(PLAN_LINE)).expect("the plan payload");
    assert!(t < f, "Thinking… first: {shows:?}");
    assert_eq!(rig.panel.last_payload().unwrap(), PanelPayload::new(vec![info("Heard: play blinding"), info("Plan, not run (2 steps):"), info(PLAN_LINE)]));
    assert_eq!(brain.texts(), vec!["play blinding"]);
}

#[test]
fn n3_a_warn_reply_keeps_its_kind() {
    let brain = FakeBrain::new(reply(vec![warn("No internet: the AI can't be reached")], 6));
    let rig = pass_rig(brain, "open youtube");
    rig.ptt();
    rig.wait_line("No internet: the AI can't be reached");
    let p = rig.panel.last_payload().unwrap();
    assert_eq!(p.lines, vec![Line::new("Heard: open youtube", Kind::Info), Line::new("No internet: the AI can't be reached", Kind::Warn)]);
}

#[test]
fn n3_a_one_word_phrase_shows_heard_only_and_never_calls_the_brain() {
    let brain = FakeBrain::new(plan_reply(12));
    let rig = pass_rig(brain.clone(), "paws");
    rig.ptt();
    rig.wait_line("Heard: paws");
    let shown_at = Instant::now();
    settle();
    assert!(brain.wants_calls.load(std::sync::atomic::Ordering::SeqCst) >= 1, "positive control: the seam asked wants()");
    assert!(brain.texts().is_empty(), "think was called for one word");
    assert_eq!(rig.panel.texts(), vec!["Listening", TRANSCRIBING_TEXT, "Heard: paws"]);
    assert!(rig.panel.all_line_texts().iter().all(|t| t != THINKING_TEXT));
    wait_until("hide after 4 s", Duration::from_secs(6), || rig.panel.hides() == 1);
    let v = shown_at.elapsed();
    assert!(v >= Duration::from_millis(3500) && v <= Duration::from_millis(5500), "{v:?}");
}

#[test]
fn n3_the_seam_obeys_wants_not_its_own_word_count() {
    let brain = FakeBrain::new(plan_reply(12));
    *brain.min_words.lock().unwrap() = 5;
    let rig = pass_rig(brain.clone(), "open youtube");
    rig.ptt();
    rig.wait_line("Heard: open youtube");
    settle();
    assert!(brain.texts().is_empty(), "wants() said no");
    assert_eq!(rig.panel.last_payload().unwrap(), PanelPayload::new(vec![info("Heard: open youtube")]));
    // Positive control: the same rig and phrase DO reach the brain once wants() says yes.
    *brain.min_words.lock().unwrap() = 2;
    rig.say("open youtube");
    rig.wait_line(PLAN_LINE);
    assert_eq!(brain.texts(), vec!["open youtube"]);
}

#[test]
fn n3_the_brain_runs_on_a_talk_thread_not_the_watcher_the_worker_or_the_caller() {
    let brain = FakeBrain::new(plan_reply(12));
    let rig = pass_rig(brain.clone(), "play blinding");
    let me = thread::current().name().map(String::from);
    rig.ptt();
    rig.wait_line(PLAN_LINE);
    let name = brain.calls.lock().unwrap()[0].2.clone();
    assert_ne!(Some(name.clone()), me, "the brain ran on the caller's thread");
    assert!(!name.is_empty() && name != "main", "unnamed or main thread: {name:?}");
    assert_ne!(name, "so-speech", "the speech worker must not block on the network");
    assert!(!name.contains("watcher") && !name.contains("hotkey"), "{name}");
}

#[test]
fn n3_the_brain_gets_the_full_text_and_the_stop_time() {
    let brain = FakeBrain::new(plan_reply(12));
    let rig = pass_rig(brain.clone(), "x");
    rig.clock.set(1000);
    *rig.tr.result.lock().unwrap() = Ok("Play  Blinding Lights\u{2014}now".into());
    rig.ptt();
    rig.wait_line(PLAN_LINE);
    let calls = brain.calls.lock().unwrap().clone();
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].0, "Play  Blinding Lights\u{2014}now", "the transcript text as the router saw it");
    assert_eq!(calls[0].1, rig.clock.at(1000), "stopped_at is the clock reading at the Stopped event");
}

#[test]
fn n3_the_reply_is_hidden_after_its_own_hold_time() {
    let brain = FakeBrain::new(plan_reply(2));
    let rig = pass_rig(brain, "play blinding");
    rig.ptt();
    rig.wait_line(PLAN_LINE);
    let shown_at = Instant::now();
    assert_eq!(rig.panel.hides(), 0);
    wait_until("hide after the reply hold (2 s)", Duration::from_secs(5), || rig.panel.hides() >= 1);
    let v = shown_at.elapsed();
    assert!(v >= Duration::from_millis(1700) && v <= Duration::from_millis(3200), "hold_secs 2, hidden after {v:?}");
}

#[test]
fn n3_the_four_second_heard_timer_does_not_hide_a_longer_reply_early() {
    let brain = FakeBrain::new(plan_reply(6));
    let rig = pass_rig(brain, "play blinding");
    rig.ptt();
    rig.wait_line(PLAN_LINE);
    let shown_at = Instant::now();
    thread::sleep(Duration::from_millis(4700));
    assert_eq!(rig.panel.hides(), 0, "hidden after {:?}: the Heard timer (4 s) must not hide a 6 s reply", shown_at.elapsed());
    wait_until("hide after the reply hold (6 s)", Duration::from_secs(4), || rig.panel.hides() >= 1);
    let v = shown_at.elapsed();
    assert!(v >= Duration::from_millis(5700), "{v:?}");
}

// ---------- N4: stale reply ----------
fn stale_brain_rig() -> (Rig, Arc<FakeBrain>) {
    let brain = FakeBrain::new(plan_reply(12));
    *brain.delay.lock().unwrap() = Duration::from_millis(800);
    let rig = pass_rig(brain.clone(), "play blinding");
    rig.ptt(); // talk 1; the brain is now busy for 800 ms
    rig.wait_line(THINKING_TEXT);
    rig.ev(Armed);
    rig.ev(Started(PushToTalk)); // talk 2
    wait("2nd Listening", || rig.panel.texts().iter().filter(|t| *t == "Listening").count() == 2);
    thread::sleep(Duration::from_millis(1300)); // talk 1's reply arrives meanwhile
    (rig, brain)
}

#[test]
fn n4_a_reply_that_arrives_after_a_newer_talk_started_is_never_shown() {
    let (rig, brain) = stale_brain_rig();
    assert_eq!(brain.texts(), vec!["play blinding"], "positive control: the brain was called");
    let all = rig.panel.all_line_texts();
    assert!(!all.iter().any(|t| t == PLAN_LINE || t.starts_with("Plan, not run")), "stale reply shown: {all:?}");
    assert_eq!(rig.panel.texts().last().unwrap(), "Listening", "talk 2's line stays");
    assert_eq!(rig.panel.hides(), 0, "the stale talk must not hide talk 2's panel");
}

#[test]
fn n4_a_current_talk_after_a_dropped_one_still_gets_its_plan() {
    let (rig, brain) = stale_brain_rig();
    *brain.delay.lock().unwrap() = Duration::ZERO;
    rig.ev(Stopped(PushToTalk)); // talk 2 ends with the same fake transcript
    rig.wait_line(PLAN_LINE);
    assert_eq!(brain.texts().len(), 2);
}

#[test]
fn n4_a_dropped_reply_is_logged_as_brain_outcome_12() {
    run_isolated(module_path!(), "n4_inner");
}
#[test]
#[ignore]
fn n4_inner() {
    if !in_child() {
        return;
    }
    install_capture();
    let (_rig, _brain) = stale_brain_rig();
    let recs = captured();
    assert!(recs.iter().any(|l| l.contains("brain outcome=12 ")), "no brain outcome 12 record: {recs:?}");
    assert!(recs.iter().all(|l| !l.contains("brain outcome=1 ")), "a dropped reply must not log a plan");
}

// ---------- N1 / N2 / N5: the fast router and the hand-off ----------
struct Fast {
    rig: Rig,
    brain: Arc<FakeBrain>,
    media: Arc<FakeMedia>,
    launcher: Arc<FakeLauncher>,
}
fn fast_rig(apps: Vec<crate::so::rules::apps::AppEntry>, to_brain: bool, text: &str) -> Fast {
    let clock = FakeClock::new();
    let media = Arc::new(FakeMedia::default());
    let launcher = Arc::new(FakeLauncher::default());
    let router = FastRouter::new(media.clone(), FakeCatalog::new(apps), launcher.clone(), clock.clone());
    let router = if to_brain { router.not_found_to_brain() } else { router };
    let brain = FakeBrain::new(plan_reply(12));
    let rig = brain_rig(clock, Arc::new(Log::default()), Arc::new(router), brain.clone(), text);
    Fast { rig, brain, media, launcher }
}

#[test]
fn n1_open_with_no_match_goes_to_the_brain_with_the_full_text_and_no_couldnt_find_line() {
    let f = fast_rig(vec![app("Calculator", false)], true, "open youtube");
    f.rig.ptt();
    f.rig.wait_line(PLAN_LINE);
    assert_eq!(f.brain.texts(), vec!["open youtube"], "the original full text, not just the app name");
    let all = f.rig.panel.all_line_texts();
    assert!(all.iter().all(|t| !t.contains("couldn't find")), "{all:?}");
    assert!(f.launcher.opens().is_empty());
    assert!(all.iter().any(|t| t == THINKING_TEXT));
}

#[test]
fn n1_the_fast_router_still_logs_outcome_4_for_the_not_found_hand_off() {
    run_isolated(module_path!(), "n1_inner");
}
#[test]
#[ignore]
fn n1_inner() {
    if !in_child() {
        return;
    }
    install_capture();
    let f = fast_rig(vec![app("Calculator", false)], true, "open youtube");
    f.rig.ptt();
    f.rig.wait_line(PLAN_LINE);
    settle();
    let recs = captured();
    let fast: Vec<&String> = recs.iter().filter(|l| l.contains("fast_rule outcome=")).collect();
    assert_eq!(fast.len(), 1, "{recs:?}");
    assert!(fast[0].contains("fast_rule outcome=4 "), "{}", fast[0]);
    assert!(recs.iter().all(|l| !l.to_lowercase().contains("youtube")), "the phrase reached the log");
}

#[test]
fn n1_with_the_hand_off_on_a_found_app_is_still_opened_by_the_fast_rule() {
    let f = fast_rig(vec![app("YouTube", false)], true, "open youtube");
    f.rig.ptt();
    f.rig.wait_line("Opening youtube");
    settle();
    assert_eq!(f.launcher.opens().len(), 1);
    assert!(f.brain.texts().is_empty(), "a found app never reaches the brain");
}

#[test]
fn n2_plain_fast_router_keeps_the_step_3_not_found_text_and_never_calls_the_brain() {
    let f = fast_rig(vec![app("Calculator", false)], false, "open youtube");
    f.rig.ptt();
    f.rig.wait_line("I couldn't find an app called youtube");
    settle();
    assert!(f.brain.texts().is_empty());
    assert_eq!(f.rig.panel.last_payload().unwrap(), PanelPayload::new(vec![info("Heard: open youtube"), info("I couldn't find an app called youtube")]));
}

#[test]
fn n5_fast_rule_phrases_never_reach_the_brain() {
    let f = fast_rig(vec![app("Calculator", false)], true, "pause");
    f.rig.ptt();
    wait("media key", || f.media.presses() == vec![MediaKey::PlayPause]);
    settle();
    assert!(f.brain.texts().is_empty(), "pause went to the brain");
    f.rig.say("open calculator");
    wait("app opened", || f.launcher.opens().len() == 1);
    settle();
    assert!(f.brain.texts().is_empty(), "open <found app> went to the brain");
    f.rig.say("next");
    wait("next key", || f.media.presses().len() == 2);
    settle();
    assert!(f.brain.texts().is_empty());
    // Positive control: an unknown phrase from the same rig does reach it.
    f.rig.say("book me a table for two");
    f.rig.wait_line(PLAN_LINE);
    assert_eq!(f.brain.texts(), vec!["book me a table for two"]);
}

// ---------- N6 ----------
#[test]
fn n6_on_event_returns_at_once_while_the_brain_blocks() {
    let brain = FakeBrain::new(plan_reply(12));
    *brain.delay.lock().unwrap() = Duration::from_secs(2);
    let rig = pass_rig(brain.clone(), "play blinding");
    for e in [Armed, Started(PushToTalk), Stopped(PushToTalk), Armed, Started(PushToTalk)] {
        let t = Instant::now();
        rig.sink.on_event(e);
        assert!(t.elapsed() < Duration::from_millis(5), "{e:?} took {:?}", t.elapsed());
        if matches!(e, Stopped(_)) {
            wait("brain called", || brain.texts().len() == 1);
        }
    }
    let t = Instant::now();
    rig.sink.on_event(Armed);
    assert!(t.elapsed() < Duration::from_millis(5), "on_event blocked while the brain was running");
}

// ---------- N7 (debug builds: the typed-phrase input) ----------
#[cfg(debug_assertions)]
#[test]
fn n7_a_typed_phrase_takes_the_same_brain_path_as_speech() {
    let brain = FakeBrain::new(plan_reply(12));
    let rig = pass_rig(brain.clone(), "play blinding");
    // Speech reference.
    rig.ptt();
    rig.wait_line(PLAN_LINE);
    let speech = rig.panel.last_payload().unwrap();
    // Typed.
    let brain2 = FakeBrain::new(plan_reply(12));
    let typed = pass_rig(brain2.clone(), "unused");
    typed.sink.inject_transcript("play blinding".to_string());
    typed.wait_line(PLAN_LINE);
    assert_eq!(brain2.texts(), vec!["play blinding"]);
    assert_eq!(typed.panel.last_payload().unwrap(), speech, "same panel as the speech path");
    assert!(typed.panel.all_line_texts().iter().any(|t| t == THINKING_TEXT), "typed text shows Thinking… too");
    // A typed one-word phrase is not sent.
    typed.sink.inject_transcript("paws".to_string());
    typed.wait_line("Heard: paws");
    settle();
    assert_eq!(brain2.texts(), vec!["play blinding"]);
}

// ---------- N8: no brain = steps 2 and 3, unchanged ----------
#[test]
fn n8_with_router_without_a_brain_passes_through_as_in_step_3() {
    let log = Arc::new(Log::default());
    let router = FakeRouter::new(log.clone(), Route::PassThrough);
    let rig = Rig::with_router(FakeClock::new(), log, router.clone(), "open youtube");
    rig.ptt();
    rig.wait_line("Heard: open youtube");
    settle();
    assert_eq!(rig.panel.texts(), vec!["Listening", TRANSCRIBING_TEXT, "Heard: open youtube"], "no Thinking… without a brain");
    assert_eq!(rig.panel.last_payload().unwrap(), PanelPayload::new(vec![info("Heard: open youtube")]));
    wait_until("hide after 4 s", Duration::from_secs(6), || rig.panel.hides() == 1);
}

#[test]
fn n8_a_handled_route_shows_the_router_lines_and_skips_the_brain() {
    let log = Arc::new(Log::default());
    let router = FakeRouter::new(log.clone(), Route::Handled(vec![info("Play/pause sent")]));
    let brain = FakeBrain::new(plan_reply(12));
    let rig = brain_rig(FakeClock::new(), log, router, brain.clone(), "play blinding");
    rig.ptt();
    rig.wait_line("Play/pause sent");
    settle();
    assert!(brain.texts().is_empty());
    assert_eq!(rig.panel.last_payload().unwrap(), PanelPayload::new(vec![info("Heard: play blinding"), info("Play/pause sent")]));
    assert!(rig.panel.all_line_texts().iter().all(|t| t != THINKING_TEXT));
}

#[test]
fn n8_blank_text_and_transcribe_errors_never_reach_the_brain() {
    let brain = FakeBrain::new(plan_reply(12));
    let rig = pass_rig(brain.clone(), "   ");
    rig.ptt();
    rig.wait_line(crate::so::stt::sink::DIDNT_CATCH_TEXT);
    *rig.tr.result.lock().unwrap() = Err("boom".into());
    rig.ptt();
    rig.wait_line(crate::so::stt::sink::SPEECH_FAILED_TEXT);
    settle();
    assert!(brain.texts().is_empty());
}
