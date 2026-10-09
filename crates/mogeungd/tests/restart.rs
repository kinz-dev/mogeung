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

    // Reproduce the old behaviour: forget where we had read to, twice. The
    // builds that wrote these databases persisted every fold eagerly; today's
    // counters coast until a quiet flush (`R-J54`), so the flush a shutdown
    // performs is what lands the double-fold in the store here.
    for _ in 0..2 {
        let store = Store::open(&db(&home)).unwrap();
        store.delete_tail_offset(&path.to_string_lossy()).unwrap();
        store.set_schema_version(0).unwrap();
        drop(store);
        let state = boot(&home).await;
        state.flush_all_quiet().await;
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

    // And it does not run a second time. `1` is the repair's own mark; later
    // versions belong to later one-off passes (`backfill_artifacts`, `R-J115`).
    assert_eq!(state.store.schema_version(), 1);
    state.repair_reingested_history().await.unwrap();
    assert_eq!(tally(&state).await, clean);

    std::fs::remove_dir_all(&home).ok();
}

/// A label written before `R-J42` still holds the CLI's markup, and the rows
/// you are looking at are the ones already in the database — a parser fix that
/// only touches lines read *from now on* leaves them wrong for as long as they
/// live. Repaired on the way in.
#[tokio::test]
async fn a_stored_label_full_of_command_markup_is_repaired_on_load() {
    let home = fake_home("markup");
    let state = boot(&home).await;

    // A row as an older build would have written it.
    let mut s = state.sessions.read().await.get(SESSION).cloned().unwrap();
    s.last_prompt = Some(
        "<command-name>/clear</command-name>\n            <command-message>clear</command-message>\n            <command-args></command-args>"
            .into(),
    );
    state.store.save_session(&s).unwrap();
    drop(state);

    let state = boot(&home).await;
    let after = state.sessions.read().await.get(SESSION).cloned().unwrap();
    // The queue reads the command, not the markup around it — and `label()`
    // is what a session without a title falls back to.
    assert_eq!(after.last_prompt.as_deref(), Some("/clear"));
}

/// A team definition survives the daemon. `R-P3`, feature 0051, the first
/// acceptance line of the Gau Geung spec: *"the definition survives a daemon
/// restart"*. The id the first daemon minted is the id the second one lists,
/// the policy comes back as written, and the mirror file is on disk for
/// anything that is not mogeung.
#[tokio::test]
async fn a_team_outlives_the_daemon_that_defined_it() {
    use mogeung_core::session::SessionSource;
    use mogeung_core::team::{PermissionMode, Policy, Seat, Team};
    let home = std::env::temp_dir().join(format!("mogeung-restart-team-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&home);
    std::fs::create_dir_all(&home).unwrap();
    let teams_dir = home.join("teams");

    let id = {
        let store = Store::open(&db(&home)).unwrap();
        let state = AppState::with_home(store, home.join("claude")).unwrap();
        state.teams_dir.set(teams_dir.clone()).unwrap();
        let mut seat = Seat::new("builder", SessionSource::Codex);
        seat.permission = Some(PermissionMode::Plan);
        let teams = state
            .save_team(Team {
                id: String::new(),
                name: "port openrig".into(),
                repo: "/home/you/mogeung".into(),
                guidance: "Read the spec.".into(),
                permission: PermissionMode::AcceptEdits,
                policy: Policy { hop_budget: 2, hourly_cap: 30, ..Policy::default() },
                seats: vec![seat],
                paused: true,
                created: 0,
                updated: 0,
            })
            .unwrap();
        assert_eq!(teams.len(), 1);
        assert!(!teams[0].id.is_empty(), "the daemon mints the id");
        assert!(teams[0].created > 0, "and stamps the clock");
        teams[0].id.clone()
    };

    let state = boot(&home).await;
    // The second daemon's mirror is the same directory, as it would be in
    // production — and pointed away from the real one, as every test must.
    state.teams_dir.set(teams_dir.clone()).unwrap();
    let teams = state.teams().unwrap();
    assert_eq!(teams.len(), 1);
    let t = &teams[0];
    assert_eq!(t.id, id);
    assert_eq!(t.name, "port openrig");
    assert_eq!((t.policy.hop_budget, t.policy.hourly_cap), (2, 30));
    assert_eq!(t.seats[0].permission, Some(PermissionMode::Plan));
    assert!(t.paused, "a paused team stays paused across a restart");

    let files: Vec<String> = std::fs::read_dir(&teams_dir)
        .unwrap()
        .flatten()
        .map(|e| e.file_name().to_string_lossy().to_string())
        .collect();
    assert_eq!(files.len(), 1, "{files:?}");
    assert!(files[0].starts_with(&format!("{id}-port-openrig")), "{files:?}");
    let md = std::fs::read_to_string(teams_dir.join(&files[0])).unwrap();
    assert!(md.contains("- hop budget: 2"), "{md}");

    state.delete_team(&id).unwrap();
    assert_eq!(std::fs::read_dir(&teams_dir).unwrap().count(), 0, "the mirror goes with it");
    std::fs::remove_dir_all(&home).ok();
}
