//! The hook-event corpus. `R-P5`, feature 0051 — the discipline `R-A3` gave
//! transcript lines, applied to what a seat's hooks post.
//!
//! `fixtures/hooks.jsonl` holds one line per hook *shape* a seat's CLI
//! hands the receiver: Claude Code's hook events and its status line, the
//! same names from a Qwen Code seat in a home of its own, and the opencode
//! bus events the generated plugin forwards. Each line says which CLI and
//! which route, then the body as the CLI would post it.
//!
//! **It contains no real data.** The shapes follow the CLIs' documented
//! payloads and the key names in their bundles (Claude Code 2.1.296, Qwen
//! Code 0.23.0, opencode 1.18.35, read 2026-10-10); every value is
//! synthetic. The receiver keeps real captures beside each seat's files,
//! under `~/.mogeung`, and that is where they stay.
//!
//! What this pins: that nothing a seat is subscribed to is unclassified,
//! that every handled name has a shape here, and that the detail the row
//! shows is read from the keys the fixtures carry — so a CLI renaming
//! `notification_type` or `properties.title` fails here, loudly.

use mogeung_core::session::SessionSource;
use mogeungd::hooks::{classify, Received, Route, CLAUDE_HANDLED, OPENCODE_HANDLED};

struct Line {
    cli: SessionSource,
    route: Route,
    body: serde_json::Value,
}

fn corpus() -> Vec<Line> {
    include_str!("fixtures/hooks.jsonl")
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(|l| {
            let v: serde_json::Value = serde_json::from_str(l).unwrap_or_else(|e| panic!("unreadable line: {e}\n{l}"));
            let cli = match v["cli"].as_str().unwrap() {
                "claude_code" => SessionSource::ClaudeCode,
                "qwen_code" => SessionSource::QwenCode,
                "codex" => SessionSource::Codex,
                "opencode" => SessionSource::OpenCode,
                other => panic!("unknown cli {other}"),
            };
            let route = match v["route"].as_str().unwrap() {
                "hook" => Route::Hook,
                "status" => Route::Status,
                other => panic!("unknown route {other}"),
            };
            Line { cli, route, body: v["body"].clone() }
        })
        .collect()
}

/// The headline: nothing in the corpus is unknown or unreadable.
#[test]
fn every_known_hook_shape_is_classified() {
    let mut unknown = Vec::new();
    let mut malformed = Vec::new();
    for l in corpus() {
        match classify(l.cli, l.route, &l.body) {
            Received::Unknown { name } => unknown.push(name),
            Received::Malformed => malformed.push(l.body.to_string()),
            _ => {}
        }
    }
    assert!(unknown.is_empty(), "hook events nobody has classified: {unknown:?}");
    assert!(malformed.is_empty(), "unreadable hook bodies: {malformed:?}");
}

/// Every name the receiver claims to handle has a shape in the corpus, so
/// the detail it reads from each is pinned against the keys.
#[test]
fn every_handled_name_has_a_shape_in_the_corpus() {
    let lines = corpus();
    let seen = |cli: SessionSource, name: &str| {
        lines.iter().any(|l| {
            l.cli == cli
                && matches!(classify(l.cli, l.route, &l.body), Received::Event { name: n, .. } if n == name)
        })
    };
    let missing: Vec<&str> = CLAUDE_HANDLED.iter().copied().filter(|n| !seen(SessionSource::ClaudeCode, n)).collect();
    assert!(missing.is_empty(), "Claude Code hook events handled but not in the corpus: {missing:?}");
    let missing: Vec<&str> = OPENCODE_HANDLED.iter().copied().filter(|n| !seen(SessionSource::OpenCode, n)).collect();
    assert!(missing.is_empty(), "opencode events handled but not in the corpus: {missing:?}");
    assert!(lines.iter().any(|l| matches!(classify(l.cli, l.route, &l.body), Received::Status { .. })));
    assert!(lines.iter().any(|l| l.cli == SessionSource::QwenCode), "a Qwen seat's hook shape");
}

/// The detail on the row comes from named keys; these are the ones.
#[test]
fn the_details_are_read_from_the_keys_the_clis_use() {
    let details: Vec<(String, Option<String>)> = corpus()
        .into_iter()
        .filter_map(|l| match classify(l.cli, l.route, &l.body) {
            Received::Event { name, detail, .. } => Some((name, detail)),
            _ => None,
        })
        .collect();
    let has = |name: &str, detail: &str| {
        details.iter().any(|(n, d)| n == name && d.as_deref() == Some(detail))
    };
    assert!(has("Notification", "permission_prompt: Claude needs your permission to use Bash"));
    assert!(has("Notification", "idle_prompt: Claude is waiting for your input"));
    assert!(has("SessionStart", "startup") && has("SessionStart", "resume"));
    assert!(has("SessionEnd", "exit"));
    assert!(has("PreCompact", "auto"));
    assert!(has("SubagentStop", "Explore"));
    assert!(has("UserPromptSubmit", "make the backoff jittered"));
    assert!(has("permission.asked", "Run `cargo test`"));
    assert!(has("question.asked", "Which database?"));
    assert!(has("session.status", "busy") && has("session.status", "retry"));
    assert!(has("session.error", "invalid key"));
    assert!(has("permission.replied", "once"));
    // A status reading carries the fill and the model, and nothing of cost.
    let status = corpus().into_iter().find(|l| l.route == Route::Status).unwrap();
    let Received::Status { context_used, model, session_id } = classify(status.cli, status.route, &status.body) else {
        panic!("not a status reading");
    };
    assert_eq!(context_used, Some(6));
    assert_eq!(model.as_deref(), Some("Haiku 4.5"));
    assert!(session_id.is_some());
}
