//! A session's published artifacts, read from a synthetic `~/.claude`. `R-J115`.
//!
//! The parser's tests pin one line; this pins the fold across lines, which is
//! where the shape of `artifact-comment-monitor` matters: each line is the whole
//! list so far, so reading it as one addition or as the final word would both
//! pass a single-line test and both be wrong here.

use mogeungd::{state::AppState, store::Store};
use std::path::Path;

fn write(path: &Path, body: &str) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, body).unwrap();
}

const OLD: &str = "4441b4aa-62f5-45c7-9410-5fb1ce6eac01";
const NEW: &str = "eabc93e7-90b3-4df5-8aa3-586c831393e8";

const ID: &str = "33333333-3333-3333-3333-333333333333";

fn monitor(entries: &[(&str, u64, &str)]) -> String {
    let body: Vec<String> = entries
        .iter()
        .map(|(uuid, at, title)| format!(r#""{uuid}":{{"state":"armed","writtenAtMs":{at},"title":"{title}"}}"#))
        .collect();
    format!(
        r#"{{"type":"artifact-comment-monitor","v":1,"sessionId":"{ID}","artifacts":{{{}}}}}"#,
        body.join(",")
    )
}

/// One prompt, then the monitor as the CLI writes it across a session.
fn home(tag: &str) -> std::path::PathBuf {
    let home = std::env::temp_dir().join(format!("mogeung-artifacts-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&home);
    write(
        &home.join("projects").join("-tmp").join(format!("{ID}.jsonl")),
        &[
            r#"{"type":"user","timestamp":"2026-10-01T10:00:00.000Z","cwd":"/tmp","message":{"role":"user","content":"draw it"}}"#.to_string(),
            monitor(&[(OLD, 1000, "First draft")]),
            // Renamed on republish, and a second page beside it.
            monitor(&[(NEW, 2000, "The map"), (OLD, 1000, "Where balances live")]),
            // The older one has left the watch list. It has not left claude.ai.
            monitor(&[(NEW, 2000, "The map")]),
        ]
        .join("\n"),
    );
    home
}

async fn boot(home: &Path) -> std::sync::Arc<AppState> {
    let store = Store::open(&home.join("mogeung.db")).unwrap();
    let state = AppState::with_home(store, home.to_path_buf()).unwrap();
    state.scan().await;
    state
}

async fn artifacts(state: &AppState) -> Vec<(String, String)> {
    let sessions = state.sessions.read().await;
    let s = sessions.get(ID).expect("session missing");
    s.artifacts.iter().map(|a| (a.id.clone(), a.title.clone())).collect()
}

fn expected() -> Vec<(String, String)> {
    vec![(NEW.into(), "The map".into()), (OLD.into(), "Where balances live".into())]
}

#[tokio::test]
async fn a_session_keeps_every_artifact_it_published_newest_first() {
    let home = home("fold");
    let state = boot(&home).await;
    assert_eq!(artifacts(&state).await, expected());
    assert_eq!(
        state.sessions.read().await[ID].artifacts[0].url,
        "https://claude.ai/artifact/VzCr55rMjnsNhymTNAA12P"
    );
    let _ = std::fs::remove_dir_all(&home);
}

/// A session read by a build before `R-J115` has its monitor lines behind the
/// saved offset, so the scan never sees them again. The backfill reads just
/// those lines — and must not count anything else a second time.
#[tokio::test]
async fn a_session_read_before_artifacts_existed_gets_them_once() {
    let home = home("backfill");
    let state = boot(&home).await;
    let turns = state.sessions.read().await[ID].turns;
    // What an older build left behind: the session, read, with no artifacts.
    state.sessions.write().await.get_mut(ID).unwrap().artifacts.clear();
    state.store.set_schema_version(1).unwrap();

    state.backfill_artifacts().await.unwrap();
    assert_eq!(artifacts(&state).await, expected());
    assert_eq!(state.sessions.read().await[ID].turns, turns, "only monitor lines are folded");
    assert_eq!(state.store.schema_version(), 2, "the artifact pass's own mark");

    // Once: a second call finds the version set and reads nothing.
    state.sessions.write().await.get_mut(ID).unwrap().artifacts.clear();
    state.backfill_artifacts().await.unwrap();
    assert!(artifacts(&state).await.is_empty());
    let _ = std::fs::remove_dir_all(&home);
}
