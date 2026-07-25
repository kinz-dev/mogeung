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
        // The thin web client (R-C3). Same WebSocket, same authority model:
        // the phone is a projection, exactly like the desktop window.
        .route("/", get(index))
        .route("/api/health", get(health))
        .route("/api/sessions", get(list_sessions))
        .route("/api/sessions/{id}", get(get_session))
        .route("/api/sessions/{id}/events", get(get_events))
        .route("/api/sessions/{id}/change", get(get_change))
        .route("/api/sessions/{id}/review_all", post(review_all))
        .route("/api/sessions/{id}/review", post(review_hunk))
        .route("/api/queue", get(get_queue))
        .route("/api/rescan", post(rescan))
        .route("/api/repos", get(list_repos))
        .route("/api/repos/{repo}/debt", get(get_debt))
        .route("/api/sessions/{id}/blast", get(get_blast))
        .route("/ws", get(ws_upgrade))
        .with_state(state)
}

/// Liveness *and* honesty: whether the daemon is up, and whether it is still
/// reading everything it should be. Curl-able without a UI, deliberately — the
/// answer to "is the board empty because nothing is happening, or because
/// mogeung went blind?" should not require a window.
async fn health(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let h = state.health().await;
    Json(serde_json::json!({
        "ok": true,
        "version": env!("CARGO_PKG_VERSION"),
        "headline": h.headline(),
        "blind_ratio": h.blind_ratio(),
        "urgent_alerts": h.urgent_alerts(),
        "alerts": h.alerts.iter().map(|a| a.message()).collect::<Vec<_>>(),
        "detail": h,
    }))
}

async fn index() -> impl IntoResponse {
    axum::response::Html(crate::web::INDEX_HTML)
}

async fn list_repos(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    Json(serde_json::to_value(state.known_repos().await).unwrap_or_default())
}

async fn get_debt(
    State(state): State<Arc<AppState>>,
    AxPath(repo): AxPath<String>,
) -> impl IntoResponse {
    let debt = state.review_debt(&repo).await;
    Json(serde_json::to_value(debt).unwrap_or_default())
}

#[derive(Deserialize)]
struct PathQuery {
    path: String,
}

async fn get_blast(
    State(state): State<Arc<AppState>>,
    AxPath(id): AxPath<String>,
    Query(q): Query<PathQuery>,
) -> impl IntoResponse {
    match state.blast_radius(&id, &q.path).await {
        Some(r) => Json(serde_json::to_value(r).unwrap_or_default()),
        None => Json(serde_json::json!({ "error": "no diff for that path" })),
    }
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
        ClientMsg::FetchHealth => {
            let health = state.health().await;
            state.broadcast(ServerMsg::Health {
                health: Box::new(health),
            });
        }
        ClientMsg::Snooze {
            session_id,
            minutes,
        } => state.snooze(&session_id, minutes).await,
        ClientMsg::FetchReviewDebt { repo } => {
            let debt = state.review_debt(&repo).await;
            state.broadcast(ServerMsg::ReviewDebt {
                debt: Box::new(debt),
            });
        }
        ClientMsg::FetchBlastRadius { session_id, path } => {
            match state.blast_radius(&session_id, &path).await {
                Some(radius) => state.broadcast(ServerMsg::BlastRadius {
                    radius: Box::new(radius),
                }),
                None => err(anyhow::anyhow!(
                    "no diff for {path} in that session, or it is not in a repo"
                )),
            }
        }
        ClientMsg::FocusTerminal { session_id } => {
            if let Err(e) = state.focus_terminal(&session_id).await {
                err(e);
            }
        }
    }
}
