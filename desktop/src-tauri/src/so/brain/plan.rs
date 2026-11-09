// SPDX-License-Identifier: FSL-1.1-MIT
//! The plan: strict schema, parser, risk table, panel rendering. Pure code, no I/O.
//! Risk is set here by code (`risk_of`), never by the AI.

use super::provider::ProviderId;
use crate::so::panel::{Kind, Line};
use serde::de::{self, Deserialize, MapAccess, SeqAccess, Visitor};
use serde_json::{Map, Number, Value};

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
        match self {
            Reject::TooLarge => 1,
            Reject::NotJson => 2,
            Reject::WrongVersion => 3,
            Reject::UnknownKind => 4,
            Reject::UnknownField => 5,
            Reject::UnknownAction => 6,
            Reject::BadArgs => 7,
            Reject::NoSteps => 8,
            Reject::TooManySteps => 9,
            Reject::MixedKinds => 10,
            Reject::QuestionBad => 11,
        }
    }
}

// ---------- strict JSON: like `serde_json::Value`, but a repeated key is an error ----------

/// A JSON value read by our own visitor. (A "visitor" is serde's hook for each kind of JSON item.)
/// `serde_json::Value` silently keeps the last of two equal keys; the AI must not get that room.
struct Strict(Value);

struct StrictVisitor;

impl<'de> Visitor<'de> for StrictVisitor {
    type Value = Strict;

    fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.write_str("any JSON value")
    }
    fn visit_bool<E: de::Error>(self, v: bool) -> Result<Strict, E> {
        Ok(Strict(Value::Bool(v)))
    }
    fn visit_i64<E: de::Error>(self, v: i64) -> Result<Strict, E> {
        Ok(Strict(Value::Number(Number::from(v))))
    }
    fn visit_u64<E: de::Error>(self, v: u64) -> Result<Strict, E> {
        Ok(Strict(Value::Number(Number::from(v))))
    }
    fn visit_f64<E: de::Error>(self, v: f64) -> Result<Strict, E> {
        Ok(Strict(Number::from_f64(v).map_or(Value::Null, Value::Number)))
    }
    fn visit_str<E: de::Error>(self, v: &str) -> Result<Strict, E> {
        Ok(Strict(Value::String(v.to_string())))
    }
    fn visit_unit<E: de::Error>(self) -> Result<Strict, E> {
        Ok(Strict(Value::Null))
    }
    fn visit_none<E: de::Error>(self) -> Result<Strict, E> {
        Ok(Strict(Value::Null))
    }
    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Strict, A::Error> {
        let mut items = Vec::new();
        while let Some(Strict(v)) = seq.next_element()? {
            items.push(v);
        }
        Ok(Strict(Value::Array(items)))
    }
    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Strict, A::Error> {
        let mut out = Map::new();
        while let Some(key) = map.next_key::<String>()? {
            if out.contains_key(&key) {
                return Err(de::Error::custom("duplicate key"));
            }
            let Strict(v) = map.next_value()?;
            out.insert(key, v);
        }
        Ok(Strict(Value::Object(out)))
    }
}

impl<'de> Deserialize<'de> for Strict {
    fn deserialize<D: de::Deserializer<'de>>(d: D) -> Result<Strict, D::Error> {
        d.deserialize_any(StrictVisitor)
    }
}

// ---------- parse ----------

/// Parses and checks the model's reply. Any problem rejects the whole reply. Order of checks: size,
/// JSON, version, kind, unknown or mixed fields, then the content.
pub fn parse(raw: &str) -> Result<BrainOutput, Reject> {
    if raw.len() > MAX_REPLY_BYTES {
        return Err(Reject::TooLarge);
    }
    let Strict(doc) = serde_json::from_str::<Strict>(raw).map_err(|_| Reject::NotJson)?;
    let Value::Object(top) = doc else {
        return Err(Reject::NotJson);
    };
    if top.get("v").and_then(Value::as_u64) != Some(u64::from(PLAN_VERSION)) {
        return Err(Reject::WrongVersion);
    }
    match top.get("kind").and_then(Value::as_str) {
        Some("plan") => {
            if top.contains_key("question") {
                return Err(Reject::MixedKinds);
            }
            only_keys(&top, &["v", "kind", "steps"])?;
            parse_plan(top.get("steps"))
        }
        Some("question") => {
            if top.contains_key("steps") {
                return Err(Reject::MixedKinds);
            }
            only_keys(&top, &["v", "kind", "question"])?;
            match top.get("question").and_then(Value::as_str) {
                Some(q) if (1..=200).contains(&q.chars().count()) => Ok(BrainOutput::Question(q.to_string())),
                _ => Err(Reject::QuestionBad),
            }
        }
        _ => Err(Reject::UnknownKind),
    }
}

fn only_keys(m: &Map<String, Value>, allowed: &[&str]) -> Result<(), Reject> {
    if m.keys().all(|k| allowed.contains(&k.as_str())) {
        Ok(())
    } else {
        Err(Reject::UnknownField)
    }
}

fn parse_plan(steps: Option<&Value>) -> Result<BrainOutput, Reject> {
    let Some(Value::Array(items)) = steps else {
        return Err(Reject::NoSteps);
    };
    if items.is_empty() {
        return Err(Reject::NoSteps);
    }
    if items.len() > MAX_STEPS {
        return Err(Reject::TooManySteps);
    }
    let mut out = Vec::with_capacity(items.len());
    for item in items {
        let Value::Object(step) = item else {
            return Err(Reject::BadArgs);
        };
        only_keys(step, &["action", "args"])?;
        let action = parse_action(step.get("action"), step.get("args"))?;
        let risk = risk_of(&action);
        out.push(Step { action, risk });
    }
    Ok(BrainOutput::Plan(out))
}

fn parse_action(name: Option<&Value>, args: Option<&Value>) -> Result<Action, Reject> {
    let name = name.and_then(Value::as_str).unwrap_or("");
    let allowed: &[&str] = match name {
        "open_app" | "switch_app" => &["name"],
        "media" => &["key"],
        "open_url" => &["url"],
        "web_search" | "youtube_play" => &["query"],
        "docs_new" => &["topic", "words"],
        "calendar_new_event" => &["title", "date", "time", "minutes", "guests"],
        _ => return Err(Reject::UnknownAction),
    };
    let Some(Value::Object(args)) = args else {
        return Err(Reject::BadArgs);
    };
    only_keys(args, allowed)?;
    let text = |k: &str, max: usize| text_arg(args, k, max);
    Ok(match name {
        "open_app" => Action::OpenApp { name: text("name", 40)? },
        "switch_app" => Action::SwitchApp { name: text("name", 40)? },
        "media" => Action::Media {
            key: match args.get("key").and_then(Value::as_str) {
                Some("play_pause") => MediaArg::PlayPause,
                Some("next") => MediaArg::Next,
                Some("previous") => MediaArg::Previous,
                _ => return Err(Reject::BadArgs),
            },
        },
        "open_url" => Action::OpenUrl { url: url_arg(args.get("url"))? },
        "web_search" => Action::WebSearch { query: text("query", 120)? },
        "youtube_play" => Action::YoutubePlay { query: text("query", 120)? },
        "docs_new" => Action::DocsNew { topic: text("topic", 200)?, words: opt_number(args, "words", 50, 2000)? },
        _ => Action::CalendarNewEvent {
            title: text("title", 100)?,
            date: date_arg(&text("date", 10)?)?,
            time: time_arg(&text("time", 5)?)?,
            minutes: opt_number(args, "minutes", 5, 480)?,
            guests: guests_arg(args.get("guests"))?,
        },
    })
}

/// A required string of 1..=max characters with no control characters.
fn text_arg(args: &Map<String, Value>, key: &str, max: usize) -> Result<String, Reject> {
    match args.get(key) {
        Some(Value::String(s)) if (1..=max).contains(&s.chars().count()) && !s.chars().any(char::is_control) => Ok(s.clone()),
        _ => Err(Reject::BadArgs),
    }
}

/// An optional whole number in `lo..=hi`. Present but wrong (text, float, negative, null) = reject.
fn opt_number(args: &Map<String, Value>, key: &str, lo: u64, hi: u64) -> Result<Option<u16>, Reject> {
    match args.get(key) {
        None => Ok(None),
        Some(v) => match v.as_u64() {
            Some(n) if (lo..=hi).contains(&n) => Ok(u16::try_from(n).ok()),
            _ => Err(Reject::BadArgs),
        },
    }
}

const WEEKDAYS: [&str; 7] = ["monday", "tuesday", "wednesday", "thursday", "friday", "saturday", "sunday"];

fn date_arg(s: &str) -> Result<String, Reject> {
    if s == "today" || s == "tomorrow" || WEEKDAYS.contains(&s) {
        return Ok(s.to_string());
    }
    let b = s.as_bytes();
    let digits = |r: std::ops::Range<usize>| b[r].iter().all(u8::is_ascii_digit);
    if b.len() == 10 && b[4] == b'-' && b[7] == b'-' && digits(0..4) && digits(5..7) && digits(8..10) {
        let year: u32 = s[0..4].parse().map_err(|_| Reject::BadArgs)?;
        let month: u32 = s[5..7].parse().map_err(|_| Reject::BadArgs)?;
        let day: u32 = s[8..10].parse().map_err(|_| Reject::BadArgs)?;
        let leap = year.is_multiple_of(4) && (!year.is_multiple_of(100) || year.is_multiple_of(400));
        let days = match month {
            1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
            4 | 6 | 9 | 11 => 30,
            2 if leap => 29,
            2 => 28,
            _ => return Err(Reject::BadArgs),
        };
        if (1..=days).contains(&day) {
            return Ok(s.to_string());
        }
    }
    Err(Reject::BadArgs)
}

fn time_arg(s: &str) -> Result<String, Reject> {
    let b = s.as_bytes();
    if b.len() == 5 && b[2] == b':' && [0, 1, 3, 4].iter().all(|&i| b[i].is_ascii_digit()) {
        let h: u32 = s[0..2].parse().map_err(|_| Reject::BadArgs)?;
        let m: u32 = s[3..5].parse().map_err(|_| Reject::BadArgs)?;
        if h < 24 && m < 60 {
            return Ok(s.to_string());
        }
    }
    Err(Reject::BadArgs)
}

fn guests_arg(v: Option<&Value>) -> Result<Vec<String>, Reject> {
    let Some(v) = v else {
        return Ok(Vec::new());
    };
    let Value::Array(items) = v else {
        return Err(Reject::BadArgs);
    };
    if items.len() > 10 {
        return Err(Reject::BadArgs);
    }
    items
        .iter()
        .map(|g| match g {
            Value::String(s) if email_ok(s) => Ok(s.clone()),
            _ => Err(Reject::BadArgs),
        })
        .collect()
}

/// A plausible address: one `@`, a local part of 1..=64 characters, a domain of dot-separated labels
/// (1..=63 characters each, letters, digits and `-`), at most 254 characters, no spaces or control characters.
fn email_ok(s: &str) -> bool {
    if s.chars().count() > 254 || s.chars().any(|c| c.is_control() || c.is_whitespace()) {
        return false;
    }
    let mut parts = s.split('@');
    let (Some(local), Some(domain), None) = (parts.next(), parts.next(), parts.next()) else {
        return false;
    };
    let labels: Vec<&str> = domain.split('.').collect();
    (1..=64).contains(&local.chars().count())
        && labels.len() >= 2
        && labels.iter().all(|l| (1..=63).contains(&l.len()) && l.chars().all(|c| c.is_ascii_alphanumeric() || c == '-'))
}

/// `https` only, at most 2,048 characters, a host, no user:password part, no spaces or control characters.
fn url_arg(v: Option<&Value>) -> Result<String, Reject> {
    let Some(Value::String(url)) = v else {
        return Err(Reject::BadArgs);
    };
    if url.chars().count() > 2048 || url.chars().any(|c| c.is_control() || c.is_whitespace()) {
        return Err(Reject::BadArgs);
    }
    let rest = url.strip_prefix("https:").and_then(|r| r.strip_prefix("//")).ok_or(Reject::BadArgs)?;
    let (authority, _) = split_authority(rest);
    let (host, port) = match authority.split_once(':') {
        Some((h, p)) => (h, Some(p)),
        None => (authority, None),
    };
    let host_ok = !host.is_empty() && host.chars().all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-');
    let port_ok = port.is_none_or(|p| !p.is_empty() && p.len() <= 5 && p.bytes().all(|b| b.is_ascii_digit()));
    if host_ok && port_ok {
        Ok(url.clone())
    } else {
        Err(Reject::BadArgs)
    }
}

/// Splits "host:port/path?query#frag" into ("host:port", "/path?query#frag").
fn split_authority(rest: &str) -> (&str, &str) {
    let end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    rest.split_at(end)
}

// ---------- risk and codes ----------

/// Risk comes from the action kind (and guests) only; never from text the model wrote.
pub fn risk_of(a: &Action) -> Risk {
    match a {
        Action::DocsNew { .. } => Risk::Approve,
        Action::CalendarNewEvent { guests, .. } if !guests.is_empty() => Risk::Approve,
        _ => Risk::Safe,
    }
}

/// 0..=7 in table order.
pub fn action_code(a: &Action) -> u8 {
    match a {
        Action::OpenApp { .. } => 0,
        Action::SwitchApp { .. } => 1,
        Action::Media { .. } => 2,
        Action::OpenUrl { .. } => 3,
        Action::WebSearch { .. } => 4,
        Action::YoutubePlay { .. } => 5,
        Action::DocsNew { .. } => 6,
        Action::CalendarNewEvent { .. } => 7,
    }
}

// ---------- render ----------

const MAX_LINE_CHARS: usize = 200;
const SHOWN_STEPS: usize = 3;
const URL_PATH_CHARS: usize = 40;

/// `prefix + body + suffix`, at most 200 characters: only the body is cut (with `…`), so a closing
/// "(needs your OK)" always survives. Control characters become spaces.
fn fit(prefix: &str, body: &str, suffix: &str) -> String {
    let clean = |s: &str| s.chars().map(|c| if c.is_control() { ' ' } else { c }).collect::<String>();
    let (prefix, body, suffix) = (clean(prefix), clean(body), clean(suffix));
    let room = MAX_LINE_CHARS.saturating_sub(prefix.chars().count() + suffix.chars().count());
    let body = if body.chars().count() <= room {
        body
    } else {
        let mut cut: String = body.chars().take(room.saturating_sub(1)).collect();
        cut.push('…');
        cut
    };
    format!("{prefix}{body}{suffix}")
}

fn step_line(a: &Action) -> String {
    let plural = |n: usize| if n == 1 { "" } else { "s" };
    match a {
        Action::OpenApp { name } => fit("Open app: ", name, ""),
        Action::SwitchApp { name } => fit("Switch to: ", name, ""),
        Action::Media { key } => {
            let k = match key {
                MediaArg::PlayPause => "play_pause",
                MediaArg::Next => "next",
                MediaArg::Previous => "previous",
            };
            format!("Media: {k}")
        }
        Action::OpenUrl { url } => {
            let rest = url.strip_prefix("https:").and_then(|r| r.strip_prefix("//")).unwrap_or(url);
            let (host, path) = split_authority(rest);
            let path: String = path.chars().take(URL_PATH_CHARS).collect();
            fit("Open: ", &format!("{host}{path}"), "")
        }
        Action::WebSearch { query } => fit("Search Google: ", query, ""),
        Action::YoutubePlay { query } => fit("Play on YouTube: ", query, ""),
        Action::DocsNew { topic, .. } => fit("New Google Doc: ", topic, " (needs your OK)"),
        Action::CalendarNewEvent { title, date, time, minutes, guests } => {
            let mut tail = format!(", {date} {time}, {} min", minutes.unwrap_or(60));
            if !guests.is_empty() {
                tail.push_str(&format!(" with {} guest{} (needs your OK)", guests.len(), plural(guests.len())));
            }
            fit("Calendar: ", title, &tail)
        }
    }
}

/// Panel lines for a checked reply (at most 5; the "Heard" line makes 6).
pub fn render(out: &BrainOutput) -> Vec<Line> {
    match out {
        BrainOutput::Question(q) => vec![
            Line::new(fit("AI asks: ", q, ""), Kind::Info),
            Line::new("Say the full command again with the answer", Kind::Info),
        ],
        BrainOutput::Plan(steps) => {
            let n = steps.len();
            let mut lines = vec![Line::new(format!("Plan, not run ({n} step{}):", if n == 1 { "" } else { "s" }), Kind::Info)];
            lines.extend(steps.iter().take(SHOWN_STEPS).map(|s| Line::new(step_line(&s.action), Kind::Info)));
            if n > SHOWN_STEPS {
                lines.push(Line::new(format!("+{} more", n - SHOWN_STEPS), Kind::Info));
            }
            lines
        }
    }
}

// ---------- schemas ----------

const ACTION_NAMES: &str = r#"["open_app","switch_app","media","open_url","web_search","youtube_play","docs_new","calendar_new_event"]"#;

/// The full schema (documentation and test J14): one shape per action.
pub fn schema_json() -> &'static str {
    r##"{"$schema":"https://json-schema.org/draft/2020-12/schema","type":"object","additionalProperties":false,"required":["v","kind"],"properties":{"v":{"const":1},"kind":{"enum":["plan","question"]},"question":{"type":"string","minLength":1,"maxLength":200},"steps":{"type":"array","minItems":1,"maxItems":25,"items":{"oneOf":[
{"type":"object","additionalProperties":false,"required":["action","args"],"properties":{"action":{"const":"open_app"},"args":{"type":"object","additionalProperties":false,"required":["name"],"properties":{"name":{"type":"string","minLength":1,"maxLength":40}}}}},
{"type":"object","additionalProperties":false,"required":["action","args"],"properties":{"action":{"const":"switch_app"},"args":{"type":"object","additionalProperties":false,"required":["name"],"properties":{"name":{"type":"string","minLength":1,"maxLength":40}}}}},
{"type":"object","additionalProperties":false,"required":["action","args"],"properties":{"action":{"const":"media"},"args":{"type":"object","additionalProperties":false,"required":["key"],"properties":{"key":{"enum":["play_pause","next","previous"]}}}}},
{"type":"object","additionalProperties":false,"required":["action","args"],"properties":{"action":{"const":"open_url"},"args":{"type":"object","additionalProperties":false,"required":["url"],"properties":{"url":{"type":"string","maxLength":2048}}}}},
{"type":"object","additionalProperties":false,"required":["action","args"],"properties":{"action":{"const":"web_search"},"args":{"type":"object","additionalProperties":false,"required":["query"],"properties":{"query":{"type":"string","minLength":1,"maxLength":120}}}}},
{"type":"object","additionalProperties":false,"required":["action","args"],"properties":{"action":{"const":"youtube_play"},"args":{"type":"object","additionalProperties":false,"required":["query"],"properties":{"query":{"type":"string","minLength":1,"maxLength":120}}}}},
{"type":"object","additionalProperties":false,"required":["action","args"],"properties":{"action":{"const":"docs_new"},"args":{"type":"object","additionalProperties":false,"required":["topic"],"properties":{"topic":{"type":"string","minLength":1,"maxLength":200},"words":{"type":"integer","minimum":50,"maximum":2000}}}}},
{"type":"object","additionalProperties":false,"required":["action","args"],"properties":{"action":{"const":"calendar_new_event"},"args":{"type":"object","additionalProperties":false,"required":["title","date","time"],"properties":{"title":{"type":"string","minLength":1,"maxLength":100},"date":{"type":"string"},"time":{"type":"string"},"minutes":{"type":"integer","minimum":5,"maximum":480},"guests":{"type":"array","maxItems":10,"items":{"type":"string","maxLength":254}}}}}}
]}}}}"##
}

/// The flat subset both providers accept: no `oneOf`, `$ref`, limits or `const`; every object closed.
/// Rust validation (`parse`) is the only authority; this only steers the model.
pub fn schema_for(_p: ProviderId) -> Value {
    let step_args = r#"{"type":"object","additionalProperties":false,"properties":{"name":{"type":"string"},"key":{"type":"string","enum":["play_pause","next","previous"]},"url":{"type":"string"},"query":{"type":"string"},"topic":{"type":"string"},"words":{"type":"integer"},"title":{"type":"string"},"date":{"type":"string"},"time":{"type":"string"},"minutes":{"type":"integer"},"guests":{"type":"array","items":{"type":"string"}}}}"#;
    let text = format!(
        r#"{{"type":"object","additionalProperties":false,"required":["v","kind"],"properties":{{"v":{{"type":"integer","enum":[1]}},"kind":{{"type":"string","enum":["plan","question"]}},"question":{{"type":"string"}},"steps":{{"type":"array","minItems":1,"items":{{"type":"object","additionalProperties":false,"required":["action","args"],"properties":{{"action":{{"type":"string","enum":{ACTION_NAMES}}},"args":{step_args}}}}}}}}}}}"#
    );
    serde_json::from_str(&text).unwrap_or(Value::Null)
}

// `Blocked` is used by step 7 (safety) and `schema_json` is the documented full schema (checked by tests);
// neither is called by shipped code yet. This reference keeps them compiled without an allow.
const _: () = {
    let _ = Risk::Blocked;
    let _ = schema_json;
};
