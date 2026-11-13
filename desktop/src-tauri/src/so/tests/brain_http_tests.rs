// SPDX-License-Identifier: FSL-1.1-MIT
//! Step 4 HTTP + provider tests H1-H13. A local 127.0.0.1 server records every request; time is injected
//! where the code allows it (retry). The only real waits: H6 (the 20 s total timeout) and a few sub-second
//! server pauses. No real AI host is ever contacted. Numbers follow draft/step-4-plan.md.
use super::brain_support::*;
use crate::so::brain::claude::{ClaudeProvider, CLAUDE_MAX_TOKENS};
use crate::so::brain::gemini::{GeminiProvider, GEMINI_MAX_OUTPUT_TOKENS};
use crate::so::brain::http::{map_status, read_capped, with_retry, RetryPolicy, CONNECT_TIMEOUT, MAX_BODY, RETRY, TOTAL_TIMEOUT};
use crate::so::brain::plan::schema_for;
use crate::so::brain::provider::{BrainRequest, Provider, ProviderError, ProviderId, CLAUDE_API_VERSION};
use serde_json::{json, Value};
use std::cell::{Cell, RefCell};
use std::io::Write;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

fn block<F: std::future::Future>(f: F) -> F::Output {
    tauri::async_runtime::block_on(f)
}
fn request() -> BrainRequest {
    BrainRequest { system: "SYS-PROMPT", user: "<command>open youtube</command>".to_string() }
}
fn gem_ok(text: &str) -> String {
    json!({"candidates": [{"content": {"role": "model", "parts": [{"text": text}]}, "finishReason": "STOP"}]}).to_string()
}
fn claude_ok(text: &str) -> String {
    json!({"id": "msg_1", "type": "message", "role": "assistant", "content": [{"type": "text", "text": text}], "stop_reason": "end_turn"}).to_string()
}
const GEM_MODEL: &str = "gemini-3.5-flash-lite";
fn gem_ask(base: &str) -> Result<String, ProviderError> {
    GeminiProvider::with_base(base).ask(&request(), GEM_MODEL, &key(KEY_SENTINEL))
}
fn claude_ask(base: &str, model: &str) -> Result<String, ProviderError> {
    ClaudeProvider::with_base(base).ask(&request(), model, &key(KEY_SENTINEL))
}

// Bodies as Google and Anthropic document them (the exact fields are [UNVERIFIED]: both shapes are covered).
const GEM_INVALID_400: &str = r#"{"error":{"code":400,"message":"API key not valid. Please pass a valid API key.","status":"INVALID_ARGUMENT","details":[{"@type":"type.googleapis.com/google.rpc.ErrorInfo","reason":"API_KEY_INVALID","domain":"googleapis.com"}]}}"#;
const GEM_INVALID_REASON_ONLY: &str = r#"{"error":{"code":400,"status":"INVALID_ARGUMENT","details":[{"reason":"API_KEY_INVALID"}]}}"#;
const GEM_INVALID_401: &str = r#"{"error":{"code":401,"message":"API key not valid. Please pass a valid API key.","status":"UNAUTHENTICATED","details":[{"reason":"API_KEY_INVALID"}]}}"#;
const GEM_INVALID_403: &str = r#"{"error":{"code":403,"message":"API key not valid. Please pass a valid API key.","status":"PERMISSION_DENIED"}}"#;
const GEM_BAD_400: &str = r#"{"error":{"code":400,"message":"Invalid JSON payload received. pomegranate","status":"INVALID_ARGUMENT"}}"#;
const GEM_OTHER_403: &str = r#"{"error":{"code":403,"message":"Permission denied on resource project pomegranate.","status":"PERMISSION_DENIED"}}"#;
const CLAUDE_SPEND_400: &str = r#"{"type":"error","error":{"type":"invalid_request_error","message":"Your organization has reached its monthly spend limit. Raise the spend limit in the Console. pomegranate"},"request_id":"req_1"}"#;
const CLAUDE_BAD_400: &str = r#"{"type":"error","error":{"type":"invalid_request_error","message":"max_tokens: 5000 > 4096, which is the maximum allowed number of output tokens pomegranate"},"request_id":"req_1"}"#;
const CLAUDE_SPEND_429: &str = r#"{"type":"error","error":{"type":"rate_limit_error","message":"Spend limit reached pomegranate","details":{"error_code":"enforced_spend_limit_reached"}},"request_id":"req_1"}"#;
const CLAUDE_RATE_429: &str = r#"{"type":"error","error":{"type":"rate_limit_error","message":"Number of request tokens has exceeded your per-minute rate limit pomegranate"},"request_id":"req_1"}"#;

// ---------- H4: status mapping ----------
fn ms(p: ProviderId, status: u16, ra: Option<&str>, body: &str) -> ProviderError {
    map_status(p, status, ra, body.as_bytes())
}
use ProviderId::{Claude, Gemini};

#[test]
fn h4_claude_status_mapping() {
    assert_eq!(ms(Claude, 401, None, "{}"), ProviderError::KeyInvalid);
    assert_eq!(ms(Claude, 401, None, ""), ProviderError::KeyInvalid);
    assert_eq!(ms(Claude, 403, None, "{}"), ProviderError::Forbidden);
    assert_eq!(ms(Claude, 404, None, "{}"), ProviderError::NotFound);
    assert_eq!(ms(Claude, 400, None, CLAUDE_SPEND_400), ProviderError::SpendLimit);
    assert_eq!(ms(Claude, 400, None, CLAUDE_BAD_400), ProviderError::BadRequest(400), "a 400 without the marker is a settings problem");
    assert_eq!(ms(Claude, 400, None, ""), ProviderError::BadRequest(400));
    assert_eq!(ms(Claude, 413, None, "{}"), ProviderError::BadRequest(413));
    assert_eq!(ms(Claude, 429, None, CLAUDE_SPEND_429), ProviderError::SpendLimit);
    assert_eq!(ms(Claude, 429, Some("2"), CLAUDE_RATE_429), ProviderError::RateLimited { retry_after_s: Some(2) });
    assert_eq!(ms(Claude, 429, None, CLAUDE_RATE_429), ProviderError::RateLimited { retry_after_s: None });
    for s in [500, 503, 504, 529] {
        assert_eq!(ms(Claude, s, None, "{}"), ProviderError::Server(s), "{s}");
    }
}

#[test]
fn h4_gemini_status_mapping() {
    for (status, body) in [(400, GEM_INVALID_400), (400, GEM_INVALID_REASON_ONLY), (401, GEM_INVALID_401), (403, GEM_INVALID_403)] {
        assert_eq!(ms(Gemini, status, None, body), ProviderError::KeyInvalid, "{status} {body}");
    }
    assert_eq!(ms(Gemini, 400, None, GEM_BAD_400), ProviderError::BadRequest(400));
    assert_eq!(ms(Gemini, 400, None, ""), ProviderError::BadRequest(400));
    assert_eq!(ms(Gemini, 403, None, GEM_OTHER_403), ProviderError::Forbidden);
    assert_eq!(ms(Gemini, 404, None, "{}"), ProviderError::NotFound);
    assert_eq!(ms(Gemini, 402, None, "{}"), ProviderError::SpendLimit);
    assert_eq!(ms(Gemini, 402, None, ""), ProviderError::SpendLimit);
    assert_eq!(ms(Gemini, 429, None, r#"{"error":{"code":429,"status":"RESOURCE_EXHAUSTED"}}"#), ProviderError::RateLimited { retry_after_s: None });
    assert_eq!(ms(Gemini, 429, Some("1"), "{}"), ProviderError::RateLimited { retry_after_s: Some(1) });
    for s in [500, 503, 504] {
        assert_eq!(ms(Gemini, s, None, "{}"), ProviderError::Server(s), "{s}");
    }
    assert_eq!(ms(Gemini, 418, None, "{}"), ProviderError::BadRequest(418), "other 4xx");
}

#[test]
fn h4_retry_after_is_whole_seconds_or_nothing() {
    let r = |h: &str| ms(Gemini, 429, Some(h), "{}");
    assert_eq!(r("1"), ProviderError::RateLimited { retry_after_s: Some(1) });
    assert_eq!(r("30"), ProviderError::RateLimited { retry_after_s: Some(30) });
    assert_eq!(r("0"), ProviderError::RateLimited { retry_after_s: Some(0) });
    for junk in ["abc", "", "-1", "1.5"] {
        assert_eq!(r(junk), ProviderError::RateLimited { retry_after_s: None }, "{junk:?}");
    }
}

// ---------- H5: retry ----------
fn run(results: Vec<Result<u32, ProviderError>>, policy: &RetryPolicy) -> (Result<u32, ProviderError>, usize, Vec<Duration>) {
    let sleeps: RefCell<Vec<Duration>> = RefCell::new(vec![]);
    let attempts = Cell::new(0usize);
    let queue = RefCell::new(results.into_iter());
    let sleep = |d: Duration| sleeps.borrow_mut().push(d);
    let out = with_retry(policy, &sleep, || {
        attempts.set(attempts.get() + 1);
        queue.borrow_mut().next().expect("more attempts than scripted results")
    });
    (out, attempts.get(), sleeps.into_inner())
}
fn rl(s: Option<u64>) -> ProviderError {
    ProviderError::RateLimited { retry_after_s: s }
}
const S1: Duration = Duration::from_secs(1);

#[test]
fn h5_constants_follow_the_plan() {
    assert_eq!(RETRY.max_attempts, 2);
    assert_eq!(RETRY.backoff, S1);
    assert_eq!(RETRY.max_retry_after, Duration::from_secs(5));
    assert_eq!(CONNECT_TIMEOUT, Duration::from_secs(5));
    assert_eq!(TOTAL_TIMEOUT, Duration::from_secs(20));
    assert_eq!(MAX_BODY, 65_536);
}

#[test]
fn h5_success_first_try_means_one_attempt_and_no_wait() {
    let (r, n, s) = run(vec![Ok(7)], &RETRY);
    assert_eq!((r, n, s), (Ok(7), 1, vec![]));
}

#[test]
fn h5_rate_limit_with_a_short_retry_after_retries_once_after_that_wait() {
    let (r, n, s) = run(vec![Err(rl(Some(1))), Ok(7)], &RETRY);
    assert_eq!((r, n, s), (Ok(7), 2, vec![S1]));
    let (r, n, s) = run(vec![Err(rl(Some(3))), Ok(7)], &RETRY);
    assert_eq!((r, n, s), (Ok(7), 2, vec![Duration::from_secs(3)]));
    let (r, n, s) = run(vec![Err(rl(Some(5))), Ok(7)], &RETRY);
    assert_eq!((r, n, s), (Ok(7), 2, vec![Duration::from_secs(5)]), "5 s is the largest wait honoured");
}

#[test]
fn h5_rate_limit_with_a_long_retry_after_is_not_retried() {
    let (r, n, s) = run(vec![Err(rl(Some(30)))], &RETRY);
    assert_eq!((r, n, s), (Err(rl(Some(30))), 1, vec![]));
    let (r, n, _) = run(vec![Err(rl(Some(6)))], &RETRY);
    assert_eq!((r, n), (Err(rl(Some(6))), 1));
}

#[test]
fn h5_rate_limit_without_retry_after_retries_once_after_one_second() {
    let (r, n, s) = run(vec![Err(rl(None)), Ok(7)], &RETRY);
    assert_eq!((r, n, s), (Ok(7), 2, vec![S1]));
    let (r, n, s) = run(vec![Err(rl(None)), Err(rl(None))], &RETRY);
    assert_eq!((r, n, s), (Err(rl(None)), 2, vec![S1]), "never more than 2 attempts");
}

#[test]
fn h5_server_errors_retry_once_after_one_second() {
    for e in [ProviderError::Server(500), ProviderError::Server(503), ProviderError::Server(529)] {
        let (r, n, s) = run(vec![Err(e.clone()), Ok(7)], &RETRY);
        assert_eq!((r, n, s), (Ok(7), 2, vec![S1]), "{e:?} then ok");
        let (r, n, s) = run(vec![Err(e.clone()), Err(e.clone())], &RETRY);
        assert_eq!((r, n, s), (Err(e.clone()), 2, vec![S1]), "{e:?} twice");
    }
}

#[test]
fn h5_the_last_error_is_the_one_returned() {
    let (r, n, _) = run(vec![Err(ProviderError::Server(500)), Err(ProviderError::KeyInvalid)], &RETRY);
    assert_eq!((r, n), (Err(ProviderError::KeyInvalid), 2));
}

#[test]
fn h5_everything_else_is_tried_exactly_once() {
    for e in [
        ProviderError::KeyInvalid,
        ProviderError::SpendLimit,
        ProviderError::Forbidden,
        ProviderError::NotFound,
        ProviderError::BadRequest(400),
        ProviderError::Offline,
        ProviderError::TooLarge,
        ProviderError::BadReply,
        ProviderError::Timeout,
    ] {
        let (r, n, s) = run(vec![Err(e.clone())], &RETRY);
        assert_eq!((r, n, s), (Err(e.clone()), 1, vec![]), "{e:?}");
    }
}

#[test]
fn h5_a_timeout_is_never_retried_and_costs_no_extra_wait() {
    // The 20 s total timeout already passed; a second try would double the worst case to ~41 s.
    let (r, n, s) = run(vec![Err(ProviderError::Timeout), Ok(7)], &RETRY);
    assert_eq!((r, n, s), (Err(ProviderError::Timeout), 1, vec![]));
    // Server error first, then a timeout: the retry happens, the timeout is final.
    let (r, n, s) = run(vec![Err(ProviderError::Server(500)), Err(ProviderError::Timeout)], &RETRY);
    assert_eq!((r, n, s), (Err(ProviderError::Timeout), 2, vec![S1]));
}

#[test]
fn h5_a_policy_of_one_attempt_never_retries() {
    let one = RetryPolicy { max_attempts: 1, backoff: S1, max_retry_after: Duration::from_secs(5) };
    let (r, n, s) = run(vec![Err(ProviderError::Server(500))], &one);
    assert_eq!((r, n, s), (Err(ProviderError::Server(500)), 1, vec![]));
}

// ---------- H1: Gemini request ----------
fn gem_server(text: &str) -> TestServer {
    TestServer::json(200, &gem_ok(text))
}

#[test]
fn h1_gemini_request_shape_and_key_placement() {
    let srv = gem_server(PLAN_OK);
    let out = gem_ask(&srv.base());
    assert_eq!(out, Ok(PLAN_OK.to_string()));
    let reqs = srv.requests();
    assert_eq!(reqs.len(), 1, "exactly one request, no retry inside the provider");
    let r = &reqs[0];
    assert_eq!(r.method, "POST");
    assert_eq!(r.path(), "/v1beta/models/gemini-3.5-flash-lite:generateContent");
    assert_eq!(r.target, r.path(), "no query string at all (the key never goes in the URL)");
    assert_eq!(r.header("x-goog-api-key"), Some(KEY_SENTINEL));
    assert!(r.header("content-type").is_some_and(|c| c.starts_with("application/json")), "{:?}", r.header("content-type"));
    assert!(r.header("authorization").is_none(), "Gemini uses x-goog-api-key only");
    assert!(r.header("x-api-key").is_none());
    let others = format!("{}\n{}", r.target, r.body_text());
    assert!(!others.contains(KEY_SENTINEL), "the key is in the URL or body");
    let b = r.json();
    assert_eq!(b["systemInstruction"]["parts"][0]["text"], "SYS-PROMPT");
    assert_eq!(b["contents"], json!([{"role": "user", "parts": [{"text": "<command>open youtube</command>"}]}]));
    let g = &b["generationConfig"];
    assert_eq!(g["responseMimeType"], "application/json");
    assert_eq!(g["maxOutputTokens"], 2048);
    assert_eq!(GEMINI_MAX_OUTPUT_TOKENS, 2048);
    assert_eq!(g["thinkingConfig"]["thinkingLevel"], "MINIMAL");
    assert_eq!(g["responseJsonSchema"], schema_for(Gemini));
}

#[test]
fn h1_the_model_name_goes_into_the_path() {
    let srv = gem_server("{}");
    let _ = GeminiProvider::with_base(&srv.base()).ask(&request(), "gemini-9.9-test", &key(KEY_SENTINEL));
    assert_eq!(srv.requests()[0].path(), "/v1beta/models/gemini-9.9-test:generateContent");
}

// ---------- H2: Claude request ----------
#[test]
fn h2_claude_request_shape_and_key_placement() {
    let srv = TestServer::json(200, &claude_ok(PLAN_OK));
    let out = claude_ask(&srv.base(), "claude-haiku-5-5");
    assert_eq!(out, Ok(PLAN_OK.to_string()));
    let reqs = srv.requests();
    assert_eq!(reqs.len(), 1);
    let r = &reqs[0];
    assert_eq!(r.method, "POST");
    assert_eq!(r.target, "/v1/messages");
    assert_eq!(r.header("authorization"), Some(format!("Bearer {KEY_SENTINEL}").as_str()));
    assert!(!r.header_names().contains(&"x-api-key".to_string()), "the legacy header is not used");
    assert_eq!(r.header("anthropic-version"), Some("2026-10-21"));
    assert_eq!(CLAUDE_API_VERSION, "2026-10-21");
    assert!(r.header("content-type").is_some_and(|c| c.starts_with("application/json")));
    let others = format!("{}\n{}", r.target, r.body_text());
    assert!(!others.contains(KEY_SENTINEL), "the key is in the URL or body");
    let b = r.json();
    assert_eq!(b["model"], "claude-haiku-5-5");
    assert_eq!(b["max_tokens"], 1024);
    assert_eq!(CLAUDE_MAX_TOKENS, 1024);
    assert_eq!(b["system"], "SYS-PROMPT");
    assert_eq!(b["messages"], json!([{"role": "user", "content": "<command>open youtube</command>"}]));
    assert_eq!(b["output_config"]["format"]["type"], "json_schema");
    assert_eq!(b["output_config"]["format"]["schema"], schema_for(Claude));
    assert_eq!(b["thinking"]["type"], "disabled", "Haiku 5.5");
}

#[test]
fn h2_sonnet_gets_between_tools_not_disabled() {
    let srv = TestServer::json(200, &claude_ok("{}"));
    let _ = claude_ask(&srv.base(), "claude-sonnet-5-5");
    let b = srv.requests()[0].json();
    assert_eq!(b["thinking"]["type"], "between_tools");
    assert_eq!(b["model"], "claude-sonnet-5-5");
}

// ---------- H3: exactly the documented keys ----------
fn sorted_keys(v: &Value) -> Vec<String> {
    let mut k: Vec<String> = v.as_object().expect("an object").keys().cloned().collect();
    k.sort();
    k
}
fn assert_no_forbidden(raw: &str) {
    for bad in ["temperature", "top_p", "top_k", "topP", "topK", "\"tools\"", "tool_choice", "toolConfig", "prefill"] {
        assert!(!raw.contains(bad), "the request body contains {bad}");
    }
}

#[test]
fn h3_claude_body_has_exactly_the_documented_keys() {
    for model in ["claude-haiku-5-5", "claude-sonnet-5-5"] {
        let srv = TestServer::json(200, &claude_ok("{}"));
        let _ = claude_ask(&srv.base(), model);
        let r = &srv.requests()[0];
        let b = r.json();
        assert_eq!(sorted_keys(&b), ["max_tokens", "messages", "model", "output_config", "system", "thinking"], "{model}");
        assert_eq!(sorted_keys(&b["output_config"]), ["format"]);
        assert_eq!(sorted_keys(&b["output_config"]["format"]), ["schema", "type"]);
        assert_eq!(sorted_keys(&b["thinking"]), ["type"]);
        let msgs = b["messages"].as_array().unwrap();
        assert_eq!(msgs.len(), 1, "one user message, no assistant prefill");
        assert_eq!(msgs[0]["role"], "user");
        assert_no_forbidden(&r.body_text());
        assert!(!r.header_names().contains(&"anthropic-beta".to_string()), "no beta header: structured output is GA");
    }
}

#[test]
fn h3_gemini_body_has_exactly_the_documented_keys() {
    let srv = gem_server("{}");
    let _ = gem_ask(&srv.base());
    let r = &srv.requests()[0];
    let b = r.json();
    assert_eq!(sorted_keys(&b), ["contents", "generationConfig", "systemInstruction"]);
    assert_eq!(sorted_keys(&b["generationConfig"]), ["maxOutputTokens", "responseJsonSchema", "responseMimeType", "thinkingConfig"]);
    assert_eq!(sorted_keys(&b["generationConfig"]["thinkingConfig"]), ["thinkingLevel"]);
    let contents = b["contents"].as_array().unwrap();
    assert_eq!(contents.len(), 1);
    assert_eq!(contents[0]["role"], "user", "no model/assistant turn (prefill)");
    assert_no_forbidden(&r.body_text());
}

// ---------- H9: reply extraction ----------
#[test]
fn h9_gemini_reply_text_comes_from_the_parts() {
    let one = TestServer::json(200, &gem_ok("hello"));
    assert_eq!(gem_ask(&one.base()), Ok("hello".to_string()));
    // Several parts are joined without a separator (a JSON text can be split anywhere).
    let body = json!({"candidates": [{"content": {"parts": [{"text": "{\"v\":1,"}, {"text": "\"kind\":\"question\"}"}]}, "finishReason": "STOP"}]});
    let two = TestServer::json(200, &body.to_string());
    assert_eq!(gem_ask(&two.base()), Ok("{\"v\":1,\"kind\":\"question\"}".to_string()));
}

#[test]
fn h9_gemini_thought_parts_are_skipped() {
    let body = json!({"candidates": [{"content": {"parts": [
        {"text": "I should answer with a question", "thought": true},
        {"text": "{\"v\":1,\"kind\":\"question\"}"},
        {"text": "more hidden reasoning", "thought": true}
    ]}, "finishReason": "STOP"}]});
    let srv = TestServer::json(200, &body.to_string());
    assert_eq!(gem_ask(&srv.base()), Ok("{\"v\":1,\"kind\":\"question\"}".to_string()));
    let only = json!({"candidates": [{"content": {"parts": [{"text": "thinking", "thought": true}]}, "finishReason": "STOP"}]});
    let srv = TestServer::json(200, &only.to_string());
    assert_eq!(gem_ask(&srv.base()), Err(ProviderError::BadReply), "only thought parts = no answer");
}

#[test]
fn h9_gemini_unusable_replies_are_bad_reply() {
    let cases = [
        json!({"candidates": []}),
        json!({}),
        json!({"promptFeedback": {"blockReason": "SAFETY"}}),
        json!({"candidates": [{"finishReason": "SAFETY"}]}),
        json!({"candidates": [{"content": {"parts": [{"text": "{\"v\":1"}]}, "finishReason": "MAX_TOKENS"}]}),
        json!({"candidates": [{"content": {"parts": [{"text": "x"}]}, "finishReason": "SAFETY"}]}),
        json!({"candidates": [{"content": {"parts": []}, "finishReason": "STOP"}]}),
    ];
    for c in cases {
        let srv = TestServer::json(200, &c.to_string());
        assert_eq!(gem_ask(&srv.base()), Err(ProviderError::BadReply), "{c}");
    }
    let not_json = TestServer::json(200, "<html>nope</html>");
    assert_eq!(gem_ask(&not_json.base()), Err(ProviderError::BadReply), "a 200 that is not JSON");
}

#[test]
fn h9_claude_reply_text_comes_from_the_text_block() {
    let srv = TestServer::json(200, &claude_ok("hello"));
    assert_eq!(claude_ask(&srv.base(), "claude-haiku-5-5"), Ok("hello".to_string()));
    let body = json!({"type": "message", "content": [{"type": "thinking", "thinking": "hmm"}, {"type": "text", "text": "answer"}], "stop_reason": "end_turn"});
    let with_thinking = TestServer::json(200, &body.to_string());
    assert_eq!(claude_ask(&with_thinking.base(), "claude-sonnet-5-5"), Ok("answer".to_string()));
}

#[test]
fn h9_claude_unusable_replies_are_bad_reply() {
    let cases = [
        json!({"type": "message", "content": [{"type": "text", "text": "x"}], "stop_reason": "refusal"}),
        json!({"type": "message", "content": [{"type": "text", "text": "{\"v\":1"}], "stop_reason": "max_tokens"}),
        json!({"type": "message", "content": [], "stop_reason": "end_turn"}),
        json!({"type": "message", "content": [{"type": "tool_use", "id": "t", "name": "n", "input": {}}], "stop_reason": "tool_use"}),
        json!({"type": "message"}),
    ];
    for c in cases {
        let srv = TestServer::json(200, &c.to_string());
        assert_eq!(claude_ask(&srv.base(), "claude-haiku-5-5"), Err(ProviderError::BadReply), "{c}");
    }
    let not_json = TestServer::json(200, "not json");
    assert_eq!(claude_ask(&not_json.base(), "claude-haiku-5-5"), Err(ProviderError::BadReply));
}

// ---------- status mapping end to end (one request each: the provider never retries by itself) ----------
#[test]
fn h4_end_to_end_errors_map_through_a_real_connection_with_a_single_request() {
    type Case = (u16, Vec<(String, String)>, &'static str, ProviderError);
    let cases: Vec<Case> = vec![
        (400, vec![], GEM_INVALID_400, ProviderError::KeyInvalid),
        (402, vec![], "{}", ProviderError::SpendLimit),
        (429, vec![("Retry-After".into(), "1".into())], "{}", ProviderError::RateLimited { retry_after_s: Some(1) }),
        (500, vec![], "{}", ProviderError::Server(500)),
        (404, vec![], "{}", ProviderError::NotFound),
    ];
    for (status, headers, body, want) in cases {
        let srv = TestServer::fixed(status, headers, body.to_string());
        assert_eq!(gem_ask(&srv.base()), Err(want.clone()), "gemini {status}");
        assert_eq!(srv.requests().len(), 1, "gemini {status}: retries belong to the brain, not the provider");
    }
    let claude_cases: Vec<(u16, &str, ProviderError)> = vec![
        (401, "{}", ProviderError::KeyInvalid),
        (400, CLAUDE_SPEND_400, ProviderError::SpendLimit),
        (429, CLAUDE_SPEND_429, ProviderError::SpendLimit),
        (529, "{}", ProviderError::Server(529)),
    ];
    for (status, body, want) in claude_cases {
        let srv = TestServer::json(status, body);
        assert_eq!(claude_ask(&srv.base(), "claude-haiku-5-5"), Err(want.clone()), "claude {status}");
        assert_eq!(srv.requests().len(), 1, "claude {status}");
    }
}

// ---------- H6: timeout and offline ----------
#[test]
fn h6_a_closed_port_is_offline_and_fast() {
    let t = Instant::now();
    assert_eq!(gem_ask(&closed_port_base()), Err(ProviderError::Offline));
    assert_eq!(claude_ask(&closed_port_base(), "claude-haiku-5-5"), Err(ProviderError::Offline));
    assert!(t.elapsed() < Duration::from_secs(4), "{:?}", t.elapsed());
}

fn silent_server() -> TestServer {
    TestServer::start(Arc::new(|_req, _s| thread::sleep(Duration::from_secs(26))))
}

#[test]
fn h6_a_server_that_never_answers_times_out_at_the_total_timeout_gemini() {
    let srv = silent_server();
    let t = Instant::now();
    assert_eq!(gem_ask(&srv.base()), Err(ProviderError::Timeout));
    let e = t.elapsed();
    assert!(e >= Duration::from_secs(19) && e <= Duration::from_secs(24), "total timeout is 20 s, took {e:?}");
}

#[test]
fn h6_a_server_that_never_answers_times_out_at_the_total_timeout_claude() {
    let srv = silent_server();
    let t = Instant::now();
    assert_eq!(claude_ask(&srv.base(), "claude-haiku-5-5"), Err(ProviderError::Timeout));
    let e = t.elapsed();
    assert!(e >= Duration::from_secs(19) && e <= Duration::from_secs(24), "total timeout is 20 s, took {e:?}");
}

// ---------- H7: redirects are never followed ----------
#[test]
fn h7_a_redirect_is_not_followed_and_the_second_server_gets_nothing() {
    for which in ["gemini", "claude"] {
        let second = TestServer::json(200, "{}");
        let target = format!("{}/stolen", second.base());
        let first = TestServer::fixed(302, vec![("Location".into(), target)], String::new());
        let out = if which == "gemini" { gem_ask(&first.base()) } else { claude_ask(&first.base(), "claude-haiku-5-5") };
        assert!(out.is_err(), "{which}: a redirect must not be a success: {out:?}");
        thread::sleep(Duration::from_millis(400));
        assert_eq!(first.requests().len(), 1, "{which}");
        assert!(second.requests().is_empty(), "{which}: the redirect was followed: {:?}", second.requests());
    }
}

// ---------- H8: body cap ----------
fn capped(url: &str, cap: usize) -> Result<Vec<u8>, ProviderError> {
    block(async {
        let resp = reqwest::Client::new().get(url).send().await.expect("local request");
        read_capped(resp, cap).await
    })
}

#[test]
fn h8_positive_controls_small_bodies_plain_and_gzip_are_read_whole() {
    let srv = TestServer::json(200, "{\"ok\":true}");
    assert_eq!(capped(&srv.base(), MAX_BODY), Ok(b"{\"ok\":true}".to_vec()));
    let (gz, total) = gzip_of_a(300);
    let srv2 = TestServer::start(Arc::new(move |_r, s| respond(s, 200, &[("Content-Encoding", "gzip")], &gz)));
    let body = capped(&srv2.base(), MAX_BODY).expect("a small gzip body is fine");
    assert_eq!(body.len(), total);
    assert!(body.iter().all(|b| *b == b'a'));
}

#[test]
fn h8_exactly_at_the_cap_is_fine_one_over_is_too_large() {
    let at = vec![b'a'; MAX_BODY];
    let srv = TestServer::start(Arc::new(move |_r, s| respond(s, 200, &[], &at)));
    assert_eq!(capped(&srv.base(), MAX_BODY).map(|b| b.len()), Ok(MAX_BODY));
    let over = vec![b'a'; MAX_BODY + 1];
    let srv = TestServer::start(Arc::new(move |_r, s| respond(s, 200, &[], &over)));
    assert_eq!(capped(&srv.base(), MAX_BODY), Err(ProviderError::TooLarge));
}

#[test]
fn h8a_a_content_length_over_the_cap_is_too_large_before_the_body_is_read() {
    let srv = TestServer::start(Arc::new(|_r, s| {
        let head = "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 70000\r\nConnection: close\r\n\r\n";
        let _ = s.write_all(head.as_bytes());
        let _ = s.flush();
        thread::sleep(Duration::from_secs(8)); // the body never comes
    }));
    let t = Instant::now();
    assert_eq!(capped(&srv.base(), MAX_BODY), Err(ProviderError::TooLarge));
    assert!(t.elapsed() < Duration::from_secs(3), "the body was waited for: {:?}", t.elapsed());
}

#[test]
fn h8b_a_chunked_body_over_the_cap_is_too_large_and_reading_stops_early() {
    let sent = Arc::new(AtomicUsize::new(0));
    let sent2 = sent.clone();
    let srv = TestServer::start(Arc::new(move |_r, s| {
        if !chunked_head(s, &[]) {
            return;
        }
        let block = vec![b'a'; 8192];
        for _ in 0..512 {
            // 4 MB in all
            if !chunk(s, &block) {
                return;
            }
            sent2.fetch_add(block.len(), Ordering::SeqCst);
            thread::sleep(Duration::from_millis(3));
        }
        let _ = s.write_all(b"0\r\n\r\n");
    }));
    assert_eq!(capped(&srv.base(), MAX_BODY), Err(ProviderError::TooLarge));
    thread::sleep(Duration::from_millis(600));
    let n = sent.load(Ordering::SeqCst);
    assert!(n < 2 * 1024 * 1024, "the client kept reading: the server wrote {n} bytes before the connection closed");
}

#[test]
fn h8b_a_chunked_gzip_body_that_decodes_over_the_cap_is_too_large() {
    // About 38 KB on the wire (under the cap), 6 MB once decoded: only counting decoded bytes catches it.
    let (gz, total) = gzip_of_a(6 * 1024 * 1024);
    assert!(gz.len() < MAX_BODY && total > 5 * 1024 * 1024, "test data: {} -> {}", gz.len(), total);
    let srv = TestServer::start(Arc::new(move |_r, s| {
        if chunked_head(s, &[("Content-Encoding", "gzip")]) {
            for part in gz.chunks(2048) {
                if !chunk(s, part) {
                    return;
                }
            }
            let _ = s.write_all(b"0\r\n\r\n");
        }
    }));
    let t = Instant::now();
    assert_eq!(capped(&srv.base(), MAX_BODY), Err(ProviderError::TooLarge));
    assert!(t.elapsed() < Duration::from_secs(5));
}

#[test]
fn h8b_a_gzip_body_with_a_small_content_length_that_decodes_over_the_cap_is_too_large() {
    let (gz, _) = gzip_of_a(6 * 1024 * 1024);
    let srv = TestServer::start(Arc::new(move |_r, s| respond(s, 200, &[("Content-Encoding", "gzip")], &gz)));
    assert_eq!(capped(&srv.base(), MAX_BODY), Err(ProviderError::TooLarge));
}

#[test]
fn h8_the_providers_apply_the_cap_to_real_replies_too() {
    let sent = Arc::new(AtomicUsize::new(0));
    let s2 = sent.clone();
    let srv = TestServer::start(Arc::new(move |_r, s| {
        if !chunked_head(s, &[]) {
            return;
        }
        let block = vec![b' '; 8192];
        for _ in 0..512 {
            if !chunk(s, &block) {
                return;
            }
            s2.fetch_add(block.len(), Ordering::SeqCst);
            thread::sleep(Duration::from_millis(3));
        }
    }));
    assert_eq!(gem_ask(&srv.base()), Err(ProviderError::TooLarge));
    let (gz, _) = gzip_of_a(6 * 1024 * 1024);
    let srv2 = TestServer::start(Arc::new(move |_r, s| respond(s, 200, &[("Content-Encoding", "gzip")], &gz)));
    assert_eq!(claude_ask(&srv2.base(), "claude-haiku-5-5"), Err(ProviderError::TooLarge));
    // An error body is capped too (and still maps to its status).
    let big = "x".repeat(MAX_BODY * 2);
    let srv3 = TestServer::start(Arc::new(move |_r, s| respond(s, 500, &[], big.as_bytes())));
    let e = gem_ask(&srv3.base());
    assert!(matches!(e, Err(ProviderError::TooLarge)) || e == Err(ProviderError::Server(500)), "{e:?}");
}

// ---------- H10 ----------
#[test]
fn h10_the_public_constructors_build_the_right_providers() {
    assert_eq!(GeminiProvider::new().id(), ProviderId::Gemini);
    assert_eq!(ClaudeProvider::new().id(), ProviderId::Claude);
    // Fixed https hosts and https_only are checked in tests/strip-checks/tests/step4_checks.rs (S2, S4).
}

// ---------- H11: check_key ----------
#[test]
fn h11_gemini_check_key_lists_models_with_the_key_header() {
    let ok = TestServer::json(200, r#"{"models":[]}"#);
    assert_eq!(GeminiProvider::with_base(&ok.base()).check_key(&key(KEY_SENTINEL)), Ok(()));
    let r = &ok.requests()[0];
    assert_eq!(r.method, "GET");
    assert_eq!(r.target, "/v1beta/models", "no query, no key in the URL");
    assert_eq!(r.header("x-goog-api-key"), Some(KEY_SENTINEL));
    assert!(r.body.is_empty(), "no body: nothing to generate");
    let bad = TestServer::json(400, GEM_INVALID_400);
    assert_eq!(GeminiProvider::with_base(&bad.base()).check_key(&key(KEY_SENTINEL)), Err(ProviderError::KeyInvalid));
    let forbidden = TestServer::json(403, GEM_INVALID_403);
    assert_eq!(GeminiProvider::with_base(&forbidden.base()).check_key(&key(KEY_SENTINEL)), Err(ProviderError::KeyInvalid));
}

#[test]
fn h11_claude_check_key_lists_models_with_the_bearer_header() {
    let ok = TestServer::json(200, r#"{"data":[],"has_more":false}"#);
    assert_eq!(ClaudeProvider::with_base(&ok.base()).check_key(&key(KEY_SENTINEL)), Ok(()));
    let r = &ok.requests()[0];
    assert_eq!(r.method, "GET");
    assert_eq!(r.target, "/v1/models");
    assert_eq!(r.header("authorization"), Some(format!("Bearer {KEY_SENTINEL}").as_str()));
    assert_eq!(r.header("anthropic-version"), Some("2026-10-21"));
    assert!(!r.header_names().contains(&"x-api-key".to_string()));
    assert!(r.body.is_empty());
    let bad = TestServer::json(401, "{}");
    assert_eq!(ClaudeProvider::with_base(&bad.base()).check_key(&key(KEY_SENTINEL)), Err(ProviderError::KeyInvalid));
}

// ---------- H12: threads ----------
#[test]
fn h12_a_provider_call_works_from_a_plain_thread_and_inside_spawn_blocking() {
    let srv = gem_server("from-thread");
    let base = srv.base();
    let b2 = base.clone();
    let out = thread::spawn(move || gem_ask(&b2)).join().expect("no panic on a plain std thread");
    assert_eq!(out, Ok("from-thread".to_string()));
    let b3 = base.clone();
    let out2 = block(async move { tauri::async_runtime::spawn_blocking(move || gem_ask(&b3)).await.expect("no panic in spawn_blocking") });
    assert_eq!(out2, Ok("from-thread".to_string()));
    let csrv = TestServer::json(200, &claude_ok("c"));
    let cb = csrv.base();
    let out3 = thread::spawn(move || claude_ask(&cb, "claude-haiku-5-5")).join().expect("claude on a std thread");
    assert_eq!(out3, Ok("c".to_string()));
    let cb2 = csrv.base();
    let out4 = block(async move { tauri::async_runtime::spawn_blocking(move || claude_ask(&cb2, "claude-haiku-5-5")).await.expect("claude in spawn_blocking") });
    assert_eq!(out4, Ok("c".to_string()));
}

// ---------- H13: the schema each provider receives is inside its documented subset ----------
fn violations(v: &Value, claude: bool, path: &str, out: &mut Vec<String>) {
    match v {
        Value::Object(m) => {
            let is_obj = m.get("type").is_some_and(|t| t == "object") || m.contains_key("properties");
            if claude && is_obj && m.get("additionalProperties") != Some(&Value::Bool(false)) {
                out.push(format!("{path}: object without additionalProperties:false"));
            }
            for (k, x) in m {
                let banned: &[&str] = if claude { &["oneOf", "maxItems", "minLength", "maxLength", "minimum", "maximum"] } else { &["oneOf"] };
                if banned.contains(&k.as_str()) {
                    out.push(format!("{path}/{k}: not accepted"));
                }
                if claude && k == "minItems" && !(x == 0 || x == 1) {
                    out.push(format!("{path}/minItems must be 0 or 1"));
                }
                violations(x, claude, &format!("{path}/{k}"), out);
            }
        }
        Value::Array(a) => a.iter().enumerate().for_each(|(i, x)| violations(x, claude, &format!("{path}[{i}]"), out)),
        _ => {}
    }
}

#[test]
fn h13_the_schemas_on_the_wire_pass_each_providers_subset_checker() {
    // Positive control: the checker flags a bad schema.
    let mut bad = vec![];
    violations(&json!({"type": "object", "properties": {"a": {"oneOf": []}}}), true, "", &mut bad);
    assert!(bad.len() >= 2, "the checker cannot fail: {bad:?}");

    let g = gem_server("{}");
    let _ = gem_ask(&g.base());
    let sent = g.requests()[0].json()["generationConfig"]["responseJsonSchema"].clone();
    assert!(sent.is_object());
    let mut v = vec![];
    violations(&sent, false, "", &mut v);
    assert!(v.is_empty(), "Gemini: {v:?}");

    let c = TestServer::json(200, &claude_ok("{}"));
    let _ = claude_ask(&c.base(), "claude-haiku-5-5");
    let sent = c.requests()[0].json()["output_config"]["format"]["schema"].clone();
    assert!(sent.is_object());
    let mut v = vec![];
    violations(&sent, true, "", &mut v);
    assert!(v.is_empty(), "Claude: {v:?}");
}
