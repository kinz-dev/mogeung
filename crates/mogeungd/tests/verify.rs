//! Pillar E end to end — `R-E1`, `R-E3`, `R-E4`, and `R-G1`'s fold.
//!
//! Unit tests prove the classifier reads a line; these prove the *session*
//! ends up telling the truth: evidence rows paired with their results, claims
//! bound to evidence, the unverified mark only where it belongs, and a limit
//! hit that clears when work resumes.
//!
//! Each test builds its own synthetic `~/.claude`, the canary.rs pattern.

use mogeungd::{state::AppState, store::Store};
use std::path::{Path, PathBuf};
use std::sync::Arc;

const SID: &str = "44444444-4444-4444-4444-444444444444";

fn write(path: &Path, body: &str) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, body).unwrap();
}

fn home_with_transcript(tag: &str, lines: &[&str]) -> PathBuf {
    let home = std::env::temp_dir().join(format!("mogeung-verify-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&home);
    write(
        &home.join("projects").join("-tmp").join(format!("{SID}.jsonl")),
        &lines.join("\n"),
    );
    home
}

async fn boot(home: PathBuf) -> Arc<AppState> {
    let store = Store::open(&home.join("mogeung.db")).unwrap();
    let state = AppState::with_home(store, home).unwrap();
    state.scan().await;
    state
}

async fn session(state: &Arc<AppState>) -> mogeung_core::Session {
    state
        .sessions
        .read()
        .await
        .get(SID)
        .cloned()
        .expect("session discovered")
}

/// R-E1 + R-E3: a test run pairs with its result; a matching claim binds to
/// it and says how.
#[tokio::test]
async fn a_test_run_pairs_with_its_result_and_backs_the_claim() {
    let home = home_with_transcript(
        "pair",
        &[
            r#"{"type":"user","timestamp":"2026-07-25T10:00:00.000Z","cwd":"/tmp","message":{"role":"user","content":"fix it"}}"#,
            r#"{"type":"assistant","timestamp":"2026-07-25T10:01:00.000Z","message":{"content":[{"type":"tool_use","id":"toolu_t1","name":"Bash","input":{"command":"cargo test --workspace"}}]}}"#,
            r#"{"type":"user","timestamp":"2026-07-25T10:02:00.000Z","message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"toolu_t1","is_error":false,"content":"test result: ok"}]}}"#,
            r#"{"type":"assistant","timestamp":"2026-07-25T10:03:00.000Z","message":{"content":[{"type":"text","text":"All tests pass."}]}}"#,
        ],
    );
    let s = session(&boot(home).await).await;

    assert_eq!(s.verify_runs.len(), 1);
    assert_eq!(s.verify_runs[0].ok, Some(true), "the result must pair up");
    assert_eq!(s.claims.len(), 1);
    assert!(
        s.claims[0].evidence.contains("cargo test"),
        "the claim must say what backed it: {}",
        s.claims[0].evidence
    );
    assert!(!s.claims[0].contradicted);
    assert!(!s.unverified());
}

/// The most useful bit in the pillar: prose says pass, the run said fail.
#[tokio::test]
async fn a_claim_over_a_failed_run_is_contradicted() {
    let home = home_with_transcript(
        "contradict",
        &[
            r#"{"type":"assistant","timestamp":"2026-07-25T10:01:00.000Z","message":{"content":[{"type":"tool_use","id":"toolu_t1","name":"Bash","input":{"command":"cargo test"}}]}}"#,
            r#"{"type":"user","timestamp":"2026-07-25T10:02:00.000Z","message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"toolu_t1","is_error":true,"content":"2 failed"}]}}"#,
            r#"{"type":"assistant","timestamp":"2026-07-25T10:03:00.000Z","message":{"content":[{"type":"text","text":"Great - all tests pass now."}]}}"#,
        ],
    );
    let s = session(&boot(home).await).await;
    assert!(s.claims[0].contradicted, "prose must not outrank the exit code");
}

/// R-E3's honesty rule: a claim with no run behind it says exactly that.
#[tokio::test]
async fn a_claim_with_no_run_is_visibly_unbacked() {
    let home = home_with_transcript(
        "unbacked",
        &[
            r#"{"type":"assistant","timestamp":"2026-07-25T10:00:00.000Z","message":{"content":[{"type":"text","text":"Tests pass."},{"type":"tool_use","id":"toolu_e","name":"Edit","input":{"file_path":"/tmp/a.rs"}}]}}"#,
        ],
    );
    let s = session(&boot(home).await).await;
    assert!(
        s.claims[0].evidence.contains("no test run seen"),
        "got: {}",
        s.claims[0].evidence
    );
}

/// R-E4: edits with no completed check wear the mark; a *failed* check is
/// still a check.
#[tokio::test]
async fn unverified_means_no_completed_check_not_no_passing_check() {
    let edited_only = home_with_transcript(
        "unverified",
        &[
            r#"{"type":"assistant","timestamp":"2026-07-25T10:00:00.000Z","message":{"content":[{"type":"tool_use","id":"toolu_e","name":"Edit","input":{"file_path":"/tmp/a.rs"}}]}}"#,
        ],
    );
    let s = session(&boot(edited_only).await).await;
    assert!(s.unverified());

    let checked_and_failed = home_with_transcript(
        "failedcheck",
        &[
            r#"{"type":"assistant","timestamp":"2026-07-25T10:00:00.000Z","message":{"content":[{"type":"tool_use","id":"toolu_e","name":"Edit","input":{"file_path":"/tmp/a.rs"}}]}}"#,
            r#"{"type":"assistant","timestamp":"2026-07-25T10:01:00.000Z","message":{"content":[{"type":"tool_use","id":"toolu_t","name":"Bash","input":{"command":"cargo test"}}]}}"#,
            r#"{"type":"user","timestamp":"2026-07-25T10:02:00.000Z","message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"toolu_t","is_error":true,"content":"1 failed"}]}}"#,
        ],
    );
    let s = session(&boot(checked_and_failed).await).await;
    assert!(!s.unverified(), "they checked; the answer being no is not 'unverified'");
}

/// R-G1's fold: a synthetic limit message pins the session; the next human
/// turn clears it.
#[tokio::test]
async fn a_limit_hit_pins_the_session_until_work_resumes() {
    let hit_only = home_with_transcript(
        "limit",
        &[
            r#"{"type":"assistant","timestamp":"2026-07-25T10:00:00.000Z","message":{"model":"<synthetic>","content":[{"type":"text","text":"You've hit your session limit · resets 8pm (Europe/London)"}]}}"#,
        ],
    );
    let s = session(&boot(hit_only).await).await;
    assert!(s.limit_hit_at.is_some());
    assert_eq!(s.limit_resets.as_deref(), Some("8pm (Europe/London)"));

    let resumed = home_with_transcript(
        "limitclear",
        &[
            r#"{"type":"assistant","timestamp":"2026-07-25T10:00:00.000Z","message":{"model":"<synthetic>","content":[{"type":"text","text":"You've hit your session limit · resets 8pm (Europe/London)"}]}}"#,
            r#"{"type":"user","timestamp":"2026-07-25T20:05:00.000Z","cwd":"/tmp","message":{"role":"user","content":"carry on"}}"#,
        ],
    );
    let s = session(&boot(resumed).await).await;
    assert!(s.limit_hit_at.is_none(), "a new turn means the limit reset");
}
