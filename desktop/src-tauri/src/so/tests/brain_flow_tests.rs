// SPDX-License-Identifier: FSL-1.1-MIT
//! Step 4 brain flow tests B1-B15 (+ B16-B19 extras): a fake provider, a fake key store, a fake clock and a
//! recording sleep. Log tests run in a child process (see `rules_support`). Texts and numbers follow the
//! error table in draft/step-4-plan.md.
use super::brain_support::*;
use super::rules_support::{captured, in_child, install_capture, run_isolated};
use crate::so::brain::brain::{Limits, ERROR_SECS, PLAN_SECS};
use crate::so::brain::keys::{KeyError, KeyStore};
use crate::so::brain::plan::{parse, render};
use crate::so::brain::provider::{ProviderError, ProviderId};
use crate::so::panel::Kind;
use crate::so::rules::SlowRouter;
use std::sync::Arc;
use std::thread;
use std::time::Duration;

const NOKEY: &str = "AI not set up: Settings → General → AI";
const OFFLINE: &str = "No internet: the AI can't be reached";
const TIMEOUT: &str = "The AI took too long: try again";
const BADREPLY: &str = "The AI's reply wasn't usable: try again";
const LIMIT_HOUR: &str = "AI limit for this hour reached";
const STILL: &str = "Still thinking about the last one";
const KEYCHAIN: &str = "Can't read the AI key from Keychain";
const FAILED: &str = "AI request failed: model or settings problem";
fn rejected(p: &str) -> String {
    format!("{p} rejected the key: check Settings → General → AI")
}
fn busy(p: &str) -> String {
    format!("{p} is busy: try again in a minute")
}
fn problems(p: &str) -> String {
    format!("{p} is having problems: try again later")
}
fn spend(p: &str) -> String {
    format!("{p} spending limit reached: raise it in the {p} console")
}
const S1: Duration = Duration::from_secs(1);
const SAY: &str = "play blinding lights on youtube";

fn one_warn(rig: &BR, say: &str, line: &str) {
    let r = rig.think(say);
    assert_eq!(texts(&r), vec![line.to_string()], "{say:?}");
    assert_eq!(r.lines[0].kind, Kind::Warn);
    assert_eq!(r.hold_secs, ERROR_SECS);
}

// ---------- B1 ----------
#[test]
fn b1_a_valid_plan_shows_the_rendered_lines_for_12_seconds() {
    let rig = BR::new();
    let r = rig.think(SAY);
    let want = render(&parse(PLAN_OK).expect("PLAN_OK parses"));
    assert_eq!(r.lines, want);
    assert_eq!(r.hold_secs, PLAN_SECS);
    assert_eq!(PLAN_SECS, 12);
    assert_eq!(texts(&r), vec!["Plan, not run (2 steps):", "Play on YouTube: Blinding Lights", "Media: next"]);
    assert_eq!(rig.gemini.count(), 1);
    assert!(rig.sleeps().is_empty(), "no wait on success");
}

#[test]
fn b1_the_request_carries_the_prompt_and_the_wrapped_transcript() {
    let rig = BR::new();
    let _ = rig.think(SAY);
    let c = rig.gemini.last();
    assert_eq!(c.user, format!("<command>{SAY}</command>"));
    assert_eq!(c.system, crate::so::brain::prompt::SYSTEM_PROMPT);
    assert_eq!(c.key, GEMINI_KEY);
}

// ---------- B2 ----------
#[test]
fn b2_a_question_shows_ai_asks_and_the_repeat_hint() {
    let rig = BR::new();
    rig.gemini.set_fallback(Ok(QUESTION_OK.to_string()));
    let r = rig.think("play hello");
    assert_eq!(
        texts(&r),
        vec!["AI asks: Which song called Hello: Adele or Lionel Richie?", "Say the full command again with the answer"]
    );
    assert_eq!(r.hold_secs, PLAN_SECS);
    assert!(r.lines.iter().all(|l| l.kind == Kind::Info));
}

// ---------- B3 ----------
#[test]
fn b3_no_key_for_the_chosen_provider_says_so_and_sends_nothing() {
    let rig = BR::no_keys();
    one_warn(&rig, SAY, NOKEY);
    assert_eq!(rig.gemini.count() + rig.claude.count(), 0);
    // A key for the OTHER provider does not help.
    rig.keys.set(ProviderId::Claude, &key(CLAUDE_KEY)).expect("set");
    one_warn(&rig, SAY, NOKEY);
    assert_eq!(rig.claude.count(), 0);
}

// ---------- B4-B8: errors and exact attempt counts ----------
fn err_case(rig: &BR, fallback: ProviderError, attempts: usize, sleeps: Vec<Duration>, line: &str) {
    let chosen = rig.chosen();
    chosen.set_fallback(Err(fallback.clone()));
    let before = chosen.count();
    one_warn(rig, SAY, line);
    assert_eq!(chosen.count() - before, attempts, "{fallback:?}: attempts");
    assert_eq!(rig.sleeps(), sleeps, "{fallback:?}: waits");
    rig.sleeps.lock().unwrap().clear();
}

#[test]
fn b4_offline_is_one_attempt() {
    err_case(&BR::new(), ProviderError::Offline, 1, vec![], OFFLINE);
}

#[test]
fn b5_timeout_is_two_attempts_with_a_one_second_wait() {
    err_case(&BR::new(), ProviderError::Timeout, 2, vec![S1], TIMEOUT);
}

#[test]
fn b6_a_rejected_key_is_one_attempt_and_names_the_provider() {
    err_case(&BR::new(), ProviderError::KeyInvalid, 1, vec![], &rejected("Gemini"));
    let c = BR::new();
    c.choose(ProviderId::Claude, "claude-haiku-5-5");
    err_case(&c, ProviderError::KeyInvalid, 1, vec![], &rejected("Claude"));
}

#[test]
fn b7_rate_limit_retries_once_then_says_busy() {
    err_case(&BR::new(), ProviderError::RateLimited { retry_after_s: None }, 2, vec![S1], &busy("Gemini"));
    err_case(&BR::new(), ProviderError::RateLimited { retry_after_s: Some(2) }, 2, vec![Duration::from_secs(2)], &busy("Gemini"));
    err_case(&BR::new(), ProviderError::RateLimited { retry_after_s: Some(30) }, 1, vec![], &busy("Gemini"));
    let c = BR::new();
    c.choose(ProviderId::Claude, "claude-haiku-5-5");
    err_case(&c, ProviderError::RateLimited { retry_after_s: None }, 2, vec![S1], &busy("Claude"));
}

#[test]
fn b8_server_errors_retry_once_then_say_problems() {
    for status in [500u16, 503, 529] {
        err_case(&BR::new(), ProviderError::Server(status), 2, vec![S1], &problems("Gemini"));
    }
    let c = BR::new();
    c.choose(ProviderId::Claude, "claude-haiku-5-5");
    err_case(&c, ProviderError::Server(529), 2, vec![S1], &problems("Claude"));
}

#[test]
fn b8_a_retry_that_works_shows_the_plan() {
    for first in [ProviderError::Timeout, ProviderError::Server(503), ProviderError::RateLimited { retry_after_s: Some(1) }] {
        let rig = BR::new();
        rig.gemini.then(Err(first.clone()));
        let r = rig.think(SAY);
        assert_eq!(texts(&r)[0], "Plan, not run (2 steps):", "{first:?}");
        assert_eq!(rig.gemini.count(), 2);
        assert_eq!(rig.sleeps().len(), 1);
    }
}

// ---------- B9 ----------
#[test]
fn b9_a_bad_reply_is_rejected_whole_and_never_retried() {
    let raws = [
        "not json at all".to_string(),
        r#"{"v":1,"kind":"plan","steps":[{"action":"docs_new","args":{"topic":"t"},"risk":"safe"}]}"#.to_string(),
        r#"{"v":2,"kind":"question","question":"q?"}"#.to_string(),
        format!(
            r#"{{"v":1,"kind":"plan","steps":[{}]}}"#,
            vec![r#"{"action":"web_search","args":{"query":"q"}}"#; 26].join(",")
        ),
    ];
    for raw in raws {
        let rig = BR::new();
        rig.gemini.set_fallback(Ok(raw.clone()));
        let r = rig.think(SAY);
        assert_eq!(texts(&r), vec![BADREPLY.to_string()], "{raw}");
        assert_eq!(rig.gemini.count(), 1, "a bad reply is not retried: {raw}");
        assert_eq!(r.hold_secs, ERROR_SECS);
    }
    let rig = BR::new();
    rig.gemini.set_fallback(Err(ProviderError::BadReply));
    assert_eq!(texts(&rig.think(SAY)), vec![BADREPLY.to_string()], "a refusal / blocked / cut-off reply from the provider");
    assert_eq!(rig.gemini.count(), 1);
}

// ---------- B10 ----------
#[test]
fn b10_wants_needs_two_words_after_normalising() {
    let rig = BR::new();
    let b = &rig.brain;
    for yes in ["play blinding lights", "open youtube", "hello, world", "  open   youtube ", "Play Blinding Lights!", "caf\u{e9} au lait"] {
        assert!(b.wants(yes), "{yes:?}");
    }
    for no in ["", "   ", "pause", "Paws", "Pause!", "pause please", "please pause", "...", "?!", "a"] {
        assert!(!b.wants(no), "{no:?}");
    }
    assert_eq!(rig.gemini.count(), 0, "wants never calls the provider");
}

// ---------- B11 ----------
#[test]
fn b11_the_121st_call_in_an_hour_is_refused_and_a_rolling_hour_frees_it() {
    let rig = BR::new();
    for i in 0..120 {
        let r = rig.think(SAY);
        assert_eq!(texts(&r)[0], "Plan, not run (2 steps):", "call {i}");
    }
    assert_eq!(rig.gemini.count(), 120);
    one_warn(&rig, SAY, LIMIT_HOUR);
    assert_eq!(rig.gemini.count(), 120, "nothing is sent over the limit");
    rig.clock.set(3_599_000);
    one_warn(&rig, SAY, LIMIT_HOUR);
    rig.clock.set(3_600_001);
    assert_eq!(texts(&rig.think(SAY))[0], "Plan, not run (2 steps):", "an hour later it works again");
}

#[test]
fn b11_the_limits_come_from_the_limits_struct() {
    let rig = BR::with_limits(Limits { max_in_flight: 2, max_per_hour: 3 });
    for _ in 0..3 {
        assert_eq!(texts(&rig.think(SAY))[0], "Plan, not run (2 steps):");
    }
    one_warn(&rig, SAY, LIMIT_HOUR);
}

#[test]
fn b11_a_third_call_at_the_same_time_says_still_thinking() {
    let rig = BR::new();
    let gate = Arc::new(Gate::default());
    *rig.gemini.gate.lock().unwrap() = Some(gate.clone());
    let mut hs = vec![];
    for _ in 0..2 {
        let (b, at) = (rig.brain.clone(), rig.clock.at(0));
        hs.push(thread::spawn(move || b.think(SAY, at)));
    }
    wait_for("two calls inside the provider", || gate.entered.load(std::sync::atomic::Ordering::SeqCst) == 2);
    one_warn(&rig, SAY, STILL);
    assert_eq!(rig.gemini.count(), 2, "the third call sent nothing");
    gate.release();
    for h in hs {
        assert_eq!(texts(&h.join().expect("no panic"))[0], "Plan, not run (2 steps):");
    }
    assert_eq!(texts(&rig.think(SAY))[0], "Plan, not run (2 steps):", "the slots are free again");
}

#[test]
fn b11_a_failed_or_panicking_call_gives_its_slot_back() {
    let rig = BR::new();
    *rig.gemini.panic_with.lock().unwrap() = Some("boom".into());
    for _ in 0..3 {
        one_warn(&rig, SAY, FAILED); // more than max_in_flight panics in a row: slots must be released
    }
    *rig.gemini.panic_with.lock().unwrap() = None;
    rig.gemini.set_fallback(Err(ProviderError::Offline));
    for _ in 0..3 {
        one_warn(&rig, SAY, OFFLINE);
    }
}

fn wait_for(what: &str, f: impl Fn() -> bool) {
    super::rules_support::wait_until(what, Duration::from_secs(3), f)
}

// ---------- B12 ----------
#[test]
fn b12_a_locked_or_denied_keychain_gives_the_keychain_line() {
    use crate::so::brain::brain::Brain;
    use crate::so::brain::provider::Provider;
    for e in [KeyError::Locked, KeyError::Denied] {
        let p = Scripted::new(ProviderId::Gemini, Ok(PLAN_OK.to_string()));
        let clock = super::rules_support::FakeClock::new();
        let brain = Brain::new(
            vec![p.clone() as Arc<dyn Provider>],
            Arc::new(FailStore(e)),
            Arc::new(|| crate::so::brain::brain::BrainChoice { provider: ProviderId::Gemini, model: "gemini-3.5-flash-lite".into() }),
            clock.clone(),
            Arc::new(|_| {}),
            Limits { max_in_flight: 2, max_per_hour: 120 },
        );
        let r = brain.think(SAY, clock.at(0));
        assert_eq!(texts(&r), vec![KEYCHAIN.to_string()], "{e:?}");
        assert_eq!(r.lines[0].kind, Kind::Warn);
        assert_eq!(r.hold_secs, ERROR_SECS);
        assert_eq!(p.count(), 0, "no request without a key");
    }
}

// ---------- B13 ----------
#[test]
fn b13_the_chosen_provider_is_called_with_its_own_key_and_model() {
    let rig = BR::new();
    let _ = rig.think(SAY);
    let g = rig.gemini.last();
    assert_eq!((g.key.as_str(), g.model.as_str()), (GEMINI_KEY, "gemini-3.5-flash-lite"));
    assert_eq!(rig.claude.count(), 0);
    rig.choose(ProviderId::Claude, "claude-sonnet-5-5");
    let _ = rig.think(SAY);
    let c = rig.claude.last();
    assert_eq!((c.key.as_str(), c.model.as_str()), (CLAUDE_KEY, "claude-sonnet-5-5"));
    assert_eq!(rig.gemini.count(), 1, "the other provider is not called");
}

#[test]
fn b13_a_bad_model_override_falls_back_to_the_default_a_good_one_is_used() {
    let rig = BR::new();
    for bad in ["Bad Model!", "", "../x", "a:generateContent", "UPPER", &"a".repeat(65)] {
        rig.choose(ProviderId::Gemini, bad);
        let _ = rig.think(SAY);
        assert_eq!(rig.gemini.last().model, "gemini-3.5-flash-lite", "{bad:?}");
        rig.choose(ProviderId::Claude, bad);
        let _ = rig.think(SAY);
        assert_eq!(rig.claude.last().model, "claude-haiku-5-5", "{bad:?}");
    }
    rig.choose(ProviderId::Claude, "claude-opus-5-5");
    let _ = rig.think(SAY);
    assert_eq!(rig.claude.last().model, "claude-opus-5-5", "a well-formed override is used as given");
}

// ---------- B14 ----------
#[test]
fn b14_panel_lines_never_hold_provider_text_or_status_numbers() {
    let hostile = [
        format!(r#"{{"v":1,"kind":"plan","steps":[{{"action":"pay_{REPLY_WORD}","args":{{"to":"{REPLY_WORD}"}}}}]}}"#),
        format!("not json {REPLY_WORD} {BODY_WORD}"),
        format!(r#"{{"v":1,"kind":"plan","steps":[{{"action":"docs_new","args":{{"topic":"t"}},"risk":"{REPLY_WORD}"}}]}}"#),
        format!(r#"{{"v":1,"kind":"question","question":"{}"}}"#, format!("{REPLY_WORD} ").repeat(40)),
    ];
    for raw in hostile {
        let rig = BR::new();
        rig.gemini.set_fallback(Ok(raw.clone()));
        let r = rig.think(SAY);
        let shown = texts(&r).join("|").to_lowercase();
        assert!(!shown.contains(REPLY_WORD) && !shown.contains(BODY_WORD), "provider text on the panel for {raw}: {shown}");
    }
    for e in [ProviderError::BadRequest(418), ProviderError::Server(503), ProviderError::NotFound, ProviderError::Forbidden] {
        let rig = BR::new();
        rig.gemini.set_fallback(Err(e.clone()));
        let r = rig.think(SAY);
        let shown = texts(&r).join("|");
        assert!(!shown.chars().any(|c| c.is_ascii_digit()), "a status number on the panel for {e:?}: {shown}");
    }
    // Positive control: a good question IS shown (so the check above is about rejection, not blanking).
    let rig = BR::new();
    rig.gemini.set_fallback(Ok(format!(r#"{{"v":1,"kind":"question","question":"Which {REPLY_WORD}?"}}"#)));
    assert!(texts(&rig.think(SAY)).join("|").contains(REPLY_WORD));
}

// ---------- B15 ----------
#[test]
fn b15_a_spend_limit_gets_its_own_line_never_the_rejected_key_line() {
    let rig = BR::new();
    rig.gemini.set_fallback(Err(ProviderError::SpendLimit));
    one_warn(&rig, SAY, &spend("Gemini"));
    assert_eq!(rig.gemini.count(), 1, "a spend limit is not retried");
    let c = BR::new();
    c.choose(ProviderId::Claude, "claude-haiku-5-5");
    c.claude.set_fallback(Err(ProviderError::SpendLimit));
    one_warn(&c, SAY, &spend("Claude"));
    assert!(!texts(&c.think(SAY))[0].contains("rejected"));
}

// ---------- B16-B19: extras the plan's table implies ----------
#[test]
fn b16_model_and_settings_problems_give_the_failed_line_with_one_attempt() {
    for e in [ProviderError::NotFound, ProviderError::BadRequest(400), ProviderError::BadRequest(413), ProviderError::Forbidden] {
        let rig = BR::new();
        rig.gemini.set_fallback(Err(e.clone()));
        one_warn(&rig, SAY, FAILED);
        assert_eq!(rig.gemini.count(), 1, "{e:?}");
    }
}

#[test]
fn b17_the_key_is_read_on_every_call_never_cached() {
    let rig = BR::new();
    let _ = rig.think(SAY);
    assert_eq!(rig.gemini.last().key, GEMINI_KEY);
    rig.keys.set(ProviderId::Gemini, &key(KEY_SENTINEL)).expect("replace");
    let _ = rig.think(SAY);
    assert_eq!(rig.gemini.last().key, KEY_SENTINEL, "the new key is used at once");
    rig.keys.delete(ProviderId::Gemini).expect("remove");
    one_warn(&rig, SAY, NOKEY);
    assert_eq!(rig.gemini.count(), 2);
}

#[test]
fn b18_each_talk_is_a_fresh_request_with_no_memory() {
    let rig = BR::new();
    let _ = rig.think("play blinding lights");
    let _ = rig.think("open youtube");
    let c = rig.gemini.calls.lock().unwrap().clone();
    assert_eq!(c[1].user, "<command>open youtube</command>");
    assert!(!c[1].user.contains("blinding"), "the previous talk leaked into the next request");
}

#[test]
fn b19_the_reply_hold_times_are_plan_12_error_6() {
    assert_eq!((PLAN_SECS, ERROR_SECS), (12, 6));
    assert_eq!(crate::so::brain::brain::MIN_WORDS, 2);
}

// ---------- metrics (isolated: the log is process-wide) ----------
#[test]
fn b_metrics_one_line_per_think_with_integer_codes_only() {
    run_isolated(module_path!(), "b_metrics_inner");
}
#[test]
#[ignore]
fn b_metrics_inner() {
    if !in_child() {
        return;
    }
    install_capture();
    let baseline = captured().len();
    let run = |rig: &BR, say: &str| {
        let _ = rig.think(say);
    };
    let with = |e: ProviderError| {
        let r = BR::new();
        r.gemini.set_fallback(Err(e));
        r
    };
    let raw = |s: &str| {
        let r = BR::new();
        r.gemini.set_fallback(Ok(s.to_string()));
        r
    };
    // 1 plan, 2 question, 3 no key, 4 offline, 5 timeout, 6 key invalid, 7 busy, 8 server
    run(&BR::new(), SAY);
    run(&raw(QUESTION_OK), SAY);
    run(&BR::no_keys(), SAY);
    run(&with(ProviderError::Offline), SAY);
    run(&with(ProviderError::Timeout), SAY);
    run(&with(ProviderError::KeyInvalid), SAY);
    run(&with(ProviderError::RateLimited { retry_after_s: None }), SAY);
    run(&with(ProviderError::Server(500)), SAY);
    // 9 bad reply (reject 2 = NotJson), 9 again (reject 9 = TooManySteps)
    run(&raw("nope"), SAY);
    let many = format!(r#"{{"v":1,"kind":"plan","steps":[{}]}}"#, vec![r#"{"action":"web_search","args":{"query":"q"}}"#; 26].join(","));
    run(&raw(&many), SAY);
    // 10 too long
    run(&BR::new(), &"a".repeat(1001));
    // 11 hour limit
    let lim = BR::with_limits(Limits { max_in_flight: 2, max_per_hour: 1 });
    run(&lim, SAY);
    run(&lim, SAY);
    // 13 keychain, 14 not found, 15 spend
    {
        use crate::so::brain::brain::{Brain, BrainChoice};
        use crate::so::brain::provider::Provider;
        let p = Scripted::new(ProviderId::Gemini, Ok(PLAN_OK.to_string()));
        let clock = super::rules_support::FakeClock::new();
        let b = Brain::new(
            vec![p as Arc<dyn Provider>],
            Arc::new(FailStore(KeyError::Locked)),
            Arc::new(|| BrainChoice { provider: ProviderId::Gemini, model: "gemini-3.5-flash-lite".into() }),
            clock.clone(),
            Arc::new(|_| {}),
            Limits { max_in_flight: 2, max_per_hour: 120 },
        );
        let _ = b.think(SAY, clock.at(0));
    }
    run(&with(ProviderError::NotFound), SAY);
    run(&with(ProviderError::SpendLimit), SAY);
    // Both providers on a good plan: the provider code differs.
    let both = BR::new();
    run(&both, SAY);
    both.choose(ProviderId::Claude, "claude-haiku-5-5");
    run(&both, SAY);

    let recs = brain_records(&captured().into_iter().skip(baseline).collect::<Vec<_>>());
    let got = outcomes(&recs);
    assert_eq!(got, vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 9, 10, 1, 11, 13, 14, 15, 1, 1], "{recs:#?}");
    assert_eq!(field(&recs[0], "steps"), Some(2));
    assert_eq!(actions_of(&recs[0]), vec![5, 2], "action codes: youtube_play = 5, media = 2: {}", recs[0]);
    assert_eq!(field(&recs[1], "steps"), Some(0), "a question has no steps");
    assert_eq!(field(&recs[8], "reject"), Some(2), "NotJson");
    assert_eq!(field(&recs[9], "reject"), Some(9), "TooManySteps");
    for r in &recs {
        for f in ["outcome", "provider", "ms", "steps", "http", "reject"] {
            assert!(field(r, f).is_some(), "field {f} is not an integer in: {r}");
        }
        assert!(r.contains("actions="), "{r}");
    }
    let (g, c) = (field(&recs[16], "provider").unwrap(), field(&recs[17], "provider").unwrap());
    assert_ne!(g, c, "Gemini and Claude need different provider codes");
    assert_eq!(leaks(&recs), Vec::<String>::new());
    // no plan text in the log either
    for r in &recs {
        let low = r.to_lowercase();
        assert!(!low.contains("blinding") && !low.contains("youtube"), "plan or transcript text in {r}");
    }
}
