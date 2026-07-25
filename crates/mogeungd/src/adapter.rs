//! Reader for Claude Code's on-disk session transcripts.
//!
//! Claude Code writes every session to `~/.claude/projects/<slug>/<id>.jsonl`
//! for its own resume/history purposes. mogeung tails those files. Nothing here
//! starts or controls an agent — that is the whole point of the observer model.
//!
//! The parser is deliberately forgiving: unknown event types and unexpected
//! shapes are ignored, never fatal. This is an undocumented internal format on
//! a tool that ships constantly, so a schema change must degrade the transcript
//! rather than break the watcher.

use chrono::{DateTime, Utc};
use mogeung_core::health::LineClass;
use mogeung_core::transcript::EventKind;
use serde_json::Value;

/// Transcript event types we have seen, understand the purpose of, and
/// deliberately do not surface.
///
/// This list is the whole point of roadmap `R-A1`. Before it existed, an
/// unrecognised type and a type we had chosen to skip were both `None`, so a
/// format change was indistinguishable from normal operation. Adding an entry
/// here is now a deliberate act: it says *we looked at this and decided it
/// carries nothing we need*, and it silences an alert that would otherwise
/// fire.
///
/// Verified against the 52-transcript corpus on the author's machine
/// (2026-07-25, Claude Code 2.1.219/2.1.220). `queue-operation`, `pr-link` and
/// `frame-link` were found only because the canary flagged them.
pub const KNOWN_IGNORED: &[&str] = &[
    // Session settings chatter.
    "mode",
    "permission-mode",
    // Pre-edit backup bookkeeping. See ADR-0004 for why we do not read it.
    "file-history-snapshot",
    // Pasted/attached content, already reflected in the message that used it.
    "attachment",
    // Turn timing and meta notices.
    "system",
    // Queued follow-up prompts, before they become real turns.
    "queue-operation",
    // Links the CLI records after opening a PR or a frame.
    "pr-link",
    "frame-link",
];

/// Types this parser extracts data from.
///
/// Kept explicit so that a line of a handled type which yields nothing can be
/// reported as [`LineClass::Barren`] rather than silently dropped — a spike
/// there means a shape we depend on has moved.
pub const HANDLED: &[&str] = &[
    "assistant",
    "user",
    "ai-title",
    "last-prompt",
    "file-history-delta",
];

/// The outcome of reading one transcript line.
#[derive(Debug)]
pub enum LineOutcome {
    /// Understood, and it produced something.
    Parsed(Box<Parsed>),
    /// A known type we deliberately skip.
    Ignored,
    /// A handled type that yielded nothing this time.
    Barren { event_type: String },
    /// A type we have never heard of. The canary.
    Unknown { event_type: String },
    /// Not JSON, or no `type` field.
    Malformed,
}

impl LineOutcome {
    pub fn class(&self) -> LineClass {
        match self {
            LineOutcome::Parsed(_) => LineClass::Parsed,
            LineOutcome::Ignored => LineClass::Ignored,
            LineOutcome::Barren { .. } => LineClass::Barren,
            LineOutcome::Unknown { .. } => LineClass::Unknown,
            LineOutcome::Malformed => LineClass::Malformed,
        }
    }

    pub fn parsed(self) -> Option<Parsed> {
        match self {
            LineOutcome::Parsed(p) => Some(*p),
            _ => None,
        }
    }
}

/// Everything one transcript line tells us.
#[derive(Debug, Default)]
pub struct Parsed {
    pub events: Vec<EventKind>,
    pub ts: Option<DateTime<Utc>>,
    pub cwd: Option<String>,
    pub git_branch: Option<String>,
    pub version: Option<String>,
    /// Claude Code's own generated title for the conversation.
    pub title: Option<String>,
    pub last_prompt: Option<String>,
    pub tokens_in: u64,
    pub tokens_out: u64,
    /// This line represents a fresh prompt from the human.
    pub is_turn: bool,
    pub tool_calls: u32,
    pub last_activity: Option<String>,
    /// Files this line shows the session touching.
    pub touched: Vec<String>,
    pub error: Option<String>,
    /// Emitted by a subagent rather than the main conversation.
    pub sidechain: bool,
}

fn truncate(s: &str, n: usize) -> String {
    let s = s.trim();
    if s.chars().count() <= n {
        return s.to_string();
    }
    let cut: String = s.chars().take(n).collect();
    format!("{cut}…")
}

/// Collapse a value onto one line. Tool summaries and activity strings are
/// rendered in single-line contexts, and a heredoc in a bash command would
/// otherwise wreck the layout.
fn one_line(s: &str, n: usize) -> String {
    let flat: String = s
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    truncate(&flat, n)
}

fn str_at<'a>(v: &'a Value, k: &str) -> Option<&'a str> {
    v.get(k).and_then(|x| x.as_str())
}

/// Human-readable one-liner for a tool call.
fn tool_summary(name: &str, input: &Value) -> String {
    let get = |k: &str| input.get(k).and_then(|v| v.as_str()).unwrap_or("");
    let s = match name {
        "Bash" => {
            let cmd = get("command");
            if cmd.is_empty() {
                get("description").to_string()
            } else {
                cmd.to_string()
            }
        }
        "Read" | "Write" | "Edit" | "NotebookEdit" => get("file_path").to_string(),
        "Glob" | "Grep" => {
            let p = get("pattern");
            let path = get("path");
            if path.is_empty() {
                p.to_string()
            } else {
                format!("{p}  in {path}")
            }
        }
        "WebFetch" => get("url").to_string(),
        "WebSearch" => get("query").to_string(),
        "Task" | "Agent" => get("description").to_string(),
        "TaskCreate" | "TaskUpdate" => get("subject").to_string(),
        "Skill" => get("skill").to_string(),
        _ => input
            .as_object()
            .and_then(|o| {
                o.iter()
                    .find(|(_, v)| v.as_str().map(|s| s.len() < 200).unwrap_or(false))
                    .and_then(|(_, v)| v.as_str())
            })
            .unwrap_or("")
            .to_string(),
    };
    one_line(&s, 160)
}

/// Tools whose use means a file on disk changed.
fn touched_path(name: &str, input: &Value) -> Option<String> {
    match name {
        "Write" | "Edit" | "NotebookEdit" => {
            str_at(input, "file_path").map(str::to_string)
        }
        _ => None,
    }
}

fn text_of(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Array(items) => items
            .iter()
            .filter_map(|b| b.get("text").and_then(|t| t.as_str()))
            .collect::<Vec<_>>()
            .join("\n"),
        other => other.to_string(),
    }
}

/// Classify and read one line of a `.jsonl` transcript.
///
/// Never panics and never fails: an unreadable line becomes an outcome, because
/// the formats are undocumented and a schema change must degrade the board
/// rather than stop the watcher.
pub fn parse_line(line: &str) -> LineOutcome {
    let line = line.trim();
    if line.is_empty() {
        // The tailer already drops blank lines; this is belt and braces, and a
        // blank line is not evidence of a format change.
        return LineOutcome::Ignored;
    }
    if !line.starts_with('{') {
        return LineOutcome::Malformed;
    }
    let Ok(v) = serde_json::from_str::<Value>(line) else {
        return LineOutcome::Malformed;
    };
    let Some(ty) = str_at(&v, "type") else {
        return LineOutcome::Malformed;
    };
    let ty = ty.to_string();

    if KNOWN_IGNORED.contains(&ty.as_str()) {
        return LineOutcome::Ignored;
    }
    if !HANDLED.contains(&ty.as_str()) {
        // The canary. We have never seen this type, so we cannot know what we
        // are missing — which is exactly why it must be said out loud.
        return LineOutcome::Unknown { event_type: ty };
    }

    match extract(&v, &ty) {
        Some(p) => LineOutcome::Parsed(Box::new(p)),
        None => LineOutcome::Barren { event_type: ty },
    }
}

/// Pull what we can out of a line whose type we handle.
///
/// `None` means the type is one we read but this line gave us nothing — a
/// normal occurrence in small numbers (a `last-prompt` with no prompt), and a
/// signal that a shape has moved if it becomes common.
fn extract(v: &Value, ty: &str) -> Option<Parsed> {
    let mut out = Parsed {
        ts: str_at(&v, "timestamp")
            .and_then(|t| DateTime::parse_from_rfc3339(t).ok())
            .map(|t| t.with_timezone(&Utc)),
        cwd: str_at(&v, "cwd").map(str::to_string),
        git_branch: str_at(&v, "gitBranch")
            .filter(|b| !b.is_empty() && *b != "HEAD")
            .map(str::to_string),
        version: str_at(&v, "version").map(str::to_string),
        sidechain: v
            .get("isSidechain")
            .and_then(|s| s.as_bool())
            .unwrap_or(false),
        ..Default::default()
    };

    match ty {
        // Claude Code's generated conversation title — the best label we get.
        "ai-title" => {
            out.title = str_at(&v, "aiTitle").map(str::to_string);
        }

        "last-prompt" => {
            out.last_prompt = str_at(&v, "lastPrompt").map(|s| truncate(s, 400));
        }

        // Records a file the session is tracking edits to.
        "file-history-delta" => {
            if let Some(p) = str_at(&v, "trackingPath") {
                out.touched.push(p.to_string());
            }
        }

        "user" => {
            let content = v.get("message").and_then(|m| m.get("content"))?;
            match content {
                // A plain string is always a human prompt.
                Value::String(s) => {
                    if !s.trim().is_empty() {
                        out.is_turn = true;
                        out.last_prompt = Some(truncate(s, 400));
                        out.events.push(EventKind::UserPrompt {
                            text: truncate(s, 4000),
                        });
                    }
                }
                Value::Array(blocks) => {
                    let mut prompt_parts = Vec::new();
                    for b in blocks {
                        match b.get("type").and_then(|t| t.as_str()) {
                            Some("tool_result") => {
                                out.events.push(EventKind::ToolResult {
                                    tool_use_id: str_at(b, "tool_use_id")
                                        .unwrap_or("")
                                        .to_string(),
                                    is_error: b
                                        .get("is_error")
                                        .and_then(|e| e.as_bool())
                                        .unwrap_or(false),
                                    preview: truncate(
                                        &text_of(b.get("content").unwrap_or(&Value::Null)),
                                        400,
                                    ),
                                });
                            }
                            Some("text") => {
                                if let Some(t) = str_at(b, "text") {
                                    prompt_parts.push(t.to_string());
                                }
                            }
                            _ => {}
                        }
                    }
                    let joined = prompt_parts.join("\n");
                    if !joined.trim().is_empty() {
                        out.is_turn = true;
                        out.last_prompt = Some(truncate(&joined, 400));
                        out.events.push(EventKind::UserPrompt {
                            text: truncate(&joined, 4000),
                        });
                    }
                }
                _ => {}
            }
        }

        "assistant" => {
            let msg = v.get("message")?;
            if let Some(usage) = msg.get("usage") {
                let n = |k: &str| usage.get(k).and_then(|x| x.as_u64()).unwrap_or(0);
                out.tokens_in = n("input_tokens")
                    + n("cache_read_input_tokens")
                    + n("cache_creation_input_tokens");
                out.tokens_out = n("output_tokens");
            }
            // API errors are marked on the message itself.
            if v.get("isApiErrorMessage")
                .and_then(|e| e.as_bool())
                .unwrap_or(false)
            {
                let detail = text_of(msg.get("content").unwrap_or(&Value::Null));
                out.error = Some(truncate(&detail, 300));
            }
            if let Some(e) = v.get("error").filter(|e| !e.is_null()) {
                out.error = Some(truncate(&text_of(e), 300));
            }

            for block in msg.get("content").and_then(|c| c.as_array())? {
                match block.get("type").and_then(|t| t.as_str()) {
                    Some("text") => {
                        if let Some(t) = str_at(block, "text") {
                            if !t.trim().is_empty() {
                                out.events.push(EventKind::AssistantText {
                                    text: truncate(t, 8000),
                                });
                            }
                        }
                    }
                    Some("thinking") => {
                        if let Some(t) = str_at(block, "thinking") {
                            if !t.trim().is_empty() {
                                out.events.push(EventKind::Thinking {
                                    text: truncate(t, 4000),
                                });
                            }
                        }
                    }
                    Some("tool_use") => {
                        let name = str_at(block, "name").unwrap_or("tool").to_string();
                        let empty = Value::Object(Default::default());
                        let input = block.get("input").unwrap_or(&empty);
                        let summary = tool_summary(&name, input);
                        if let Some(p) = touched_path(&name, input) {
                            out.touched.push(p);
                        }
                        out.tool_calls += 1;
                        // Subagent chatter should not masquerade as the main
                        // session's current activity.
                        if !out.sidechain {
                            out.last_activity = Some(if summary.is_empty() {
                                name.clone()
                            } else {
                                format!("{name}: {summary}")
                            });
                        }
                        out.events.push(EventKind::ToolUse {
                            tool_use_id: str_at(block, "id").unwrap_or("").to_string(),
                            name,
                            summary,
                        });
                    }
                    _ => {}
                }
            }
        }

        // Unreachable: `parse_line` has already sorted every type into
        // handled, known-ignored or unknown. Kept so that adding a name to
        // `HANDLED` without writing an arm degrades to "barren" rather than
        // failing to compile into a silent lie.
        _ => return None,
    }

    if out.events.is_empty()
        && out.title.is_none()
        && out.last_prompt.is_none()
        && out.touched.is_empty()
        && out.error.is_none()
        && out.tokens_out == 0
    {
        return None;
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Unwrap a line we expect to yield data.
    fn parsed(line: &str) -> Parsed {
        match parse_line(line) {
            LineOutcome::Parsed(p) => *p,
            other => panic!("expected a parsed line, got {other:?}"),
        }
    }

    fn class(line: &str) -> LineClass {
        parse_line(line).class()
    }

    #[test]
    fn known_bookkeeping_is_ignored_quietly() {
        assert_eq!(class(""), LineClass::Ignored);
        for ty in KNOWN_IGNORED {
            let line = format!(r#"{{"type":"{ty}"}}"#);
            assert_eq!(
                class(&line),
                LineClass::Ignored,
                "{ty} is in KNOWN_IGNORED but did not classify as ignored"
            );
        }
    }

    /// The canary. This is the distinction the whole feature exists to draw:
    /// an unclassified type must never look like ordinary skipped bookkeeping.
    #[test]
    fn an_unheard_of_type_is_not_confused_with_bookkeeping() {
        match parse_line(r#"{"type":"warp-drive","x":1}"#) {
            LineOutcome::Unknown { event_type } => assert_eq!(event_type, "warp-drive"),
            other => panic!("a future event type must be flagged, got {other:?}"),
        }
    }

    /// Regression: these three exist in the real corpus and were being
    /// swallowed by a catch-all. They were found *by* the canary.
    #[test]
    fn types_found_in_the_real_corpus_are_all_classified() {
        for ty in ["queue-operation", "pr-link", "frame-link"] {
            let line = format!(r#"{{"type":"{ty}","sessionId":"s"}}"#);
            assert_eq!(
                class(&line),
                LineClass::Ignored,
                "{ty} occurs in real transcripts and must be classified, not unknown"
            );
        }
    }

    #[test]
    fn unreadable_lines_are_malformed_not_silently_dropped() {
        assert_eq!(class("not json"), LineClass::Malformed);
        assert_eq!(class("{ truncated"), LineClass::Malformed);
        // Valid JSON, but nothing tells us what it is.
        assert_eq!(class(r#"{"sessionId":"s"}"#), LineClass::Malformed);
    }

    /// A handled type that yields nothing is its own category: normal in small
    /// numbers, and evidence a shape moved if it becomes common.
    #[test]
    fn a_handled_type_yielding_nothing_is_barren() {
        // Seen 42× in the real corpus: a last-prompt with no lastPrompt.
        match parse_line(r#"{"type":"last-prompt","leafUuid":"u","sessionId":"s"}"#) {
            LineOutcome::Barren { event_type } => assert_eq!(event_type, "last-prompt"),
            other => panic!("expected barren, got {other:?}"),
        }
        assert_eq!(class(r#"{"type":"assistant"}"#), LineClass::Barren);
    }

    #[test]
    fn multiline_commands_stay_on_one_line() {
        let l = r#"{"type":"assistant","message":{"content":[{"type":"tool_use","id":"t1","name":"Bash","input":{"command":"python3 - <<'PY'\nimport os\nprint(1)\nPY"}}]}}"#;
        let p = parsed(l);
        let activity = p.last_activity.unwrap();
        assert!(!activity.contains('\n'), "activity leaked a newline: {activity:?}");
        assert!(activity.starts_with("Bash: python3 - <<'PY' import os"));
    }

    #[test]
    fn reads_the_generated_title() {
        let p = parsed(r#"{"type":"ai-title","aiTitle":"Fix the retry loop"}"#);
        assert_eq!(p.title.as_deref(), Some("Fix the retry loop"));
    }

    #[test]
    fn a_string_user_message_is_a_turn() {
        let l = r#"{"type":"user","timestamp":"2026-07-17T14:24:21.804Z","cwd":"/w","gitBranch":"main","message":{"role":"user","content":"do the thing"}}"#;
        let p = parsed(l);
        assert!(p.is_turn);
        assert_eq!(p.cwd.as_deref(), Some("/w"));
        assert_eq!(p.git_branch.as_deref(), Some("main"));
        assert!(p.ts.is_some());
    }

    #[test]
    fn a_tool_result_carrier_is_not_a_turn() {
        let l = r#"{"type":"user","message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"t1","is_error":false,"content":"ok"}]}}"#;
        let p = parsed(l);
        assert!(!p.is_turn, "tool results must not be counted as human turns");
        assert!(matches!(p.events[0], EventKind::ToolResult { .. }));
    }

    #[test]
    fn detached_head_is_not_treated_as_a_branch() {
        let l = r#"{"type":"user","gitBranch":"HEAD","message":{"role":"user","content":"x"}}"#;
        assert!(parsed(l).git_branch.is_none());
    }

    #[test]
    fn write_and_edit_register_as_touched_files() {
        let l = r#"{"type":"assistant","message":{"content":[{"type":"tool_use","id":"t1","name":"Edit","input":{"file_path":"/w/src/a.rs"}}]}}"#;
        let p = parsed(l);
        assert_eq!(p.touched, vec!["/w/src/a.rs"]);
        assert_eq!(p.tool_calls, 1);
        assert_eq!(p.last_activity.as_deref(), Some("Edit: /w/src/a.rs"));
    }

    #[test]
    fn reads_but_does_not_count_as_a_file_change() {
        let l = r#"{"type":"assistant","message":{"content":[{"type":"tool_use","id":"t1","name":"Read","input":{"file_path":"/w/src/a.rs"}}]}}"#;
        assert!(parsed(l).touched.is_empty());
    }

    #[test]
    fn subagent_activity_does_not_become_the_session_headline() {
        let l = r#"{"type":"assistant","isSidechain":true,"message":{"content":[{"type":"tool_use","id":"t1","name":"Bash","input":{"command":"ls"}}]}}"#;
        let p = parsed(l);
        assert!(p.sidechain);
        assert!(p.last_activity.is_none());
    }

    #[test]
    fn api_errors_surface_as_errors() {
        let l = r#"{"type":"assistant","isApiErrorMessage":true,"message":{"content":[{"type":"text","text":"overloaded_error"}]}}"#;
        let p = parsed(l);
        assert!(p.error.unwrap().contains("overloaded"));
    }

    #[test]
    fn usage_totals_are_read() {
        let l = r#"{"type":"assistant","message":{"content":[],"usage":{"input_tokens":10,"output_tokens":20,"cache_read_input_tokens":5}}}"#;
        let p = parsed(l);
        assert_eq!(p.tokens_in, 15);
        assert_eq!(p.tokens_out, 20);
    }
}
