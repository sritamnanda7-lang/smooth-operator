// SPDX-License-Identifier: FSL-1.1-MIT
//! T24 (payload) and T25 (TalkEvent -> fixed strings).
use crate::so::hotkey::session::{Mode, TalkEvent};
use crate::so::panel::{action_for, Kind, Line, PanelAction, PanelPayload, MAX_CHARS, MAX_LINES};
use crate::so::settings::HotkeyChoice;
use std::time::Duration;

#[test]
fn t24_at_most_6_lines() {
    let lines: Vec<Line> = (0..10).map(|i| Line::new(format!("line {i}"), Kind::Info)).collect();
    let p = PanelPayload::new(lines);
    assert_eq!(p.v, 1);
    assert_eq!(p.lines.len(), MAX_LINES);
    assert_eq!(MAX_LINES, 6);
}

#[test]
fn t24_at_most_200_chars_per_line_counted_in_chars() {
    let long_ascii = "a".repeat(500);
    let long_multibyte = "é".repeat(500); // 2 bytes each
    let emoji = "😀".repeat(300); // 4 bytes each, must not be cut mid-char
    let p = PanelPayload::new(vec![
        Line::new(long_ascii, Kind::Info),
        Line::new(long_multibyte, Kind::Warn),
        Line::new(emoji, Kind::Listening),
        Line::new("short", Kind::Info),
    ]);
    assert_eq!(MAX_CHARS, 200);
    for l in &p.lines[..3] {
        assert_eq!(l.text.chars().count(), 200, "cut to exactly 200 chars");
    }
    assert_eq!(p.lines[3].text, "short");
}

/// Pull the JS string literal out of `window.soPanel.set(JSON.parse(<literal>))`.
fn literal_of(script: &str) -> &str {
    let s = script.trim_end_matches(';');
    let inner = s
        .strip_prefix("window.soPanel.set(JSON.parse(")
        .and_then(|r| r.strip_suffix("))"))
        .unwrap_or_else(|| panic!("unexpected eval shape: {script}"));
    inner
}

#[test]
fn t24_eval_literal_is_double_encoded_and_decodes_exactly() {
    let nasty = [
        "say \"hi\"",
        "back\\slash and \\\" mix",
        "</script><script>alert(1)</script>",
        "line sep \u{2028} para sep \u{2029} end",
        "new\nline\ttab",
        "\u{0000}nul",
        "'single' `tick` ${x}",
    ];
    for text in nasty {
        let payload = PanelPayload::new(vec![Line::new(text, Kind::Info), Line::new(text, Kind::Warn)]);
        let script = payload.eval_script();
        let lit = literal_of(&script);
        assert!(lit.starts_with('"') && lit.ends_with('"'), "literal must be a quoted string: {lit}");
        assert!(!lit.contains('\n') && !lit.contains('\r'), "no raw newline in the literal");
        // Layer 1: JS string literal -> JSON text. Layer 2: JSON text -> payload.
        let json_text: String = serde_json::from_str(lit).expect("literal is a JSON string");
        let back: PanelPayload = serde_json::from_str(&json_text).expect("inner is payload JSON");
        assert_eq!(back, payload, "round trip for {text:?}");
        assert_eq!(back.lines[0].text, text);
    }
}

#[test]
fn t24_wire_format_is_v1_lines_kind_lowercase() {
    let p = PanelPayload::new(vec![Line::new("x", Kind::Listening)]);
    let v: serde_json::Value = serde_json::to_value(&p).unwrap();
    assert_eq!(v, serde_json::json!({"v":1,"lines":[{"text":"x","kind":"listening"}]}));
}

#[test]
fn t24_exactly_one_script_shape_no_raw_payload_in_script() {
    let p = PanelPayload::new(vec![Line::new("</script>", Kind::Info)]);
    let s = p.eval_script();
    assert!(!s.contains("</script>"), "payload text must only appear escaped inside the literal");
    assert!(s.starts_with("window.soPanel.set(JSON.parse("));
}

fn one(a: PanelAction) -> (PanelPayload, Option<Duration>) {
    match a {
        PanelAction::Show(p) => (p, None),
        PanelAction::ShowThenHide(p, d) => (p, Some(d)),
        PanelAction::Hide => panic!("expected text, got Hide"),
    }
}

#[test]
fn t25_started_push_to_talk() {
    let (p, hide) = one(action_for(TalkEvent::Started(Mode::PushToTalk), HotkeyChoice::Fn));
    assert_eq!(hide, None);
    assert_eq!(p.lines, vec![Line::new("Listening", Kind::Listening)]);
}

#[test]
fn t25_started_free_talk_names_the_key() {
    for (hk, label) in [
        (HotkeyChoice::Fn, "Fn"),
        (HotkeyChoice::RightCmd, "Right Cmd"),
        (HotkeyChoice::RightCtrl, "Right Ctrl"),
    ] {
        let (p, hide) = one(action_for(TalkEvent::Started(Mode::FreeTalk), hk));
        assert_eq!(hide, None);
        assert_eq!(p.lines, vec![Line::new(format!("Free talk: press {label} to stop"), Kind::Listening)]);
    }
}

#[test]
fn t25_stopped_shows_then_hides_after_3s() {
    for m in [Mode::PushToTalk, Mode::FreeTalk] {
        let (p, hide) = one(action_for(TalkEvent::Stopped(m), HotkeyChoice::Fn));
        assert_eq!(hide, Some(Duration::from_secs(3)));
        assert_eq!(p.lines, vec![Line::new("Stopped", Kind::Info)]);
    }
}

#[test]
fn t25_cancelled_hides_at_once_with_no_text() {
    for m in [Mode::PushToTalk, Mode::FreeTalk] {
        assert_eq!(action_for(TalkEvent::Cancelled(m), HotkeyChoice::Fn), PanelAction::Hide);
    }
}
