//! The hook receiver, served the way `server::run` serves it. `R-P5`,
//! feature 0051, ADR-0043 clause 3.
//!
//! What is pinned: a seat's hook is heard with the seat's own token and no
//! other — not a wrong one, not the daemon's shared token, which is a
//! window's key; the shared token still guards every other route after the
//! seat routes were merged in beside it; a status reading answers with the
//! seat's line and counts nothing; an event name nobody classified is raised
//! through Health as `hook:<name>` rather than dropped; and the record a
//! window receives never carries the token.
//!
//! Costs nothing: the "seat" is a launch record, and the "hook" is this test
//! posting what Claude Code would. No agent is launched.

use futures_util::{SinkExt, StreamExt};
use mogeung_core::team::{SeatLaunch, SeatSignals, SignalKind};
use mogeung_core::{ClientMsg, ServerMsg};
use mogeungd::{api, state::AppState, store::Store};
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio_tungstenite::tungstenite::Message;

const SEAT_TOKEN: &str = "seat-token-0123456789abcdef";
const SHARED_TOKEN: &str = "window-token";

struct Daemon {
    state: Arc<AppState>,
    port: u16,
    files: std::path::PathBuf,
}

async fn daemon(tag: &str) -> Daemon {
    let dir = std::env::temp_dir().join(format!("mogeung-seat-hooks-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let store = Store::open(&dir.join("t.db")).unwrap();
    let files = dir.join("files");
    store
        .save_launch(&SeatLaunch {
            team_id: "t1".into(),
            team: "port".into(),
            seat: "builder".into(),
            cli: mogeung_core::session::SessionSource::ClaudeCode,
            session_id: Some("abc".into()),
            pane_id: "%999998".into(),
            tmux_target: "geung-port-builder:0.0".into(),
            tmux_session: "geung-port-builder".into(),
            team_session: String::new(),
            dir: dir.to_string_lossy().into_owned(),
            worktree: false,
            files_dir: files.to_string_lossy().into_owned(),
            started: 1_700_000_000,
            token: SEAT_TOKEN.into(),
            signals: SeatSignals { kind: SignalKind::Hooked, ..Default::default() },
        })
        .unwrap();
    let state = AppState::with_home(store, dir.join("claude")).unwrap();
    let _ = state.teams_dir.set(dir.join("teams"));
    let _ = state.seats_dir.set(dir.join("seats"));
    let app = api::router_with_token(state.clone(), Some(SHARED_TOKEN.into()));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move {
        axum::serve(listener, app.into_make_service_with_connect_info::<std::net::SocketAddr>())
            .await
            .unwrap();
    });
    Daemon { state, port, files }
}

/// One HTTP/1.1 request over a raw socket — the daemon has no HTTP client
/// dependency, and a hook's `curl` is exactly this.
async fn http(port: u16, method: &str, path: &str, bearer: Option<&str>, body: &str) -> (u16, String) {
    let mut s = tokio::net::TcpStream::connect(("127.0.0.1", port)).await.unwrap();
    let auth = bearer.map(|t| format!("Authorization: Bearer {t}\r\n")).unwrap_or_default();
    let req = format!(
        "{method} {path} HTTP/1.1\r\nHost: 127.0.0.1\r\n{auth}Content-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
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

fn stop() -> String {
    serde_json::json!({
        "session_id": "abc", "transcript_path": "/t.jsonl", "cwd": "/w",
        "hook_event_name": "Stop", "permission_mode": "default", "stop_hook_active": false
    })
    .to_string()
}

#[tokio::test]
async fn a_hook_is_heard_with_the_seats_token_and_no_other() {
    let d = daemon("token").await;

    let (status, _) = http(d.port, "POST", "/api/seat/hook", None, &stop()).await;
    assert_eq!(status, 401, "no token");
    let (status, _) = http(d.port, "POST", "/api/seat/hook", Some("not-it"), &stop()).await;
    assert_eq!(status, 401, "a wrong token");
    let (status, _) = http(d.port, "POST", "/api/seat/hook", Some(SHARED_TOKEN), &stop()).await;
    assert_eq!(status, 401, "the daemon's shared token is a window's key, not a seat's");
    assert_eq!(d.state.launches.read().await[0].signals.events, 0, "nothing was counted for a refusal");

    let (status, body) = http(d.port, "POST", "/api/seat/hook", Some(SEAT_TOKEN), &stop()).await;
    assert_eq!(status, 200);
    assert!(body.is_empty(), "a hook's reply is nothing: Claude Code reads stdout");
    let l = d.state.launches.read().await[0].clone();
    assert_eq!(l.signals.events, 1);
    assert_eq!(l.signals.last_event.as_deref(), Some("Stop"));
    assert!(l.signals.last_event_at.is_some());
    // The body is kept beside the seat's files, one line, for the week of
    // captures — and nowhere else.
    let kept = std::fs::read_to_string(d.files.join("signals.jsonl")).unwrap();
    assert_eq!(kept.lines().count(), 1);
    assert!(kept.contains("\"Stop\""));

    // The shared token still guards everything else: merging the seat routes
    // beside the layer must not have opened the board.
    let (status, _) = http(d.port, "GET", "/api/health", None, "").await;
    assert_eq!(status, 401, "the shared token is still asked of a window");
    let (status, _) = http(d.port, "GET", "/api/health", Some(SHARED_TOKEN), "").await;
    assert_eq!(status, 200);
}

#[tokio::test]
async fn the_record_a_window_receives_never_carries_the_token() {
    let d = daemon("wire").await;
    let (mut ws, _) = tokio_tungstenite::connect_async(format!("ws://127.0.0.1:{}/ws?token={SHARED_TOKEN}", d.port))
        .await
        .unwrap();
    ws.send(Message::Text(serde_json::to_string(&ClientMsg::TeamList).unwrap().into()))
        .await
        .unwrap();
    // Then an event, which is broadcast as `Seats`.
    let (status, _) = http(d.port, "POST", "/api/seat/hook", Some(SEAT_TOKEN), &stop()).await;
    assert_eq!(status, 200);
    let wait = async {
        let mut seen = 0;
        while let Some(Ok(Message::Text(t))) = ws.next().await {
            if let Ok(ServerMsg::Seats { launches }) = serde_json::from_str::<ServerMsg>(&t) {
                assert!(!t.contains(SEAT_TOKEN), "the token crossed the wire: {t}");
                assert!(launches[0].token.is_empty());
                assert_eq!(launches[0].signals.kind, SignalKind::Hooked);
                seen += 1;
                if launches[0].signals.events == 1 {
                    return seen;
                }
            }
        }
        panic!("the socket closed before the seat's event was broadcast");
    };
    let seen = tokio::time::timeout(std::time::Duration::from_secs(10), wait).await.expect("a Seats event in time");
    assert!(seen >= 1);
}

#[tokio::test]
async fn a_status_reading_answers_with_the_seats_line_and_counts_nothing() {
    let d = daemon("status").await;
    let reading = serde_json::json!({
        "hook_event_name": "Status", "session_id": "abc", "transcript_path": "/t.jsonl", "cwd": "/w",
        "model": { "id": "claude-haiku-4-5", "display_name": "Haiku 4.5" },
        "workspace": { "current_dir": "/w", "project_dir": "/w" },
        "cost": { "total_cost_usd": 0.01, "total_duration_ms": 1000 },
        "context_window": { "context_window_size": 200000, "used_percentage": 43.4 }
    })
    .to_string();
    let (status, body) = http(d.port, "POST", "/api/seat/status", Some(SEAT_TOKEN), &reading).await;
    assert_eq!(status, 200);
    assert!(body.contains("port/builder"), "the status line names the seat: {body}");
    let l = d.state.launches.read().await[0].clone();
    assert_eq!(l.signals.events, 0, "a reading is not an event");
    assert_eq!(l.signals.context_used, Some(43));
    assert_eq!(l.signals.model.as_deref(), Some("Haiku 4.5"));
    assert!(!d.files.join("signals.jsonl").exists(), "a reading is never kept");
    // Dollars stay where ADR-0024 put them: nothing of `cost` is on the record.
    assert!(!serde_json::to_string(&l).unwrap().contains("0.01"));
}

#[tokio::test]
async fn an_unclassified_hook_name_is_raised_through_health_not_dropped() {
    let d = daemon("unknown").await;
    let strange = serde_json::json!({ "session_id": "abc", "hook_event_name": "WarpDrive" }).to_string();
    let (status, _) = http(d.port, "POST", "/api/seat/hook", Some(SEAT_TOKEN), &strange).await;
    assert_eq!(status, 200, "accepted — it is the seat's — but not read");
    let l = d.state.launches.read().await[0].clone();
    assert_eq!(l.signals.events, 0);
    assert_eq!(l.signals.unread, 1);
    assert_eq!(l.signals.last_event, None);
    let (_, health) = http(d.port, "GET", "/api/health", Some(SHARED_TOKEN), "").await;
    assert!(health.contains("hook:WarpDrive"), "the canary did not name it: {health}");
    // And a body that is not JSON at all is unread too, never a crash.
    let (status, _) = http(d.port, "POST", "/api/seat/hook", Some(SEAT_TOKEN), "not json").await;
    assert_eq!(status, 200);
    assert_eq!(d.state.launches.read().await[0].signals.unread, 2);
}
