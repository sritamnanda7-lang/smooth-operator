// SPDX-License-Identifier: FSL-1.1-MIT
//! The plan: strict schema, parser, risk table, panel rendering. Pure code, no I/O.
//! Risk is set here by code (`risk_of`), never by the AI.

use super::provider::ProviderId;
use crate::so::panel::Line;

pub const PLAN_VERSION: u8 = 1;
pub const MAX_STEPS: usize = 25;
pub const MAX_REPLY_BYTES: usize = 16_384;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MediaArg {
    PlayPause,
    Next,
    Previous,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    OpenApp { name: String },
    SwitchApp { name: String },
    Media { key: MediaArg },
    OpenUrl { url: String },
    WebSearch { query: String },
    YoutubePlay { query: String },
    DocsNew { topic: String, words: Option<u16> },
    CalendarNewEvent { title: String, date: String, time: String, minutes: Option<u16>, guests: Vec<String> },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Risk {
    Safe,
    Approve,
    Blocked,
}

/// `risk` comes only from `risk_of`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Step {
    pub action: Action,
    pub risk: Risk,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BrainOutput {
    Plan(Vec<Step>),
    Question(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reject {
    TooLarge,
    NotJson,
    WrongVersion,
    UnknownKind,
    UnknownField,
    UnknownAction,
    BadArgs,
    NoSteps,
    TooManySteps,
    MixedKinds,
    QuestionBad,
}

impl Reject {
    /// Fixed code 1..=11 in declaration order (logged as an integer).
    pub fn code(&self) -> u8 {
        todo!()
    }
}

pub fn parse(_raw: &str) -> Result<BrainOutput, Reject> {
    todo!()
}

pub fn risk_of(_a: &Action) -> Risk {
    todo!()
}

/// 0..=7 in table order.
pub fn action_code(_a: &Action) -> u8 {
    todo!()
}

pub fn render(_out: &BrainOutput) -> Vec<Line> {
    todo!()
}

pub fn schema_json() -> &'static str {
    todo!()
}

/// The flat subset of the schema that one provider accepts.
pub fn schema_for(_p: ProviderId) -> serde_json::Value {
    todo!()
}

// Stub phase: nothing in the app calls these yet (increments 1-6 wire them). This reference keeps them
// compiled without a dead-code warning and without an allow (strip-check s05).
const _: () = {
    let _ = PLAN_VERSION;
    let _ = MAX_STEPS;
    let _ = MAX_REPLY_BYTES;
    let _ = MediaArg::Next;
    std::mem::forget(Action::OpenApp { name: String::new() });
    let _ = Risk::Safe;
    std::mem::forget(Step { action: Action::OpenApp { name: String::new() }, risk: Risk::Safe });
    std::mem::forget(BrainOutput::Question(String::new()));
    let _ = Reject::TooLarge;
    let _ = Reject::code;
    let _ = parse;
    let _ = risk_of;
    let _ = action_code;
    let _ = render;
    let _ = schema_json;
    let _ = schema_for;
    let _ = (MediaArg::PlayPause, MediaArg::Previous);
    let _ = (Risk::Approve, Risk::Blocked);
    let plan = BrainOutput::Plan(Vec::new());
    std::mem::forget(plan);
    let actions = [
        Action::SwitchApp { name: String::new() },
        Action::Media { key: MediaArg::Next },
        Action::OpenUrl { url: String::new() },
        Action::WebSearch { query: String::new() },
        Action::YoutubePlay { query: String::new() },
        Action::DocsNew { topic: String::new(), words: None },
        Action::CalendarNewEvent { title: String::new(), date: String::new(), time: String::new(), minutes: None, guests: Vec::new() },
    ];
    std::mem::forget(actions);
    let _ = (
        Reject::NotJson, Reject::WrongVersion, Reject::UnknownKind, Reject::UnknownField, Reject::UnknownAction,
        Reject::BadArgs, Reject::NoSteps, Reject::TooManySteps, Reject::MixedKinds, Reject::QuestionBad,
    );
};
