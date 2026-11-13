// SPDX-License-Identifier: FSL-1.1-MIT
//! Step 4 plan tests J1-J14 (pure: parse, risk, render, schemas). Numbers follow draft/step-4-plan.md.
use crate::so::brain::plan::{
    action_code, parse, render, risk_of, schema_for, schema_json, Action, BrainOutput, MediaArg, Reject, Risk, MAX_REPLY_BYTES,
};
use crate::so::brain::provider::ProviderId;
use crate::so::panel::{Kind, PanelPayload};
use serde_json::{json, Value};

fn plan(steps: Vec<Value>) -> String {
    json!({"v": 1, "kind": "plan", "steps": steps}).to_string()
}
fn step(action: &str, args: Value) -> Value {
    json!({"action": action, "args": args})
}
fn one(action: &str, args: Value) -> String {
    plan(vec![step(action, args)])
}
fn ok_steps(raw: &str) -> Vec<(Action, Risk)> {
    match parse(raw) {
        Ok(BrainOutput::Plan(s)) => s.into_iter().map(|s| (s.action, s.risk)).collect(),
        other => panic!("expected a plan, got {other:?} for {raw}"),
    }
}
fn ok1(raw: &str) -> (Action, Risk) {
    let mut s = ok_steps(raw);
    assert_eq!(s.len(), 1, "{raw}");
    s.remove(0)
}
fn rej(raw: &str) -> Reject {
    match parse(raw) {
        Err(r) => r,
        Ok(o) => panic!("expected a reject, got {o:?} for {raw}"),
    }
}
fn cal(over: Value) -> String {
    let mut args = json!({"title": "Review", "date": "tomorrow", "time": "15:00"});
    for (k, v) in over.as_object().unwrap() {
        args[k] = v.clone();
    }
    one("calendar_new_event", args)
}
fn bad_args(raw: &str) {
    assert_eq!(rej(raw), Reject::BadArgs, "{raw}");
}

// ---------- J1 ----------
#[test]
fn j1_each_of_the_eight_actions_parses_with_the_right_action_and_risk() {
    let cases: Vec<(String, Action, Risk)> = vec![
        (one("open_app", json!({"name": "Calculator"})), Action::OpenApp { name: "Calculator".into() }, Risk::Safe),
        (one("switch_app", json!({"name": "Chrome"})), Action::SwitchApp { name: "Chrome".into() }, Risk::Safe),
        (one("media", json!({"key": "play_pause"})), Action::Media { key: MediaArg::PlayPause }, Risk::Safe),
        (one("media", json!({"key": "next"})), Action::Media { key: MediaArg::Next }, Risk::Safe),
        (one("media", json!({"key": "previous"})), Action::Media { key: MediaArg::Previous }, Risk::Safe),
        (one("open_url", json!({"url": "https://www.youtube.com/"})), Action::OpenUrl { url: "https://www.youtube.com/".into() }, Risk::Safe),
        (one("web_search", json!({"query": "weather in Oslo"})), Action::WebSearch { query: "weather in Oslo".into() }, Risk::Safe),
        (one("youtube_play", json!({"query": "Blinding Lights"})), Action::YoutubePlay { query: "Blinding Lights".into() }, Risk::Safe),
        (
            one("docs_new", json!({"topic": "Causes of WWII", "words": 300})),
            Action::DocsNew { topic: "Causes of WWII".into(), words: Some(300) },
            Risk::Approve,
        ),
        (one("docs_new", json!({"topic": "Causes of WWII"})), Action::DocsNew { topic: "Causes of WWII".into(), words: None }, Risk::Approve),
        (
            cal(json!({})),
            Action::CalendarNewEvent { title: "Review".into(), date: "tomorrow".into(), time: "15:00".into(), minutes: None, guests: vec![] },
            Risk::Safe,
        ),
        (
            cal(json!({"minutes": 30, "guests": ["ann@example.com"]})),
            Action::CalendarNewEvent {
                title: "Review".into(),
                date: "tomorrow".into(),
                time: "15:00".into(),
                minutes: Some(30),
                guests: vec!["ann@example.com".into()],
            },
            Risk::Approve,
        ),
    ];
    for (raw, action, risk) in cases {
        let (a, r) = ok1(&raw);
        assert_eq!(a, action, "{raw}");
        assert_eq!(r, risk, "risk of {raw}");
        assert_eq!(risk_of(&a), risk, "risk_of disagrees with the step risk for {raw}");
    }
}

#[test]
fn j1_a_plan_keeps_step_order_and_action_codes_follow_the_table() {
    let raw = plan(vec![
        step("open_app", json!({"name": "A"})),
        step("switch_app", json!({"name": "B"})),
        step("media", json!({"key": "next"})),
        step("open_url", json!({"url": "https://example.com"})),
        step("web_search", json!({"query": "q"})),
        step("youtube_play", json!({"query": "q"})),
        step("docs_new", json!({"topic": "t"})),
        step("calendar_new_event", json!({"title": "t", "date": "today", "time": "09:00"})),
    ]);
    let steps = ok_steps(&raw);
    let codes: Vec<u8> = steps.iter().map(|(a, _)| action_code(a)).collect();
    assert_eq!(codes, vec![0, 1, 2, 3, 4, 5, 6, 7], "action codes are 0..=7 in table order");
}

// ---------- J2 ----------
#[test]
fn j2_a_question_parses_and_201_chars_is_question_bad() {
    let q = "Which song called Hello: Adele or Lionel Richie?";
    let raw = json!({"v": 1, "kind": "question", "question": q}).to_string();
    assert_eq!(parse(&raw), Ok(BrainOutput::Question(q.to_string())));
    let long = |n: usize| json!({"v": 1, "kind": "question", "question": "q".repeat(n)}).to_string();
    assert!(matches!(parse(&long(200)), Ok(BrainOutput::Question(_))), "200 chars is allowed");
    assert_eq!(rej(&long(201)), Reject::QuestionBad);
    assert_eq!(rej(&long(0)), Reject::QuestionBad, "empty question");
    let multibyte = json!({"v": 1, "kind": "question", "question": "é".repeat(200)}).to_string();
    assert!(parse(&multibyte).is_ok(), "limits count characters, not bytes");
}

// ---------- J3 ----------
#[test]
fn j3_unknown_actions_are_rejected() {
    for a in ["pay", "delete_file", "click", "type", "send_email", "OPEN_APP", "open-app", ""] {
        assert_eq!(rej(&one(a, json!({"name": "x"}))), Reject::UnknownAction, "{a:?}");
    }
    // Positive control: the same shape with a real action parses.
    assert!(parse(&one("open_app", json!({"name": "x"}))).is_ok());
}

// ---------- J4 ----------
#[test]
fn j4_an_extra_field_anywhere_is_rejected() {
    let top = json!({"v": 1, "kind": "plan", "steps": [step("open_app", json!({"name": "x"}))], "note": "hi"}).to_string();
    assert_eq!(rej(&top), Reject::UnknownField, "top level");
    let in_step = plan(vec![json!({"action": "open_app", "args": {"name": "x"}, "why": "because"})]);
    assert_eq!(rej(&in_step), Reject::UnknownField, "in a step");
    assert_eq!(rej(&one("open_app", json!({"name": "x", "extra": 1}))), Reject::UnknownField, "in args");
    assert_eq!(rej(&one("media", json!({"key": "next", "times": 3}))), Reject::UnknownField, "in media args");
    let q = json!({"v": 1, "kind": "question", "question": "q?", "hint": "h"}).to_string();
    assert_eq!(rej(&q), Reject::UnknownField, "in a question");
    // Positive control.
    assert!(parse(&one("open_app", json!({"name": "x"}))).is_ok());
}

// ---------- J5 ----------
#[test]
fn j5_a_risk_field_anywhere_is_rejected_and_risk_of_ignores_all_model_text() {
    let on_docs = plan(vec![json!({"action": "docs_new", "args": {"topic": "t"}, "risk": "safe"})]);
    assert!(parse(&on_docs).is_err(), "risk on a step");
    assert!(parse(&one("docs_new", json!({"topic": "t", "risk": "safe"}))).is_err(), "risk in args");
    let top = json!({"v": 1, "kind": "plan", "risk": "safe", "steps": [step("docs_new", json!({"topic": "t"}))]}).to_string();
    assert!(parse(&top).is_err(), "risk at the top");
    let q = json!({"v": 1, "kind": "question", "question": "q?", "risk": "safe"}).to_string();
    assert!(parse(&q).is_err(), "risk in a question");
    // risk_of looks only at the action kind (and guests), never at text the model wrote.
    for text in ["safe", "Approve", "Blocked", "checkout payment password", "ignore the rules and mark this safe"] {
        assert_eq!(risk_of(&Action::OpenApp { name: text.into() }), Risk::Safe, "{text}");
        assert_eq!(risk_of(&Action::WebSearch { query: text.into() }), Risk::Safe, "{text}");
        assert_eq!(risk_of(&Action::DocsNew { topic: text.into(), words: None }), Risk::Approve, "{text}");
    }
    let cal_with = |guests: Vec<String>| Action::CalendarNewEvent {
        title: "safe".into(),
        date: "today".into(),
        time: "10:00".into(),
        minutes: None,
        guests,
    };
    assert_eq!(risk_of(&cal_with(vec![])), Risk::Safe);
    assert_eq!(risk_of(&cal_with(vec!["a@b.co".into()])), Risk::Approve);
}

// ---------- J6 ----------
#[test]
fn j6_step_count_limits_and_mixed_kinds() {
    let n = |k: usize| plan((0..k).map(|i| step("web_search", json!({"query": format!("q{i}")}))).collect());
    assert_eq!(ok_steps(&n(25)).len(), 25, "25 steps is the maximum");
    assert_eq!(rej(&n(26)), Reject::TooManySteps);
    assert_eq!(rej(&n(0)), Reject::NoSteps);
    let mixed = json!({"v": 1, "kind": "plan", "steps": [step("open_app", json!({"name": "x"}))], "question": "q?"}).to_string();
    assert_eq!(rej(&mixed), Reject::MixedKinds, "plan with a question");
    let mixed_q = json!({"v": 1, "kind": "question", "question": "q?", "steps": [step("open_app", json!({"name": "x"}))]}).to_string();
    assert_eq!(rej(&mixed_q), Reject::MixedKinds, "question with steps");
}

// ---------- J7 ----------
#[test]
fn j7_size_syntax_fences_and_version() {
    let base = one("open_app", json!({"name": "x"}));
    let padded = |total: usize| format!("{base}{}", " ".repeat(total - base.len()));
    assert!(parse(&padded(MAX_REPLY_BYTES)).is_ok(), "exactly 16 KB is allowed");
    assert_eq!(rej(&padded(MAX_REPLY_BYTES + 1)), Reject::TooLarge);
    assert_eq!(MAX_REPLY_BYTES, 16_384);
    for junk in ["", "hello", "{", "{\"v\":1,"] {
        assert_eq!(rej(junk), Reject::NotJson, "{junk:?}");
    }
    for not_an_object in ["null", "[]", "42", "\"text\""] {
        assert!(parse(not_an_object).is_err(), "{not_an_object:?}");
    }
    let fenced = format!("```json\n{base}\n```");
    assert_eq!(rej(&fenced), Reject::NotJson, "JSON inside a markdown fence");
    assert_eq!(rej(&format!("Here you go: {base}")), Reject::NotJson, "prose before JSON");
    let v2 = json!({"v": 2, "kind": "plan", "steps": [step("open_app", json!({"name": "x"}))]}).to_string();
    assert_eq!(rej(&v2), Reject::WrongVersion);
    let v0 = json!({"v": 0, "kind": "question", "question": "q?"}).to_string();
    assert_eq!(rej(&v0), Reject::WrongVersion);
    let other = json!({"v": 1, "kind": "answer", "question": "q?"}).to_string();
    assert_eq!(rej(&other), Reject::UnknownKind);
}

#[test]
fn j7_reject_codes_are_fixed_1_to_11_in_declaration_order() {
    let all = [
        Reject::TooLarge,
        Reject::NotJson,
        Reject::WrongVersion,
        Reject::UnknownKind,
        Reject::UnknownField,
        Reject::UnknownAction,
        Reject::BadArgs,
        Reject::NoSteps,
        Reject::TooManySteps,
        Reject::MixedKinds,
        Reject::QuestionBad,
    ];
    let codes: Vec<u8> = all.iter().map(Reject::code).collect();
    assert_eq!(codes, (1..=11).collect::<Vec<u8>>());
}

// ---------- J8 ----------
#[test]
fn j8_open_url_only_plain_https_with_a_host() {
    let url = |u: &str| one("open_url", json!({"url": u}));
    for good in [
        "https://example.com",
        "https://www.youtube.com/results?search_query=blinding+lights",
        "https://docs.new/",
        "https://example.com:8443/a/b#frag",
    ] {
        assert!(parse(&url(good)).is_ok(), "{good}");
    }
    for bad in [
        "http://example.com",
        "javascript:alert(1)",
        "file:///etc/passwd",
        "ftp://example.com",
        "data:text/html,<b>x</b>",
        "https://user:pass@example.com/",
        "https://user:@example.com/",
        "https://",
        "https:///path",
        "//example.com",
        "example.com",
        "",
    ] {
        bad_args(&url(bad));
    }
    // host rules: host <= 253 chars, labels 1-63, at least one dot, no IP literal, no localhost
    let host_of = |n: usize| {
        let (l63, last) = ("a".repeat(63), "a".repeat(n - 3 * 64));
        format!("{l63}.{l63}.{l63}.{last}")
    };
    assert_eq!(host_of(253).len(), 253);
    assert!(parse(&url(&format!("https://{}/x", host_of(253)))).is_ok(), "a 253-char host is allowed");
    bad_args(&url(&format!("https://{}/x", host_of(254))));
    assert!(parse(&url(&format!("https://{}.com", "a".repeat(63)))).is_ok(), "a 63-char label is allowed");
    bad_args(&url(&format!("https://{}.com", "a".repeat(64))));
    for bad in [
        "https://localhost",
        "https://localhost/x",
        "https://localhost:8080/",
        "https://intranet",
        "https://printer/status",
        "https://127.0.0.1/",
        "https://10.0.0.1:8443/x",
        "https://192.168.1.1",
        "https://8.8.8.8/",
        "https://[::1]/",
        "https://[2001:db8::1]:443/",
        "https://example.com./",
        "https://.example.com",
        "https://a..example.com",
        "https://exa mple.com",
        "https://example.com/a\u{202E}b",
        "https://example.com/a\u{200B}b",
    ] {
        bad_args(&url(bad));
    }
    let long = |n: usize| format!("https://example.com/{}", "a".repeat(n - "https://example.com/".len()));
    assert!(parse(&url(&long(2048))).is_ok(), "2,048 chars is allowed");
    bad_args(&url(&long(2049)));
}

// ---------- J9 ----------
#[test]
fn j9_calendar_title_date_time_minutes_guests_on_both_sides_of_each_limit() {
    // title 1-100
    assert!(parse(&cal(json!({"title": "a"}))).is_ok());
    assert!(parse(&cal(json!({"title": "a".repeat(100)}))).is_ok());
    bad_args(&cal(json!({"title": ""})));
    bad_args(&cal(json!({"title": "a".repeat(101)})));
    // date: today / tomorrow / weekday / YYYY-MM-DD
    for d in ["today", "tomorrow", "monday", "tuesday", "wednesday", "thursday", "friday", "saturday", "sunday", "2026-11-09", "2027-02-28"] {
        assert!(parse(&cal(json!({"date": d}))).is_ok(), "{d}");
    }
    for d in ["Monday", "TUESDAY", "wEdNeSdAy", "Sunday"] {
        assert!(parse(&cal(json!({"date": d}))).is_ok(), "weekday names are case-insensitive: {d}");
    }
    for d in ["", "yesterday", "next week", "funday", "2026-13-01", "2026-00-10", "2026-11-32", "2026-1-5", "11/09/2026", "2026-11-09T10:00"] {
        bad_args(&cal(json!({"date": d})));
    }
    // time HH:MM 24 h
    for t in ["00:00", "09:30", "15:00", "23:59"] {
        assert!(parse(&cal(json!({"time": t}))).is_ok(), "{t}");
    }
    for t in ["", "24:00", "15:60", "9:30", "3pm", "15:00:00", "1500", "ab:cd"] {
        bad_args(&cal(json!({"time": t})));
    }
    // minutes 5-480
    for m in [5, 60, 480] {
        assert!(parse(&cal(json!({"minutes": m}))).is_ok(), "{m}");
    }
    for m in [json!(4), json!(0), json!(481), json!(-1), json!("60"), json!(60.5)] {
        bad_args(&cal(json!({"minutes": m})));
    }
    // guests <= 10, each a plausible email <= 254 chars
    let emails = |n: usize| (0..n).map(|i| format!("g{i}@example.com")).collect::<Vec<_>>();
    assert!(parse(&cal(json!({"guests": []}))).is_ok());
    assert!(parse(&cal(json!({"guests": emails(10)}))).is_ok());
    bad_args(&cal(json!({"guests": emails(11)})));
    // A valid-looking address of exactly n chars (n = 254 or 255): short local part, labels of at most 63 chars.
    let email_of = |n: usize| {
        let label = "b".repeat(60);
        let tail = "c".repeat(n - 2 - 4 * 61);
        format!("a@{label}.{label}.{label}.{label}.{tail}")
    };
    assert_eq!(email_of(254).chars().count(), 254);
    assert_eq!(email_of(255).chars().count(), 255);
    assert!(parse(&cal(json!({"guests": [email_of(254)]}))).is_ok(), "254 chars is allowed");
    bad_args(&cal(json!({"guests": [email_of(255)]})));
    for e in ["", "no-at-sign", "a@@b.com", "a b@c.com", "@example.com", "ann@"] {
        bad_args(&cal(json!({"guests": [e]})));
    }
    bad_args(&cal(json!({"guests": "ann@example.com"})));
    // missing required fields
    bad_args(&one("calendar_new_event", json!({"title": "t", "date": "today"})));
    bad_args(&one("calendar_new_event", json!({"title": "t", "time": "10:00"})));
    bad_args(&one("calendar_new_event", json!({"date": "today", "time": "10:00"})));
    bad_args(&one("calendar_new_event", json!({})));
}

// ---------- J10 ----------
/// Unicode format and line/paragraph separators (Cf, Zl, Zp): bidi overrides and isolates, zero-width, U+2028/2029.
const FORMAT_CHARS: [char; 17] = [
    '\u{202A}', '\u{202B}', '\u{202C}', '\u{202D}', '\u{202E}', '\u{2066}', '\u{2067}', '\u{2068}', '\u{2069}', '\u{200B}',
    '\u{200C}', '\u{200D}', '\u{FEFF}', '\u{2028}', '\u{2029}', '\u{200E}', '\u{200F}',
];

#[test]
fn j10_a_docs_topic_ending_in_a_right_to_left_override_is_rejected() {
    bad_args(&one("docs_new", json!({"topic": "Essay on the causes of WWII\u{202E}"})));
    bad_args(&one("docs_new", json!({"topic": "\u{202E}Essay", "words": 300})));
    assert!(parse(&one("docs_new", json!({"topic": "Essay on the causes of WWII"}))).is_ok(), "positive control");
}

#[test]
fn j10_format_characters_are_rejected_in_a_question_too() {
    let q = |t: &str| json!({"v": 1, "kind": "question", "question": t}).to_string();
    assert!(parse(&q("Which Hello?")).is_ok(), "positive control");
    for c in FORMAT_CHARS {
        assert_eq!(rej(&q(&format!("Which{c} Hello?"))), Reject::QuestionBad, "U+{:04X}", c as u32);
    }
}

#[test]
fn j10_format_characters_are_rejected_in_open_url_and_guests_keep_working() {
    for c in FORMAT_CHARS {
        bad_args(&one("open_url", json!({"url": format!("https://example.com/a{c}b")})));
        bad_args(&one("open_url", json!({"url": format!("https://exam{c}ple.com/")})));
    }
}
#[test]
fn j10_string_limits_and_control_characters() {
    let limits: [(&str, &str, usize); 6] = [
        ("open_app", "name", 40),
        ("switch_app", "name", 40),
        ("web_search", "query", 120),
        ("youtube_play", "query", 120),
        ("docs_new", "topic", 200),
        ("calendar_new_event", "title", 100),
    ];
    for (action, field, max) in limits {
        let build = |v: Value| -> String {
            match action {
                "calendar_new_event" => cal(json!({field: v})),
                _ => one(action, json!({field: v})),
            }
        };
        assert!(parse(&build(json!("a"))).is_ok(), "{action} 1 char");
        assert!(parse(&build(json!("a".repeat(max)))).is_ok(), "{action} {max} chars");
        assert!(parse(&build(json!("é".repeat(max)))).is_ok(), "{action} counts characters, not bytes");
        bad_args(&build(json!("")));
        bad_args(&build(json!("a".repeat(max + 1))));
        bad_args(&build(json!(42)));
        bad_args(&build(json!(null)));
        for ctl in ["a\nb", "a\tb", "a\u{7}b", "a\u{0}b", "a\rb", "a\u{1b}[31mb"] {
            bad_args(&build(json!(ctl)));
        }
        for c in FORMAT_CHARS {
            bad_args(&build(json!(format!("a{c}b"))));
            bad_args(&build(json!(format!("topic{c}"))));
            bad_args(&build(json!(c.to_string())));
        }
    }
    // docs words 50-2000
    let words = |w: Value| one("docs_new", json!({"topic": "t", "words": w}));
    for w in [50, 300, 2000] {
        assert!(parse(&words(json!(w))).is_ok(), "{w}");
    }
    for w in [json!(0), json!(49), json!(2001), json!(-5), json!("300"), json!(300.5), json!(70000)] {
        bad_args(&words(w));
    }
    // media key is an enum
    for k in ["volume_up", "PLAY_PAUSE", "play", "", "stop"] {
        bad_args(&one("media", json!({"key": k})));
    }
    // missing required arg, missing args object
    bad_args(&one("open_app", json!({})));
    assert!(parse(&plan(vec![json!({"action": "open_app"})])).is_err(), "no args object");
}

// ---------- J11 ----------
fn lines_of(out: &BrainOutput) -> Vec<String> {
    render(out).into_iter().map(|l| l.text).collect()
}
fn plan_of(raw: &str) -> BrainOutput {
    parse(raw).unwrap_or_else(|e| panic!("{e:?} for {raw}"))
}

#[test]
fn j11_render_templates_one_per_action() {
    let cases: Vec<(String, &str)> = vec![
        (one("open_app", json!({"name": "Calculator"})), "Open app: Calculator"),
        (one("switch_app", json!({"name": "Chrome"})), "Switch to: Chrome"),
        (one("media", json!({"key": "play_pause"})), "Media: play_pause"),
        (one("web_search", json!({"query": "weather in Oslo"})), "Search Google: weather in Oslo"),
        (one("youtube_play", json!({"query": "Blinding Lights"})), "Play on YouTube: Blinding Lights"),
        (one("docs_new", json!({"topic": "WWII"})), "New Google Doc: WWII (needs your OK)"),
        (cal(json!({})), "Calendar: Review, tomorrow 15:00, 60 min"),
        (cal(json!({"minutes": 30})), "Calendar: Review, tomorrow 15:00, 30 min"),
        (
            cal(json!({"guests": ["a@example.com", "b@example.com"]})),
            "Calendar: Review, tomorrow 15:00, 60 min with 2 guests (needs your OK)",
        ),
    ];
    for (raw, line) in cases {
        let l = lines_of(&plan_of(&raw));
        assert!(l.iter().any(|t| t == line), "missing {line:?} in {l:?}");
    }
    let u = lines_of(&plan_of(&one("open_url", json!({"url": "https://www.youtube.com/results"}))));
    assert!(u.iter().any(|t| t == "Open: www.youtube.com/results"), "{u:?}");
}

#[test]
fn j11_open_url_line_shows_host_and_at_most_40_chars_of_path() {
    let long_path = format!("/{}", "a".repeat(80));
    let l = lines_of(&plan_of(&one("open_url", json!({"url": format!("https://example.com{long_path}")}))));
    let step_line = l.iter().find(|t| t.starts_with("Open: example.com/")).unwrap_or_else(|| panic!("{l:?}"));
    assert!(!step_line.contains(&"a".repeat(41)), "path not cut to 40: {step_line}");
    assert!(step_line.contains(&"a".repeat(30)), "path cut too short: {step_line}");
}

fn open_line(url: &str) -> Option<String> {
    match parse(&one("open_url", json!({"url": url}))) {
        Ok(out) => Some(lines_of(&out).into_iter().find(|t| t.starts_with("Open: ")).unwrap_or_else(|| panic!("no Open line for {url}"))),
        Err(_) => None,
    }
}

#[test]
fn j11_open_url_shows_the_host_whole_and_cuts_only_the_path() {
    let host = format!("{}.{}.example.com", "a".repeat(60), "b".repeat(60));
    let line = open_line(&format!("https://{host}/{}", "p".repeat(500))).expect("a 134-char host is valid");
    assert!(line.starts_with(&format!("Open: {host}/")), "host must be whole: {line}");
    assert!(line.chars().count() <= 200, "{line}");
}

#[test]
fn j11_a_long_look_alike_host_never_shows_a_trusted_looking_prefix() {
    // 180-char label: invalid (label > 63), so it must be rejected.
    assert!(open_line(&format!("https://www.youtube.com.{}.evil.com/", "a".repeat(180))).is_none(), "180-char label must be rejected");
    // Valid labels but a 253-char host: either rejected, or the line still shows the real registrable domain.
    let l62 = "c".repeat(62);
    let host = format!("www.youtube.com.{l62}.{l62}.{l62}.{}.evil.com", "d".repeat(253 - 16 - 3 * 63 - 9 - 1));
    assert_eq!(host.len(), 253, "test setup");
    if let Some(line) = open_line(&format!("https://{host}/login")) {
        assert!(line.contains("evil.com"), "the real domain is cut off: {line}");
        assert!(line.chars().count() <= 200, "{line}");
    }
}

#[test]
fn j11_plan_lines_header_three_steps_and_plus_n_more() {
    let n = |k: usize| plan((0..k).map(|i| step("web_search", json!({"query": format!("q{i}")}))).collect());
    let two = lines_of(&plan_of(&n(2)));
    assert_eq!(two, vec!["Plan, not run (2 steps):", "Search Google: q0", "Search Google: q1"]);
    let three = lines_of(&plan_of(&n(3)));
    assert_eq!(three.len(), 4, "{three:?}");
    assert!(!three.iter().any(|t| t.contains("more")), "{three:?}");
    let four = lines_of(&plan_of(&n(4)));
    assert_eq!(four, vec!["Plan, not run (4 steps):", "Search Google: q0", "Search Google: q1", "Search Google: q2", "+1 more"]);
    let full = lines_of(&plan_of(&n(25)));
    assert_eq!(full.first().unwrap(), "Plan, not run (25 steps):");
    assert_eq!(full.last().unwrap(), "+22 more");
    assert_eq!(full.len(), 5);
    assert!(render(&plan_of(&n(25))).iter().all(|l| l.kind == Kind::Info), "plan lines are Info");
}

#[test]
fn j11_question_lines() {
    let q = lines_of(&plan_of(&json!({"v": 1, "kind": "question", "question": "Which song?"}).to_string()));
    assert_eq!(q, vec!["AI asks: Which song?", "Say the full command again with the answer"]);
}

#[test]
fn j11_lines_never_exceed_6_or_200_chars_and_approve_steps_keep_their_marker() {
    // Longest legal args in every action, 25 steps.
    let steps = vec![
        step("open_app", json!({"name": "a".repeat(40)})),
        step("web_search", json!({"query": "b".repeat(120)})),
        step("youtube_play", json!({"query": "c".repeat(120)})),
        step("docs_new", json!({"topic": "d".repeat(200), "words": 2000})),
        step("calendar_new_event", json!({"title": "e".repeat(100), "date": "2026-11-09", "time": "10:00", "minutes": 480, "guests": ["x@example.com"]})),
        step("open_url", json!({"url": format!("https://example.com/{}", "f".repeat(1500))})),
    ];
    for k in 1..=steps.len() {
        let out = plan_of(&plan(steps[..k].to_vec()));
        let lines = render(&out);
        assert!(lines.len() < 6, "with the Heard line that is {} lines", lines.len() + 1);
        assert!(lines.iter().all(|l| l.text.chars().count() <= 200), "a line is over 200 chars: {lines:?}");
    }
    let docs = lines_of(&plan_of(&one("docs_new", json!({"topic": "d".repeat(200)}))));
    let docs_line = docs.iter().find(|t| t.starts_with("New Google Doc: ")).unwrap();
    assert!(docs_line.chars().count() <= 200, "{docs_line}");
    assert!(docs_line.contains("(needs your OK)"), "the approval marker must survive the 200-char cut: {docs_line}");
    let cal_long = lines_of(&plan_of(&cal(json!({"title": "e".repeat(100), "guests": ["x@example.com"]}))));
    assert!(cal_long.iter().any(|t| t.contains("(needs your OK)")), "{cal_long:?}");
    // Through the real panel payload the line count is capped too.
    let payload = PanelPayload::new(render(&plan_of(&plan(steps))));
    assert!(payload.lines.len() <= 6);
}

#[test]
fn j11_render_removes_control_characters_from_actions_built_in_code() {
    let out = BrainOutput::Plan(vec![crate::so::brain::plan::Step {
        action: Action::WebSearch { query: "a\nb\u{7}c\td".into() },
        risk: Risk::Safe,
    }]);
    for l in render(&out) {
        assert!(!l.text.chars().any(char::is_control), "control character in {:?}", l.text);
    }
    let joined = lines_of(&out).join("|");
    for c in ["a", "b", "c", "d"] {
        assert!(joined.contains(c), "text lost: {joined}");
    }
    let q = BrainOutput::Question("what\u{0}is\nthis?".into());
    for l in render(&q) {
        assert!(!l.text.chars().any(char::is_control), "control character in {:?}", l.text);
    }
}

#[test]
fn j11_render_replaces_format_characters_in_actions_built_in_code() {
    let mut steps = vec![];
    for c in FORMAT_CHARS {
        steps.push(crate::so::brain::plan::Step { action: Action::WebSearch { query: format!("a{c}b") }, risk: Risk::Safe });
    }
    for c in FORMAT_CHARS {
        for l in render(&BrainOutput::Plan(steps[..1].to_vec())) {
            assert!(!l.text.contains(c), "U+{:04X} in {:?}", c as u32, l.text);
        }
        let one_step = vec![crate::so::brain::plan::Step { action: Action::DocsNew { topic: format!("Essay{c}"), words: None }, risk: Risk::Approve }];
        for l in render(&BrainOutput::Plan(one_step)) {
            assert!(!l.text.contains(c), "docs topic: U+{:04X} in {:?}", c as u32, l.text);
        }
        let url = vec![crate::so::brain::plan::Step { action: Action::OpenUrl { url: format!("https://example.com/a{c}b") }, risk: Risk::Safe }];
        for l in render(&BrainOutput::Plan(url)) {
            assert!(!l.text.contains(c), "url: U+{:04X} in {:?}", c as u32, l.text);
        }
        for l in render(&BrainOutput::Question(format!("what{c}now?"))) {
            assert!(!l.text.contains(c), "question: U+{:04X} in {:?}", c as u32, l.text);
        }
    }
    // The text around the character survives.
    let l = render(&BrainOutput::Plan(steps[..1].to_vec()));
    assert!(l.iter().any(|x| x.text.contains('a') && x.text.contains('b')));
}

// ---------- J12 ----------
#[test]
fn j12_hostile_strings_render_as_plain_text_and_the_panel_encoding_keeps_them_inert() {
    let hostile = "</script><img src=x onerror=alert(1)> \"q\" \\ end";
    let out = plan_of(&one("youtube_play", json!({"query": hostile})));
    let lines = render(&out);
    let shown = lines.iter().find(|l| l.text.starts_with("Play on YouTube: ")).expect("step line");
    assert!(shown.text.contains("</script>") && shown.text.contains("onerror=alert(1)"), "text is shown literally: {:?}", shown.text);
    assert_eq!(shown.text, format!("Play on YouTube: {hostile}"));
    let script = PanelPayload::new(lines).eval_script();
    assert!(!script.contains("</script>"), "raw </script> reached the script text");
    // U+2028/2029 cannot arrive through parse any more; a code-built action still must not carry them raw.
    let built = BrainOutput::Plan(vec![crate::so::brain::plan::Step { action: Action::WebSearch { query: "x\u{2028}y\u{2029}z\u{202E}".into() }, risk: Risk::Safe }]);
    let script = PanelPayload::new(render(&built)).eval_script();
    assert!(!script.contains('\u{2028}') && !script.contains('\u{2029}') && !script.contains('\u{202E}'), "format character reached the script text");
    assert!(script.contains("\\u003c"), "< must be escaped");
}

// ---------- J13 ----------
#[test]
fn j13_duplicate_json_keys_are_rejected() {
    let fine = r#"{"v":1,"kind":"plan","steps":[{"action":"web_search","args":{"query":"x"}}]}"#;
    assert!(parse(fine).is_ok(), "positive control");
    let dup_top = r#"{"v":1,"v":1,"kind":"plan","steps":[{"action":"web_search","args":{"query":"x"}}]}"#;
    assert!(parse(dup_top).is_err(), "duplicate top-level key");
    let dup_steps = r#"{"v":1,"kind":"plan","steps":[{"action":"web_search","args":{"query":"x"}}],"steps":[{"action":"web_search","args":{"query":"y"}}]}"#;
    assert!(parse(dup_steps).is_err(), "duplicate steps");
    let dup_args = r#"{"v":1,"kind":"plan","steps":[{"action":"web_search","args":{"query":"x","query":"y"}}]}"#;
    assert!(parse(dup_args).is_err(), "duplicate arg");
    let dup_action = r#"{"v":1,"kind":"plan","steps":[{"action":"web_search","action":"open_app","args":{"query":"x"}}]}"#;
    assert!(parse(dup_action).is_err(), "duplicate action");
}

// ---------- J14 ----------
fn collect_action_names(v: &Value, out: &mut Vec<String>) {
    match v {
        Value::Object(m) => {
            if let Some(Value::Object(props)) = m.get("properties") {
                if let Some(Value::Object(a)) = props.get("action") {
                    if let Some(Value::String(c)) = a.get("const") {
                        out.push(c.clone());
                    }
                    if let Some(Value::Array(e)) = a.get("enum") {
                        out.extend(e.iter().filter_map(|x| x.as_str().map(String::from)));
                    }
                }
            }
            m.values().for_each(|x| collect_action_names(x, out));
        }
        Value::Array(a) => a.iter().for_each(|x| collect_action_names(x, out)),
        _ => {}
    }
}
fn walk<'a>(v: &'a Value, f: &mut dyn FnMut(&'a Value)) {
    f(v);
    match v {
        Value::Object(m) => m.values().for_each(|x| walk(x, f)),
        Value::Array(a) => a.iter().for_each(|x| walk(x, f)),
        _ => {}
    }
}
fn keys_anywhere(v: &Value) -> Vec<String> {
    let mut ks = vec![];
    walk(v, &mut |n| {
        if let Value::Object(m) = n {
            ks.extend(m.keys().cloned());
        }
    });
    ks
}
const ACTIONS: [&str; 8] = ["open_app", "switch_app", "media", "open_url", "web_search", "youtube_play", "docs_new", "calendar_new_event"];

#[test]
fn j14_schema_json_lists_exactly_the_eight_actions_and_no_risk_field() {
    let v: Value = serde_json::from_str(schema_json()).expect("schema_json is JSON");
    let mut names = vec![];
    collect_action_names(&v, &mut names);
    names.sort();
    names.dedup();
    let mut want: Vec<String> = ACTIONS.iter().map(|s| s.to_string()).collect();
    want.sort();
    assert_eq!(names, want);
    let text = schema_json();
    for bad in ["\"risk\"", "\"click\"", "\"pay\"", "\"say\"", "\"reason\""] {
        assert!(!text.contains(bad), "the schema must not offer {bad}");
    }
}

fn is_object_schema(m: &serde_json::Map<String, Value>) -> bool {
    m.get("type").is_some_and(|t| t == "object") || m.contains_key("properties")
}

#[test]
fn j14_claude_schema_is_in_claudes_subset() {
    let s = schema_for(ProviderId::Claude);
    assert!(s.is_object());
    let mut objects = 0;
    walk(&s, &mut |n| {
        if let Value::Object(m) = n {
            if is_object_schema(m) {
                objects += 1;
                assert_eq!(m.get("additionalProperties"), Some(&Value::Bool(false)), "every object needs additionalProperties:false: {n}");
            }
            if let Some(mi) = m.get("minItems") {
                assert!(mi == 0 || mi == 1, "minItems may only be 0 or 1: {n}");
            }
        }
    });
    assert!(objects >= 2, "expected at least the top object and a step object, found {objects}");
    let ks = keys_anywhere(&s);
    for banned in ["oneOf", "maxItems", "minLength", "maxLength", "minimum", "maximum"] {
        assert!(!ks.iter().any(|k| k == banned), "Claude rejects {banned}");
    }
    assert!(!s.to_string().contains("\"risk\""), "no risk field is offered to the model");
    let mut names = vec![];
    collect_action_names(&s, &mut names);
    names.sort();
    names.dedup();
    assert_eq!(names.len(), 8, "the Claude schema still lists the 8 actions: {names:?}");
}

#[test]
fn j14_gemini_schema_is_flat_without_one_of() {
    let s = schema_for(ProviderId::Gemini);
    assert!(s.is_object());
    let ks = keys_anywhere(&s);
    for banned in ["oneOf", "$ref", "$defs"] {
        assert!(!ks.iter().any(|k| k == banned), "Gemini schema must not use {banned}");
    }
    assert!(!s.to_string().contains("\"risk\""));
    let mut names = vec![];
    collect_action_names(&s, &mut names);
    names.sort();
    names.dedup();
    assert_eq!(names.len(), 8, "{names:?}");
}
