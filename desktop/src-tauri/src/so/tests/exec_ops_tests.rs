// SPDX-License-Identifier: FSL-1.1-MIT
//! Step 6 A1-A9: the URL and time builders (`exec::ops`). Pure: a scripted zone (`FakeWall`), no clock, no
//! OS. The calendar URL is compared by its decoded query pairs (the `/` in `dates` may be sent raw or as
//! `%2F`; the extension reads it decoded either way).
use super::exec_support::*;
use crate::so::brain::plan::Action;
use crate::so::exec::ops::{cal_url, command_for, yt_url};
use crate::so::exec::Res;
use crate::so::brain::plan::OpKind;
use crate::so::link::protocol::{cmd_json, Command};
use tauri::Url;

const CAL_PREFIX: &str = "https://calendar.google.com/calendar/render?action=TEMPLATE&text=";

fn pairs(u: &str) -> Vec<(String, String)> {
    Url::parse(u).unwrap_or_else(|e| panic!("not a URL: {u}: {e}")).query_pairs().map(|(k, v)| (k.into_owned(), v.into_owned())).collect()
}
fn keys(u: &str) -> Vec<String> {
    pairs(u).into_iter().map(|p| p.0).collect()
}
fn val(u: &str, k: &str) -> String {
    pairs(u).into_iter().find(|p| p.0 == k).map(|p| p.1).unwrap_or_else(|| panic!("no key {k} in {u}"))
}
/// (start, end) of `dates` as the 16-char "YYYYMMDDTHHMMSSZ" texts.
fn dates(u: &str) -> (String, String) {
    let d = val(u, "dates");
    let (s, e) = d.split_once('/').unwrap_or_else(|| panic!("dates has no slash: {d}"));
    (s.to_string(), e.to_string())
}
fn ok_url(r: Result<String, Res>) -> String {
    r.unwrap_or_else(|e| panic!("expected a URL, got {e:?}"))
}
fn secs(stamp: &str) -> i64 {
    chrono::NaiveDateTime::parse_from_str(stamp, "%Y%m%dT%H%M%SZ").unwrap_or_else(|e| panic!("bad stamp {stamp}: {e}")).and_utc().timestamp()
}

// ---------- A1 ----------
#[test]
fn a1_yt_url_is_exactly_the_results_url_with_plus_for_spaces() {
    assert_eq!(yt_url("Blinding Lights"), "https://www.youtube.com/results?search_query=Blinding+Lights");
}

#[test]
fn a1_hostile_and_non_ascii_text_stays_one_value_of_the_one_key() {
    for q in ["a&b=c#d%25/?x", "Beyoncé — Halo", "x\"y\\z", "100% = <b>&amp;</b>", "ünï cödé ♪ 𝄞"] {
        let u = yt_url(q);
        assert!(u.starts_with("https://www.youtube.com/results?search_query="), "{u}");
        assert!(!u.contains('#'), "no fragment can survive: {u}");
        assert_eq!(keys(&u), vec!["search_query"], "exactly one key for {q:?}: {u}");
        assert_eq!(val(&u, "search_query"), q, "the value round-trips for {q:?}");
    }
}

// ---------- A2 ----------
#[test]
fn a2_tomorrow_3pm_in_utc_plus_0530_is_0930z_to_1030z_with_exactly_three_keys() {
    let wall = FakeWall::india();
    let u = ok_url(cal_url("Review", "tomorrow", "15:00", None, wall.as_ref()));
    assert!(u.starts_with(CAL_PREFIX), "{u}");
    assert!(u.contains("&dates="), "{u}");
    assert_eq!(keys(&u), vec!["action", "text", "dates"]);
    assert_eq!((val(&u, "action"), val(&u, "text")), ("TEMPLATE".to_string(), "Review".to_string()));
    assert_eq!(dates(&u), ("20261116T093000Z".to_string(), "20261116T103000Z".to_string()));
    let lower = u.to_lowercase();
    for banned in ["add=", "ctz=", "authuser="] {
        assert!(!lower.contains(banned), "{banned} must never be in the URL: {u}");
    }
    let parsed = Url::parse(&u).unwrap();
    assert_eq!((parsed.scheme(), parsed.host_str(), parsed.path(), parsed.fragment()), ("https", Some("calendar.google.com"), "/calendar/render", None));
}

// ---------- A3 ----------
#[test]
fn a3_a_title_that_tries_to_add_a_guest_or_dates_stays_inside_text() {
    let title = "x&add=evil@x.com&dates=1/2#f";
    let u = ok_url(cal_url(title, "tomorrow", "15:00", None, FakeWall::india().as_ref()));
    assert_eq!(keys(&u), vec!["action", "text", "dates"], "{u}");
    assert_eq!(val(&u, "text"), title);
    assert_eq!(dates(&u), ("20261116T093000Z".to_string(), "20261116T103000Z".to_string()), "the real dates survive");
    assert!(!u.contains('#'), "{u}");
}

// ---------- A4 ----------
fn day(wall: &FakeWall, date: &str, time: &str) -> String {
    let u = ok_url(cal_url("T", date, time, None, wall));
    dates(&u).0
}
#[test]
fn a4_today_tomorrow_and_iso_dates() {
    let w = FakeWall::at(dt(2026, 11, 15, 10, 0), 0);
    assert_eq!(day(&w, "today", "15:00"), "20261115T150000Z");
    assert_eq!(day(&w, "tomorrow", "09:00"), "20261116T090000Z");
    assert_eq!(day(&w, "2026-12-24", "18:30"), "20261224T183000Z");
    assert_eq!(day(&w, "TOMORROW", "09:00"), "20261116T090000Z", "case is not significant");
}

#[test]
fn a4_each_weekday_is_its_next_occurrence_sunday_now() {
    // 2026-11-15 is a Sunday; 10:00 now.
    let w = FakeWall::at(dt(2026, 11, 15, 10, 0), 0);
    for (name, date) in [("monday", "20261116"), ("tuesday", "20261117"), ("wednesday", "20261118"), ("thursday", "20261119"), ("friday", "20261120"), ("saturday", "20261121")] {
        assert_eq!(day(&w, name, "12:00"), format!("{date}T120000Z"), "{name}");
    }
    assert_eq!(day(&w, "Friday", "12:00"), "20261120T120000Z", "capitalised weekday");
}

#[test]
fn a4_the_same_weekday_is_today_if_still_ahead_else_seven_days_on() {
    let w = FakeWall::at(dt(2026, 11, 15, 10, 0), 0);
    assert_eq!(day(&w, "sunday", "11:00"), "20261115T110000Z", "later today");
    assert_eq!(day(&w, "sunday", "09:00"), "20261122T090000Z", "already past today: next week");
}

#[test]
fn a4_month_year_and_leap_day_rollovers() {
    let w = FakeWall::at(dt(2026, 12, 31, 10, 0), 0);
    assert_eq!(day(&w, "tomorrow", "09:00"), "20270101T090000Z");
    let leap = FakeWall::at(dt(2028, 2, 28, 10, 0), 0);
    assert_eq!(day(&leap, "tomorrow", "09:00"), "20280229T090000Z");
    let after = FakeWall::at(dt(2028, 2, 29, 10, 0), 0);
    assert_eq!(day(&after, "tomorrow", "09:00"), "20280301T090000Z");
}

// ---------- A5 ----------
fn us_dst() -> std::sync::Arc<FakeWall> {
    // US Eastern 2026: spring gap 2026-03-08 02:30 does not exist; autumn 2026-11-01 01:30 happens twice
    // (EDT -4h, then EST -5h).
    FakeWall::with_dst(dt(2026, 3, 7, 10, 0), -18_000, vec![dt(2026, 3, 8, 2, 30)], vec![(dt(2026, 11, 1, 1, 30), -14_400, -18_000)])
}
#[test]
fn a5_a_time_inside_the_spring_gap_is_no_such_time() {
    let w = us_dst();
    assert_eq!(cal_url("T", "2026-03-08", "02:30", None, w.as_ref()), Err(Res::NoSuchTime));
    assert!(cal_url("T", "2026-03-08", "03:30", None, w.as_ref()).is_ok(), "a time after the gap is fine");
}

#[test]
fn a5_an_autumn_overlap_takes_the_earlier_instant() {
    let w = us_dst();
    *w.now.lock().unwrap() = dt(2026, 10, 31, 10, 0);
    let u = ok_url(cal_url("T", "2026-11-01", "01:30", None, w.as_ref()));
    assert_eq!(dates(&u).0, "20261101T053000Z", "01:30 EDT = 05:30Z (the earlier of 05:30Z and 06:30Z)");
}

#[test]
fn a5_sixty_minutes_across_the_change_is_3600_seconds_not_two_hours() {
    let w = us_dst();
    *w.now.lock().unwrap() = dt(2026, 10, 31, 10, 0);
    let u = ok_url(cal_url("T", "2026-11-01", "01:30", Some(60), w.as_ref()));
    let (s, e) = dates(&u);
    assert_eq!(secs(&e) - secs(&s), 3600, "end = start instant + minutes, not a second local conversion: {s} {e}");
}

// ---------- A6 ----------
#[test]
fn a6_a_start_at_or_before_now_is_past_time() {
    let w = FakeWall::at(dt(2026, 11, 15, 10, 0), 0);
    assert_eq!(cal_url("T", "today", "09:00", None, w.as_ref()), Err(Res::PastTime));
    assert_eq!(cal_url("T", "today", "10:00", None, w.as_ref()), Err(Res::PastTime), "start == now is past");
    assert_eq!(cal_url("T", "2026-11-14", "15:00", None, w.as_ref()), Err(Res::PastTime));
    assert!(cal_url("T", "today", "10:01", None, w.as_ref()).is_ok(), "one minute ahead is fine");
}

// ---------- A7 ----------
#[test]
fn a7_minutes_default_is_sixty_480_is_eight_hours_and_midnight_rolls_the_date() {
    let w = FakeWall::at(dt(2026, 11, 15, 10, 0), 0);
    let d = |m| dates(&ok_url(cal_url("T", "tomorrow", "09:00", m, w.as_ref())));
    assert_eq!(d(None), ("20261116T090000Z".to_string(), "20261116T100000Z".to_string()));
    assert_eq!(d(Some(480)), ("20261116T090000Z".to_string(), "20261116T170000Z".to_string()));
    assert_eq!(d(Some(5)), ("20261116T090000Z".to_string(), "20261116T090500Z".to_string()));
    let late = dates(&ok_url(cal_url("T", "tomorrow", "23:30", Some(60), w.as_ref())));
    assert_eq!(late, ("20261116T233000Z".to_string(), "20261117T003000Z".to_string()), "next date after midnight");
}

// ---------- A8 ----------
#[test]
fn a8_a_100_code_point_4_byte_title_fits_in_one_message() {
    let title = "𝄞".repeat(100);
    assert_eq!(title.chars().count(), 100);
    assert_eq!(title.len(), 400);
    let url = ok_url(cal_url(&title, "tomorrow", "15:00", None, FakeWall::india().as_ref()));
    assert_eq!(val(&url, "text"), title);
    let json = cmd_json(7, &Command::CalOpen { url });
    assert!(json.len() <= 4096, "{} bytes", json.len());
}

#[test]
fn a8_quotes_and_backslashes_round_trip_through_cmd_json() {
    let title = "He said \"hi\" \\ back";
    let url = ok_url(cal_url(title, "tomorrow", "15:00", None, FakeWall::india().as_ref()));
    let v: serde_json::Value = serde_json::from_str(&cmd_json(9, &Command::CalOpen { url })).expect("cmd_json is valid JSON");
    assert_eq!(v["op"], "cal_open");
    assert_eq!(val(v["url"].as_str().unwrap(), "text"), title);
    let yq = yt_url("say \"x\" \\ y");
    let v: serde_json::Value = serde_json::from_str(&cmd_json(10, &Command::YtPlay { url: yq })).unwrap();
    assert_eq!(val(v["url"].as_str().unwrap(), "search_query"), "say \"x\" \\ y");
}

// ---------- A9 ----------
#[test]
fn a9_400_days_ahead_is_fine_one_minute_more_is_too_far() {
    let w = FakeWall::at(dt(2026, 11, 15, 10, 0), 0);
    assert!(cal_url("T", "2027-12-20", "10:00", None, w.as_ref()).is_ok(), "now + 400 days exactly");
    assert_eq!(cal_url("T", "2027-12-20", "10:01", None, w.as_ref()), Err(Res::TooFar), "now + 400 days + 1 minute");
    assert_eq!(cal_url("T", "2030-01-01", "10:00", None, w.as_ref()), Err(Res::TooFar));
}

// ---------- instants, not wall clock (review of increment 1) ----------
#[test]
fn a9_the_400_day_limit_is_in_real_time_across_a_dst_change() {
    // US Eastern: EDT (-4h) until 2026-11-01 02:00, then EST (-5h). now = 2026-10-20 10:00 EDT = 14:00Z.
    // now + 400 days = 2027-11-24 14:00Z = 09:00 EST. Wall-clock gap to 10:00 that day is exactly 400 days, real gap 400 d + 1 h.
    let w = FakeWall::with_switch(dt(2026, 10, 20, 10, 0), -14_400, dt(2026, 11, 1, 2, 0), -18_000);
    assert!(cal_url("T", "2027-11-24", "08:59", None, w.as_ref()).is_ok(), "one minute before the limit");
    assert!(cal_url("T", "2027-11-24", "09:00", None, w.as_ref()).is_ok(), "exactly now + 400 days of real time");
    assert_eq!(cal_url("T", "2027-11-24", "09:01", None, w.as_ref()), Err(Res::TooFar), "one minute past the limit");
    assert_eq!(cal_url("T", "2027-11-24", "10:00", None, w.as_ref()), Err(Res::TooFar), "wall-clock 400 days is 400 d + 1 h of real time");
}

#[test]
fn a6_past_is_checked_on_instants_with_an_ambiguous_now_taken_as_the_later_instant() {
    // 2026-11-01 01:15 and 01:30 each happen twice (EDT -4h, then EST -5h). now = 01:15 (taken as the LATER one = 06:15Z).
    let overlaps = vec![(dt(2026, 11, 1, 1, 15), -14_400, -18_000), (dt(2026, 11, 1, 1, 30), -14_400, -18_000)];
    let w = FakeWall::with_dst(dt(2026, 11, 1, 1, 15), -18_000, vec![], overlaps.clone());
    // 'today 01:30' resolves to the earlier instant 05:30Z, which is before now (06:15Z): already past.
    assert_eq!(cal_url("T", "today", "01:30", None, w.as_ref()), Err(Res::PastTime));
    // positive control: an unambiguous later time is fine, and so is 01:30 when now is before the overlap.
    assert!(cal_url("T", "today", "03:00", None, w.as_ref()).is_ok());
    let early = FakeWall::with_dst(dt(2026, 11, 1, 0, 30), -14_400, vec![], overlaps);
    assert!(cal_url("T", "today", "01:30", None, early.as_ref()).is_ok());
}

#[test]
fn a7_minutes_outside_5_to_480_fail() {
    let w = FakeWall::at(dt(2026, 11, 15, 10, 0), 0);
    for m in [0u16, 1, 4, 481, 1000, u16::MAX] {
        assert_eq!(cal_url("T", "tomorrow", "09:00", Some(m), w.as_ref()), Err(Res::Failed), "{m}");
    }
    for m in [5u16, 480] {
        assert!(cal_url("T", "tomorrow", "09:00", Some(m), w.as_ref()).is_ok(), "{m}");
    }
}

// ---------- command_for ties the pieces together ----------
#[test]
fn command_for_builds_the_three_commands_and_passes_time_errors_up() {
    let w = FakeWall::india();
    let a = Action::YoutubePlay { query: "Blinding Lights".into() };
    assert_eq!(command_for(OpKind::YoutubePlay, &a, w.as_ref()), Ok(Command::YtPlay { url: yt_url("Blinding Lights") }));
    let d = Action::DocsNew { topic: "WWII".into(), words: Some(300) };
    assert_eq!(command_for(OpKind::DocsOpen, &d, w.as_ref()), Ok(Command::DocsOpen), "no topic or text ever travels with docs_open");
    let c = Action::CalendarNewEvent { title: "Review".into(), date: "tomorrow".into(), time: "15:00".into(), minutes: None, guests: vec![] };
    match command_for(OpKind::CalendarOpen, &c, w.as_ref()) {
        Ok(Command::CalOpen { url }) => assert_eq!(url, ok_url(cal_url("Review", "tomorrow", "15:00", None, w.as_ref()))),
        other => panic!("expected CalOpen, got {other:?}"),
    }
    let past = Action::CalendarNewEvent { title: "x".into(), date: "2026-11-14".into(), time: "09:00".into(), minutes: None, guests: vec![] };
    assert_eq!(command_for(OpKind::CalendarOpen, &past, w.as_ref()), Err(Res::PastTime));
}

// ---------- A10: exact outputs, shared with the extension tests (round trip) ----------
/// Hostile and international inputs. The fixture `tests/so/fixtures/url-roundtrip.json` holds the exact URLs
/// Rust built for them; `cmd.test.ts` feeds the same strings to the extension's checks.
fn roundtrip_inputs() -> Vec<String> {
    vec![
        "Blinding Lights".into(),
        "a&b=c#d%25/?x".into(),
        "100% + ~!*'() \"quoted\" \\ back".into(),
        "𝄞".repeat(100),
        "漢字かな交じり文 한국어".into(),
        "👨‍👩‍👧‍👦 family".into(),
        "line\u{2028}sep".into(),
        "nb\u{a0}space".into(),
        " leading space".into(),
        "x&add=evil@x.com&dates=1/2#f".into(),
        "Beyoncé — Halo".into(),
    ]
}
fn fixture_path() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/so/fixtures/url-roundtrip.json")
}
fn build_fixture() -> serde_json::Value {
    let wall = FakeWall::india();
    let items: Vec<serde_json::Value> = roundtrip_inputs()
        .iter()
        .map(|t| {
            let cal = cal_url(t, "tomorrow", "15:00", None, wall.as_ref());
            serde_json::json!({"input": t, "yt": yt_url(t), "cal": cal.ok()})
        })
        .collect();
    serde_json::json!({"note": "generated by exec_ops_tests::a10_generate_roundtrip_fixture; wall = UTC+05:30, now 2026-11-15 10:00, tomorrow 15:00", "now_utc_ms": 1_794_717_000_000u64, "items": items})
}
#[test]
#[ignore]
fn a10_generate_roundtrip_fixture() {
    let path = fixture_path();
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, serde_json::to_string_pretty(&build_fixture()).unwrap() + "\n").unwrap();
}
#[test]
fn a10_rust_output_matches_the_shared_fixture_exactly() {
    let text = std::fs::read_to_string(fixture_path()).expect("tests/so/fixtures/url-roundtrip.json (run a10_generate_roundtrip_fixture --ignored)");
    let want: serde_json::Value = serde_json::from_str(&text).unwrap();
    assert_eq!(want["items"], build_fixture()["items"], "Rust URL building drifted from the fixture the extension tests use");
    for it in want["items"].as_array().unwrap() {
        let input = it["input"].as_str().unwrap();
        assert_eq!(val(it["yt"].as_str().unwrap(), "search_query"), input, "yt round trip");
        assert_eq!(val(it["cal"].as_str().expect("every input builds a calendar URL"), "text"), input, "cal round trip");
    }
}

// ---------- TooFar must not loosen by the DST overlap hour (ops.rs nit) ----------
#[test]
fn a9_toofar_uses_the_earlier_instant_of_an_ambiguous_now() {
    // Rule: an ambiguous `now` counts as its EARLIER instant for the 400-day limit (tighter), so anything accepted
    // here is also inside the extension's real-instant window whichever occurrence of the hour it really is.
    // (PastTime keeps the LATER instant: see a6_past_is_checked_on_instants...)
    // now = 2026-11-01 01:15, happening twice: EDT (-4h) = 05:15Z, EST (-5h) = 06:15Z. 2027-12-06 is EST (-5h).
    let w = FakeWall::with_dst(dt(2026, 11, 1, 1, 15), -18_000, vec![], vec![(dt(2026, 11, 1, 1, 15), -14_400, -18_000)]);
    // earlier now + 400 days = 2027-12-06 05:15Z = 00:15 EST.
    assert!(cal_url("T", "2027-12-06", "00:15", None, w.as_ref()).is_ok(), "exactly the tighter limit");
    assert_eq!(cal_url("T", "2027-12-06", "00:16", None, w.as_ref()), Err(Res::TooFar), "one minute past the tighter limit");
    assert_eq!(cal_url("T", "2027-12-06", "01:15", None, w.as_ref()), Err(Res::TooFar), "the later-instant limit (06:15Z) must not be used");
}
