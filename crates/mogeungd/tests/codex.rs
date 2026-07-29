//! The Codex adapter, end to end — roadmap `R-I1`.
//!
//! Each test builds its own synthetic `~/.codex` in a tempdir; nothing reads
//! the real one, which mogeung must only ever open read-only.
//!
//! Honesty note: no real rollout file has ever existed on the machine this was
//! written on, so every fixture line encodes inference from the CLI binary.
//! The tests therefore lean hardest on the degradation paths — schema drift,
//! unknown kinds, stale paths — because those are what reality will exercise
//! first.

use mogeungd::codex::{
    read_rollout, CodexInstall, CodexStatus, RolloutTailer, TailEvent,
};
use rusqlite::Connection;
use std::io::Write as _;
use std::path::{Path, PathBuf};

fn temp_home(tag: &str) -> PathBuf {
    let home = std::env::temp_dir().join(format!("mogeung-codex-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&home);
    std::fs::create_dir_all(&home).unwrap();
    home
}

/// The `threads` table as read from a real `state_5.sqlite` (CLI 0.145.0),
/// trimmed to the columns the adapter uses plus a few it does not.
const CURRENT_SCHEMA: &str = "
    CREATE TABLE threads (
        id TEXT PRIMARY KEY,
        rollout_path TEXT,
        created_at INTEGER,
        updated_at INTEGER,
        recency_at INTEGER,
        created_at_ms INTEGER,
        updated_at_ms INTEGER,
        recency_at_ms INTEGER,
        source TEXT,
        model_provider TEXT,
        model TEXT,
        cwd TEXT,
        title TEXT,
        name TEXT,
        preview TEXT,
        first_user_message TEXT,
        sandbox_policy TEXT,
        approval_mode TEXT,
        tokens_used INTEGER,
        has_user_event INTEGER,
        archived INTEGER,
        archived_at INTEGER,
        git_sha TEXT,
        git_branch TEXT,
        git_origin_url TEXT,
        cli_version TEXT
    )";

fn make_db(home: &Path, version: u32, schema: &str) -> Connection {
    let conn = Connection::open(home.join(format!("state_{version}.sqlite"))).unwrap();
    conn.execute_batch(schema).unwrap();
    conn
}

const THREAD_ID: &str = "0199a213-81b0-72f2-93ec-a9f37d4dcc1b";

#[test]
fn a_current_schema_db_yields_threads_with_fields() {
    let home = temp_home("current");
    let conn = make_db(&home, 5, CURRENT_SCHEMA);
    conn.execute(
        "INSERT INTO threads (id, rollout_path, created_at, updated_at, recency_at,
             updated_at_ms, source, model_provider, model, cwd, title, tokens_used,
             archived, git_branch, cli_version)
         VALUES (?1, '/nowhere/rollout-x.jsonl', 1753700000, 1753700100, 1753700100,
             1753700100500, 'cli', 'openai', 'gpt-5.2-codex', '/w/repo', 'Fix the build',
             12345, 0, 'main', '0.145.0')",
        [THREAD_ID],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO threads (id, recency_at, archived) VALUES ('older-thread', 1753600000, 1)",
        [],
    )
    .unwrap();
    drop(conn);

    let install = CodexInstall::discover(&home).expect("install exists");
    let idx = install.list_threads();
    assert!(idx.error.is_none(), "unexpected error: {:?}", idx.error);
    assert!(
        idx.missing_columns.is_empty(),
        "current schema should miss nothing: {:?}",
        idx.missing_columns
    );
    assert_eq!(idx.threads.len(), 2);

    // Newest first.
    let t = &idx.threads[0];
    assert_eq!(t.id, THREAD_ID);
    assert_eq!(t.cwd.as_deref(), Some("/w/repo"));
    assert_eq!(t.title.as_deref(), Some("Fix the build"));
    assert_eq!(t.model.as_deref(), Some("gpt-5.2-codex"));
    assert_eq!(t.tokens_used, Some(12345));
    assert_eq!(t.git_branch.as_deref(), Some("main"));
    assert_eq!(t.cli_version.as_deref(), Some("0.145.0"));
    assert!(!t.archived);
    // The *_ms column wins over seconds when both exist.
    assert_eq!(t.updated_at.unwrap().timestamp_millis(), 1753700100500);
    // Seconds-only stamps still resolve.
    assert_eq!(t.created_at.unwrap().timestamp(), 1753700000);
    assert!(idx.threads[1].archived);

    std::fs::remove_dir_all(&home).ok();
}

/// 42 migrations exist; older installs have fewer columns. A poorer table must
/// still yield threads, with the gaps defaulted and named.
#[test]
fn a_column_poor_db_still_yields_threads() {
    let home = temp_home("poor");
    let conn = make_db(
        &home,
        2,
        "CREATE TABLE threads (id TEXT PRIMARY KEY, rollout_path TEXT, updated_at INTEGER)",
    );
    conn.execute(
        "INSERT INTO threads VALUES (?1, '/gone/rollout-x.jsonl', 1753700000)",
        [THREAD_ID],
    )
    .unwrap();
    drop(conn);

    let install = CodexInstall::discover(&home).unwrap();
    let idx = install.list_threads();
    assert!(idx.error.is_none(), "a poor schema is not an error");
    assert_eq!(idx.threads.len(), 1);
    let t = &idx.threads[0];
    assert_eq!(t.id, THREAD_ID);
    assert_eq!(t.updated_at.unwrap().timestamp(), 1753700000);
    // Everything absent degrades to default, silently per-field...
    assert!(t.cwd.is_none());
    assert!(t.tokens_used.is_none());
    assert!(!t.archived);
    // ...but the drift itself is named.
    assert!(idx.missing_columns.contains(&"cwd".to_string()));
    assert!(idx.missing_columns.contains(&"tokens_used".to_string()));

    std::fs::remove_dir_all(&home).ok();
}

/// The situation on the author's machine today: `~/.codex` exists, the index
/// exists, zero sessions. That must read as "present, no sessions", never as
/// an error and never as no install at all.
#[test]
fn an_empty_install_is_reported_honestly() {
    let home = temp_home("empty");
    make_db(&home, 5, CURRENT_SCHEMA);

    let install = CodexInstall::discover(&home).expect("present");
    let idx = install.list_threads();
    assert!(idx.db.is_some(), "the index was found");
    assert!(idx.error.is_none());
    assert!(idx.threads.is_empty());

    std::fs::remove_dir_all(&home).ok();
}

/// A `~/.codex` with no index at all (never run) is still an install —
/// honestly empty, with `db: None` saying why.
#[test]
fn an_indexless_install_is_present_with_no_threads() {
    let home = temp_home("indexless");
    let install = CodexInstall::discover(&home).expect("the directory exists");
    let idx = install.list_threads();
    assert!(idx.db.is_none());
    assert!(idx.error.is_none());
    assert!(idx.threads.is_empty());
    std::fs::remove_dir_all(&home).ok();
}

/// A corrupt index is an error, flagged — not a panic, and not an invisible
/// install.
#[test]
fn a_corrupt_index_is_flagged_not_fatal() {
    let home = temp_home("corrupt");
    std::fs::write(home.join("state_9.sqlite"), b"this is not sqlite").unwrap();
    let install = CodexInstall::discover(&home).unwrap();
    let idx = install.list_threads();
    assert!(idx.error.is_some(), "corruption must be flagged");
    assert!(idx.threads.is_empty());
    std::fs::remove_dir_all(&home).ok();
}

const ROLLOUT: &str = concat!(
    r#"{"timestamp":"2026-07-29T10:00:00.000Z","type":"session_meta","payload":{"id":"t","cwd":"/w/repo","cli_version":"0.145.0","git":{"branch":"main"}}}"#, "\n",
    r#"{"timestamp":"2026-07-29T10:00:01.000Z","type":"turn_context","payload":{"cwd":"/w/repo","model":"gpt-5.2-codex"}}"#, "\n",
    r#"{"timestamp":"2026-07-29T10:00:02.000Z","type":"event_msg","payload":{"type":"user_message","message":"fix the build"}}"#, "\n",
    r#"{"timestamp":"2026-07-29T10:00:03.000Z","type":"event_msg","payload":{"type":"turn_started"}}"#, "\n",
    r#"{"timestamp":"2026-07-29T10:00:04.000Z","type":"response_item","payload":{"type":"local_shell_call","command":["bash","-lc","cargo build"]}}"#, "\n",
    r#"{"timestamp":"2026-07-29T10:00:05.000Z","type":"response_item","payload":{"type":"agent_message","message":"done"}}"#, "\n",
    r#"{"timestamp":"2026-07-29T10:00:06.000Z","type":"event_msg","payload":{"type":"turn_complete","usage":{"input_tokens":100,"cached_input_tokens":40,"output_tokens":20,"reasoning_output_tokens":5}}}"#, "\n",
    r#"{"type":"ghost_snapshot","payload":{}}"#, "\n",
);

#[test]
fn a_rollout_is_classified_and_summarised() {
    let home = temp_home("rollout");
    let path = home.join("rollout-t.jsonl");
    std::fs::write(&path, ROLLOUT).unwrap();

    let r = read_rollout(&path);
    assert!(r.error.is_none());
    assert_eq!(r.counts.parsed, 7);
    assert_eq!(r.counts.ignored, 1);
    assert_eq!(r.counts.unknown, 0);
    assert_eq!(r.counts.malformed, 0);
    assert_eq!(r.cwd.as_deref(), Some("/w/repo"));
    assert_eq!(r.model.as_deref(), Some("gpt-5.2-codex"));
    assert_eq!(r.git_branch.as_deref(), Some("main"));
    assert_eq!(r.last_text.as_deref(), Some("done"));
    assert_eq!(r.tool_calls, 1);
    let u = r.last_usage.expect("usage was on the turn_complete");
    assert_eq!(u.total_in(), 140);
    assert_eq!(u.total_out(), 25);
    assert_eq!(r.status(), CodexStatus::Done);

    std::fs::remove_dir_all(&home).ok();
}

/// The canary, Codex edition. Every shape here is inferred, so an unknown kind
/// is expected — it must be counted and named, never a panic, and never quiet.
#[test]
fn an_unknown_kind_is_counted_not_panicked() {
    let home = temp_home("unknown");
    let path = home.join("rollout-t.jsonl");
    let body = format!(
        "{ROLLOUT}{}\n{}\n{}\n",
        r#"{"type":"hologram","payload":{"x":1}}"#,
        r#"{"type":"hologram","payload":{"x":2}}"#,
        r#"{"type":"response_item","payload":{"type":"telepathy"}}"#,
    );
    std::fs::write(&path, body).unwrap();

    let r = read_rollout(&path);
    assert_eq!(r.counts.unknown, 3);
    assert!(r
        .counts
        .unknown_kinds
        .contains(&("hologram".to_string(), 2)));
    assert!(r
        .counts
        .unknown_kinds
        .contains(&("response_item/telepathy".to_string(), 1)));
    // The rest of the file is still read; blindness is partial and stated.
    assert_eq!(r.counts.parsed, 7);
    assert_eq!(r.cwd.as_deref(), Some("/w/repo"));

    std::fs::remove_dir_all(&home).ok();
}

/// Archived rollouts are zstd-compressed. Same file, same answers.
#[test]
fn a_zst_rollout_decodes_to_the_same_answers() {
    let home = temp_home("zst");
    let path = home.join("rollout-t.jsonl.zst");
    let compressed = zstd::encode_all(ROLLOUT.as_bytes(), 3).unwrap();
    std::fs::write(&path, compressed).unwrap();

    let r = read_rollout(&path);
    assert!(r.error.is_none(), "zst decode failed: {:?}", r.error);
    assert_eq!(r.counts.parsed, 7);
    assert_eq!(r.counts.ignored, 1);
    assert_eq!(r.cwd.as_deref(), Some("/w/repo"));
    assert_eq!(r.status(), CodexStatus::Done);

    // A truncated/corrupt .zst degrades to a flagged error, not a panic.
    let bad = home.join("rollout-bad.jsonl.zst");
    std::fs::write(&bad, b"\x28\xb5\x2f\xfdgarbage").unwrap();
    let r = read_rollout(&bad);
    assert!(r.error.is_some());
    assert_eq!(r.counts.lines_seen(), 0);

    std::fs::remove_dir_all(&home).ok();
}

/// Waiting/working/done from real-shaped tails, through the full parse path.
#[test]
fn status_derives_from_the_rollout_tail() {
    let home = temp_home("status");

    // Trailing approval request, nothing after: waiting on the human.
    let waiting = home.join("rollout-w.jsonl");
    std::fs::write(
        &waiting,
        format!(
            "{ROLLOUT}{}\n{}\n",
            r#"{"type":"event_msg","payload":{"type":"turn_started"}}"#,
            r#"{"type":"event_msg","payload":{"type":"exec_approval_request","command":"rm -rf build"}}"#,
        ),
    )
    .unwrap();
    let r = read_rollout(&waiting);
    assert_eq!(r.status(), CodexStatus::Waiting);
    assert!(r.last_activity.unwrap().contains("rm -rf build"));

    // Turn started, chatter since: working.
    let working = home.join("rollout-k.jsonl");
    std::fs::write(
        &working,
        format!(
            "{ROLLOUT}{}\n{}\n",
            r#"{"type":"event_msg","payload":{"type":"turn_started"}}"#,
            r#"{"type":"response_item","payload":{"type":"reasoning"}}"#,
        ),
    )
    .unwrap();
    assert_eq!(read_rollout(&working).status(), CodexStatus::Working);

    // The base fixture ends in turn_complete: done.
    let done = home.join("rollout-d.jsonl");
    std::fs::write(&done, ROLLOUT).unwrap();
    assert_eq!(read_rollout(&done).status(), CodexStatus::Done);

    // An aborted turn is also done.
    let aborted = home.join("rollout-a.jsonl");
    std::fs::write(
        &aborted,
        format!("{ROLLOUT}{}\n", r#"{"type":"event_msg","payload":{"type":"turn_aborted"}}"#),
    )
    .unwrap();
    assert_eq!(read_rollout(&aborted).status(), CodexStatus::Done);

    std::fs::remove_dir_all(&home).ok();
}

/// `rollout_path` in the index can be stale — Codex itself read-repairs it.
/// A missing file must fall back to searching `sessions/` and
/// `archived_sessions/`, and only then degrade to index-only.
#[test]
fn a_stale_rollout_path_is_repaired_or_degrades_to_index_only() {
    let home = temp_home("stale");
    let conn = make_db(&home, 5, CURRENT_SCHEMA);
    conn.execute(
        "INSERT INTO threads (id, rollout_path, recency_at) VALUES (?1, ?2, 1753700000)",
        rusqlite::params![
            THREAD_ID,
            home.join("sessions/2026/07/01/rollout-moved.jsonl")
                .to_string_lossy(),
        ],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO threads (id, rollout_path, recency_at) VALUES ('lost-thread', '/gone/rollout-lost.jsonl', 1753600000)",
        [],
    )
    .unwrap();
    drop(conn);

    // The recorded path does not exist, but the file lives on under
    // archived_sessions/ named with the thread id.
    let real = home.join(format!(
        "archived_sessions/2026/07/29/rollout-2026-07-29T10-00-00-{THREAD_ID}.jsonl"
    ));
    std::fs::create_dir_all(real.parent().unwrap()).unwrap();
    std::fs::write(&real, ROLLOUT).unwrap();

    let install = CodexInstall::discover(&home).unwrap();
    let idx = install.list_threads();
    assert_eq!(idx.threads.len(), 2);

    let repaired = &idx.threads[0];
    assert_eq!(repaired.id, THREAD_ID);
    assert_eq!(install.resolve_rollout(repaired), Some(real.clone()));
    let r = install.read_thread_rollout(repaired).expect("repaired");
    assert_eq!(r.cwd.as_deref(), Some("/w/repo"));

    // No file anywhere: index-only, honestly.
    let lost = &idx.threads[1];
    assert_eq!(lost.id, "lost-thread");
    assert!(install.resolve_rollout(lost).is_none());
    assert!(install.read_thread_rollout(lost).is_none());

    std::fs::remove_dir_all(&home).ok();
}

/// The tail-incremental reader hands back only what was appended, and a
/// growing rollout moves waiting → done as the approval is resolved.
#[test]
fn the_rollout_tailer_reads_incrementally() {
    let home = temp_home("tailer");
    let path = home.join("rollout-t.jsonl");
    std::fs::write(&path, ROLLOUT).unwrap();

    let mut tailer = RolloutTailer::default();
    let first = tailer.read_new(&path).unwrap();
    assert_eq!(first.len(), 8);
    assert!(tailer.read_new(&path).unwrap().is_empty(), "nothing new yet");

    let mut f = std::fs::OpenOptions::new().append(true).open(&path).unwrap();
    writeln!(
        f,
        r#"{{"type":"event_msg","payload":{{"type":"exec_approval_request","command":"x"}}}}"#
    )
    .unwrap();
    drop(f);

    let new = tailer.read_new(&path).unwrap();
    assert_eq!(new.len(), 1);

    // Fold the increments into one rollout and watch the status move.
    let mut rollout = mogeungd::codex::CodexRollout::default();
    for l in first.iter().chain(new.iter()) {
        rollout.absorb_line(l);
    }
    assert_eq!(rollout.status(), CodexStatus::Waiting);

    std::fs::remove_dir_all(&home).ok();
}

/// Incremental reads over a compressed rollout: the offset tracks decoded
/// bytes, so a re-read of an unchanged file yields nothing.
#[test]
fn the_rollout_tailer_handles_zst() {
    let home = temp_home("tailer-zst");
    let path = home.join("rollout-t.jsonl.zst");
    std::fs::write(&path, zstd::encode_all(ROLLOUT.as_bytes(), 3).unwrap()).unwrap();

    let mut tailer = RolloutTailer::default();
    assert_eq!(tailer.read_new(&path).unwrap().len(), 8);
    assert!(tailer.read_new(&path).unwrap().is_empty());

    // The file is rewritten with more content — e.g. re-archived.
    let longer = format!("{ROLLOUT}{}\n", r#"{"type":"compacted","payload":{}}"#);
    std::fs::write(&path, zstd::encode_all(longer.as_bytes(), 3).unwrap()).unwrap();
    assert_eq!(tailer.read_new(&path).unwrap().len(), 1);

    std::fs::remove_dir_all(&home).ok();
}

/// The tail heuristic drives the queue signal, so the pure function's edges
/// get one integration-level pin: an answered approval is not waiting.
#[test]
fn an_answered_approval_is_not_waiting() {
    use TailEvent::*;
    assert_eq!(
        mogeungd::codex::derive_status(&[TurnStarted, ApprovalRequest, TurnComplete]),
        CodexStatus::Done
    );
}
