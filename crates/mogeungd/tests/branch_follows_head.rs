//! A live session's branch is its repository's `HEAD`, read on every pass.
//! `R-J110`.
//!
//! Asked 2026-09-25: *"I have changed the git branch but it didn't refresh
//! immediately, and I have to run some command then it will get refresh"*. The
//! branch came only from the `gitBranch` Claude Code stamps on each transcript
//! line, so a `git switch` in another terminal showed when the session next
//! spoke. Each test here fails against that: the first because nothing is
//! written after the switch, the second because the stamp would put the old
//! branch straight back.
//!
//! Costs nothing: no agent is ever launched.

use mogeung_core::ServerMsg;
use mogeungd::{state::AppState, store::Store};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;

const ID: &str = "b1b1b1b1-1111-1111-1111-111111111111";

fn write(path: &Path, body: &str) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, body).unwrap();
}

fn git_in(dir: &Path, args: &[&str]) {
    let out = Command::new("git")
        .args(args)
        .current_dir(dir)
        .output()
        .unwrap_or_else(|e| panic!("git {args:?}: {e}"));
    assert!(out.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&out.stderr));
}

/// A repository with one commit on `main`, its own identity, and nothing global.
fn repo(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("mogeung-branch-repo-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    for args in [
        &["init", "-q", "-b", "main"][..],
        &["config", "user.email", "t@example.com"],
        &["config", "user.name", "t"],
        &["config", "commit.gpgsign", "false"],
    ] {
        git_in(&dir, args);
    }
    std::fs::write(dir.join("kept.txt"), "one\n").unwrap();
    git_in(&dir, &["add", "kept.txt"]);
    git_in(&dir, &["commit", "-q", "-m", "first"]);
    dir
}

fn line(cwd: &str, branch: &str, text: &str) -> String {
    let now = chrono::Utc::now().to_rfc3339();
    format!(
        r#"{{"type":"user","timestamp":"{now}","cwd":"{cwd}","gitBranch":"{branch}","message":{{"role":"user","content":"{text}"}}}}"#
    )
}

/// A session in `cwd` whose transcript stamps `branch`; live when `alive`
/// (our own pid, so the liveness check passes), ended otherwise.
async fn boot(tag: &str, cwd: &str, branch: &str, alive: bool) -> (Arc<AppState>, PathBuf) {
    let home = std::env::temp_dir().join(format!("mogeung-branch-home-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&home);
    let pid = if alive { std::process::id() } else { 999_999 };
    write(
        &home.join("sessions").join(format!("{pid}.json")),
        &format!(
            r#"{{"pid":{pid},"sessionId":"{ID}","cwd":"{cwd}","kind":"interactive","status":"idle",
                "startedAt":1784995957755,"statusUpdatedAt":1784995999000}}"#
        ),
    );
    let transcript = home.join("projects").join("-repo").join(format!("{ID}.jsonl"));
    write(&transcript, &format!("{}\n", line(cwd, branch, "first")));
    let store = Store::open(&home.join("mogeung.db")).unwrap();
    let state = AppState::with_home(store, home).unwrap();
    state.scan().await;
    (state, transcript)
}

async fn branch(state: &AppState) -> Option<String> {
    state.sessions.read().await.get(ID).unwrap().git_branch.clone()
}

/// What was said about this session's branch, in order.
fn announced(rx: &mut tokio::sync::broadcast::Receiver<ServerMsg>) -> Vec<Option<String>> {
    let mut out = Vec::new();
    while let Ok(msg) = rx.try_recv() {
        if let ServerMsg::SessionUpdated { session } = msg {
            if session.id == ID {
                out.push(session.git_branch.clone());
            }
        }
    }
    out
}

#[tokio::test]
async fn a_switch_shows_on_the_next_pass_with_no_transcript_line() {
    let dir = repo("switch");
    let (state, _) = boot("switch", &dir.to_string_lossy(), "main", true).await;
    assert_eq!(branch(&state).await.as_deref(), Some("main"));

    let mut rx = state.tx.subscribe();
    git_in(&dir, &["checkout", "-q", "-b", "feature/ENG-441"]);
    state.scan().await;
    assert_eq!(branch(&state).await.as_deref(), Some("feature/ENG-441"));
    assert_eq!(announced(&mut rx), vec![Some("feature/ENG-441".to_string())]);

    // And an idle machine says nothing further.
    state.scan().await;
    assert_eq!(announced(&mut rx), Vec::<Option<String>>::new());
}

/// Claude Code's stamp can trail a switch; a line written with the old
/// branch must not flip the header back, even for a moment.
#[tokio::test]
async fn a_stale_stamp_does_not_put_the_old_branch_back() {
    let dir = repo("stale");
    let cwd = dir.to_string_lossy().to_string();
    let (state, transcript) = boot("stale", &cwd, "main", true).await;
    git_in(&dir, &["checkout", "-q", "-b", "next"]);
    state.scan().await;

    let mut rx = state.tx.subscribe();
    let mut body = std::fs::read_to_string(&transcript).unwrap();
    body.push_str(&format!("{}\n", line(&cwd, "main", "a prompt written as it switched")));
    std::fs::write(&transcript, body).unwrap();
    state.scan().await;

    assert_eq!(branch(&state).await.as_deref(), Some("next"));
    assert!(
        !announced(&mut rx).contains(&Some("main".to_string())),
        "the old branch went out on the wire"
    );
}

#[tokio::test]
async fn a_detached_head_reads_as_no_branch() {
    let dir = repo("detached");
    let (state, _) = boot("detached", &dir.to_string_lossy(), "main", true).await;
    git_in(&dir, &["checkout", "-q", "--detach"]);
    state.scan().await;
    assert_eq!(branch(&state).await, None);
}

/// An ended session's branch is the one it ran on — history, not the
/// checkout's present, which may have moved on for someone else's reasons.
#[tokio::test]
async fn an_ended_session_keeps_the_branch_it_ran_on() {
    let dir = repo("ended");
    let (state, _) = boot("ended", &dir.to_string_lossy(), "old-work", false).await;
    state.scan().await;
    assert_eq!(branch(&state).await.as_deref(), Some("old-work"));
}
