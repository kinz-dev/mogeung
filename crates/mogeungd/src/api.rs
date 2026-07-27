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
        // The explorer (R-B24). Read-only — there is no write route, by design.
        .route("/api/sessions/{id}/ls", get(get_dir))
        .route("/api/sessions/{id}/file", get(get_file))
        .route("/api/sessions/{id}/tree", get(get_tree))
        .route("/api/sessions/{id}/search", get(get_search))
        .route("/api/sessions/{id}/git/log", get(get_git_log))
        .route("/api/sessions/{id}/git/show", get(get_git_show))
        .route("/api/sessions/{id}/git/status", get(get_git_status))
        .route("/api/sessions/{id}/git/diff", get(get_git_diff))
        .route("/api/sessions/{id}/git/blame", get(get_git_blame))
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
        // So a client can confirm an already-running daemon is watching the
        // same place before attaching to it.
        "claude_home": state.claude_home.to_string_lossy(),
        "pid": std::process::id(),
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

#[derive(Deserialize)]
struct DirQuery {
    /// Relative to the session root; absent means the root.
    #[serde(default)]
    path: String,
}

async fn get_dir(
    State(state): State<Arc<AppState>>,
    AxPath(id): AxPath<String>,
    Query(q): Query<DirQuery>,
) -> impl IntoResponse {
    match state.list_dir(&id, &q.path).await {
        Ok(entries) => Json(serde_json::to_value(entries).unwrap_or_default()),
        Err(e) => Json(serde_json::json!({ "error": e.to_string() })),
    }
}

async fn get_file(
    State(state): State<Arc<AppState>>,
    AxPath(id): AxPath<String>,
    Query(q): Query<PathQuery>,
) -> impl IntoResponse {
    match state.read_file(&id, &q.path).await {
        Ok((content, truncated)) => Json(serde_json::json!({
            "path": q.path,
            "content": content,
            "truncated": truncated,
        })),
        Err(e) => Json(serde_json::json!({ "error": e.to_string() })),
    }
}

async fn get_tree(
    State(state): State<Arc<AppState>>,
    AxPath(id): AxPath<String>,
) -> impl IntoResponse {
    match state.list_tree(&id).await {
        Ok((paths, truncated)) => Json(serde_json::json!({
            "paths": paths,
            "truncated": truncated,
        })),
        Err(e) => Json(serde_json::json!({ "error": e.to_string() })),
    }
}

#[derive(Deserialize)]
struct SearchQuery {
    q: String,
}

async fn get_search(
    State(state): State<Arc<AppState>>,
    AxPath(id): AxPath<String>,
    Query(q): Query<SearchQuery>,
) -> impl IntoResponse {
    match state.search_content(&id, &q.q).await {
        Ok((matches, truncated)) => Json(serde_json::json!({
            "matches": matches,
            "truncated": truncated,
        })),
        Err(e) => Json(serde_json::json!({ "error": e.to_string() })),
    }
}

#[derive(Deserialize)]
struct LogQuery {
    #[serde(default)]
    skip: u32,
    #[serde(default = "default_log_limit")]
    limit: u32,
}

fn default_log_limit() -> u32 {
    50
}

async fn get_git_log(
    State(state): State<Arc<AppState>>,
    AxPath(id): AxPath<String>,
    Query(q): Query<LogQuery>,
) -> impl IntoResponse {
    match state.git_log(&id, q.skip, q.limit).await {
        Ok((commits, done)) => Json(serde_json::json!({ "commits": commits, "done": done })),
        Err(e) => Json(serde_json::json!({ "error": e.to_string() })),
    }
}

#[derive(Deserialize)]
struct ShaQuery {
    sha: String,
}

async fn get_git_show(
    State(state): State<Arc<AppState>>,
    AxPath(id): AxPath<String>,
    Query(q): Query<ShaQuery>,
) -> impl IntoResponse {
    match state.git_show(&id, &q.sha).await {
        Ok(files) => Json(serde_json::json!({ "sha": q.sha, "files": files })),
        Err(e) => Json(serde_json::json!({ "error": e.to_string() })),
    }
}

async fn get_git_status(
    State(state): State<Arc<AppState>>,
    AxPath(id): AxPath<String>,
) -> impl IntoResponse {
    match state.git_status(&id).await {
        Ok(entries) => Json(serde_json::json!({ "entries": entries })),
        Err(e) => Json(serde_json::json!({ "error": e.to_string() })),
    }
}

async fn get_git_diff(
    State(state): State<Arc<AppState>>,
    AxPath(id): AxPath<String>,
    Query(q): Query<PathQuery>,
) -> impl IntoResponse {
    match state.git_diff_file(&id, &q.path).await {
        Ok(files) => Json(serde_json::json!({ "path": q.path, "files": files })),
        Err(e) => Json(serde_json::json!({ "error": e.to_string() })),
    }
}

async fn get_git_blame(
    State(state): State<Arc<AppState>>,
    AxPath(id): AxPath<String>,
    Query(q): Query<PathQuery>,
) -> impl IntoResponse {
    match state.git_blame(&id, &q.path).await {
        Ok((lines, truncated)) => {
            Json(serde_json::json!({ "lines": lines, "truncated": truncated }))
        }
        Err(e) => Json(serde_json::json!({ "error": e.to_string() })),
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
        ClientMsg::ListDir { session_id, path } => {
            match state.list_dir(&session_id, &path).await {
                Ok(entries) => state.broadcast(ServerMsg::DirListing {
                    session_id,
                    path,
                    entries,
                }),
                Err(e) => err(e),
            }
        }
        ClientMsg::FetchFile { session_id, path } => {
            match state.read_file(&session_id, &path).await {
                Ok((content, truncated)) => state.broadcast(ServerMsg::FileContent {
                    session_id,
                    path,
                    content,
                    truncated,
                }),
                Err(e) => err(e),
            }
        }
        ClientMsg::ListTree { session_id } => match state.list_tree(&session_id).await {
            Ok((paths, truncated)) => state.broadcast(ServerMsg::TreeListing {
                session_id,
                paths,
                truncated,
            }),
            Err(e) => err(e),
        },
        ClientMsg::SearchContent { session_id, query } => {
            match state.search_content(&session_id, &query).await {
                Ok((matches, truncated)) => state.broadcast(ServerMsg::ContentMatches {
                    session_id,
                    query,
                    matches,
                    truncated,
                }),
                Err(e) => err(e),
            }
        }
        ClientMsg::GitLog {
            session_id,
            skip,
            limit,
        } => match state.git_log(&session_id, skip, limit).await {
            Ok((commits, done)) => state.broadcast(ServerMsg::GitCommits {
                session_id,
                skip,
                commits,
                done,
            }),
            Err(e) => err(e),
        },
        ClientMsg::GitShow { session_id, sha } => match state.git_show(&session_id, &sha).await {
            Ok(files) => state.broadcast(ServerMsg::GitCommitDiff {
                session_id,
                sha,
                files,
            }),
            Err(e) => err(e),
        },
        ClientMsg::GitStatus { session_id } => match state.git_status(&session_id).await {
            Ok(entries) => state.broadcast(ServerMsg::GitLocalChanges {
                session_id,
                entries,
            }),
            Err(e) => err(e),
        },
        ClientMsg::GitDiffFile { session_id, path } => {
            match state.git_diff_file(&session_id, &path).await {
                Ok(files) => state.broadcast(ServerMsg::GitFileDiff {
                    session_id,
                    path,
                    files,
                }),
                Err(e) => err(e),
            }
        }
        ClientMsg::GitBlame { session_id, path } => {
            match state.git_blame(&session_id, &path).await {
                Ok((lines, truncated)) => state.broadcast(ServerMsg::GitAnnotation {
                    session_id,
                    path,
                    lines,
                    truncated,
                }),
                Err(e) => err(e),
            }
        }
    }
}
