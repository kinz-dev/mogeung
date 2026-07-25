//! End-to-end tests over the real WebSocket API.
//!
//! Everything here is free — the observer model means no test ever spawns an
//! agent.

use futures_util::{SinkExt, StreamExt};
use mogeung_core::{ClientMsg, ServerMsg};
use mogeungd::{api, state::AppState, store::Store};
use std::path::PathBuf;
use std::time::{Duration, Instant};
use tokio_tungstenite::tungstenite::Message;

struct Harness {
    url: String,
    _dir: PathBuf,
}

async fn boot(name: &str) -> Harness {
    let dir = std::env::temp_dir().join(format!("mogeung-test-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();

    let store = Store::open(&dir.join("test.db")).unwrap();
    let state = AppState::new(store).unwrap();
    let app = api::router(state.clone());

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    Harness {
        url: format!("ws://127.0.0.1:{port}/ws"),
        _dir: dir,
    }
}

type Ws =
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

async fn send(ws: &mut Ws, msg: ClientMsg) {
    ws.send(Message::Text(serde_json::to_string(&msg).unwrap().into()))
        .await
        .unwrap();
}

async fn wait_for<T>(
    ws: &mut Ws,
    secs: u64,
    mut pred: impl FnMut(&ServerMsg) -> Option<T>,
) -> Option<T> {
    let deadline = Instant::now() + Duration::from_secs(secs);
    while Instant::now() < deadline {
        let remaining = deadline.saturating_duration_since(Instant::now());
        match tokio::time::timeout(remaining, ws.next()).await {
            Ok(Some(Ok(Message::Text(t)))) => {
                if let Ok(msg) = serde_json::from_str::<ServerMsg>(&t) {
                    if let Some(v) = pred(&msg) {
                        return Some(v);
                    }
                }
            }
            Ok(Some(Ok(_))) => {}
            _ => return None,
        }
    }
    None
}

#[tokio::test]
async fn snapshot_arrives_unsolicited_on_connect() {
    let h = boot("snapshot").await;
    let (mut ws, _) = tokio_tungstenite::connect_async(&h.url).await.unwrap();

    let got = wait_for(&mut ws, 5, |m| match m {
        ServerMsg::Snapshot { sessions, queue } => Some((sessions.len(), queue.len())),
        _ => None,
    })
    .await;
    assert!(got.is_some(), "no snapshot on connect");
}

#[tokio::test]
async fn a_malformed_command_is_reported_without_dropping_the_socket() {
    let h = boot("badcmd").await;
    let (mut ws, _) = tokio_tungstenite::connect_async(&h.url).await.unwrap();
    wait_for(&mut ws, 5, |m| {
        matches!(m, ServerMsg::Snapshot { .. }).then_some(())
    })
    .await
    .unwrap();

    ws.send(Message::Text("{\"cmd\":\"nonsense\"}".to_string().into()))
        .await
        .unwrap();
    let err = wait_for(&mut ws, 5, |m| match m {
        ServerMsg::Error { message } => Some(message.clone()),
        _ => None,
    })
    .await;
    assert!(err.is_some(), "malformed command should be reported");

    // ...and the socket still works afterwards.
    send(&mut ws, ClientMsg::Subscribe).await;
    let alive = wait_for(&mut ws, 5, |m| {
        matches!(m, ServerMsg::Snapshot { .. }).then_some(())
    })
    .await;
    assert!(alive.is_some(), "connection died after a bad command");
}

#[tokio::test]
async fn commands_about_unknown_sessions_are_harmless() {
    let h = boot("unknown").await;
    let (mut ws, _) = tokio_tungstenite::connect_async(&h.url).await.unwrap();
    wait_for(&mut ws, 5, |m| {
        matches!(m, ServerMsg::Snapshot { .. }).then_some(())
    })
    .await
    .unwrap();

    let ghost = "00000000-0000-0000-0000-000000000000".to_string();
    send(
        &mut ws,
        ClientMsg::FetchEvents {
            session_id: ghost.clone(),
            since: 0,
        },
    )
    .await;
    send(
        &mut ws,
        ClientMsg::RefreshChange {
            session_id: ghost.clone(),
        },
    )
    .await;
    send(
        &mut ws,
        ClientMsg::ReviewAll {
            session_id: ghost.clone(),
        },
    )
    .await;

    send(&mut ws, ClientMsg::Subscribe).await;
    let alive = wait_for(&mut ws, 5, |m| {
        matches!(m, ServerMsg::Snapshot { .. }).then_some(())
    })
    .await;
    assert!(alive.is_some(), "unknown session id killed the connection");
}

#[tokio::test]
async fn rescan_is_safe_to_request_over_the_wire() {
    let h = boot("rescan").await;
    let (mut ws, _) = tokio_tungstenite::connect_async(&h.url).await.unwrap();
    wait_for(&mut ws, 5, |m| {
        matches!(m, ServerMsg::Snapshot { .. }).then_some(())
    })
    .await
    .unwrap();

    send(&mut ws, ClientMsg::Rescan).await;
    let queued = wait_for(&mut ws, 10, |m| {
        matches!(m, ServerMsg::Queue { .. }).then_some(())
    })
    .await;
    assert!(queued.is_some(), "rescan produced no queue update");
}
