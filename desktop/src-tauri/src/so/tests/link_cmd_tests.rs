// SPDX-License-Identifier: FSL-1.1-MIT
//! Step 6 N1-N14: the command channel on a REAL loopback server (port 0) with the step-5 auth helpers.
//! `Link::request` is blocking, so every call runs on a plain `std::thread` (never inside the runtime).
//! Log checks (outcomes 23, 30, 31, 32 and the no-leak scan) run in child processes.
use super::link_support::*;
use super::rules_support::{captured, in_child, install_capture, run_isolated};
use crate::so::link::commands::LinkState;
use crate::so::link::protocol::*;
use crate::so::link::server::{CmdReply, Link, LinkError};
use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

const TOK: &str = "abababababababababababababababababababababababababababababababab";
const YT_URL: &str = "https://www.youtube.com/results?search_query=Blinding+Lights";
type Reply = Result<CmdReply, LinkError>;

fn yt() -> Command {
    Command::YtPlay { url: YT_URL.to_string() }
}
fn spawn_req(link: &Arc<Link>, cmd: Command, wait: Duration, stop: Arc<AtomicBool>) -> JoinHandle<Reply> {
    let l = link.clone();
    std::thread::spawn(move || l.request(&cmd, wait, &stop))
}
fn spawn_plain(link: &Arc<Link>, cmd: Command, wait: Duration) -> JoinHandle<Reply> {
    spawn_req(link, cmd, wait, Arc::new(AtomicBool::new(false)))
}
async fn join(h: JoinHandle<Reply>) -> Reply {
    tokio::task::spawn_blocking(move || h.join().expect("request panicked")).await.unwrap()
}
async fn connect(rig: &Rig, n: u8) -> Ws {
    let ws = auth_ok(rig.port, TOK, &cnonce(n)).await;
    wait_state(&rig.link, LinkState::Connected).await;
    ws
}
fn res_text(id: u64, r: &str, ms: u64) -> String {
    serde_json::json!({"t":"res","v":1,"id":id,"r":r,"ms":ms}).to_string()
}
/// Reads the cmd the app sent and returns (id, json).
async fn recv_cmd(ws: &mut Ws) -> (u64, serde_json::Value) {
    let m = recv_json(ws).await;
    assert_eq!(m["t"], "cmd", "{m}");
    (m["id"].as_u64().expect("numeric id"), m)
}
async fn nothing_for(ws: &mut Ws, ms: u64) {
    use futures_util::StreamExt;
    if let Ok(m) = tokio::time::timeout(Duration::from_millis(ms), ws.next()).await {
        panic!("expected silence for {ms} ms, got {m:?}");
    }
}

// ---------- protocol shapes (pure) ----------
fn j(s: &str) -> serde_json::Value {
    serde_json::from_str(s).expect("valid JSON")
}
#[test]
fn cmd_and_cancel_json_have_exactly_the_planned_keys() {
    assert_eq!(j(&cmd_json(7, &yt())), serde_json::json!({"t":"cmd","v":1,"id":7,"op":"yt_play","url":YT_URL}));
    assert_eq!(j(&cmd_json(8, &Command::DocsOpen)), serde_json::json!({"t":"cmd","v":1,"id":8,"op":"docs_open"}));
    let cal = "https://calendar.google.com/calendar/render?action=TEMPLATE&text=Review&dates=20261116T093000Z/20261116T103000Z";
    assert_eq!(j(&cmd_json(9, &Command::CalOpen { url: cal.into() })), serde_json::json!({"t":"cmd","v":1,"id":9,"op":"cal_open","url":cal}));
    assert_eq!(j(&cancel_json(7)), serde_json::json!({"t":"cancel","v":1,"id":7}));
    assert_eq!((MAX_CMD_ID, MAX_RESULT_MS), (2_147_483_647, 120_000));
}

const CODES: [(&str, ResultCode); 14] = [
    ("playing", ResultCode::Playing), ("ad", ResultCode::Ad), ("paused", ResultCode::Paused), ("no_video", ResultCode::NoVideo), ("editor_open", ResultCode::EditorOpen),
    ("needs_login", ResultCode::NeedsLogin), ("consent", ResultCode::Consent), ("no_access", ResultCode::NoAccess), ("timeout", ResultCode::Timeout),
    ("bad_cmd", ResultCode::BadCmd), ("busy", ResultCode::Busy), ("unsafe", ResultCode::Unsafe), ("cancelled", ResultCode::Cancelled), ("error", ResultCode::Error),
];

#[test]
fn parse_ext_accepts_a_result_with_each_of_the_14_codes() {
    for (name, code) in CODES {
        let msg = parse_ext(&res_text(5, name, 1234), 4096).unwrap_or_else(|e| panic!("{name} must parse: {e:?}"));
        assert!(matches!(msg.get(), ExtMsg::Result { id: 5, r, ms: 1234 } if *r == code), "{name}");
    }
    for ms in [0u64, 120_000] {
        assert!(parse_ext(&res_text(1, "playing", ms), 4096).is_ok(), "ms {ms}");
    }
    assert!(parse_ext(&res_text(2_147_483_647, "playing", 1), 4096).is_ok());
}

#[test]
fn parse_ext_rejects_every_bad_result_shape() {
    let bad: Vec<(String, &str)> = vec![
        (r#"{"t":"res","v":1,"id":1,"r":"playing","ms":5,"extra":1}"#.into(), "extra key"),
        (r#"{"t":"res","v":1,"id":1,"r":"playing","ms":5,"title":"x"}"#.into(), "page text key"),
        (r#"{"t":"res","v":1,"id":1,"r":"playing"}"#.into(), "no ms"),
        (r#"{"t":"res","v":1,"r":"playing","ms":5}"#.into(), "no id"),
        (r#"{"t":"res","v":1,"id":1,"ms":5}"#.into(), "no r"),
        (r#"{"t":"res","v":2,"id":1,"r":"playing","ms":5}"#.into(), "v 2"),
        (res_text(1, "nope", 5), "unknown r"),
        (res_text(1, "PLAYING", 5), "upper-case r"),
        (res_text(1, "", 5), "empty r"),
        (r#"{"t":"res","v":1,"id":1,"r":5,"ms":5}"#.into(), "r number"),
        (r#"{"t":"res","v":1,"id":0,"r":"playing","ms":5}"#.into(), "id 0"),
        (r#"{"t":"res","v":1,"id":2147483648,"r":"playing","ms":5}"#.into(), "id above max"),
        (r#"{"t":"res","v":1,"id":-1,"r":"playing","ms":5}"#.into(), "id negative"),
        (r#"{"t":"res","v":1,"id":"1","r":"playing","ms":5}"#.into(), "id string"),
        (r#"{"t":"res","v":1,"id":1.5,"r":"playing","ms":5}"#.into(), "id float"),
        (r#"{"t":"res","v":1,"id":1,"r":"playing","ms":120001}"#.into(), "ms above max"),
        (r#"{"t":"res","v":1,"id":1,"r":"playing","ms":-1}"#.into(), "ms negative"),
        (r#"{"t":"res","v":1,"id":1,"r":"playing","ms":1.5}"#.into(), "ms float"),
        (r#"{"t":"res","v":1,"id":1,"r":"playing","ms":"5"}"#.into(), "ms string"),
        (r#"{"t":"res","v":1,"id":1,"r":"playing","ms":null}"#.into(), "ms null"),
    ];
    assert!(parse_ext(&res_text(1, "playing", 5), 4096).is_ok(), "positive control: a valid res parses");
    for (text, why) in bad {
        assert!(parse_ext(&text, 4096).is_err(), "{why} must be refused: {text}");
    }
}

// ---------- N1 ----------
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn n1_no_session_is_not_connected_at_once() {
    let rig = start(Some(TOK)).await;
    let t = Instant::now();
    let r = join(spawn_plain(&rig.link, yt(), Duration::from_secs(5))).await;
    assert_eq!(r, Err(LinkError::NotConnected));
    assert!(t.elapsed() < Duration::from_millis(500), "returned at once, took {:?}", t.elapsed());
}

// ---------- N2 ----------
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn n2_the_cmd_has_the_exact_keys_and_the_reply_comes_back() {
    let rig = start(Some(TOK)).await;
    let mut ws = connect(&rig, 1).await;
    let h = spawn_plain(&rig.link, yt(), Duration::from_secs(5));
    let (id, m) = recv_cmd(&mut ws).await;
    assert_eq!(m, serde_json::json!({"t":"cmd","v":1,"id":id,"op":"yt_play","url":YT_URL}));
    assert!((1..=MAX_CMD_ID as u64).contains(&id));
    send(&mut ws, &res_text(id, "playing", 1234)).await;
    assert_eq!(join(h).await, Ok(CmdReply { r: ResultCode::Playing, ms: 1234 }));
    // docs_open and cal_open shapes, and the id increases
    let h = spawn_plain(&rig.link, Command::DocsOpen, Duration::from_secs(5));
    let (id2, m) = recv_cmd(&mut ws).await;
    assert_eq!(m, serde_json::json!({"t":"cmd","v":1,"id":id2,"op":"docs_open"}));
    assert!(id2 > id, "ids increase: {id} then {id2}");
    send(&mut ws, &res_text(id2, "editor_open", 5)).await;
    assert_eq!(join(h).await, Ok(CmdReply { r: ResultCode::EditorOpen, ms: 5 }));
    let cal = "https://calendar.google.com/calendar/render?action=TEMPLATE&text=Review&dates=20261116T093000Z/20261116T103000Z";
    let h = spawn_plain(&rig.link, Command::CalOpen { url: cal.into() }, Duration::from_secs(5));
    let (id3, m) = recv_cmd(&mut ws).await;
    assert_eq!(m, serde_json::json!({"t":"cmd","v":1,"id":id3,"op":"cal_open","url":cal}));
    assert!(id3 > id2);
    send(&mut ws, &res_text(id3, "needs_login", 0)).await;
    assert_eq!(join(h).await, Ok(CmdReply { r: ResultCode::NeedsLogin, ms: 0 }));
}

// ---------- N3 ----------
const BAD_RESULTS: [&str; 10] = [
    r#"{"t":"res","v":1,"id":ID,"r":"playing","ms":5,"extra":1}"#,
    r#"{"t":"res","v":1,"id":ID,"r":"surprise","ms":5}"#,
    r#"{"t":"res","v":1,"id":0,"r":"playing","ms":5}"#,
    r#"{"t":"res","v":1,"id":2147483648,"r":"playing","ms":5}"#,
    r#"{"t":"res","v":1,"id":"ID","r":"playing","ms":5}"#,
    r#"{"t":"res","v":1,"id":1.5,"r":"playing","ms":5}"#,
    r#"{"t":"res","v":1,"id":ID,"r":"playing","ms":120001}"#,
    r#"{"t":"res","v":1,"id":ID,"r":"playing","ms":-1}"#,
    r#"{"t":"res","v":1,"id":ID,"r":"playing","ms":1.5}"#,
    r#"{"t":"res","v":1,"id":ID,"r":"playing"}"#,
];

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn n3_a_malformed_result_closes_4400_and_the_pending_request_is_lost() {
    let rig = start(Some(TOK)).await;
    for (i, template) in BAD_RESULTS.iter().enumerate() {
        let mut ws = connect(&rig, 10 + i as u8).await;
        let h = spawn_plain(&rig.link, yt(), Duration::from_secs(10));
        let (id, _) = recv_cmd(&mut ws).await;
        send(&mut ws, &template.replace("ID", &id.to_string())).await;
        assert_eq!(expect_close(&mut ws).await, 4400, "{template}");
        assert_eq!(join(h).await, Err(LinkError::Lost), "{template}");
    }
}

#[test]
fn n3_outcome_23_is_logged_for_a_bad_result() {
    run_isolated(module_path!(), "n3_outcome_inner");
}
#[test]
#[ignore]
fn n3_outcome_inner() {
    if !in_child() {
        return;
    }
    install_capture();
    let rt = tokio::runtime::Builder::new_multi_thread().worker_threads(3).enable_all().build().unwrap();
    rt.block_on(async {
        let rig = start(Some(TOK)).await;
        let from = link_records(&captured()).len();
        let mut ws = connect(&rig, 1).await;
        let h = spawn_plain(&rig.link, yt(), Duration::from_secs(10));
        let (id, _) = recv_cmd(&mut ws).await;
        send(&mut ws, &BAD_RESULTS[1].replace("ID", &id.to_string())).await;
        let r = wait_outcome(captured, from, 23);
        assert_eq!(r.1, 4400);
        assert_eq!(join(h).await, Err(LinkError::Lost));
    });
}

// ---------- N4 ----------
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn n4_a_result_for_another_id_is_ignored_and_the_right_one_is_accepted_later() {
    let rig = start(Some(TOK)).await;
    let mut ws = connect(&rig, 1).await;
    let h = spawn_plain(&rig.link, yt(), Duration::from_secs(5));
    let (id, _) = recv_cmd(&mut ws).await;
    send(&mut ws, &res_text(id + 1, "playing", 5)).await;
    send(&mut ws, &res_text(id + 1000, "ad", 5)).await;
    tokio::time::sleep(Duration::from_millis(150)).await;
    assert!(!h.is_finished(), "a wrong id must not answer the pending request");
    send(&mut ws, &res_text(id, "paused", 77)).await;
    assert_eq!(join(h).await, Ok(CmdReply { r: ResultCode::Paused, ms: 77 }));
}

#[test]
fn n4_outcomes_30_31_32_are_logged() {
    run_isolated(module_path!(), "n4_outcomes_inner");
}
#[test]
#[ignore]
fn n4_outcomes_inner() {
    if !in_child() {
        return;
    }
    install_capture();
    let rt = tokio::runtime::Builder::new_multi_thread().worker_threads(3).enable_all().build().unwrap();
    rt.block_on(async {
        let rig = start(Some(TOK)).await;
        let mut ws = connect(&rig, 1).await;
        let from = link_records(&captured()).len();
        let h = spawn_plain(&rig.link, yt(), Duration::from_secs(5));
        let (id, _) = recv_cmd(&mut ws).await;
        wait_outcome(captured, from, 30);
        send(&mut ws, &res_text(id + 1, "playing", 5)).await;
        wait_outcome(captured, from, 31);
        send(&mut ws, &res_text(id, "playing", 5)).await;
        wait_outcome(captured, from, 32);
        assert!(join(h).await.is_ok());
    });
}

// ---------- N5 ----------
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn n5_no_reply_in_time_is_timeout_the_client_gets_cancel_and_a_late_result_is_ignored() {
    let rig = start(Some(TOK)).await;
    let mut ws = connect(&rig, 1).await;
    let t = Instant::now();
    let h = spawn_plain(&rig.link, yt(), Duration::from_millis(300));
    let (id, _) = recv_cmd(&mut ws).await;
    assert_eq!(join(h).await, Err(LinkError::Timeout));
    let took = t.elapsed();
    assert!(took >= Duration::from_millis(250) && took < Duration::from_millis(1500), "{took:?}");
    let c = recv_json(&mut ws).await;
    assert_eq!(c, serde_json::json!({"t":"cancel","v":1,"id":id}));
    // The late result is ignored: the session stays open and still answers a ping.
    send(&mut ws, &res_text(id, "playing", 5)).await;
    send(&mut ws, &ping_text()).await;
    assert_eq!(recv_json(&mut ws).await, serde_json::json!({"t":"pong","v":1}));
    // ... and a new request works.
    let h = spawn_plain(&rig.link, yt(), Duration::from_secs(5));
    let (id2, _) = recv_cmd(&mut ws).await;
    assert_ne!(id2, id);
    send(&mut ws, &res_text(id2, "playing", 9)).await;
    assert!(join(h).await.is_ok());
}

#[test]
fn n5_a_late_result_logs_outcome_31() {
    run_isolated(module_path!(), "n5_late_inner");
}
#[test]
#[ignore]
fn n5_late_inner() {
    if !in_child() {
        return;
    }
    install_capture();
    let rt = tokio::runtime::Builder::new_multi_thread().worker_threads(3).enable_all().build().unwrap();
    rt.block_on(async {
        let rig = start(Some(TOK)).await;
        let mut ws = connect(&rig, 1).await;
        let h = spawn_plain(&rig.link, yt(), Duration::from_millis(200));
        let (id, _) = recv_cmd(&mut ws).await;
        assert_eq!(join(h).await, Err(LinkError::Timeout));
        let from = link_records(&captured()).len();
        send(&mut ws, &res_text(id, "playing", 5)).await;
        wait_outcome(captured, from, 31);
    });
}

// ---------- N6 ----------
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn n6_the_stop_flag_ends_the_wait_within_200ms_and_sends_cancel() {
    let rig = start(Some(TOK)).await;
    let mut ws = connect(&rig, 1).await;
    let stop = Arc::new(AtomicBool::new(false));
    let h = spawn_req(&rig.link, yt(), Duration::from_secs(10), stop.clone());
    let (id, _) = recv_cmd(&mut ws).await;
    let t = Instant::now();
    stop.store(true, Ordering::SeqCst);
    assert_eq!(join(h).await, Err(LinkError::Stopped));
    assert!(t.elapsed() < Duration::from_millis(200), "stopped after {:?}", t.elapsed());
    assert_eq!(recv_json(&mut ws).await, serde_json::json!({"t":"cancel","v":1,"id":id}));
}

// ---------- N7 ----------
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn n7_a_second_request_while_one_is_pending_is_busy_and_sends_nothing() {
    let rig = start(Some(TOK)).await;
    let mut ws = connect(&rig, 1).await;
    let first = spawn_plain(&rig.link, yt(), Duration::from_secs(10));
    let (id, _) = recv_cmd(&mut ws).await;
    let t = Instant::now();
    assert_eq!(join(spawn_plain(&rig.link, Command::DocsOpen, Duration::from_secs(10))).await, Err(LinkError::Busy));
    assert!(t.elapsed() < Duration::from_millis(500));
    nothing_for(&mut ws, 200).await;
    send(&mut ws, &res_text(id, "playing", 5)).await;
    assert!(join(first).await.is_ok());
}

// ---------- N8 ----------
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn n8_a_closed_session_loses_the_pending_request() {
    let rig = start(Some(TOK)).await;
    let mut ws = connect(&rig, 1).await;
    let h = spawn_plain(&rig.link, yt(), Duration::from_secs(10));
    let _ = recv_cmd(&mut ws).await;
    let t = Instant::now();
    drop(ws);
    assert_eq!(join(h).await, Err(LinkError::Lost));
    assert!(t.elapsed() < Duration::from_secs(3), "{:?}", t.elapsed());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn n8_a_replaced_session_4409_loses_the_pending_request() {
    let rig = start(Some(TOK)).await;
    let mut first = connect(&rig, 1).await;
    let h = spawn_plain(&rig.link, yt(), Duration::from_secs(10));
    let _ = recv_cmd(&mut first).await;
    let _second = auth_ok(rig.port, TOK, &cnonce(2)).await;
    assert_eq!(expect_close(&mut first).await, 4409);
    assert_eq!(join(h).await, Err(LinkError::Lost));
}

// ---------- N9 ----------
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn n9_a_result_as_the_first_message_closes_4400() {
    let rig = start(Some(TOK)).await;
    let mut ws = expect_next_101(rig.port).await;
    let _ = hello(&mut ws).await;
    send(&mut ws, &res_text(1, "playing", 5)).await;
    assert_eq!(expect_close(&mut ws).await, 4400);
    let _next = expect_next_101(rig.port).await;
}

// ---------- N10 ----------
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn n10_a_command_over_the_message_limit_is_too_long_and_nothing_is_sent() {
    let rig = start(Some(TOK)).await;
    let mut ws = connect(&rig, 1).await;
    let long = Command::CalOpen { url: format!("https://calendar.google.com/calendar/render?action=TEMPLATE&text={}", "a".repeat(5000)) };
    assert_eq!(join(spawn_plain(&rig.link, long, Duration::from_secs(5))).await, Err(LinkError::TooLong));
    nothing_for(&mut ws, 200).await;
    // The channel is still free: the next request is the first thing the client sees.
    let h = spawn_plain(&rig.link, yt(), Duration::from_secs(5));
    let (id, m) = recv_cmd(&mut ws).await;
    assert_eq!(m["op"], "yt_play");
    send(&mut ws, &res_text(id, "playing", 1)).await;
    assert!(join(h).await.is_ok());
}

// ---------- N11 ----------
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn n11_ids_never_repeat_not_even_across_sessions() {
    let rig = start(Some(TOK)).await;
    let mut ids: Vec<u64> = vec![];
    let mut ws = connect(&rig, 1).await;
    for _ in 0..4 {
        let h = spawn_plain(&rig.link, yt(), Duration::from_secs(5));
        let (id, _) = recv_cmd(&mut ws).await;
        send(&mut ws, &res_text(id, "playing", 1)).await;
        assert!(join(h).await.is_ok());
        ids.push(id);
    }
    drop(ws);
    let mut ws = connect(&rig, 2).await;
    for _ in 0..2 {
        let h = spawn_plain(&rig.link, Command::DocsOpen, Duration::from_secs(5));
        let (id, _) = recv_cmd(&mut ws).await;
        send(&mut ws, &res_text(id, "editor_open", 1)).await;
        assert!(join(h).await.is_ok());
        ids.push(id);
    }
    assert_eq!(ids.iter().collect::<HashSet<_>>().len(), ids.len(), "{ids:?}");
    assert!(ids.windows(2).all(|w| w[0] < w[1]), "increasing: {ids:?}");
    assert!(ids.iter().all(|i| (1..=MAX_CMD_ID as u64).contains(i)));
}

// ---------- N12 ----------
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn n12_a_ping_is_answered_while_a_command_is_pending() {
    let rig = start(Some(TOK)).await;
    let mut ws = connect(&rig, 1).await;
    let h = spawn_plain(&rig.link, yt(), Duration::from_secs(5));
    let (id, _) = recv_cmd(&mut ws).await;
    send(&mut ws, &ping_text()).await;
    assert_eq!(recv_json(&mut ws).await, serde_json::json!({"t":"pong","v":1}));
    assert!(!h.is_finished(), "a ping is not an answer");
    send(&mut ws, &res_text(id, "playing", 5)).await;
    assert!(join(h).await.is_ok());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn n12_a_result_resets_the_idle_timer() {
    // idle 800 ms: without a reset the session would end about 800 ms after auth.
    let rig = start_with(test_cfg(500, 800), TestStore::new(Some(TOK))).await;
    let mut ws = connect(&rig, 1).await;
    tokio::time::sleep(Duration::from_millis(500)).await;
    let h = spawn_plain(&rig.link, yt(), Duration::from_secs(5));
    let (id, _) = recv_cmd(&mut ws).await;
    send(&mut ws, &res_text(id, "playing", 5)).await;
    assert!(join(h).await.is_ok());
    tokio::time::sleep(Duration::from_millis(500)).await; // t = 1000 ms: past the first deadline
    send(&mut ws, &ping_text()).await;
    assert_eq!(recv_json(&mut ws).await, serde_json::json!({"t":"pong","v":1}), "the session must still be open");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn n12_an_outgoing_command_does_not_reset_the_idle_timer() {
    // idle 800 ms, the client says nothing: sending a cmd at 500 ms must not push the end past ~800 ms.
    let rig = start_with(test_cfg(500, 800), TestStore::new(Some(TOK))).await;
    let mut ws = connect(&rig, 1).await;
    tokio::time::sleep(Duration::from_millis(400)).await;
    let h = spawn_plain(&rig.link, yt(), Duration::from_secs(5));
    let _ = recv_cmd(&mut ws).await;
    let t = Instant::now();
    assert_eq!(join(h).await, Err(LinkError::Lost), "the idle session ends and the request is lost");
    assert!(t.elapsed() < Duration::from_millis(700), "closed {:?} after the cmd: an outgoing cmd must not reset the idle timer", t.elapsed());
}

// ---------- N13 ----------
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn n13_a_pending_request_does_not_block_the_server() {
    let rig = start(Some(TOK)).await;
    let mut ws = connect(&rig, 1).await;
    let h = spawn_plain(&rig.link, yt(), Duration::from_secs(3));
    let (id, _) = recv_cmd(&mut ws).await;
    let t = Instant::now();
    let mut other = expect_next_101(rig.port).await;
    let _ = hello(&mut other).await;
    assert!(t.elapsed() < Duration::from_millis(1500), "another handshake took {:?} while a request was pending", t.elapsed());
    send(&mut ws, &res_text(id, "playing", 5)).await;
    assert!(join(h).await.is_ok());
}

// ---------- N14: no URL / query in the log (child process) ----------
#[test]
fn n14_no_url_or_query_reaches_the_log_text_or_hex() {
    run_isolated(module_path!(), "n14_inner");
}
#[test]
#[ignore]
fn n14_inner() {
    if !in_child() {
        return;
    }
    install_capture();
    let (query, title) = ("kumquatsentinel", "walrussentinel");
    let secrets = vec![query.to_string(), title.to_string()];
    log::info!(target: "positive_control", "oops {query}");
    assert_eq!(leaked(&captured(), &secrets).len(), 1, "the scanner cannot fail");
    let baseline = captured().len();
    let rt = tokio::runtime::Builder::new_multi_thread().worker_threads(3).enable_all().build().unwrap();
    rt.block_on(async {
        let rig = start(Some(TOK)).await;
        let mut ws = connect(&rig, 1).await;
        let url = format!("https://www.youtube.com/results?search_query={query}");
        let h = spawn_plain(&rig.link, Command::YtPlay { url }, Duration::from_secs(5));
        let (id, _) = recv_cmd(&mut ws).await;
        send(&mut ws, &res_text(id, "playing", 5)).await;
        assert!(join(h).await.is_ok());
        let cal = format!("https://calendar.google.com/calendar/render?action=TEMPLATE&text={title}&dates=20261116T093000Z/20261116T103000Z");
        let h = spawn_plain(&rig.link, Command::CalOpen { url: cal }, Duration::from_millis(200));
        let _ = recv_cmd(&mut ws).await;
        assert_eq!(join(h).await, Err(LinkError::Timeout));
        send(&mut ws, &BAD_RESULTS[1].replace("ID", "1")).await;
    });
    std::thread::sleep(Duration::from_millis(200));
    // tungstenite itself logs frame text at Trace in this process (the app filters it, see step 5 W14b): scan ours.
    let after: Vec<String> = captured().into_iter().skip(baseline).filter(|r| r.contains("::so::")).collect();
    assert_eq!(leaked(&after, &secrets), Vec::<String>::new(), "a URL or query reached our log (text or ASCII hex)");
    assert!(!link_records(&after).is_empty(), "positive control: link records exist");
}
