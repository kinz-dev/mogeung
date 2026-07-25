//! HTTP + WebSocket API.
//!
//! One WebSocket carries the whole live state: commands in, events out.
//! Commands are fire-and-forget — their effect comes back on the same stream —
//! which keeps clients a pure projection of daemon state. Bulk reads
//! (transcripts, diffs) are plain GETs so a client can fetch them lazily.

use crate::state::AppState;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Path as AxPath, Query, State};
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::{Json, Router};
use futures_util::{SinkExt, StreamExt};
use mogeung_core::{ClientMsg, NewRunSpec, Run, RunId, ServerMsg};
use serde::Deserialize;
use std::str::FromStr;
use std::sync::Arc;

pub fn router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/api/health", get(health))
        .route("/api/runs", get(list_runs).post(start_run))
        .route("/api/runs/{id}", get(get_run))
        .route("/api/runs/{id}/events", get(get_events))
        .route("/api/runs/{id}/change", get(get_change))
        .route("/api/runs/{id}/follow_up", post(follow_up))
        .route("/api/runs/{id}/review_all", post(review_all))
        .route("/api/runs/{id}/review", post(review_hunk))
        .route("/api/repos", get(list_repos))
        .route("/ws", get(ws_upgrade))
        .with_state(state)
}

async fn health() -> impl IntoResponse {
    Json(serde_json::json!({ "ok": true, "version": env!("CARGO_PKG_VERSION") }))
}

/// The REST surface below duplicates a few WebSocket commands on purpose: it
/// makes the daemon scriptable from a shell, and debuggable with curl, without
/// standing up a client.
async fn list_runs(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let mut runs: Vec<Run> = state.runs.read().await.values().cloned().collect();
    runs.sort_by_key(|r| r.created_at);
    Json(serde_json::to_value(runs).unwrap_or_default())
}

async fn get_run(
    State(state): State<Arc<AppState>>,
    AxPath(id): AxPath<String>,
) -> impl IntoResponse {
    let Ok(run_id) = RunId::from_str(&id) else {
        return Json(serde_json::json!({ "error": "bad run id" }));
    };
    match state.get_run(run_id).await {
        Some(r) => Json(serde_json::to_value(r).unwrap_or_default()),
        None => Json(serde_json::json!({ "error": "no such run" })),
    }
}

async fn list_repos(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    Json(serde_json::json!({ "repos": state.repos.read().await.clone() }))
}

async fn start_run(
    State(state): State<Arc<AppState>>,
    Json(spec): Json<NewRunSpec>,
) -> impl IntoResponse {
    match state.start_run(spec).await {
        Ok(id) => Json(serde_json::json!({ "run_id": id.to_string() })),
        Err(e) => Json(serde_json::json!({ "error": e.to_string() })),
    }
}

#[derive(Deserialize)]
struct PromptBody {
    prompt: String,
}

async fn follow_up(
    State(state): State<Arc<AppState>>,
    AxPath(id): AxPath<String>,
    Json(body): Json<PromptBody>,
) -> impl IntoResponse {
    let Ok(run_id) = RunId::from_str(&id) else {
        return Json(serde_json::json!({ "error": "bad run id" }));
    };
    match state.follow_up(run_id, body.prompt).await {
        Ok(new_id) => Json(serde_json::json!({ "run_id": new_id.to_string() })),
        Err(e) => Json(serde_json::json!({ "error": e.to_string() })),
    }
}

async fn review_all(
    State(state): State<Arc<AppState>>,
    AxPath(id): AxPath<String>,
) -> impl IntoResponse {
    let Ok(run_id) = RunId::from_str(&id) else {
        return Json(serde_json::json!({ "error": "bad run id" }));
    };
    state.review_all(run_id).await;
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
    let Ok(run_id) = RunId::from_str(&id) else {
        return Json(serde_json::json!({ "error": "bad run id" }));
    };
    state
        .set_hunk_reviewed(run_id, &body.anchor, body.reviewed)
        .await;
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
    let Ok(run_id) = RunId::from_str(&id) else {
        return Json(serde_json::json!({ "error": "bad run id" }));
    };
    match state.store.load_events(run_id, q.since) {
        Ok(evs) => Json(serde_json::to_value(evs).unwrap_or_default()),
        Err(e) => Json(serde_json::json!({ "error": e.to_string() })),
    }
}

async fn get_change(
    State(state): State<Arc<AppState>>,
    AxPath(id): AxPath<String>,
) -> impl IntoResponse {
    let Ok(run_id) = RunId::from_str(&id) else {
        return Json(serde_json::json!({ "error": "bad run id" }));
    };
    // Serve the cached diff if we have one; otherwise compute on demand.
    if let Some(c) = state.changes.read().await.get(&run_id).cloned() {
        return Json(serde_json::to_value(c).unwrap_or_default());
    }
    match state.recompute_change(run_id).await {
        Some(c) => Json(serde_json::to_value(c).unwrap_or_default()),
        None => Json(serde_json::json!({ "error": "no such run" })),
    }
}

async fn ws_upgrade(
    ws: WebSocketUpgrade,
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
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
                // A slow client that fell behind gets dropped rather than
                // wedging the broadcast channel for everyone else.
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
        ClientMsg::StartRun(spec) => {
            if let Err(e) = state.start_run(spec).await {
                err(e);
            }
        }
        ClientMsg::FollowUp { run_id, prompt } => {
            if let Err(e) = state.follow_up(run_id, prompt).await {
                err(e);
            }
        }
        ClientMsg::CancelRun { run_id } => state.cancel_run(run_id).await,
        ClientMsg::DeleteRun {
            run_id,
            remove_worktree,
        } => {
            if let Err(e) = state.delete_run(run_id, remove_worktree).await {
                err(e);
            }
        }
        ClientMsg::SetHunkReviewed {
            run_id,
            anchor,
            reviewed,
        } => state.set_hunk_reviewed(run_id, &anchor, reviewed).await,
        ClientMsg::ReviewAll { run_id } => state.review_all(run_id).await,
        ClientMsg::RefreshChange { run_id } => {
            state.recompute_change(run_id).await;
        }
        ClientMsg::FetchEvents { run_id, since } => match state.store.load_events(run_id, since) {
            Ok(events) if !events.is_empty() => state.broadcast(ServerMsg::Events { events }),
            Ok(_) => {}
            Err(e) => err(anyhow::anyhow!(e)),
        },
        ClientMsg::AddRepo { path } => {
            if let Err(e) = state.add_repo(&path).await {
                err(e);
            }
        }
        ClientMsg::RemoveRepo { path } => {
            if let Err(e) = state.remove_repo(&path).await {
                err(e);
            }
        }
    }
}
