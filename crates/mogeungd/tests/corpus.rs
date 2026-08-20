//! Golden-corpus test — roadmap `R-A3`.
//!
//! `fixtures/corpus.jsonl` holds one line per distinct transcript *shape*
//! observed in real use: every event type, and for `user`/`assistant` every
//! content-block variant, including the awkward ones (a `last-prompt` carrying
//! no prompt, a `tool_result` with no `is_error`, a detached-HEAD branch, a
//! multi-line bash command).
//!
//! **It contains no real data.** The shapes were derived from the author's
//! corpus by reading key names only; every value here is synthetic. That is a
//! deliberate constraint — a fixture built by copying real transcripts would
//! put prompts, paths and source code into the repository forever.
//!
//! What this pins down is the thing a unit test cannot: that the parser's view
//! of the world still matches the world. If Claude Code adds an event type, the
//! corpus goes stale silently — but if it *renames* or *reshapes* one we
//! already rely on, this fails loudly.

use mogeung_core::health::LineClass;
use mogeungd::adapter::{parse_line, LineOutcome, HANDLED, KNOWN_IGNORED};

fn corpus() -> Vec<String> {
    let raw = include_str!("fixtures/corpus.jsonl");
    raw.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(str::to_string)
        .collect()
}

/// The headline guarantee: nothing in the corpus is unclassified.
///
/// This is the test that would have failed before `R-A1`: `queue-operation`,
/// `pr-link` and `frame-link` all occur in real transcripts and were being
/// swallowed by a catch-all arm.
#[test]
fn every_known_shape_is_classified() {
    let mut unknown = Vec::new();
    let mut malformed = Vec::new();

    for line in corpus() {
        match parse_line(&line) {
            LineOutcome::Unknown { event_type } => unknown.push(event_type),
            LineOutcome::Malformed => malformed.push(line.chars().take(60).collect::<String>()),
            _ => {}
        }
    }

    assert!(
        unknown.is_empty(),
        "transcript types nobody has classified: {unknown:?}\n\
         Add them to KNOWN_IGNORED (with a comment saying why they carry \
         nothing we need) or handle them."
    );
    assert!(malformed.is_empty(), "unreadable corpus lines: {malformed:?}");
}

/// Every type we claim to handle or ignore must actually appear in the corpus.
///
/// Without this the corpus rots in the other direction: a type gets dropped
/// from Claude Code, our list keeps asserting it, and the fixture stops
/// representing anything real.
#[test]
fn the_corpus_covers_every_type_we_claim_to_know() {
    let lines = corpus();
    let types: Vec<String> = lines
        .iter()
        .filter_map(|l| serde_json::from_str::<serde_json::Value>(l).ok())
        .filter_map(|v| v.get("type")?.as_str().map(str::to_string))
        .collect();

    for ty in HANDLED.iter().chain(KNOWN_IGNORED.iter()) {
        assert!(
            types.iter().any(|t| t == ty),
            "'{ty}' is in HANDLED/KNOWN_IGNORED but has no example in the corpus"
        );
    }
}

/// The corpus must exercise the paths that produce data, not just the ones we
/// skip — otherwise "everything classified" could be satisfied by ignoring all
/// of it.
#[test]
fn the_corpus_actually_produces_data() {
    let mut parsed = 0;
    let mut turns = 0;
    let mut tools = 0;
    let mut touched = 0;
    let mut errors = 0;
    let mut titles = 0;
    let mut tokens_out = 0u64;

    for line in corpus() {
        if let LineOutcome::Parsed(p) = parse_line(&line) {
            parsed += 1;
            turns += p.is_turn as u32;
            tools += p.tool_calls;
            touched += p.touched.len();
            errors += p.error.is_some() as u32;
            titles += p.title.is_some() as u32;
            tokens_out += p.tokens_out;
        }
    }

    assert!(parsed >= 10, "only {parsed} corpus lines yielded anything");
    assert!(turns >= 2, "no human turns in the corpus");
    assert!(tools >= 3, "no tool calls in the corpus");
    assert!(touched >= 1, "no touched files in the corpus");
    assert_eq!(errors, 1, "the API-error shape is not represented");
    // Two writers since 2026-08-20: `ai-title` and `agent-name`. Exact rather
    // than `>= 1`, so losing either example from the fixture is a failure
    // rather than a quiet halving of what this file covers.
    assert_eq!(titles, 2, "both title shapes — ai-title and agent-name — must be represented");
    assert!(tokens_out > 0, "usage accounting is not represented");
}

/// Awkward real shapes that have bitten us, pinned individually so a failure
/// names the specific thing that broke.
#[test]
fn awkward_shapes_behave() {
    let lines = corpus();
    let find = |needle: &str| {
        lines
            .iter()
            .find(|l| l.contains(needle))
            .unwrap_or_else(|| panic!("corpus lost its {needle:?} example"))
            .clone()
    };

    // A last-prompt with no lastPrompt: recognised, yields nothing.
    let barren = find(r#""leafUuid":"aaaaaaaa-0000-0000-0000-000000000002""#);
    assert_eq!(parse_line(&barren).class(), LineClass::Barren);

    // Detached HEAD is not a branch name.
    let detached = find(r#""gitBranch":"HEAD""#);
    let p = parse_line(&detached).parsed().expect("sidechain tool_use");
    assert!(p.git_branch.is_none());
    // ...and subagent chatter is not the session's headline activity.
    assert!(p.sidechain);
    assert!(p.last_activity.is_none());

    // A multi-line bash command must not leak newlines into the one-line
    // activity string.
    let multiline = find("with a second line");
    let p = parse_line(&multiline).parsed().expect("bash tool_use");
    let activity = p.last_activity.expect("bash sets last_activity");
    assert!(!activity.contains('\n'), "newline leaked: {activity:?}");

    // A tool_result without `is_error` must not be assumed to be an error.
    let no_flag = find(r#""tool_use_id":"toolu_0002""#);
    assert_eq!(parse_line(&no_flag).class(), LineClass::Parsed);
}
