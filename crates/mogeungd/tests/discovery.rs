//! Session discovery against a synthetic `~/.claude`.
//!
//! Each test gets its own synthetic home, injected into the daemon, so nothing
//! here touches your real `~/.claude` and the tests are parallel-safe.
//!
//! Costs nothing: no agent is ever launched.

use mogeung_core::attention::AttentionReason;
use mogeung_core::session::LiveStatus;
use mogeungd::{state::AppState, store::Store};
use std::path::{Path, PathBuf};

fn write(path: &Path, body: &str) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, body).unwrap();
}

/// Build a fake Claude home containing one live session and one that exited.
fn fake_home(tag: &str) -> PathBuf {
    let home = std::env::temp_dir().join(format!("mogeung-home-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&home);

    // A live session: our own pid, so the liveness check passes.
    let live_id = "11111111-1111-1111-1111-111111111111";
    write(
        &home.join("sessions").join("4242.json"),
        &format!(
            r#"{{"pid":{},"sessionId":"{live_id}","cwd":"/tmp","name":"demo-1",
                "version":"2.1.220","kind":"interactive","status":"idle",
                "startedAt":1784995957755,"statusUpdatedAt":1784995999000}}"#,
            std::process::id()
        ),
    );
    write(
        &home.join("projects").join("-tmp").join(format!("{live_id}.jsonl")),
        &[
            r#"{"type":"ai-title","aiTitle":"Wire up the retry loop","sessionId":"x"}"#,
            r#"{"type":"user","timestamp":"2026-07-25T10:00:00.000Z","cwd":"/tmp","gitBranch":"main","message":{"role":"user","content":"please fix the retry loop"}}"#,
            r#"{"type":"assistant","timestamp":"2026-07-25T10:00:05.000Z","cwd":"/tmp","message":{"content":[{"type":"tool_use","id":"t1","name":"Edit","input":{"file_path":"/tmp/retry.rs"}}],"usage":{"output_tokens":42}}}"#,
            // The tool came back. Without this the session would be sitting on
            // an unanswered tool call, which is a permission prompt, not
            // "finished and waiting for a new instruction".
            r#"{"type":"user","timestamp":"2026-07-25T10:00:06.000Z","cwd":"/tmp","message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"t1","is_error":false,"content":"ok"}]}}"#,
        ]
        .join("\n"),
    );

    // An exited session: pid 999999 will not be running.
    let dead_id = "22222222-2222-2222-2222-222222222222";
    write(
        &home.join("sessions").join("999999.json"),
        &format!(
            r#"{{"pid":999999,"sessionId":"{dead_id}","cwd":"/tmp","name":"demo-2",
                "kind":"interactive","status":"busy"}}"#
        ),
    );
    write(
        &home.join("projects").join("-tmp").join(format!("{dead_id}.jsonl")),
        r#"{"type":"user","timestamp":"2026-07-25T09:00:00.000Z","cwd":"/tmp","message":{"role":"user","content":"older session"}}"#,
    );

    home
}

async fn boot(tag: &str) -> std::sync::Arc<AppState> {
    // The watch root is injected, not read from the environment, so these
    // tests are safe to run in parallel.
    let home = fake_home(tag);
    let store = Store::open(&home.join("mogeung.db")).unwrap();
    let state = AppState::with_home(store, home).unwrap();
    state.scan().await;
    state
}

#[tokio::test]
async fn discovers_sessions_and_reads_their_titles() {
    let state = boot("discover").await;
    let sessions = state.sessions.read().await;
    assert_eq!(sessions.len(), 2, "should find both transcripts");

    let live = sessions
        .get("11111111-1111-1111-1111-111111111111")
        .expect("live session missing");
    assert_eq!(live.title.as_deref(), Some("Wire up the retry loop"));
    assert_eq!(live.git_branch.as_deref(), Some("main"));
    assert_eq!(live.turns, 1);
    assert_eq!(live.tool_calls, 1);
    assert!(live.touched_files.iter().any(|f| f.ends_with("retry.rs")));
}

#[tokio::test]
async fn liveness_comes_from_the_os_not_the_registry_file() {
    let state = boot("liveness").await;
    let sessions = state.sessions.read().await;

    let live = sessions.get("11111111-1111-1111-1111-111111111111").unwrap();
    assert!(live.alive, "session with a running pid should be alive");
    assert_eq!(live.live_status, Some(LiveStatus::Idle));

    // The registry file claims status "busy", but the pid is dead. A stale
    // registry entry must never produce a phantom live session.
    let dead = sessions.get("22222222-2222-2222-2222-222222222222").unwrap();
    assert!(!dead.alive, "dead pid must not read as alive");
    assert_eq!(dead.live_status, None);
}

#[tokio::test]
async fn an_idle_live_session_tops_the_queue() {
    let state = boot("queue").await;
    let sessions: Vec<_> = state.sessions.read().await.values().cloned().collect();
    let queue = mogeung_core::attention::rank(&sessions, chrono::Utc::now(), &state.attention);

    assert_eq!(queue[0].session_id, "11111111-1111-1111-1111-111111111111");
    assert_eq!(queue[0].reason, AttentionReason::AwaitingInput);
    assert!(queue[0].detail.contains("waiting for you"));
}

/// R-B4, end to end. The same registry status ("idle") must produce a different
/// queue tier depending on whether a tool call is still outstanding.
#[tokio::test]
async fn an_unanswered_tool_call_reads_as_a_permission_prompt() {
    let state = boot("permission").await;
    let id = "11111111-1111-1111-1111-111111111111";
    let path = {
        let s = state.sessions.read().await;
        s.get(id).unwrap().transcript_path.clone()
    };

    // The agent asks to run something, and nothing answers.
    let mut f = std::fs::OpenOptions::new().append(true).open(&path).unwrap();
    use std::io::Write;
    writeln!(
        f,
        r#"{{"type":"assistant","timestamp":"2026-07-25T10:06:00.000Z","cwd":"/tmp","message":{{"content":[{{"type":"tool_use","id":"t9","name":"Bash","input":{{"command":"rm -rf build/"}}}}]}}}}"#
    )
    .unwrap();
    drop(f);
    state.scan().await;

    let sessions: Vec<_> = state.sessions.read().await.values().cloned().collect();
    let queue = mogeung_core::attention::rank(&sessions, chrono::Utc::now(), &state.attention);
    assert_eq!(queue[0].session_id, id);
    assert_eq!(queue[0].reason, AttentionReason::AwaitingPermission);
    assert!(
        queue[0].detail.contains("rm -rf build/"),
        "the queue must say what needs approving, got: {}",
        queue[0].detail
    );

    // And once the tool returns, it is an ordinary wait again.
    let mut f = std::fs::OpenOptions::new().append(true).open(&path).unwrap();
    writeln!(
        f,
        r#"{{"type":"user","timestamp":"2026-07-25T10:06:30.000Z","cwd":"/tmp","message":{{"role":"user","content":[{{"type":"tool_result","tool_use_id":"t9","is_error":false,"content":"done"}}]}}}}"#
    )
    .unwrap();
    drop(f);
    state.scan().await;

    let sessions: Vec<_> = state.sessions.read().await.values().cloned().collect();
    let queue = mogeung_core::attention::rank(&sessions, chrono::Utc::now(), &state.attention);
    assert_eq!(queue[0].reason, AttentionReason::AwaitingInput);
}

#[tokio::test]
async fn rescanning_is_idempotent() {
    let state = boot("idempotent").await;
    let before = state.sessions.read().await.len();
    let turns_before = state
        .sessions
        .read()
        .await
        .get("11111111-1111-1111-1111-111111111111")
        .unwrap()
        .turns;

    state.scan().await;
    state.scan().await;

    let after = state.sessions.read().await.len();
    let turns_after = state
        .sessions
        .read()
        .await
        .get("11111111-1111-1111-1111-111111111111")
        .unwrap()
        .turns;

    assert_eq!(before, after, "rescanning must not duplicate sessions");
    assert_eq!(
        turns_before, turns_after,
        "rescanning must not re-count already-read lines"
    );
}

#[tokio::test]
async fn appended_lines_are_picked_up_incrementally() {
    let state = boot("append").await;
    let id = "11111111-1111-1111-1111-111111111111";
    let path = {
        let s = state.sessions.read().await;
        s.get(id).unwrap().transcript_path.clone()
    };
    let turns_before = state.sessions.read().await.get(id).unwrap().turns;

    let mut f = std::fs::OpenOptions::new().append(true).open(&path).unwrap();
    use std::io::Write;
    writeln!(
        f,
        r#"{{"type":"user","timestamp":"2026-07-25T10:05:00.000Z","cwd":"/tmp","message":{{"role":"user","content":"and now also add a test"}}}}"#
    )
    .unwrap();
    drop(f);

    state.scan().await;

    let s = state.sessions.read().await;
    let after = s.get(id).unwrap();
    assert_eq!(after.turns, turns_before + 1, "new turn not observed");
    assert_eq!(
        after.last_prompt.as_deref(),
        Some("and now also add a test")
    );
}
