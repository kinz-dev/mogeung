//! Which files a session's diff is credited with.
//!
//! Several sessions can share a working tree, so a session's change is
//! narrowed to the files it actually touched. The narrowing is only sound for
//! touches that landed *inside* the repo, and it used to be applied whenever
//! the touch list was non-empty at all — so a session whose only recorded
//! write was a scratchpad under `/tmp` or a note under `~/.claude` matched
//! nothing and was credited with an empty diff while its worktree held real
//! work. `R-J62`.

use mogeungd::{state::AppState, store::Store};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;

fn git_in(dir: &Path, args: &[&str]) -> std::process::Output {
    Command::new("git")
        .args(args)
        .current_dir(dir)
        .output()
        .unwrap_or_else(|e| panic!("git {args:?}: {e}"))
}

fn home_for(tag: &str) -> PathBuf {
    let home = std::env::temp_dir().join(format!("mogeung-attr-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&home);
    home
}

/// A repository with two committed files, so narrowing to one is observable.
fn repo(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("mogeung-attr-repo-{tag}-{}", std::process::id()));
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
    std::fs::write(dir.join("other.txt"), "one\n").unwrap();
    assert!(git_in(&dir, &["add", "-A"]).status.success());
    assert!(git_in(&dir, &["commit", "-q", "-m", "first"]).status.success());
    dir
}

/// A dead session (transcript only) that reports one `Edit` per given path.
fn session_touching(home: &Path, id: &str, cwd: &str, paths: &[&str]) {
    let now = chrono::Utc::now().to_rfc3339();
    let mut lines = Vec::new();
    for (i, path) in paths.iter().enumerate() {
        lines.push(format!(
            r#"{{"type":"assistant","timestamp":"{now}","cwd":"{cwd}","message":{{"content":[{{"type":"tool_use","id":"t{i}","name":"Edit","input":{{"file_path":"{path}"}}}}]}}}}"#
        ));
        lines.push(format!(
            r#"{{"type":"user","timestamp":"{now}","cwd":"{cwd}","message":{{"role":"user","content":[{{"type":"tool_result","tool_use_id":"t{i}","is_error":false,"content":"ok"}}]}}}}"#
        ));
    }
    let file = home.join("projects").join("-x").join(format!("{id}.jsonl"));
    std::fs::create_dir_all(file.parent().unwrap()).unwrap();
    std::fs::write(file, lines.join("\n")).unwrap();
}

/// Both tracked files carry real work, so an empty diff is unambiguous.
fn dirty(dir: &Path) {
    std::fs::write(dir.join("kept.txt"), "one\ntwo\n").unwrap();
    std::fs::write(dir.join("other.txt"), "one\ntwo\n").unwrap();
}

async fn changed_paths(state: &Arc<AppState>, id: &str) -> Vec<String> {
    let change = state.recompute_change(id).await.expect("a change");
    let mut paths: Vec<String> = change.files.iter().map(|f| f.path.clone()).collect();
    paths.sort();
    paths
}

/// The regression: every recorded touch was outside the repo, so the filter
/// matched nothing and blanked a diff that was really there. A session whose
/// touches cannot attribute anything must fall back to the whole worktree
/// diff — the same answer a session with no touches at all already got.
#[tokio::test]
async fn touches_outside_the_repo_do_not_blank_the_diff() {
    let dir = repo("outside");
    let cwd = dir.to_string_lossy().to_string();
    let home = home_for("outside");
    let id = "aaaaaaaa-1111-1111-1111-111111111111";
    let scratch = std::env::temp_dir().join("some-scratchpad/notes.md");
    session_touching(
        &home,
        id,
        &cwd,
        &[&scratch.to_string_lossy(), "/home/nobody/.claude/memory/a.md"],
    );

    let store = Store::open(&home.join("mogeung.db")).unwrap();
    let state: Arc<AppState> = AppState::with_home(store, home).unwrap();
    state.scan().await;
    dirty(&dir);

    let session = state.get(id).await.expect("the session");
    assert_eq!(
        session.touched_files.len(),
        2,
        "the touches were recorded; it is the attribution that must cope"
    );

    assert_eq!(
        changed_paths(&state, id).await,
        vec!["kept.txt".to_string(), "other.txt".to_string()],
        "no touch could attribute anything, so the whole worktree diff stands"
    );

    std::fs::remove_dir_all(&dir).ok();
}

/// The behaviour the fix must not cost: a touch inside the repo still narrows
/// the diff to that file, which is how two sessions sharing a worktree stay
/// told apart.
#[tokio::test]
async fn a_touch_inside_the_repo_still_narrows_the_diff() {
    let dir = repo("inside");
    let cwd = dir.to_string_lossy().to_string();
    let home = home_for("inside");
    let id = "bbbbbbbb-2222-2222-2222-222222222222";
    session_touching(&home, id, &cwd, &[&format!("{cwd}/kept.txt")]);

    let store = Store::open(&home.join("mogeung.db")).unwrap();
    let state: Arc<AppState> = AppState::with_home(store, home).unwrap();
    state.scan().await;
    dirty(&dir);

    assert_eq!(
        changed_paths(&state, id).await,
        vec!["kept.txt".to_string()],
        "the session touched one file, so it is credited with one"
    );

    std::fs::remove_dir_all(&dir).ok();
}

/// A scratchpad write alongside real work must not widen the diff back to the
/// whole worktree either — the out-of-repo path is dropped, not the filter.
#[tokio::test]
async fn an_out_of_repo_touch_does_not_widen_a_diff_that_can_be_attributed() {
    let dir = repo("mixed");
    let cwd = dir.to_string_lossy().to_string();
    let home = home_for("mixed");
    let id = "cccccccc-3333-3333-3333-333333333333";
    let scratch = std::env::temp_dir().join("some-scratchpad/plan.md");
    session_touching(
        &home,
        id,
        &cwd,
        &[&scratch.to_string_lossy(), &format!("{cwd}/kept.txt")],
    );

    let store = Store::open(&home.join("mogeung.db")).unwrap();
    let state: Arc<AppState> = AppState::with_home(store, home).unwrap();
    state.scan().await;
    dirty(&dir);

    assert_eq!(
        changed_paths(&state, id).await,
        vec!["kept.txt".to_string()],
        "the in-repo touch attributes; the scratchpad is simply not a candidate"
    );

    std::fs::remove_dir_all(&dir).ok();
}
