//! The format sweep says what it found, and its exit code means something.
//! `R-J28`.
//!
//! The tool exists so drift is findable on a schedule rather than by luck, and
//! the property that makes it usable that way is the **exit code**: a check you
//! have to read is a check nobody runs twice. So that is what is pinned here,
//! in both directions, against corpora built for the purpose.
//!
//! Deliberately not pinned: the classification itself. The sweep imports
//! `adapter::HANDLED` and friends rather than keeping its own copy, so a test
//! asserting that a known type is reported as known would only be asserting
//! that `contains` works. What *would* be worth catching — a second list
//! drifting from the parser's — is prevented by construction instead.

use std::path::Path;
use std::process::Command;

fn scratch(tag: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("mogeung-sweep-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("claude/projects/-p")).unwrap();
    std::fs::create_dir_all(dir.join("codex/sessions/2026")).unwrap();
    dir
}

fn write(path: &Path, lines: &[&str]) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, format!("{}\n", lines.join("\n"))).unwrap();
}

/// The binary, pointed at a corpus built for the test. Both homes are injected
/// through the env vars the daemon already honours — the same door
/// [ADR-0006](../../../docs/decisions/0006-inject-the-watch-root.md) opened, so
/// the sweep needs no test-only switch of its own.
fn run(home: &Path) -> (bool, String) {
    let out = Command::new(env!("CARGO_BIN_EXE_sweep"))
        .env("CLAUDE_CONFIG_DIR", home.join("claude"))
        .env("CODEX_HOME", home.join("codex"))
        .output()
        .expect("sweep runs");
    (out.status.success(), String::from_utf8_lossy(&out.stdout).into_owned())
}

const KNOWN_CLAUDE: &str = r#"{"type":"user","timestamp":"2026-08-09T10:00:00.000Z","cwd":"/p","message":{"role":"user","content":"hi"}}"#;

#[test]
fn a_corpus_the_parser_understands_passes_quietly() {
    let home = scratch("clean");
    write(&home.join("claude/projects/-p/s1.jsonl"), &[KNOWN_CLAUDE]);
    write(
        &home.join("codex/sessions/2026/r.jsonl"),
        &[r#"{"type":"event_msg","payload":{"type":"turn_started"}}"#],
    );

    let (ok, out) = run(&home);
    assert!(ok, "a classified corpus must exit zero:\n{out}");
    assert!(out.contains("Everything on this machine is classified"), "{out}");
}

/// **The point of the tool.** A type nobody has taught the parser about has to
/// be named, counted, shown, and to fail the run — `R-J12`'s fourteenth event
/// type sat in 47 of 60 transcripts and was found by hand.
#[test]
fn an_unclassified_type_is_named_and_fails_the_run() {
    let home = scratch("drift");
    write(
        &home.join("claude/projects/-p/s1.jsonl"),
        &[KNOWN_CLAUDE, r#"{"type":"holographic-projection","x":1}"#],
    );

    let (ok, out) = run(&home);
    assert!(!ok, "unclassified shapes must fail the run:\n{out}");
    assert!(out.contains("holographic-projection"), "it must name the type:\n{out}");
    assert!(out.contains("UNCLASSIFIED"), "{out}");
    // The example line, so the next decision can be made from evidence rather
    // than from the name alone.
    assert!(out.contains("\"x\":1"), "it must show an example:\n{out}");
}

/// Codex nests its taxonomy under `payload.type`, and drift there is as
/// consequential as drift in the outer type — an inner kind the parser does not
/// know means whole lines are dropped. It is reported `outer/inner`, the same
/// compound the parser's own health report uses.
#[test]
fn codex_drift_below_the_top_level_is_reported_compound() {
    let home = scratch("nested");
    write(&home.join("claude/projects/-p/s1.jsonl"), &[KNOWN_CLAUDE]);
    write(
        &home.join("codex/sessions/2026/r.jsonl"),
        &[r#"{"type":"event_msg","payload":{"type":"telepathy_started","turn_id":"t"}}"#],
    );

    let (ok, out) = run(&home);
    assert!(!ok);
    assert!(
        out.contains("event_msg/telepathy_started"),
        "the inner kind must be named with its outer one:\n{out}"
    );
}

/// The inventory is what makes a *shape* change visible — a new usage key is
/// how prompt caching arrived, and no list exists to check one against.
#[test]
fn shapes_below_the_type_are_inventoried() {
    let home = scratch("shapes");
    write(
        &home.join("claude/projects/-p/s1.jsonl"),
        &[r#"{"type":"assistant","timestamp":"2026-08-09T10:00:00.000Z","cwd":"/p","message":{"model":"claude-opus-5","content":[{"type":"text","text":"hi"}],"usage":{"input_tokens":1,"telepathy_tokens":2}}}"#],
    );

    let (_, out) = run(&home);
    assert!(out.contains("claude-opus-5"), "models:\n{out}");
    assert!(out.contains("telepathy_tokens"), "a new usage key must show up:\n{out}");
    assert!(out.contains("content blocks"), "{out}");
}
