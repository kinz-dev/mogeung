//! Cross-session signals — roadmap `R-B3`, `R-B5`, `R-B7`, `R-D8`.
//!
//! These are the things a single agent cannot know about itself: that another
//! session is editing the same file, that it has been going in circles, that
//! the pile of unread work is growing. Each needs a synthetic `~/.claude` with
//! more than one session in it, which is why they live here rather than beside
//! the ranking unit tests.

use mogeung_core::attention::AttentionReason;
use mogeungd::{state::AppState, store::Store};
use std::path::{Path, PathBuf};
use std::sync::Arc;

fn write(path: &Path, body: &str) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, body).unwrap();
}

/// Register a live session (our own pid, so it passes the liveness check) with
/// the given transcript.
fn live_session(home: &Path, slot: u32, id: &str, cwd: &str, lines: &str) {
    write(
        &home.join("sessions").join(format!("{slot}.json")),
        &format!(
            r#"{{"pid":{},"sessionId":"{id}","cwd":"{cwd}","name":"demo-{slot}",
                "version":"2.1.220","kind":"interactive","status":"busy",
                "startedAt":1784995957755,"statusUpdatedAt":1784995999000}}"#,
            std::process::id()
        ),
    );
    write(
        &home.join("projects").join("-tmp").join(format!("{id}.jsonl")),
        lines,
    );
}

fn home_for(tag: &str) -> PathBuf {
    let home = std::env::temp_dir().join(format!("mogeung-sig-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&home);
    home
}

async fn boot(home: PathBuf) -> Arc<AppState> {
    let store = Store::open(&home.join("mogeung.db")).unwrap();
    let state = AppState::with_home(store, home).unwrap();
    state.scan().await;
    state
}

/// An Edit of `path` at `ts`, as an assistant tool_use plus its result.
fn edit(ts: &str, id: &str, path: &str) -> String {
    format!(
        r#"{{"type":"assistant","timestamp":"{ts}","cwd":"/tmp","message":{{"content":[{{"type":"tool_use","id":"{id}","name":"Edit","input":{{"file_path":"{path}"}}}}]}}}}
{{"type":"user","timestamp":"{ts}","cwd":"/tmp","message":{{"role":"user","content":[{{"type":"tool_result","tool_use_id":"{id}","is_error":false,"content":"ok"}}]}}}}"#
    )
}

/// R-B3. The signature capability of the observer model: neither agent can see
/// this, because neither can see the other.
#[tokio::test]
async fn two_live_sessions_editing_one_file_both_get_warned() {
    let home = home_for("collide");
    let now = chrono::Utc::now().to_rfc3339();
    let a = "aaaaaaaa-1111-1111-1111-111111111111";
    let b = "bbbbbbbb-2222-2222-2222-222222222222";

    live_session(&home, 5001, a, "/tmp", &edit(&now, "t1", "/tmp/shared.rs"));
    live_session(&home, 5002, b, "/tmp", &edit(&now, "t2", "/tmp/shared.rs"));

    let state = boot(home).await;
    let sessions = state.sessions.read().await;

    let sa = sessions.get(a).expect("session a");
    let sb = sessions.get(b).expect("session b");

    assert_eq!(sa.collisions.len(), 1, "a was not warned: {:?}", sa.collisions);
    assert_eq!(sb.collisions.len(), 1, "b was not warned");
    // Both sides get told, because either one might be the one to stop.
    assert_eq!(sa.collisions[0].other, b);
    assert_eq!(sb.collisions[0].other, a);
    assert_eq!(sa.collisions[0].path, "/tmp/shared.rs");
}

#[tokio::test]
async fn sessions_editing_different_files_are_left_alone() {
    let home = home_for("nocollide");
    let now = chrono::Utc::now().to_rfc3339();
    let a = "aaaaaaaa-1111-1111-1111-111111111111";
    let b = "bbbbbbbb-2222-2222-2222-222222222222";

    live_session(&home, 5003, a, "/tmp", &edit(&now, "t1", "/tmp/one.rs"));
    live_session(&home, 5004, b, "/tmp", &edit(&now, "t2", "/tmp/two.rs"));

    let state = boot(home).await;
    let sessions = state.sessions.read().await;
    assert!(sessions.get(a).unwrap().collisions.is_empty());
    assert!(sessions.get(b).unwrap().collisions.is_empty());
}

/// The window matters: "we both touched this file at some point today" is not
/// a collision, and reporting it as one would make the warning worthless.
#[tokio::test]
async fn an_old_edit_is_not_a_collision() {
    let home = home_for("stale");
    let now = chrono::Utc::now();
    let recent = now.to_rfc3339();
    let ancient = (now - chrono::Duration::hours(3)).to_rfc3339();
    let a = "aaaaaaaa-1111-1111-1111-111111111111";
    let b = "bbbbbbbb-2222-2222-2222-222222222222";

    live_session(&home, 5005, a, "/tmp", &edit(&recent, "t1", "/tmp/shared.rs"));
    live_session(&home, 5006, b, "/tmp", &edit(&ancient, "t2", "/tmp/shared.rs"));

    let state = boot(home).await;
    let sessions = state.sessions.read().await;
    assert!(
        sessions.get(a).unwrap().collisions.is_empty(),
        "a three-hour-old edit was reported as a live collision"
    );
}

/// R-B7. Repetition is suggestive, not proof — so it produces an advisory
/// string rather than a queue tier.
#[tokio::test]
async fn repeating_the_same_edit_reads_as_thrashing() {
    let home = home_for("loop");
    let now = chrono::Utc::now().to_rfc3339();
    let id = "cccccccc-3333-3333-3333-333333333333";

    let mut lines = String::new();
    for i in 0..6 {
        lines.push_str(&edit(&now, &format!("t{i}"), "/tmp/stuck.rs"));
        lines.push('\n');
    }
    live_session(&home, 5007, id, "/tmp", &lines);

    let state = boot(home).await;
    let s = state.sessions.read().await;
    let sig = s
        .get(id)
        .unwrap()
        .loop_signal
        .clone()
        .expect("six identical edits should register as a loop");
    assert!(sig.contains("stuck.rs"), "loop signal must name the target: {sig}");

    // It is advisory only: the queue tier is unchanged.
    let sessions: Vec<_> = s.values().cloned().collect();
    drop(s);
    let queue = mogeung_core::attention::rank(&sessions, chrono::Utc::now(), &state.attention);
    assert_ne!(queue[0].reason, AttentionReason::Failed);
}

#[tokio::test]
async fn varied_work_is_not_thrashing() {
    let home = home_for("noloop");
    let now = chrono::Utc::now().to_rfc3339();
    let id = "cccccccc-3333-3333-3333-333333333333";

    let mut lines = String::new();
    for i in 0..6 {
        lines.push_str(&edit(&now, &format!("t{i}"), &format!("/tmp/file{i}.rs")));
        lines.push('\n');
    }
    live_session(&home, 5008, id, "/tmp", &lines);

    let state = boot(home).await;
    let s = state.sessions.read().await;
    assert!(s.get(id).unwrap().loop_signal.is_none());
}

/// R-B5. Snooze has to survive a rescan, or it would silently expire the moment
/// the session did anything.
#[tokio::test]
async fn a_snooze_survives_rescanning() {
    let home = home_for("snooze");
    let now = chrono::Utc::now().to_rfc3339();
    let id = "dddddddd-4444-4444-4444-444444444444";
    live_session(&home, 5009, id, "/tmp", &edit(&now, "t1", "/tmp/a.rs"));

    let state = boot(home).await;
    state.snooze(id, 30).await;

    let before = state.get(id).await.unwrap().snoozed_until;
    assert!(before.is_some());

    state.scan().await;
    state.scan().await;

    let after = state.get(id).await.unwrap();
    assert_eq!(after.snoozed_until, before, "rescanning cleared the snooze");
    assert!(after.is_snoozed(chrono::Utc::now()));

    // Ranked as quiet while snoozed.
    let sessions: Vec<_> = state.sessions.read().await.values().cloned().collect();
    let queue = mogeung_core::attention::rank(&sessions, chrono::Utc::now(), &state.attention);
    let item = queue.iter().find(|i| i.session_id == id).unwrap();
    assert_eq!(item.reason, AttentionReason::Idle);
    assert!(item.detail.contains("snoozed"));

    // And waking restores it.
    state.snooze(id, 0).await;
    assert!(!state.get(id).await.unwrap().is_snoozed(chrono::Utc::now()));
}

/// R-D8. With no repo there is nothing to owe, and the answer must be "nothing
/// outstanding" rather than "0% read" — a false alarm on an empty set is how a
/// metric loses credibility on day one.
#[tokio::test]
async fn debt_for_a_repo_with_no_changes_is_not_an_alarm() {
    let home = home_for("debt");
    let now = chrono::Utc::now().to_rfc3339();
    let id = "eeeeeeee-5555-5555-5555-555555555555";
    live_session(&home, 5010, id, "/tmp", &edit(&now, "t1", "/tmp/a.rs"));

    let state = boot(home).await;
    let debt = state.review_debt("/nonexistent/repo").await;

    assert_eq!(debt.hunks_total, 0);
    assert_eq!(debt.progress(), 1.0);
    assert_eq!(debt.headline(), "nothing outstanding");
    assert!(debt.worst_files.is_empty());
}
