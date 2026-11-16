// SPDX-License-Identifier: FSL-1.1-MIT
//! Builds the command for an op: URLs in Rust (percent-encoded by the `url` crate), calendar time in UTC.

use super::{Res, MAX_DAYS_AHEAD};
use crate::so::brain::plan::{Action, OpKind, WEEKDAYS};
use crate::so::link::protocol::Command;
use chrono::{DateTime, Datelike, Duration, LocalResult, NaiveDate, NaiveDateTime, NaiveTime, Utc};
use tauri::Url;

const YT_BASE: &str = "https://www.youtube.com/results";
const CAL_BASE: &str = "https://calendar.google.com/calendar/render";

/// Injected zone/time so tests can script a zone and DST.
pub trait WallClock: Send + Sync {
    fn now_local(&self) -> NaiveDateTime;
    fn to_utc(&self, local: NaiveDateTime) -> LocalResult<DateTime<Utc>>;
}

/// `Url::parse_with_params` percent-encodes every value (space becomes `+`), so text can never add a key.
fn with_params(base: &str, params: &[(&str, &str)]) -> Option<String> {
    Url::parse_with_params(base, params).ok().map(String::from)
}

pub fn yt_url(query: &str) -> String {
    // Failure is impossible (constant base); the bare base would be rejected by the extension anyway.
    with_params(YT_BASE, &[("search_query", query)]).unwrap_or_else(|| YT_BASE.to_string())
}

/// Errors: PastTime, NoSuchTime, TooFar (Failed for a date/time the parser should have refused).
pub fn cal_url(title: &str, date: &str, time: &str, minutes: Option<u16>, wall: &dyn WallClock) -> Result<String, Res> {
    let now = wall.now_local();
    let time = NaiveTime::parse_from_str(time, "%H:%M").map_err(|_| Res::Failed)?;
    let day = resolve_date(date, now, time).ok_or(Res::Failed)?;
    let start_local = day.and_time(time);
    let now_utc = match wall.to_utc(now) {
        LocalResult::None => return Err(Res::Failed),
        LocalResult::Single(t) => t,
        LocalResult::Ambiguous(a, b) => a.max(b), // ambiguous now: the later instant, so a start is never wrongly "future"
    };
    let start = match wall.to_utc(start_local) {
        LocalResult::None => return Err(Res::NoSuchTime),
        LocalResult::Single(t) => t,
        LocalResult::Ambiguous(a, b) => a.min(b),
    };
    if start <= now_utc {
        return Err(Res::PastTime);
    }
    if start > now_utc.checked_add_signed(Duration::days(MAX_DAYS_AHEAD)).ok_or(Res::Failed)? {
        return Err(Res::TooFar);
    }
    if !(5..=480).contains(&minutes.unwrap_or(60)) {
        return Err(Res::Failed);
    }
    // End = start instant + minutes, so a clock change in between cannot stretch the event.
    let end = start.checked_add_signed(Duration::minutes(i64::from(minutes.unwrap_or(60)))).ok_or(Res::Failed)?;
    let stamp = |t: DateTime<Utc>| t.format("%Y%m%dT%H%M%SZ").to_string();
    let dates = format!("{}/{}", stamp(start), stamp(end));
    with_params(CAL_BASE, &[("action", "TEMPLATE"), ("text", title), ("dates", &dates)]).ok_or(Res::Failed)
}

/// today / tomorrow / weekday (next occurrence; same weekday = today if still ahead) / YYYY-MM-DD.
fn resolve_date(date: &str, now: NaiveDateTime, time: NaiveTime) -> Option<NaiveDate> {
    let lower = date.to_ascii_lowercase();
    let today = now.date();
    match lower.as_str() {
        "today" => Some(today),
        "tomorrow" => today.succ_opt(),
        _ => match WEEKDAYS.iter().position(|w| *w == lower) {
            Some(want) => {
                let cur = today.weekday().num_days_from_monday() as usize;
                let mut ahead = (want + 7 - cur) % 7;
                if ahead == 0 && today.and_time(time) <= now {
                    ahead = 7;
                }
                today.checked_add_signed(Duration::days(ahead as i64))
            }
            None => NaiveDate::parse_from_str(date, "%Y-%m-%d").ok(),
        },
    }
}

pub fn command_for(op: OpKind, a: &Action, wall: &dyn WallClock) -> Result<Command, Res> {
    match (op, a) {
        (OpKind::YoutubePlay, Action::YoutubePlay { query }) => Ok(Command::YtPlay { url: yt_url(query) }),
        (OpKind::DocsOpen, Action::DocsNew { .. }) => Ok(Command::DocsOpen),
        (OpKind::CalendarOpen, Action::CalendarNewEvent { title, date, time, minutes, .. }) => {
            cal_url(title, date, time, *minutes, wall).map(|url| Command::CalOpen { url })
        }
        _ => Err(Res::Failed),
    }
}
