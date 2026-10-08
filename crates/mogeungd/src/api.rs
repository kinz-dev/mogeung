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
    // Raw **or** decoded. A client that composes the URL properly — the
    // window's `dialledUrl`, through `URLSearchParams` — percent-encodes a
    // token with a `/`, `+`, `=` or a space in it, and until 2026-09-22 this
    // compared the encoded bytes against the plain token and refused every
    // such token, while a hand-typed raw one passed. Found on the first
    // `R-I19` Test pressed against a real daemon. The raw form is still
    // accepted, so a URL typed with a literal `+` keeps working.
    req.uri()
        .query()
        .map(|q| {
            q.split('&').filter_map(|kv| kv.split_once('=')).any(|(k, v)| {
                k == "token" && (constant_time_eq(v, token) || constant_time_eq(&percent_decode(v), token))
            })
        })
        .unwrap_or(false)
}

/// `%XX` to the byte, `+` to a space, anything malformed left as it was —
/// the decoding a `URLSearchParams` value needs, and no more. Lossy on
/// invalid UTF-8, which a token never is.
fn percent_decode(v: &str) -> String {
    let bytes = v.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'+' => out.push(b' '),
            // Bytes, not a `&str` slice: a `%` followed by a multibyte
            // character would make the slice panic, and a token is input.
            b'%' if i + 2 < bytes.len() => {
                let hex = std::str::from_utf8(&bytes[i + 1..i + 3]).ok();
                match hex.and_then(|h| u8::from_str_radix(h, 16).ok()) {
                    Some(b) => {
                        out.push(b);
                        i += 3;
                        continue;
                    }
                    None => out.push(b'%'),
                }
            }
            b => out.push(b),
        }
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
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
/// Order one session's changed files with a model. `R-O3`.
///
/// The diff comes from the daemon's own state rather than from the client, so
/// the guide is about the same change the Changes view is showing. Every
/// failure arrives as `error` on the same message: a panel that shows nothing
/// reads as broken, which is `R-O5`'s lesson applied one surface over.
async fn reading_guide(state: &AppState, session_id: String, from_this_machine: bool) -> ServerMsg {
    let fail = |e: String| ServerMsg::ReadingGuideReady {
        session_id: session_id.clone(),
        files: Vec::new(),
        summary: String::new(),
        model: String::new(),
        elapsed_ms: 0,
        error: Some(e),
    };

    let Some(change) = state.changes.read().await.get(&session_id).cloned() else {
        return fail("no diff for that session yet".into());
    };
    let asked = crate::guide::askable(&change.files);
    if asked.is_empty() {
        return fail("nothing in this diff to order".into());
    }

    let prompt = crate::guide::prompt(&asked);
    match state
        .model
        .chat_as(&[mogeung_core::model::ChatTurn::user(prompt)], from_this_machine)
        .await
    {
        Ok(a) => {
            let order = crate::guide::parse_order(&a.text, &asked);
            ServerMsg::ReadingGuideReady {
                session_id,
                files: crate::guide::assemble(&asked, &order),
                summary: crate::guide::parse_summary(&a.text),
                model: a.model,
                elapsed_ms: a.elapsed_ms,
                error: None,
            }
        }
        Err(e) => fail(e),
    }
}

/// Why the chat history is unavailable, if it is. `R-O9`, ADR-0032.
///
/// Two reasons and they are different sentences, because they send you
/// somewhere different: a bind beyond loopback is a property of how this
/// daemon was started, and `chat_history = false` is a line you wrote.
fn chat_refusal(state: &AppState, from_this_machine: bool) -> Option<String> {
    if !state.model.chat_allowed() && !from_this_machine {
        return Some(
            "the chat history is not served by a daemon bound beyond loopback              (ADR-0031 clause 4): it holds the same free-form text the ask              does. Bind 127.0.0.1 and reach it over ssh."
                .to_string(),
        );
    }
    // Unset reads as on, matching `Options`' default and the file's
    // absent-means-yes.
    if !*state.chat_history.get().unwrap_or(&true) {
        return Some(
            "not keeping conversations — `chat_history = false` in              ~/.mogeung/config.toml. Anything kept before that line was              written is still here; turning the tap off does not empty the              bucket."
                .to_string(),
        );
    }
    None
}

/// The kept conversations, or the sentence saying why there are none.
fn chat_history_msg(state: &AppState, from_this_machine: bool) -> ServerMsg {
    let refusal = chat_refusal(state, from_this_machine);
    ServerMsg::ChatHistory {
        // Still listed when history is *off*: what was kept before the line
        // was written is still kept, and a list that hid it would be lying
        // about what is on disk — and would take away the only way to delete
        // it. The bind refusal is the one that returns nothing.
        chats: if state.model.chat_allowed() || from_this_machine {
            state.store.load_chats().unwrap_or_default()
        } else {
            Vec::new()
        },
        refusal,
    }
}

/// Append one answered exchange to a conversation. `R-O9`.
///
/// Failures are logged and swallowed: the answer is already on its way to a
/// panel that is showing it, and losing the *record* of a reply must never
/// look like losing the reply.
/// One `R-O4` question, answered from the turns that produced the change.
///
/// **The daemon retrieves, because the daemon holds the transcripts.** ADR-0030
/// clause 1 put the endpoint here for a reason that applies twice over to the
/// evidence: a window watching another machine has never read that machine's
/// sessions, so a client that sent turns would be sending a worse copy of what
/// is already here.
///
/// Three answers, and they are told apart on purpose — `--bin why` measured why
/// that matters. Over 14 edit moments a reason was found in five, and the
/// nearest-in-time retrieval rested four of its five answers on the assistant's
/// own narration:
///
/// | basis | what it means |
/// | --- | --- |
/// | `Turns` | the conversation says why, and the citations open at it |
/// | `Unanswered` | the turns exist and do not say. The majority case, so it is an answer |
/// | `Code` | no transcript covers this file; the diff was read, and the label says so |
async fn ask_about(
    state: &AppState,
    id: &str,
    messages: &[mogeung_core::model::ChatTurn],
    about: &mogeung_core::wire::AskAbout,
    from_this_machine: bool,
) -> ServerMsg {
    use mogeung_core::wire::{AnswerBasis, Citation};

    let fail = |e: String| ServerMsg::ModelReply {
        id: id.to_string(),
        text: None,
        error: Some(e),
        model: String::new(),
        elapsed_ms: 0,
        cites: Vec::new(),
        basis: None,
        narration: false,
        stopped: false,
    };

    let question = messages
        .last()
        .map(|m| m.content.trim().to_string())
        .unwrap_or_default();
    if question.is_empty() {
        return fail("no question was asked".into());
    }
    let Some(session) = state.get(&about.session_id).await else {
        return fail("that session is not one this daemon knows".into());
    };

    // The hunk the reader is looking at: one line of *where* for the prompt,
    // and the whole of what there is to read when no transcript covers the file.
    let hunk = match state.change_for_request(&about.session_id, false).await {
        Some(change) => change
            .files
            .iter()
            .find(|f| f.path == about.path)
            .and_then(|f| match &about.anchor {
                Some(a) => f.hunks.iter().find(|h| &h.anchor == a).cloned(),
                None => f.hunks.first().cloned(),
            }),
        None => None,
    };

    // Reading a transcript is file work and can be megabytes of it. Off the
    // reactor, like every other scan this daemon does.
    let transcript = std::path::PathBuf::from(session.transcript_path.clone());
    let path = about.path.clone();
    let turns = tokio::task::spawn_blocking(move || {
        let moments = crate::why::edit_moments(&transcript, Some(&path));
        // The **last** edit of this file, not the first: a file written five
        // times in a session was being worked on, and the reason you are
        // looking at the diff is the state it ended in.
        moments.last().map_or_else(Vec::new, |m| {
            crate::why::turns_for(&transcript, m, crate::why::Shape::LeadingUp, ASK_TURNS)
        })
    })
    .await
    .unwrap_or_default();

    let header = hunk.as_ref().map(|h| h.header.clone());
    let (prompt, basis) = if !turns.is_empty() {
        (
            crate::why::prompt(&question, &about.path, header.as_deref(), &turns),
            AnswerBasis::Turns,
        )
    } else if let Some(h) = &hunk {
        (
            crate::why::prompt_from_code(&question, &about.path, &h.header, &h.lines),
            AnswerBasis::Code,
        )
    } else {
        return fail(
            "no conversation covers this file and the diff is no longer on disk — \
             there is nothing here to answer from"
                .into(),
        );
    };

    let answered = state
        .model
        .chat_as(&[mogeung_core::model::ChatTurn::user(prompt)], from_this_machine)
        .await;
    let a = match answered {
        Ok(a) => a,
        Err(e) => return fail(e),
    };

    // Read from the code alone is prose, not a form: there are no turns to
    // cite, so there is nothing to parse and nothing to claim.
    if basis == AnswerBasis::Code {
        return ServerMsg::ModelReply {
            id: id.to_string(),
            text: Some(a.text),
            error: None,
            model: a.model,
            elapsed_ms: a.elapsed_ms,
            cites: Vec::new(),
            basis: Some(AnswerBasis::Code),
            narration: false,
            stopped: false,
        };
    }

    let parsed = crate::why::parse_answer(&a.text, &turns);
    let cites: Vec<Citation> = parsed
        .cites
        .iter()
        .filter_map(|line| turns.iter().find(|t| t.line == *line))
        .map(|t| Citation {
            line: t.line,
            role: t.role.clone(),
            timestamp: t.timestamp,
            preview: t.text.chars().take(160).collect(),
        })
        .collect();
    // Decided here so that no client can render *the agent said it did this* as
    // *this is why it was done*. `R-N7`'s rule with a measurement behind it.
    let narration = crate::why::is_narration(cites.iter().map(|c| c.role.as_str()));
    ServerMsg::ModelReply {
        id: id.to_string(),
        text: (!parsed.no_reason).then(|| parsed.reason.clone()),
        error: None,
        model: a.model,
        elapsed_ms: a.elapsed_ms,
        cites,
        basis: Some(if parsed.no_reason { AnswerBasis::Unanswered } else { AnswerBasis::Turns }),
        narration,
        stopped: false,
    }
}

/// How many turns one question is answered from.
///
/// `--bin why`'s own number, so the panel is asking what the harness measured.
const ASK_TURNS: usize = 6;

fn keep_conversation(
    state: &AppState,
    id: &str,
    sent: &[mogeung_core::model::ChatTurn],
    answer: &str,
    from_this_machine: bool,
) {
    if chat_refusal(state, from_this_machine).is_some() {
        return;
    }
    // What was sent already **is** the conversation: the client filters
    // failed exchanges out before asking, so this is the thread as it will be
    // re-sent — not a second reconstruction of it that could disagree.
    let mut turns = sent.to_vec();
    turns.push(mogeung_core::model::ChatTurn::assistant(answer));
    let title = mogeung_core::model::chat_title(&turns);
    if let Err(e) = state
        .store
        .save_chat(id, &title, &turns, chrono::Utc::now().timestamp())
    {
        tracing::warn!("could not keep the conversation: {e}");
    }
}


/// Why the config file may not be edited from here, if it may not. `R-J79`.
///
/// Loopback only, and there is deliberately no flag: the file holds `push_url`
/// and `model_url`, both of which point outward, so a daemon a second machine
/// can reconfigure is a daemon a second machine can aim at itself. Reading is
/// not refused — a LAN client may see the settings it is being served under.
fn config_refusal(state: &AppState) -> Option<String> {
    // Unset reads as allowed, matching `writes_allowed`: the callers that never
    // set it are tests and the window's own hosted daemon, both loopback in
    // fact.
    if *state.config_editable.get().unwrap_or(&true) {
        return None;
    }
    Some(
        "this daemon is reachable beyond loopback, so its config is read-only          here — it holds outbound URLs, and a daemon anyone can reconfigure is          a daemon anyone can aim. Edit ~/.mogeung/config.toml on that machine."
            .to_string(),
    )
}

/// Why this daemon will not type into an agent, when it will not. `R-B54`,
/// [ADR-0003's amendment](../../../docs/decisions/0003-observe-do-not-spawn.md) clause 4.
///
/// The chat refusal's shape, for a worse consequence. The chat risks somebody
/// else spending your tokens; this is somebody else **typing into your running
/// agents**, and the ADR says plainly that no flag opens it. `may_write` is not
/// the gate here and must not become it: that one accepts a token, and a shared
/// secret on a LAN is not the same thing as a person at this machine.
///
/// Asked per connection since 2026-09-23 (`R-J104`, ADR-0003's amendment of
/// that date): the clause wants *a person at this machine*, and a client on
/// loopback is one — or is at the far end of an `ssh -L` into it, which is
/// the route this refusal itself recommends. Still no token and no flag.
fn send_refusal(state: &AppState, from_this_machine: bool) -> Option<String> {
    if *state.send_allowed.get().unwrap_or(&true) || from_this_machine {
        return None;
    }
    Some(
        "this daemon is reachable beyond loopback, so it will not type into a \
         session here — that is somebody else's keyboard in your agents. Bind \
         127.0.0.1 and reach it over ssh, or copy the text and paste it yourself."
            .to_string(),
    )
}

/// One answer for the **similar** list, refusals included. `R-O6`.
///
/// Every silence is a sentence: no embedding model configured, no index built,
/// or an endpoint that would not answer. An empty list with no reason is the
/// shape that gets reported as a bug, and — worse for this feature — the shape
/// that reads as *there is nothing similar*, which is a claim rather than a
/// state.
async fn semantic_reply(state: &Arc<AppState>, query: String) -> ServerMsg {
    let (model, built_ms) = state.store.semantic_meta().unwrap_or_default();
    // An index is a photograph of a corpus that keeps growing. Compared against
    // the newest transcript rather than a count, because a file that grew is a
    // change the index cannot see and a count would miss.
    let stale = built_ms > 0 && newest_corpus_ms(&state.claude_home) > built_ms;
    let settings = state.model.settings();
    if query.trim().is_empty() {
        return ServerMsg::SemanticResults {
            query,
            hits: Vec::new(),
            model,
            built_ms,
            stale,
            refusal: None,
        };
    }
    match crate::embed::search_index(&settings, &state.store, &query).await {
        Ok(hits) => ServerMsg::SemanticResults {
            query,
            hits,
            model,
            built_ms,
            stale,
            refusal: None,
        },
        Err(e) => ServerMsg::SemanticResults {
            query,
            hits: Vec::new(),
            model,
            built_ms,
            stale,
            refusal: Some(e),
        },
    }
}

/// The newest modification time anywhere in the corpus, in epoch milliseconds.
fn newest_corpus_ms(home: &std::path::Path) -> i64 {
    fn walk(dir: &std::path::Path, depth: u8, newest: &mut i64) {
        if depth > 4 {
            return;
        }
        let Ok(rd) = std::fs::read_dir(dir) else { return };
        for e in rd.flatten() {
            let Ok(ft) = e.file_type() else { continue };
            let p = e.path();
            if ft.is_dir() {
                walk(&p, depth + 1, newest);
            } else if p.extension().is_some_and(|x| x == "jsonl") {
                if let Ok(ms) = e.metadata().and_then(|m| m.modified()) {
                    if let Ok(d) = ms.duration_since(std::time::UNIX_EPOCH) {
                        *newest = (*newest).max(d.as_millis() as i64);
                    }
                }
            }
        }
    }
    let mut newest = 0i64;
    walk(&home.join("projects"), 0, &mut newest);
    if let Ok(m) = std::fs::metadata(home.join("history.jsonl")).and_then(|m| m.modified()) {
        if let Ok(d) = m.duration_since(std::time::UNIX_EPOCH) {
            newest = newest.max(d.as_millis() as i64);
        }
    }
    newest
}

/// The config file as the editor sees it.
fn config_msg(state: &AppState, error: Option<String>, saved: bool) -> ServerMsg {
    let path = mogeung_core::config::Config::path();
    // A read failure lands in `error` beside empty text rather than as a
    // separate message: the editor has one place to look for what went wrong.
    let (text, read_error) = match mogeung_core::config::Config::read_text(&path) {
        Ok(t) => (t, None),
        Err(e) => (String::new(), Some(e)),
    };
    ServerMsg::Config {
        path: path.to_string_lossy().into_owned(),
        text,
        keys: mogeung_core::config::Config::known_keys(),
        readonly: config_refusal(state),
        error: error.or(read_error),
        saved,
    }
}

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
    #[serde(default)]
    all: bool,
    #[serde(default)]
    since: Option<i64>,
    #[serde(default)]
    until: Option<i64>,
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
        all: q.all,
        since: q.since,
        until: q.until,
    };
    match state.git_log(&id, None, q.skip, q.limit, q.rev, filter).await {
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
    match state.git_show(&id, None, &q.sha, opts).await {
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
    match state.git_status(&id, None).await {
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
    match state.git_diff_file(&id, None, &q.path, opts).await {
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
    match state.git_refs(&id, None).await {
        Ok(info) => Json(serde_json::json!(info)),
        Err(e) => Json(serde_json::json!({ "error": e.to_string() })),
    }
}

async fn get_git_stashes(
    State(state): State<Arc<AppState>>,
    AxPath(id): AxPath<String>,
) -> impl IntoResponse {
    match state.git_stashes(&id, None).await {
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
    match state.git_stash_show(&id, None, q.index, opts).await {
        Ok(files) => Json(serde_json::json!({ "index": q.index, "files": files })),
        Err(e) => Json(serde_json::json!({ "error": e.to_string() })),
    }
}

async fn get_git_submodules(
    State(state): State<Arc<AppState>>,
    AxPath(id): AxPath<String>,
) -> impl IntoResponse {
    match state.git_submodules(&id, None).await {
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
    match state.git_diff_range(&id, None, &q.from, &q.to, opts).await {
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
    match state.git_compare(&id, None, &q.branch).await {
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
    match state.git_reflog(&id, None).await {
        Ok(entries) => Json(serde_json::json!({ "entries": entries })),
        Err(e) => Json(serde_json::json!({ "error": e.to_string() })),
    }
}

async fn get_git_worktrees(
    State(state): State<Arc<AppState>>,
    AxPath(id): AxPath<String>,
) -> impl IntoResponse {
    match state.git_worktrees(&id, None).await {
        Ok(worktrees) => Json(serde_json::json!({ "worktrees": worktrees })),
        Err(e) => Json(serde_json::json!({ "error": e.to_string() })),
    }
}

async fn get_git_conflict(
    State(state): State<Arc<AppState>>,
    AxPath(id): AxPath<String>,
    Query(q): Query<PathQuery>,
) -> impl IntoResponse {
    match state.git_conflict_stages(&id, None, &q.path).await {
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
    match state.git_file_at_rev(&id, None, &q.sha, &q.path).await {
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

/// The address a request came from, when the server was started with it.
///
/// `server::run` serves with `into_make_service_with_connect_info`, so every
/// request carries it; a router served without it — a test harness, an
/// embedding — has none, and the caller falls back to the bind. An extractor
/// of our own rather than `ConnectInfo` itself, because that one rejects a
/// request that lacks it, and a missing address is a fact to reason about, not
/// an error to answer with a 500.
struct Peer(Option<std::net::SocketAddr>);

impl<S: Send + Sync> axum::extract::FromRequestParts<S> for Peer {
    type Rejection = std::convert::Infallible;
    async fn from_request_parts(
        parts: &mut axum::http::request::Parts,
        _state: &S,
    ) -> Result<Self, Self::Rejection> {
        Ok(Peer(
            parts
                .extensions
                .get::<axum::extract::ConnectInfo<std::net::SocketAddr>>()
                .map(|c| c.0),
        ))
    }
}

/// Loopback, including an IPv4 loopback address arriving on a dual-stack
/// socket as `::ffff:127.0.0.1` — which `is_loopback` on the v6 form denies.
pub fn is_loopback_ip(ip: std::net::IpAddr) -> bool {
    match ip {
        std::net::IpAddr::V4(v) => v.is_loopback(),
        std::net::IpAddr::V6(v) => v.is_loopback() || v.to_ipv4_mapped().is_some_and(|m| m.is_loopback()),
    }
}

/// Is the client on this machine? `R-J103`, ADR-0030's 2026-09-23 amendment.
///
/// ADR-0030 clause 4 exists so a daemon reachable from the LAN does not become
/// an LLM proxy *for the LAN*, and it was decided from the **bind** — so the
/// day the window's own daemon learnt to bind the LAN from `config.toml`
/// (`R-I17` step 7), the window on this very machine was refused its own chat.
/// Decided per connection instead: a loopback peer is on this machine — or on
/// the far end of an `ssh -L` tunnel, the route the refusal itself has always
/// recommended — and anyone else is judged by the bind, as before.
fn from_this_machine(peer: Option<std::net::SocketAddr>, state: &AppState) -> bool {
    match peer {
        Some(addr) => is_loopback_ip(addr.ip()),
        // No address to go on: the bind is the only fact, and it decides.
        None => state.model.chat_allowed(),
    }
}

async fn ws_upgrade(
    ws: WebSocketUpgrade,
    Peer(peer): Peer,
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    let local = from_this_machine(peer, &state);
    ws.on_upgrade(move |socket| ws_conn(socket, state, local))
}

async fn ws_conn(socket: WebSocket, state: Arc<AppState>, from_this_machine: bool) {
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
    if let Ok(txt) = serde_json::to_string(&state.snapshot(from_this_machine).await) {
        if sink.send(Message::Text(txt.into())).await.is_err() {
            return;
        }
    }

    let health_state = state.clone();
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
                Ok(mut msg) => {
                    // **No re-export** (`R-I21`, ADR-0013's amendment): a
                    // socket from another machine never learns what this
                    // daemon mirrors, or whom it follows — so following is
                    // never transitive, and two daemons following each other
                    // cannot loop.
                    if !from_this_machine {
                        use mogeung_core::federation::is_mirrored;
                        match &mut msg {
                            ServerMsg::SessionUpdated { session } if is_mirrored(&session.id) => continue,
                            ServerMsg::SessionRemoved { session_id } if is_mirrored(session_id) => continue,
                            ServerMsg::Queue { queue } => queue.retain(|i| !is_mirrored(&i.session_id)),
                            ServerMsg::Peers { .. } => continue,
                            _ => {}
                        }
                    }
                    // Health is broadcast once for every window, with the
                    // model row as everyone but this machine reads it; a
                    // client on this machine is shown its own (`R-J103`).
                    // The run panel's flag follows the same rule.
                    if from_this_machine {
                        match &mut msg {
                            ServerMsg::Health { health } => {
                                health.model = health_state.model.health_for(true);
                            }
                            // Likewise the run panel's *will this daemon run
                            // anything* (`R-J104`): broadcast as another
                            // machine reads it, shown to this one as its own.
                            // Not a mirrored session's: that flag is the other
                            // machine's gate and this follow's consent, already
                            // folded by the relay (`R-I23`), and this machine's
                            // own authority says nothing about it.
                            ServerMsg::RunConfigs { allowed, session_id, .. }
                                if !mogeung_core::federation::is_mirrored(session_id) =>
                            {
                                *allowed = true
                            }
                            _ => {}
                        }
                    }
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

    // Commands about mirrored sessions go to the daemon that owns them, up a
    // socket that belongs to this window (`R-I22`).
    let mut relays = crate::federation::Relays::new(state.clone(), reply_tx.clone());
    while let Some(Ok(msg)) = stream.next().await {
        let Message::Text(txt) = msg else { continue };
        match serde_json::from_str::<ClientMsg>(&txt) {
            Ok(cmd) => {
                if relays.intercept(&cmd, from_this_machine) {
                    continue;
                }
                handle(&state, cmd, &reply_tx, from_this_machine).await
            }
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
async fn rebroadcast_status(state: &Arc<AppState>, session_id: String, repo: Option<String>) {
    match state.git_status(&session_id, repo.clone()).await {
        Ok(entries) => state.broadcast(ServerMsg::GitLocalChanges {
            session_id,
            repo,
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
async fn after_ref_change(state: &Arc<AppState>, session_id: String, repo: Option<String>) {
    match state.git_refs(&session_id, repo.clone()).await {
        Ok(info) => state.broadcast(ServerMsg::GitRefsInfo {
            session_id: session_id.clone(),
            repo: repo.clone(),
            info: Box::new(info),
        }),
        Err(e) => state.broadcast(ServerMsg::Error {
            message: e.to_string(),
        }),
    }
    match state.git_stashes(&session_id, repo.clone()).await {
        Ok(stashes) => state.broadcast(ServerMsg::GitStashList {
            session_id: session_id.clone(),
            repo: repo.clone(),
            stashes,
        }),
        Err(e) => state.broadcast(ServerMsg::Error {
            message: e.to_string(),
        }),
    }
    rebroadcast_status(state, session_id, repo).await
}

/// The scratch tree as one message. `R-L9`.
///
/// Built in one place so the file list and the folder list cannot be broadcast
/// out of step — a folder missing from a message whose files reference it is a
/// panel that cannot draw the row it was told about.
fn scratches(dir: &std::path::Path) -> ServerMsg {
    ServerMsg::Scratches {
        names: crate::scratch::list(dir).unwrap_or_default(),
        folders: crate::scratch::folders(dir).unwrap_or_default(),
    }
}

async fn handle(
    state: &Arc<AppState>,
    cmd: ClientMsg,
    reply: &tokio::sync::mpsc::Sender<ServerMsg>,
    from_this_machine: bool,
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
        // `R-O5`. Spawned rather than awaited in the read loop: an ask can
        // take a minute against a cold local model, and a connection that
        // stopped accepting commands for its duration would freeze the whole
        // window over one text box. The answer goes down the asking socket's
        // reply lane whenever it arrives, which is the lane's whole purpose.
        // The config file. `R-J79`.
        //
        // Both verbs answer with the same message: after a save you are looking
        // at what the daemon now has, which is the only version worth showing.
        // On the asking socket rather than broadcast — this is a reply to a
        // question, and another window's editor should not be scrolled by it.
        // The one command that reaches an agent's input. `R-B54`, ADR-0003's 2026-08-29 amendment.
        //
        // Everything this does is a refusal until the last line: not off
        // loopback, not for a session this daemon does not know, not for one
        // with no pane to aim at, and not with nothing to say. What it never
        // does is come back for a reply — one shot, and no loop.
        ClientMsg::SendToSession { session_id, text } => {
            if let Some(why) = send_refusal(state, from_this_machine) {
                return err(anyhow::anyhow!(why));
            }
            let Some(session) = state.get(&session_id).await else {
                return err(anyhow::anyhow!("that session is not one this daemon knows"));
            };
            let Some(target) = session.tmux_target.clone() else {
                return err(anyhow::anyhow!(
                    "that session is not running under tmux, so there is no pane to \
                     send to — start sessions with `yolomo` for this, or copy the text \
                     and paste it yourself"
                ));
            };
            // Three subprocesses. Off the reactor like every other shell-out.
            let t = target.clone();
            let sent = tokio::task::spawn_blocking(move || crate::send::send(&t, &text))
                .await
                .unwrap_or_else(|e| Err(anyhow::anyhow!("the send did not run: {e}")));
            match sent {
                Ok(()) => send_reply(&reply, ServerMsg::SentToSession { session_id, target }),
                Err(e) => return err(e),
            }
        }
        // The second list, and the index behind it. `R-O6`.
        //
        // Spawned rather than awaited for `R-O5`'s reason — a build embeds
        // thousands of lines and a read loop waiting on it would freeze the
        // window — and **asked for** rather than automatic: ADR-0031 clause 6
        // keeps model work off anything that runs on its own.
        ClientMsg::SemanticSearch { query } => {
            let state = state.clone();
            let reply = reply.clone();
            tokio::spawn(async move {
                send_reply(&reply, semantic_reply(&state, query).await);
            });
        }
        // Failures by meaning. `R-F4` by meaning, `R-O6`.
        //
        // A second grouping the panel switches to — the literal list is
        // untouched and is what shows without a model, which is pillar K's
        // refusal of a blend applied to a list rather than to an ordering.
        ClientMsg::ClusterFailures { min_sessions } => {
            let state = state.clone();
            let reply = reply.clone();
            tokio::spawn(async move {
                let root = state.claude_home.join("projects");
                let min = min_sessions.max(1);
                let failures =
                    tokio::task::spawn_blocking(move || crate::insight::recurring_failures(&root, min))
                        .await
                        .unwrap_or_default();
                let settings = state.model.settings();
                let msg = match crate::embed::cluster_failures(&settings, failures).await {
                    Ok(clusters) => ServerMsg::FailureClusters {
                        clusters,
                        model: settings.embed_model.clone().unwrap_or_default(),
                        refusal: None,
                    },
                    Err(e) => ServerMsg::FailureClusters {
                        clusters: Vec::new(),
                        model: String::new(),
                        refusal: Some(e),
                    },
                };
                send_reply(&reply, msg);
            });
        }
        ClientMsg::BuildSemanticIndex {} => {
            let state = state.clone();
            let reply = reply.clone();
            tokio::spawn(async move {
                let settings = state.model.settings();
                let root = state.claude_home.join("projects");
                let history = state.claude_home.join("history.jsonl");
                match crate::embed::build_index(&settings, &state.store, &root, &history).await {
                    // Answered with an empty query, so the panel learns what
                    // the index now is — model, age, size — without having to
                    // ask a question it does not have yet.
                    Ok(_) => send_reply(&reply, semantic_reply(&state, String::new()).await),
                    Err(e) => send_reply(&reply, ServerMsg::Error { message: e }),
                }
            });
        }
        ClientMsg::ConfigGet {} => send_reply(&reply, config_msg(&state, None, false)),
        ClientMsg::ConfigSave { text } => {
            let path = mogeung_core::config::Config::path();
            let msg = match config_refusal(&state) {
                Some(why) => config_msg(&state, Some(why), false),
                None => match mogeung_core::config::Config::write_to(&path, &text) {
                    // Re-read rather than echo what was sent: what is on disk
                    // is the truth, and a save that half-worked must not be
                    // reported by showing the text that did not land.
                    Ok(()) => {
                        // The three keys a hosted daemon reads are re-applied
                        // now, so fixing the model endpoint here does not need
                        // a restart. Everything else does, and the window says
                        // so — this is not a general live-reload and pretending
                        // otherwise would be worse than not doing it at all.
                        if let Ok(c) = mogeung_core::config::Config::check(&text) {
                            let mut settings = state.model.settings();
                            settings.url = c.model_url.clone();
                            settings.model = c.model_name.clone();
                            // A flag passed on the command line is broader than
                            // anything the file can say and must not be
                            // narrowed by editing the file (ADR-0031).
                            if settings.consent != mogeung_core::model::RemoteConsent::Any {
                                settings.consent = c.allow_remote_model.clone();
                            }
                            state.model.configure(settings);
                        }
                        config_msg(&state, None, true)
                    }
                    Err(e) => config_msg(&state, Some(e), false),
                },
            };
            send_reply(&reply, msg);
            // `refresh_pricing` is read at each check rather than at start-up,
            // so a save that switched readings on is acted on now. `R-J124`.
            state.pricing_kick.notify_one();
            // The model row in Health moves when these keys do.
            let state = state.clone();
            tokio::spawn(async move {
                state.broadcast(ServerMsg::Health {
                    health: Box::new(state.health().await),
                });
            });
        }
        // The reading guide. `R-O3`.
        //
        // Spawned, like the chat: a model ordering a 40-file diff takes a
        // minute on a local endpoint, and awaiting it here would stop that
        // connection accepting anything else for the whole of it.
        ClientMsg::ReadingGuide { session_id } => {
            let state = state.clone();
            let reply = reply.clone();
            tokio::spawn(async move {
                let msg = reading_guide(&state, session_id, from_this_machine).await;
                send_reply(&reply, msg);
            });
        }
        // The chat history. `R-O9`, ADR-0032.
        //
        // Refused wherever the ask itself is: these rows hold the same
        // free-form text, and a daemon that will not take a question has no
        // business handing back the last two hundred.
        ClientMsg::ChatList {} => send_reply(&reply, chat_history_msg(&state, from_this_machine)),
        ClientMsg::ChatLoad { id } => {
            if chat_refusal(&state, from_this_machine).is_none() {
                let turns = state.store.load_chat(&id).unwrap_or_default();
                send_reply(&reply, ServerMsg::ChatConversation { id, turns });
            }
        }
        ClientMsg::ChatDelete { id } => {
            if chat_refusal(&state, from_this_machine).is_none() {
                if let Err(e) = state.store.delete_chat(&id) {
                    err(anyhow::anyhow!("could not forget that conversation: {e}"));
                }
            }
            send_reply(&reply, chat_history_msg(&state, from_this_machine));
        }
        ClientMsg::ModelChat {
            id,
            messages,
            conversation,
            about,
        } => {
            // This machine's model cannot read another machine's files, and
            // the question is not forwarded: it is free-form text, which is
            // exactly what ADR-0030 clause 4 keeps off other machines.
            if let Some(a) = about.as_ref().filter(|a| mogeung_core::federation::is_mirrored(&a.session_id)) {
                let host = state
                    .federation
                    .get(&a.session_id)
                    .and_then(|s| s.origin.and_then(|o| o.host))
                    .unwrap_or_else(|| "another machine".into());
                send_reply(
                    reply,
                    ServerMsg::ModelReply {
                        id,
                        text: None,
                        error: Some(format!(
                            "that session is on {host} — ask about it from a window on that machine, whose model can read its files"
                        )),
                        model: String::new(),
                        elapsed_ms: 0,
                        cites: Vec::new(),
                        basis: None,
                        narration: false,
                        stopped: false,
                    },
                );
                return;
            }
            let state = state.clone();
            let reply = reply.clone();
            tokio::spawn(async move {
                // A question *about* something is a different question through
                // the same door (`R-O4`, ADR-0034's *revisit if*). The daemon
                // holds the transcripts, so it does the retrieving; the client
                // sent ids and a question and cannot have sent the evidence.
                //
                // Answered whole rather than streamed, unlike the chat: the
                // reply is a **form** (`REASON:` / `CITES:`) that is parsed
                // before it is shown, and streaming the scaffolding into a
                // panel would put the form on screen instead of the answer.
                if let Some(about) = about {
                    let msg = ask_about(&state, &id, &messages, &about, from_this_machine).await;
                    send_reply(&reply, msg);
                    state.broadcast(ServerMsg::Health {
                        health: Box::new(state.health().await),
                    });
                    return;
                }
                // Forwarded as it arrives. `R-O11`: on a route to a large
                // model this is the difference between one second and forty
                // before anything appears at all.
                //
                // The batches are already coalesced by the model seam, so this
                // is a handful of small events per answer rather than one per
                // token — which is what keeps the bounded reply lane (`R-J59`)
                // from dropping a client mid-answer for lag.
                let chunk_reply = reply.clone();
                let chunk_id = id.clone();
                let msg = match state
                    .model
                    .chat_streaming(&id, &messages, from_this_machine, |delta| {
                        send_reply(
                            &chunk_reply,
                            ServerMsg::ModelChunk { id: chunk_id.clone(), delta },
                        );
                    })
                    .await
                {
                    Ok(a) => {
                        // Kept only when the client named a conversation and
                        // the file has not said no — and only on an answer, so
                        // a refusal or a dead endpoint leaves no half-thread
                        // behind that would be re-sent as context next time.
                        //
                        // A stopped answer is kept with what it had, which is
                        // what you read and what the next question follows
                        // on from; one stopped before a word arrived is not.
                        if let Some(cid) = conversation.as_deref().filter(|_| !a.text.is_empty()) {
                            keep_conversation(&state, cid, &messages, &a.text, from_this_machine);
                        }
                        ServerMsg::ModelReply {
                            id,
                            text: Some(a.text),
                            error: None,
                            model: a.model,
                            elapsed_ms: a.elapsed_ms,
                            // The chat rests on nothing this daemon holds, so
                            // it claims nothing: no citations and no basis.
                            cites: Vec::new(),
                            basis: None,
                            narration: false,
                            stopped: a.stopped,
                        }
                    }
                    // A refusal and a failure arrive by the same door. The
                    // panel has to say *something* — a silent box reads as a
                    // bug in mogeung rather than as a daemon declining.
                    Err(e) => ServerMsg::ModelReply {
                        id,
                        text: None,
                        error: Some(e),
                        model: String::new(),
                        elapsed_ms: 0,
                        cites: Vec::new(),
                        basis: None,
                        narration: false,
                        stopped: false,
                    },
                };
                send_reply(&reply, msg);
                // Health carries the residue of that ask, and the window shows
                // it in a panel that is probably open — so republish rather
                // than wait for the next scan to volunteer it.
                state.broadcast(ServerMsg::Health {
                    health: Box::new(state.health().await),
                });
            });
        }
        // `R-J118`. The reply comes from the ask itself, which ends with what it
        // had; an id not in flight has already answered, so there is nothing
        // to say here either way.
        ClientMsg::ModelStop { id } => {
            state.model.stop(&id);
        }
        ClientMsg::Subscribe => {
            // Direct, not broadcast: this used to make every *other* window
            // re-ingest the full board whenever any window reconnected —
            // which after a laptop sleep is all of them, at once.
            let snap = state.snapshot(from_this_machine).await;
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
                .start_as(&repo, &config_id, Some(session_id), &configs, from_this_machine)
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
            headless,
        } => {
            if let Err(e) = state.launch_terminal(&dir, worktree, source, headless).await {
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
        // Federation. `R-I21`. Loopback clients only: following is a hand's
        // act at this machine, and the follow list is never served elsewhere
        // (ADR-0013's and ADR-0036's amendments).
        // Another machine's window asking what this one follows is told
        // nothing rather than refused: every window asks on every snapshot,
        // and a refusal would be a toast on each connect to a remote.
        ClientMsg::FetchPeers if !from_this_machine => {}
        ClientMsg::Follow { .. } | ClientMsg::Unfollow { .. } | ClientMsg::SetPeerWrites { .. }
            if !from_this_machine =>
        {
            err(anyhow::anyhow!(
                "following is managed from a window on this machine only"
            ))
        }
        ClientMsg::Follow { id, name, url, token, allow_writes } => {
            let entry = crate::federation::PeerEntry { id, name, url, token, allow_writes };
            if let Err(e) = state.federation.follow(state, entry).await {
                err(anyhow::anyhow!(e));
            }
        }
        ClientMsg::Unfollow { id } => {
            if let Err(e) = state.federation.unfollow(state, &id).await {
                err(anyhow::anyhow!(e));
            }
        }
        ClientMsg::SetPeerWrites { id, allow } => {
            if let Err(e) = state.federation.set_writes(state, &id, allow) {
                err(anyhow::anyhow!(e));
            }
        }
        ClientMsg::FetchPeers => send_reply(reply, ServerMsg::Peers { peers: state.federation.peers() }),
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
        ClientMsg::ProbeIntellij { session_id } => match state.probe_intellij(&session_id).await {
            Ok((root, project, launcher)) => state.broadcast(ServerMsg::IntellijProbe {
                session_id,
                root,
                project,
                launcher,
            }),
            Err(e) => err(e),
        },
        ClientMsg::OpenInIntellij { session_id } => {
            if let Err(e) = state.open_in_intellij(&session_id).await {
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
            all,
            since,
            until,
            repo,
        } => {
            let filter = crate::git::LogFilter {
                grep: grep.clone(),
                author: author.clone(),
                path: path.clone(),
                pickaxe: pickaxe.clone(),
                all,
                since,
                until,
            };
            match state
                .git_log(&session_id, repo.clone(), skip, limit, rev.clone(), filter)
                .await
            {
                Ok((commits, done)) => state.broadcast(ServerMsg::GitCommits {
                    session_id,
                    repo,
                    skip,
                    commits,
                    done,
                    rev,
                    grep,
                    author,
                    path,
                    pickaxe,
                    all,
                    since,
                    until,
                }),
                Err(e) => err(e),
            }
        }
        ClientMsg::GitShow {
            session_id,
            sha,
            context,
            ignore_ws,
            repo,
        } => {
            let opts = crate::git::DiffOpts::from_wire(context, ignore_ws);
            match state.git_show(&session_id, repo.clone(), &sha, opts).await {
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
        ClientMsg::GitStatus { session_id , repo } => match state.git_status(&session_id, repo.clone()).await {
            Ok(entries) => state.broadcast(ServerMsg::GitLocalChanges {
                session_id,
                repo,
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
        ClientMsg::GitStage { session_id, paths , repo } => {
            match state.git_stage(&session_id, repo.clone(), paths).await {
                Ok(()) => rebroadcast_status(state, session_id, repo).await,
                Err(e) => err(e),
            }
        }
        ClientMsg::GitUnstage { session_id, paths , repo } => {
            match state.git_unstage(&session_id, repo.clone(), paths).await {
                Ok(()) => rebroadcast_status(state, session_id, repo).await,
                Err(e) => err(e),
            }
        }
        ClientMsg::GitCommit {
            session_id,
            message,
            amend,
            session_trailer,
            repo,
        } => {
            match state
                .git_commit(&session_id, repo.clone(), message, amend, session_trailer)
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
                    rebroadcast_status(state, session_id, repo).await
                }
                Err(e) => err(e),
            }
        }
        ClientMsg::GitBranchCreate {
            session_id,
            name,
            switch_to,
            repo,
        } => match state.git_branch_create(&session_id, repo.clone(), name, switch_to).await {
            // Refs *and* status: creating a branch changes the refs list, and
            // switching onto it can change what is uncommitted.
            Ok(()) => after_ref_change(state, session_id, repo).await,
            Err(e) => err(e),
        },
        ClientMsg::GitSwitch {
            session_id,
            name,
            detach,
            repo,
        } => {
            match state.git_switch(&session_id, repo.clone(), name, detach).await {
                Ok(()) => {
                    state.recompute_change(&session_id).await;
                    after_ref_change(state, session_id, repo).await
                }
                Err(e) => err(e),
            }
        }
        ClientMsg::GitStashPush {
            session_id,
            message,
            include_untracked,
            repo,
        } => match state
            .git_stash_push(&session_id, repo.clone(), message, include_untracked)
            .await
        {
            Ok(()) => {
                state.recompute_change(&session_id).await;
                after_ref_change(state, session_id, repo).await
            }
            Err(e) => err(e),
        },
        ClientMsg::GitStashPop { session_id, index , repo } => {
            match state.git_stash_pop(&session_id, repo.clone(), index).await {
                Ok(()) => {
                    state.recompute_change(&session_id).await;
                    after_ref_change(state, session_id, repo).await
                }
                Err(e) => err(e),
            }
        }
        ClientMsg::GitStashDrop { session_id, index , repo } => {
            match state.git_stash_drop(&session_id, repo.clone(), index).await {
                // Nothing in the worktree moved, so only the stash list did.
                Ok(()) => after_ref_change(state, session_id, repo).await,
                Err(e) => err(e),
            }
        }
        ClientMsg::GitFetch { session_id , repo } => match state.git_fetch(&session_id, repo.clone()).await {
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
                after_ref_change(state, session_id, repo).await
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
        // Tasks. `R-L3`, ADR-0015. A save re-derives them, so the note
        // handlers below broadcast both — a checkbox typed into a document has
        // to appear in the list without asking for it.
        ClientMsg::TaskList => match state.tasks() {
            Ok((tasks, closed_today)) => state.broadcast(ServerMsg::Tasks { tasks, closed_today }),
            Err(e) => err(e),
        },
        ClientMsg::TaskSet { note_id, ord, done } => {
            match state.set_task(&note_id, ord, done).await {
                Ok(notes) => {
                    // Both, and in this order: the document changed, and the
                    // derived view of it changed with it.
                    state.broadcast(ServerMsg::Notes { notes });
                    if let Ok((tasks, closed_today)) = state.tasks() {
                        state.broadcast(ServerMsg::Tasks { tasks, closed_today });
                    }
                }
                Err(e) => err(e),
            }
        }
        ClientMsg::NoteSave {
            id,
            body,
            session_id,
            seq,
            repo,
        } => match state.save_note(id, body, session_id, seq, repo).await {
            Ok(notes) => {
                state.broadcast(ServerMsg::Notes { notes });
                if let Ok((tasks, closed_today)) = state.tasks() {
                    state.broadcast(ServerMsg::Tasks { tasks, closed_today });
                }
            }
            Err(e) => err(e),
        },
        ClientMsg::NoteDelete { id } => match state.delete_note(&id).await {
            Ok(notes) => {
                state.broadcast(ServerMsg::Notes { notes });
                if let Ok((tasks, closed_today)) = state.tasks() {
                    state.broadcast(ServerMsg::Tasks { tasks, closed_today });
                }
            }
            Err(e) => err(e),
        },
        // -- Scratch files. `R-L5`, ADR-0035. The list is broadcast so two
        // windows agree on what exists; a file's content goes to whoever
        // asked, because a read must never open a pane somewhere else.
        ClientMsg::ScratchList => match crate::scratch::list(&state.scratch_dir()) {
            Ok(_) => state.broadcast(scratches(&state.scratch_dir())),
            Err(e) => err(e),
        },
        ClientMsg::ScratchCreate { ext, folder } => {
            let dir = state.scratch_dir();
            match crate::scratch::create_in(&dir, folder.as_deref(), &ext) {
                Ok(name) => {
                    send_reply(
                        reply,
                        ServerMsg::ScratchContent {
                            name,
                            content: String::new(),
                            fresh: true,
                        },
                    );
                    state.broadcast(scratches(&dir));
                }
                Err(e) => err(e),
            }
        }
        ClientMsg::ScratchRead { name } => match crate::scratch::read(&state.scratch_dir(), &name) {
            Ok(content) => send_reply(
                reply,
                ServerMsg::ScratchContent {
                    name,
                    content,
                    fresh: false,
                },
            ),
            Err(e) => err(e),
        },
        ClientMsg::ScratchWrite { name, content } => {
            match crate::scratch::write(&state.scratch_dir(), &name, &content) {
                Ok(()) => send_reply(reply, ServerMsg::ScratchSaved { name }),
                Err(e) => err(e),
            }
        }
        // The three that manage the files rather than their contents. `R-L7`.
        // Each **broadcasts** the new list, like `ScratchCreate` does: two
        // windows on one daemon share this directory, and a panel still
        // offering a file that another window deleted is worse than a panel
        // that flickers.
        ClientMsg::ScratchRename { name, to } => {
            let dir = state.scratch_dir();
            match crate::scratch::rename(&dir, &name, &to) {
                Ok(()) => {
                    state.broadcast(scratches(&dir));
                    send_reply(reply, ServerMsg::ScratchSaved { name: to });
                }
                Err(e) => err(e),
            }
        }
        ClientMsg::ScratchDelete { name } => {
            let dir = state.scratch_dir();
            match crate::scratch::delete(&dir, &name) {
                Ok(()) => {
                    state.broadcast(scratches(&dir));
                }
                Err(e) => err(e),
            }
        }
        // Folders. `R-L9`. Both broadcast, because a folder another window
        // makes has to appear in this one's tree without a reload.
        ClientMsg::ScratchMkdir { path } => {
            let dir = state.scratch_dir();
            match crate::scratch::mkdir(&dir, &path) {
                Ok(()) => state.broadcast(scratches(&dir)),
                Err(e) => err(e),
            }
        }
        ClientMsg::ScratchRmdir { path } => {
            let dir = state.scratch_dir();
            match crate::scratch::rmdir(&dir, &path) {
                Ok(()) => state.broadcast(scratches(&dir)),
                Err(e) => err(e),
            }
        }
        ClientMsg::ScratchDuplicate { name } => {
            let dir = state.scratch_dir();
            match crate::scratch::duplicate(&dir, &name) {
                Ok(fresh) => {
                    match crate::scratch::read(&dir, &fresh) {
                        Ok(content) => send_reply(
                            reply,
                            ServerMsg::ScratchContent {
                                name: fresh,
                                content,
                                // The window opens a duplicate the same way it
                                // opens a new file — it is one.
                                fresh: true,
                            },
                        ),
                        Err(e) => err(e),
                    }
                    state.broadcast(scratches(&dir));
                }
                Err(e) => err(e),
            }
        }
        ClientMsg::GitResolve {
            session_id,
            path,
            side,
            repo,
        } => match state.git_resolve(&session_id, repo.clone(), path, side).await {
            Ok(()) => {
                state.recompute_change(&session_id).await;
                rebroadcast_status(state, session_id, repo).await
            }
            Err(e) => err(e),
        },
        ClientMsg::GitDiscard { session_id, paths , repo } => {
            match state.git_discard(&session_id, repo.clone(), paths).await {
                Ok(()) => {
                    // The diff the Changes tab is showing was computed from
                    // files that may no longer exist. Recomputing is not
                    // decoration: a stale hunk offers a "discard" of something
                    // already gone.
                    state.recompute_change(&session_id).await;
                    rebroadcast_status(state, session_id, repo).await
                }
                Err(e) => err(e),
            }
        }
        ClientMsg::GitDiffFile {
            session_id,
            path,
            context,
            ignore_ws,
            repo,
        } => {
            let opts = crate::git::DiffOpts::from_wire(context, ignore_ws);
            match state.git_diff_file(&session_id, repo.clone(), &path, opts).await {
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
        ClientMsg::GitRefs { session_id , repo } => match state.git_refs(&session_id, repo.clone()).await {
            Ok(info) => state.broadcast(ServerMsg::GitRefsInfo {
                session_id,
                repo,
                info: Box::new(info),
            }),
            Err(e) => err(e),
        },
        ClientMsg::GitStashes { session_id , repo } => match state.git_stashes(&session_id, repo.clone()).await {
            Ok(stashes) => state.broadcast(ServerMsg::GitStashList {
                session_id,
                repo,
                stashes,
            }),
            Err(e) => err(e),
        },
        ClientMsg::GitStashShow {
            session_id,
            index,
            context,
            ignore_ws,
            repo,
        } => {
            let opts = crate::git::DiffOpts::from_wire(context, ignore_ws);
            match state.git_stash_show(&session_id, repo.clone(), index, opts).await {
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
        ClientMsg::GitSubmodules { session_id , repo } => {
            match state.git_submodules(&session_id, repo.clone()).await {
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
            repo,
        } => {
            let opts = crate::git::DiffOpts::from_wire(context, ignore_ws);
            match state.git_diff_range(&session_id, repo.clone(), &from, &to, opts).await {
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
        ClientMsg::GitCompare { session_id, branch , repo } => {
            match state.git_compare(&session_id, repo.clone(), &branch).await {
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
        ClientMsg::GitReflog { session_id , repo } => match state.git_reflog(&session_id, repo.clone()).await {
            Ok(entries) => state.broadcast(ServerMsg::GitReflogList {
                session_id,
                entries,
            }),
            Err(e) => err(e),
        },
        ClientMsg::GitWorktrees { session_id , repo } => {
            match state.git_worktrees(&session_id, repo.clone()).await {
                Ok(worktrees) => state.broadcast(ServerMsg::GitWorktreeList {
                    session_id,
                    worktrees,
                }),
                Err(e) => err(e),
            }
        }
        ClientMsg::GitConflictFile { session_id, path , repo } => {
            match state.git_conflict_stages(&session_id, repo.clone(), &path).await {
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
            repo,
        } => match state.git_file_at_rev(&session_id, repo.clone(), &sha, &path).await {
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
            ClientMsg::GitStage { session_id: id(), repo: None, paths: paths() },
            ClientMsg::GitUnstage { session_id: id(), repo: None, paths: paths() },
            ClientMsg::GitDiscard { session_id: id(), repo: None, paths: paths() },
            ClientMsg::GitCommit {
                session_id: id(),
                repo: None,
                message: "m".into(),
                amend: false,
                session_trailer: true,
            },
            ClientMsg::GitBranchCreate {
                session_id: id(),
                repo: None,
                name: "b".into(),
                switch_to: true,
            },
            ClientMsg::GitSwitch { session_id: id(), repo: None, name: "b".into(), detach: false },
            ClientMsg::GitStashPush {
                session_id: id(),
                repo: None,
                message: String::new(),
                include_untracked: true,
            },
            ClientMsg::GitStashPop { session_id: id(), repo: None, index: 0 },
            ClientMsg::GitStashDrop { session_id: id(), repo: None, index: 0 },
            ClientMsg::GitResolve {
                session_id: id(),
                repo: None,
                path: "a.rs".into(),
                side: mogeung_core::wire::ResolveSide::Ours,
            },
            // Not a repository write, but gated with them deliberately: it is
            // the one verb that reaches a network beyond this machine
            // (ADR-0014), and an open socket must not be able to make this
            // daemon talk to someone else's server.
            ClientMsg::GitFetch { session_id: id(), repo: None },
        ] {
            assert!(is_write(&cmd), "{cmd:?} changes the repository");
        }

        // The neighbours. `GitStatus` especially: it is the answer every write
        // verb broadcasts, so gating it would make a successful write look
        // like a failed one.
        for cmd in [
            ClientMsg::GitStatus { session_id: id(), repo: None },
            ClientMsg::GitRefs { session_id: id(), repo: None },
            ClientMsg::GitStashes { session_id: id(), repo: None },
            ClientMsg::RefreshChange { session_id: id(), force: true },
            ClientMsg::ReviewAll { session_id: id() },
            ClientMsg::Subscribe,
        ] {
            assert!(!is_write(&cmd), "{cmd:?} only reads");
        }
    }

    /// A window built before `R-D32` sends no `detach`, and must keep meaning
    /// *move onto a branch* — the whole point of `#[serde(default)]` on a flag
    /// whose other value moves the worktree somewhere you did not ask for.
    #[test]
    fn a_switch_without_the_detach_flag_is_an_ordinary_switch() {
        let msg: ClientMsg =
            serde_json::from_str(r#"{"cmd":"git_switch","session_id":"s","name":"main"}"#).unwrap();
        match msg {
            ClientMsg::GitSwitch { detach, name, .. } => {
                assert!(!detach);
                assert_eq!(name, "main");
            }
            other => panic!("decoded as {other:?}"),
        }
        let asked: ClientMsg = serde_json::from_str(
            r#"{"cmd":"git_switch","session_id":"s","name":"v1","detach":true}"#,
        )
        .unwrap();
        assert!(matches!(asked, ClientMsg::GitSwitch { detach: true, .. }));
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

#[cfg(test)]
mod token_tests {
    use super::percent_decode;

    /// What `URLSearchParams` does to a token, undone — and nothing else.
    #[test]
    fn a_query_value_is_decoded_the_way_a_url_encoder_wrote_it() {
        assert_eq!(percent_decode("ab%2Fcd%2Bef"), "ab/cd+ef");
        assert_eq!(percent_decode("a+b"), "a b");
        assert_eq!(percent_decode("plain-token_1"), "plain-token_1");
        // Malformed escapes are left alone rather than eaten.
        assert_eq!(percent_decode("50%"), "50%");
        assert_eq!(percent_decode("%zz"), "%zz");
        assert_eq!(percent_decode("%2"), "%2");
        // A `%` before a multibyte character is input, not a reason to stop.
        assert_eq!(percent_decode("%€"), "%€");
        assert_eq!(percent_decode("caf%C3%A9"), "café");
    }
}

#[cfg(test)]
mod peer_tests {
    use super::is_loopback_ip;

    /// A dual-stack socket reports an IPv4 loopback client as a mapped v6
    /// address, and `Ipv6Addr::is_loopback` says no to it.
    #[test]
    fn loopback_is_loopback_in_every_spelling() {
        assert!(is_loopback_ip("127.0.0.1".parse().unwrap()));
        assert!(is_loopback_ip("::1".parse().unwrap()));
        assert!(is_loopback_ip("::ffff:127.0.0.1".parse().unwrap()));
        assert!(!is_loopback_ip("10.0.0.39".parse().unwrap()));
        assert!(!is_loopback_ip("::ffff:10.0.0.39".parse().unwrap()));
        assert!(!is_loopback_ip("fd00::1".parse().unwrap()));
    }
}
