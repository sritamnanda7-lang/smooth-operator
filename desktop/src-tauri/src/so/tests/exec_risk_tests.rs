// SPDX-License-Identifier: FSL-1.1-MIT
//! Step 6 R1-R4: the op table and the risk gate. R1/R2 are pure; R3/R4 run the executor with fakes.
//! The executor must recompute risk from the action (`op_risk`) and never read `Step.risk`.
use super::exec_support::*;
use crate::so::brain::plan::{op_risk, ops_of, risk_of, Action, MediaArg, OpKind, Risk, Step};
use crate::so::exec::{Res, StepResult, Tail};
use crate::so::link::protocol::{Command, ResultCode};
use crate::so::rules::phrase::MediaKey;

fn all_actions() -> Vec<Action> {
    vec![
        Action::OpenApp { name: "Notes".into() },
        Action::SwitchApp { name: "Notes".into() },
        Action::Media { key: MediaArg::Next },
        Action::OpenUrl { url: "https://example.com/".into() },
        Action::WebSearch { query: "cats".into() },
        Action::YoutubePlay { query: "Blinding Lights".into() },
        Action::DocsNew { topic: "WWII".into(), words: None },
        Action::CalendarNewEvent { title: "T".into(), date: "tomorrow".into(), time: "15:00".into(), minutes: None, guests: vec![] },
    ]
}
fn cal_guests() -> Action {
    Action::CalendarNewEvent { title: "T".into(), date: "tomorrow".into(), time: "15:00".into(), minutes: None, guests: vec!["a@b.co".into()] }
}

// ---------- R1 ----------
#[test]
fn r1_ops_of_lists_the_fixed_ops_for_all_eight_actions() {
    use OpKind::*;
    let want: [&[OpKind]; 8] = [&[AppOpen], &[AppSwitch], &[MediaKey], &[OpenUrl], &[WebSearch], &[YoutubePlay], &[DocsOpen, DocsPaste], &[CalendarOpen, CalendarSave]];
    let actions = all_actions();
    assert_eq!(actions.len(), 8);
    for (a, w) in actions.iter().zip(want) {
        assert_eq!(ops_of(a), w, "{a:?}");
    }
    assert_eq!(ops_of(&cal_guests()), &[CalendarOpen, CalendarSave], "guests change the risk, not the op list");
}

// ---------- R2 ----------
#[test]
fn r2_op_risk_table() {
    use OpKind::*;
    let docs = Action::DocsNew { topic: "t".into(), words: None };
    assert_eq!(op_risk(DocsOpen, &docs), Risk::Safe, "Q1: opening the blank doc runs without approval in step 6");
    assert_eq!(op_risk(DocsPaste, &docs), Risk::Approve);
    let cal = &all_actions()[7];
    assert_eq!(op_risk(CalendarOpen, cal), Risk::Safe);
    assert_eq!(op_risk(CalendarSave, cal), Risk::Safe, "no guests: Save is Safe (not built yet)");
    assert_eq!(op_risk(CalendarOpen, &cal_guests()), Risk::Safe);
    assert_eq!(op_risk(CalendarSave, &cal_guests()), Risk::Approve, "guests: Save needs approval");
}

#[test]
fn r2_every_op_of_every_action_matches_the_table_and_risk_of_is_unchanged() {
    for a in all_actions().iter().chain(std::iter::once(&cal_guests())) {
        for op in ops_of(a) {
            let want = match op {
                OpKind::DocsPaste => Risk::Approve,
                OpKind::CalendarSave => risk_of(a),
                _ => Risk::Safe,
            };
            assert_eq!(op_risk(*op, a), want, "{op:?} of {a:?}");
        }
    }
    assert_eq!(risk_of(&Action::DocsNew { topic: "t".into(), words: None }), Risk::Approve, "J-tests unchanged");
    assert_eq!(risk_of(&cal_guests()), Risk::Approve);
    assert_eq!(risk_of(&all_actions()[7]), Risk::Safe);
    assert_eq!(risk_of(&all_actions()[5]), Risk::Safe);
}

// ---------- R3 ----------
#[test]
fn r3_a_forged_safe_docs_step_runs_docsopen_and_the_tail_needs_ok() {
    let r = Rig::new();
    r.browser.script(vec![ok(ResultCode::EditorOpen)]);
    let forged = Step { action: Action::DocsNew { topic: "WWII".into(), words: Some(300) }, risk: Risk::Safe };
    let rep = r.run(&[forged]);
    assert_eq!(r.browser.cmds(), vec![Command::DocsOpen]);
    assert_eq!(rep.steps, vec![StepResult { first: OpKind::DocsOpen, res: Res::Ext(ResultCode::EditorOpen), tail: Tail::NeedsOk }]);
}

#[test]
fn r3_a_forged_safe_calendar_with_guests_still_needs_ok_and_without_guests_is_deferred() {
    let r = Rig::new();
    r.browser.script(vec![ok(ResultCode::EditorOpen), ok(ResultCode::EditorOpen)]);
    let forged = Step { action: cal_guests(), risk: Risk::Safe };
    let rep = r.run(&[forged]);
    assert_eq!(rep.steps, vec![StepResult { first: OpKind::CalendarOpen, res: Res::Ext(ResultCode::EditorOpen), tail: Tail::NeedsOk }]);
    // The other way round: a forged Approve on a no-guest event must not add approval.
    let plain = Step { action: all_actions().remove(7), risk: Risk::Approve };
    let rep = r.run(&[plain]);
    assert_eq!(rep.steps, vec![StepResult { first: OpKind::CalendarOpen, res: Res::Ext(ResultCode::EditorOpen), tail: Tail::Deferred }]);
}

#[test]
fn r3_a_forged_blocked_risk_on_a_safe_action_does_not_stop_it() {
    let r = Rig::new();
    r.browser.script(vec![ok(ResultCode::Playing)]);
    let rep = r.run(&[Step { action: Action::YoutubePlay { query: "x y".into() }, risk: Risk::Blocked }]);
    assert_eq!(rep.steps, vec![StepResult { first: OpKind::YoutubePlay, res: Res::Ext(ResultCode::Playing), tail: Tail::None }]);
}

// ---------- R4 ----------
#[test]
fn r4_open_url_and_web_search_are_not_yet_with_no_chrome_and_no_request_and_the_next_step_runs() {
    let r = Rig::new();
    let steps = [open_url("https://example.com/a"), web_search("cats"), media(MediaArg::Next)];
    let rep = r.run(&steps);
    assert_eq!(rep.steps.len(), 3, "all three steps ran to a result: the chain continues past NotYet");
    assert_eq!(rep.steps[0], StepResult { first: OpKind::OpenUrl, res: Res::NotYet, tail: Tail::None });
    assert_eq!(rep.steps[1], StepResult { first: OpKind::WebSearch, res: Res::NotYet, tail: Tail::None });
    assert_eq!(rep.steps[2].first, OpKind::MediaKey);
    assert_eq!(rep.steps[2].res, Res::Done);
    assert_eq!(r.chrome.fronts(), 0, "Chrome is not touched");
    assert!(r.browser.reqs().is_empty(), "no request");
    assert_eq!(r.media.presses(), vec![MediaKey::Next], "the next step still ran");
    assert_eq!(rep.not_run, 0);
}
