// SPDX-License-Identifier: FSL-1.1-MIT
//! Step 4 key tests K1-K6, K8-K10 and the provider table (PV1). K7 = brain_ipc_tests.rs, K11 = static
//! (step4_checks.rs), X1 = brain_keychain_real.rs. Fakes only: `MemoryKeyStore`, a local server.
//! Log tests run in a child process (see `rules_support`). Sentinels are real words (`brain_support`).
use super::brain_support::*;
use super::rules_support::{captured, in_child, install_capture, run_isolated};
use crate::so::brain::brain::{BrainChoice, ERROR_SECS};
use crate::so::brain::commands::{save_key_with, status_with, BrainStatus};
use crate::so::brain::gemini::GeminiProvider;
use crate::so::brain::keys::{service, ApiKey, KeyError, KeyFormatError, KeyStore, MemoryKeyStore, SERVICE_DEBUG, SERVICE_RELEASE};
use crate::so::brain::provider::{Provider, ProviderError, ProviderId, PROVIDERS};
use crate::so::panel::Kind;
use std::fmt::Debug;
use std::sync::Arc;

// ---------- a Debug-or-not probe (so a missing `Debug` impl is a failed assertion, not a compile error) ----------
struct W<'a, T>(&'a T);
trait ViaDebug {
    fn show(&self) -> String;
}
impl<T: Debug> ViaDebug for W<'_, T> {
    fn show(&self) -> String {
        format!("{:?}", self.0)
    }
}
trait ViaNone {
    fn show(&self) -> String;
}
impl<T> ViaNone for &W<'_, T> {
    fn show(&self) -> String {
        "<no Debug impl>".to_string()
    }
}
macro_rules! dbg_of {
    ($e:expr) => {{
        let w = W(&$e);
        (&w).show()
    }};
}

// ---------- K1 ----------
fn fmt_err(s: &str) -> Option<KeyFormatError> {
    ApiKey::parse(s).err()
}

#[test]
fn k1_parse_rejects_empty_short_long_spaces_and_non_ascii() {
    assert_eq!(fmt_err(""), Some(KeyFormatError::Empty));
    assert_eq!(fmt_err("   \n\t "), Some(KeyFormatError::Empty), "only spaces is empty");
    assert_eq!(fmt_err(&"a".repeat(19)), Some(KeyFormatError::TooShort));
    assert_eq!(fmt_err(&format!("  {}  ", "a".repeat(19))), Some(KeyFormatError::TooShort), "length counts after trimming");
    assert_eq!(fmt_err(&"a".repeat(257)), Some(KeyFormatError::TooLong));
    assert_eq!(fmt_err(&format!("{} {}", "a".repeat(15), "b".repeat(15))), Some(KeyFormatError::BadChars), "inner space");
    assert_eq!(fmt_err(&format!("{}\t{}", "a".repeat(15), "b".repeat(15))), Some(KeyFormatError::BadChars), "inner tab");
    assert_eq!(fmt_err(&format!("{}\n{}", "a".repeat(15), "b".repeat(15))), Some(KeyFormatError::BadChars), "inner newline");
    assert_eq!(fmt_err(&"\u{e9}".repeat(25)), Some(KeyFormatError::BadChars), "non-ASCII");
    assert_eq!(fmt_err(&format!("{}\u{7f}", "a".repeat(25))), Some(KeyFormatError::BadChars), "DEL");
    assert_eq!(fmt_err(&format!("{}\u{0}", "a".repeat(25))), Some(KeyFormatError::BadChars), "NUL");
}

#[test]
fn k1_parse_accepts_20_to_256_visible_chars_and_trims_the_outside() {
    for n in [20, 21, 100, 255, 256] {
        assert!(ApiKey::parse(&"a".repeat(n)).is_ok(), "{n}");
    }
    assert!(ApiKey::parse("AIzaSy-_.~+/=0123456789abcdefghij").is_ok(), "key punctuation is fine");
    assert!(ApiKey::parse("sk-ant-api03-AbCd_Ef-0123456789").is_ok());
    let k = key(&format!("  \t{KEY_SENTINEL}\r\n "));
    assert_eq!(k.expose(), KEY_SENTINEL, "outer whitespace (a pasted newline) is trimmed");
}

// ---------- K2 ----------
struct Leaky(String);
impl Debug for Leaky {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Leaky({})", self.0)
    }
}

#[test]
fn k2_debug_output_never_holds_the_key_and_the_scanner_can_fail() {
    // Positive controls: a plain string and a derived-Debug wrapper DO leak, so the check below means something.
    assert!(format!("{:?}", KEY_SENTINEL).to_lowercase().contains(KEY_WORD));
    assert!(format!("{:?}", Leaky(KEY_SENTINEL.into())).to_lowercase().contains(KEY_WORD));

    let k = key(KEY_SENTINEL);
    assert_eq!(dbg_of!(k), "ApiKey(<redacted>)");
    let parsed = ApiKey::parse(KEY_SENTINEL);
    assert_eq!(dbg_of!(parsed), "Ok(ApiKey(<redacted>))");
    let store = MemoryKeyStore::new();
    store.set(ProviderId::Gemini, &k).expect("set");
    let got = store.get(ProviderId::Gemini);
    let shown = dbg_of!(got);
    assert!(!shown.to_lowercase().contains(KEY_WORD), "{shown}");
    assert!(shown.contains("redacted"), "{shown}");
    for e in [KeyError::Locked, KeyError::Denied, KeyError::Backend] {
        assert!(!dbg_of!(e).to_lowercase().contains(KEY_WORD));
    }
    for e in [
        ProviderError::KeyInvalid,
        ProviderError::Forbidden,
        ProviderError::NotFound,
        ProviderError::BadRequest(400),
        ProviderError::SpendLimit,
        ProviderError::RateLimited { retry_after_s: Some(1) },
        ProviderError::Server(500),
        ProviderError::Timeout,
        ProviderError::Offline,
        ProviderError::TooLarge,
        ProviderError::BadReply,
    ] {
        let s = dbg_of!(e);
        assert!(!s.to_lowercase().contains(KEY_WORD), "{s}");
        assert!(s.len() < 60, "error Debug stays a short code, never a message: {s}");
    }
    store.set(ProviderId::Claude, &key(CLAUDE_KEY)).expect("set");
    let st: BrainStatus = status_with(&store, &BrainChoice { provider: ProviderId::Gemini, model: "gemini-3.5-flash-lite".into() });
    let s = dbg_of!(st);
    assert!(!s.to_lowercase().contains(KEY_WORD) && !s.contains(CLAUDE_KEY), "{s}");
    assert!(s.contains("key_saved: true"), "positive control: status Debug shows the saved flag: {s}");
}

// ---------- K3 ----------
#[test]
fn k3_memory_store_set_get_delete() {
    let s = MemoryKeyStore::new();
    assert!(matches!(s.get(ProviderId::Gemini), Ok(None)), "empty store");
    s.set(ProviderId::Gemini, &key(GEMINI_KEY)).expect("set");
    assert_eq!(stored(&s, ProviderId::Gemini).as_deref(), Some(GEMINI_KEY));
    assert_eq!(stored(&s, ProviderId::Claude), None, "providers are separate");
    s.set(ProviderId::Claude, &key(CLAUDE_KEY)).expect("set");
    assert_eq!(stored(&s, ProviderId::Claude).as_deref(), Some(CLAUDE_KEY));
    assert_eq!(stored(&s, ProviderId::Gemini).as_deref(), Some(GEMINI_KEY));
    s.set(ProviderId::Gemini, &key(KEY_SENTINEL)).expect("overwrite");
    assert_eq!(stored(&s, ProviderId::Gemini).as_deref(), Some(KEY_SENTINEL), "set replaces");
    assert!(s.delete(ProviderId::Gemini).is_ok());
    assert_eq!(stored(&s, ProviderId::Gemini), None);
    assert_eq!(stored(&s, ProviderId::Claude).as_deref(), Some(CLAUDE_KEY), "delete touches one provider only");
    assert!(s.delete(ProviderId::Gemini).is_ok(), "deleting a missing key is Ok");
    assert!(matches!(s.get(ProviderId::Gemini), Ok(None)));
}

// ---------- K4 ----------
const WRONG: &str = "Key looks wrong: paste the whole key";
const COULDNT: &str = "Couldn't save the key";

#[test]
fn k4_save_key_stores_the_trimmed_key() {
    let s = MemoryKeyStore::new();
    assert_eq!(save_key_with(&s, ProviderId::Gemini, format!("  {KEY_SENTINEL}\n")), Ok(()));
    assert_eq!(stored(&s, ProviderId::Gemini).as_deref(), Some(KEY_SENTINEL));
    assert_eq!(stored(&s, ProviderId::Claude), None);
    assert_eq!(save_key_with(&s, ProviderId::Gemini, CLAUDE_KEY.to_string()), Ok(()));
    assert_eq!(stored(&s, ProviderId::Gemini).as_deref(), Some(CLAUDE_KEY), "a second save replaces");
}

#[test]
fn k4_a_bad_format_gives_the_fixed_text_and_stores_nothing() {
    let s = MemoryKeyStore::new();
    for bad in ["", "short", &format!("{KEY_SENTINEL} with a space"), &"\u{e9}".repeat(30), &"a".repeat(300)] {
        let e = save_key_with(&s, ProviderId::Gemini, bad.to_string()).expect_err("must refuse");
        assert_eq!(e, WRONG, "{bad:?}");
        assert!(!e.to_lowercase().contains(KEY_WORD), "the error echoes the input");
    }
    assert_eq!(stored(&s, ProviderId::Gemini), None);
}

#[test]
fn k4_a_failing_store_gives_the_fixed_text_only() {
    for e in [KeyError::Locked, KeyError::Denied, KeyError::Backend] {
        let msg = save_key_with(&FailStore(e), ProviderId::Gemini, KEY_SENTINEL.to_string()).expect_err("must fail");
        assert_eq!(msg, COULDNT, "{e:?}");
        assert!(!msg.to_lowercase().contains(KEY_WORD));
    }
}

// ---------- K5 ----------
fn choice(p: ProviderId, m: &str) -> BrainChoice {
    BrainChoice { provider: p, model: m.to_string() }
}

#[test]
fn k5_status_reports_key_saved_per_provider_and_never_the_key() {
    let s = MemoryKeyStore::new();
    s.set(ProviderId::Claude, &key(KEY_SENTINEL)).expect("set");
    let st = status_with(&s, &choice(ProviderId::Gemini, "gemini-3.5-flash-lite"));
    assert_eq!(st.provider, ProviderId::Gemini);
    assert_eq!(st.providers.len(), 2);
    let claude = st.providers.iter().find(|p| p.id == ProviderId::Claude).expect("claude row");
    let gemini = st.providers.iter().find(|p| p.id == ProviderId::Gemini).expect("gemini row");
    assert!(claude.key_saved && !gemini.key_saved);
    assert_eq!((claude.label.as_str(), claude.recommended), ("Claude", true));
    assert_eq!((gemini.label.as_str(), gemini.recommended), ("Gemini", false));
    assert_eq!(gemini.model, "gemini-3.5-flash-lite", "the chosen provider shows the chosen model");
    assert!(!claude.model.is_empty() && claude.model.starts_with("claude-"), "{}", claude.model);
    let json = serde_json::to_string(&st).expect("status serialises");
    assert!(!json.to_lowercase().contains(KEY_WORD), "the key is in the status JSON: {json}");
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(v["provider"], "gemini");
    let mut keys: Vec<String> = v["providers"][0].as_object().unwrap().keys().cloned().collect();
    keys.sort();
    assert_eq!(keys, ["id", "key_saved", "label", "model", "recommended"], "no field that could carry a key");
}

#[test]
fn k5_after_save_and_remove_the_flag_follows() {
    let s = MemoryKeyStore::new();
    let c = choice(ProviderId::Claude, "claude-haiku-5-5");
    assert!(status_with(&s, &c).providers.iter().all(|p| !p.key_saved));
    save_key_with(&s, ProviderId::Gemini, KEY_SENTINEL.to_string()).expect("save");
    assert!(status_with(&s, &c).providers.iter().any(|p| p.id == ProviderId::Gemini && p.key_saved));
    s.delete(ProviderId::Gemini).expect("delete");
    assert!(status_with(&s, &c).providers.iter().all(|p| !p.key_saved));
}

#[test]
fn k5_a_locked_keychain_reads_as_no_key_not_a_crash() {
    let st = status_with(&FailStore(KeyError::Locked), &choice(ProviderId::Gemini, "gemini-3.5-flash-lite"));
    assert!(st.providers.iter().all(|p| !p.key_saved));
}

// ---------- PV1: the provider table ----------
#[test]
fn pv1_providers_table_matches_the_plan() {
    assert_eq!(PROVIDERS.len(), 2);
    let c = PROVIDERS.iter().find(|p| p.id == ProviderId::Claude).unwrap();
    let g = PROVIDERS.iter().find(|p| p.id == ProviderId::Gemini).unwrap();
    assert_eq!((c.label, c.recommended, c.default_model), ("Claude", true, "claude-haiku-5-5"));
    assert_eq!(c.models, ["claude-haiku-5-5", "claude-sonnet-5-5"]);
    assert_eq!((g.label, g.recommended, g.default_model), ("Gemini", false, "gemini-3.5-flash-lite"));
    assert_eq!(g.models, ["gemini-3.5-flash-lite"]);
    assert_eq!(c.key_url, "https://platform.claude.com/settings/keys");
    assert_eq!(c.spend_url, "https://platform.claude.com/settings/billing");
    assert_eq!(g.key_url, "https://aistudio.google.com/apikey");
    assert_eq!(g.spend_url, "https://aistudio.google.com/spend");
    for p in PROVIDERS {
        assert!(p.models.contains(&p.default_model), "{}", p.label);
    }
}

#[test]
fn pv1_provider_ids_serialise_snake_case() {
    assert_eq!(serde_json::to_string(&ProviderId::Claude).unwrap(), "\"claude\"");
    assert_eq!(serde_json::to_string(&ProviderId::Gemini).unwrap(), "\"gemini\"");
    assert_eq!(serde_json::from_str::<ProviderId>("\"gemini\"").unwrap(), ProviderId::Gemini);
    assert!(serde_json::from_str::<ProviderId>("\"Gemini\"").is_err());
    assert!(serde_json::from_str::<ProviderId>("\"openai\"").is_err(), "no third provider yet");
}

// ---------- K10 ----------
#[test]
fn k10_service_names_keep_dev_and_shipped_keys_apart() {
    assert_eq!(SERVICE_RELEASE, "com.sritamnanda.smoothoperator.brain");
    assert_eq!(SERVICE_DEBUG, "com.sritamnanda.smoothoperator.brain.dev");
    assert_ne!(SERVICE_RELEASE, SERVICE_DEBUG);
    #[cfg(debug_assertions)]
    assert_eq!(service(), SERVICE_DEBUG, "debug builds use the .dev service");
    #[cfg(not(debug_assertions))]
    assert_eq!(service(), SERVICE_RELEASE);
}

// ---------- K8: no key, transcript, reply or error body in any log record, even at Trace ----------
#[test]
fn k8_nothing_secret_reaches_the_log_at_trace() {
    run_isolated(module_path!(), "k8_inner");
}
#[test]
#[ignore]
fn k8_inner() {
    if !in_child() {
        return;
    }
    install_capture();
    // Positive control: the capture works and the scanner finds a deliberate leak.
    log::info!(target: "positive_control", "oops {KEY_SENTINEL}");
    let baseline = captured().len();
    assert_eq!(leaks(&captured()).len(), 1, "the scanner cannot fail");

    let keys = Arc::new(MemoryKeyStore::new());
    assert_eq!(save_key_with(&*keys, ProviderId::Gemini, KEY_SENTINEL.to_string()), Ok(()));
    assert_eq!(save_key_with(&*keys, ProviderId::Claude, KEY_SENTINEL.to_string()), Ok(()));

    // check_key against local servers: ok, invalid-key (body carries a sentinel word)
    let ok = TestServer::json(200, r#"{"models":[]}"#);
    assert_eq!(GeminiProvider::with_base(&ok.base()).check_key(&key(KEY_SENTINEL)), Ok(()));
    let bad = TestServer::json(400, &format!(r#"{{"error":{{"code":400,"message":"API key not valid {BODY_WORD}","details":[{{"reason":"API_KEY_INVALID"}}]}}}}"#));
    assert_eq!(GeminiProvider::with_base(&bad.base()).check_key(&key(KEY_SENTINEL)), Err(ProviderError::KeyInvalid));

    // brain calls through the real providers on local servers
    use crate::so::brain::brain::{Brain, Limits};
    use crate::so::brain::claude::ClaudeProvider;
    let question = format!(r#"{{"v":1,"kind":"question","question":"Which {REPLY_WORD} do you mean?"}}"#);
    let gem_srv = TestServer::json(200, &gem_ok_body(&question));
    let claude_401 = TestServer::json(401, &format!(r#"{{"type":"error","error":{{"type":"authentication_error","message":"bad key {BODY_WORD}"}}}}"#));
    let gem_500 = TestServer::json(500, &format!(r#"{{"error":{{"message":"boom {BODY_WORD}"}}}}"#));
    let clock = super::rules_support::FakeClock::new();
    let which = Arc::new(std::sync::Mutex::new((ProviderId::Gemini, "gemini-3.5-flash-lite".to_string())));
    let mk = |gem_base: &str, claude_base: &str| {
        let w = which.clone();
        Brain::new(
            vec![Arc::new(ClaudeProvider::with_base(claude_base)) as Arc<dyn Provider>, Arc::new(GeminiProvider::with_base(gem_base))],
            keys.clone(),
            Arc::new(move || {
                let c = w.lock().unwrap();
                BrainChoice { provider: c.0, model: c.1.clone() }
            }),
            clock.clone(),
            Arc::new(|_| {}),
            Limits { max_in_flight: 2, max_per_hour: 120 },
        )
    };
    let said = format!("play {SAID_WORD} music");
    use crate::so::rules::SlowRouter;
    let b1 = mk(&gem_srv.base(), &claude_401.base());
    let r1 = b1.think(&said, clock.at(0));
    assert!(r1.lines.iter().any(|l| l.text.starts_with("AI asks:")), "positive control: the reply was used: {:?}", r1.lines);
    *which.lock().unwrap() = (ProviderId::Claude, "claude-haiku-5-5".to_string());
    let r2 = b1.think(&said, clock.at(0));
    assert!(r2.lines.iter().any(|l| l.kind == Kind::Warn), "the 401 gave a warning line");
    let b2 = mk(&gem_500.base(), &claude_401.base());
    *which.lock().unwrap() = (ProviderId::Gemini, "gemini-3.5-flash-lite".to_string());
    let r3 = b2.think(&said, clock.at(0));
    assert_eq!(r3.hold_secs, ERROR_SECS);

    thread_settle();
    let after: Vec<String> = captured().into_iter().skip(baseline).collect();
    assert!(!brain_records(&after).is_empty(), "no brain metrics record: the log capture is broken: {after:?}");
    assert_eq!(leaks(&after), Vec::<String>::new(), "secret words reached the log");
    for l in &after {
        assert!(!l.contains(KEY_SENTINEL), "the key reached the log: {l}");
    }
}

fn gem_ok_body(text: &str) -> String {
    serde_json::json!({"candidates": [{"content": {"parts": [{"text": text}]}, "finishReason": "STOP"}]}).to_string()
}
fn thread_settle() {
    std::thread::sleep(std::time::Duration::from_millis(300)); // library log lines from other threads
}

// ---------- K9: a panic inside a brain call is caught; nothing leaks ----------
#[test]
fn k9_a_panicking_provider_is_caught_and_gives_the_outcome_14_line() {
    let rig = BR::new();
    *rig.gemini.panic_with.lock().unwrap() = Some(format!("boom {KEY_SENTINEL} {SAID_WORD}"));
    let r = rig.think("play walrus music");
    assert_eq!(texts(&r), vec!["AI request failed: model or settings problem"]);
    assert_eq!(r.lines[0].kind, Kind::Warn);
    assert_eq!(r.hold_secs, ERROR_SECS);
    let shown = texts(&r).join("|").to_lowercase();
    assert!(!shown.contains(KEY_WORD) && !shown.contains(SAID_WORD), "{shown}");
    // The brain still works afterwards (the in-flight counter was released).
    *rig.gemini.panic_with.lock().unwrap() = None;
    let again = rig.think("play blinding lights");
    assert!(texts(&again).iter().any(|t| t.starts_with("Plan, not run")), "{:?}", texts(&again));
}

#[test]
fn k9_the_panic_text_never_reaches_the_log_and_outcome_14_is_logged() {
    run_isolated(module_path!(), "k9_inner");
}
#[test]
#[ignore]
fn k9_inner() {
    if !in_child() {
        return;
    }
    std::panic::set_hook(Box::new(|_| {})); // the default hook would print the (deliberately secret) message
    install_capture();
    let baseline = captured().len();
    let rig = BR::new();
    *rig.gemini.panic_with.lock().unwrap() = Some(format!("boom {KEY_SENTINEL} {SAID_WORD} {REPLY_WORD}"));
    let r = rig.think("play walrus music");
    assert_eq!(texts(&r), vec!["AI request failed: model or settings problem"]);
    let after: Vec<String> = captured().into_iter().skip(baseline).collect();
    assert_eq!(outcomes(&after), vec![14], "{after:?}");
    assert_eq!(leaks(&after), Vec::<String>::new());
}
