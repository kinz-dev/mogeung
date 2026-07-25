//! Claude Code adapter.
//!
//! Wraps `claude -p --output-format stream-json` and normalises its event
//! stream into mogeung's `TranscriptEvent`. Everything CLI-specific lives in
//! this file, so adding Codex or Gemini later means adding a sibling module
//! rather than touching the daemon (CONCEPT.md A3).
//!
//! The parser is deliberately forgiving: unknown event types are ignored, not
//! fatal. Agent CLIs are moving targets and a schema change should degrade the
//! transcript, never kill the run.

use mogeung_core::run::PermissionMode;
use mogeung_core::transcript::{EventKind, NoticeLevel};
use serde_json::Value;

pub struct SpawnSpec {
    pub cwd: String,
    pub prompt: String,
    pub model: Option<String>,
    pub permission_mode: PermissionMode,
    /// `--session-id` for a root run.
    pub session_id: Option<String>,
    /// `--resume` for a follow-up. Mutually exclusive with `session_id`.
    pub resume: Option<String>,
}

pub fn build_args(spec: &SpawnSpec) -> Vec<String> {
    let mut a: Vec<String> = vec![
        "-p".into(),
        spec.prompt.clone(),
        "--output-format".into(),
        "stream-json".into(),
        // stream-json output is rejected without --verbose.
        "--verbose".into(),
        "--permission-mode".into(),
        spec.permission_mode.as_cli().into(),
    ];
    if let Some(m) = &spec.model {
        if !m.trim().is_empty() {
            a.push("--model".into());
            a.push(m.clone());
        }
    }
    if let Some(r) = &spec.resume {
        a.push("--resume".into());
        a.push(r.clone());
    } else if let Some(s) = &spec.session_id {
        a.push("--session-id".into());
        a.push(s.clone());
    }
    a
}

/// Everything one stdout line tells us.
#[derive(Debug, Default)]
pub struct Ingest {
    pub events: Vec<EventKind>,
    pub session_id: Option<String>,
    pub cost_usd: Option<f64>,
    pub num_turns: Option<u32>,
    pub tokens_in: u64,
    pub tokens_out: u64,
    pub denials: Option<u32>,
    pub last_activity: Option<String>,
    pub finish: Option<Finish>,
}

#[derive(Debug, Clone)]
pub struct Finish {
    pub is_error: bool,
    pub terminal_reason: String,
    pub text: String,
}

fn truncate(s: &str, n: usize) -> String {
    let s = s.trim();
    if s.chars().count() <= n {
        return s.to_string();
    }
    let cut: String = s.chars().take(n).collect();
    format!("{cut}…")
}

/// Human-readable one-liner for a tool call, so the transcript and the run
/// board can show "what is it doing" without expanding anything.
fn tool_summary(name: &str, input: &Value) -> String {
    let get = |k: &str| input.get(k).and_then(|v| v.as_str()).unwrap_or("");
    let s = match name {
        "Bash" => {
            let cmd = get("command");
            if cmd.is_empty() { get("description").to_string() } else { cmd.to_string() }
        }
        "Read" | "Write" | "Edit" | "NotebookEdit" => get("file_path").to_string(),
        "Glob" | "Grep" => {
            let p = get("pattern");
            let path = get("path");
            if path.is_empty() { p.to_string() } else { format!("{p}  in {path}") }
        }
        "WebFetch" => get("url").to_string(),
        "WebSearch" => get("query").to_string(),
        "Task" | "Agent" => get("description").to_string(),
        "TaskCreate" | "TaskUpdate" => get("subject").to_string(),
        "Skill" => get("skill").to_string(),
        _ => {
            // Unknown tool: show the first short string field rather than nothing.
            input
                .as_object()
                .and_then(|o| {
                    o.iter()
                        .find(|(_, v)| v.as_str().map(|s| s.len() < 200).unwrap_or(false))
                        .and_then(|(_, v)| v.as_str())
                })
                .unwrap_or("")
                .to_string()
        }
    };
    truncate(&s, 160)
}

fn result_preview(v: &Value) -> String {
    let text = match v {
        Value::String(s) => s.clone(),
        Value::Array(items) => items
            .iter()
            .filter_map(|b| b.get("text").and_then(|t| t.as_str()))
            .collect::<Vec<_>>()
            .join("\n"),
        other => other.to_string(),
    };
    truncate(&text, 400)
}

/// Parse one line of `stream-json` stdout.
///
/// Returns `None` for blank lines, non-JSON noise, and event types we do not
/// model. That tolerance is intentional — see the module docs.
pub fn parse_line(line: &str) -> Option<Ingest> {
    let line = line.trim();
    if line.is_empty() || !line.starts_with('{') {
        return None;
    }
    let v: Value = serde_json::from_str(line).ok()?;
    let ty = v.get("type").and_then(|t| t.as_str())?;
    let mut out = Ingest::default();

    match ty {
        "system" => {
            let subtype = v.get("subtype").and_then(|s| s.as_str()).unwrap_or("");
            if subtype != "init" {
                return None; // thinking_tokens and friends are not worth a row
            }
            out.session_id = v
                .get("session_id")
                .and_then(|s| s.as_str())
                .map(str::to_string);
            out.events.push(EventKind::Init {
                model: v
                    .get("model")
                    .and_then(|s| s.as_str())
                    .unwrap_or("?")
                    .to_string(),
                cwd: v
                    .get("cwd")
                    .and_then(|s| s.as_str())
                    .unwrap_or("?")
                    .to_string(),
                tool_count: v
                    .get("tools")
                    .and_then(|t| t.as_array())
                    .map(|a| a.len())
                    .unwrap_or(0),
            });
        }

        "assistant" => {
            let msg = v.get("message")?;
            if let Some(usage) = msg.get("usage") {
                let n = |k: &str| usage.get(k).and_then(|x| x.as_u64()).unwrap_or(0);
                out.tokens_in += n("input_tokens") + n("cache_read_input_tokens") + n("cache_creation_input_tokens");
                out.tokens_out += n("output_tokens");
            }
            for block in msg.get("content").and_then(|c| c.as_array())?.iter() {
                match block.get("type").and_then(|t| t.as_str()) {
                    Some("text") => {
                        let text = block.get("text").and_then(|t| t.as_str()).unwrap_or("");
                        if !text.trim().is_empty() {
                            out.events.push(EventKind::AssistantText {
                                text: text.to_string(),
                            });
                        }
                    }
                    Some("thinking") => {
                        let text = block.get("thinking").and_then(|t| t.as_str()).unwrap_or("");
                        if !text.trim().is_empty() {
                            out.events.push(EventKind::Thinking {
                                text: text.to_string(),
                            });
                        }
                    }
                    Some("tool_use") => {
                        let name = block
                            .get("name")
                            .and_then(|n| n.as_str())
                            .unwrap_or("tool")
                            .to_string();
                        let empty = Value::Object(Default::default());
                        let summary = tool_summary(&name, block.get("input").unwrap_or(&empty));
                        out.last_activity = Some(if summary.is_empty() {
                            name.clone()
                        } else {
                            format!("{name}: {summary}")
                        });
                        out.events.push(EventKind::ToolUse {
                            tool_use_id: block
                                .get("id")
                                .and_then(|i| i.as_str())
                                .unwrap_or("")
                                .to_string(),
                            name,
                            summary,
                        });
                    }
                    _ => {}
                }
            }
        }

        "user" => {
            let content = v.get("message").and_then(|m| m.get("content"))?;
            if let Some(arr) = content.as_array() {
                for block in arr {
                    if block.get("type").and_then(|t| t.as_str()) == Some("tool_result") {
                        let empty = Value::Null;
                        out.events.push(EventKind::ToolResult {
                            tool_use_id: block
                                .get("tool_use_id")
                                .and_then(|i| i.as_str())
                                .unwrap_or("")
                                .to_string(),
                            is_error: block
                                .get("is_error")
                                .and_then(|e| e.as_bool())
                                .unwrap_or(false),
                            preview: result_preview(block.get("content").unwrap_or(&empty)),
                        });
                    }
                }
            }
        }

        "result" => {
            out.cost_usd = v.get("total_cost_usd").and_then(|c| c.as_f64());
            out.num_turns = v.get("num_turns").and_then(|c| c.as_u64()).map(|n| n as u32);
            out.denials = v
                .get("permission_denials")
                .and_then(|d| d.as_array())
                .map(|a| a.len() as u32);
            if let Some(usage) = v.get("usage") {
                let n = |k: &str| usage.get(k).and_then(|x| x.as_u64()).unwrap_or(0);
                out.tokens_in = n("input_tokens") + n("cache_read_input_tokens") + n("cache_creation_input_tokens");
                out.tokens_out = n("output_tokens");
            }
            let is_error = v.get("is_error").and_then(|e| e.as_bool()).unwrap_or(false);
            let terminal_reason = v
                .get("terminal_reason")
                .and_then(|t| t.as_str())
                .unwrap_or("unknown")
                .to_string();
            let text = v
                .get("result")
                .and_then(|t| t.as_str())
                .unwrap_or("")
                .to_string();
            out.events.push(EventKind::Result {
                cost_usd: out.cost_usd.unwrap_or(0.0),
                num_turns: out.num_turns.unwrap_or(0),
                terminal_reason: terminal_reason.clone(),
                is_error,
                text: text.clone(),
            });
            out.finish = Some(Finish {
                is_error,
                terminal_reason,
                text,
            });
        }

        _ => return None,
    }

    if out.events.is_empty()
        && out.session_id.is_none()
        && out.finish.is_none()
        && out.tokens_in == 0
        && out.tokens_out == 0
    {
        return None;
    }
    Some(out)
}

pub fn notice(level: NoticeLevel, message: impl Into<String>) -> EventKind {
    EventKind::Notice {
        level,
        message: message.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ignores_noise_and_unknown_types() {
        assert!(parse_line("").is_none());
        assert!(parse_line("not json").is_none());
        assert!(parse_line(r#"{"type":"rate_limit_event"}"#).is_none());
        assert!(parse_line(r#"{"type":"system","subtype":"thinking_tokens"}"#).is_none());
        // A future event type must not blow up the parser.
        assert!(parse_line(r#"{"type":"some_future_thing","x":1}"#).is_none());
    }

    #[test]
    fn reads_init() {
        let l = r#"{"type":"system","subtype":"init","cwd":"/w","session_id":"abc","model":"claude-sonnet-5","tools":["Bash","Read"]}"#;
        let i = parse_line(l).unwrap();
        assert_eq!(i.session_id.as_deref(), Some("abc"));
        match &i.events[0] {
            EventKind::Init { model, cwd, tool_count } => {
                assert_eq!(model, "claude-sonnet-5");
                assert_eq!(cwd, "/w");
                assert_eq!(*tool_count, 2);
            }
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn reads_tool_use_with_a_useful_summary() {
        let l = r#"{"type":"assistant","message":{"content":[{"type":"tool_use","id":"t1","name":"Bash","input":{"command":"echo hi","description":"say hi"}}],"usage":{"output_tokens":5}}}"#;
        let i = parse_line(l).unwrap();
        assert_eq!(i.tokens_out, 5);
        assert_eq!(i.last_activity.as_deref(), Some("Bash: echo hi"));
        match &i.events[0] {
            EventKind::ToolUse { name, summary, tool_use_id } => {
                assert_eq!(name, "Bash");
                assert_eq!(summary, "echo hi");
                assert_eq!(tool_use_id, "t1");
            }
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn reads_tool_result() {
        let l = r#"{"type":"user","message":{"content":[{"type":"tool_result","tool_use_id":"t1","is_error":true,"content":"boom"}]}}"#;
        let i = parse_line(l).unwrap();
        match &i.events[0] {
            EventKind::ToolResult { is_error, preview, .. } => {
                assert!(is_error);
                assert_eq!(preview, "boom");
            }
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn reads_result_totals_and_denials() {
        let l = r#"{"type":"result","total_cost_usd":0.25,"num_turns":3,"is_error":false,"terminal_reason":"completed","result":"done","permission_denials":[{"tool":"Bash"}],"usage":{"input_tokens":10,"output_tokens":20,"cache_read_input_tokens":5}}"#;
        let i = parse_line(l).unwrap();
        assert_eq!(i.cost_usd, Some(0.25));
        assert_eq!(i.num_turns, Some(3));
        assert_eq!(i.denials, Some(1));
        assert_eq!(i.tokens_in, 15);
        assert_eq!(i.tokens_out, 20);
        let f = i.finish.unwrap();
        assert!(!f.is_error);
        assert_eq!(f.terminal_reason, "completed");
    }

    #[test]
    fn root_and_followup_use_different_session_flags() {
        let root = build_args(&SpawnSpec {
            cwd: "/w".into(),
            prompt: "go".into(),
            model: Some("sonnet".into()),
            permission_mode: PermissionMode::AcceptEdits,
            session_id: Some("sid".into()),
            resume: None,
        });
        assert!(root.windows(2).any(|w| w[0] == "--session-id" && w[1] == "sid"));
        assert!(!root.iter().any(|a| a == "--resume"));

        let follow = build_args(&SpawnSpec {
            cwd: "/w".into(),
            prompt: "more".into(),
            model: None,
            permission_mode: PermissionMode::AcceptEdits,
            session_id: Some("ignored".into()),
            resume: Some("sid".into()),
        });
        assert!(follow.windows(2).any(|w| w[0] == "--resume" && w[1] == "sid"));
        assert!(!follow.iter().any(|a| a == "--session-id"));
        assert!(!follow.iter().any(|a| a == "--model"));
    }
}
