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

/// The router, optionally behind a shared token — `R-I4`.
///
/// The token rides `Authorization: Bearer …` or, for WebSocket clients that
/// cannot set headers, a `token=` query parameter. Comparison is
/// constant-time; a miss is a plain 401, never a hang. No token configured
/// means the historical open-on-loopback behaviour, unchanged.
pub fn router_with_token(state: Arc<AppState>, token: Option<String>) -> Router {
    let r = router(state);
    match token.filter(|t| !t.is_empty()) {
        None => r,
        Some(t) => r.layer(axum::middleware::from_fn(
            move |req: axum::extract::Request, next: axum::middleware::Next| {
                let t = t.clone();
                async move {
                    if request_authorized(&req, &t) {
                        next.run(req).await
                    } else {
                        (
                            axum::http::StatusCode::UNAUTHORIZED,
                            "missing or wrong token",
                        )
                            .into_response()
                    }
                }
            },
        )),
    }
}

fn request_authorized(req: &axum::extract::Request, token: &str) -> bool {
    let header_ok = req
        .headers()
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .map(|v| constant_time_eq(v, token))
        .unwrap_or(false);
    if header_ok {
        return true;
    }
    req.uri()
        .query()
        .map(|q| {
            q.split('&')
                .filter_map(|kv| kv.split_once('='))
                .any(|(k, v)| k == "token" && constant_time_eq(v, token))
        })
        .unwrap_or(false)
}

/// Length-leaking only — every byte is compared regardless of mismatches, so
/// timing does not walk the token one prefix at a time.
fn constant_time_eq(a: &str, b: &str) -> bool {
    let (a, b) = (a.as_bytes(), b.as_bytes());
    if a.len() != b.len() {
        return false;
    }
    a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

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
        // Tokens, never dollars (ADR-0005); any threshold inside is an
        // estimate from observed limit hits, not a quota. R-G1–R-G3.
        .route("/api/usage", get(get_usage))
        // Cross-session intelligence (pillar F) and docs (pillar H) — the
        // WS commands' curl-able twins, read-only like everything here.
        .route("/api/insight/search", get(get_insight_search))
        .route("/api/insight/digest", get(get_insight_digest))
        .route("/api/insight/recurring", get(get_insight_recurring))
        .route("/api/insight/prompts", get(get_insight_prompts))
        .route("/api/insight/analytics", get(get_insight_analytics))
        .route("/api/insight/file", get(get_insight_file))
        .route("/api/sessions/{id}/subagents", get(get_subagents))
        .route("/api/sessions/{id}/decisions", get(get_decisions))
        .route("/api/repos/{repo}/docscan", get(get_docscan))
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
        .route("/api/sessions/{id}/git/refs", get(get_git_refs))
        .route("/api/sessions/{id}/git/stashes", get(get_git_stashes))
        .route("/api/sessions/{id}/git/stash", get(get_git_stash))
        .route("/api/sessions/{id}/git/submodules", get(get_git_submodules))
        .route("/api/sessions/{id}/git/range", get(get_git_range))
        .route("/api/sessions/{id}/git/compare", get(get_git_compare))
        .route("/api/sessions/{id}/git/reflog", get(get_git_reflog))
        .route("/api/sessions/{id}/git/worktrees", get(get_git_worktrees))
        .route("/api/sessions/{id}/git/conflict", get(get_git_conflict))
        .route("/api/sessions/{id}/git/file_at", get(get_git_file_at))
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

async fn get_usage(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let report = state.usage_report().await;
    Json(serde_json::to_value(report).unwrap_or_default())
}

#[derive(Deserialize)]
struct InsightSearchQuery {
    q: String,
}

async fn get_insight_search(
    State(state): State<Arc<AppState>>,
    Query(q): Query<InsightSearchQuery>,
) -> impl IntoResponse {
    Json(serde_json::to_value(state.insight_search(q.q).await).unwrap_or_default())
}

#[derive(Deserialize)]
struct DayQuery {
    day: String,
}

async fn get_insight_digest(
    State(state): State<Arc<AppState>>,
    Query(q): Query<DayQuery>,
) -> impl IntoResponse {
    Json(serde_json::to_value(state.day_digest(q.day).await).unwrap_or_default())
}

async fn get_insight_recurring(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    Json(serde_json::to_value(state.recurring_failures().await).unwrap_or_default())
}

async fn get_insight_prompts(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    Json(serde_json::to_value(state.prompt_library().await).unwrap_or_default())
}

async fn get_insight_analytics(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    Json(serde_json::to_value(state.analytics().await).unwrap_or_default())
}

async fn get_insight_file(
    State(state): State<Arc<AppState>>,
    Query(q): Query<PathQuery>,
) -> impl IntoResponse {
    Json(serde_json::to_value(state.file_sessions(&q.path).await).unwrap_or_default())
}

async fn get_subagents(
    State(state): State<Arc<AppState>>,
    AxPath(id): AxPath<String>,
) -> impl IntoResponse {
    Json(serde_json::to_value(state.subagent_tree(&id).await).unwrap_or_default())
}

async fn get_decisions(
    State(state): State<Arc<AppState>>,
    AxPath(id): AxPath<String>,
) -> impl IntoResponse {
    Json(serde_json::to_value(state.decision_candidates(&id).await).unwrap_or_default())
}

async fn get_docscan(
    State(state): State<Arc<AppState>>,
    AxPath(repo): AxPath<String>,
) -> impl IntoResponse {
    match state.doc_scan(&repo).await {
        Ok(inv) => Json(serde_json::to_value(inv).unwrap_or_default()),
        Err(e) => Json(serde_json::json!({ "error": e.to_string() })),
    }
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
    #[serde(default)]
    rev: Option<String>,
    #[serde(default)]
    grep: Option<String>,
    #[serde(default)]
    author: Option<String>,
    #[serde(default)]
    path: Option<String>,
    #[serde(default)]
    pickaxe: Option<String>,
}

fn default_log_limit() -> u32 {
    50
}

async fn get_git_log(
    State(state): State<Arc<AppState>>,
    AxPath(id): AxPath<String>,
    Query(q): Query<LogQuery>,
) -> impl IntoResponse {
    let filter = crate::git::LogFilter {
        grep: q.grep,
        author: q.author,
        path: q.path,
        pickaxe: q.pickaxe,
    };
    match state.git_log(&id, q.skip, q.limit, q.rev, filter).await {
        Ok((commits, done)) => Json(serde_json::json!({ "commits": commits, "done": done })),
        Err(e) => Json(serde_json::json!({ "error": e.to_string() })),
    }
}

#[derive(Deserialize)]
struct ShaQuery {
    sha: String,
    #[serde(default)]
    context: Option<u32>,
    #[serde(default)]
    ignore_ws: Option<bool>,
}

async fn get_git_show(
    State(state): State<Arc<AppState>>,
    AxPath(id): AxPath<String>,
    Query(q): Query<ShaQuery>,
) -> impl IntoResponse {
    let opts = crate::git::DiffOpts::from_wire(q.context, q.ignore_ws);
    match state.git_show(&id, &q.sha, opts).await {
        Ok((files, detail)) => Json(serde_json::json!({
            "sha": q.sha, "files": files, "detail": detail,
        })),
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

#[derive(Deserialize)]
struct DiffFileQuery {
    path: String,
    #[serde(default)]
    context: Option<u32>,
    #[serde(default)]
    ignore_ws: Option<bool>,
}

async fn get_git_diff(
    State(state): State<Arc<AppState>>,
    AxPath(id): AxPath<String>,
    Query(q): Query<DiffFileQuery>,
) -> impl IntoResponse {
    let opts = crate::git::DiffOpts::from_wire(q.context, q.ignore_ws);
    match state.git_diff_file(&id, &q.path, opts).await {
        Ok(files) => Json(serde_json::json!({ "path": q.path, "files": files })),
        Err(e) => Json(serde_json::json!({ "error": e.to_string() })),
    }
}

#[derive(Deserialize)]
struct BlameQuery {
    path: String,
    #[serde(default)]
    rev: Option<String>,
}

async fn get_git_blame(
    State(state): State<Arc<AppState>>,
    AxPath(id): AxPath<String>,
    Query(q): Query<BlameQuery>,
) -> impl IntoResponse {
    match state.git_blame(&id, &q.path, q.rev).await {
        Ok((lines, truncated)) => {
            Json(serde_json::json!({ "lines": lines, "truncated": truncated }))
        }
        Err(e) => Json(serde_json::json!({ "error": e.to_string() })),
    }
}

async fn get_git_refs(
    State(state): State<Arc<AppState>>,
    AxPath(id): AxPath<String>,
) -> impl IntoResponse {
    match state.git_refs(&id).await {
        Ok(info) => Json(serde_json::json!(info)),
        Err(e) => Json(serde_json::json!({ "error": e.to_string() })),
    }
}

async fn get_git_stashes(
    State(state): State<Arc<AppState>>,
    AxPath(id): AxPath<String>,
) -> impl IntoResponse {
    match state.git_stashes(&id).await {
        Ok(stashes) => Json(serde_json::json!({ "stashes": stashes })),
        Err(e) => Json(serde_json::json!({ "error": e.to_string() })),
    }
}

#[derive(Deserialize)]
struct StashQuery {
    index: u32,
    #[serde(default)]
    context: Option<u32>,
    #[serde(default)]
    ignore_ws: Option<bool>,
}

async fn get_git_stash(
    State(state): State<Arc<AppState>>,
    AxPath(id): AxPath<String>,
    Query(q): Query<StashQuery>,
) -> impl IntoResponse {
    let opts = crate::git::DiffOpts::from_wire(q.context, q.ignore_ws);
    match state.git_stash_show(&id, q.index, opts).await {
        Ok(files) => Json(serde_json::json!({ "index": q.index, "files": files })),
        Err(e) => Json(serde_json::json!({ "error": e.to_string() })),
    }
}

async fn get_git_submodules(
    State(state): State<Arc<AppState>>,
    AxPath(id): AxPath<String>,
) -> impl IntoResponse {
    match state.git_submodules(&id).await {
        Ok(submodules) => Json(serde_json::json!({ "submodules": submodules })),
        Err(e) => Json(serde_json::json!({ "error": e.to_string() })),
    }
}

#[derive(Deserialize)]
struct RangeQuery {
    from: String,
    to: String,
    #[serde(default)]
    context: Option<u32>,
    #[serde(default)]
    ignore_ws: Option<bool>,
}

async fn get_git_range(
    State(state): State<Arc<AppState>>,
    AxPath(id): AxPath<String>,
    Query(q): Query<RangeQuery>,
) -> impl IntoResponse {
    let opts = crate::git::DiffOpts::from_wire(q.context, q.ignore_ws);
    match state.git_diff_range(&id, &q.from, &q.to, opts).await {
        Ok(files) => {
            Json(serde_json::json!({ "from": q.from, "to": q.to, "files": files }))
        }
        Err(e) => Json(serde_json::json!({ "error": e.to_string() })),
    }
}

#[derive(Deserialize)]
struct BranchQuery {
    branch: String,
}

async fn get_git_compare(
    State(state): State<Arc<AppState>>,
    AxPath(id): AxPath<String>,
    Query(q): Query<BranchQuery>,
) -> impl IntoResponse {
    match state.git_compare(&id, &q.branch).await {
        Ok((from, to, files)) => {
            Json(serde_json::json!({ "from": from, "to": to, "files": files }))
        }
        Err(e) => Json(serde_json::json!({ "error": e.to_string() })),
    }
}

async fn get_git_reflog(
    State(state): State<Arc<AppState>>,
    AxPath(id): AxPath<String>,
) -> impl IntoResponse {
    match state.git_reflog(&id).await {
        Ok(entries) => Json(serde_json::json!({ "entries": entries })),
        Err(e) => Json(serde_json::json!({ "error": e.to_string() })),
    }
}

async fn get_git_worktrees(
    State(state): State<Arc<AppState>>,
    AxPath(id): AxPath<String>,
) -> impl IntoResponse {
    match state.git_worktrees(&id).await {
        Ok(worktrees) => Json(serde_json::json!({ "worktrees": worktrees })),
        Err(e) => Json(serde_json::json!({ "error": e.to_string() })),
    }
}

async fn get_git_conflict(
    State(state): State<Arc<AppState>>,
    AxPath(id): AxPath<String>,
    Query(q): Query<PathQuery>,
) -> impl IntoResponse {
    match state.git_conflict_stages(&id, &q.path).await {
        Ok((base, ours, theirs, truncated)) => Json(serde_json::json!({
            "path": q.path, "base": base, "ours": ours, "theirs": theirs,
            "truncated": truncated,
        })),
        Err(e) => Json(serde_json::json!({ "error": e.to_string() })),
    }
}

#[derive(Deserialize)]
struct FileAtQuery {
    sha: String,
    path: String,
}

async fn get_git_file_at(
    State(state): State<Arc<AppState>>,
    AxPath(id): AxPath<String>,
    Query(q): Query<FileAtQuery>,
) -> impl IntoResponse {
    match state.git_file_at_rev(&id, &q.sha, &q.path).await {
        Ok((content, truncated)) => Json(serde_json::json!({
            "sha": q.sha, "path": q.path, "content": content, "truncated": truncated,
        })),
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
        ClientMsg::FetchUsage => {
            let report = state.usage_report().await;
            state.broadcast(ServerMsg::UsageStats {
                report: Box::new(report),
            });
        }
        ClientMsg::SetSignalCommand { repo, command } => {
            state.set_signal_command(&repo, &command).await;
        }
        ClientMsg::RunSignal { session_id } => {
            if let Err(e) = state.run_signal(&session_id).await {
                err(e);
            }
        }
        ClientMsg::FetchSignal { repo } => {
            let status = state.signal_status(&repo).await;
            state.broadcast(status);
        }
        ClientMsg::InsightSearch { query } => {
            let results = state.insight_search(query.clone()).await;
            state.broadcast(ServerMsg::InsightSearchResults {
                query,
                results: Box::new(results),
            });
        }
        ClientMsg::FetchDigest { day } => {
            let digest = state.day_digest(day.clone()).await;
            state.broadcast(ServerMsg::DayDigestReport {
                day,
                digest: Box::new(digest),
            });
        }
        ClientMsg::FetchRecurring => {
            let failures = state.recurring_failures().await;
            state.broadcast(ServerMsg::RecurringFailures { failures });
        }
        ClientMsg::FetchPromptLibrary => {
            let clusters = state.prompt_library().await;
            state.broadcast(ServerMsg::PromptLibrary { clusters });
        }
        ClientMsg::FetchAnalytics => {
            let analytics = state.analytics().await;
            state.broadcast(ServerMsg::AnalyticsReport {
                analytics: Box::new(analytics),
            });
        }
        ClientMsg::FetchSubagents { session_id } => {
            let nodes = state.subagent_tree(&session_id).await;
            state.broadcast(ServerMsg::SubagentTreeReport { session_id, nodes });
        }
        ClientMsg::FetchDecisions { session_id } => {
            let candidates = state.decision_candidates(&session_id).await;
            state.broadcast(ServerMsg::DecisionReport {
                session_id,
                candidates,
            });
        }
        ClientMsg::FetchFileSessions { path } => {
            let entries = state.file_sessions(&path).await;
            state.broadcast(ServerMsg::FileSessions { path, entries });
        }
        ClientMsg::FetchDocScan { repo } => match state.doc_scan(&repo).await {
            Ok(inventory) => state.broadcast(ServerMsg::DocReport {
                repo,
                inventory: Box::new(inventory),
            }),
            Err(e) => err(e),
        },
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
            rev,
            grep,
            author,
            path,
            pickaxe,
        } => {
            let filter = crate::git::LogFilter {
                grep: grep.clone(),
                author: author.clone(),
                path: path.clone(),
                pickaxe: pickaxe.clone(),
            };
            match state
                .git_log(&session_id, skip, limit, rev.clone(), filter)
                .await
            {
                Ok((commits, done)) => state.broadcast(ServerMsg::GitCommits {
                    session_id,
                    skip,
                    commits,
                    done,
                    rev,
                    grep,
                    author,
                    path,
                    pickaxe,
                }),
                Err(e) => err(e),
            }
        }
        ClientMsg::GitShow {
            session_id,
            sha,
            context,
            ignore_ws,
        } => {
            let opts = crate::git::DiffOpts::from_wire(context, ignore_ws);
            match state.git_show(&session_id, &sha, opts).await {
                Ok((files, detail)) => state.broadcast(ServerMsg::GitCommitDiff {
                    session_id,
                    sha,
                    files,
                    detail: detail.map(Box::new),
                    context,
                    ignore_ws,
                }),
                Err(e) => err(e),
            }
        }
        ClientMsg::GitStatus { session_id } => match state.git_status(&session_id).await {
            Ok(entries) => state.broadcast(ServerMsg::GitLocalChanges {
                session_id,
                entries,
            }),
            Err(e) => err(e),
        },
        ClientMsg::GitDiffFile {
            session_id,
            path,
            context,
            ignore_ws,
        } => {
            let opts = crate::git::DiffOpts::from_wire(context, ignore_ws);
            match state.git_diff_file(&session_id, &path, opts).await {
                Ok(files) => state.broadcast(ServerMsg::GitFileDiff {
                    session_id,
                    path,
                    files,
                    context,
                    ignore_ws,
                }),
                Err(e) => err(e),
            }
        }
        ClientMsg::GitBlame {
            session_id,
            path,
            rev,
        } => match state.git_blame(&session_id, &path, rev.clone()).await {
            Ok((lines, truncated)) => state.broadcast(ServerMsg::GitAnnotation {
                session_id,
                path,
                lines,
                truncated,
                rev,
            }),
            Err(e) => err(e),
        },
        ClientMsg::GitRefs { session_id } => match state.git_refs(&session_id).await {
            Ok(info) => state.broadcast(ServerMsg::GitRefsInfo {
                session_id,
                info: Box::new(info),
            }),
            Err(e) => err(e),
        },
        ClientMsg::GitStashes { session_id } => match state.git_stashes(&session_id).await {
            Ok(stashes) => state.broadcast(ServerMsg::GitStashList {
                session_id,
                stashes,
            }),
            Err(e) => err(e),
        },
        ClientMsg::GitStashShow {
            session_id,
            index,
            context,
            ignore_ws,
        } => {
            let opts = crate::git::DiffOpts::from_wire(context, ignore_ws);
            match state.git_stash_show(&session_id, index, opts).await {
                Ok(files) => state.broadcast(ServerMsg::GitStashDiff {
                    session_id,
                    index,
                    files,
                    context,
                    ignore_ws,
                }),
                Err(e) => err(e),
            }
        }
        ClientMsg::GitSubmodules { session_id } => {
            match state.git_submodules(&session_id).await {
                Ok(submodules) => state.broadcast(ServerMsg::GitSubmoduleList {
                    session_id,
                    submodules,
                }),
                Err(e) => err(e),
            }
        }
        ClientMsg::GitDiffRange {
            session_id,
            from,
            to,
            context,
            ignore_ws,
        } => {
            let opts = crate::git::DiffOpts::from_wire(context, ignore_ws);
            match state.git_diff_range(&session_id, &from, &to, opts).await {
                Ok(files) => state.broadcast(ServerMsg::GitRangeDiff {
                    session_id,
                    from,
                    to,
                    files,
                    context,
                    ignore_ws,
                }),
                Err(e) => err(e),
            }
        }
        ClientMsg::GitCompare { session_id, branch } => {
            match state.git_compare(&session_id, &branch).await {
                Ok((from, to, files)) => state.broadcast(ServerMsg::GitRangeDiff {
                    session_id,
                    from,
                    to,
                    files,
                    context: None,
                    ignore_ws: None,
                }),
                Err(e) => err(e),
            }
        }
        ClientMsg::GitReflog { session_id } => match state.git_reflog(&session_id).await {
            Ok(entries) => state.broadcast(ServerMsg::GitReflogList {
                session_id,
                entries,
            }),
            Err(e) => err(e),
        },
        ClientMsg::GitWorktrees { session_id } => {
            match state.git_worktrees(&session_id).await {
                Ok(worktrees) => state.broadcast(ServerMsg::GitWorktreeList {
                    session_id,
                    worktrees,
                }),
                Err(e) => err(e),
            }
        }
        ClientMsg::GitConflictFile { session_id, path } => {
            match state.git_conflict_stages(&session_id, &path).await {
                Ok((base, ours, theirs, truncated)) => {
                    state.broadcast(ServerMsg::GitConflictStages {
                        session_id,
                        path,
                        base,
                        ours,
                        theirs,
                        truncated,
                    })
                }
                Err(e) => err(e),
            }
        }
        ClientMsg::GitFileAtRev {
            session_id,
            sha,
            path,
        } => match state.git_file_at_rev(&session_id, &sha, &path).await {
            Ok((content, truncated)) => state.broadcast(ServerMsg::GitFileAtRevContent {
                session_id,
                sha,
                path,
                content,
                truncated,
            }),
            Err(e) => err(e),
        },
    }
}
