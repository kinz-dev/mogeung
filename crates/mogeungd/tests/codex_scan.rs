//! R-I1 end to end: the scan loop absorbs a Codex install into the same
//! queue as Claude Code sessions — which is A23's question asked of real
//! code. Fixture installs only; the real `~/.codex` is never touched.

use mogeung_core::session::SessionSource;
use mogeung_core::LiveStatus;
use mogeungd::{state::AppState, store::Store};
use rusqlite::Connection;
use std::path::{Path, PathBuf};
use std::sync::Arc;

const SCHEMA: &str = "
    CREATE TABLE threads (
        id TEXT PRIMARY KEY,
        rollout_path TEXT,
        created_at INTEGER,
        updated_at INTEGER,
        recency_at INTEGER,
        cwd TEXT,
        title TEXT,
        first_user_message TEXT,
        tokens_used INTEGER,
        archived INTEGER,
        git_branch TEXT,
        cli_version TEXT
    )";

fn homes(tag: &str) -> (PathBuf, PathBuf) {
    let root = std::env::temp_dir().join(format!("mogeung-cscan-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let claude = root.join("claude");
    let codex = root.join("codex");
    std::fs::create_dir_all(claude.join("projects")).unwrap();
    std::fs::create_dir_all(codex.join("sessions")).unwrap();
    (claude, codex)
}

async fn boot(claude: PathBuf, codex: PathBuf) -> Arc<AppState> {
    let store = Store::open(&claude.join("mogeung.db")).unwrap();
    // The Qwen home is pointed at a sibling that does not exist, so this test
    // stays about Codex and never reaches the developer's real `~/.qwen`.
    let homes = mogeungd::state::AgentHomes {
        codex,
        qwen: claude.parent().unwrap().join("qwen"),
    };
    let state = AppState::with_homes(store, claude.clone(), homes).unwrap();
    state.scan().await;
    state
}

/// Present, watched, empty — reported as exactly that, not as nothing.
#[tokio::test]
async fn an_empty_codex_install_is_reported_honestly() {
    let (claude, codex) = homes("empty");
    let conn = Connection::open(codex.join("state_5.sqlite")).unwrap();
    conn.execute_batch(SCHEMA).unwrap();
    drop(conn);

    let state = boot(claude, codex).await;
    let h = state.health().await;
    assert!(h.codex_present);
    assert_eq!(h.codex_threads, 0);
    assert!(h.codex_error.is_none());
    assert!(state.sessions.read().await.is_empty());
}

/// A thread waiting on an approval lands in the queue at the permission
/// tier, marked with its source — the Session model generalising (A23).
#[tokio::test]
async fn a_waiting_codex_thread_reaches_the_queue() {
    let (claude, codex) = homes("waiting");
    let now = chrono::Utc::now().timestamp();
    let rollout = codex.join("sessions").join("rollout-2026-07-29-t1.jsonl");
    std::fs::write(
        &rollout,
        concat!(
            r#"{"timestamp":"2026-07-29T10:00:00.000Z","type":"session_meta","payload":{"id":"t1","cwd":"/w/repo","cli_version":"0.145.0"}}"#,
            "\n",
            r#"{"timestamp":"2026-07-29T10:00:02.000Z","type":"event_msg","payload":{"type":"user_message","message":"fix the build"}}"#,
            "\n",
            r#"{"timestamp":"2026-07-29T10:00:03.000Z","type":"event_msg","payload":{"type":"turn_started"}}"#,
            "\n",
            r#"{"type":"event_msg","payload":{"type":"exec_approval_request","command":"rm -rf build"}}"#,
            "\n",
        ),
    )
    .unwrap();

    let conn = Connection::open(codex.join("state_5.sqlite")).unwrap();
    conn.execute_batch(SCHEMA).unwrap();
    conn.execute(
        "INSERT INTO threads (id, rollout_path, created_at, updated_at, recency_at,
             cwd, title, first_user_message, tokens_used, archived, cli_version)
         VALUES ('codex-t1', ?1, ?2, ?2, ?2, '/w/repo', 'Fix the build',
             'fix the build', 5000, 0, '0.145.0')",
        rusqlite::params![rollout.to_string_lossy(), now],
    )
    .unwrap();
    drop(conn);

    let state = boot(claude, codex).await;
    let sessions = state.sessions.read().await;
    let s = sessions.get("codex-t1").expect("codex thread became a session");
    assert_eq!(s.source, SessionSource::Codex);
    assert_eq!(s.title.as_deref(), Some("Fix the build"));
    assert!(s.alive, "recent + unfinished turn means alive");
    assert_eq!(s.live_status, Some(LiveStatus::Idle));
    assert!(
        s.awaiting_permission().is_some(),
        "a trailing approval request is a permission prompt"
    );
    drop(sessions);

    let h = state.health().await;
    assert_eq!(h.codex_threads, 1);
}

/// No install at all stays silent — absence is not an error state.
#[tokio::test]
async fn no_codex_install_reports_nothing() {
    let (claude, codex) = homes("absent");
    let _ = std::fs::remove_dir_all(&codex);
    let state = boot(claude, Path::new(&codex).to_path_buf()).await;
    let h = state.health().await;
    assert!(!h.codex_present);
    assert!(h.codex_error.is_none());
}

/// **A Codex thread you have just opened is on the board.** `R-J73`.
///
/// Reported 2026-08-26: *"After running codexmo, I have to type a message
/// before it appear on the ATTENTION list."* Codex writes the `threads` row on
/// the first user turn, so between starting a session and speaking to it there
/// was a live agent, a tmux pane hosting it, and nothing in the queue — which
/// is the moment you are most likely to be looking for it. This is `R-J30` one
/// CLI over, and the writer lock is what makes it answerable.
///
/// The lock is **held for real** rather than merely created: on Linux
/// `codex::scan_live` reads `/proc/locks` and correctly ignores an unheld file,
/// so a test that only touched one would pass while proving nothing.
#[tokio::test]
async fn a_thread_that_has_never_been_spoken_to_still_reaches_the_board() {
    use std::os::unix::io::AsRawFd;

    let (claude, codex) = homes("unspoken");
    // An index that exists and is empty: the thread is open, and Codex has not
    // written a row for it yet. Exactly the reported state.
    let conn = Connection::open(codex.join("state_5.sqlite")).unwrap();
    conn.execute_batch(SCHEMA).unwrap();
    drop(conn);

    let id = "01a04095-6927-7be0-8953-ee38457ef7fd";
    let dir = codex.join("thread-writer-locks");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join(".coordination.lock"), "").unwrap();
    let lock = std::fs::File::create(dir.join(format!("{id}.lock"))).unwrap();
    // Held for the life of this test, the way a running `codex` holds it.
    let rc = unsafe { libc::flock(lock.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) };
    assert_eq!(rc, 0, "the test must actually hold the lock");

    let state = boot(claude, codex).await;

    let sessions = state.sessions.read().await;
    let s = sessions
        .get(id)
        .expect("an open thread belongs on the board before its first turn");
    assert_eq!(s.source, SessionSource::Codex);
    assert!(s.alive, "the lock is held, so the thread is open");
    assert_eq!(
        s.live_status,
        Some(LiveStatus::Idle),
        "open and mid-nothing is waiting for you — the reason to show it this early"
    );
    assert!(
        !sessions.contains_key(".coordination"),
        "the coordination lock names no thread"
    );
    drop(sessions);

    // And it must not be adopted twice, nor fight with the index pass once the
    // thread finally writes its row.
    state.scan().await;
    assert_eq!(
        state.sessions.read().await.len(),
        1,
        "a second pass must not adopt the same thread again"
    );

    drop(lock);
}

/// **The index gate must not close over a session that has just started.**
/// `R-J76`.
///
/// Reported 2026-08-26, after `R-J73` was supposed to have fixed exactly this:
/// *"even if I start a session without the headless for codex, I still need to
/// type a sample message before the session appear in the ATTENTION list."*
///
/// `R-J56` reads the `threads` index only when its mtime moved, except while a
/// Codex session is marked alive — and `R-J73`'s lock scan ran **after** that
/// gate. A thread nobody has spoken to writes no index row, so with no Codex
/// session yet alive the pass returned before it ever looked at the locks. The
/// first user message wrote the row, moved the mtime, opened the gate, and the
/// session appeared — which is precisely the symptom.
///
/// The bug lives on the *second* scan: the first always runs, because nothing
/// has been stamped yet. So this boots, then starts a thread, then scans again.
#[tokio::test]
async fn a_thread_that_starts_after_the_first_scan_still_reaches_the_board() {
    use std::os::unix::io::AsRawFd;

    let (claude, codex) = homes("late-start");
    let conn = Connection::open(codex.join("state_5.sqlite")).unwrap();
    conn.execute_batch(SCHEMA).unwrap();
    drop(conn);
    // The directory exists from the start, empty — as it does on a machine
    // where Codex has run before. Only its *contents* change below.
    std::fs::create_dir_all(codex.join("thread-writer-locks")).unwrap();

    // Scan one: nothing running, and the stamp is recorded.
    let state = boot(claude, codex.clone()).await;
    assert!(state.sessions.read().await.is_empty());

    // Now a thread opens. The index is untouched — Codex writes that row on the
    // first user turn, and there has not been one.
    let id = "01a040bb-5505-7130-8fd1-96d84ad92852";
    let lock =
        std::fs::File::create(codex.join("thread-writer-locks").join(format!("{id}.lock"))).unwrap();
    let rc = unsafe { libc::flock(lock.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) };
    assert_eq!(rc, 0, "the test must actually hold the lock");

    state.scan().await;

    let sessions = state.sessions.read().await;
    assert!(
        sessions.contains_key(id),
        "a thread that opened after the first scan must not wait for you to type: {:?}",
        sessions.keys().collect::<Vec<_>>()
    );
    assert!(sessions[id].alive);
    drop(sessions);
    drop(lock);
}
