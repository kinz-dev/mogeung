//! HTTP + WebSocket API.
//!
//! One WebSocket carries the whole live state: commands in, events out.
//! Commands are fire-and-forget — their effect comes back on the same stream —
//! which keeps clients a pure projection of daemon state. Bulk reads
//! (transcripts, diffs) are plain GETs so a client can fetch them lazily, and
//! so the daemon is curl-able without a UI.

use crate::state::AppState;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Path as AxPath, Query, State};
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::{Json, Router};
use futures_util::{SinkExt, StreamExt};
use mogeung_core::{ClientMsg, ServerMsg, Session};
use serde::Deserialize;
use std::sync::Arc;

pub fn router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/api/health", get(health))
        .route("/api/sessions", get(list_sessions))
        .route("/api/sessions/{id}", get(get_session))
        .route("/api/sessions/{id}/events", get(get_events))
        .route("/api/sessions/{id}/change", get(get_change))
        .route("/api/sessions/{id}/review_all", post(review_all))
        .route("/api/sessions/{id}/review", post(review_hunk))
        .route("/api/queue", get(get_queue))
        .route("/api/rescan", post(rescan))
        .route("/ws", get(ws_upgrade))
        .with_state(state)
}

async fn health() -> impl IntoResponse {
    Json(serde_json::json!({ "ok": true, "version": env!("CARGO_PKG_VERSION") }))
}

async fn list_sessions(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let mut sessions: Vec<Session> = state.sessions.read().await.values().cloned().collect();
    sessions.sort_by_key(|s| s.last_event_at);
    sessions.reverse();
    Json(serde_json::to_value(sessions).unwrap_or_default())
}

async fn get_session(
    State(state): State<Arc<AppState>>,
    AxPath(id): AxPath<String>,
) -> impl IntoResponse {
    match state.get(&id).await {
        Some(s) => Json(serde_json::to_value(s).unwrap_or_default()),
        None => Json(serde_json::json!({ "error": "no such session" })),
    }
}

async fn get_queue(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let sessions: Vec<Session> = state.sessions.read().await.values().cloned().collect();
    let queue = mogeung_core::attention::rank(&sessions, chrono::Utc::now(), &state.attention);
    Json(serde_json::to_value(queue).unwrap_or_default())
}

async fn rescan(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    state.scan().await;
    Json(serde_json::json!({ "ok": true }))
}

#[derive(Deserialize)]
struct SinceQuery {
    #[serde(default)]
    since: u64,
}

async fn get_events(
    State(state): State<Arc<AppState>>,
    AxPath(id): AxPath<String>,
    Query(q): Query<SinceQuery>,
) -> impl IntoResponse {
    match state.store.load_events(&id, q.since) {
        Ok(evs) => Json(serde_json::to_value(evs).unwrap_or_default()),
        Err(e) => Json(serde_json::json!({ "error": e.to_string() })),
    }
}

async fn get_change(
    State(state): State<Arc<AppState>>,
    AxPath(id): AxPath<String>,
) -> impl IntoResponse {
    if let Some(c) = state.changes.read().await.get(&id).cloned() {
        return Json(serde_json::to_value(c).unwrap_or_default());
    }
    match state.recompute_change(&id).await {
        Some(c) => Json(serde_json::to_value(c).unwrap_or_default()),
        None => Json(serde_json::json!({ "error": "no such session, or it has no working directory" })),
    }
}

async fn review_all(
    State(state): State<Arc<AppState>>,
    AxPath(id): AxPath<String>,
) -> impl IntoResponse {
    state.review_all(&id).await;
    Json(serde_json::json!({ "ok": true }))
}

#[derive(Deserialize)]
struct ReviewBody {
    anchor: String,
    #[serde(default = "yes")]
    reviewed: bool,
}

fn yes() -> bool {
    true
}

async fn review_hunk(
    State(state): State<Arc<AppState>>,
    AxPath(id): AxPath<String>,
    Json(body): Json<ReviewBody>,
) -> impl IntoResponse {
    state
        .set_hunk_reviewed(&id, &body.anchor, body.reviewed)
        .await;
    Json(serde_json::json!({ "ok": true }))
}

async fn ws_upgrade(ws: WebSocketUpgrade, State(state): State<Arc<AppState>>) -> impl IntoResponse {
    ws.on_upgrade(move |socket| ws_conn(socket, state))
}

async fn ws_conn(socket: WebSocket, state: Arc<AppState>) {
    let (mut sink, mut stream) = socket.split();
    let mut rx = state.tx.subscribe();

    // Push the full snapshot immediately so a client is useful before it sends
    // anything, and so reconnects self-heal.
    if let Ok(txt) = serde_json::to_string(&state.snapshot().await) {
        if sink.send(Message::Text(txt.into())).await.is_err() {
            return;
        }
    }

    let send_task = tokio::spawn(async move {
        loop {
            match rx.recv().await {
                Ok(msg) => {
                    let Ok(txt) = serde_json::to_string(&msg) else {
                        continue;
                    };
                    if sink.send(Message::Text(txt.into())).await.is_err() {
                        break;
                    }
                }
                // A slow client that fell behind gets told to reconnect rather
                // than wedging the broadcast channel for everyone else.
                Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                    let warn = ServerMsg::Error {
                        message: format!("client fell behind, dropped {n} messages — reconnect"),
                    };
                    let txt = serde_json::to_string(&warn).unwrap_or_default();
                    let _ = sink.send(Message::Text(txt.into())).await;
                }
                Err(_) => break,
            }
        }
    });

    while let Some(Ok(msg)) = stream.next().await {
        let Message::Text(txt) = msg else { continue };
        match serde_json::from_str::<ClientMsg>(&txt) {
            Ok(cmd) => handle(&state, cmd).await,
            Err(e) => state.broadcast(ServerMsg::Error {
                message: format!("bad command: {e}"),
            }),
        }
    }

    send_task.abort();
}

async fn handle(state: &Arc<AppState>, cmd: ClientMsg) {
    let err = |e: anyhow::Error| {
        state.broadcast(ServerMsg::Error {
            message: e.to_string(),
        })
    };

    match cmd {
        ClientMsg::Subscribe => {
            let snap = state.snapshot().await;
            state.broadcast(snap);
        }
        ClientMsg::SetHunkReviewed {
            session_id,
            anchor,
            reviewed,
        } => state.set_hunk_reviewed(&session_id, &anchor, reviewed).await,
        ClientMsg::ReviewAll { session_id } => state.review_all(&session_id).await,
        ClientMsg::RefreshChange { session_id } => {
            state.recompute_change(&session_id).await;
        }
        ClientMsg::FetchEvents { session_id, since } => {
            match state.store.load_events(&session_id, since) {
                Ok(events) if !events.is_empty() => state.broadcast(ServerMsg::Events { events }),
                Ok(_) => {}
                Err(e) => err(anyhow::anyhow!(e)),
            }
        }
        ClientMsg::ForgetSession { session_id } => {
            if let Err(e) = state.forget(&session_id).await {
                err(e);
            }
        }
        ClientMsg::LaunchTerminal { dir, worktree } => {
            if let Err(e) = state.launch_terminal(&dir, worktree).await {
                err(e);
            }
        }
        ClientMsg::Rescan => state.scan().await,
    }
}
