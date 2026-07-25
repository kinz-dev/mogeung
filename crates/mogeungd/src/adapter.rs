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
use mogeung_core::transcript::EventKind;
use serde_json::Value;

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

/// Parse one line of a `.jsonl` transcript.
pub fn parse_line(line: &str) -> Option<Parsed> {
    let line = line.trim();
    if line.is_empty() || !line.starts_with('{') {
        return None;
    }
    let v: Value = serde_json::from_str(line).ok()?;
    let ty = str_at(&v, "type")?;

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

        // Bookkeeping we do not surface: mode, permission-mode, attachment,
        // file-history-snapshot, system/turn_duration.
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

    #[test]
    fn ignores_bookkeeping_and_unknown_types() {
        assert!(parse_line("").is_none());
        assert!(parse_line("not json").is_none());
        assert!(parse_line(r#"{"type":"mode","mode":"normal"}"#).is_none());
        assert!(parse_line(r#"{"type":"permission-mode","permissionMode":"auto"}"#).is_none());
        assert!(parse_line(r#"{"type":"file-history-snapshot","snapshot":{}}"#).is_none());
        // A future event type must not blow up the watcher.
        assert!(parse_line(r#"{"type":"some_future_thing","x":1}"#).is_none());
    }

    #[test]
    fn multiline_commands_stay_on_one_line() {
        let l = r#"{"type":"assistant","message":{"content":[{"type":"tool_use","id":"t1","name":"Bash","input":{"command":"python3 - <<'PY'\nimport os\nprint(1)\nPY"}}]}}"#;
        let p = parse_line(l).unwrap();
        let activity = p.last_activity.unwrap();
        assert!(!activity.contains('\n'), "activity leaked a newline: {activity:?}");
        assert!(activity.starts_with("Bash: python3 - <<'PY' import os"));
    }

    #[test]
    fn reads_the_generated_title() {
        let p = parse_line(r#"{"type":"ai-title","aiTitle":"Fix the retry loop"}"#).unwrap();
        assert_eq!(p.title.as_deref(), Some("Fix the retry loop"));
    }

    #[test]
    fn a_string_user_message_is_a_turn() {
        let l = r#"{"type":"user","timestamp":"2026-07-17T14:24:21.804Z","cwd":"/w","gitBranch":"main","message":{"role":"user","content":"do the thing"}}"#;
        let p = parse_line(l).unwrap();
        assert!(p.is_turn);
        assert_eq!(p.cwd.as_deref(), Some("/w"));
        assert_eq!(p.git_branch.as_deref(), Some("main"));
        assert!(p.ts.is_some());
    }

    #[test]
    fn a_tool_result_carrier_is_not_a_turn() {
        let l = r#"{"type":"user","message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"t1","is_error":false,"content":"ok"}]}}"#;
        let p = parse_line(l).unwrap();
        assert!(!p.is_turn, "tool results must not be counted as human turns");
        assert!(matches!(p.events[0], EventKind::ToolResult { .. }));
    }

    #[test]
    fn detached_head_is_not_treated_as_a_branch() {
        let l = r#"{"type":"user","gitBranch":"HEAD","message":{"role":"user","content":"x"}}"#;
        assert!(parse_line(l).unwrap().git_branch.is_none());
    }

    #[test]
    fn write_and_edit_register_as_touched_files() {
        let l = r#"{"type":"assistant","message":{"content":[{"type":"tool_use","id":"t1","name":"Edit","input":{"file_path":"/w/src/a.rs"}}]}}"#;
        let p = parse_line(l).unwrap();
        assert_eq!(p.touched, vec!["/w/src/a.rs"]);
        assert_eq!(p.tool_calls, 1);
        assert_eq!(p.last_activity.as_deref(), Some("Edit: /w/src/a.rs"));
    }

    #[test]
    fn reads_but_does_not_count_as_a_file_change() {
        let l = r#"{"type":"assistant","message":{"content":[{"type":"tool_use","id":"t1","name":"Read","input":{"file_path":"/w/src/a.rs"}}]}}"#;
        assert!(parse_line(l).unwrap().touched.is_empty());
    }

    #[test]
    fn subagent_activity_does_not_become_the_session_headline() {
        let l = r#"{"type":"assistant","isSidechain":true,"message":{"content":[{"type":"tool_use","id":"t1","name":"Bash","input":{"command":"ls"}}]}}"#;
        let p = parse_line(l).unwrap();
        assert!(p.sidechain);
        assert!(p.last_activity.is_none());
    }

    #[test]
    fn api_errors_surface_as_errors() {
        let l = r#"{"type":"assistant","isApiErrorMessage":true,"message":{"content":[{"type":"text","text":"overloaded_error"}]}}"#;
        let p = parse_line(l).unwrap();
        assert!(p.error.unwrap().contains("overloaded"));
    }

    #[test]
    fn usage_totals_are_read() {
        let l = r#"{"type":"assistant","message":{"content":[],"usage":{"input_tokens":10,"output_tokens":20,"cache_read_input_tokens":5}}}"#;
        let p = parse_line(l).unwrap();
        assert_eq!(p.tokens_in, 15);
        assert_eq!(p.tokens_out, 20);
    }
}
