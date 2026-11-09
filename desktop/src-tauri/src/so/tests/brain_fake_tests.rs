// SPDX-License-Identifier: FSL-1.1-MIT
//! Step 4 debug fake provider tests FK1-FK5 (debug builds only, like the module). The plan lists the fake's
//! phrases but no unit tests; the fake feeds Claude's manual runs M1-M7, so its replies are pinned here.
//! Every reply goes through the REAL parser. Fakes only: no network, no key.
use super::brain_support::*;
use crate::so::brain::fake::{FakeProvider, ENV_FLAG};
use crate::so::brain::plan::{parse, BrainOutput, Reject};
use crate::so::brain::prompt::request;
use crate::so::brain::provider::{Provider, ProviderError, ProviderId};

fn ask(phrase: &str) -> Result<String, ProviderError> {
    FakeProvider::new().ask(&request(phrase), "fake-model", &key(KEY_SENTINEL))
}

#[test]
fn fk1_the_flag_name_is_so_brain_fake() {
    assert_eq!(ENV_FLAG, "SO_BRAIN_FAKE");
    let id = FakeProvider::new().id();
    assert!(id == ProviderId::Claude || id == ProviderId::Gemini);
}

#[test]
fn fk2_the_default_reply_is_a_valid_two_step_plan_through_the_real_parser() {
    for phrase in ["play blinding lights on youtube", "open youtube", "anything else here"] {
        let raw = ask(phrase).expect("a reply");
        match parse(&raw) {
            Ok(BrainOutput::Plan(steps)) => assert_eq!(steps.len(), 2, "{phrase}"),
            other => panic!("{phrase}: {other:?}"),
        }
    }
}

#[test]
fn fk3_phrases_drive_a_question_garbage_and_a_risk_field() {
    let q = ask("fake question about hello").expect("a reply");
    assert!(matches!(parse(&q), Ok(BrainOutput::Question(_))), "{q}");
    let g = ask("fake garbage x").expect("a reply");
    assert_eq!(parse(&g), Err(Reject::NotJson), "{g}");
    let r = ask("fake risk x").expect("a reply");
    assert!(r.contains("\"risk\""), "the risk reply must carry a risk field: {r}");
    assert!(parse(&r).is_err(), "and the real parser must reject it: {r}");
}

#[test]
fn fk4_phrases_drive_each_error() {
    assert_eq!(ask("fake 401 x"), Err(ProviderError::KeyInvalid));
    assert_eq!(ask("fake 429 x"), Err(ProviderError::RateLimited { retry_after_s: None }));
    assert_eq!(ask("fake 500 x"), Err(ProviderError::Server(500)));
    assert_eq!(ask("fake offline x"), Err(ProviderError::Offline));
    assert_eq!(ask("fake spend x"), Err(ProviderError::SpendLimit));
}

#[test]
fn fk5_the_fake_reads_the_phrase_from_the_wrapped_transcript_only() {
    // A phrase that merely contains the word "fake" later on, or in other case, is a normal command.
    let raw = ask("make a fake plan please").expect("a reply");
    assert!(matches!(parse(&raw), Ok(BrainOutput::Plan(_))), "{raw}");
    assert!(FakeProvider::new().check_key(&key(KEY_SENTINEL)).is_ok());
}
