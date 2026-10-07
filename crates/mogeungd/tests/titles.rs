//! A session's name, read from a synthetic `~/.claude`. `R-J120`.
//!
//! The parser's tests pin one line; this pins the fold across lines, which is
//! where `custom-title` matters: Claude Code re-emits the rename **interleaved**
//! with its generated `ai-title` for the rest of the session, so a fold that
//! took whichever came last would flip the queue's name back and forth — and
//! would pass any single-line test.

use mogeungd::{state::AppState, store::Store};
use std::path::Path;

fn write(path: &Path, body: &str) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, body).unwrap();
}

const ID: &str = "44444444-4444-4444-4444-444444444444";

fn ai(t: &str) -> String {
    format!(r#"{{"type":"ai-title","aiTitle":"{t}","sessionId":"{ID}"}}"#)
}

fn custom(t: &str) -> String {
    format!(r#"{{"type":"custom-title","customTitle":"{t}","sessionId":"{ID}"}}"#)
}

/// The order the CLI writes them in: a generated title, then a `/rename`, then
/// the generated one again — ending on `ai-title`, which is the case that
/// matters, since last-writer-wins would end on the wrong name.
fn home(tag: &str) -> std::path::PathBuf {
    let home = std::env::temp_dir().join(format!("mogeung-titles-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&home);
    write(
        &home.join("projects").join("-tmp").join(format!("{ID}.jsonl")),
        &[
            r#"{"type":"user","timestamp":"2026-10-07T10:00:00.000Z","cwd":"/tmp","message":{"role":"user","content":"compare the adapters"}}"#.to_string(),
            ai("Market data adapter gap analysis"),
            custom("MDA - venue"),
            ai("Market data adapter gap analysis"),
            format!(r#"{{"type":"agent-name","agentName":"MDA - venue","sessionId":"{ID}"}}"#),
            ai("Market data adapter gap analysis"),
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

async fn title(state: &AppState) -> Option<String> {
    state.sessions.read().await[ID].title.clone()
}

#[tokio::test]
async fn a_rename_holds_against_the_generated_titles_written_after_it() {
    let home = home("fold");
    let state = boot(&home).await;
    assert_eq!(title(&state).await.as_deref(), Some("MDA - venue"));
    let _ = std::fs::remove_dir_all(&home);
}

/// Before any rename, the generated title is the name — the pin must not stop
/// `ai-title` doing the job it did before `R-J120`.
#[tokio::test]
async fn an_unrenamed_session_keeps_its_generated_title() {
    let home = std::env::temp_dir().join(format!("mogeung-titles-plain-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&home);
    write(
        &home.join("projects").join("-tmp").join(format!("{ID}.jsonl")),
        &[
            r#"{"type":"user","timestamp":"2026-10-07T10:00:00.000Z","cwd":"/tmp","message":{"role":"user","content":"hi"}}"#.to_string(),
            ai("First guess"),
            ai("Better guess"),
        ]
        .join("\n"),
    );
    let state = boot(&home).await;
    assert_eq!(title(&state).await.as_deref(), Some("Better guess"));
    let _ = std::fs::remove_dir_all(&home);
}

/// A session read by a build before `R-J120` has its rename behind the saved
/// offset, so the scan never sees it again. The backfill reads just those
/// lines — and must not count anything else a second time.
#[tokio::test]
async fn a_session_renamed_before_this_build_gets_its_name_once() {
    let home = home("backfill");
    let state = boot(&home).await;
    let turns = state.sessions.read().await[ID].turns;
    // What an older build left behind: the generated title, and no pin.
    {
        let mut sessions = state.sessions.write().await;
        let s = sessions.get_mut(ID).unwrap();
        s.title = Some("Market data adapter gap analysis".into());
        s.custom_title = None;
    }
    state.store.set_schema_version(2).unwrap();

    state.backfill_custom_titles().await.unwrap();
    assert_eq!(title(&state).await.as_deref(), Some("MDA - venue"));
    assert_eq!(state.sessions.read().await[ID].turns, turns, "only rename lines are folded");
    assert_eq!(state.store.schema_version(), mogeungd::store::SCHEMA_VERSION);

    // Once: a second call finds the version set and reads nothing.
    state.sessions.write().await.get_mut(ID).unwrap().title = None;
    state.backfill_custom_titles().await.unwrap();
    assert_eq!(title(&state).await, None);
    let _ = std::fs::remove_dir_all(&home);
}
