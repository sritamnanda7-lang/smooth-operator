// SPDX-License-Identifier: FSL-1.1-MIT
//! Step 6 X1-X24 (+ `result_text`/`Res` unit tests): the executor with fakes. A scripted browser, a scripted
//! Chrome-to-front, the step-3 fakes, a test clock and a recording sleep that advances the clock (nothing
//! really sleeps). X16 runs in a child process (one logger per process, see `rules_support::run_isolated`).
use super::exec_support::*;
use super::rules_support::{captured, in_child, install_capture, run_isolated, wait};
use crate::so::brain::plan::{step_line, Action, MediaArg, OpKind, Risk, Step};
use crate::so::exec::{result_text, ExecConfig, Res, StepResult, Tail, RESULT_SECS};
use crate::so::link::commands::LinkState;
use crate::so::link::protocol::{Command, ResultCode};
use crate::so::link::server::LinkError;
use crate::so::link::PORT_BUSY_TEXT;
use crate::so::rules::phrase::MediaKey;
use std::sync::atomic::Ordering;
use std::time::Duration;

fn line(i: usize, text: &str) -> String {
    format!("{i}. {text}")
}
fn texts(r: &Rig, steps: &[Step]) -> Vec<String> {
    last_body(&r.plan(steps).1)
}
fn first_pos(log: &[String], what: &str) -> usize {
    log.iter().position(|l| l == what).unwrap_or_else(|| panic!("{what} never happened: {log:?}"))
}
fn sr(first: OpKind, res: Res, tail: Tail) -> StepResult {
    StepResult { first, res, tail }
}
use ResultCode::*;

// ---------- Res and result_text (pure) ----------
#[test]
fn res_ok_is_true_only_for_done_playing_ad_paused_editor_open() {
    for r in [Res::Done, Res::Ext(Playing), Res::Ext(Ad), Res::Ext(Paused), Res::Ext(EditorOpen)] {
        assert!(r.ok(), "{r:?}");
    }
    for r in [
        Res::Ext(NoVideo), Res::Ext(NeedsLogin), Res::Ext(Consent), Res::Ext(NoAccess), Res::Ext(Timeout), Res::Ext(BadCmd), Res::Ext(Busy), Res::Ext(Unsafe),
        Res::Ext(Cancelled), Res::Ext(Error), Res::ChromeMissing, Res::NotPaired, Res::NotConnected, Res::PortBusy, Res::KeychainError, Res::LinkTimeout, Res::Lost, Res::BadResult,
        Res::TooLong, Res::PastTime, Res::NoSuchTime, Res::TooFar, Res::AppNotFound, Res::AppAmbiguous, Res::Failed, Res::NotYet, Res::Refused, Res::Stopped, Res::Deadline,
    ] {
        assert!(!r.ok(), "{r:?}");
    }
}

#[test]
fn res_codes_are_distinct_integers_for_the_log() {
    let all = [
        Res::Done, Res::ChromeMissing, Res::NotPaired, Res::NotConnected, Res::PortBusy, Res::KeychainError, Res::LinkTimeout, Res::Lost, Res::BadResult, Res::TooLong, Res::PastTime,
        Res::NoSuchTime, Res::TooFar, Res::AppNotFound, Res::AppAmbiguous, Res::Failed, Res::NotYet, Res::Refused, Res::Stopped, Res::Deadline,
        Res::Ext(Playing), Res::Ext(Ad), Res::Ext(Paused), Res::Ext(NoVideo), Res::Ext(EditorOpen), Res::Ext(NeedsLogin), Res::Ext(Consent), Res::Ext(NoAccess),
        Res::Ext(Timeout), Res::Ext(BadCmd), Res::Ext(Busy), Res::Ext(Unsafe), Res::Ext(Cancelled), Res::Ext(Error),
    ];
    let mut seen = std::collections::HashSet::new();
    for r in all {
        assert!(seen.insert(r.code()), "{r:?} shares its code with another result");
    }
}

#[test]
fn result_text_has_the_fixed_texts() {
    let t = |res, first, tail, guests| result_text(&sr(first, res, tail), guests);
    use OpKind::*;
    assert_eq!(t(Res::Ext(Playing), YoutubePlay, Tail::None, false), YT_PLAYING);
    assert_eq!(t(Res::Ext(Ad), YoutubePlay, Tail::None, false), "An ad is playing; your video starts after it");
    assert_eq!(t(Res::Ext(Paused), YoutubePlay, Tail::None, false), "Video opened but paused: press play in Chrome");
    assert_eq!(t(Res::Ext(NoVideo), YoutubePlay, Tail::None, false), NO_VIDEO_TEXT);
    assert_eq!(t(Res::Ext(EditorOpen), DocsOpen, Tail::NeedsOk, false), DOCS_TEXT);
    assert_eq!(t(Res::Ext(EditorOpen), CalendarOpen, Tail::Deferred, false), CAL_TEXT);
    let guests = t(Res::Ext(EditorOpen), CalendarOpen, Tail::NeedsOk, true);
    assert!(guests.starts_with(CAL_TEXT) && guests.ends_with(GUESTS_SENTENCE), "{guests}");
    assert_eq!(t(Res::Ext(NeedsLogin), YoutubePlay, Tail::None, false), LOGIN_TEXT);
    assert_eq!(t(Res::Ext(Consent), YoutubePlay, Tail::None, false), LOGIN_TEXT);
    assert_eq!(t(Res::Ext(NoAccess), YoutubePlay, Tail::None, false), NO_ACCESS_TEXT);
    assert_eq!(t(Res::Ext(Timeout), YoutubePlay, Tail::None, false), TIMEOUT_TEXT);
    assert!(t(Res::NotYet, OpenUrl, Tail::None, false).starts_with("Not available yet"));
    assert_eq!(t(Res::Refused, YoutubePlay, Tail::None, false), BLOCKED_TEXT);
    assert_eq!(t(Res::ChromeMissing, YoutubePlay, Tail::None, false), CHROME_MISSING_TEXT);
    assert_eq!(t(Res::NotPaired, YoutubePlay, Tail::None, false), PAIR_TEXT);
    assert_eq!(t(Res::NotConnected, YoutubePlay, Tail::None, false), NOT_CONNECTED_TEXT);
    assert_eq!(t(Res::PortBusy, YoutubePlay, Tail::None, false), PORT_BUSY_TEXT);
    assert_eq!(t(Res::KeychainError, YoutubePlay, Tail::None, false), KEYCHAIN_TEXT);
    assert_eq!(t(Res::PastTime, CalendarOpen, Tail::None, false), "That time has already passed");
    assert_eq!(t(Res::NoSuchTime, CalendarOpen, Tail::None, false), "That time doesn't exist (clock change)");
    assert_eq!(t(Res::TooFar, CalendarOpen, Tail::None, false), "That date is too far ahead");
    assert_eq!(t(Res::Stopped, YoutubePlay, Tail::None, false), STOPPED_TEXT);
    assert_eq!(t(Res::Deadline, YoutubePlay, Tail::None, false), DEADLINE_TEXT);
    for other in [Res::Failed, Res::Lost, Res::LinkTimeout, Res::BadResult, Res::TooLong, Res::Ext(Error), Res::Ext(BadCmd), Res::Ext(Unsafe)] {
        assert_eq!(t(other, YoutubePlay, Tail::None, false), FAILED_TEXT, "{other:?}");
    }
}

// ---------- X1 ----------
#[test]
fn x1_one_youtube_step_checks_the_link_fronts_chrome_once_then_asks_with_the_yt_wait() {
    let r = Rig::new();
    r.browser.script(vec![ok(Playing)]);
    let step = yt("Blinding Lights");
    let (hold, shown) = r.plan(std::slice::from_ref(&step));
    assert_eq!(r.browser.reqs(), vec![(Command::YtPlay { url: crate::so::exec::ops::yt_url("Blinding Lights") }, YT_WAIT)]);
    let log = r.log.all();
    assert_eq!(log[0], "state", "the link state is checked first: {log:?}");
    assert!(first_pos(&log, "front") < first_pos(&log, "request"), "Chrome comes to the front before the request: {log:?}");
    assert_eq!(r.chrome.fronts(), 1);
    assert_eq!(hold, RESULT_SECS);
    assert_eq!(RESULT_SECS, 10);
    assert_eq!(last_body(&shown), vec![line(1, YT_PLAYING)]);
    assert!(all_lines(&shown).contains(&format!("Working (step 1 of 1): {}", step_line(&step.action))), "{:?}", all_lines(&shown));
}

#[test]
fn x1_the_task_report_of_a_playing_step() {
    let r = Rig::new();
    r.browser.script(vec![ok(Playing)]);
    let rep = r.run(&[yt("Blinding Lights")]);
    assert_eq!(rep.steps, vec![sr(OpKind::YoutubePlay, Res::Ext(Playing), Tail::None)]);
    assert_eq!((rep.not_run, rep.retries, rep.busy), (0, 0, false));
    assert!(!r.exec.busy(), "the task is over");
}

// ---------- X2 ----------
#[test]
fn x2_youtube_then_calendar_makes_two_requests_in_order_and_fronts_once() {
    let r = Rig::new();
    r.browser.script(vec![ok(Playing), ok(EditorOpen)]);
    let steps = [yt("Blinding Lights"), cal("Review", &[])];
    let (_, shown) = r.plan(&steps);
    let cmds = r.browser.cmds();
    assert_eq!(cmds.len(), 2);
    assert!(matches!(&cmds[0], Command::YtPlay { .. }));
    let want = crate::so::exec::ops::command_for(OpKind::CalendarOpen, &steps[1].action, r.wall.as_ref()).unwrap();
    assert_eq!(cmds[1], want);
    assert_eq!(r.chrome.fronts(), 1);
    assert_eq!(last_body(&shown), vec![line(1, YT_PLAYING), line(2, CAL_TEXT)]);
    assert!(all_lines(&shown).contains(&format!("Working (step 2 of 2): {}", step_line(&steps[1].action))));
}

// ---------- X3 ----------
#[test]
fn x3_no_video_stops_the_chain_and_counts_what_was_not_run() {
    let r = Rig::new();
    r.browser.script(vec![ok(NoVideo)]);
    let steps = [yt("zzzz qqqq"), cal("Review", &[])];
    let (_, shown) = r.plan(&steps);
    assert_eq!(r.browser.reqs().len(), 1, "the calendar was not requested");
    assert_eq!(last_body(&shown), vec![line(1, NO_VIDEO_TEXT), "Not run: 1 more step".to_string()]);
    let r2 = Rig::new();
    r2.browser.script(vec![ok(NoVideo)]);
    let rep = r2.run(&steps);
    assert_eq!((rep.steps.len(), rep.not_run), (1, 1));
    // plural
    let r3 = Rig::new();
    r3.browser.script(vec![ok(NoVideo)]);
    assert_eq!(texts(&r3, &[yt("a b"), cal("x", &[]), cal("y", &[])]).last().unwrap(), "Not run: 2 more steps");
}

// ---------- X4 ----------
#[test]
fn x4_docs_opens_a_blank_doc_with_the_page_wait_and_the_next_step_waits_for_approval() {
    let r = Rig::new();
    r.browser.script(vec![ok(EditorOpen)]);
    let steps = [docs("WWII"), media(MediaArg::Next)];
    let (_, shown) = r.plan(&steps);
    assert_eq!(r.browser.reqs(), vec![(Command::DocsOpen, PAGE_WAIT)]);
    assert_eq!(last_body(&shown), vec![line(1, DOCS_TEXT), "Not run: 1 more step".to_string()]);
    assert!(r.media.presses().is_empty(), "the chain waits at its risky step");
    let r2 = Rig::new();
    r2.browser.script(vec![ok(EditorOpen)]);
    let rep = r2.run(&steps);
    assert_eq!(rep.steps, vec![sr(OpKind::DocsOpen, Res::Ext(EditorOpen), Tail::NeedsOk)]);
    assert_eq!(rep.not_run, 1);
}

// ---------- X5 ----------
#[test]
fn x5_calendar_with_guests_leaves_them_out_of_the_url_and_waits_for_approval() {
    let r = Rig::new();
    r.browser.script(vec![ok(EditorOpen)]);
    let steps = [cal("Sync", &["a@b.co", "c@d.co"]), media(MediaArg::Next)];
    let (_, shown) = r.plan(&steps);
    match &r.browser.cmds()[..] {
        [Command::CalOpen { url }] => {
            let lower = url.to_lowercase();
            assert!(!lower.contains("add=") && !lower.contains("a@b.co") && !lower.contains("a%40b.co"), "{url}");
        }
        other => panic!("one CalOpen expected: {other:?}"),
    }
    let body = last_body(&shown);
    assert_eq!(body.len(), 2, "{body:?}");
    assert!(body[0].starts_with("1. ") && body[0].contains(CAL_TEXT) && body[0].ends_with(GUESTS_SENTENCE), "{}", body[0]);
    assert_eq!(body[1], "Not run: 1 more step");
    assert!(r.media.presses().is_empty());
    let r2 = Rig::new();
    r2.browser.script(vec![ok(EditorOpen)]);
    assert_eq!(r2.run(&steps).steps, vec![sr(OpKind::CalendarOpen, Res::Ext(EditorOpen), Tail::NeedsOk)]);
}

// ---------- X6 ----------
#[test]
fn x6_not_paired_shows_the_pair_text_and_never_touches_chrome() {
    let r = Rig::new();
    r.browser.fixed(LinkState::NotPaired);
    assert_eq!(texts(&r, &[yt("a b")]), vec![line(1, PAIR_TEXT)]);
    assert_eq!(r.chrome.fronts(), 0, "Chrome must not be touched when the extension is not paired");
    assert!(r.browser.reqs().is_empty());
}

#[test]
fn x6_port_busy_shows_the_port_busy_text_and_never_touches_chrome() {
    let r = Rig::new();
    r.browser.fixed(LinkState::PortBusy);
    assert_eq!(texts(&r, &[yt("a b")]), vec![line(1, PORT_BUSY_TEXT)]);
    assert_eq!(r.chrome.fronts(), 0);
    assert!(r.browser.reqs().is_empty());
}

#[test]
fn x6_never_connected_waits_the_full_link_wait_then_says_not_connected() {
    let r = Rig::new();
    r.browser.fixed(LinkState::NotFound);
    let (_, shown) = r.plan(&[yt("a b")]);
    assert_eq!(last_body(&shown), vec![line(1, NOT_CONNECTED_TEXT)]);
    assert!(r.browser.reqs().is_empty());
    assert_eq!(r.chrome.fronts(), 1);
    let waited = elapsed_ms(&r.clock);
    assert!((35_000..=36_500).contains(&waited), "waited {waited} ms, link_wait is 35 s");
    // "Connecting to Chrome…" appears after about 1 s, not before.
    let at: Vec<u64> = shown.iter().filter(|s| s.lines.iter().any(|l| l == CONNECTING_TEXT)).map(|s| s.at_ms).collect();
    assert!(!at.is_empty(), "no Connecting to Chrome… line");
    assert!(at[0] >= 1000 && at[0] <= 1400, "shown at {} ms", at[0]);
}

#[test]
fn x6_connected_after_three_polls_proceeds_and_shows_no_connecting_line() {
    let r = Rig::new();
    let c = r.clock.clone();
    r.browser.set_state(move || if elapsed_ms(&c) >= 600 { LinkState::Connected } else { LinkState::NotFound });
    r.browser.script(vec![ok(Playing)]);
    let (_, shown) = r.plan(&[yt("a b")]);
    assert_eq!(last_body(&shown), vec![line(1, YT_PLAYING)]);
    assert_eq!(r.browser.reqs().len(), 1);
    assert!(!all_lines(&shown).contains(&CONNECTING_TEXT.to_string()), "it connected within 1 s");
    assert_eq!(r.chrome.fronts(), 1);
    let waited = elapsed_ms(&r.clock);
    assert!((600..=1000).contains(&waited), "{waited}");
}

#[test]
fn x6_a_slow_connect_shows_the_connecting_line_then_proceeds() {
    let r = Rig::new();
    let c = r.clock.clone();
    r.browser.set_state(move || if elapsed_ms(&c) >= 3000 { LinkState::Connected } else { LinkState::NotFound });
    r.browser.script(vec![ok(Playing)]);
    let (_, shown) = r.plan(&[yt("a b")]);
    assert!(all_lines(&shown).contains(&CONNECTING_TEXT.to_string()));
    assert_eq!(last_body(&shown), vec![line(1, YT_PLAYING)]);
}

const KEYCHAIN_TEXT: &str = "Can't read the Chrome link key from the Keychain. Unlock your Mac and try again.";

#[test]
fn x6_a_keychain_error_stops_the_step_with_its_text_and_never_touches_chrome() {
    let r = Rig::new();
    r.browser.fixed(LinkState::KeychainError);
    r.browser.script(vec![ok(Playing)]);
    let (_, shown) = r.plan(&[yt("a b"), media(MediaArg::Next)]);
    assert_eq!(last_body(&shown), vec![line(1, KEYCHAIN_TEXT), "Not run: 1 more step".to_string()]);
    assert_eq!(r.chrome.fronts(), 0, "Chrome must not be touched (like NotPaired)");
    assert!(r.browser.reqs().is_empty(), "no request");
    assert!(r.media.presses().is_empty(), "the chain stops at that step");
    assert_eq!(r.slept_total(), Duration::ZERO, "no retry pause, no link wait");
    let r2 = Rig::new();
    r2.browser.fixed(LinkState::KeychainError);
    let rep = r2.run(&[docs("t")]);
    assert_eq!((rep.steps[0].res, rep.retries), (Res::KeychainError, 0));
    assert_eq!(r2.chrome.fronts(), 0);
    assert!(r2.browser.reqs().is_empty());
}

// ---------- X7 ----------
#[test]
fn x7_chrome_missing_has_its_own_text_and_sends_nothing() {
    let r = Rig::new();
    r.chrome.fails(FrontKind::NotInstalled);
    assert_eq!(texts(&r, &[yt("a b")]), vec![line(1, CHROME_MISSING_TEXT)]);
    assert!(r.browser.reqs().is_empty());
    let r2 = Rig::new();
    r2.chrome.fails(FrontKind::NotInstalled);
    assert_eq!(r2.run(&[yt("a b")]).steps[0].res, Res::ChromeMissing);
    // Chrome found but it would not open: nothing is sent either.
    let r3 = Rig::new();
    r3.chrome.fails(FrontKind::Failed);
    let rep = r3.run(&[yt("a b")]);
    assert!(r3.browser.reqs().is_empty() && !rep.steps[0].res.ok());
}

// ---------- X8 ----------
#[test]
fn x8_busy_busy_playing_is_ok_with_two_retries() {
    let r = Rig::new();
    r.browser.script(vec![ok(Busy), ok(Busy), ok(Playing)]);
    let rep = r.run(&[yt("a b")]);
    assert_eq!(rep.steps, vec![sr(OpKind::YoutubePlay, Res::Ext(Playing), Tail::None)]);
    assert_eq!(rep.retries, 2);
    assert_eq!(r.browser.reqs().len(), 3);
}

#[test]
fn x8_youtube_retries_on_ext_error_timeout_and_on_link_not_connected_busy_timeout_lost() {
    for first in [ok(Error), ok(Timeout), ok(Busy), err(LinkError::NotConnected), err(LinkError::Busy), err(LinkError::Timeout), err(LinkError::Lost)] {
        let r = Rig::new();
        r.browser.script(vec![first, ok(Playing)]);
        let rep = r.run(&[yt("a b")]);
        assert_eq!(rep.steps[0].res, Res::Ext(Playing), "{:?}", r.browser.reqs());
        assert_eq!(rep.retries, 1);
        assert_eq!(r.browser.reqs().len(), 2);
    }
}

#[test]
fn x8_calendar_is_idempotent_too_and_retries_like_youtube() {
    for first in [ok(Timeout), ok(Busy), ok(Error), err(LinkError::Timeout), err(LinkError::Lost)] {
        let r = Rig::new();
        r.browser.script(vec![first, ok(EditorOpen)]);
        let rep = r.run(&[cal("Review", &[])]);
        assert_eq!(rep.steps[0].res, Res::Ext(EditorOpen));
        assert_eq!(r.browser.reqs().len(), 2);
    }
}

#[test]
fn x8_four_timeouts_fail_after_four_requests_with_three_retries() {
    let r = Rig::new();
    r.browser.script(vec![ok(Timeout), ok(Timeout), ok(Timeout), ok(Timeout), ok(Playing)]);
    let (_, shown) = r.plan(&[yt("a b"), media(MediaArg::Next)]);
    assert_eq!(r.browser.reqs().len(), 4, "1 try + 3 retries");
    assert_eq!(last_body(&shown), vec![line(1, TIMEOUT_TEXT), "Not run: 1 more step".to_string()]);
    let r2 = Rig::new();
    r2.browser.script(vec![ok(Timeout); 5]);
    let rep = r2.run(&[yt("a b")]);
    assert_eq!((rep.steps[0].res, rep.retries), (Res::Ext(Timeout), 3));
}

#[test]
fn x8_the_retry_budget_is_shared_across_steps() {
    let r = Rig::new();
    r.browser.script(vec![ok(Busy), ok(Busy), ok(Playing), ok(Busy), ok(Busy), ok(Playing)]);
    let rep = r.run(&[yt("a b"), yt("c d")]);
    assert_eq!(r.browser.reqs().len(), 5, "2 retries for step 1, then only 1 left for step 2");
    assert_eq!(rep.steps[0].res, Res::Ext(Playing));
    assert_eq!(rep.steps[1].res, Res::Ext(Busy), "the budget ran out in step 2");
    assert_eq!(rep.retries, 3);
}

// ---------- X9 ----------
#[test]
fn x9_docs_open_retries_only_when_nothing_was_sent_or_the_extension_said_busy() {
    for first in [err(LinkError::NotConnected), err(LinkError::Busy), ok(Busy)] {
        let r = Rig::new();
        r.browser.script(vec![first, ok(EditorOpen)]);
        let rep = r.run(&[docs("t")]);
        assert_eq!(r.browser.reqs().len(), 2);
        assert_eq!(rep.steps[0].res, Res::Ext(EditorOpen));
        assert_eq!(rep.retries, 1);
    }
}

#[test]
fn x9_docs_open_never_retries_after_it_may_have_made_a_doc() {
    for first in [ok(Timeout), ok(Error), ok(NeedsLogin), err(LinkError::Timeout), err(LinkError::Lost)] {
        let r = Rig::new();
        r.browser.script(vec![first, ok(EditorOpen)]);
        let rep = r.run(&[docs("t")]);
        assert_eq!(r.browser.reqs().len(), 1, "a second docs_open could make a second empty doc");
        assert_ne!(rep.steps[0].res, Res::Ext(EditorOpen));
        assert_eq!(rep.retries, 0);
    }
}

// ---------- X10 ----------
fn stop_in_hook(r: &Rig) {
    let e = r.exec.clone();
    r.browser.set_hook(move |_, _| e.stop());
}

#[test]
fn x10_stop_during_a_request_is_stopped_and_the_rest_is_not_run() {
    let r = Rig::new();
    stop_in_hook(&r);
    let steps = [yt("a b"), cal("x", &[]), media(MediaArg::Next)];
    let (_, shown) = r.plan(&steps);
    assert_eq!(r.browser.reqs().len(), 1);
    assert_eq!(last_body(&shown), vec![line(1, STOPPED_TEXT), "Not run: 2 more steps".to_string()]);
    assert!(r.media.presses().is_empty());
}

#[test]
fn x10_stop_during_the_link_wait_sends_no_request() {
    let r = Rig::new();
    r.browser.fixed(LinkState::NotFound);
    let e = r.exec.clone();
    *r.sleep_hook.lock().unwrap() = Some(Box::new(move |i| {
        if i == 3 {
            e.stop();
        }
    }));
    let rep = r.run(&[yt("a b"), media(MediaArg::Next)]);
    assert!(r.browser.reqs().is_empty());
    assert_eq!(rep.steps[0].res, Res::Stopped);
    assert_eq!(rep.not_run, 1);
    assert!(elapsed_ms(&r.clock) < 1000, "it stopped at once, not after the 35 s wait");
}

#[test]
fn x10_stop_during_a_retry_pause_sends_no_further_request() {
    let r = Rig::new();
    r.browser.script(vec![ok(Busy), ok(Playing)]);
    let e = r.exec.clone();
    *r.sleep_hook.lock().unwrap() = Some(Box::new(move |i| {
        if i == 0 {
            e.stop();
        }
    }));
    let rep = r.run(&[yt("a b")]);
    assert_eq!(r.browser.reqs().len(), 1);
    assert_eq!(rep.steps[0].res, Res::Stopped);
}

#[test]
fn x10_stop_between_steps_means_the_next_step_never_starts() {
    let r = Rig::new();
    r.browser.ignore_stop.store(true, Ordering::SeqCst);
    r.browser.script(vec![ok(Playing), ok(Playing)]);
    stop_in_hook(&r);
    let rep = r.run(&[yt("a b"), yt("c d"), media(MediaArg::Next)]);
    assert_eq!(r.browser.reqs().len(), 1, "the second step must not start");
    assert_eq!(rep.steps.len() + rep.not_run, 3);
    assert!(rep.not_run >= 2);
    assert!(r.media.presses().is_empty());
}

#[test]
fn x10_a_stop_with_no_task_running_does_not_poison_the_next_task() {
    let r = Rig::new();
    r.exec.stop();
    r.browser.script(vec![ok(Playing)]);
    assert_eq!(r.run(&[yt("a b")]).steps[0].res, Res::Ext(Playing));
}

// ---------- X11 ----------
#[test]
fn x11_a_second_task_while_one_runs_is_busy_and_sends_nothing() {
    let r = Rig::new();
    r.browser.hold();
    r.browser.script(vec![ok(Playing)]);
    let exec = r.exec.clone();
    let t = std::thread::spawn(move || exec.run(&[yt("a b")]));
    wait("the first request", || r.browser.reqs().len() == 1);
    assert!(r.exec.busy());
    let (_, shown) = r.plan(&[yt("c d")]);
    assert_eq!(last_body(&shown), vec![BUSY_TEXT.to_string()]);
    let rep = r.run(&[yt("c d"), media(MediaArg::Next)]);
    assert!(rep.busy && rep.steps.is_empty());
    assert_eq!(rep.not_run, 2);
    assert_eq!(r.browser.reqs().len(), 1, "no request for the second task");
    r.browser.release();
    let first = t.join().expect("the first task finishes");
    assert!(!first.busy);
    assert_eq!(first.steps[0].res, Res::Ext(Playing));
    assert!(!r.exec.busy());
}

// ---------- X12 ----------
#[test]
fn x12_twenty_six_steps_run_nothing_and_are_refused() {
    let r = Rig::new();
    let steps: Vec<Step> = (0..26).map(|_| media(MediaArg::Next)).collect();
    let (_, shown) = r.plan(&steps);
    assert!(r.media.presses().is_empty() && r.browser.reqs().is_empty() && r.launcher.opens().is_empty());
    assert_eq!(r.chrome.fronts(), 0);
    assert!(all_lines(&shown).contains(&BLOCKED_TEXT.to_string()), "{:?}", all_lines(&shown));
    let rep = r.run(&steps);
    assert!(rep.steps.iter().all(|s| s.res == Res::Refused));
    assert_eq!(rep.steps.len() + rep.not_run, 26);
    // 25 is still fine
    let r2 = Rig::new();
    let ok25: Vec<Step> = (0..25).map(|_| media(MediaArg::Next)).collect();
    assert_eq!(r2.run(&ok25).steps.len(), 25);
    assert_eq!(r2.media.presses().len(), 25);
}

// ---------- X13 ----------
#[test]
fn x13_media_and_app_steps_use_the_step_3_effects_and_never_touch_chrome() {
    let r = Rig::new();
    let rep = r.run(&[media(MediaArg::PlayPause)]);
    assert_eq!(rep.steps, vec![sr(OpKind::MediaKey, Res::Done, Tail::None)]);
    assert_eq!(r.media.presses(), vec![MediaKey::PlayPause]);
    let rep = r.run(&[open_app("Notes")]);
    assert_eq!(rep.steps, vec![sr(OpKind::AppOpen, Res::Done, Tail::None)]);
    let rep = r.run(&[switch_app("notes")]);
    assert_eq!(rep.steps, vec![sr(OpKind::AppSwitch, Res::Done, Tail::None)]);
    assert_eq!(r.launcher.opens(), vec![std::path::PathBuf::from("/Applications/Notes.app"), std::path::PathBuf::from("/Applications/Notes.app")]);
    assert_eq!(r.chrome.fronts(), 0);
    assert!(r.browser.reqs().is_empty());
    assert!(r.catalog.lists() >= 1, "app names come from the installed list");
}

#[test]
fn x13_not_found_and_ambiguous_apps_use_the_step_3_texts() {
    let r = Rig::new();
    let (_, shown) = r.plan(&[open_app("Nonexistent")]);
    let body = last_body(&shown);
    assert!(body.iter().any(|l| l.contains("I couldn't find an app called Nonexistent")), "{body:?}");
    assert!(r.launcher.opens().is_empty());
    let (_, shown) = r.plan(&[open_app("drive")]);
    let body = last_body(&shown);
    assert!(body.iter().any(|l| l.contains("More than one app matches drive: Adobe Drive, Google Drive. Say the full name.")), "{body:?}");
    assert!(r.launcher.opens().is_empty());
    assert_eq!(r.run(&[open_app("Nonexistent")]).steps[0].res, Res::AppNotFound);
    assert_eq!(r.run(&[open_app("drive")]).steps[0].res, Res::AppAmbiguous);
    assert_eq!(r.chrome.fronts(), 0);
}

// ---------- X14 ----------
#[test]
fn x14_a_result_that_does_not_belong_to_the_op_is_bad_result_and_is_not_retried() {
    let yt_bad = [EditorOpen];
    for code in yt_bad {
        let r = Rig::new();
        r.browser.script(vec![ok(code), ok(Playing)]);
        let rep = r.run(&[yt("a b")]);
        assert_eq!(rep.steps[0].res, Res::BadResult, "{code:?} for yt_play");
        assert_eq!(r.browser.reqs().len(), 1);
    }
    for (step, name) in [(docs("t"), "docs_open"), (cal("x", &[]), "cal_open")] {
        for code in [Playing, Ad, Paused, NoVideo, Consent] {
            let r = Rig::new();
            r.browser.script(vec![ok(code), ok(EditorOpen)]);
            let rep = r.run(std::slice::from_ref(&step));
            assert_eq!(rep.steps[0].res, Res::BadResult, "{code:?} for {name}");
            assert_eq!(r.browser.reqs().len(), 1, "no retry after a bad result");
        }
    }
}

// ---------- X15 ----------
fn assert_panel_rules(shown: &[Shown], what: &str) {
    assert!(!shown.is_empty(), "{what}: nothing shown");
    for s in shown {
        assert!(s.lines.len() <= 6, "{what}: {} lines: {:?}", s.lines.len(), s.lines);
        for l in &s.lines {
            assert!(l.chars().count() <= 200, "{what}: {} chars: {l}", l.chars().count());
            assert!(!l.to_lowercase().contains("http"), "{what}: a URL in the panel: {l}");
        }
    }
}

#[test]
fn x15_every_line_is_at_most_200_chars_at_most_6_lines_and_never_a_url() {
    let long = "x".repeat(400);
    let answers: Vec<Answer> = vec![
        ok(Playing), ok(Ad), ok(Paused), ok(NoVideo), ok(EditorOpen), ok(NeedsLogin), ok(Consent), ok(NoAccess), ok(Timeout), ok(BadCmd), ok(Busy), ok(Unsafe),
        ok(Cancelled), ok(Error), err(LinkError::NotConnected), err(LinkError::Busy), err(LinkError::TooLong), err(LinkError::Timeout), err(LinkError::Lost), err(LinkError::Stopped),
    ];
    for a in answers {
        let r = Rig::new();
        r.browser.script(vec![a; 5]);
        let (_, shown) = r.plan(&[yt(&long), cal(&long, &[]), docs(&long), open_url(&format!("https://example.com/{long}")), media(MediaArg::Next)]);
        assert_panel_rules(&shown, "long chain");
    }
    // six steps: running line + 4 result lines + "Not run" fit in 6
    let r = Rig::new();
    r.browser.script(vec![ok(Playing); 8]);
    let many: Vec<Step> = (0..9).map(|i| yt(&format!("song {i}"))).collect();
    let (_, shown) = r.plan(&many);
    assert_panel_rules(&shown, "nine steps");
    let body = last_body(&shown);
    assert!(body.len() <= 6 && body.iter().filter(|l| l.starts_with(|c: char| c.is_ascii_digit())).count() <= 4, "at most 4 numbered lines: {body:?}");
    let r = Rig::new();
    r.browser.fixed(LinkState::NotFound);
    assert_panel_rules(&r.plan(&[yt(&long)]).1, "never connected");
}

// ---------- X16: log capture (child process) ----------
#[test]
fn x16_exec_records_are_integers_only_and_no_query_or_title_is_logged() {
    run_isolated(module_path!(), "x16_inner");
}

/// "exec outcome=1 op=2 res=3 ms=4 retries=5" -> 5 integers, else None.
fn parse_exec(args: &str) -> Option<[u64; 5]> {
    let parts: Vec<&str> = args.split(' ').collect();
    if parts.len() != 6 || parts[0] != "exec" {
        return None;
    }
    let mut out = [0u64; 5];
    for (i, (name, p)) in ["outcome", "op", "res", "ms", "retries"].iter().zip(&parts[1..]).enumerate() {
        out[i] = p.strip_prefix(&format!("{name}="))?.parse().ok()?;
    }
    Some(out)
}

#[test]
#[ignore]
fn x16_inner() {
    if !in_child() {
        return;
    }
    install_capture();
    // The parser can fail.
    assert!(parse_exec("exec outcome=1 op=2 res=3 ms=4 retries=5").is_some());
    assert!(parse_exec("exec outcome=1 op=2 res=3 ms=4 retries=5 url=x").is_none() && parse_exec("exec outcome=a op=2 res=3 ms=4 retries=5").is_none());
    let (q, title) = ("kumquatsentinel", "walrussentinel");
    let r = Rig::new();
    r.browser.script(vec![ok(Playing), ok(EditorOpen), ok(Timeout), ok(Timeout), ok(Timeout), ok(Timeout), ok(NoVideo)]);
    r.run(&[yt(q), cal(title, &[]), docs(title), yt(q), open_url(&format!("https://example.com/{q}")), web_search(q)]);
    let r2 = Rig::new();
    r2.browser.fixed(LinkState::NotPaired);
    r2.run(&[yt(q)]);
    let all = captured();
    let exec_recs: Vec<&String> = all.iter().filter(|x| x.contains("::so::exec")).collect();
    assert!(!exec_recs.is_empty(), "no exec record at all (positive control): {all:?}");
    for rec in &exec_recs {
        let args = rec.split_once(' ').map(|x| x.1).unwrap_or(rec);
        assert!(parse_exec(args).is_some(), "not an integers-only exec record: {rec}");
    }
    for rec in &all {
        assert!(!rec.contains(q) && !rec.contains(title), "a query or title was logged: {rec}");
    }
}

// ---------- X17 ----------
#[test]
fn x17_past_no_such_and_too_far_times_show_their_text_and_touch_nothing() {
    let cases = [("2026-11-14", "09:00", Res::PastTime, "That time has already passed"), ("2026-11-16", "02:30", Res::NoSuchTime, "That time doesn't exist (clock change)"), ("2030-01-01", "10:00", Res::TooFar, "That date is too far ahead")];
    for (date, time, res, text) in cases {
        let wall = FakeWall::with_dst(dt(2026, 11, 15, 10, 0), 0, vec![dt(2026, 11, 16, 2, 30)], vec![]);
        let r = Rig::with(cfg(), wall);
        let step = Step { action: Action::CalendarNewEvent { title: "T".into(), date: date.into(), time: time.into(), minutes: None, guests: vec![] }, risk: Risk::Safe };
        let (_, shown) = r.plan(std::slice::from_ref(&step));
        assert_eq!(last_body(&shown), vec![line(1, text)], "{date} {time}");
        assert_eq!(r.chrome.fronts(), 0);
        assert!(r.browser.reqs().is_empty());
        let r2 = Rig::with(cfg(), FakeWall::with_dst(dt(2026, 11, 15, 10, 0), 0, vec![dt(2026, 11, 16, 2, 30)], vec![]));
        assert_eq!(r2.run(&[step]).steps[0].res, res);
    }
}

// ---------- X18 ----------
#[test]
fn x18_sleep_comes_in_slices_of_at_most_poll_and_each_retry_pauses_the_retry_pause() {
    let r = Rig::new();
    r.browser.script(vec![ok(Busy), ok(Busy), ok(Playing)]);
    r.run(&[yt("a b")]);
    assert!(!r.sleeps().is_empty(), "the retry pauses must go through the injected sleep");
    assert!(r.sleeps().iter().all(|d| *d <= POLL && !d.is_zero()), "{:?}", r.sleeps());
    assert_eq!(r.slept_total(), RETRY_PAUSE * 2, "one pause per retry");
    let r = Rig::new();
    r.browser.script(vec![ok(Timeout); 4]);
    r.run(&[yt("a b")]);
    assert_eq!(r.slept_total(), RETRY_PAUSE * 3);
    // the link wait also only sleeps in slices
    let r = Rig::new();
    r.browser.fixed(LinkState::NotFound);
    r.run(&[yt("a b")]);
    assert!(r.sleeps().iter().all(|d| *d <= POLL));
}

// ---------- X19 ----------
#[test]
fn x19_calendar_without_guests_then_youtube_both_run() {
    let r = Rig::new();
    r.browser.script(vec![ok(EditorOpen), ok(Playing)]);
    let steps = [cal("Review", &[]), yt("Halo")];
    let (_, shown) = r.plan(&steps);
    assert_eq!(r.browser.cmds().len(), 2);
    assert!(matches!(r.browser.cmds()[0], Command::CalOpen { .. }) && matches!(r.browser.cmds()[1], Command::YtPlay { .. }));
    assert_eq!(last_body(&shown), vec![line(1, CAL_TEXT), line(2, YT_PLAYING)]);
    assert_eq!(r.chrome.fronts(), 1);
    let r2 = Rig::new();
    r2.browser.script(vec![ok(EditorOpen), ok(Playing)]);
    let rep = r2.run(&steps);
    assert_eq!(rep.steps, vec![sr(OpKind::CalendarOpen, Res::Ext(EditorOpen), Tail::Deferred), sr(OpKind::YoutubePlay, Res::Ext(Playing), Tail::None)]);
    assert_eq!(rep.not_run, 0);
}

// ---------- X20 ----------
#[test]
fn x20_open_url_then_youtube_says_not_available_yet_and_youtube_runs() {
    let r = Rig::new();
    r.browser.script(vec![ok(Playing)]);
    let (_, shown) = r.plan(&[open_url("https://example.com/a"), yt("Halo")]);
    assert_eq!(last_body(&shown), vec![line(1, "Not available yet: Open: example.com/a"), line(2, YT_PLAYING)]);
    assert_eq!(r.browser.reqs().len(), 1);
}

// ---------- X21 ----------
#[test]
fn x21_the_deadline_during_the_link_wait_stops_the_task() {
    let c = ExecConfig { link_wait: Duration::from_secs(100), task_deadline: Duration::from_secs(10), ..cfg() };
    let r = Rig::with(c, FakeWall::india());
    r.browser.fixed(LinkState::NotFound);
    let (_, shown) = r.plan(&[yt("a b"), media(MediaArg::Next)]);
    assert_eq!(last_body(&shown), vec![line(1, DEADLINE_TEXT), "Not run: 1 more step".to_string()]);
    assert!(r.browser.reqs().is_empty());
    let t = elapsed_ms(&r.clock);
    assert!((10_000..=11_000).contains(&t), "stopped at {t} ms, the deadline is 10 s");
    assert!(r.media.presses().is_empty());
}

#[test]
fn x21_a_deadline_that_passes_during_a_request_is_not_retried_and_the_rest_is_not_run() {
    let r = Rig::new();
    let c = r.clock.clone();
    r.browser.set_hook(move |_, _| c.set(95_000));
    r.browser.script(vec![ok(Timeout), ok(Playing)]);
    let (_, shown) = r.plan(&[yt("a b"), media(MediaArg::Next)]);
    assert_eq!(r.browser.reqs().len(), 1);
    assert_eq!(last_body(&shown), vec![line(1, DEADLINE_TEXT), "Not run: 1 more step".to_string()]);
    assert!(r.media.presses().is_empty());
}

#[test]
fn x21_a_web_step_starting_late_gets_a_request_wait_capped_to_the_time_left() {
    let r = Rig::new();
    let c = r.clock.clone();
    // The first request ends 70 s into the 90 s deadline.
    r.browser.set_hook(move |i, _| {
        if i == 0 {
            c.set(70_000);
        }
    });
    r.browser.script(vec![ok(Playing), ok(Playing)]);
    let rep = r.run(&[yt("a b"), yt("c d")]);
    let reqs = r.browser.reqs();
    assert_eq!(reqs.len(), 2);
    assert!(reqs[1].1 <= Duration::from_secs(20), "wait {:?} must not exceed the ~20 s left of the 90 s deadline (yt_wait is 41 s)", reqs[1].1);
    assert!(reqs[1].1 >= Duration::from_secs(15), "and not much below the ~20 s left: {:?}", reqs[1].1);
    assert_eq!(rep.steps.len(), 2);
    // A docs step 85 s in: at most 5 s left.
    let r = Rig::new();
    let c = r.clock.clone();
    r.browser.set_hook(move |i, _| {
        if i == 0 {
            c.set(85_000);
        }
    });
    r.browser.script(vec![ok(Playing), ok(EditorOpen)]);
    r.run(&[yt("a b"), cal("x", &[])]);
    assert!(r.browser.reqs()[1].1 <= Duration::from_secs(5), "{:?}", r.browser.reqs()[1].1);
}

// ---------- X22 ----------
#[test]
fn x22_no_access_needs_login_and_consent_have_their_texts_and_are_not_retried() {
    for (code, text) in [(NoAccess, NO_ACCESS_TEXT), (NeedsLogin, LOGIN_TEXT), (Consent, LOGIN_TEXT)] {
        let r = Rig::new();
        r.browser.script(vec![ok(code), ok(Playing)]);
        assert_eq!(texts(&r, &[yt("a b")]), vec![line(1, text)], "{code:?}");
        assert_eq!(r.browser.reqs().len(), 1, "{code:?} is not retried");
    }
    for code in [NoAccess, NeedsLogin] {
        let r = Rig::new();
        r.browser.script(vec![ok(code), ok(EditorOpen)]);
        assert_eq!(r.run(&[cal("x", &[])]).steps[0].res, Res::Ext(code));
        assert_eq!(r.browser.reqs().len(), 1);
    }
}

// ---------- X23 ----------
#[test]
fn x23_paused_counts_as_ok_and_the_chain_continues() {
    let r = Rig::new();
    r.browser.script(vec![ok(Paused), ok(Playing)]);
    let (_, shown) = r.plan(&[yt("a b"), yt("c d")]);
    assert_eq!(last_body(&shown), vec![line(1, "Video opened but paused: press play in Chrome"), line(2, YT_PLAYING)]);
    let r2 = Rig::new();
    r2.browser.script(vec![ok(Ad), ok(Playing)]);
    assert_eq!(r2.run(&[yt("a b"), yt("c d")]).steps.len(), 2, "ad is ok too");
}

// ---------- X24 ----------
#[test]
fn x24_chrome_comes_to_the_front_once_per_task_even_with_three_web_steps() {
    let r = Rig::new();
    r.browser.script(vec![ok(Playing), ok(EditorOpen), ok(Playing)]);
    r.run(&[yt("a b"), cal("x", &[]), yt("c d")]);
    assert_eq!(r.chrome.fronts(), 1);
    assert_eq!(r.browser.reqs().len(), 3);
    // a second task fronts again (once per task)
    r.browser.script(vec![ok(Playing)]);
    r.run(&[yt("e f")]);
    assert_eq!(r.chrome.fronts(), 2);
}
