//! The retention pass (`R-J57`): sessions nobody can act on stop paying rent.
//!
//! Everything the store holds for a session is derived and re-derivable; what
//! is not derived is the user's own writing, so a session a note anchors to
//! survives. And a session whose transcript *file* is still inside the scan
//! window is never pruned however old its own timestamps read — the scan
//! filters on mtime and would re-adopt (and re-read from byte 0) on the very
//! next pass, which is a churn loop wearing a cleanup's clothes.

use mogeung_core::wire::Note;
use mogeung_core::session::SessionSource;
use mogeung_core::Session;
use mogeungd::{state::AppState, store::Store};
use std::path::PathBuf;

fn home_for(tag: &str) -> PathBuf {
    let home = std::env::temp_dir().join(format!("mogeung-ret-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&home);
    home
}

/// A dead session as the store would have kept it: last active `days_ago`,
/// its transcript pointing wherever `transcript` says.
fn aged_session(id: &str, days_ago: i64, transcript: &str) -> Session {
    let then = chrono::Utc::now() - chrono::Duration::days(days_ago);
    Session {
        id: id.into(),
        title: None,
        name: None,
        last_prompt: None,
        cwd: "/tmp".into(),
        repo_root: None,
        git_branch: None,
        pid: None,
        alive: false,
        live_status: None,
        version: None,
        started_at: then,
        last_event_at: then,
        status_since: None,
        turns: 1,
        tool_calls: 0,
        tokens_in: 0,
        tokens_out: 0,
        last_activity: None,
        touched_files: vec![],
        base_sha: None,
        files_changed: 0,
        insertions: 0,
        deletions: 0,
        error: None,
        transcript_path: transcript.into(),
        reviewed: false,
        open_tools: vec![],
        snoozed_until: None,
        collisions: vec![],
        loop_signal: None,
        recent_touches: vec![],
        tmux_target: None,
        recent_tools: vec![],
        limit_hit_at: None,
        limit_resets: None,
        verify_runs: vec![],
        claims: vec![],
        source: SessionSource::ClaudeCode,
        announced_dirs: vec![],
    }
}

#[tokio::test]
async fn stale_sessions_are_pruned_and_protected_ones_survive() {
    let home = home_for("prune");
    std::fs::create_dir_all(home.join("projects")).unwrap();

    // A transcript file that still exists with a fresh mtime — the case the
    // canary caught: pruning it would re-adopt and re-read it next pass.
    let watched_path = home.join("projects").join("watched.jsonl");
    std::fs::write(&watched_path, "").unwrap();

    let stale = aged_session("stale-1", 40, "/nowhere/gone.jsonl");
    let noted = aged_session("noted-1", 40, "/nowhere/gone-too.jsonl");
    let watched = aged_session("watched-1", 40, &watched_path.to_string_lossy());
    let fresh = aged_session("fresh-1", 1, "/nowhere/recent.jsonl");

    let store = Store::open(&home.join("mogeung.db")).unwrap();
    for s in [&stale, &noted, &watched, &fresh] {
        store.save_session(s).unwrap();
    }
    store
        .save_note(&Note {
            id: "n1".into(),
            body: "worth keeping".into(),
            created: 1,
            updated: 1,
            session_id: Some(noted.id.clone()),
            seq: None,
            repo: None,
        })
        .unwrap();
    let state = AppState::with_home(store, home).unwrap();

    state.prune_stale_sessions().await;

    assert!(state.get(&stale.id).await.is_none(), "a 40-day-idle session is pruned");
    assert!(state.get(&noted.id).await.is_some(), "a noted session survives, however old");
    assert!(
        state.get(&watched.id).await.is_some(),
        "a session whose transcript file is still in the scan window survives"
    );
    assert!(state.get(&fresh.id).await.is_some(), "a fresh session is untouched");

    // The store agrees with memory — the row went with the map entry.
    let kept: Vec<String> = state
        .store
        .load_sessions()
        .unwrap()
        .into_iter()
        .map(|s| s.id)
        .collect();
    assert!(!kept.contains(&stale.id));
    assert!(kept.contains(&noted.id));
    assert!(kept.contains(&watched.id));
    assert!(kept.contains(&fresh.id));
}
