//! Restarting the daemon must not re-ingest history. `R-A6`.
//!
//! The tailer's byte offsets used to live only in the process, while the
//! sessions they belonged to lived in the database. A restart therefore found
//! every session "known" and every transcript unread, re-folded each file from
//! byte 0, and appended the whole history again under fresh `seq`s — one extra
//! copy of every event, and one extra count on every counter, per restart.
//!
//! Costs nothing: no agent is ever launched.

use mogeungd::{state::AppState, store::Store};
use std::path::{Path, PathBuf};
use std::sync::Arc;

const SESSION: &str = "33333333-3333-3333-3333-333333333333";

fn write(path: &Path, body: &str) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, body).unwrap();
}

fn transcript_path(home: &Path) -> PathBuf {
    home.join("projects").join("-tmp").join(format!("{SESSION}.jsonl"))
}

/// A home with one exited session: two prompts and one tool call.
fn fake_home(tag: &str) -> PathBuf {
    let home = std::env::temp_dir().join(format!("mogeung-restart-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&home);
    write(
        &transcript_path(&home),
        &([
            r#"{"type":"user","timestamp":"2026-07-25T10:00:00.000Z","cwd":"/tmp","gitBranch":"main","message":{"role":"user","content":"first prompt"}}"#,
            r#"{"type":"assistant","timestamp":"2026-07-25T10:00:05.000Z","cwd":"/tmp","message":{"content":[{"type":"tool_use","id":"t1","name":"Edit","input":{"file_path":"/tmp/retry.rs"}}],"usage":{"output_tokens":42}}}"#,
            r#"{"type":"user","timestamp":"2026-07-25T10:00:06.000Z","cwd":"/tmp","message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"t1","is_error":false,"content":"ok"}]}}"#,
            r#"{"type":"user","timestamp":"2026-07-25T10:00:09.000Z","cwd":"/tmp","message":{"role":"user","content":"second prompt"}}"#,
        ]
        .join("\n")
            + "\n"),
    );
    home
}

fn db(home: &Path) -> PathBuf {
    home.join("mogeung.db")
}

/// Start a daemon over an existing home and database, exactly as a restart
/// would: same files, brand new process state.
async fn boot(home: &Path) -> Arc<AppState> {
    let store = Store::open(&db(home)).unwrap();
    let state = AppState::with_home(store, home.to_path_buf()).unwrap();
    state.scan().await;
    state
}

/// (events stored, turns counted, tool calls counted, output tokens)
async fn tally(state: &AppState) -> (usize, u32, u32, u64) {
    let events = state.store.load_events(SESSION, 0).unwrap().len();
    let s = state.sessions.read().await.get(SESSION).cloned().unwrap();
    (events, s.turns, s.tool_calls, s.tokens_out)
}

#[tokio::test]
async fn a_restart_re_reads_nothing() {
    let home = fake_home("plain");

    let first = boot(&home).await;
    let before = tally(&first).await;
    assert!(before.0 > 0, "the first run should have stored events");
    assert_eq!(before.1, 2, "two prompts");
    drop(first);

    // Three more starts over the same database. Each one used to add a copy.
    for _ in 0..3 {
        let state = boot(&home).await;
        assert_eq!(
            tally(&state).await,
            before,
            "a restart re-ingested the transcript"
        );
    }

    std::fs::remove_dir_all(&home).ok();
}

#[tokio::test]
async fn a_restart_still_follows_the_rest_of_the_file() {
    let home = fake_home("append");

    let first = boot(&home).await;
    let before = tally(&first).await;
    drop(first);

    // The session kept working while the daemon was down.
    let path = transcript_path(&home);
    let mut body = std::fs::read_to_string(&path).unwrap();
    body.push_str(
        r#"{"type":"user","timestamp":"2026-07-25T10:05:00.000Z","cwd":"/tmp","message":{"role":"user","content":"third prompt"}}"#,
    );
    body.push('\n');
    std::fs::write(&path, body).unwrap();

    let state = boot(&home).await;
    let after = tally(&state).await;
    assert_eq!(after.1, before.1 + 1, "the appended prompt was missed");
    assert_eq!(
        after.0,
        before.0 + 1,
        "the appended prompt should add exactly one event"
    );

    std::fs::remove_dir_all(&home).ok();
}

/// A transcript that is rewritten shorter is a different file; it is read from
/// the start again on purpose, and the persisted offset must not prevent that.
#[tokio::test]
async fn a_truncated_transcript_is_read_again() {
    let home = fake_home("truncate");
    let first = boot(&home).await;
    drop(first);

    let path = transcript_path(&home);
    std::fs::write(
        &path,
        "{\"type\":\"user\",\"timestamp\":\"2026-07-26T10:00:00.000Z\",\"cwd\":\"/tmp\",\"message\":{\"role\":\"user\",\"content\":\"rewritten\"}}\n",
    )
    .unwrap();

    let state = boot(&home).await;
    let s = state.sessions.read().await.get(SESSION).cloned().unwrap();
    assert_eq!(s.last_prompt.as_deref(), Some("rewritten"));

    std::fs::remove_dir_all(&home).ok();
}

/// The one-time repair of a database written before the offsets were durable.
#[tokio::test]
async fn the_repair_rebuilds_a_database_that_was_re_ingested() {
    let home = fake_home("repair");
    let path = transcript_path(&home);

    let first = boot(&home).await;
    let clean = tally(&first).await;
    drop(first);

    // Reproduce the old behaviour: forget where we had read to, twice.
    for _ in 0..2 {
        let store = Store::open(&db(&home)).unwrap();
        store.delete_tail_offset(&path.to_string_lossy()).unwrap();
        store.set_schema_version(0).unwrap();
        drop(store);
        let state = boot(&home).await;
        drop(state);
    }

    let polluted = {
        let store = Store::open(&db(&home)).unwrap();
        store.set_schema_version(0).unwrap();
        let state = AppState::with_home(store, home.clone()).unwrap();
        let t = tally(&state).await;
        drop(state);
        t
    };
    assert_eq!(polluted.0, clean.0 * 3, "the bug did not reproduce");
    assert_eq!(polluted.1, clean.1 * 3);

    // Now the repair, on a daemon that starts the way `server::prepare` does.
    let store = Store::open(&db(&home)).unwrap();
    let state = AppState::with_home(store, home.clone()).unwrap();
    state.repair_reingested_history().await.unwrap();
    state.scan().await;
    assert_eq!(tally(&state).await, clean, "the repair did not restore it");

    // And it does not run a second time.
    assert_eq!(state.store.schema_version(), mogeungd::store::SCHEMA_VERSION);
    state.repair_reingested_history().await.unwrap();
    assert_eq!(tally(&state).await, clean);

    std::fs::remove_dir_all(&home).ok();
}
