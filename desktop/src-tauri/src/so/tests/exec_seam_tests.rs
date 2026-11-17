// SPDX-License-Identifier: FSL-1.1-MIT
//! Step 6 P1-P5: how a parsed plan reaches the executor (`SpeechSink::with_runner`, `SlowRouter::think_plan`,
//! `Brain::think_plan`, the debug fake phrases). Fakes only: `PlanBrain`, `FakeRunner`, the step-3/4 rig fakes.
use super::brain_support::*;
use super::exec_support::*;
use super::rules_support::*;
use super::rules_support::Rig; // the step-3 rig (exec_support has its own Rig)
use crate::so::brain::plan::{risk_of, Action, MediaArg, Risk, Step};
use crate::so::panel::PanelPayload;
use crate::so::rules::{Reply, Route, SlowRouter, THINKING_TEXT};
use crate::so::settings::HotkeyChoice;
use crate::so::stt::sink::SpeechSink;
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};
use std::time::Duration;

fn plan_reply() -> Reply {
    reply(vec![info("Plan, not run (2 steps):"), info("Play on YouTube: Blinding Lights"), info("Media: next")], 12)
}
fn two_steps() -> Vec<Step> {
    vec![yt("Blinding Lights"), media(MediaArg::Next)]
}
fn runner_rig(router: Arc<dyn crate::so::rules::TranscriptRouter>, brain: Arc<dyn SlowRouter>, runner: Arc<FakeRunner>, text: &str) -> Rig {
    let log = Arc::new(Log::default());
    let clock = FakeClock::new();
    let rec = Arc::new(FakeRec { samples: Mutex::new(Some(vec![0.1; SAMPLES])) });
    let tr = Arc::new(FakeTr { log: log.clone(), result: Mutex::new(Ok(text.into())), delay: Mutex::new(Duration::ZERO) });
    let panel = Arc::new(FakePanel::default());
    let sink = SpeechSink::with_runner(
        rec.clone(),
        tr.clone(),
        panel.clone(),
        clock.clone(),
        Arc::new(FakeTap),
        Arc::new(FakeStopper),
        Box::new(|| HotkeyChoice::Fn),
        router,
        brain,
        runner,
    );
    Rig { log, rec, tr, panel, clock, sink: Arc::new(sink) }
}
fn pass() -> Arc<dyn crate::so::rules::TranscriptRouter> {
    FakeRouter::new(Arc::new(Log::default()), Route::PassThrough)
}
fn runner_lines() -> Vec<Vec<crate::so::panel::Line>> {
    vec![vec![info("Working (step 1 of 2): Play on YouTube: Blinding Lights")], vec![info("1. Playing on YouTube"), info("Not run: 1 more step")]]
}

// ---------- P1 ----------
#[test]
fn p1_a_plan_goes_to_the_runner_with_exactly_the_parsed_steps_and_heard_comes_first() {
    let brain = PlanBrain::new(plan_reply(), two_steps());
    let runner = FakeRunner::new(runner_lines(), 1);
    let rig = runner_rig(pass(), brain.clone(), runner.clone(), "play blinding lights");
    rig.ptt();
    rig.wait_line("Not run: 1 more step");
    assert_eq!(runner.calls(), vec![two_steps()], "exactly the parsed steps, once");
    assert_eq!(brain.plan_calls.load(Ordering::SeqCst), 1, "the seam asks think_plan, not think");
    let heard = info("Heard: play blinding lights");
    assert_eq!(rig.panel.last_payload().unwrap(), PanelPayload::new(vec![heard.clone(), info("1. Playing on YouTube"), info("Not run: 1 more step")]));
    for p in rig.panel.shows() {
        let texts: Vec<&str> = p.lines.iter().map(|l| l.text.as_str()).collect();
        if texts.iter().any(|t| t.starts_with("Working") || t.starts_with("1. ")) {
            assert_eq!(texts[0], "Heard: play blinding lights", "Heard first: {texts:?}");
        }
    }
    assert!(rig.panel.all_line_texts().contains(&THINKING_TEXT.to_string()), "Thinking… still shows while the brain works");
    // the hold seconds the runner returned decide when the panel hides
    wait_until("hide after the runner's hold", Duration::from_secs(4), || rig.panel.hides() == 1);
}

#[test]
fn p1_a_question_does_not_run_anything() {
    let q = reply(vec![info("AI asks: Which one?"), info("Say the full command again with the answer")], 8);
    let brain = PlanBrain::new(q, vec![]);
    let runner = FakeRunner::new(runner_lines(), 1);
    let rig = runner_rig(pass(), brain.clone(), runner.clone(), "play hello please");
    rig.ptt();
    rig.wait_line("AI asks: Which one?");
    settle();
    assert!(runner.calls().is_empty());
    assert_eq!(rig.panel.last_payload().unwrap(), PanelPayload::new(vec![info("Heard: play hello please"), info("AI asks: Which one?"), info("Say the full command again with the answer")]));
}

#[test]
fn p1_a_fast_rule_that_handled_the_phrase_runs_nothing_and_asks_no_brain() {
    let brain = PlanBrain::new(plan_reply(), two_steps());
    let runner = FakeRunner::new(runner_lines(), 1);
    let router = FakeRouter::new(Arc::new(Log::default()), Route::Handled(vec![info("Play/pause sent")]));
    let rig = runner_rig(router, brain.clone(), runner.clone(), "pause please now");
    rig.ptt();
    rig.wait_line("Play/pause sent");
    settle();
    assert!(runner.calls().is_empty());
    assert_eq!(brain.plan_calls.load(Ordering::SeqCst) + brain.think_calls.load(Ordering::SeqCst), 0);
}

#[test]
fn p1_a_brain_reply_with_steps_but_no_runner_never_runs_them() {
    // P2's other half: the same brain, the step-4 constructor.
    let brain = PlanBrain::new(plan_reply(), two_steps());
    let log = Arc::new(Log::default());
    let rig = brain_rig(FakeClock::new(), log, pass(), brain, "play blinding lights");
    rig.ptt();
    rig.wait_line("Media: next");
    assert_eq!(
        rig.panel.last_payload().unwrap(),
        PanelPayload::new(vec![info("Heard: play blinding lights"), info("Plan, not run (2 steps):"), info("Play on YouTube: Blinding Lights"), info("Media: next")])
    );
}

// ---------- P2 ----------
#[test]
fn p2_with_brain_and_no_runner_the_step_4_lines_are_unchanged() {
    let brain = FakeBrain::new(reply(vec![info("Plan, not run (2 steps):"), info("Play on YouTube: Blinding Lights"), info("Media: next")], 12));
    let log = Arc::new(Log::default());
    let rig = brain_rig(FakeClock::new(), log, pass(), brain.clone(), "play blinding lights");
    rig.ptt();
    rig.wait_line("Media: next");
    assert_eq!(
        rig.panel.last_payload().unwrap(),
        PanelPayload::new(vec![info("Heard: play blinding lights"), info("Plan, not run (2 steps):"), info("Play on YouTube: Blinding Lights"), info("Media: next")])
    );
    assert_eq!(brain.texts(), vec!["play blinding lights"]);
}

// ---------- P3 ----------
#[test]
fn p3_a_newer_talk_before_the_reply_means_the_old_plan_is_not_run() {
    let brain = PlanBrain::new(plan_reply(), two_steps());
    *brain.delay.lock().unwrap() = Duration::from_millis(500);
    let runner = FakeRunner::new(runner_lines(), 1);
    let rig = runner_rig(pass(), brain.clone(), runner.clone(), "play blinding lights");
    rig.ptt();
    rig.wait_line(THINKING_TEXT);
    rig.say("paws");
    rig.wait_line("Heard: paws");
    wait_until("the brain answered", Duration::from_secs(3), || brain.plan_calls.load(Ordering::SeqCst) == 1);
    std::thread::sleep(Duration::from_millis(800));
    assert!(runner.calls().is_empty(), "a talk that lost the panel must not run its plan");
    assert_eq!(rig.panel.last_payload().unwrap(), PanelPayload::new(vec![info("Heard: paws")]));
}

// ---------- P4 ----------
const RISKY: &str = r#"{"v":1,"kind":"plan","steps":[{"action":"docs_new","args":{"topic":"WWII essay"}},{"action":"calendar_new_event","args":{"title":"Sync","date":"friday","time":"10:00","guests":["a@b.co"]}},{"action":"youtube_play","args":{"query":"Halo"}}]}"#;

#[test]
fn p4_brain_think_plan_returns_the_steps_with_risk_from_the_table() {
    let br = BR::new();
    br.gemini.then(Ok(RISKY.to_string()));
    let (r, steps) = br.brain.think_plan("do three things now", br.clock.at(0));
    assert_eq!(steps.len(), 3, "{:?}", texts(&r));
    for s in &steps {
        assert_eq!(s.risk, risk_of(&s.action), "{:?}", s.action);
    }
    assert_eq!(steps[0].risk, Risk::Approve, "risk comes from the table, not from the reply");
    assert_eq!(steps[1].risk, Risk::Approve);
    assert_eq!(steps[2], Step { action: Action::YoutubePlay { query: "Halo".into() }, risk: Risk::Safe });
    assert!(!r.lines.is_empty());
}

#[test]
fn p4_think_lines_are_unchanged_and_a_question_has_no_steps() {
    let a = BR::new();
    let b = BR::new();
    a.gemini.then(Ok(RISKY.to_string()));
    b.gemini.then(Ok(RISKY.to_string()));
    let lines_think = texts(&a.think("do three things now"));
    let (r, steps) = b.brain.think_plan("do three things now", b.clock.at(0));
    assert!(!steps.is_empty());
    assert_eq!(texts(&r), lines_think, "think() and think_plan() show the same lines");
    assert!(lines_think[0].starts_with("Plan, not run"), "{lines_think:?}");
    let q = BR::new();
    q.gemini.then(Ok(QUESTION_OK.to_string()));
    let (r, steps) = q.brain.think_plan("play hello please", q.clock.at(0));
    assert!(steps.is_empty());
    assert!(texts(&r)[0].starts_with("AI asks: "));
    let bad = BR::new();
    bad.gemini.then(Ok("this is not json".to_string()));
    let (_, steps) = bad.brain.think_plan("play hello please", bad.clock.at(0));
    assert!(steps.is_empty(), "a rejected reply has no steps");
}

#[test]
fn p4_the_default_think_plan_of_a_plain_slow_router_has_no_steps() {
    let b = FakeBrain::new(reply(vec![info("x")], 1));
    let (r, steps) = b.think_plan("a b", std::time::Instant::now());
    assert_eq!(texts(&r), vec!["x"]);
    assert!(steps.is_empty());
}

// ---------- P5 (debug builds only, like the fake) ----------
#[cfg(debug_assertions)]
mod p5 {
    use super::*;
    use crate::so::brain::fake;

    fn plan(phrase: &str) -> Vec<Step> {
        let f = fake::front();
        let (_, steps) = f.think_plan(phrase, std::time::Instant::now());
        for s in &steps {
            assert_eq!(s.risk, risk_of(&s.action), "{phrase}: risk comes from the table");
        }
        steps
    }

    #[test]
    fn p5_fake_play_is_one_youtube_step() {
        let s = plan("fake play now");
        assert!(matches!(&s[..], [Step { action: Action::YoutubePlay { query }, .. }] if !query.is_empty()), "{s:?}");
    }
    #[test]
    fn p5_fake_docs_is_one_docs_step() {
        let s = plan("fake docs now");
        assert!(matches!(&s[..], [Step { action: Action::DocsNew { .. }, risk: Risk::Approve }]), "{s:?}");
    }
    #[test]
    fn p5_fake_calendar_is_one_calendar_step_without_guests() {
        let s = plan("fake calendar now");
        assert!(matches!(&s[..], [Step { action: Action::CalendarNewEvent { guests, .. }, risk: Risk::Safe }] if guests.is_empty()), "{s:?}");
    }
    #[test]
    fn p5_fake_chain_has_a_calendar_step_and_a_youtube_step() {
        let s = plan("fake chain now");
        assert!(s.len() >= 2, "{s:?}");
        assert!(s.iter().any(|x| matches!(x.action, Action::CalendarNewEvent { .. })), "{s:?}");
        assert!(s.iter().any(|x| matches!(x.action, Action::YoutubePlay { .. })), "{s:?}");
    }
    #[test]
    fn p5_fake_url_has_an_open_url_step() {
        let s = plan("fake url now");
        assert!(s.iter().any(|x| matches!(x.action, Action::OpenUrl { .. })), "{s:?}");
    }
    #[test]
    fn p5_the_default_plan_is_unchanged() {
        let s = plan("anything else here");
        assert_eq!(s, vec![yt("Blinding Lights"), media(MediaArg::Next)]);
    }
}
