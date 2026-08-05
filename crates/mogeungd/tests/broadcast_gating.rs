//! What the daemon says, and — as important now — what it stays quiet about.
//!
//! The scan loop used to end every pass with a `Queue` broadcast and re-send
//! the full diff (`ChangeUpdated`, all hunks, all lines) for every session
//! whose transcript grew, changed or not. Every client parsed and re-applied
//! all of it at the poll rate. These tests pin the gates: unchanged state is
//! not re-announced by the scan path, while a client that *asks* still gets
//! its answer even when nothing moved.
//!
//! Also here: parity between the in-memory rendering of untracked files and
//! `git diff --no-index`, which it replaced. The hunk anchor hashes the `+`
//! lines, so byte-for-byte agreement is what keeps review marks stuck to
//! untracked files across the change.

use mogeung_core::ServerMsg;
use mogeungd::{git, state::AppState, store::Store};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;

fn write(path: &Path, body: &str) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, body).unwrap();
}

fn home_for(tag: &str) -> PathBuf {
    let home = std::env::temp_dir().join(format!("mogeung-gate-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&home);
    home
}

fn git_in(dir: &Path, args: &[&str]) -> std::process::Output {
    Command::new("git")
        .args(args)
        .current_dir(dir)
        .output()
        .unwrap_or_else(|e| panic!("git {args:?}: {e}"))
}

/// A repository with one commit, its own identity, and nothing global.
fn repo(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("mogeung-gate-repo-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    for args in [
        vec!["init", "-q", "-b", "main"],
        vec!["config", "user.email", "t@example.com"],
        vec!["config", "user.name", "t"],
        vec!["config", "commit.gpgsign", "false"],
    ] {
        assert!(git_in(&dir, &args).status.success(), "git {args:?} failed");
    }
    std::fs::write(dir.join("kept.txt"), "one\n").unwrap();
    assert!(git_in(&dir, &["add", "kept.txt"]).status.success());
    assert!(git_in(&dir, &["commit", "-q", "-m", "first"]).status.success());
    dir
}

/// A dead session (transcript only, no live registry entry) whose one edit
/// happened in `cwd`.
fn dead_session(home: &Path, id: &str, cwd: &str) {
    let now = chrono::Utc::now().to_rfc3339();
    write(
        &home.join("projects").join("-x").join(format!("{id}.jsonl")),
        &format!(
            r#"{{"type":"assistant","timestamp":"{now}","cwd":"{cwd}","message":{{"content":[{{"type":"tool_use","id":"t1","name":"Edit","input":{{"file_path":"{cwd}/kept.txt"}}}}]}}}}
{{"type":"user","timestamp":"{now}","cwd":"{cwd}","message":{{"role":"user","content":[{{"type":"tool_result","tool_use_id":"t1","is_error":false,"content":"ok"}}]}}}}"#
        ),
    );
}

fn drain(rx: &mut tokio::sync::broadcast::Receiver<ServerMsg>) -> Vec<ServerMsg> {
    let mut out = Vec::new();
    while let Ok(msg) = rx.try_recv() {
        out.push(msg);
    }
    out
}

fn queues(msgs: &[ServerMsg]) -> usize {
    msgs.iter().filter(|m| matches!(m, ServerMsg::Queue { .. })).count()
}

fn change_updates(msgs: &[ServerMsg]) -> usize {
    msgs.iter()
        .filter(|m| matches!(m, ServerMsg::ChangeUpdated { .. }))
        .count()
}

/// The scan loop must not re-announce a queue nobody changed. Before the
/// gate, this received one `Queue` per scan, forever.
#[tokio::test]
async fn an_unchanged_queue_is_not_rebroadcast() {
    let home = home_for("queue");
    dead_session(&home, "aaaaaaaa-1111-1111-1111-111111111111", "/tmp");

    let store = Store::open(&home.join("mogeung.db")).unwrap();
    let state = AppState::with_home(store, home).unwrap();

    let mut rx = state.tx.subscribe();
    state.scan().await;
    let first = drain(&mut rx);
    assert_eq!(queues(&first), 1, "the first scan must announce the queue");

    state.scan().await;
    state.scan().await;
    let after = drain(&mut rx);
    assert_eq!(
        queues(&after),
        0,
        "nothing changed, so nothing should have been re-announced: {after:?}"
    );
}

/// An unchanged diff is silent on the scan path but still answered on the
/// request path — the client that clicked refresh is owed a reply either way.
#[tokio::test]
async fn an_unchanged_diff_is_silent_on_scan_but_answered_on_request() {
    let dir = repo("diff");
    let cwd = dir.to_string_lossy().to_string();
    let home = home_for("diff");
    let id = "cccccccc-3333-3333-3333-333333333333";
    dead_session(&home, id, &cwd);

    let store = Store::open(&home.join("mogeung.db")).unwrap();
    let state: Arc<AppState> = AppState::with_home(store, home).unwrap();
    state.scan().await;

    // Real work to diff, so the change is non-trivial both times.
    std::fs::write(dir.join("kept.txt"), "one\ntwo\n").unwrap();
    std::fs::write(dir.join("fresh.txt"), "new\n").unwrap();

    let mut rx = state.tx.subscribe();
    state.recompute_change_scan(id).await.expect("a change");
    let first = drain(&mut rx);
    assert_eq!(change_updates(&first), 1, "a new diff must be announced");

    state.recompute_change_scan(id).await.expect("a change");
    let second = drain(&mut rx);
    assert_eq!(change_updates(&second), 0, "an identical diff must not be");

    state.recompute_change(id).await.expect("a change");
    let asked = drain(&mut rx);
    assert_eq!(change_updates(&asked), 1, "an explicit request is always answered");

    std::fs::remove_dir_all(&dir).ok();
}

/// The in-memory rendering of an untracked file must parse to the same hunks
/// `git diff --no-index` produced, `+` line for `+` line — the anchors hash
/// those lines, and review marks live by the anchors.
#[test]
fn untracked_files_match_gits_own_no_index_diff() {
    let dir = repo("parity");
    let base = String::from_utf8_lossy(&git_in(&dir, &["rev-parse", "HEAD"]).stdout)
        .trim()
        .to_string();

    std::fs::write(dir.join("plain.txt"), "alpha\nbeta\n").unwrap();
    std::fs::write(dir.join("no-eol.txt"), "no newline at end").unwrap();
    std::fs::write(dir.join("crlf.txt"), "one\r\ntwo\r\n").unwrap();

    let change = git::compute_change(&dir, Some(&base), &Default::default());

    for name in ["plain.txt", "no-eol.txt", "crlf.txt"] {
        let ours = change
            .files
            .iter()
            .find(|f| f.path == name)
            .unwrap_or_else(|| panic!("{name} missing from change"));
        let out = git_in(&dir, &["diff", "--no-color", "--no-index", "--", "/dev/null", name]);
        let text = String::from_utf8_lossy(&out.stdout);
        let git_plus: Vec<&str> = text
            .lines()
            .filter(|l| l.starts_with('+') && !l.starts_with("+++"))
            .collect();
        let our_lines: Vec<&str> = ours
            .hunks
            .iter()
            .flat_map(|h| h.lines.iter().map(String::as_str))
            .collect();
        assert_eq!(our_lines, git_plus, "{name} diverged from git's own diff");
    }

    std::fs::remove_dir_all(&dir).ok();
}
