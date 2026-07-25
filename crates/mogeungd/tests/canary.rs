//! The format canary, end to end — roadmap `R-A1`, `R-A2`, `R-A5`.
//!
//! Unit tests prove `parse_line` classifies a line correctly. These prove the
//! daemon *notices*: that a classification reaches `Health`, becomes an alert,
//! and says something a human can act on. A canary nobody hears is not a canary.
//!
//! Each test builds its own synthetic `~/.claude`, so nothing reads the real one.

use mogeung_core::health::Alert;
use mogeungd::{state::AppState, store::Store};
use std::path::{Path, PathBuf};
use std::sync::Arc;

fn write(path: &Path, body: &str) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, body).unwrap();
}

/// A synthetic home with one dead session whose transcript is `lines`.
///
/// Dead rather than live so the test does not depend on pid liveness; the
/// transcript is read either way.
fn home_with_transcript(tag: &str, lines: &str) -> PathBuf {
    let home = std::env::temp_dir().join(format!("mogeung-canary-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&home);
    let id = "33333333-3333-3333-3333-333333333333";
    write(
        &home.join("projects").join("-tmp").join(format!("{id}.jsonl")),
        lines,
    );
    home
}

async fn boot(home: PathBuf) -> Arc<AppState> {
    let store = Store::open(&home.join("mogeung.db")).unwrap();
    let state = AppState::with_home(store, home).unwrap();
    state.scan().await;
    state
}

/// The whole point. A transcript type from the future must produce a named,
/// human-readable alert rather than vanishing into a catch-all.
#[tokio::test]
async fn an_unknown_event_type_reaches_the_health_report() {
    let home = home_with_transcript(
        "unknown",
        &[
            r#"{"type":"user","timestamp":"2026-07-25T10:00:00.000Z","cwd":"/tmp","message":{"role":"user","content":"hello"}}"#,
            r#"{"type":"holographic-projection","payload":{"x":1}}"#,
            r#"{"type":"holographic-projection","payload":{"x":2}}"#,
        ]
        .join("\n"),
    );
    let state = boot(home).await;
    let h = state.health().await;

    assert_eq!(h.lines_unknown, 2);
    assert_eq!(
        h.unknown_types,
        vec![("holographic-projection".to_string(), 2)]
    );
    assert!(h.blind_ratio() > 0.0, "unknown lines must count as blindness");
    assert_eq!(h.urgent_alerts(), 1);

    let msg = h.alerts[0].message();
    assert!(
        msg.contains("holographic-projection"),
        "the alert must name the type, got: {msg}"
    );
    assert!(
        h.headline().contains("cannot see"),
        "headline should not read as healthy: {}",
        h.headline()
    );
}

/// Bookkeeping we have classified must stay silent. If the canary cried wolf on
/// ordinary traffic, the user would learn to ignore it — which is exactly the
/// failure this feature exists to prevent.
#[tokio::test]
async fn ordinary_traffic_raises_nothing() {
    let home = home_with_transcript(
        "quiet",
        &[
            r#"{"type":"mode","mode":"normal","sessionId":"s"}"#,
            r#"{"type":"permission-mode","permissionMode":"acceptEdits","sessionId":"s"}"#,
            r#"{"type":"queue-operation","operation":"add","sessionId":"s"}"#,
            r#"{"type":"pr-link","prNumber":1,"sessionId":"s"}"#,
            r#"{"type":"frame-link","frameUrl":"u","sessionId":"s"}"#,
            r#"{"type":"file-history-snapshot","snapshot":{}}"#,
            r#"{"type":"user","timestamp":"2026-07-25T10:00:00.000Z","cwd":"/tmp","version":"2.1.220","message":{"role":"user","content":"hello"}}"#,
        ]
        .join("\n"),
    );
    let state = boot(home).await;
    let h = state.health().await;

    assert!(h.alerts.is_empty(), "unexpected alerts: {:?}", h.alerts);
    assert_eq!(h.lines_unknown, 0);
    assert_eq!(h.lines_malformed, 0);
    assert_eq!(h.blind_ratio(), 0.0);
    assert!(h.lines_ignored >= 6);
    assert_eq!(h.headline(), "Reading everything it recognises");
}

/// Truncated or corrupt lines are data we did not get. Counting them is the
/// only way "the board looks thin" becomes a checkable claim.
#[tokio::test]
async fn unreadable_lines_are_counted_not_dropped() {
    let home = home_with_transcript(
        "malformed",
        &[
            r#"{"type":"user","timestamp":"2026-07-25T10:00:00.000Z","cwd":"/tmp","message":{"role":"user","content":"hi"}}"#,
            r#"{"type":"assistant","message":{"content":[{"type":"tex"#,
            r#"not json at all"#,
        ]
        .join("\n"),
    );
    let state = boot(home).await;
    let h = state.health().await;

    assert_eq!(h.lines_malformed, 2);
    match h.alerts.iter().find(|a| matches!(a, Alert::MalformedLines { .. })) {
        Some(Alert::MalformedLines { count }) => assert_eq!(*count, 2),
        _ => panic!("malformed lines raised no alert: {:?}", h.alerts),
    }
}

/// R-A2. A version change is when formats move, so it is worth surfacing — but
/// as "look at this", not "something broke".
#[tokio::test]
async fn a_version_change_is_surfaced() {
    let home = home_with_transcript(
        "version",
        &[
            r#"{"type":"user","timestamp":"2026-07-25T10:00:00.000Z","cwd":"/tmp","version":"2.1.219","message":{"role":"user","content":"one"}}"#,
            r#"{"type":"user","timestamp":"2026-07-25T11:00:00.000Z","cwd":"/tmp","version":"2.1.220","message":{"role":"user","content":"two"}}"#,
        ]
        .join("\n"),
    );
    let state = boot(home).await;
    let h = state.health().await;

    assert_eq!(h.versions_seen, vec!["2.1.219", "2.1.220"]);
    match h.alerts.iter().find(|a| matches!(a, Alert::VersionChanged { .. })) {
        Some(a @ Alert::VersionChanged { from, to }) => {
            assert_eq!(from, "2.1.219");
            assert_eq!(to, "2.1.220");
            assert!(a.message().contains("2.1.220"));
        }
        _ => panic!("no version-change alert: {:?}", h.alerts),
    }
}

/// R-A5. A transcript over the cap must be followed from its tail, and the fact
/// that history was skipped must be stated rather than silent.
#[tokio::test]
async fn an_oversized_transcript_is_tailed_and_says_so() {
    // Comfortably over the 4 MiB cap.
    let mut body = String::with_capacity(6 << 20);
    let mut i = 0;
    while body.len() < (5 << 20) {
        body.push_str(&format!(
            r#"{{"type":"user","timestamp":"2026-07-25T10:00:00.000Z","cwd":"/tmp","message":{{"role":"user","content":"line {i} padding padding padding padding padding padding"}}}}"#
        ));
        body.push('\n');
        i += 1;
    }
    let home = home_with_transcript("oversized", &body);
    let state = boot(home).await;
    let h = state.health().await;

    assert!(
        h.history_skipped_bytes > 0,
        "an oversized transcript was read whole"
    );
    match h.alerts.iter().find(|a| matches!(a, Alert::HistorySkipped { .. })) {
        Some(a) => {
            assert!(!a.is_urgent(), "a stated cap is a limitation, not a fault");
            assert!(a.message().contains("MB"));
        }
        None => panic!("history was skipped without saying so: {:?}", h.alerts),
    }

    // Crucially: tailing must not manufacture blindness. Starting mid-file at a
    // line boundary means every line read is still valid JSON.
    assert_eq!(h.lines_malformed, 0, "the tail started mid-line");
    assert_eq!(h.lines_unknown, 0);
    assert!(h.lines_parsed > 0, "nothing was read from the tail");
}

/// Rescanning must not inflate the counters. A health report that climbs on its
/// own is one the user stops reading.
#[tokio::test]
async fn repeated_scans_do_not_double_count() {
    let home = home_with_transcript(
        "idempotent",
        r#"{"type":"user","timestamp":"2026-07-25T10:00:00.000Z","cwd":"/tmp","message":{"role":"user","content":"hi"}}"#,
    );
    let state = boot(home).await;
    let first = state.health().await;

    for _ in 0..3 {
        state.scan().await;
    }
    let after = state.health().await;

    assert_eq!(first.lines_seen, after.lines_seen);
    assert_eq!(after.scans, 4);
    assert!(after.last_scan >= first.last_scan);
}
