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
        .route("/api/health", get(health))
        .route("/api/sessions", get(list_sessions))
        .route("/api/sessions/{id}", get(get_session))
        .route("/api/sessions/{id}/events", get(get_events))
        .route("/api/sessions/{id}/change", get(get_change))
        .route("/api/sessions/{id}/review_all", post(review_all))
        .route("/api/sessions/{id}/review", post(review_hunk))
        .route("/api/queue", get(get_queue))
        // Tokens, never dollars here (ADR-0024 confines cost to Analytics);
        // any threshold inside is an
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
        // same place before attaching to it. `claude_home` and `pid` are kept
        // at the top level as well as inside `daemon` because the window's
        // start-up probe reads them there and probes an *older* daemon too.
        "claude_home": state.claude_home.to_string_lossy(),
        "pid": std::process::id(),
        // Who is answering (R-I5) — the same shape the snapshot carries, so a
        // curl and a client agree about which machine this is.
        "daemon": state.daemon_identity(),
        "headline": h.headline(),
        "blind_ratio": h.blind_ratio(),
        "urgent_alerts": h.urgent_alerts(),
        "alerts": h.alerts.iter().map(|a| a.message()).collect::<Vec<_>>(),
        "detail": h,
    }))
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
    match state.store.load_recent_events(&id, q.since, EVENT_REPLAY_CAP) {
        Ok(evs) => Json(serde_json::to_value(evs).unwrap_or_default()),
        Err(e) => Json(serde_json::json!({ "error": e.to_string() })),
    }
}

async fn get_change(
    State(state): State<Arc<AppState>>,
    AxPath(id): AxPath<String>,
) -> impl IntoResponse {
    match state.change_for_request(&id, false).await {
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

/// Newest events served per replay. Matches the window's own retention cap
/// (`EVENTS_CAP` in the client store) — serving more builds a Vec and a wire
/// frame the receiver immediately trims.
const EVENT_REPLAY_CAP: u64 = 5000;

/// Replies a connection may have queued while its sink is slow. The broadcast
/// lane sheds a lagging client (`Lagged` → reconnect); the reply lane bounds
/// instead — a full lane drops the reply with a warning, and the client's own
/// retry (reconnect, re-select) asks again.
const REPLY_LANE_DEPTH: usize = 256;

async fn ws_upgrade(ws: WebSocketUpgrade, State(state): State<Arc<AppState>>) -> impl IntoResponse {
    ws.on_upgrade(move |socket| ws_conn(socket, state))
}

async fn ws_conn(socket: WebSocket, state: Arc<AppState>) {
    let (mut sink, mut stream) = socket.split();
    let mut rx = state.tx.subscribe();
    // The connection's own lane. `R-J59`. The daemon still *volunteers* state
    // on the broadcast — the client contract is unchanged, answers arrive on
    // the one event stream — but a request whose answer only the asker can use
    // (history replay, a diff's hunks, a snapshot, an error you caused) goes
    // down this lane instead of to every window. Broadcasting those meant one
    // window selecting a long session made every other window parse and store
    // that session's entire history.
    let (reply_tx, mut reply_rx) = tokio::sync::mpsc::channel::<ServerMsg>(REPLY_LANE_DEPTH);

    // Push the full snapshot immediately so a client is useful before it sends
    // anything, and so reconnects self-heal.
    if let Ok(txt) = serde_json::to_string(&state.snapshot().await) {
        if sink.send(Message::Text(txt.into())).await.is_err() {
            return;
        }
    }

    let send_task = tokio::spawn(async move {
        loop {
            // Unbiased select, so neither lane can starve the other. The two
            // lanes carry no ordering promise between them — a reply computed
            // after a broadcast may still be delivered first. Everything on
            // the wire converges regardless (summaries derive from the same
            // cached change a reply carries; session rows re-flush), but a
            // handler that ever *depends* on cross-lane order is a bug.
            let msg = tokio::select! {
                m = reply_rx.recv() => match m {
                    Some(m) => Ok(m),
                    // The request side hung up; the broadcast task ends with it.
                    None => break,
                },
                m = rx.recv() => m,
            };
            match msg {
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
            Ok(cmd) => handle(&state, cmd, &reply_tx).await,
            Err(e) => {
                send_reply(
                    &reply_tx,
                    ServerMsg::Error {
                        message: format!("bad command: {e}"),
                    },
                );
            }
        }
    }

    send_task.abort();
}

/// Queue one message on a connection's reply lane, shedding on overflow: a
/// client whose lane is full is one whose sink has stalled, and its own
/// reconnect or re-ask is the recovery — the daemon must not hold the memory.
fn send_reply(reply: &tokio::sync::mpsc::Sender<ServerMsg>, msg: ServerMsg) {
    if let Err(e) = reply.try_send(msg) {
        tracing::warn!("reply lane full or closed, dropping a reply: {e}");
    }
}

/// Whether a command changes the repository. `R-D19`.
///
/// Enumerated in one function rather than checked at each verb, so that adding
/// a write verb without adding it here is a visible omission in a list of
/// three rather than a missing line in a 45-arm match. The default arm is
/// `false` on purpose: a *read* wrongly listed here is refused on an open
/// bind, which is annoying; a write wrongly omitted is a repository an
/// unauthenticated socket can change.
///
/// See [ADR-0012](../../../docs/decisions/0012-write-locally-never-publish.md).
fn is_write(cmd: &ClientMsg) -> bool {
    matches!(
        cmd,
        ClientMsg::GitStage { .. }
            | ClientMsg::GitUnstage { .. }
            | ClientMsg::GitDiscard { .. }
            | ClientMsg::GitCommit { .. }
            | ClientMsg::GitBranchCreate { .. }
            | ClientMsg::GitSwitch { .. }
            | ClientMsg::GitStashPush { .. }
            | ClientMsg::GitStashPop { .. }
            | ClientMsg::GitStashDrop { .. }
            | ClientMsg::GitResolve { .. }
            // Not a repository write. Gated with them because it is the one
            // verb that reaches a network beyond this machine (ADR-0014), and
            // "an open socket must not be able to make this daemon talk to
            // someone else's server" is the same rule wearing a different hat.
            | ClientMsg::GitFetch { .. }
            // Also not a repository write, and gated for the third version of
            // the same rule: these change **what this daemon will read out**
            // (`R-J40`). A window dialled in from another machine must not be
            // able to extend the read surface of the daemon it is watching.
            | ClientMsg::AddWorkspaceDir { .. }
            | ClientMsg::RemoveWorkspaceDir { .. }
    )
}

/// Answer a write by re-reading the repository. `R-D19`.
///
/// Every write verb ends here, so "what happened" is always git's answer
/// rather than the client's optimism.
async fn rebroadcast_status(state: &Arc<AppState>, session_id: String) {
    match state.git_status(&session_id).await {
        Ok(entries) => state.broadcast(ServerMsg::GitLocalChanges {
            session_id,
            entries,
        }),
        Err(e) => state.broadcast(ServerMsg::Error {
            message: e.to_string(),
        }),
    }
}

/// Answer a ref or stash write by re-reading what it could have moved.
/// `R-D21`.
///
/// The same rule as `R-D19`'s: the client is told what git now says, never
/// what it was asked to do.
async fn after_ref_change(state: &Arc<AppState>, session_id: String) {
    match state.git_refs(&session_id).await {
        Ok(info) => state.broadcast(ServerMsg::GitRefsInfo {
            session_id: session_id.clone(),
            info: Box::new(info),
        }),
        Err(e) => state.broadcast(ServerMsg::Error {
            message: e.to_string(),
        }),
    }
    match state.git_stashes(&session_id).await {
        Ok(stashes) => state.broadcast(ServerMsg::GitStashList {
            session_id: session_id.clone(),
            stashes,
        }),
        Err(e) => state.broadcast(ServerMsg::Error {
            message: e.to_string(),
        }),
    }
    rebroadcast_status(state, session_id).await
}

async fn handle(
    state: &Arc<AppState>,
    cmd: ClientMsg,
    reply: &tokio::sync::mpsc::Sender<ServerMsg>,
) {
    // Your mistake is your toast: an error provoked by one window's command
    // used to be broadcast, so every other window raised it too.
    let err = |e: anyhow::Error| {
        send_reply(
            reply,
            ServerMsg::Error {
                message: e.to_string(),
            },
        );
    };

    // One gate, before dispatch, for every verb that can change a repository.
    // It is redundant with `server::admit`, which refuses to start a daemon
    // that could reach it — and it is here anyway, because `admit` guards the
    // *binary* while this guards the *router*, and the router is what a test,
    // an embedding, or a future entry point actually constructs.
    if is_write(&cmd) && !state.may_write() {
        return err(anyhow::anyhow!(
            "this daemon is not listening on loopback and no token was configured, \
             so it will not write to a repository"
        ));
    }

    match cmd {
        ClientMsg::Subscribe => {
            // Direct, not broadcast: this used to make every *other* window
            // re-ingest the full board whenever any window reconnected —
            // which after a laptop sleep is all of them, at once.
            let snap = state.snapshot().await;
            send_reply(reply, snap);
        }
        ClientMsg::SetHunkReviewed {
            session_id,
            anchor,
            reviewed,
        } => state.set_hunk_reviewed(&session_id, &anchor, reviewed).await,
        ClientMsg::ReviewAll { session_id } => state.review_all(&session_id).await,
        ClientMsg::RefreshChange { session_id, force } => {
            if let Some(change) = state.change_for_request(&session_id, force).await {
                send_reply(reply, ServerMsg::ChangeUpdated { session_id, change });
            }
        }
        ClientMsg::FetchEvents { session_id, since } => {
            // Direct: a history replay can run to tens of thousands of
            // events, and every open window used to receive — and keep —
            // every other window's replays.
            // Newest window only: an unbounded replay of a months-old session
            // built a Vec of everything and one giant frame, which the client
            // trimmed to its own cap on arrival anyway.
            match state
                .store
                .load_recent_events(&session_id, since, EVENT_REPLAY_CAP)
            {
                Ok(events) if !events.is_empty() => {
                    send_reply(reply, ServerMsg::Events { events });
                }
                Ok(_) => {}
                Err(e) => err(anyhow::anyhow!(e)),
            }
        }
        ClientMsg::ForgetSession { session_id } => {
            if let Err(e) = state.forget(&session_id).await {
                err(e);
            }
        }
        // Run and Debug. `R-N4`, `R-N5`.
        //
        // Note what is **not** here: a verb carrying a command. ADR-0025
        // clause 1 is the whole security argument for this feature, and the
        // shape of these four arms is where it is kept.
        ClientMsg::FetchRunConfigs { session_id } => {
            let (configs, unknown) = state.run_configs(&session_id).await;
            if !unknown.is_empty() {
                state.record_unknown_run_types(&unknown).await;
            }
            state.broadcast(ServerMsg::RunConfigs {
                session_id,
                configs,
                allowed: state.runs.allowed(),
            });
        }
        ClientMsg::RunStart {
            session_id,
            config_id,
        } => {
            let (configs, _) = state.run_configs(&session_id).await;
            let Some(repo) = state.run_repo(&session_id).await else {
                err(anyhow::anyhow!("no such session"));
                return;
            };
            match state
                .runs
                .start(&repo, &config_id, Some(session_id), &configs)
                .await
            {
                Ok(_) => {}
                // A refusal is not a crash and reads as one if it arrives as a
                // bare error, so ADR-0025's own wording travels intact — to
                // the window that asked, not to every window.
                Err(why) => send_reply(reply, ServerMsg::Error { message: why }),
            }
        }
        ClientMsg::RunStop { run_id } => {
            if let Err(why) = state.runs.stop(&run_id).await {
                send_reply(reply, ServerMsg::Error { message: why });
            }
        }
        ClientMsg::RevealRunEnv {
            session_id,
            config_id,
            key,
        } => {
            let Some(repo) = state.run_repo(&session_id).await else {
                err(anyhow::anyhow!("no such session"));
                return;
            };
            let value = tokio::task::spawn_blocking({
                let (repo, config_id, key) = (repo, config_id.clone(), key.clone());
                move || {
                    crate::runconfig::env_for(&repo, &config_id)
                        .into_iter()
                        .find(|(k, _)| *k == key)
                        .map(|(_, v)| v)
                }
            })
            .await
            .ok()
            .flatten();
            match value {
                // To the asker alone — this is a *revealed secret* (`R-N6`),
                // and broadcasting it put the value in front of every
                // connected window, asked or not.
                Some(value) => send_reply(
                    reply,
                    ServerMsg::RunEnvValue {
                        config_id,
                        key,
                        value,
                    },
                ),
                None => err(anyhow::anyhow!("no `{key}` in that configuration")),
            }
        }
        ClientMsg::FetchRunOutput { run_id } => {
            // A history replay is the asker's alone, same as `FetchEvents`.
            let lines = state.runs.lines(&run_id).await;
            send_reply(reply, ServerMsg::RunOutputHistory { run_id, lines });
        }
        ClientMsg::LaunchTerminal {
            dir,
            worktree,
            source,
        } => {
            if let Err(e) = state.launch_terminal(&dir, worktree, source).await {
                err(e);
            }
        }
        ClientMsg::Rescan => {
            state.scan().await;
            // The scan's own queue and health publishes are gated on change;
            // the client that asked still gets both back, changed or not —
            // its "rescanning…" spinner clears on the health message.
            state.republish_queue().await;
            state.republish_health().await;
        }
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
        ClientMsg::FetchKit => {
            let entries = crate::kit::scan(&state.claude_home);
            state.broadcast(ServerMsg::Kit { entries });
        }
        ClientMsg::FetchKitDoc { path } => match crate::kit::read_doc(&state.claude_home, &path) {
            Ok(doc) => state.broadcast(ServerMsg::KitDoc { doc }),
            Err(e) => err(anyhow::anyhow!(e)),
        },
        ClientMsg::FetchDocScan { repo } => match state.doc_scan(&repo).await {
            Ok(inventory) => state.broadcast(ServerMsg::DocReport {
                repo,
                inventory: Box::new(inventory),
            }),
            Err(e) => err(e),
        },
        ClientMsg::FetchHealth => {
            let health = state.health().await;
            send_reply(
                reply,
                ServerMsg::Health {
                    health: Box::new(health),
                },
            );
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
        ClientMsg::OpenFolder { session_id } => {
            if let Err(e) = state.open_folder(&session_id).await {
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
        ClientMsg::FetchWorkspace { session_id } => match state.workspace(&session_id).await {
            Ok(view) => state.broadcast(view.into_msg(session_id)),
            Err(e) => err(e),
        },
        ClientMsg::AddWorkspaceDir { session_id, path } => {
            match state.add_workspace_dir(&session_id, &path).await {
                // Answered with the whole workspace rather than an
                // acknowledgement: every client watching this session needs
                // the new root, and one message that carries the truth beats
                // two that have to agree.
                Ok(()) => match state.workspace(&session_id).await {
                    Ok(view) => state.broadcast(view.into_msg(session_id)),
                    Err(e) => err(e),
                },
                Err(e) => err(e),
            }
        }
        ClientMsg::RemoveWorkspaceDir { session_id, path } => {
            match state.remove_workspace_dir(&session_id, &path).await {
                Ok(()) => match state.workspace(&session_id).await {
                    Ok(view) => state.broadcast(view.into_msg(session_id)),
                    Err(e) => err(e),
                },
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

        // -- The write family. `R-D19`.
        //
        // Each answers by re-reading and re-broadcasting the status, rather
        // than by reporting what it did. The client therefore never models
        // repository state locally and the two cannot drift — the pane shows
        // what git says, one round trip after the click, including when git
        // did something other than what was asked.
        ClientMsg::GitStage { session_id, paths } => {
            match state.git_stage(&session_id, paths).await {
                Ok(()) => rebroadcast_status(state, session_id).await,
                Err(e) => err(e),
            }
        }
        ClientMsg::GitUnstage { session_id, paths } => {
            match state.git_unstage(&session_id, paths).await {
                Ok(()) => rebroadcast_status(state, session_id).await,
                Err(e) => err(e),
            }
        }
        ClientMsg::GitCommit {
            session_id,
            message,
            amend,
            session_trailer,
        } => {
            match state
                .git_commit(&session_id, message, amend, session_trailer)
                .await
            {
                Ok(_sha) => {
                    // The staged files are gone from the working tree's point
                    // of view, and the session's diff was computed against a
                    // base that has just moved. The log is the client's own
                    // problem: it knows it asked for a commit, so it re-asks
                    // rather than being told — which keeps a `GitLogStale`
                    // message that would exist for exactly one caller out of
                    // the protocol.
                    state.recompute_change(&session_id).await;
                    rebroadcast_status(state, session_id).await
                }
                Err(e) => err(e),
            }
        }
        ClientMsg::GitBranchCreate {
            session_id,
            name,
            switch_to,
        } => match state.git_branch_create(&session_id, name, switch_to).await {
            // Refs *and* status: creating a branch changes the refs list, and
            // switching onto it can change what is uncommitted.
            Ok(()) => after_ref_change(state, session_id).await,
            Err(e) => err(e),
        },
        ClientMsg::GitSwitch { session_id, name } => {
            match state.git_switch(&session_id, name).await {
                Ok(()) => {
                    state.recompute_change(&session_id).await;
                    after_ref_change(state, session_id).await
                }
                Err(e) => err(e),
            }
        }
        ClientMsg::GitStashPush {
            session_id,
            message,
            include_untracked,
        } => match state
            .git_stash_push(&session_id, message, include_untracked)
            .await
        {
            Ok(()) => {
                state.recompute_change(&session_id).await;
                after_ref_change(state, session_id).await
            }
            Err(e) => err(e),
        },
        ClientMsg::GitStashPop { session_id, index } => {
            match state.git_stash_pop(&session_id, index).await {
                Ok(()) => {
                    state.recompute_change(&session_id).await;
                    after_ref_change(state, session_id).await
                }
                Err(e) => err(e),
            }
        }
        ClientMsg::GitStashDrop { session_id, index } => {
            match state.git_stash_drop(&session_id, index).await {
                // Nothing in the worktree moved, so only the stash list did.
                Ok(()) => after_ref_change(state, session_id).await,
                Err(e) => err(e),
            }
        }
        ClientMsg::GitFetch { session_id } => match state.git_fetch(&session_id).await {
            Ok(r) => {
                state.broadcast(ServerMsg::GitFetched {
                    session_id: session_id.clone(),
                    updates: r.updates,
                    upstream: r.upstream,
                    ahead: r.ahead,
                    behind: r.behind,
                });
                // Ahead/behind live on the refs answer too, so the lists
                // behind the popup agree with what it just said.
                after_ref_change(state, session_id).await
            }
            Err(e) => err(e),
        },
        // -- Notes. `R-B35`. Every one answers with the whole set, so two
        // windows on one daemon cannot drift — the property daemon ownership
        // was chosen for (ADR-0015).
        ClientMsg::NoteList => match state.notes().await {
            Ok(notes) => state.broadcast(ServerMsg::Notes { notes }),
            Err(e) => err(e),
        },
        ClientMsg::NoteSave {
            id,
            body,
            session_id,
            seq,
            repo,
        } => match state.save_note(id, body, session_id, seq, repo).await {
            Ok(notes) => state.broadcast(ServerMsg::Notes { notes }),
            Err(e) => err(e),
        },
        ClientMsg::NoteDelete { id } => match state.delete_note(&id).await {
            Ok(notes) => state.broadcast(ServerMsg::Notes { notes }),
            Err(e) => err(e),
        },
        ClientMsg::GitResolve {
            session_id,
            path,
            side,
        } => match state.git_resolve(&session_id, path, side).await {
            Ok(()) => {
                state.recompute_change(&session_id).await;
                rebroadcast_status(state, session_id).await
            }
            Err(e) => err(e),
        },
        ClientMsg::GitDiscard { session_id, paths } => {
            match state.git_discard(&session_id, paths).await {
                Ok(()) => {
                    // The diff the Changes tab is showing was computed from
                    // files that may no longer exist. Recomputing is not
                    // decoration: a stale hunk offers a "discard" of something
                    // already gone.
                    state.recompute_change(&session_id).await;
                    rebroadcast_status(state, session_id).await
                }
                Err(e) => err(e),
            }
        }
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

#[cfg(test)]
mod write_guard_tests {
    use super::*;

    /// The enumeration in `is_write` is the single point of failure this whole
    /// guard has: one verb missing from it and an unauthenticated socket can
    /// change a repository. So this asserts the list from both directions —
    /// every write is in it, and the reads that sit closest to them are not.
    #[test]
    fn every_write_verb_is_named_and_no_read_is() {
        let id = || "s".to_string();
        let paths = || vec!["a.rs".to_string()];

        for cmd in [
            ClientMsg::GitStage { session_id: id(), paths: paths() },
            ClientMsg::GitUnstage { session_id: id(), paths: paths() },
            ClientMsg::GitDiscard { session_id: id(), paths: paths() },
            ClientMsg::GitCommit {
                session_id: id(),
                message: "m".into(),
                amend: false,
                session_trailer: true,
            },
            ClientMsg::GitBranchCreate {
                session_id: id(),
                name: "b".into(),
                switch_to: true,
            },
            ClientMsg::GitSwitch { session_id: id(), name: "b".into() },
            ClientMsg::GitStashPush {
                session_id: id(),
                message: String::new(),
                include_untracked: true,
            },
            ClientMsg::GitStashPop { session_id: id(), index: 0 },
            ClientMsg::GitStashDrop { session_id: id(), index: 0 },
            ClientMsg::GitResolve {
                session_id: id(),
                path: "a.rs".into(),
                side: mogeung_core::wire::ResolveSide::Ours,
            },
            // Not a repository write, but gated with them deliberately: it is
            // the one verb that reaches a network beyond this machine
            // (ADR-0014), and an open socket must not be able to make this
            // daemon talk to someone else's server.
            ClientMsg::GitFetch { session_id: id() },
        ] {
            assert!(is_write(&cmd), "{cmd:?} changes the repository");
        }

        // The neighbours. `GitStatus` especially: it is the answer every write
        // verb broadcasts, so gating it would make a successful write look
        // like a failed one.
        for cmd in [
            ClientMsg::GitStatus { session_id: id() },
            ClientMsg::GitRefs { session_id: id() },
            ClientMsg::GitStashes { session_id: id() },
            ClientMsg::RefreshChange { session_id: id(), force: true },
            ClientMsg::ReviewAll { session_id: id() },
            ClientMsg::Subscribe,
        ] {
            assert!(!is_write(&cmd), "{cmd:?} only reads");
        }
    }

    /// `ForgetSession` is destructive and deliberately *not* a repository
    /// write: it drops mogeung's own records and touches no file git owns.
    /// Worth pinning, because "destructive" and "writes the repo" are the two
    /// things most likely to be conflated by whoever extends this next.
    #[test]
    fn forgetting_a_session_is_not_a_repository_write() {
        assert!(!is_write(&ClientMsg::ForgetSession {
            session_id: "s".into()
        }));
    }
}
