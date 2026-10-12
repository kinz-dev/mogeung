//! Sends: a seat's message to a teammate, the hop budget, the cap, the
//! pause, wakes, watchdogs, and the log. `R-P6`, feature 0051, ADR-0003's
//! 2026-10-09 amendment, fence 4.
//!
//! What is pinned: a seat reaches a seat of its own team through the
//! receiver and the text lands in that seat's pane; the fourth hop in a chain
//! is held, ranks the seat it was for as needing you, and a release delivers
//! it and starts the count again; a seat outside the team, itself, or one
//! not running is refused in words; a paused team holds everything automatic
//! and a stale form cannot un-pause it; a `Stop` hook fires the wake declared
//! on it; a watchdog nudges a silent seat once per silence; your own send
//! into a seat is logged and resets the chain while an observed session has
//! no ledger at all; and the log survives a restart with the chain intact.
//!
//! Costs nothing: no agent is launched. The "panes" are the ids on launch
//! records, and the typing is a recording sender set on the state — the one
//! seam over `send::send`, which is still the only code that reaches a pane.

use futures_util::{SinkExt, StreamExt};
use mogeung_core::attention::{classify, AttentionConfig, AttentionReason};
use mogeung_core::session::{LiveStatus, SessionSource};
use mogeung_core::team::{Policy, Seat, SeatEvent, SeatLaunch, SeatSignals, SendKind, SendStatus, SignalKind, Team, Wake, Watchdog};
use mogeung_core::{ClientMsg, ServerMsg};
use mogeungd::{api, state::AppState, store::Store};
use std::sync::{Arc, Mutex};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio_tungstenite::tungstenite::Message;

const PLANNER: &str = "planner-token-0123456789abcdef0123456789abcdef";
const BUILDER: &str = "builder-token-0123456789abcdef0123456789abcdef";
const OTHER: &str = "other-token-0123456789abcdef0123456789abcdef00";
const SHARED: &str = "window-token";

type Typed = Arc<Mutex<Vec<(String, String)>>>;

struct Daemon {
    state: Arc<AppState>,
    port: u16,
    typed: Typed,
    dir: std::path::PathBuf,
}

fn launch(team_id: &str, team: &str, seat: &str, cli: SessionSource, pane: &str, token: &str, kind: SignalKind, started: i64) -> SeatLaunch {
    SeatLaunch {
        team_id: team_id.into(),
        team: team.into(),
        seat: seat.into(),
        cli,
        session_id: None,
        pane_id: pane.into(),
        tmux_target: format!("geung-{team}-{seat}:0.0"),
        tmux_session: format!("geung-{team}-{seat}"),
        team_session: String::new(),
        dir: "/w".into(),
        worktree: false,
        files_dir: "/f".into(),
        started,
        token: token.into(),
        signals: SeatSignals { kind, ..Default::default() },
    }
}

fn team(id: &str, name: &str, seats: &[(&str, SessionSource)], policy: Policy) -> Team {
    Team {
        id: id.into(),
        name: name.into(),
        repo: "/r".into(),
        guidance: String::new(),
        permission: Default::default(),
        policy,
        seats: seats.iter().map(|(n, c)| Seat::new(*n, *c)).collect(),
        paused: false,
        created: 1,
        updated: 1,
    }
}

fn policy() -> Policy {
    Policy {
        hop_budget: 3,
        hourly_cap: 60,
        wakes: vec![Wake {
            on_seat: "planner".into(),
            on: SeatEvent::Stop,
            to: "builder".into(),
            text: "planner stopped — your turn".into(),
        }],
        watchdogs: vec![Watchdog {
            seat: Some("builder".into()),
            silent_minutes: 5,
            while_busy: true,
            text: "status?".into(),
        }],
        ..Policy::default()
    }
}

async fn daemon(tag: &str) -> Daemon {
    let dir = std::env::temp_dir().join(format!("mogeung-seat-sends-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let store = Store::open(&dir.join("t.db")).unwrap();
    let now = chrono::Utc::now().timestamp();
    store.save_team(&team("t1", "port", &[("planner", SessionSource::ClaudeCode), ("builder", SessionSource::Codex)], policy())).unwrap();
    store.save_team(&team("t2", "docs", &[("other", SessionSource::ClaudeCode)], Policy::default())).unwrap();
    store.save_launch(&launch("t1", "port", "planner", SessionSource::ClaudeCode, "%990001", PLANNER, SignalKind::Hooked, now)).unwrap();
    store.save_launch(&launch("t1", "port", "builder", SessionSource::Codex, "%990002", BUILDER, SignalKind::Plain, now - 1200)).unwrap();
    store.save_launch(&launch("t2", "docs", "other", SessionSource::ClaudeCode, "%990003", OTHER, SignalKind::Hooked, now)).unwrap();
    let state = AppState::with_home(store, dir.join("claude")).unwrap();
    let _ = state.teams_dir.set(dir.join("teams"));
    let _ = state.seats_dir.set(dir.join("seats"));
    let typed: Typed = Arc::new(Mutex::new(Vec::new()));
    let rec = typed.clone();
    let _ = state.sender.set(Arc::new(move |target: &str, text: &str| {
        rec.lock().unwrap().push((target.to_string(), text.to_string()));
        Ok(())
    }));
    let app = api::router_with_token(state.clone(), Some(SHARED.into()));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move {
        axum::serve(listener, app.into_make_service_with_connect_info::<std::net::SocketAddr>())
            .await
            .unwrap();
    });
    Daemon { state, port, typed, dir }
}

/// One HTTP/1.1 request over a raw socket: what `mogeung-send`'s `curl` does.
async fn http(port: u16, path: &str, bearer: Option<&str>, body: &str) -> (u16, String) {
    let mut s = tokio::net::TcpStream::connect(("127.0.0.1", port)).await.unwrap();
    let auth = bearer.map(|t| format!("Authorization: Bearer {t}\r\n")).unwrap_or_default();
    let req = format!(
        "POST {path} HTTP/1.1\r\nHost: 127.0.0.1\r\n{auth}Content-Type: text/plain\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    s.write_all(req.as_bytes()).await.unwrap();
    let mut raw = Vec::new();
    tokio::time::timeout(std::time::Duration::from_secs(10), s.read_to_end(&mut raw))
        .await
        .expect("an answer in time")
        .unwrap();
    let text = String::from_utf8_lossy(&raw).into_owned();
    let status: u16 = text.split_whitespace().nth(1).and_then(|s| s.parse().ok()).expect("a status line");
    let body = text.split_once("\r\n\r\n").map(|(_, b)| b.to_string()).unwrap_or_default();
    (status, body)
}

async fn send(d: &Daemon, from: &str, to: &str, text: &str) -> (u16, String) {
    http(d.port, &format!("/api/seat/send?to={to}"), Some(from), text).await
}

/// The board as a window first sees it, seat rows included.
async fn board(state: &AppState) -> Vec<mogeung_core::session::Session> {
    let ServerMsg::Snapshot { sessions, .. } = state.snapshot(false).await else { panic!("not a snapshot") };
    sessions
}

async fn summary(d: &Daemon, team_id: &str) -> mogeung_core::team::TeamSends {
    let ServerMsg::Sends { teams } = d.state.sends_msg().await else { panic!("not sends") };
    teams.into_iter().find(|t| t.team_id == team_id).expect("the team's summary")
}

/// A live row for a seat, as the scan would publish one — so the policy's
/// clock has something to read without a tmux pane behind it.
async fn live_row(d: &Daemon, id: &str, seat: &str, status: LiveStatus, silent_secs: i64) {
    let now = chrono::Utc::now();
    let mut s = mogeungd::state::blank_session_with_id(id);
    s.alive = true;
    s.live_status = Some(status);
    s.last_event_at = now - chrono::Duration::seconds(silent_secs);
    s.started_at = now - chrono::Duration::seconds(silent_secs + 60);
    s.seat = Some(mogeung_core::team::SeatTag {
        team_id: "t1".into(),
        team: "port".into(),
        seat: seat.into(),
        signals: SignalKind::Plain,
    });
    d.state.sessions.write().await.insert(id.to_string(), s);
}

#[tokio::test]
async fn a_seat_reaches_its_teammate_and_the_fourth_hop_is_held_until_you_release_it() {
    let d = daemon("hops").await;

    // No token, a wrong token, the window's token: nobody.
    assert_eq!(http(d.port, "/api/seat/send?to=builder", None, "hi").await.0, 401);
    assert_eq!(http(d.port, "/api/seat/send?to=builder", Some(SHARED), "hi").await.0, 401);
    assert!(d.typed.lock().unwrap().is_empty());

    let (code, line) = send(&d, PLANNER, "builder", "please build the thing").await;
    assert_eq!(code, 200, "{line}");
    assert_eq!(line, "delivered to `builder` — hop 1 of 3");
    assert_eq!(
        d.typed.lock().unwrap().as_slice(),
        &[("%990002".to_string(), "please build the thing".to_string())],
        "the text went into the recipient's pane, by pane id, verbatim"
    );

    let (code, line) = send(&d, BUILDER, "planner", "done").await;
    assert_eq!((code, line.as_str()), (200, "delivered to `planner` — hop 2 of 3"));
    let (code, line) = send(&d, PLANNER, "builder", "now the tests").await;
    assert_eq!((code, line.as_str()), (200, "delivered to `builder` — hop 3 of 3"));

    // The fourth hop is held, not typed.
    let before = d.typed.lock().unwrap().len();
    let (code, line) = send(&d, BUILDER, "planner", "tests pass").await;
    assert_eq!(code, 202, "{line}");
    assert!(line.starts_with("held for a human — the hop budget of 3 is spent"), "{line}");
    assert!(line.contains("not lost"), "{line}");
    assert_eq!(d.typed.lock().unwrap().len(), before, "nothing was typed");

    let s = summary(&d, "t1").await;
    assert_eq!((s.chain, s.hop_budget, s.this_hour, s.held), (3, 3, 3, 1));
    assert_eq!(s.log.len(), 4);
    let held = s.log.last().unwrap().clone();
    assert_eq!(held.status, SendStatus::Held);
    assert_eq!(held.kind, SendKind::Agent);
    assert_eq!((held.from.as_deref(), held.to.as_str()), (Some("builder"), "planner"));
    assert_eq!(held.text, "tests pass");

    // The seat it was for ranks as needing you, with the hold as its detail.
    let rows = board(&d.state).await;
    let planner = rows.iter().find(|r| r.id == "seat:t1:planner").expect("the planner's row");
    let h = planner.held_send.as_ref().expect("held on the row");
    assert_eq!(h.count, 1);
    assert!(h.why.starts_with("the hop budget of 3 is spent"));
    let mut live = planner.clone();
    live.alive = true;
    live.live_status = Some(LiveStatus::Busy);
    let item = classify(&live, chrono::Utc::now(), &AttentionConfig::default());
    assert_eq!(item.reason, AttentionReason::AwaitingInput, "{}", item.detail);
    assert!(item.detail.starts_with("a send held for it — the hop budget"), "{}", item.detail);
    assert!(item.since.is_some(), "the row has a clock");
    let builder = rows.iter().find(|r| r.id == "seat:t1:builder").expect("the builder's row");
    assert!(builder.held_send.is_none(), "nothing is held for the sender");

    // A release types it, and is your act: the chain starts again.
    d.state.release_send("t1", &held.id).await.unwrap();
    assert_eq!(d.typed.lock().unwrap().last().unwrap(), &("%990001".to_string(), "tests pass".to_string()));
    let s = summary(&d, "t1").await;
    assert_eq!((s.chain, s.held), (0, 0));
    assert_eq!(s.log.last().unwrap().status, SendStatus::Released);
    assert_eq!(s.log.last().unwrap().id, held.id, "the same entry, released");
    assert_eq!(s.this_hour, 3, "a release is yours and does not count against the hour");
    let rows = board(&d.state).await;
    assert!(rows.iter().find(|r| r.id == "seat:t1:planner").unwrap().held_send.is_none());
    assert!(d.state.release_send("t1", &held.id).await.is_err(), "released once");

    // And the next hop is the first again.
    let (code, line) = send(&d, PLANNER, "builder", "ship it").await;
    assert_eq!((code, line.as_str()), (200, "delivered to `builder` — hop 1 of 3"));

    // A drop never types.
    let before = d.typed.lock().unwrap().len();
    send(&d, BUILDER, "planner", "a").await;
    send(&d, PLANNER, "builder", "b").await;
    let (code, _) = send(&d, BUILDER, "planner", "c").await;
    assert_eq!(code, 202);
    let id = summary(&d, "t1").await.log.last().unwrap().id.clone();
    d.state.drop_send("t1", &id).await.unwrap();
    assert_eq!(d.typed.lock().unwrap().len(), before + 2);
    assert_eq!(summary(&d, "t1").await.log.last().unwrap().status, SendStatus::Dropped);
    std::fs::remove_dir_all(&d.dir).ok();
}

#[tokio::test]
async fn a_seat_talks_only_to_its_own_team_and_is_told_why_otherwise() {
    let d = daemon("refusals").await;
    let (code, line) = send(&d, PLANNER, "other", "hello over there").await;
    assert_eq!(code, 404, "{line}");
    assert!(line.contains("no seat `other` in team `port`"), "{line}");
    assert!(line.contains("a seat talks only to its own team: `builder`"), "{line}");
    let (code, line) = send(&d, PLANNER, "planner", "note to self").await;
    assert_eq!(code, 409, "{line}");
    assert!(line.contains("cannot send to itself"), "{line}");
    let (code, line) = send(&d, PLANNER, "builder", "   ").await;
    assert_eq!(code, 400, "{line}");
    assert!(line.contains("nothing to send"), "{line}");
    let (code, line) = send(&d, OTHER, "planner", "hi").await;
    assert_eq!(code, 404, "{line}");
    assert!(line.contains("there is no other seat"), "{line}");
    // A seat not running is refused, not held: there is no pane to hold for.
    d.state.launches.write().await.retain(|l| l.seat != "builder");
    let (code, line) = send(&d, PLANNER, "builder", "hi").await;
    assert_eq!(code, 409, "{line}");
    assert!(line.contains("seat `builder` is not running"), "{line}");
    assert!(d.typed.lock().unwrap().is_empty(), "nothing was ever typed");
    // Every refusal is on the log, visible, as a refusal.
    let s = summary(&d, "t1").await;
    assert_eq!(s.log.len(), 4, "{:?}", s.log);
    assert!(s.log.iter().all(|e| e.status == SendStatus::Refused));
    assert_eq!((s.chain, s.this_hour, s.held), (0, 0, 0));
    // The other team's log is untouched: a refusal belongs to the sender's team.
    assert_eq!(summary(&d, "t2").await.log.len(), 1, "the one `other` sent");
    std::fs::remove_dir_all(&d.dir).ok();
}

#[tokio::test]
async fn pause_holds_everything_automatic_and_a_stale_form_cannot_unpause() {
    let d = daemon("pause").await;
    let form = d.state.teams().unwrap().into_iter().find(|t| t.id == "t1").unwrap();
    assert!(!form.paused);
    d.state.set_paused("t1", true).await.unwrap();
    assert!(summary(&d, "t1").await.paused);
    let (code, line) = send(&d, PLANNER, "builder", "hi").await;
    assert_eq!(code, 202, "{line}");
    assert!(line.contains("the team is paused"), "{line}");
    assert!(d.typed.lock().unwrap().is_empty());
    // A wake while paused is held too.
    d.state.policy_event("t1", "planner", SeatEvent::Stop).await;
    let s = summary(&d, "t1").await;
    assert_eq!(s.held, 2);
    assert_eq!(s.log.last().unwrap().kind, SendKind::Wake);
    // A form opened before the pause, saved now, keeps the team paused.
    d.state.save_team(form).unwrap();
    assert!(summary(&d, "t1").await.paused, "the switch is TeamPause's to write");
    // Your own send still goes — typing always works — and the switch opens again.
    d.state.set_paused("t1", false).await.unwrap();
    let (code, _) = send(&d, PLANNER, "builder", "hi").await;
    assert_eq!(code, 200);
    std::fs::remove_dir_all(&d.dir).ok();
}

#[tokio::test]
async fn a_stop_hook_fires_the_wake_declared_on_it_and_a_watchdog_nudges_once() {
    let d = daemon("wakes").await;
    let stop = serde_json::json!({
        "session_id": "abc", "hook_event_name": "Stop", "cwd": "/w", "stop_hook_active": false
    })
    .to_string();
    let (code, _) = http(d.port, "/api/seat/hook", Some(PLANNER), &stop).await;
    assert_eq!(code, 200);
    assert_eq!(
        d.typed.lock().unwrap().as_slice(),
        &[("%990002".to_string(), "planner stopped — your turn".to_string())],
        "the declared text, into the declared seat"
    );
    let s = summary(&d, "t1").await;
    let w = s.log.last().unwrap();
    assert_eq!((w.kind, w.status, w.hop), (SendKind::Wake, SendStatus::Delivered, 1));
    assert_eq!(w.why, "wake on `planner` Stop");
    assert_eq!(w.from.as_deref(), Some("planner"));
    // A hook that is not a seat event fires nothing.
    let prompt = serde_json::json!({ "session_id": "abc", "hook_event_name": "UserPromptSubmit", "prompt": "x" }).to_string();
    http(d.port, "/api/seat/hook", Some(PLANNER), &prompt).await;
    assert_eq!(d.typed.lock().unwrap().len(), 1);

    // The watchdog: builder silent six minutes and busy — nudged; the next
    // tick within the same silence — not again; idle — never.
    live_row(&d, "builder-session", "builder", LiveStatus::Busy, 360).await;
    d.state.policy_tick().await;
    assert_eq!(d.typed.lock().unwrap().last().unwrap(), &("%990002".to_string(), "status?".to_string()));
    let n = summary(&d, "t1").await.log.iter().filter(|e| e.kind == SendKind::Watchdog).count();
    assert_eq!(n, 1);
    d.state.policy_tick().await;
    assert_eq!(summary(&d, "t1").await.log.iter().filter(|e| e.kind == SendKind::Watchdog).count(), 1, "once per silence");
    let s = summary(&d, "t1").await;
    let wd = s.log.iter().find(|e| e.kind == SendKind::Watchdog).unwrap();
    assert_eq!(wd.why, "watchdog 1 — silent 5 min while busy");
    assert_eq!(wd.hop, 0, "a nudge is not a hop");
    assert_eq!(s.chain, 1, "the wake was the one hop");
    assert_eq!(s.this_hour, 2, "both count against the hour");

    // The plain seat's turn ends: busy → idle is its Stop. No wake is
    // declared on builder's Stop, so nothing fires — but the transition is
    // read, which the planner's wake would need were it declared the other way.
    live_row(&d, "builder-session", "builder", LiveStatus::Idle, 0).await;
    d.state.policy_tick().await;
    assert_eq!(d.typed.lock().unwrap().len(), 2);
    std::fs::remove_dir_all(&d.dir).ok();
}

#[tokio::test]
async fn your_send_into_a_seat_resets_the_chain_and_an_observed_session_has_no_ledger() {
    let d = daemon("human").await;
    send(&d, PLANNER, "builder", "one").await;
    send(&d, BUILDER, "planner", "two").await;
    assert_eq!(summary(&d, "t1").await.chain, 2);
    // Your send into the planner's row — the row carries the seat's tag.
    let mut row = mogeungd::state::blank_session_with_id("planner-session");
    row.seat = Some(mogeung_core::team::SeatTag {
        team_id: "t1".into(),
        team: "port".into(),
        seat: "planner".into(),
        signals: SignalKind::Hooked,
    });
    d.state.human_sent(&row, "take a different approach").await;
    let s = summary(&d, "t1").await;
    assert_eq!(s.chain, 0, "your send starts the count again");
    let h = s.log.last().unwrap();
    assert_eq!((h.kind, h.status), (SendKind::Human, SendStatus::Delivered));
    assert_eq!(h.why, "you pressed send");
    assert_eq!(h.to, "planner");
    assert_eq!(s.this_hour, 2, "yours does not count against the hour");
    // An observed session: no tag, no ledger, nothing — fence 1.
    let observed = mogeungd::state::blank_session_with_id("hand-started");
    d.state.human_sent(&observed, "hello").await;
    let ServerMsg::Sends { teams } = d.state.sends_msg().await else { panic!() };
    assert_eq!(teams.iter().map(|t| t.log.len()).sum::<usize>(), 3, "nothing was logged for it anywhere");
    std::fs::remove_dir_all(&d.dir).ok();
}

#[tokio::test]
async fn the_log_survives_a_restart_with_the_chain_intact_and_reaches_the_window() {
    let d = daemon("restart").await;
    send(&d, PLANNER, "builder", "one").await;
    send(&d, BUILDER, "planner", "two").await;
    send(&d, PLANNER, "builder", "three").await;
    let (code, _) = send(&d, BUILDER, "planner", "four").await;
    assert_eq!(code, 202);

    // The window hears `Sends` with the counters, after `TeamList` and on change.
    let (mut ws, _) = tokio_tungstenite::connect_async(format!("ws://127.0.0.1:{}/ws?token={SHARED}", d.port))
        .await
        .unwrap();
    ws.send(Message::Text(serde_json::to_string(&ClientMsg::TeamList).unwrap().into()))
        .await
        .unwrap();
    let wait = async {
        while let Some(Ok(Message::Text(t))) = ws.next().await {
            if let Ok(ServerMsg::Sends { teams }) = serde_json::from_str::<ServerMsg>(&t) {
                return teams;
            }
        }
        panic!("the socket closed first");
    };
    let teams = tokio::time::timeout(std::time::Duration::from_secs(10), wait).await.expect("Sends in time");
    let port = teams.iter().find(|t| t.team_id == "t1").unwrap();
    assert_eq!((port.chain, port.held, port.log.len()), (3, 1, 4));
    assert!(!serde_json::to_string(&teams).unwrap().contains(PLANNER), "no token rides on the log");

    // A new daemon over the same store counts the same.
    let store = Store::open(&d.dir.join("t.db")).unwrap();
    let again = AppState::with_home(store, d.dir.join("claude")).unwrap();
    let _ = again.teams_dir.set(d.dir.join("teams"));
    let ServerMsg::Sends { teams } = again.sends_msg().await else { panic!() };
    let port = teams.iter().find(|t| t.team_id == "t1").unwrap();
    assert_eq!((port.chain, port.held, port.log.len()), (3, 1, 4));
    // And the held send is still held for the planner, so the row still
    // ranks — after the first scan, which is where a fresh daemon builds
    // its seat rows from the records.
    again.scan().await;
    let rows = board(&again).await;
    assert!(rows.iter().find(|r| r.id == "seat:t1:planner").unwrap().held_send.is_some());
    std::fs::remove_dir_all(&d.dir).ok();
}
